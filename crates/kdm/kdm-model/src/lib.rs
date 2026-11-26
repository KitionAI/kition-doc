//! Bridges Kition Doc's [`Document`] ([`kition_doc_doc`]) to a `kit.doc` KDM replica.
//!
//! [`DocModel`] implements [`KitModel`]: the engine's materialised view of a replica, the same
//! [`Document`] the rest of the engine already knows. Blocks (paragraphs and tables, at every
//! depth), table rows and cells, parts (headers, footers, notes, comment texts, text boxes),
//! comments, revisions and media become KDM objects with their own identity, so concurrent edits
//! of different paragraphs, rows or comments merge; [`write_document`] and [`write_changes`] are
//! the engine command bridge's entry points: writing a whole document in one creating
//! transaction, or only what a command changed.
//!
//! What lives where (`model/schema.json`, schema version 1):
//! - a paragraph's text is a KDM `text` field with marks, so
//!   concurrent typing in one paragraph merges by character: one mark per run property, unknown
//!   ones in one `extra` mark, each U+FFFC anchor marked with the `Inline` object holding its
//!   inline object (`text`); the paragraph's properties, mark and section are its `para` register.
//!   A split keeps the first half's object and its text field: the moved text is deleted there
//!   and inserted into the new paragraph, in one transaction, and a split anchor at the split
//!   point names the new paragraph, so text typed concurrently at the paragraph's end shows at
//!   the end of the second half (`pieces`). A merge (Backspace at a paragraph's start) keeps the
//!   second paragraph's object, text and place in the list and puts a merge anchor naming the
//!   first at the start of its field: the reader shows its text at the end of the first, so text
//!   typed into it concurrently is kept. A paragraph whose `para` register was never written
//!   (a script's) reads with the default properties;
//! - a table's properties, a row's and a cell's properties are registers; the table's columns
//!   (`cols`, one `Col` object with its `width` per grid column), rows, cells and each story's
//!   blocks are lists, so two people resizing two columns at once both keep theirs;
//! - the document's settings, properties, sources, custom properties and last section are `json`
//!   registers; each style is an entry of the `styles` map, keyed by its style id
//!   (`{order, style}`), and the document defaults are the `styleDefaults` register, so styles
//!   made or changed concurrently are all kept; each list (a `Num` with its abstract list) is an
//!   entry of the `lists` map, keyed by the list's UUID, so lists made concurrently are all kept
//!   (the rest of the numbering is the `numbering` register); media bytes and the package parts
//!   kept verbatim (`passthrough`) are content-addressed blobs ([`BlobSink`]/[`BlobSource`]); what
//!   serde skips (OLE objects and charts kept from a file) is added to the paragraph's JSON, so
//!   nothing is lost on a round trip.
//!
//! - comments are beside the content (Kition platform rule M23, `comments`): a `CommentThread`
//!   per thread, a `Comment` per comment, the thread's `anchor` naming the paragraph and range it
//!   comments on; paragraphs hold no comment anchor and comment texts are no parts, so a change
//!   that only comments touches only those two types.
//!
//! **Identity (W3).** The engine's blocks have no ids: [`write_changes`] infers which object each
//! block is (see `write`), and [`Ids`] keeps the objects of the view.
//!
//! **Ids (W4).** Part, comment, list and abstract list ids, revision indexes and media keys are
//! local: the reader mints them (keeping, through [`Known`], the ones a session's view already gave
//! an object). Every one stored as data is written as the object id it names (a UUID for a list or
//! an abstract list, which have no object) and translated back on read (`refs`).
//!
//! **Repair (W5).** [`KitModel::validate`] clears what a merge left dangling (`validate`); what
//! only a read can see in a text field (an anchor whose comment or note is gone, a mark naming a
//! deleted revision) [`DocSession`] writes back in a `system` transaction after each import,
//! undo or redo and when it opens a replica (`repair`).
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod blob_gc;
mod caret;
mod charts;
mod comments;
mod compare;
mod frames;
mod ids;
mod list;
mod pieces;
mod read;
mod refs;
mod repair;
mod reuse;
mod session;
mod text;
mod validate;
mod write;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use kition_doc_doc::Document;
use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema, Tx};
use kition_kit::session::{BlobSink, BlobSource, Damage, KitError, KitModel};

pub use compare::normalize;
pub use ids::{BlockIds, CellIds, Ids, Known, PartIds, RowIds, TableIds};
pub use read::{read_document, read_document_known};
pub use session::DocSession;
pub use text::ParaText;
pub use write::{write_changes, write_document};

