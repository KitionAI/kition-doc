//! [`DocSession`]: one document's replica, its [`DocModel`] view and its blobs, for the engine's
//! command bridge (Kition phase 3e, Task 2; Kition Slide's `SlideSession`, same owner and licence).
//!
//! It is `kition_kit::session::KitSession<DocModel>`'s flow (transactions, per-actor undo, the
//! change exchange, the post-merge repair through [`KitModel::validate`], the view patched through
//! [`KitModel::apply`]) on concrete parts, as Kition Photo's `PhotoSession` does: the engine
//! must enumerate, ship and garbage-collect the media blobs, which `KitSession`'s boxed blob store
//! does not allow. Edits are diff-bridged: the engine changes its own [`Document`], and
//! [`DocSession::record`] writes what changed as one transaction ([`write_changes`]), then
//! adopts it as the view. Everything here is `Send`.

use std::collections::BTreeSet;
use std::sync::Arc;

use kition_doc_doc::Document;
use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, OriginKind, Replica, Tx, UndoOutcome, UndoScope, Version};
use kition_kit::session::{BlobSource, Damage, KitModel, MemoryBlobs};

use crate::blob_gc::{BlobIndex, Moved};
use crate::{DocModel, Ids, Result, kit_err, normalize, read_document, write_changes, write_document};

/// One open document as a KDM replica and the engine's view of it. See the module docs.
pub struct DocSession {
    replica: Replica,
    model: DocModel,
    blobs: MemoryBlobs,
    /// Which blobs are still needed ([`crate::blob_gc`]); the rest are dropped after each change.
    index: BlobIndex,
    /// The hashes each record, import, undo and redo started referencing, with the version right
    /// after it, for [`DocSession::blobs_since`] (Kition Photo's blob log: chronological, so
    /// `since` cleanly splits it into "already has" and "needs"; trimmed with the store).
    blob_log: Vec<(Version, Vec<String>)>,
}

impl std::fmt::Debug for DocSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocSession").field("doc", &self.replica.doc_id()).field("actor", &self.replica.actor()).finish_non_exhaustive()
    }
}

impl DocSession {
    /// An isolated replica with the same local view identities and retained blobs.
    pub fn fork(&self) -> Result<Self> {
        let mut fork = Self::open_snapshot(&self.snapshot()?, self.blobs.clone())?;
        fork.model = DocModel::from_parts(self.model.document.clone(), self.model.ids().clone());
        fork.model.known = self.model.known.clone();
        Ok(fork)
    }

    /// A new replica holding `document`, written in its creating transaction (not an undo step).
    /// The view is `document` itself, with its own local ids.
    pub fn create(document: &Document) -> Result<Self> {
        let mut blobs = MemoryBlobs::default();
        let mut ids = Ids::default();
        let (mut replica, ()) = Replica::create_with(DocModel::schema(), DocId::new(), ActorId::random(), &Origin::system("create"), |tx| {
            write_document(tx, document, &mut ids, &mut blobs).map(|_| ())
        })
        .map_err(kit_err)?;
        replica.take_changed();
        let index = BlobIndex::build(&replica);
        let model = DocModel::from_parts(document.clone(), ids);
        Ok(Self { replica, model, blobs, index, blob_log: Vec::new() })
    }

    /// Opens a snapshot (or a full change export) of another replica as a new replica of a fresh
    /// actor. `blobs` must hold the blobs it references.
    pub fn open_snapshot(bytes: &[u8], blobs: MemoryBlobs) -> Result<Self> {
        let mut replica = Replica::load(DocModel::schema(), bytes, ActorId::random()).map_err(kit_err)?;
        replica.take_changed();
        let model = DocModel::load(&replica, &blobs)?;
        let index = BlobIndex::build(&replica);
        let mut session = Self { replica, model, blobs, index, blob_log: Vec::new() };
        // Best effort: a replica merged elsewhere may need the read repair; a failure leaves it.
        let _ = session.repair_read(None);
        session.index.collect_garbage(&mut session.blobs);
        Ok(session)
    }

