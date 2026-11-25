//! Kition Doc's collaboration members (engine contract 2, Kition phase 3e Task 3): the document
//! as a KDM replica.
//!
//! The engine keeps its own `Session` (the command bridge to the replica,
//! `kition_doc_engine::kdm_bridge`), not a `kition_kit::session::KitSession` directly, so
//! `kition_kit::export_kit_session!` does not apply here; these members are added by hand with
//! the exact JS names and shapes `kition-sdk`'s `session::web` gives
//! (`crates/kition-kit/src/session/web.rs`), so Kition's host code is the same for every kit (this
//! mirrors Kition Slide's `bindings/wasm/src/collab.rs`, same owner and licence). The wasm
//! bindings live in [`crate::web`].
//!
//! Word documents embed media (pictures) in the replica, so [`DocDocument::blobs`],
//! [`DocDocument::add_blobs`] and [`DocDocument::blobs_since`] move real content, encoded the
//! same way Kition Slide's and Kition Photo's do: `(64-char ASCII-hex sha256, u64 LE length,
//! bytes) * N`.

use std::collections::HashMap;
use std::sync::Arc;

use kition_doc_engine::{Damage, MemoryBlobs, Origin, Session, Version};
use kition_kit::KitDocument;
use serde_json::{Value, json};

use crate::DocDocument;

fn engine_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

