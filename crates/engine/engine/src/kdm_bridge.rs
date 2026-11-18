//! The command bridge to the document's KDM replica (Kition phase 3e, Task 2; Kition Slide's
//! bridge, same owner and licence).
//!
//! The replica ([`DocSession`]) is the document's shared source of truth; [`Session::doc`] is the
//! engine's own view of it. Commands run exactly as before on the in-memory [`Document`]; at the
//! end of every top-level [`Session::run`] (and of [`Session::undo`]/[`Session::redo`],
//! [`Session::restore`] and [`Session::set_document`] called directly) the bridge writes what
//! changed since the replica last saw it, one transaction per undo step pushed since, found by
//! [`Session::pushed`] (a counter, not the stack's length: at the limit a push drops the oldest
//! step). A change that pushed no step belongs to the step on top: typing coalesced into one undo
//! step is one transaction per keystroke, all of them that step's; so is a drag's every frame. A
//! nested command pushes no step of its own ([`Session::checkpoint`]), so one top-level command is
//! one transaction; an automatic change Word undoes on its own (AutoCorrect, AutoFormat:
//! [`Session::push_undo`]) is its own.
//!
//! Undo is per person: each undo step remembers how many replica undo steps it took (`steps`);
//! undo and redo step the replica that many times and take the replica's result (with every
//! collaborator's edit since) as the document, not the engine's own snapshot. A step the replica
//! does not hold (`None`: taken before a reset, put back by [`Session::restore`], or past what the
//! replica still counts) undoes from the engine's snapshot, written as a `system` change. The
//! selection, the find state, the clipboard, painters, the view, preferences, versions and macros
//! are this person's own: never written; after an import, undo or redo the selection follows its
//! paragraph and is clamped to what is there.
//!
//! A replica that refuses a write or an undo is left ([`Session::kdm_resets`]): the document keeps
//! working from its own history snapshots, and the next edit makes a fresh replica.

use kdm_model::DocSession;
use kdm_model::kition_kit::kdm::{ObjectId, Origin, OriginKind, UndoScope, Version};
use kdm_model::kition_kit::session::{Damage, MemoryBlobs};
use kition_doc_doc::{Block, Document, Path, Pos, StoryRef};

use crate::session::Undo;
use crate::{CmdError, Session};

/// The origin of the local person's commands (used when nothing else was set with
/// [`Session::execute_as`]/[`Session::set_origin`]).
pub fn local_person() -> Origin {
    Origin::person("person:local")
}

fn kdm_error(error: impl std::fmt::Display) -> CmdError {
    CmdError::Failed(format!("document replica: {error}"))
}

fn no_replica() -> CmdError {
    CmdError::Failed("the document has no replica and none could be made".into())
}

/// The document's bridge state.
#[derive(Debug)]
pub(crate) struct Link {
    /// The replica, made lazily (the first edit or passthrough); `None` again after a reset.
    pub(crate) kdm: Option<DocSession>,
    /// Without a replica: the document one is made from (the document as opened, or as the
    /// replica last saw it). With one, the replica's view is what it saw.
    base: Document,
    /// [`Session::rev`] and [`Session::pushed`] when the bridge last looked.
    rev: u64,
    mark: u64,
    resets: u64,
    /// Replica undo steps this session ever recorded, and imports it ever took (only grow).
    recorded: u64,
    imports: u64,
    /// [`Session::set_document`] replaced the document since the last look.
    pub(crate) replaced: bool,
}

/// What [`Session::restore`] needs of the bridge: its counters when the snapshot was taken, and
/// the replica steps of the undo step then on top (typing may add to it).
#[derive(Debug, Clone)]
pub(crate) struct Mark {
    recorded: u64,
    imports: u64,
    resets: u64,
    top: Option<Option<u32>>,
}

impl Link {
    pub(crate) fn new(doc: &Document) -> Self {
        Self { kdm: None, base: doc.clone(), rev: 1, mark: 0, resets: 0, recorded: 0, imports: 0, replaced: false }
    }

    pub(crate) fn mark(&self, top: Option<Option<u32>>) -> Mark {
        Mark { recorded: self.recorded, imports: self.imports, resets: self.resets, top }
    }
}

/// Where a position's paragraph is, to find it again after the document changed under it: its
/// block's object, and its text then.
struct Anchor {
    object: Option<ObjectId>,
    text: Option<String>,
}