    /// The view: the document as of the last record, undo, redo or import.
    pub fn document(&self) -> &Document {
        &self.model.document
    }

    pub fn model(&self) -> &DocModel {
        &self.model
    }

    /// Writes what differs between the view and `after` as one transaction of `origin`, then
    /// makes `after` the view. Answers how many undo steps that made: 1, or 0 when nothing
    /// replicated changed or `origin` is a `system`/`import` one (never an undo step). On failure
    /// nothing is written and the view is unchanged.
    pub fn record(&mut self, origin: &Origin, after: &Document) -> Result<u32> {
        let mut ids = self.model.ids().clone();
        let before = self.replica.version();
        let (model, blobs) = (&self.model, &mut self.blobs);
        let result = self.replica.transact(origin, |tx| write_changes(tx, &model.document, after, &mut ids, blobs).map(|_| ()));
        let changed = self.replica.take_changed();
        if let Err(error) = result {
            self.index.collect_garbage(&mut self.blobs);
            return Err(kit_err(error));
        }
        let moved = self.index.update(&self.replica, &changed);
        self.model.adopt(after.clone(), ids);
        let wrote = self.replica.version() != before;
        let undoable = wrote && !matches!(origin.kind, OriginKind::System | OriginKind::Import);
        self.log_added(moved.added.iter().cloned());
        if undoable {
            self.index.push_step(moved);
        }
        self.index.collect_garbage(&mut self.blobs);
        Ok(u32::from(undoable))
    }

    /// Whether the replica holds `document`'s content, compared id-blind ([`normalize`]) as
    /// [`read_document`] reads both back. Errs if `document` cannot be written to a replica.
    pub fn holds(&self, document: &Document) -> Result<bool> {
        let mut blobs = MemoryBlobs::default();
        let mut ids = Ids::default();
        let (scratch, ()) = Replica::create_with(DocModel::schema(), DocId::new(), ActorId::random(), &Origin::system("create"), |tx| {
            write_document(tx, document, &mut ids, &mut blobs).map(|_| ())
        })
        .map_err(kit_err)?;
        let (theirs, _) = read_document(&scratch, &blobs).map_err(kit_err)?;
        let (mine, _) = read_document(&self.replica, &self.blobs).map_err(kit_err)?;
        let value = |d: &Document| serde_json::to_value(normalize(d)).map_err(kit_err);
        Ok(value(&theirs)? == value(&mine)?)
    }

    /// Undoes this replica's last step of the person (never an import or a repair), repairs the
    /// result and patches the view. Answers whether there was one, and the damage.
    pub fn undo(&mut self) -> Result<(bool, Damage)> {
        self.undo_in(UndoScope::Person)
    }

    pub fn redo(&mut self) -> Result<(bool, Damage)> {
        self.redo_in(UndoScope::Person)
    }