impl DocDocument {
    /// Opens a file with the replica a host stored for it (the Kit UI protocol's `replica`
    /// capability: `.docx` cannot carry its own replica): the document is the replica's, so it
    /// keeps its KDM identity across saves. The host vouches that the replica was saved with
    /// these bytes; when they disagree (an older save, a document edited outside the replica) the
    /// engine records the file on top as one import. A replica that does not open (damaged, a
    /// newer schema) is ignored and the file opens as [`DocDocument::open`] opens it.
    ///
    /// Empty `bytes` mean there is no file yet, only the replica (a host opening a shared
    /// document for the first time from its sync room's snapshot, to save it as a new file): the
    /// document is the replica's alone.
    pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        match restore(name, bytes, snapshot, blobs) {
            Ok(session) => Ok(Self::from_session(session)),
            Err(_) => Self::open(name, bytes),
        }
    }

    /// Whether the document has made its replica yet: it makes one lazily, on the first shared
    /// edit or when asked for it ([`DocDocument::version`] and the rest).
    pub fn has_replica(&self) -> bool {
        self.session().kdm().is_some()
    }

    /// The replica's own document id (a `kition-sdk` `DocId`, stable across saves and across
    /// opening another replica of the same document — never Kition Doc's own `.docx` path/name):
    /// Kit UI's generic runtime reads this for the Kit UI protocol's `replica/info` message.
    /// `None` (`undefined` in JS, through [`crate::web`]) before the document has made a replica
    /// yet ([`DocDocument::has_replica`]).
    pub fn doc_id(&self) -> Option<String> {
        self.session().doc_id()
    }

    /// The document's replica version (empty before it has made one).
    pub fn version(&mut self) -> Result<Vec<u8>, String> {
        Ok(self.session().version().map_err(engine_error)?.encode())
    }

    /// The document's changes since `since` (empty bytes: every change).
    pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        let since = if since.is_empty() { Version::empty() } else { Version::decode(since).map_err(engine_error)? };
        self.session().export_changes(&since).map_err(engine_error)
    }

    /// Imports a peer's (or stored) changes; add their blobs first with
    /// [`DocDocument::add_blobs`]. Answers what went stale on screen.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage, String> {
        self.session().import_changes(bytes).map_err(engine_error)
    }

    /// The document's replica as a snapshot, for a peer to open as a new replica of the same
    /// document ([`DocDocument::open`]'ing the saved bytes does not: `.docx` carries no
    /// replica).
    pub fn snapshot(&mut self) -> Result<Vec<u8>, String> {
        self.session().snapshot().map_err(engine_error)
    }

    /// Undoes the document's last local step on its replica. `done` is `false`, not an error,
    /// when there is nothing to undo.
    pub fn undo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_undo();
        if done {
            self.execute("edit.undo", json!({}))?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    pub fn redo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_redo();
        if done {
            self.execute("edit.redo", json!({}))?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    pub fn can_undo(&self) -> bool {
        self.session().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.session().can_redo()
    }

    /// Runs a command as `origin` (a person, an agent): the replica records it as theirs, and it
    /// undoes as that replica's own step. Contract 1's [`DocDocument::execute`](kition_kit::KitDocument::execute)
    /// is this with the local person.
    pub fn execute_as(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        self.refuse_unknown(command, &params)?;
        self.clipboard_gate(command, &params)?;
        let params = self.resolved(command, params);
        let selection = self.session.borrow().sel.clone();
        let answer = self.session().execute_as(command, &params, origin).map_err(engine_error)?;
        self.selection_active |= self.session.borrow().sel != selection || matches!(command, "select.range" | "caret.set");
        self.copied_by(command);
        self.note_inserted_chart(command, &params, &answer);
        Ok(answer)
    }

    /// Every blob the document's replica references or can still bring back, as one buffer a peer
    /// feeds straight into [`DocDocument::add_blobs`].
    pub fn blobs(&mut self) -> Result<Vec<u8>, String> {
        let blobs = self.session().blobs().map_err(engine_error)?;
        Ok(encode_blobs(&blobs))
    }

    /// Adds a peer's blobs (from their [`DocDocument::blobs`]): kept until the next
    /// [`DocDocument::import_changes`], which a host calls right after.
    pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), String> {
        let blobs = decode_blobs(bytes)?;
        self.session().add_blobs(&blobs).map_err(engine_error)
    }

    /// The blobs the document's replica has newly needed since `since` (empty bytes: every blob
    /// the current state references), encoded the same way as [`DocDocument::blobs`]: for live
    /// sync, so one change ships only the blobs it needs rather than the whole store.
    pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        let since = if since.is_empty() { Version::empty() } else { Version::decode(since).map_err(engine_error)? };
        let blobs = self.session().blobs_since(&since).map_err(engine_error)?;
        Ok(encode_blobs(&blobs))
    }

    /// The replicated state, the replica made first if there is none yet.
    fn replicated_state(&mut self) -> Result<Value, String> {
        self.session().version().map_err(engine_error)?;
        let session = self.session();
        let kdm = session.kdm().ok_or("the document has no replica")?;
        kdm.replica().state_json().map_err(engine_error)
    }

    /// Takes a draft's changes (from a replica forked from this one) in as one undo step of the
    /// person (`mergeChanges`, as `kition-sdk`'s `export_kit_session!` answers it): the engine's
    /// document follows, and one Undo takes the whole draft back. Add the draft's blobs first.
    /// Answers `{"damage", "summary"}`, `summary` the `StateDiff` of the replicated state before
    /// and after. A refused merge changes nothing and fails with the KDM's message.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<Value, String> {
        let before = self.replicated_state()?;
        let damage = self.session().merge_changes(bytes, origin).map_err(engine_error)?;
        let after = self.replicated_state()?;
        Ok(json!({ "damage": damage, "summary": kition_kit::kdm::diff::diff_states(&before, &after).to_json() }))
    }

    /// What merging a draft's changes would change in the document now, as `StateDiff` JSON;
    /// nothing changes. Refuses what [`DocDocument::merge_changes`] refuses.
    pub fn compare_changes(&self, bytes: &[u8]) -> Result<Value, String> {
        let session = self.session();
        let kdm = session.kdm().ok_or("the document has no replica, so no draft of it")?;
        let before = kdm.replica().state_json().map_err(engine_error)?;
        let after = kdm.merged_state(bytes).map_err(engine_error)?;
        Ok(kition_kit::kdm::diff::diff_states(&before, &after).to_json())
    }

    /// Writes the state a snapshot holds (a checkpoint of this document, the Kit UI protocol's
    /// `replica/restore`) over the document by object id, as one undo step of `origin`'s scope:
    /// the engine's document follows. Add the blobs it references first. Nothing changes when the
    /// states are equal; another document's snapshot is refused.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<Damage, String> {
        self.session().restore_state(snapshot, origin).map_err(engine_error)
    }

    /// An agent's reply (JSON text) with every object id it holds as a value or a key named as
    /// the tool interface names it (its tool id, KitObjectModel OM9); unchanged without a replica,
    /// when the schema declares no tool ids, or when it names no object. The editor sends every
    /// `ai/inspect`, `document/check` and `ai/run` reply through it, so the tool ids are read at the
    /// objects the reply names alone (`ToolIds::show_reply`), not for the whole document.
    pub fn model_reply(&self, value: &str) -> Result<String, String> {
        let value: Value = serde_json::from_str(value).map_err(|error| format!("the reply is not JSON: {error}"))?;
        let session = self.session();
        let Some(kdm) = session.kdm() else { return Ok(value.to_string()) };
        Ok(kition_kit::kdm::tool_ids::ToolIds::show_reply(kdm.replica(), &value).to_string())
    }

    /// What changed since the version `since` (empty bytes: since the replica began), a receipt as
    /// `kition-sdk`'s `changesSince` answers it: `{"changes": StateDiff}`, or `{"changes": null,
    /// "unavailable": "<why>"}` when the replica's log does not hold `since` (or there is no
    /// replica yet).
    pub fn changes_since(&self, since: &[u8]) -> Value {
        let session = self.session();
        let Some(kdm) = session.kdm() else { return json!({ "changes": null, "unavailable": "the document has no replica yet" }) };
        let replica = kdm.replica();
        let since = if since.is_empty() { Ok(Version::empty()) } else { Version::decode(since) };
        match since.and_then(|since| replica.changes_since(&since)) {
            Ok(changes) => json!({ "changes": changes.to_recorded_json(replica.schema()) }),
            Err(error) => json!({ "changes": null, "unavailable": error.to_string() }),
        }
    }

    /// How many times the document left its replica because it refused a write or an undo (a
    /// document it cannot hold, or a bug). The document keeps working, undoing from its own
    /// history; the next shared edit makes a fresh replica, which collaborators must resync (its
    /// full [`DocDocument::snapshot`]). Hosts watch this.
    pub fn kdm_resets(&self) -> u32 {
        u32::try_from(self.session().kdm_resets()).unwrap_or(u32::MAX)
    }
}