impl Session {
    /// Who the running (or next) command acts for: the origin of [`Session::execute_as`], or the
    /// local person.
    pub fn origin(&self) -> Origin {
        self.origin.clone().unwrap_or_else(local_person)
    }

    /// Sets who the following commands act for (`None`: the local person); answers the previous
    /// setting, to put back.
    pub fn set_origin(&mut self, origin: Option<Origin>) -> Option<Origin> {
        std::mem::replace(&mut self.origin, origin)
    }

    /// [`Session::run`] as `origin` (a person, an agent): the replica records the edit as theirs,
    /// and it undoes as this replica's own step.
    pub fn execute_as(&mut self, id: &str, params: &serde_json::Value, origin: &Origin) -> Result<serde_json::Value, CmdError> {
        let previous = self.set_origin(Some(origin.clone()));
        let r = self.run(id, params);
        self.set_origin(previous);
        r
    }

    /// The document's replica, if it has made one (never makes one).
    pub fn kdm(&self) -> Option<&DocSession> {
        self.link.kdm.as_ref()
    }

    /// How many times the document left its replica. Hosts watch this: the next edit's fresh
    /// replica is a new KDM document collaborators must join from its snapshot.
    pub fn kdm_resets(&self) -> u64 {
        self.link.resets
    }

    /// The replica undo steps of each undo step, oldest first (see the module docs).
    pub fn kdm_undo_steps(&self) -> Vec<Option<u32>> {
        self.history.iter().map(|u| u.steps).collect()
    }

    /// The document's KDM identity, `None` before it has made a replica.
    pub fn doc_id(&self) -> Option<String> {
        Some(self.kdm()?.doc_id().to_string())
    }

    /// The replica, made now if there is none yet (anything not written yet is written first).
    fn active_kdm(&mut self) -> Result<&mut DocSession, CmdError> {
        self.sync_kdm();
        if self.link.kdm.is_none() {
            let mut kdm = DocSession::create(&self.doc).map_err(kdm_error)?;
            kdm.set_comment_only(self.comment_only);
            self.link.kdm = Some(kdm);
        }
        self.link.kdm.as_mut().ok_or_else(no_replica)
    }

    /// The replica's version.
    pub fn version(&mut self) -> Result<Version, CmdError> {
        Ok(self.active_kdm()?.version())
    }

    /// The replica as a snapshot, for a peer ([`Session::open_snapshot`]).
    pub fn snapshot(&mut self) -> Result<Vec<u8>, CmdError> {
        self.active_kdm()?.snapshot().map_err(kdm_error)
    }

    /// SHA-256 of the replicated state: equal on replicas that converged.
    pub fn state_hash(&mut self) -> Result<String, CmdError> {
        self.active_kdm()?.state_hash().map_err(kdm_error)
    }

    /// The changes since `since` (`Version::empty()` for all of them).
    pub fn export_changes(&mut self, since: &Version) -> Result<Vec<u8>, CmdError> {
        self.active_kdm()?.export_changes(since).map_err(kdm_error)
    }

    /// Every blob the replica references or can still bring back.
    pub fn blobs(&mut self) -> Result<MemoryBlobs, CmdError> {
        Ok(self.active_kdm()?.blobs().clone())
    }

    /// Adds a peer's blobs (before importing its changes): kept until the next import.
    pub fn add_blobs(&mut self, blobs: &MemoryBlobs) -> Result<(), CmdError> {
        let anchors = self.anchors();
        // Blobs that arrive after their changes: the document reads them now.
        if self.active_kdm()?.add_blobs(blobs).map_err(kdm_error)? {
            self.follow_kdm(anchors);
        }
        Ok(())
    }

    /// The blobs a peer at `since` needs (`Version::empty()`: every blob the current state
    /// references), so a live change ships only its own pictures.
    pub fn blobs_since(&mut self, since: &Version) -> Result<MemoryBlobs, CmdError> {
        Ok(MemoryBlobs(self.active_kdm()?.blobs_since(since).into_iter().collect()))
    }