/// Re-exported so downstream crates (the engine command bridge) share this crate's exact pin.
pub use kition_kit;

type Result<T> = kition_kit::session::Result<T>;
pub(crate) type KdmResult<T> = std::result::Result<T, KdmError>;

const SCHEMA: &str = include_str!("../model/schema.json");

/// `model/schema.json`'s schema version. There is no migration from the shapes earlier builds
/// wrote: their replicas are rebuilt from their files.
pub const SCHEMA_VERSION: u32 = 1;

/// The `Document` object's id: one per replica, minted by [`write_document`] with
/// [`Tx::create_with_id`], so every reader finds it without an index.
pub(crate) fn root_id() -> ObjectId {
    ObjectId::from_string("document")
}

fn kit_err(error: impl std::fmt::Display) -> KitError {
    KitError::new(error.to_string())
}

pub(crate) fn kdm_err(error: impl std::fmt::Display) -> KdmError {
    KdmError::Engine(error.to_string())
}

/// Blob bytes the view already holds, by hash, so a reload does not copy them again.
pub(crate) type MediaCache = HashMap<String, Arc<Vec<u8>>>;

fn ext_of(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.contains('.') { lower.rsplit('.').next().unwrap_or("").to_owned() } else { String::new() }
}

/// A file's document, as [`KitModel::import`] reads it: a Word package (`.docx`, `.docm`,
/// `.dotx`, `.dotm`, or any zip by content but an OpenDocument one), a Word 97-2003 `.doc`, the
/// engine's JSON, plain text, or any other format the engine opens (Markdown, HTML, RTF, ODT,
/// LaTeX), by `name`'s extension. List ids are made unique (a replica names each list once).
pub fn decode(name: &str, bytes: &[u8]) -> Result<Document> {
    let ext = ext_of(name);
    let package = matches!(ext.as_str(), "docx" | "docm" | "dotx" | "dotm") || (bytes.starts_with(b"PK\x03\x04") && ext != "odt");
    let mut doc = if package || kition_doc_docx::is_encrypted(bytes) {
        kition_doc_docx::read_with_password(bytes, None).map_err(kit_err)?
    } else {
        match ext.as_str() {
            "doc" | "dot" => kition_doc_docbin::read(bytes).map_err(kit_err)?,
            "json" => serde_json::from_slice::<Document>(bytes).map_err(kit_err)?,
            "txt" | "text" | "" => Document::from_text(&String::from_utf8_lossy(bytes)),
            other => match kition_doc_formats::import(other, bytes) {
                Some(result) => result.map_err(kit_err)?,
                None => return Err(kit_err(format!("{name}: unsupported format `.{other}`"))),
            },
        }
    };
    doc.ensure_nonempty();
    dedupe_nums(&mut doc);
    Ok(doc)
}

/// Gives a list whose id another list already has (or 0, which means "no list") a fresh id. A
/// paragraph naming that id meant the first list, and still does.
fn dedupe_nums(doc: &mut Document) {
    let mut next = doc.numbering.nums.iter().map(|n| n.id).max().unwrap_or(0);
    let mut seen = HashSet::new();
    for num in &mut doc.numbering.nums {
        if num.id == 0 || !seen.insert(num.id) {
            next = next.saturating_add(1);
            num.id = next;
            seen.insert(next);
        }
    }
}

/// The document as file bytes in `name`'s format: a Word package for `.docx`, `.docm`, `.dotx`
/// and `.dotm` (and any name the engine has no other format for), else the engine's other formats.
pub fn encode(name: &str, doc: &Document) -> Result<Vec<u8>> {
    let ext = ext_of(name);
    if let Some(flavor) = kition_doc_docx::Flavor::from_ext(&ext) {
        return kition_doc_docx::write_as(doc, flavor).map_err(kit_err);
    }
    match ext.as_str() {
        "json" => serde_json::to_vec_pretty(doc).map_err(kit_err),
        "txt" | "text" => Ok(doc.plain_text(kition_doc_doc::StoryRef::Body).replace('\n', "\r\n").into_bytes()),
        other => match kition_doc_formats::export(other, doc) {
            Some(result) => result.map_err(kit_err),
            None => kition_doc_docx::write(doc).map_err(kit_err),
        },
    }
}

/// The engine's view of a `kit.doc` replica: the same [`Document`] the rest of the engine knows,
/// plus the objects [`write_changes`] needs to find each piece again.
pub struct DocModel {
    pub document: Document,
    ids: Ids,
    known: Known,
    media: MediaCache,
    /// Blobs the view read as empty because they had not arrived ([`DocModel::missing_blobs`]).
    missing: BTreeSet<String>,
}