/// Encodes blobs for one wasm call's worth of blob transfer
/// ([`DocDocument::blobs`]/[`DocDocument::add_blobs`]): `(64-byte ASCII-hex hash, u64 LE
/// length, bytes) * N`. Every hash the KDM model produces is a 64-char hex SHA-256, so the hash
/// field needs no length prefix of its own.
fn encode_blobs(blobs: &MemoryBlobs) -> Vec<u8> {
    let mut out = Vec::new();
    for (hash, data) in &blobs.0 {
        out.extend_from_slice(hash.as_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// Reverses [`encode_blobs`]; truncated or malformed input is an error, never a panic.
fn decode_blobs(bytes: &[u8]) -> Result<MemoryBlobs, String> {
    let mut map = HashMap::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let hash_end = i.checked_add(64).ok_or("blob bundle: overflow")?;
        let hash = bytes.get(i..hash_end).ok_or("truncated blob bundle: hash")?;
        let hash = std::str::from_utf8(hash).map_err(|_| "blob bundle: hash is not ASCII".to_owned())?.to_owned();
        let len_end = hash_end.checked_add(8).ok_or("blob bundle: overflow")?;
        let len_bytes: [u8; 8] =
            bytes.get(hash_end..len_end).ok_or("truncated blob bundle: length")?.try_into().map_err(|_| "blob bundle: length".to_owned())?;
        let len = usize::try_from(u64::from_le_bytes(len_bytes)).map_err(|_| "blob bundle: length too large".to_owned())?;
        let data_end = len_end.checked_add(len).ok_or("blob bundle: overflow")?;
        let data = bytes.get(len_end..data_end).ok_or("truncated blob bundle: data")?;
        map.insert(hash, Arc::from(data));
        i = data_end;
    }
    Ok(MemoryBlobs(map))
}

/// The object model (`KitObjectModel 1.0`) over the document's change exchange: `kition-doc` serves
/// `model.describe` and `model.batch` headless through `kition_kit::session::model::Exchange`,
/// and `model.open`, `model.import` and `model.export` with the media the changes reference.
impl kition_kit::session::model::ReplicaPort for DocDocument {
    fn model_schema(&self) -> std::sync::Arc<kition_kit::kdm::Schema> {
        <kdm_model::DocModel as kition_kit::session::KitModel>::schema()
    }

    fn model_export(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.export_changes(since)
    }

    fn model_import(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.import_changes(bytes).map(|_| ())
    }

    fn model_command(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        self.execute_as(command, params, origin)
    }

    fn model_commands(&self) -> Vec<kition_kit::KitCommand> {
        self.commands()
    }

    fn model_add_blobs(&mut self, blobs: &[u8]) -> Result<(), String> {
        self.add_blobs(blobs)
    }

    fn model_blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.blobs_since(since)
    }

    /// A document with no file yet (an agent's headless draft, a server's headless kit opening a
    /// sync room): the replica alone, failing rather than falling back to an empty document.
    fn model_open_replica(name: &str, snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        restore(name, &[], snapshot, blobs).map(Self::from_session)
    }
}

/// A session with the document `snapshot` holds (and `bytes` on top, when given), saved to `name`.
fn restore(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<Session, String> {
    let blobs = decode_blobs(blobs)?;
    let mut session = crate::files::session(kition_doc_engine::doc::Document::new());
    session.open_with_replica(bytes, snapshot, blobs).map_err(engine_error)?;
    // The new document is the active one, as after `open`, and is saved to `name`.
    session.path = Some(std::path::PathBuf::from(name));
    Ok(session)
}

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;
    use serde_json::json;

    use super::*;

    fn fixture() -> Vec<u8> {
        let mut doc = DocDocument::create("Document.docx", 0, 0).unwrap();
        doc.execute("text.insert", json!({ "text": "Hello world" })).unwrap();
        doc.save(None).unwrap()
    }

    fn extra_edit(doc: &mut DocDocument) {
        doc.execute("text.insert", json!({ "text": " and more" })).unwrap();
        doc.execute("format.bold", json!({})).unwrap();
    }

    #[test]
    fn version_and_snapshot_force_a_replica_lazily() {
        let mut doc = DocDocument::open("Document.docx", &fixture()).unwrap();
        assert!(!doc.has_replica(), "opening a .docx plainly makes no replica");
        let version = doc.version().unwrap();
        assert!(doc.has_replica());
        assert_eq!(doc.version().unwrap(), version, "asking twice makes no further replica");
        let snap = doc.snapshot().unwrap();
        assert!(!snap.is_empty());
    }

    #[test]
    fn undo_redo_report_done_and_damage() {
        let mut doc = DocDocument::open("Document.docx", &fixture()).unwrap();
        assert!(!doc.can_undo());
        doc.execute("text.insert", json!({ "text": "more text" })).unwrap();
        assert!(doc.can_undo());
        let (done, damage) = doc.undo().unwrap();
        assert!(done);
        assert_ne!(damage, Damage::None);
        assert!(!doc.can_undo());
        assert!(doc.can_redo());
        let (done, _) = doc.redo().unwrap();
        assert!(done);
        // Undoing with nothing to undo is a normal `false`, not an error.
        doc.undo().unwrap();
        let (done, damage) = doc.undo().unwrap();
        assert!(!done);
        assert_eq!(damage, Damage::None);
    }

    #[test]
    fn execute_as_records_the_given_origin() {
        let mut doc = DocDocument::open("Document.docx", &fixture()).unwrap();
        let origin = Origin::person("user:agent-1");
        doc.execute_as("text.insert", json!({ "text": "agent text" }), &origin).unwrap();
        assert!(doc.can_undo());
    }

    /// `docId` is `None` before any replica exists, and the replica's own id (not the file's
    /// `name`, and stable across a save/reopen with that same replica) once one does.
    #[test]
    fn doc_id_is_none_until_a_replica_exists_then_the_replicas_own_id() {
        let mut doc = DocDocument::open("Document.docx", &fixture()).unwrap();
        assert_eq!(doc.doc_id(), None, "no replica yet");
        doc.version().unwrap(); // makes the replica lazily
        let id = doc.doc_id().unwrap_or_else(|| panic!("doc_id should be set once a replica exists"));
        assert!(!id.is_empty());
        assert_eq!(doc.doc_id().as_deref(), Some(id.as_str()), "stable across further asks");

        let bytes = doc.save(None).unwrap();
        let snapshot = doc.snapshot().unwrap();
        let blobs = doc.blobs().unwrap();
        let reopened = DocDocument::open_with_replica("Document.docx", &bytes, &snapshot, &blobs).unwrap();
        assert_eq!(reopened.doc_id(), Some(id), "the reopened replica keeps the same document id");
    }

    /// With no file yet (empty bytes), the replica alone opens: a peer's full change export
    /// becomes the same document, same identity, and it saves as a new `.docx`.
    #[test]
    fn a_replica_opens_without_a_file() {
        let mut a = DocDocument::open("Document.docx", &fixture()).unwrap();
        extra_edit(&mut a);
        let changes = a.export_changes(&[]).unwrap();
        let mut b = DocDocument::open_with_replica("Shared.docx", &[], &changes, &a.blobs().unwrap()).unwrap();
        assert_eq!(b.doc_id(), a.doc_id());
        assert_eq!(b.version().unwrap(), a.version().unwrap());
        assert!(!b.dirty());
        assert!(DocDocument::open("Shared.docx", &b.save(None).unwrap()).is_ok());
        assert!(DocDocument::open_with_replica("Shared.docx", &[], b"not a replica", &[]).is_err());
    }

    #[test]
    fn blobs_round_trip_through_encode_and_decode() {
        let mut a = MemoryBlobs::default();
        a.0.insert("a".repeat(64), Arc::from(b"hello".as_slice()));
        a.0.insert("b".repeat(64), Arc::from(b"".as_slice()));
        let bytes = encode_blobs(&a);
        let b = decode_blobs(&bytes).unwrap();
        assert_eq!(b.0.len(), 2);
        assert_eq!(b.0.get(&"a".repeat(64)).map(|v| v.as_ref()), Some(b"hello".as_ref()));
        assert_eq!(b.0.get(&"b".repeat(64)).map(|v| v.as_ref()), Some(b"".as_ref()));
    }

    #[test]
    fn decode_blobs_rejects_truncated_input_without_panicking() {
        assert!(decode_blobs(&[1, 2, 3]).is_err());
        assert!(decode_blobs(&"a".repeat(64).into_bytes()).is_err(), "hash with no length");
        let mut malformed = "a".repeat(64).into_bytes();
        malformed.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode_blobs(&malformed).is_err(), "a length claiming more data than exists");
    }

    /// A `.docx` saved with its replica reopens as that replica: same version, same rendered
    /// pixels, not dirty, and it takes new edits (which undo) at once.
    #[test]
    fn a_document_reopens_with_its_stored_replica() {
        let mut a = DocDocument::open("Document.docx", &fixture()).unwrap();
        extra_edit(&mut a);
        let bytes = a.save(None).unwrap();
        let (snapshot, blobs) = (a.snapshot().unwrap(), a.blobs().unwrap());
        let version = a.version().unwrap();

        let mut b = DocDocument::open_with_replica("Document.docx", &bytes, &snapshot, &blobs).unwrap();
        assert!(b.has_replica());
        assert_eq!(b.version().unwrap(), version);
        assert_eq!(b.render(0.0, 0.0, 1.0, b.width(), b.height()).unwrap(), a.render(0.0, 0.0, 1.0, a.width(), a.height()).unwrap());
        assert!(!b.dirty());
        assert!(!b.can_undo());
        extra_edit(&mut b);
        assert!(b.dirty());
        assert_ne!(b.version().unwrap(), version);
        assert!(b.undo().unwrap().0);
        assert!(DocDocument::open("Document.docx", &b.save(None).unwrap()).is_ok(), "it still saves as .docx");
    }

    /// A host saves the way Kit UI does (`save`, then `snapshot` and `blobs`, then `mark_saved`):
    /// the save's stamp (modified time, revision) is in the file but is no edit of the replica,
    /// so the version stays, and the file reopens with its stored replica as that version.
    #[test]
    fn a_save_stamps_the_file_and_leaves_the_replica_alone() {
        let mut a = DocDocument::open("Document.docx", &fixture()).unwrap();
        extra_edit(&mut a);
        let revision = a.session().doc.core.revision;
        let before = a.version().unwrap();
        let bytes = a.save(None).unwrap();
        let (snapshot, blobs) = (a.snapshot().unwrap(), a.blobs().unwrap());
        a.mark_saved();
        assert!(!a.dirty());
        assert_eq!(a.version().unwrap(), before, "saving is not an edit of the replica");

        let plain = DocDocument::open("Document.docx", &bytes).unwrap();
        assert_eq!(plain.session().doc.core.revision, revision.saturating_add(1), "the file carries its save's stamp");
        assert!(!plain.session().doc.core.modified.is_empty());
        let mut b = DocDocument::open_with_replica("Document.docx", &bytes, &snapshot, &blobs).unwrap();
        assert_eq!(b.version().unwrap(), before, "the file reopens as its stored replica, stamp and all");
        assert!(!b.dirty());
    }

    /// Saving with nothing new writes the same bytes (Kition One saw a second Save write a
    /// version differing only in `dcterms:modified`): after a save, after opening the file
    /// plainly or with its stored replica. An edit stamps the next save.
    #[test]
    fn saving_with_nothing_new_writes_the_same_bytes() {
        let mut a = DocDocument::open("Document.docx", &fixture()).unwrap();
        extra_edit(&mut a);
        let bytes = a.save(None).unwrap();
        let (snapshot, blobs) = (a.snapshot().unwrap(), a.blobs().unwrap());
        a.mark_saved();
        assert_eq!(a.save(None).unwrap(), bytes, "saving again with nothing new");

        let plain = DocDocument::open("Document.docx", &bytes).unwrap();
        assert_eq!(plain.save(None).unwrap(), bytes, "the file opened plainly saves as it was");
        let mut b = DocDocument::open_with_replica("Document.docx", &bytes, &snapshot, &blobs).unwrap();
        let version = b.version().unwrap();
        assert_eq!(b.save(None).unwrap(), bytes, "the file opened with its replica saves as it was");
        assert_eq!(b.version().unwrap(), version);

        let revision = DocDocument::open("Document.docx", &bytes).unwrap().session().doc.core.revision;
        extra_edit(&mut b);
        let edited = b.save(None).unwrap();
        assert_ne!(edited, bytes);
        let stamped = DocDocument::open("Document.docx", &edited).unwrap();
        assert_eq!(stamped.session().doc.core.revision, revision.saturating_add(1), "an edit stamps the next save");
        assert_eq!(b.save(None).unwrap(), edited);
    }

    /// An unchanged document reopened with its stored replica keeps every object id and saves
    /// the same bytes (plan 2026-11-24, section 9.3).
    #[test]
    fn reopening_with_the_stored_replica_keeps_every_object_id() {
        use kition_kit::kdm::{ActorId, Replica};
        use kition_kit::session::KitModel;
        let ids = |doc: &mut DocDocument| -> Vec<String> {
            let replica = Replica::load(kdm_model::DocModel::schema(), &doc.snapshot().unwrap(), ActorId::random()).unwrap();
            replica.state_json().unwrap()["objects"].as_object().map(|objects| objects.keys().cloned().collect()).unwrap_or_default()
        };
        let mut a = DocDocument::open("Document.docx", &fixture()).unwrap();
        extra_edit(&mut a);
        let bytes = a.save(None).unwrap();
        let (snapshot, blobs) = (a.snapshot().unwrap(), a.blobs().unwrap());
        let before = ids(&mut a);
        assert!(before.len() > 1, "the document's objects: {before:?}");
        let mut b = DocDocument::open_with_replica("Document.docx", &bytes, &snapshot, &blobs).unwrap();
        assert_eq!(ids(&mut b), before, "the same object ids after reopening");
        assert_eq!(b.save(None).unwrap(), bytes, "and the same bytes when saved unchanged");
        let mut c = DocDocument::open_with_replica("Document.docx", &b.save(None).unwrap(), &b.snapshot().unwrap(), &b.blobs().unwrap()).unwrap();
        assert_eq!(ids(&mut c), before, "and again after a second reopening");
    }

    /// A damaged (or missing) stored replica never stops the file from opening: it opens plainly,
    /// never as a blank document.
    #[test]
    fn a_damaged_stored_replica_opens_the_file_plainly() {
        let bytes = fixture();
        let a = DocDocument::open("Document.docx", &bytes).unwrap();
        for (snapshot, blobs) in [(&b"not a snapshot"[..], &[][..]), (&[][..], &[1u8, 2, 3][..])] {
            let doc = DocDocument::open_with_replica("Document.docx", &bytes, snapshot, blobs).unwrap();
            assert_eq!((doc.width(), doc.height()), (a.width(), a.height()));
            assert!(!doc.has_replica(), "the file opened plainly");
        }
        assert!(DocDocument::open_with_replica("Document.docx", b"not a word document", b"not a snapshot", &[]).is_err());
    }

    /// `a` saves with its replica; `b` opens with it, so both start from the same replica
    /// identity. `a` edits further;
    /// `b.importChanges(a.exportChanges(b.version()))` converges `b` to the same replicated
    /// state as `a`, through the public SDK surface alone.
    #[test]
    fn two_documents_converge_through_export_and_import_changes() {
        let mut a = DocDocument::open("Document.docx", &fixture()).unwrap();
        extra_edit(&mut a);
        let bytes = a.save(None).unwrap();
        let snapshot = a.snapshot().unwrap();
        let blobs = a.blobs().unwrap();

        let mut b = DocDocument::open_with_replica("Document.docx", &bytes, &snapshot, &blobs).unwrap();
        assert_eq!(b.version().unwrap(), a.version().unwrap(), "b starts from a's own replica");

        a.execute("text.insert", json!({ "text": " even more" })).unwrap();

        let since = b.version().unwrap();
        let changes = a.export_changes(&since).unwrap();
        b.add_blobs(&a.blobs().unwrap()).unwrap();
        b.import_changes(&changes).unwrap();

        assert_eq!(a.version().unwrap(), b.version().unwrap());
        assert_eq!(
            a.render(0.0, 0.0, 1.0, a.width(), a.height()).unwrap(),
            b.render(0.0, 0.0, 1.0, b.width(), b.height()).unwrap(),
            "converged replicas render the same document"
        );
    }

    /// Changes from a newer build that raised the document's minimum schema version (rule M20)
    /// are refused with an error the host can show as is, and change nothing.
    #[test]
    fn changes_needing_a_newer_schema_are_refused_with_a_clear_error() {
        use kition_kit::kdm::{ActorId, Origin, Replica, Schema};
        let mut a = DocDocument::open("Document.docx", &fixture()).unwrap();
        let snapshot = a.snapshot().unwrap();
        let mut schema: Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
        let (ours, next) = (kdm_model::SCHEMA_VERSION, kdm_model::SCHEMA_VERSION + 1);
        schema["schemaVersion"] = json!(next);
        let schema = std::sync::Arc::new(Schema::parse(&schema.to_string()).unwrap());
        let mut newer = Replica::load(schema, &snapshot, ActorId::random()).unwrap();
        assert!(newer.transact(&Origin::system("migrate"), |tx| tx.raise_schema_version(next)).unwrap());

        let before = a.version().unwrap();
        let error = a.import_changes(&newer.export_changes(&Version::decode(&before).unwrap()).unwrap()).unwrap_err();
        assert!(error.contains(&format!("the document needs schema version {next}; this kit reads schema versions up to {ours}")), "{error}");
        assert_eq!(a.version().unwrap(), before, "a refused import changes nothing");
        // The document stays editable and shared.
        a.execute("text.insert", json!({ "text": "!" })).unwrap();
        assert_ne!(a.version().unwrap(), before);
        assert!(DocDocument::open_with_replica("Document.docx", &[], &newer.snapshot().unwrap(), &[]).is_err(), "nor opens it");
    }
    /// The body's text, read without moving the person.
    fn body(doc: &DocDocument) -> String {
        doc.session().doc.plain_text(kition_doc_engine::doc::StoryRef::Body)
    }

    /// The engine's document, canonical, for exact comparisons.
    fn view(doc: &DocDocument) -> Value {
        serde_json::to_value(kdm_model::normalize(&doc.session().doc)).unwrap()
    }

    /// An agent's draft (a replica forked from the document's) merges in as one step of the
    /// person's Undo (M12a), as the host merges one (`draft/merge` records the agent's origin):
    /// the comparison beforehand names what the merge does and changes nothing, the engine's
    /// document follows the merge, the person's edits made here meanwhile stay (a paragraph added
    /// to the same body list, and the title), the person's Undo gives back the exact state and
    /// document before the merge and Redo the merged ones, the agent's Undo does not take it, the
    /// replica's and the engine's histories agree, and a refused merge changes nothing.
    #[test]
    fn a_draft_merges_in_as_one_undo_step_and_the_document_follows() {
        use kition_kit::kdm::UndoScope;
        let mut doc = DocDocument::open("Document.docx", &fixture()).unwrap();
        let base = doc.version().unwrap();
        let mut draft = DocDocument::open_with_replica("Document.docx", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
        draft.execute("caret.docEnd", json!({})).unwrap();
        draft.execute("text.newParagraph", json!({})).unwrap();
        draft.execute("text.insert", json!({ "text": "From the draft" })).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        // Here meanwhile: a paragraph added at the same place of the body, and the document's title.
        doc.execute("caret.docEnd", json!({})).unwrap();
        doc.execute("text.newParagraph", json!({})).unwrap();
        doc.execute("text.insert", json!({ "text": "Meanwhile" })).unwrap();
        doc.execute("file.properties", json!({ "title": "Meanwhile title" })).unwrap();
        let version = doc.version().unwrap();
        let (state, before) = (doc.replicated_state().unwrap(), view(&doc));
        let preview = doc.compare_changes(&changes).unwrap();
        assert_eq!(doc.version().unwrap(), version, "comparing changes nothing");
        assert_eq!(body(&doc), "Hello world\nMeanwhile");
        let agent = Origin::agent("agent:writer");
        let answer = doc.merge_changes(&changes, &agent).unwrap();
        assert_eq!(answer["summary"], preview, "the comparison named what the merge did: {answer}");
        assert_ne!(answer["damage"], json!("none"), "{answer}");
        assert_eq!(body(&doc), "Hello world\nMeanwhile\nFrom the draft", "both paragraphs, the document's first");
        assert!(doc.replicated_state().unwrap().to_string().contains("Meanwhile title"), "the title set here stays");
        assert!(doc.dirty());
        let (merged_state, merged) = (doc.replicated_state().unwrap(), view(&doc));
        // The merge is the person's step, in the replica and in the engine alike.
        {
            let session = doc.session();
            let kdm = session.kdm().unwrap();
            assert!(!kdm.can_undo_in(UndoScope::Agent) && !session.can_undo_in(UndoScope::Agent), "no step of the agent's");
            assert!(kdm.can_undo_in(UndoScope::Person) && session.can_undo_in(UndoScope::Person));
        }
        let _ = doc.execute_as("edit.undo", json!({}), &agent);
        assert_eq!(doc.replicated_state().unwrap(), merged_state, "the agent's undo does not take the merge back");
        assert_eq!(view(&doc), merged);
        assert!(doc.undo().unwrap().0);
        assert_eq!(doc.replicated_state().unwrap(), state, "one undo gives back the exact state before the merge");
        assert_eq!(view(&doc), before, "and the document");
        assert!(doc.redo().unwrap().0);
        assert_eq!(doc.replicated_state().unwrap(), merged_state, "one redo gives back the merged state");
        assert_eq!(view(&doc), merged);
        let at = doc.version().unwrap();
        assert!(doc.merge_changes(b"not changes", &agent).is_err());
        assert_eq!(doc.version().unwrap(), at, "a refused merge changes nothing");
    }

    /// An earlier state (a checkpoint's snapshot) is written over the document by object id as
    /// one undo step: the replica holds that state again, with the same ids, and one Undo takes
    /// it back.
    #[test]
    fn a_restored_state_keeps_the_ids_and_undoes_as_one_step() {
        use kition_kit::kdm::{ActorId, Replica};
        use kition_kit::session::KitModel;
        let mut doc = DocDocument::open("Document.docx", &fixture()).unwrap();
        doc.version().unwrap();
        let checkpoint = doc.snapshot().unwrap();
        let before = body(&doc);
        extra_edit(&mut doc);
        let edited = body(&doc);
        assert_ne!(edited, before);
        let origin = Origin::person("person:local");
        let damage = doc.restore_state(&checkpoint, &origin).unwrap();
        assert_ne!(damage, Damage::None);
        assert_eq!(body(&doc), before, "the engine's document follows the restore");
        let then = Replica::load(kdm_model::DocModel::schema(), &checkpoint, ActorId::random()).unwrap().state_json().unwrap();
        assert_eq!(doc.replicated_state().unwrap()["objects"], then["objects"], "the checkpoint's objects, under their ids");
        let at = doc.version().unwrap();
        assert_eq!(doc.restore_state(&checkpoint, &origin).unwrap(), Damage::None, "the same state again changes nothing");
        assert_eq!(doc.version().unwrap(), at);
        assert!(doc.undo().unwrap().0);
        assert_eq!(body(&doc), edited, "one undo takes the restore back");
        let other = DocDocument::open("Document.docx", &fixture()).unwrap().snapshot().unwrap();
        assert!(doc.restore_state(&other, &origin).is_err(), "another document's state is refused");
    }

    /// An agent's reply names objects by their tool ids; text that merely holds an id stays.
    #[test]
    fn a_reply_names_objects_by_their_tool_ids() {
        let mut doc = DocDocument::open("Document.docx", &fixture()).unwrap();
        let state = doc.replicated_state().unwrap();
        let block = state["objects"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, object)| object["$type"] == "Block")
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| panic!("a block object: {state}"));
        let tool_id = {
            let session = doc.session();
            let ids = kition_kit::kdm::tool_ids::ToolIds::of_replica(session.kdm().unwrap().replica());
            ids.tool_id(&kition_kit::kdm::ObjectId::from_string(block.as_str())).map(str::to_owned)
        };
        let tool_id = tool_id.unwrap_or_else(|| panic!("block {block} has a tool id"));
        assert_ne!(tool_id, block);
        let shown: Value =
            serde_json::from_str(&doc.model_reply(&json!({ "target": block, "note": format!("about {block}") }).to_string()).unwrap()).unwrap();
        assert_eq!(shown["target"], json!(tool_id), "{shown}");
        assert_eq!(shown["note"], json!(format!("about {block}")), "text that holds an id stays");
        assert!(doc.model_reply("not json").is_err());
    }

    /// A reply naming one object of a document with many is named as the whole document's tool
    /// ids name it, though only that object's are read; a reply naming none comes back unchanged.
    #[test]
    fn a_reply_reads_the_tool_ids_of_the_objects_it_names() {
        let mut doc = DocDocument::create("Document.docx", 0, 0).unwrap();
        for i in 0..300 {
            if i > 0 {
                doc.execute("text.newParagraph", json!({})).unwrap();
            }
            doc.execute("text.insert", json!({ "text": format!("Paragraph {i}") })).unwrap();
        }
        let state = doc.replicated_state().unwrap();
        let blocks: Vec<String> =
            state["objects"].as_object().unwrap().iter().filter(|(_, object)| object["$type"] == "Block").map(|(id, _)| id.clone()).collect();
        assert!(blocks.len() >= 300, "{}", blocks.len());
        let whole = {
            let session = doc.session();
            kition_kit::kdm::tool_ids::ToolIds::of_replica(session.kdm().unwrap().replica())
        };
        let one = &blocks[blocks.len() / 2];
        let about: serde_json::Map<String, Value> = std::iter::once((one.clone(), json!("a paragraph"))).collect();
        let reply = json!({ "target": one, "range": [one], "about": about });
        let shown: Value = serde_json::from_str(&doc.model_reply(&reply.to_string()).unwrap()).unwrap();
        assert_eq!(shown, whole.show_json(&reply), "the same tool ids as the whole document's");
        assert_ne!(shown, reply, "the object is named by its tool id: {shown}");
        let plain = json!({ "summary": "Hello", "blocks": ["p1", "p2"], "count": 3 });
        assert_eq!(serde_json::from_str::<Value>(&doc.model_reply(&plain.to_string()).unwrap()).unwrap(), plain, "a reply naming no object stays");
    }

    /// A receipt names what changed since a version the replica's log holds, and says so when it
    /// cannot (no replica yet, a version of another line).
    #[test]
    fn changes_since_answers_a_receipt_or_why_not() {
        let mut doc = DocDocument::open("Document.docx", &fixture()).unwrap();
        assert!(doc.changes_since(&[])["changes"].is_null(), "no replica yet");
        let since = doc.version().unwrap();
        extra_edit(&mut doc);
        let receipt = doc.changes_since(&since);
        assert!(receipt["changes"].is_object(), "{receipt}");
        let now = doc.version().unwrap();
        assert_ne!(receipt["changes"], doc.changes_since(&now)["changes"], "the edit is in the receipt: {receipt}");
        let other = DocDocument::open("Document.docx", &fixture()).unwrap().version().unwrap();
        let refused = doc.changes_since(&other);
        assert!(refused["changes"].is_null() && refused["unavailable"].is_string(), "{refused}");
    }

    /// `kition-doc` serves the object model through `ReplicaPort`: a command and a model edit in
    /// batches reach the engine, and a read-only batch refuses writes.
    #[test]
    fn serves_the_object_model_over_its_change_exchange() {
        use kition_kit::session::model::{Exchange, ModelErrorCode};
        use serde_json::{Value, json};

        let mut doc = DocDocument::open("Document.docx", &fixture()).unwrap();
        let mut exchange = Exchange::new();
        let described = exchange.describe(&mut doc).unwrap();
        assert_eq!(described["capability"], "KitObjectModel 1.0");
        assert!(described["commands"].as_array().unwrap().iter().any(|command| command["id"] == "text.insert"));

        let command = json!({ "ops": [{ "op": "command", "id": "text.insert", "params": { "text": "agent text" } }] });
        assert_eq!(exchange.batch(&mut doc, &command, true).unwrap_err().code, ModelErrorCode::ReadOnly);
        let before = doc.version().unwrap();
        exchange.batch(&mut doc, &command, false).unwrap();
        assert_ne!(doc.version().unwrap(), before, "the command ran in the engine");

        let load = |exchange: &mut Exchange, doc: &mut DocDocument, target: &str, field: &str| -> Value {
            let answer = exchange.batch(doc, &json!({ "ops": [], "load": [{ "target": target, "fields": [field] }] }), false).unwrap();
            answer["loaded"][0]["fields"][field].clone()
        };
        let body = load(&mut exchange, &mut doc, "root", "body");
        let block = body[0].as_str().unwrap().to_owned();
        let insert = json!({ "ops": [{ "op": "text.insert", "target": block, "field": "text", "index": 0, "text": "Scripted " }] });
        exchange.batch(&mut doc, &insert, false).unwrap();
        let text = load(&mut Exchange::new(), &mut doc, &block, "text");
        // Text with marks loads as its runs.
        let plain = text.as_str().map_or_else(|| text.as_array().unwrap().iter().filter_map(|run| run["insert"].as_str()).collect(), str::to_owned);
        assert!(plain.starts_with("Scripted "), "{text}");
    }

    /// A script that creates a paragraph block with only its text (`para` null or left out) adds a
    /// paragraph with the default properties and that text, which the document saves.
    #[test]
    fn a_scripted_paragraph_without_its_register_keeps_its_text() {
        use kition_kit::session::model::Exchange;

        let mut doc = DocDocument::open("Document.docx", &fixture()).unwrap();
        let mut exchange = Exchange::new();
        let batch = json!({ "ops": [
            { "op": "create", "type": "Block", "in": "root.body", "ref": "$a", "fields": { "kind": "para", "para": null } },
            { "op": "text.insert", "target": "$a", "field": "text", "index": 0, "text": "Null register" },
            { "op": "create", "type": "Block", "in": "root.body", "ref": "$b", "fields": { "kind": "para" } },
            { "op": "text.insert", "target": "$b", "field": "text", "index": 0, "text": "No register" },
        ] });
        exchange.batch(&mut doc, &batch, false).unwrap();
        let text = doc.session().doc.plain_text(kition_doc_engine::doc::StoryRef::Body);
        assert!(text.contains("Null register") && text.contains("No register"), "{text:?}");
        let saved = DocDocument::open("Document.docx", &doc.save(None).unwrap()).unwrap();
        let text = saved.session().doc.plain_text(kition_doc_engine::doc::StoryRef::Body);
        assert!(text.contains("Null register") && text.contains("No register"), "{text:?}");
    }
}