    /// Imports a collaborator's (or stored) changes (their blobs added first,
    /// [`Session::add_blobs`]). Local edits not written yet are written first. The document
    /// follows the replica and the selection follows its paragraphs; a view that could not be
    /// loaded leaves the replica (a reset) rather than diff against it later.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage, CmdError> {
        let anchors = self.anchors();
        let kdm = self.active_kdm()?;
        let before = kdm.version();
        let result = kdm.import_changes(bytes);
        let changed = kdm.version() != before;
        let follow = result.is_ok() || (changed && kdm.view_loaded());
        if changed {
            self.link.imports = self.link.imports.saturating_add(1);
            if follow {
                self.follow_kdm(anchors);
            } else {
                self.detach_kdm();
            }
        }
        self.trim_kdm();
        result.map_err(kdm_error)
    }

    /// Imports only the saved-file difference, retaining the live replica and its undo scopes.
    pub fn reread_document(&mut self, after: &Document) -> Result<Damage, CmdError> {
        let anchors = self.anchors();
        let kdm = self.active_kdm()?;
        let before = kdm.version();
        kdm.record(&Origin::new(OriginKind::Import, "file reread"), after).map_err(kdm_error)?;
        if kdm.version() == before {
            return Ok(Damage::None);
        }
        self.link.imports = self.link.imports.saturating_add(1);
        self.follow_kdm(anchors);
        Ok(Damage::Full)
    }

    /// Opens another replica's snapshot (with the blobs it references) as the document: a replica
    /// of the same KDM document under a fresh actor, clean, with an empty history.
    pub fn open_snapshot(&mut self, bytes: &[u8], blobs: MemoryBlobs) -> Result<(), CmdError> {
        let kdm = DocSession::open_snapshot(bytes, blobs).map_err(kdm_error)?;
        self.attach(kdm);
        Ok(())
    }

    /// [`Session::open_snapshot`] for a host that keeps a document's replica beside its saved
    /// file. The file is what the person saved: when `file` is not empty and the replica does not
    /// hold its content (id-blind, and apart from the save stamp a save writes into the file but
    /// not the replica, [`Session::save_stamped`]), the file wins, recorded onto the replica as
    /// one `import` change (not an undo step) under the replica's own identity, so collaborators
    /// get it as ordinary changes. The document opens clean either way.
    pub fn open_with_replica(&mut self, file: &[u8], snapshot: &[u8], blobs: MemoryBlobs) -> Result<(), CmdError> {
        let loaded = if file.is_empty() { None } else { Some(kdm_model::decode("", file).map_err(kdm_error)?) };
        let mut kdm = DocSession::open_snapshot(snapshot, blobs).map_err(kdm_error)?;
        let file_core = loaded.as_ref().map(|doc| doc.core.clone());
        if let Some(doc) = loaded {
            let mut unstamped = doc.clone();
            crate::session::copy_stamp(&kdm.document().core, &mut unstamped.core);
            if !kdm.holds(&unstamped).map_err(kdm_error)? {
                kdm.record(&Origin::system("import"), &doc).map_err(kdm_error)?;
            }
        }
        self.attach(kdm);
        // The file's save stamp is the last save's: saving with nothing new writes it again.
        if let Some(core) = file_core.filter(|core| !core.modified.is_empty()) {
            let mut saved = self.doc.clone();
            crate::session::copy_stamp(&core, &mut saved.core);
            self.saved = Some(saved);
        }
        Ok(())
    }

    /// Makes `kdm`'s view the document, as a new document (see [`Session::set_document`]).
    fn attach(&mut self, kdm: DocSession) {
        self.set_document(kdm.document().clone());
        self.link.replaced = false;
        let mut kdm = kdm;
        kdm.set_comment_only(self.comment_only);
        self.link.kdm = Some(kdm);
        self.link.mark = self.pushed;
        // `set_document` may have made the view nonempty: the next look writes that.
        self.link.rev = self.rev.wrapping_sub(1);
        self.dirty = false;
        if self.depth == 0 {
            self.sync_kdm();
        }
    }

    /// The undo stack's indexes of the steps pushed since [`Session::pushed`] was `mark` that are
    /// still on it (the newest ones).
    fn pushed_since(&self, mark: u64) -> std::ops::Range<usize> {
        let new = usize::try_from(self.pushed.saturating_sub(mark)).unwrap_or(usize::MAX);
        let len = self.history.len();
        len.saturating_sub(new)..len
    }

    fn set_steps(&mut self, i: usize, steps: Option<u32>) {
        if let Some(u) = self.history.get_mut(i) {
            std::sync::Arc::make_mut(u).steps = steps;
        }
    }

    /// Writes what the document changed since the replica last saw it (see the module docs).
    pub(crate) fn sync_kdm(&mut self) {
        if self.link.rev == self.rev && self.link.mark == self.pushed && !self.link.replaced {
            return;
        }
        self.link.rev = self.rev;
        let origin = self.origin();
        let pushed = self.pushed_since(self.link.mark);
        self.link.mark = self.pushed;
        if std::mem::take(&mut self.link.replaced) {
            // A new document: nobody's undo step, and no step on the stacks holds anything of it.
            let doc = self.doc.clone();
            if self.link.kdm.is_some() {
                self.record_kdm(&Origin::system("replace"), &doc);
            } else {
                self.link.base = doc;
            }
            for u in self.history.iter_mut().chain(self.redo.iter_mut()) {
                std::sync::Arc::make_mut(u).steps = None;
            }
        } else if pushed.is_empty() {
            let top = self.history.len().checked_sub(1).filter(|_| self.redo.is_empty());
            let doc = self.doc.clone();
            self.record_onto(top, &origin, &doc);
        } else {
            // What changed before the first new step (the protection check's track-changes
            // switch) belongs to the step below it.
            let resets = self.link.resets;
            if let Some(first) = self.history.get(pushed.start).map(|u| u.doc.clone()) {
                self.record_onto(pushed.start.checked_sub(1), &origin, &first);
            }
            for i in pushed {
                if self.link.resets != resets {
                    // The replica refused a write and was left: the next edit starts afresh.
                    break;
                }
                let after = self.history.get(i + 1).map(|u| u.doc.clone()).unwrap_or_else(|| self.doc.clone());
                let steps = self.record_kdm(&origin, &after);
                self.set_steps(i, steps);
            }
        }
        self.trim_kdm();
    }

    /// Writes `after` as part of undo step `onto` (adding to its replica steps), or as a `system`
    /// change when there is no such step or the replica does not hold it.
    fn record_onto(&mut self, onto: Option<usize>, origin: &Origin, after: &Document) {
        let held = onto.and_then(|i| self.history.get(i)).and_then(|u| u.steps);
        match (onto, held) {
            (Some(i), Some(had)) => {
                if let Some(n) = self.record_kdm(origin, after) {
                    self.set_steps(i, Some(had.saturating_add(n)));
                }
            }
            _ => {
                self.record_kdm(&Origin::system("view"), after);
            }
        }
    }

    /// Writes `after` on top of the replica (made now, from [`Link::base`], if there is none) and
    /// answers the undo steps that took; `None` when the replica refused and was left.
    fn record_kdm(&mut self, origin: &Origin, after: &Document) -> Option<u32> {
        if self.link.kdm.is_none() {
            match DocSession::create(&self.link.base) {
                Ok(mut kdm) => {
                    kdm.set_comment_only(self.comment_only);
                    self.link.kdm = Some(kdm);
                }
                Err(error) => {
                    log::warn!("the document can't be held in a replica: {error}");
                    self.link.base = after.clone();
                    return Some(0);
                }
            }
        }
        let kdm = self.link.kdm.as_mut()?;
        match kdm.record(origin, after) {
            Ok(steps) => {
                self.link.recorded = self.link.recorded.saturating_add(u64::from(steps));
                Some(steps)
            }
            Err(error) => {
                log::warn!("the document replica refused an edit: {error}");
                self.detach_kdm();
                None
            }
        }
    }

    /// Leaves the replica: every step's replica steps were the dropped replica's, and a fresh one
    /// never holds them.
    fn detach_kdm(&mut self) {
        self.link.kdm = None;
        self.link.resets = self.link.resets.saturating_add(1);
        self.link.base = self.doc.clone();
        self.link.rev = self.rev;
        self.link.mark = self.pushed;
        self.none_held();
    }

    fn none_held(&mut self) {
        for u in self.history.iter_mut().chain(self.redo.iter_mut()) {
            if u.steps.is_some() {
                std::sync::Arc::make_mut(u).steps = None;
            }
        }
    }

    /// Lets the replica forget the steps the history can no longer take, and their blobs.
    pub(crate) fn trim_kdm(&mut self) {
        let sum = |v: &[std::sync::Arc<Undo>]| v.iter().map(|u| u.steps.unwrap_or(0) as usize).fold(0usize, usize::saturating_add);
        let (undo, redo) = (sum(&self.history), sum(&self.redo));
        if let Some(kdm) = self.link.kdm.as_mut() {
            kdm.retain_steps(undo, redo);
        }
    }

    /// After [`Session::undo`] or [`Session::redo`] set the document to the step's own snapshot
    /// (`reference`): steps the replica too and takes its view, with every collaborator's edit
    /// since, instead. A step the replica does not hold (or no longer counts) keeps the snapshot,
    /// which the next look writes as a `system` change.
    pub(crate) fn step_kdm(&mut self, steps: Option<u32>, undo: bool, scope: UndoScope, reference: &Document) {
        let Some(kdm) = self.link.kdm.as_mut() else { return };
        let Some(steps) = steps else { return };
        let available = if undo { kdm.steps().0 } else { kdm.steps().1 };
        if usize::try_from(steps).unwrap_or(usize::MAX) > available {
            // The replica no longer counts the steps this one took (the history limit trimmed
            // them, a restore brought the step back): this and every older step undo from their
            // snapshots.
            self.none_held();
            self.trim_kdm();
            return;
        }
        if steps > 0 && !(0..steps).all(|_| matches!(if undo { kdm.undo_in(scope) } else { kdm.redo_in(scope) }, Ok((true, _)))) {
            self.detach_kdm();
            return;
        }
        let view = kdm.document().clone();
        let sel = self.sel.clone();
        self.doc = view;
        self.sel.anchor = follow_by_content(reference, &self.doc, &sel.anchor);
        self.sel.focus = follow_by_content(reference, &self.doc, &sel.focus);
    }

    /// [`Session::step`] under the other scope's later steps: the replica undoes (or redoes) the
    /// step's `steps` transactions of `scope` and the document becomes its view, with the other
    /// scope's later edits and every collaborator's. `false`, changing nothing, when the replica
    /// does not hold them.
    pub(crate) fn replay_kdm(&mut self, steps: u32, undo: bool, scope: UndoScope) -> bool {
        let Some(kdm) = self.link.kdm.as_mut() else { return false };
        let (available, can) = if undo { (kdm.steps().0, kdm.can_undo_in(scope)) } else { (kdm.steps().1, kdm.can_redo_in(scope)) };
        if steps == 0 || !can || usize::try_from(steps).unwrap_or(usize::MAX) > available {
            return false;
        }
        if !(0..steps).all(|_| matches!(if undo { kdm.undo_in(scope) } else { kdm.redo_in(scope) }, Ok((true, _)))) {
            self.detach_kdm();
            return false;
        }
        let view = kdm.document().clone();
        let before = std::mem::replace(&mut self.doc, view);
        let sel = self.sel.clone();
        self.sel.anchor = follow_by_content(&before, &self.doc, &sel.anchor);
        self.sel.focus = follow_by_content(&before, &self.doc, &sel.focus);
        true
    }

    /// [`Session::restore`]'s part: the replica undoes what it recorded since the snapshot, the
    /// step then on top gets its replica steps back, and steps the replica no longer holds (redo
    /// steps a new transaction cleared) undo from their snapshots.
    pub(crate) fn restore_kdm(&mut self, mark: &Mark) {
        if self.link.resets != mark.resets {
            self.none_held();
            return;
        }
        let k = self.link.recorded.saturating_sub(mark.recorded);
        if let Some(top) = mark.top
            && let Some(last) = self.history.len().checked_sub(1)
        {
            self.set_steps(last, top);
        }
        if k > 0 {
            for u in self.redo.iter_mut() {
                std::sync::Arc::make_mut(u).steps = None;
            }
        }
        let scope = self.undo_scope();
        let Some(kdm) = self.link.kdm.as_mut() else { return };
        if !(0..k).all(|_| matches!(kdm.undo_in(scope), Ok((true, _)))) {
            self.detach_kdm();
            return;
        }
        self.link.recorded = mark.recorded;
        if self.link.imports != mark.imports {
            // A collaborator's edit came in meanwhile: it stays.
            let view = kdm.document().clone();
            self.doc = view;
            self.clamp_selection();
        }
        self.trim_kdm();
    }

    /// Where the selection's ends are, by object, before an import.
    fn anchors(&self) -> [Anchor; 2] {
        let ids = self.link.kdm.as_ref().map(|k| k.model().ids());
        let anchor = |p: &Pos| Anchor {
            object: ids.and_then(|ids| ids.block_at(part_of(p.story), &p.path.0)).cloned(),
            text: self.doc.para_at(p).map(|q| q.text.clone()),
        };
        [anchor(&self.sel.anchor), anchor(&self.sel.focus)]
    }

    /// After an import: the document follows the replica, and the selection follows its
    /// paragraphs (by object), its offsets past what changed before them.
    fn follow_kdm(&mut self, anchors: [Anchor; 2]) {
        let Some(kdm) = self.link.kdm.as_ref() else { return };
        let ids = kdm.model().ids();
        let doc = kdm.document().clone();
        let [a, f] = anchors;
        let moved = |pos: &Pos, at: Anchor| -> Pos {
            let found = at.object.as_ref().and_then(|o| ids.find_block(o));
            let mut p = match found {
                Some((part, path)) => Pos { story: part.map_or(StoryRef::Body, StoryRef::Part), path: Path(path), off: pos.off },
                None => pos.clone(),
            };
            if let (Some(old), Some(new)) = (at.text.as_deref(), doc.para_at(&p).map(|q| q.text.as_str())) {
                p.off = shift_offset(old, new, p.off);
            }
            doc.clamp(&p)
        };
        let anchor = moved(&self.sel.anchor, a);
        let focus = moved(&self.sel.focus, f);
        self.doc = doc;
        // Two replicas that each made a drawing object or picture gave both the same number (and
        // an equation's number is its place): the view renumbers as a document opened from the
        // replica does (`set_document`, C8-doc-6), the later one in story order taking the next
        // free number, the same in every replica; the next local write records it.
        self.doc.assign_object_ids();
        self.sel.anchor = anchor;
        self.sel.focus = focus;
        self.also_selected.clear();
        self.column = None;
        self.find.results.clear();
        self.clamp_selection();
        self.touch();
        self.link.rev = self.rev;
    }
}