    /// [`DocSession::undo`] in `scope` (KitModel M12a): the person's and the agent's steps on
    /// this replica are separate histories.
    pub fn undo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        self.step_in(scope, true).map(|(outcome, damage)| (outcome.is_done(), damage))
    }

    pub fn redo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        self.step_in(scope, false).map(|(outcome, damage)| (outcome.is_done(), damage))
    }

    /// One undo (or redo) of the replica in `scope`, with what it took off its stack: the steps
    /// it passed over (their objects gone since, or their changes cancelling out) and the one it
    /// took back, if any ([`UndoOutcome`]). The engine's history counts replica steps with it.
    /// Nothing to take is the empty outcome.
    pub fn step_in(&mut self, scope: UndoScope, undo: bool) -> Result<(UndoOutcome, Damage)> {
        let can = if undo { self.can_undo_in(scope) } else { self.can_redo_in(scope) };
        if !can {
            return Ok((UndoOutcome::default(), Damage::None));
        }
        let outcome = if undo { self.replica.undo_in(scope) } else { self.replica.redo_in(scope) }.map_err(kit_err)?;
        let (damage, moved) = self.settle();
        if outcome.steps() > 0 {
            if outcome.is_done() {
                self.log_added(moved.added.iter().cloned());
            }
            if undo {
                self.index.undone(outcome.passed.len(), outcome.is_done(), moved);
            } else {
                self.index.redone(outcome.passed.len(), outcome.is_done(), moved);
            }
        }
        self.index.collect_garbage(&mut self.blobs);
        Ok((outcome, damage?))
    }

    /// Whether the person has an undo step to take: one of this replica's, not trimmed by
    /// [`DocSession::retain_steps`].
    pub fn can_undo(&self) -> bool {
        self.can_undo_in(UndoScope::Person)
    }

    pub fn can_redo(&self) -> bool {
        self.can_redo_in(UndoScope::Person)
    }

    pub fn can_undo_in(&self, scope: UndoScope) -> bool {
        self.replica.can_undo_in(scope) && self.index.undo_steps() > 0
    }

    pub fn can_redo_in(&self, scope: UndoScope) -> bool {
        self.replica.can_redo_in(scope) && self.index.redo_steps() > 0
    }

    /// Keeps only the newest `undo` undo steps and the next `redo` redo steps reachable (the
    /// engine history's own) and drops the blobs only older steps needed: the replica has no
    /// limit of its own, so this is what keeps a long session's media bounded.
    pub fn retain_steps(&mut self, undo: usize, redo: usize) {
        self.index.retain(undo, redo);
        self.index.collect_garbage(&mut self.blobs);
        let blobs = &self.blobs;
        self.blob_log.retain(|(_, hashes)| hashes.iter().any(|hash| blobs.0.contains_key(hash)));
    }

    /// Undo and redo steps reachable, as [`DocSession::retain_steps`] left them.
    pub fn steps(&self) -> (usize, usize) {
        (self.index.undo_steps(), self.index.redo_steps())
    }

    /// Imports remote or stored changes (their blobs added first, [`DocSession::add_blobs`]),
    /// repairs the merged state and patches the view. An import whose repair or view load fails
    /// has still imported its changes: [`DocSession::view_loaded`] tells whether the view
    /// followed.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage> {
        if let Err(error) = self.replica.import_changes(bytes) {
            // A refused import of a newer schema's changes leaves the state as it was but clears
            // the replica's undo history: drop the steps (and the blobs only they kept) too.
            if [UndoScope::Person, UndoScope::Agent].iter().all(|&scope| !self.replica.can_undo_in(scope) && !self.replica.can_redo_in(scope)) {
                self.index.retain(0, 0);
                self.index.collect_garbage(&mut self.blobs);
            }
            return Err(kit_err(error));
        }
        let (damage, moved) = self.settle();
        self.log_added(moved.added.iter().cloned());
        self.index.imported();
        self.index.collect_garbage(&mut self.blobs);
        damage
    }

    /// Takes a draft's changes (made on a fork of this replica) in as one transaction of `origin`
    /// (`Replica::merge_changes_repaired`): the repair of the merged state ([`KitModel::validate`])
    /// is part of that change and undo step, which is the person's even for an agent's draft
    /// (M12a). Patches the view as an import does; add the draft's blobs first
    /// ([`DocSession::add_blobs`]). Answers the undo scope the step went to (none for a `system`
    /// or `import` origin, or when nothing changed) and the damage. A refused merge, or one whose
    /// repair fails, changes nothing.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<(Option<UndoScope>, Damage)> {
        let before = self.replica.version();
        let merged = self.replica.merge_changes_repaired(bytes, origin, |tx, changed| {
            DocModel::validate(tx, changed).map(|_| ()).map_err(|error| KdmError::Engine(error.to_string()))
        });
        match merged {
            Ok(merged) => self.took_in(before, merged.scope),
            Err(error) => {
                self.replica.take_changed();
                Err(kit_err(error))
            }
        }
    }

    /// What merging a draft's changes would leave: the canonical state, nothing changed.
    pub fn merged_state(&self, bytes: &[u8]) -> Result<serde_json::Value> {
        self.replica.merged_state(bytes).map_err(kit_err)
    }

    /// Writes the state `snapshot` holds (a snapshot or content of this document, as a host's
    /// checkpoint keeps it) over the replica as one transaction of `origin`, keyed by object id
    /// (`kition_kit::session::write_state`), so every object the two share keeps its id: one undo
    /// step of its scope unless `origin` is a `system` or `import` one. Nothing changes when the
    /// states are equal. Refuses another document's snapshot. Add the blobs it references first.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<(u32, Damage)> {
        let schema = DocModel::schema();
        let loaded = Replica::load(Arc::clone(&schema), snapshot, ActorId::random()).map_err(kit_err)?;
        if loaded.doc_id() != self.replica.doc_id() {
            return Err(kit_err(KdmError::WrongDocument { expected: self.replica.doc_id().to_string(), found: loaded.doc_id().to_string() }));
        }
        let (now, then) = (kition_kit::session::state(&self.replica)?, kition_kit::session::state(&loaded)?);
        if now == then {
            return Ok((0, Damage::None));
        }
        let before = self.replica.version();
        // The repair of the restored state is part of the same change and undo step, as a merge's.
        let result = self.replica.transact(origin, |tx| {
            kition_kit::session::write_state(tx, &schema, &now, &then).map_err(crate::kdm_err)?;
            DocModel::validate(tx, &BTreeSet::new()).map(|_| ()).map_err(|error| KdmError::Engine(error.to_string()))
        });
        if let Err(error) = result {
            self.replica.take_changed();
            return Err(kit_err(error));
        }
        self.took_in(before, UndoScope::of(origin.kind))
    }

    /// After a merge or a restore wrote one transaction (an undo step of `scope`, if any) since
    /// `before`: patches the view as an import does, and counts the transaction as an undo step.
    /// A failed view load is the error, as an import's is: the transaction stays
    /// ([`DocSession::view_loaded`] tells whether the view followed). Answers the step's scope.
    fn took_in(&mut self, before: Version, scope: Option<UndoScope>) -> Result<(Option<UndoScope>, Damage)> {
        let scope = scope.filter(|_| self.replica.version() != before);
        let (damage, moved) = self.settle();
        self.log_added(moved.added.iter().cloned());
        if scope.is_some() {
            self.index.push_step(moved);
        }
        self.index.imported();
        self.index.collect_garbage(&mut self.blobs);
        damage.map(|damage| (scope, damage))
    }

    /// Whether the view matches the replica as it is now (its own read succeeds).
    pub fn view_loaded(&self) -> bool {
        read_document(&self.replica, &self.blobs).is_ok()
    }

    /// The blobs a peer at `since` needs to reach this session's current state (Kition Photo's
    /// `blobs_since`): the hashes logged for a version `since` does not include, still in the
    /// store. An empty `since` answers every blob the current state references (a creating
    /// transaction or a snapshot is never logged).
    pub fn blobs_since(&self, since: &Version) -> Vec<(String, Arc<[u8]>)> {
        if *since == Version::empty() {
            return self.index.referenced().into_iter().filter_map(|hash| self.blobs.0.get(&hash).cloned().map(|bytes| (hash, bytes))).collect();
        }
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for (version, hashes) in &self.blob_log {
            if since.includes(version) {
                continue;
            }
            for hash in hashes {
                if seen.insert(hash.clone())
                    && let Some(bytes) = self.blobs.0.get(hash)
                {
                    out.push((hash.clone(), bytes.clone()));
                }
            }
        }
        out
    }

    fn log_added(&mut self, added: impl IntoIterator<Item = String>) {
        let added: Vec<String> = added.into_iter().collect();
        if !added.is_empty() {
            self.blob_log.push((self.replica.version(), added));
        }
    }

    pub fn export_changes(&self, since: &Version) -> Result<Vec<u8>> {
        self.replica.export_changes(since).map_err(kit_err)
    }

    pub fn version(&self) -> Version {
        self.replica.version()
    }

    pub fn snapshot(&self) -> Result<Vec<u8>> {
        self.replica.snapshot().map_err(kit_err)
    }

    pub fn state_hash(&self) -> Result<String> {
        self.replica.state_hash().map_err(kit_err)
    }

    pub fn doc_id(&self) -> DocId {
        self.replica.doc_id()
    }

    pub fn replica(&self) -> &Replica {
        &self.replica
    }

    /// The blobs the replica references or an undo, redo or pending import can still need.
    pub fn blobs(&self) -> &MemoryBlobs {
        &self.blobs
    }

    /// Adds blobs (from a peer, before importing its changes): kept until the next import. Blobs
    /// that arrive after their changes (the view read them as empty) are read into the view: the
    /// answer is whether the view changed.
    pub fn add_blobs(&mut self, blobs: &MemoryBlobs) -> Result<bool> {
        for (hash, bytes) in &blobs.0 {
            self.blobs.0.entry(hash.clone()).or_insert_with(|| bytes.clone());
        }
        self.index.protect(blobs.0.keys().cloned());
        if !blobs.0.keys().any(|hash| self.model.missing_blobs().contains(hash)) {
            return Ok(false);
        }
        let source: &dyn BlobSource = &self.blobs;
        self.model.apply(&self.replica, &BTreeSet::new(), source)?;
        Ok(true)
    }

    /// As `KitSession`'s: repairs what changed in a `system` transaction (not an undo step), then
    /// patches the view with both; the view follows the replica even if the repair fails. Also
    /// answers which blobs moved (the index is updated either way).
    fn settle(&mut self) -> (Result<Damage>, Moved) {
        let mut changed = self.replica.take_changed();
        let repaired = if changed.is_empty() {
            Ok(())
        } else {
            let mut kit_error = None;
            let result = self.replica.transact(&Origin::system("repair"), |tx: &mut Tx<'_>| {
                DocModel::validate(tx, &changed).map(|_| ()).map_err(|error| {
                    let message = error.to_string();
                    kit_error = Some(error);
                    KdmError::Engine(message)
                })
            });
            match (result, kit_error) {
                (_, Some(error)) => Err(error),
                (result, None) => result.map_err(kit_err),
            }
        };
        changed.extend(self.replica.take_changed());
        let moved = self.index.update(&self.replica, &changed);
        let damage = self.refresh(&changed).and_then(|damage| repaired.map(|()| damage));
        let damage = match damage {
            Ok(damage) => self.repair_read(Some(&changed)).map(|repaired| match repaired {
                Some(more) => damage.merge(more),
                None => damage,
            }),
            failed => failed,
        };
        (damage, moved)
    }

    /// The read repair (`crate::repair`): dangling anchors and references in run marks the view
    /// already drops, written as a `system` transaction (not an undo step), then the view
    /// patched. `changed`: what the last import, undo or redo touched; `None` checks everything.
    /// Answers the view's damage when it wrote.
    fn repair_read(&mut self, changed: Option<&BTreeSet<ObjectId>>) -> Result<Option<Damage>> {
        let repairs = crate::repair::plan(&self.replica, self.model.ids(), changed);
        if repairs.is_empty() {
            return Ok(None);
        }
        let result = self.replica.transact(&Origin::system("repair"), |tx| crate::repair::apply(tx, &repairs));
        let changed = self.replica.take_changed();
        result.map_err(kit_err)?;
        let moved = self.index.update(&self.replica, &changed);
        self.log_added(moved.added.iter().cloned());
        self.refresh(&changed).map(Some)
    }

    fn refresh(&mut self, changed: &BTreeSet<ObjectId>) -> Result<Damage> {
        if changed.is_empty() {
            return Ok(Damage::None);
        }
        let blobs: &dyn BlobSource = &self.blobs;
        self.model.apply(&self.replica, changed, blobs)
    }
}