impl DocModel {
    /// A view built by the caller itself: `document` as written by [`write_document`] or
    /// [`write_changes`], with the ids they filled.
    pub fn from_parts(document: Document, ids: Ids) -> Self {
        let mut known = Known::default();
        known.remember(&document, &ids);
        Self { document, ids, known, media: MediaCache::new(), missing: BTreeSet::new() }
    }

    /// Replaces the view after the caller wrote `document` to the replica itself (a diff bridge's
    /// [`write_changes`]), with the ids that write left.
    pub fn adopt(&mut self, document: Document, ids: Ids) {
        self.known.remember(&document, &ids);
        self.document = document;
        self.ids = ids;
    }

    /// The objects of the view, current as of the last `load`/`apply`/`adopt`. A diff bridge
    /// clones them into its own working copy for a [`write_changes`] transaction, then hands them
    /// back through [`DocModel::adopt`].
    pub fn ids(&self) -> &Ids {
        &self.ids
    }

    /// The local ids the view ever gave objects, for [`read_document_known`].
    pub fn known(&self) -> &Known {
        &self.known
    }

    /// The hashes of the blobs the view read as empty bytes because they had not arrived yet: once
    /// they do, the view is read again ([`DocSession::add_blobs`]).
    pub fn missing_blobs(&self) -> &BTreeSet<String> {
        &self.missing
    }

    fn reload(&mut self, replica: &Replica, blobs: &dyn BlobSource) -> Result<()> {
        let (mut document, ids, media, missing) = read::read_with(replica, blobs, &self.known, &self.media).map_err(kit_err)?;
        reuse::reuse(&self.document, &self.ids, &mut document, &ids);
        self.known.remember(&document, &ids);
        self.document = document;
        self.ids = ids;
        self.media = media;
        self.missing = missing;
        Ok(())
    }
}

impl std::fmt::Debug for DocModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocModel").field("blocks", &self.document.body.len()).field("parts", &self.document.parts.len()).finish_non_exhaustive()
    }
}

impl KitModel for DocModel {
    fn schema() -> Arc<Schema> {
        #[allow(clippy::expect_used)]
        Arc::new(Schema::parse(SCHEMA).expect("model/schema.json is a fixed, tested file"))
    }

    fn load(replica: &Replica, blobs: &dyn BlobSource) -> Result<Self> {
        let (document, ids, media, missing) = read::read_with(replica, blobs, &Known::default(), &MediaCache::new()).map_err(kit_err)?;
        let mut model = Self::from_parts(document, ids);
        model.media = media;
        model.missing = missing;
        Ok(model)
    }

    /// Reloads the whole view (keeping every surviving object's local id, the blob bytes it
    /// already holds, and the old `Arc` of every block that reads back unchanged, so its
    /// paragraphs keep their `rev` and the engine's layout cache stays warm).
    fn apply(&mut self, replica: &Replica, _changed: &BTreeSet<ObjectId>, blobs: &dyn BlobSource) -> Result<Damage> {
        self.reload(replica, blobs)?;
        Ok(Damage::Full)
    }

    /// The state repair (`validate`), then the text repair (`repair`) on what it leaves, in
    /// the same transaction: a merge's or a restore's own change and undo step. The text repair
    /// checks every paragraph when the state repair wrote (it may delete a note reference) or
    /// when `changed` is empty (a restore names no objects).
    fn validate(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> Result<bool> {
        let wrote = validate::validate(tx, changed).map_err(kit_err)?;
        let scope = (!wrote && !changed.is_empty()).then_some(changed);
        let repaired = repair::repair(tx, scope).map_err(kit_err)?;
        Ok(wrote || repaired)
    }

    fn import(name: &str, bytes: &[u8], blobs: &mut dyn BlobSink, actor: ActorId) -> Result<Replica> {
        let document = decode(name, bytes)?;
        let mut ids = Ids::default();
        let (replica, ()) = Replica::create_with(Self::schema(), DocId::new(), actor, &Origin::system("import"), |tx| {
            write_document(tx, &document, &mut ids, blobs)?;
            Ok(())
        })
        .map_err(kit_err)?;
        Ok(replica)
    }

    fn export(&self, _replica: &Replica, name: &str, _blobs: &dyn BlobSource) -> Result<Vec<u8>> {
        encode(name, &self.document)
    }
}