fn part_of(story: StoryRef) -> Option<u32> {
    match story {
        StoryRef::Body => None,
        StoryRef::Part(id) => Some(id),
    }
}

/// `off` (in `old`) in `new`, one paragraph's text before and after a change: unchanged before
/// what changed, moved with the text after it, at the end of the change inside it.
fn shift_offset(old: &str, new: &str, off: usize) -> usize {
    let prefix: usize = old.chars().zip(new.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum();
    if off <= prefix {
        return off;
    }
    let room = old.len().min(new.len()).saturating_sub(prefix);
    let suffix: usize = old.chars().rev().zip(new.chars().rev()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum::<usize>().min(room);
    let tail = old.len().saturating_sub(off);
    if tail <= suffix { new.len().saturating_sub(tail) } else { new.len().saturating_sub(suffix) }
}

/// `pos` (made on `old`) on `new` after an undo or redo through the replica: the top-level block
/// it is in, found by `Arc` identity or equal content near its old place (a collaborator may
/// have added or removed blocks before it); the rest of its path as it was. Clamped afterwards.
fn follow_by_content(old: &Document, new: &Document, pos: &Pos) -> Pos {
    const NEAR: usize = 64;
    let (Some(was), Some(blocks)) =
        (old.story(pos.story).and_then(|b| b.get(pos.path.0.first().copied().unwrap_or(0) as usize)), new.story(pos.story))
    else {
        return pos.clone();
    };
    let at = pos.path.0.first().copied().unwrap_or(0) as usize;
    let same = |b: &std::sync::Arc<Block>| std::sync::Arc::ptr_eq(b, was);
    let equal = |b: &std::sync::Arc<Block>| **b == **was;
    let near = |test: &dyn Fn(&std::sync::Arc<Block>) -> bool| -> Option<usize> {
        (0..=NEAR).find_map(|d| {
            let up = at.checked_sub(d).filter(|i| blocks.get(*i).is_some_and(test));
            up.or_else(|| Some(at + d).filter(|i| blocks.get(*i).is_some_and(test)))
        })
    };
    match near(&same).or_else(|| near(&equal)) {
        Some(i) => {
            let mut path = pos.path.clone();
            if let Some(first) = path.0.first_mut() {
                *first = u32::try_from(i).unwrap_or(u32::MAX);
            }
            Pos { story: pos.story, path, off: pos.off }
        }
        None => pos.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::shift_offset;

    #[test]
    fn offsets_follow_text_changed_before_them() {
        assert_eq!(shift_offset("hello world", "hello brave world", 8), 14, "after an insert before it");
        assert_eq!(shift_offset("hello world", "hello brave world", 3), 3, "before the change");
        assert_eq!(shift_offset("hello world", "world", 8), 2, "after a deletion before it");
        assert_eq!(shift_offset("abc", "", 2), 0);
        assert_eq!(shift_offset("abc", "xabc", 3), 4, "a letter added at the start");
    }
}
