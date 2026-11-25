//! The editing session: document, selection, undo/redo, layout, view state.

use std::sync::Arc;

use kition_doc_doc::edit::Fragment;
use kition_doc_doc::props::CharProps;
use kition_doc_doc::{Document, Path, Pos, StoryRef};
use kition_doc_layout::{DocLayout, LayoutCache, LayoutOptions, ViewMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CmdError, Registry};
use kdm_model::kition_kit::kdm::UndoScope;

/// A selection: anchor (where it started) and focus (where the caret is).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Selection {
    pub anchor: Pos,
    pub focus: Pos,
}

impl Selection {
    pub fn caret(p: Pos) -> Self {
        Selection { anchor: p.clone(), focus: p }
    }
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.focus
    }
    /// (start, end) in document order.
    pub fn ordered(&self) -> (Pos, Pos) {
        if self.anchor <= self.focus { (self.anchor.clone(), self.focus.clone()) } else { (self.focus.clone(), self.anchor.clone()) }
    }
}

/// A column (block) selection: the same x range on every line between two corners.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColumnBlock {
    /// The stream selection the block was made with (its corners). The block is current only
    /// while `Session::sel` still equals it, so any ordinary caret move or selection drops it.
    pub sel: Selection,
    /// Page x of the anchor and focus corners (points); the block spans the range between.
    pub anchor_x: f32,
    pub focus_x: f32,
    /// One `(start, end)` per line, in document order, each inside one paragraph.
    pub segments: Vec<(Pos, Pos)>,
}

/// View state that commands can change (ribbon View tab, status bar).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewState {
    pub zoom: f32,
    pub mode: ViewMode,
    pub read_mode: bool,
    pub focus_mode: bool,
    pub marks: bool,
    pub ruler: bool,
    /// View › Show › Gridlines: the drawing grid over the page's text area (on screen only).
    pub gridlines: bool,
    /// Table Layout › View Gridlines: outlines of table cells (on screen only); on by default, as in Word.
    #[serde(default = "on")]
    pub table_gridlines: bool,
    pub nav_pane: bool,
    pub styles_pane: bool,
    /// Style Inspector pane (Home › Styles).
    #[serde(default)]
    pub style_inspector: bool,
    pub comments_pane: bool,
    /// The Clipboard pane (Home › Clipboard): items collected by Copy and Cut.
    #[serde(default)]
    pub clipboard_pane: bool,
    pub multi_page: bool,
    /// Zoom to fit: "pageWidth", "onePage", "multiplePages", or empty.
    pub fit: String,
    pub web_width: f32,
    pub dark_mode: bool,
    pub show_markup: bool,
    pub track_changes_pane: bool,
    /// Check spelling and grammar as you type.
    pub proofing: bool,
    /// Review › Hide Ink: ink strokes aren't shown on screen (they stay in the document).
    #[serde(default)]
    pub hide_ink: bool,
    /// Draw tab: the tool dragging on the page uses, and each pen's colour and thickness.
    #[serde(default)]
    pub draw: crate::cmd::draw::DrawState,
}

fn on() -> bool {
    true
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            zoom: 1.0,
            mode: ViewMode::Print,
            read_mode: false,
            focus_mode: false,
            marks: false,
            ruler: true,
            gridlines: false,
            table_gridlines: true,
            nav_pane: false,
            styles_pane: false,
            style_inspector: false,
            comments_pane: false,
            clipboard_pane: false,
            multi_page: false,
            fit: String::new(),
            web_width: 800.0,
            dark_mode: false,
            show_markup: true,
            track_changes_pane: false,
            proofing: true,
            hide_ink: false,
            draw: Default::default(),
        }
    }
}

/// Find/replace state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FindState {
    pub query: String,
    pub replace: String,
    pub match_case: bool,
    pub whole_word: bool,
    pub regex: bool,
    /// Matches from the last search: (start, end).
    #[serde(skip)]
    pub results: Vec<(Pos, Pos)>,
    pub current: usize,
    /// Reading Highlight: mark every match of `query` in the body.
    pub highlight: bool,
    /// Highlighted matches, for the document revision they were found in.
    #[serde(skip)]
    pub highlights: Option<(u64, Vec<(Pos, Pos)>)>,
}

#[derive(Clone)]
pub(crate) struct Undo {
    pub(crate) label: String,
    pub(crate) doc: Document,
    pub(crate) sel: Selection,
    /// How many of the document replica's undo steps this step took ([`crate::kdm_bridge`]):
    /// `Some(0)` for a step that changed nothing replicated, `None` for one the replica does not
    /// hold (taken before a reset, or put back by [`Session::restore`]), which undoes from this
    /// snapshot instead.
    pub(crate) steps: Option<u32>,
    /// Whose history the step is in (KitModel M12a): the person's, or the agents' working for
    /// them, as the origin of the command that made it.
    pub(crate) scope: UndoScope,
}

/// What [`Session::restore`] needs to take commands back exactly: the document, the selection,
/// the undo and redo stacks (also the oldest undo steps the undo limit pushes out meanwhile), the
/// open typing group, a pending [`Session::join_next_undo`] and the dirty flag. See
/// [`Session::edit_snapshot`].
pub struct EditSnapshot {
    doc: Document,
    sel: Selection,
    history_len: usize,
    /// `Session::undo_evicted` when the snapshot was taken.
    evicted: u64,
    /// The oldest undo steps, kept only when the stack is near its limit (the commands that
    /// follow may push them out); at most [`SNAPSHOT_HEAD`].
    head: Vec<Arc<Undo>>,
    redo: Vec<Arc<Undo>>,
    typing_open: bool,
    join_next: bool,
    dirty: bool,
    /// The replica bridge's counters then (see [`crate::kdm_bridge::Mark`]).
    kdm: crate::kdm_bridge::Mark,
}

/// Undo steps that the commands run between [`Session::edit_snapshot`] and [`Session::restore`]
/// may push out of a full stack and still come back.
const SNAPSHOT_HEAD: usize = 4;

impl EditSnapshot {
    /// The document when the snapshot was taken.
    pub fn doc(&self) -> &Document {
        &self.doc
    }
    /// The dirty flag then.
    pub fn dirty(&self) -> bool {
        self.dirty
    }
    /// The number of undo steps then.
    pub fn undo_depth(&self) -> usize {
        self.history_len
    }
}

/// An editing session.
pub struct Session {
    pub doc: Document,
    pub sel: Selection,
    pub view: ViewState,
    /// Formatting picked with a collapsed caret, applied to the next typed text. It belongs to
    /// the story it was picked in: moving the caret to another story drops it.
    pub pending: Option<CharProps>,
    /// Where the caret was in the body when it last went into a header, footer, note or text
    /// box: where Close Header and Footer (and leaving a note) take it back to.
    pub body_return: Option<Pos>,
    pub path: Option<std::path::PathBuf>,
    pub dirty: bool,
    pub clipboard: Option<Fragment>,
    /// Plain text mirror of the clipboard (for the system clipboard).
    pub clipboard_text: String,
    /// Items collected by Copy and Cut this session, for the Clipboard pane.
    pub clip_history: crate::cmd::edit::ClipHistory,
    pub find: FindState,
    /// Page x the caret tries to keep on Up/Down.
    pub goal_x: Option<f32>,
    /// Page the caret was last on (headers/footers repeat on many pages).
    pub page_hint: usize,
    pub registry: Arc<Registry>,
    /// Author name for comments and tracked changes.
    pub author: String,
    /// Format painter: copied formatting waiting to be applied (and whether it stays on).
    pub painter: Option<(CharProps, kition_doc_doc::props::ParaProps, bool)>,
    /// Last message for the status bar / agents.
    pub status: String,
    /// Undo and redo steps. Shared so `run` can snapshot both stacks cheaply (a pointer per
    /// step) and put them back exactly when a command fails.
    pub(crate) history: Vec<Arc<Undo>>,
    /// Undo steps dropped so far because of the undo limit.
    undo_evicted: u64,
    pub(crate) redo: Vec<Arc<Undo>>,
    /// Typing is coalesced into one undo step until something else happens.
    typing_open: bool,
    /// The next mutating command joins the previous undo step (later frames of a drag).
    join_next: bool,
    pub(crate) rev: u64,
    /// Which document this is: changes when [`Session::set_document`] replaces it (open, new,
    /// mail merge, recover), never on an edit.
    document_id: u64,
    cache: LayoutCache,
    layout: Option<(u64, f32, ViewMode, Arc<DocLayout>, bool)>,
    /// Picture edits: edited media key → original media key (Reset Picture).
    pub originals: std::collections::HashMap<String, String>,
    /// Last mutating command (Repeat).
    pub last_command: Option<(String, Value)>,
    /// The mutating command before the running one: AutoFormat on Enter acts only right after
    /// typing ([`Session::typed_just_before`]).
    previous_command: Option<(String, Value)>,
    /// Macro being recorded: (name, steps).
    pub recording: Option<(String, Vec<(String, Value)>)>,
    pub macros: std::collections::BTreeMap<String, Vec<(String, Value)>>,
    pub autocorrect_on: bool,
    pub autocorrect_user: Vec<(String, String)>,
    /// Saved versions: (label, date, document).
    pub versions: Vec<(String, String, Document)>,
    /// Quick Parts / AutoText entries.
    pub building_blocks: std::collections::BTreeMap<String, Fragment>,
    pub autosave: bool,
    /// Citation style: APA, MLA, Chicago, IEEE.
    pub bib_style: String,
    /// Mail merge data source and preview.
    pub merge: crate::cmd::mailings::MergeState,
    /// Requests from commands to the UI (open a dialog, scroll…), drained by the front end.
    pub ui_requests: Vec<Value>,
    /// Read Aloud player (Review › Speech).
    pub read_aloud: crate::speech::ReadAloud,
    /// Preferences the front end saves between runs.
    pub prefs: Prefs,
    /// Editing inside an equation: which one and the caret in it.
    pub math: Option<MathEdit>,
    /// Equations are typed in LaTeX rather than the linear format.
    pub math_latex: bool,
    /// Text typed into equations is normal (non-math) text.
    pub math_normal_text: bool,
    /// The password Word documents are saved with (File › Info › Protect Document › Encrypt with
    /// Password, `file.encrypt`); `None` saves them unencrypted. Kept from a password-protected
    /// file that was opened, cleared when the document is replaced.
    pub password: Option<crate::io::Password>,
    /// More pictures and shapes selected along with the one the selection holds (Shift+click),
    /// for Group. Cleared by any edit or selection change other than adding to it.
    pub also_selected: Vec<Pos>,
    /// Column (block) selection, if one was made (see [`Session::column_segments`]).
    pub column: Option<ColumnBlock>,
    /// Column selection mode (Ctrl+Shift+F8): caret movement extends the block.
    pub column_mode: bool,
    /// The document as last saved (or as opened, when its file has a save stamp), with the save
    /// stamp it was written with: a save with nothing new since writes that stamp again, so its
    /// bytes are the same ([`Session::save_stamped`]).
    pub(crate) saved: Option<Document>,
    /// The undo limit (default 500).
    pub(crate) undo_limit: usize,
    /// How many undo steps were ever pushed (only grows): the replica bridge finds the steps a
    /// command pushed by it, since at the limit a push leaves the stack's length unchanged.
    pub(crate) pushed: u64,
    /// How deep [`Session::run`] is nested: 0 between top-level calls.
    pub(crate) depth: u32,
    /// Whether the running top-level command pushed an undo step: a nested command then pushes
    /// none, so one top-level command is one undo step (and one replica transaction).
    pub(crate) run_pushed: bool,
    /// The document's KDM replica and what it last saw ([`crate::kdm_bridge`]).
    pub(crate) link: crate::kdm_bridge::Link,
    /// Who the commands act for on the replica; `None`: the local person.
    pub(crate) origin: Option<kdm_model::kition_kit::kdm::Origin>,
    /// Only comments may be written (a commenter of a shared document, Kit UI protocol
    /// `document.comments`): [`Session::set_comment_only`].
    pub(crate) comment_only: bool,
    /// Every file belongs to the host ([`Session::for_host`]: a kit's document; KHP A29): the
    /// engine reads only what the host lends and reaches nothing else by path ([`crate::disk`]).
    host_files: Option<crate::disk::HostFiles>,
    /// What anyone but a person may not run (an agent, a `model.batch` script or extension, a
    /// host's own change), checked before every command however it arrives: directly, as a
    /// batch's step, as a step of a macro (`macro.list`) or as a repeat (`edit.repeat`). The
    /// kit's own list: commands that name files or start programs.
    pub agent_gate: Option<fn(&str) -> Result<(), String>>,
    /// Every paragraph and table row keeps a unique `w14:paraId`, its tool id
    /// ([`Document::ensure_para_ids`], C8-land-14-doc), every comment its durable id and every
    /// drawing object, picture and equation its number (C8-doc-6, [`Document::ensure_tool_ids`]):
    /// checked after every top-level command.
    /// A kit's session turns it on; the engine's own tests leave
    /// documents as they read them.
    pub stable_ids: bool,
    /// Stamp every top-level command that changes the document with the save stamp (modified
    /// time, last modified by, revision) as it runs (C8-doc-2): a session served for a host's
    /// agents, whose receipts must say what the saved file holds, the stamp included. A save
    /// then writes the document as it is ([`Session::saved_as_is`]).
    pub stamp_edits: bool,
}

/// The equation being edited.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MathEdit {
    /// The equation object (its U+FFFC).
    pub at: Pos,
    /// The caret inside it.
    pub pos: kition_office_core::math::edit::MathPos,
}

/// Editing preferences that persist between runs (the front end saves and restores them).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// Word Count includes text boxes, footnotes and endnotes (Word's default).
    pub count_notes: bool,
    /// Track Changes Options: what markup shows and how revisions are drawn (per user, as in
    /// Word).
    pub markup: kition_doc_layout::display::MarkupOptions,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { count_notes: true, markup: Default::default() }
    }
}

/// Maximum undo depth.
const MAX_UNDO: usize = 500;

impl Session {
    /// An isolated command-planning session; no history or transient input is shared mutably.
    pub fn fork_for_premises(&self) -> Result<Self, CmdError> {
        let mut fork = Self::new(self.doc.clone());
        fork.sel = self.sel.clone();
        fork.view = self.view.clone();
        fork.pending = self.pending.clone();
        fork.body_return = self.body_return.clone();
        fork.path = self.path.clone();
        fork.dirty = self.dirty;
        fork.clipboard = self.clipboard.clone();
        fork.clipboard_text = self.clipboard_text.clone();
        fork.clip_history = self.clip_history.clone();
        fork.find = self.find.clone();
        fork.goal_x = self.goal_x;
        fork.page_hint = self.page_hint;
        fork.registry = self.registry.clone();
        fork.author = self.author.clone();
        fork.painter = self.painter.clone();
        fork.history = self
            .history
            .iter()
            .map(|step| {
                let mut step = (**step).clone();
                step.steps = None;
                Arc::new(step)
            })
            .collect();
        fork.redo = self
            .redo
            .iter()
            .map(|step| {
                let mut step = (**step).clone();
                step.steps = None;
                Arc::new(step)
            })
            .collect();
        fork.originals = self.originals.clone();
        fork.last_command = self.last_command.clone();
        fork.previous_command = self.previous_command.clone();
        fork.macros = self.macros.clone();
        fork.autocorrect_on = self.autocorrect_on;
        fork.autocorrect_user = self.autocorrect_user.clone();
        fork.versions = self.versions.clone();
        fork.building_blocks = self.building_blocks.clone();
        fork.bib_style = self.bib_style.clone();
        fork.merge = self.merge.clone();
        fork.prefs = self.prefs.clone();
        fork.math = self.math.clone();
        fork.math_latex = self.math_latex;
        fork.math_normal_text = self.math_normal_text;
        fork.password = self.password.clone();
        fork.also_selected = self.also_selected.clone();
        fork.column = self.column.clone();
        fork.column_mode = self.column_mode;
        fork.saved = self.saved.clone();
        fork.host_files = self.host_files;
        fork.agent_gate = self.agent_gate;
        fork.stable_ids = self.stable_ids;
        fork.stamp_edits = self.stamp_edits;
        fork.comment_only = self.comment_only;
        fork.link.kdm = self.link.kdm.as_ref().map(kdm_model::DocSession::fork).transpose().map_err(|error| CmdError::Failed(error.to_string()))?;
        Ok(fork)
    }

    pub fn new(doc: Document) -> Self {
        let start = doc.start_of(StoryRef::Body);
        let mut session = Session {
            link: crate::kdm_bridge::Link::new(&doc),
            undo_limit: MAX_UNDO,
            pushed: 0,
            depth: 0,
            run_pushed: false,
            origin: None,
            comment_only: false,
            host_files: None,
            agent_gate: None,
            stable_ids: false,
            stamp_edits: false,
            doc,
            sel: Selection::caret(start),
            view: ViewState::default(),
            pending: None,
            body_return: None,
            path: None,
            dirty: false,
            clipboard: None,
            clipboard_text: String::new(),
            clip_history: Default::default(),
            find: FindState::default(),
            goal_x: None,
            page_hint: 0,
            registry: Arc::new(crate::cmd::registry()),
            author: "Kition Doc User".into(),
            painter: None,
            status: String::new(),
            history: Vec::new(),
            undo_evicted: 0,
            redo: Vec::new(),
            typing_open: false,
            join_next: false,
            rev: 1,
            document_id: 1,
            cache: LayoutCache::new(),
            layout: None,
            originals: Default::default(),
            last_command: None,
            previous_command: None,
            recording: None,
            macros: Default::default(),
            autocorrect_on: true,
            autocorrect_user: Vec::new(),
            versions: Vec::new(),
            building_blocks: Default::default(),
            autosave: true,
            bib_style: "APA".into(),
            merge: Default::default(),
            ui_requests: Vec::new(),
            math: None,
            math_latex: false,
            math_normal_text: false,
            also_selected: Vec::new(),
            prefs: Prefs::default(),
            read_aloud: Default::default(),
            password: None,
            column: None,
            column_mode: false,
            saved: None,
        };
        session.saved = session.stamped_baseline();
        session
    }

    /// A session whose files all belong to the host (a kit's document, served headless or in the
    /// page; KHP A29, A45): it reads a file a command names only through `files`, by its path in
    /// the workspace, reaches nothing else by path and starts no program ([`crate::disk`]).
    pub fn for_host(doc: Document, files: crate::disk::HostFiles) -> Self {
        let mut session = Session::new(doc);
        session.host_files = Some(files);
        session
    }

    /// Whether the host owns every file ([`Session::for_host`]).
    pub fn host_files_only(&self) -> bool {
        self.host_files.is_some()
    }

    /// The line pieces of the current column selection, if there is one (the selection hasn't
    /// moved since the block was made).
    pub fn column_segments(&self) -> Option<&[(Pos, Pos)]> {
        self.column.as_ref().filter(|c| c.sel == self.sel).map(|c| c.segments.as_slice())
    }

    /// Document revision (bumped by every change).
    pub fn rev(&self) -> u64 {
        self.rev
    }
    /// Which document is open (see `document_id`): lets a caller tell an edit from a replacement.
    pub fn document_id(&self) -> u64 {
        self.document_id
    }
    pub fn touch(&mut self) {
        self.rev = self.rev.wrapping_add(1);
        self.dirty = true;
    }

    /// Reading Highlight ranges for the current document (searched again after edits); empty when it's off.
    pub fn find_highlights(&mut self) -> &[(Pos, Pos)] {
        if !self.find.highlight {
            return &[];
        }
        if self.find.highlights.as_ref().is_none_or(|(r, _)| *r != self.rev) {
            let found = crate::cmd::edit::search(self, StoryRef::Body).unwrap_or_default();
            self.find.highlights = Some((self.rev, found));
        }
        self.find.highlights.as_ref().map(|(_, v)| v.as_slice()).unwrap_or(&[])
    }

    /// The current layout (recomputed when the document or view changed).
    pub fn layout(&mut self) -> Arc<DocLayout> {
        let ww = self.view.web_width;
        if let Some((r, w, m, l, pf)) = &self.layout
            && *r == self.rev
            && (*w == ww || self.view.mode == ViewMode::Print)
            && *m == self.view.mode
            && *pf == self.view.proofing
        {
            return l.clone();
        }
        let opts = LayoutOptions {
            view: self.view.mode,
            web_width: ww,
            show_hidden: self.view.marks,
            hide_deleted: !self.view.show_markup || self.prefs.markup.hides_deletions(),
            proofing: self.view.proofing,
        };
        let l = Arc::new(kition_doc_layout::layout(&self.doc, &mut self.cache, &opts));
        self.layout = Some((self.rev, ww, self.view.mode, l.clone(), self.view.proofing));
        l
    }
    /// A layout for output (PDF, images, print): no proofing marks, print view.
    pub fn export_layout(&self) -> Arc<DocLayout> {
        let opts = LayoutOptions { view: ViewMode::Print, web_width: 0.0, show_hidden: false, hide_deleted: false, proofing: false };
        Arc::new(kition_doc_layout::layout(&self.doc, &mut LayoutCache::new(), &opts))
    }

    /// The document as it prints ([`crate::print::printed`]: page references updated) and its
    /// print layout.
    pub fn export_document(&self) -> (std::borrow::Cow<'_, kition_doc_doc::Document>, Arc<DocLayout>) {
        match crate::print::printed(&self.doc) {
            Some(doc) => {
                let opts = LayoutOptions { view: ViewMode::Print, web_width: 0.0, show_hidden: false, hide_deleted: false, proofing: false };
                let layout = Arc::new(kition_doc_layout::layout(&doc, &mut LayoutCache::new(), &opts));
                (std::borrow::Cow::Owned(doc), layout)
            }
            None => (std::borrow::Cow::Borrowed(&self.doc), self.export_layout()),
        }
    }

    /// Invalidate the cached layout (fonts changed etc.).
    pub fn relayout(&mut self) {
        self.layout = None;
        self.cache.clear();
    }

    /// Snapshot for undo before a change.
    /// Inside a command that already pushed its step (a nested command), nothing: one top-level
    /// command is one undo step.
    pub fn checkpoint(&mut self, label: &str) {
        if label == "Typing" && self.typing_open {
            return;
        }
        if self.depth > 1 && self.run_pushed {
            return;
        }
        self.typing_open = label == "Typing";
        let scope = self.undo_scope();
        self.push_step(Undo { label: label.to_string(), doc: self.doc.clone(), sel: self.sel.clone(), steps: Some(0), scope });
        // A new step clears its own history's redo, never the other scope's.
        self.redo.retain(|u| u.scope != scope);
    }
    pub(crate) fn push_step(&mut self, u: Undo) {
        self.history.push(Arc::new(u));
        self.pushed = self.pushed.saturating_add(1);
        if self.depth > 0 {
            self.run_pushed = true;
        }
        while self.history.len() > self.undo_limit.max(1) {
            self.history.remove(0);
            self.undo_evicted += 1;
        }
    }
    /// Sets the undo limit (at least 1), dropping the oldest steps past it.
    pub fn set_undo_limit(&mut self, limit: usize) {
        self.undo_limit = limit.max(1);
        while self.history.len() > self.undo_limit {
            self.history.remove(0);
            self.undo_evicted += 1;
        }
        self.trim_kdm();
    }
    /// Record `doc`/`sel` as their own undo step after the fact and close the typing group, so
    /// Undo right after an automatic change (AutoFormat, AutoCorrect) reverts only that change.
    pub fn push_undo(&mut self, label: &str, doc: Document, sel: Selection) {
        let scope = self.undo_scope();
        self.push_step(Undo { label: label.to_string(), doc, sel, steps: Some(0), scope });
        self.typing_open = false;
    }
    /// Make the next mutating command part of the previous undo step instead of a new one, so a
    /// drag that runs a command every frame is a single Undo. Call it on every frame of the drag
    /// except the first; it is consumed by the next `run`.
    pub fn join_next_undo(&mut self) {
        self.join_next = true;
    }
    /// Stamp the document's save metadata (modified time, last modified by, revision; created
    /// when it has none) the way Word does on a save, when the document changed since it was
    /// last saved or opened: a save with nothing new keeps the stamp it has, so it writes the
    /// same bytes. `file.save` calls it, and so does any front end that writes the bytes itself
    /// (the web build's download).
    pub fn stamp_save(&mut self) {
        let stamped = self.stamp_of(&self.doc);
        if stamped.core != self.doc.core {
            // A change the replica bridge must see (the dirty flag is the caller's).
            self.rev = self.rev.wrapping_add(1);
            self.doc.core = stamped.core;
        }
        self.saved = Some(self.doc.clone());
    }
    /// The document as a save writes it, stamped like [`Session::stamp_save`], leaving the
    /// document (and its replica) as it is: for a front end that keeps the document's replica
    /// beside the file (Kition's embedded editor), where saving is not an edit of the shared
    /// document. [`Session::open_with_replica`] compares a file to its replica without the stamp.
    /// The session remembers what it wrote, so saving again with nothing new writes the same.
    pub fn save_stamped(&mut self) -> Document {
        let doc = self.stamp_of(&self.doc);
        self.saved = Some(doc.clone());
        doc
    }
    /// A save of the document as it is ([`Session::stamp_edits`]: its commands stamped it): what
    /// the next command's stamp compares with.
    pub fn saved_as_is(&mut self) -> Document {
        self.saved = Some(self.doc.clone());
        self.doc.clone()
    }
    /// `doc` with the stamp of the last save when nothing but the stamp differs from what that
    /// save wrote, else with a new stamp (on the last save's: its created time and revision).
    fn stamp_of(&self, doc: &Document) -> Document {
        let mut doc = doc.clone();
        if let Some(saved) = &self.saved {
            copy_stamp(&saved.core, &mut doc.core);
            if doc == *saved {
                return doc;
            }
        }
        stamp_core(&mut doc.core, &self.author);
        doc
    }
    /// Close an open typing group (caret moved, other command).
    pub fn close_typing(&mut self) {
        self.typing_open = false;
    }
    /// The undo scope of who runs the next command ([`Session::origin`]): `edit.undo` and
    /// `edit.redo` work in it unless their `scope` parameter names another (engine rule E14a).
    pub fn undo_scope(&self) -> UndoScope {
        UndoScope::of(self.origin().kind).unwrap_or(UndoScope::Person)
    }
    pub fn can_undo(&self) -> bool {
        self.can_undo_in(self.undo_scope())
    }
    pub fn can_redo(&self) -> bool {
        self.can_redo_in(self.undo_scope())
    }
    pub fn can_undo_in(&self, scope: UndoScope) -> bool {
        self.history.iter().any(|u| u.scope == scope)
    }
    pub fn can_redo_in(&self, scope: UndoScope) -> bool {
        self.redo.iter().any(|u| u.scope == scope)
    }
    pub fn undo_label(&self) -> Option<&str> {
        let scope = self.undo_scope();
        self.history.iter().rev().find(|u| u.scope == scope).map(|u| u.label.as_str())
    }
    pub fn redo_label(&self) -> Option<&str> {
        let scope = self.undo_scope();
        self.redo.iter().rev().find(|u| u.scope == scope).map(|u| u.label.as_str())
    }
    /// The number of undo steps in [`Session::undo_scope`].
    pub fn undo_depth(&self) -> usize {
        let scope = self.undo_scope();
        self.history.iter().filter(|u| u.scope == scope).count()
    }
    pub fn undo_labels(&self) -> Vec<String> {
        let scope = self.undo_scope();
        self.history.iter().rev().filter(|u| u.scope == scope).map(|u| u.label.clone()).collect()
    }
    /// Undoes the last step of [`Session::undo_scope`]. On a document with a replica, the replica
    /// steps back the transactions the step took and the document becomes its result, which keeps
    /// every collaborator's edit since, and the other scope's ([`crate::kdm_bridge`]).
    pub fn undo(&mut self) -> bool {
        self.undo_in(self.undo_scope())
    }
    pub fn redo(&mut self) -> bool {
        self.redo_in(self.undo_scope())
    }
    pub fn undo_in(&mut self, scope: UndoScope) -> bool {
        self.step(false, scope)
    }
    pub fn redo_in(&mut self, scope: UndoScope) -> bool {
        self.step(true, scope)
    }
    fn step(&mut self, redo: bool, scope: UndoScope) -> bool {
        self.typing_open = false;
        // What changed since the last write belongs to the step on top.
        self.sync_kdm();
        let from = if redo { &self.redo } else { &self.history };
        let Some(i) = from.iter().rposition(|u| u.scope == scope) else { return false };
        if i + 1 < from.len() {
            // The other scope has steps after this one: only the replica can take this one back
            // and keep theirs.
            return self.step_under(redo, i, scope);
        }
        let from = if redo { &mut self.redo } else { &mut self.history };
        let Some(u) = from.pop().map(Arc::unwrap_or_clone) else { return false };
        let reference = u.doc.clone();
        let cur = Undo {
            label: u.label.clone(),
            doc: std::mem::replace(&mut self.doc, u.doc),
            sel: std::mem::replace(&mut self.sel, u.sel),
            steps: u.steps,
            scope,
        };
        if redo {
            self.history.push(Arc::new(cur))
        } else {
            self.redo.push(Arc::new(cur))
        }
        self.step_kdm(u.steps, !redo, scope, &reference);
        self.touch();
        self.clamp_selection();
        if self.depth == 0 {
            self.sync_kdm();
        }
        true
    }
    /// [`Session::step`] for a step under the other scope's later ones: the replica undoes (or
    /// redoes) its transactions and the document becomes the replica's view. A step the replica
    /// does not hold can't be taken back without the later steps, so there is nothing to do.
    fn step_under(&mut self, redo: bool, i: usize, scope: UndoScope) -> bool {
        let from = if redo { &self.redo } else { &self.history };
        let Some(steps) = from.get(i).and_then(|u| u.steps) else { return false };
        let before = self.doc.clone();
        let Some((taken, past)) = self.replay_kdm(steps, !redo, scope) else { return false };
        let from = if redo { &mut self.redo } else { &mut self.history };
        let u = Arc::unwrap_or_clone(from.remove(i));
        self.consume_older(!redo, scope, past);
        let cur = Undo { label: u.label, doc: before, sel: self.sel.clone(), steps: Some(taken), scope };
        if redo {
            self.history.push(Arc::new(cur))
        } else {
            self.redo.push(Arc::new(cur))
        }
        self.touch();
        self.clamp_selection();
        true
    }
    /// Drop history (after open / new).
    pub fn reset_history(&mut self) {
        self.history.clear();
        self.redo.clear();
        self.typing_open = false;
    }

    /// Replace the document (open/new).
    pub fn set_document(&mut self, doc: Document) {
        // The replica (if the document has one) gets the new document as one change that is
        // nobody's undo step ([`crate::kdm_bridge`]).
        self.link.replaced = true;
        self.document_id = self.document_id.wrapping_add(1);
        self.doc = doc;
        self.doc.ensure_nonempty();
        // Every shape has an id of its own (KitDrawing DR2): a file's drawing ids, else new ones.
        self.doc.assign_object_ids();
        self.password = None;
        self.sel = Selection::caret(self.doc.start_of(StoryRef::Body));
        self.pending = None;
        self.body_return = None;
        self.also_selected.clear();
        self.column = None;
        self.column_mode = false;
        self.reset_history();
        self.touch();
        self.dirty = false;
        self.saved = self.stamped_baseline();
        self.relayout();
    }

    /// The document as it is, as the last save when it carries a save stamp (a file saved
    /// before): saving it again with nothing new writes the stamp it has.
    pub(crate) fn stamped_baseline(&self) -> Option<Document> {
        (!self.doc.core.modified.is_empty()).then(|| self.doc.clone())
    }

    /// A snapshot to put back with [`Session::restore`], for a caller that runs a command and
    /// may want to take it back without a trace (cheap: the document's blocks are shared).
    ///
    /// Covered: `doc`, `sel`, the undo stack (its length, and its oldest steps when it is near
    /// the limit), `redo`, the open typing group, a pending `join_next_undo` and `dirty`. `rev`,
    /// `cache` and `layout` are derived (`restore` bumps `rev`). The other fields are not
    /// covered: a caller that lets the command change them (the view, the find state, the
    /// clipboard, macros, the file path…) saves and puts them back itself.
    ///
    /// `restore` is exact when the commands in between only add undo steps (every mutating
    /// command adds at most one; up to [`SNAPSHOT_HEAD`] may push old steps out of a full
    /// stack). Taking steps off the stack (`edit.undo`) or clearing it (`file.new`,
    /// `file.open`) in between is not covered.
    ///
    /// The destructuring lists every field, so a new field does not build until someone decides
    /// whether a snapshot must cover it.
    pub fn edit_snapshot(&self) -> EditSnapshot {
        let Session {
            doc,
            sel,
            view: _,
            pending: _,
            // Like the caret's pending formatting: where the caret goes back to, not an edit.
            body_return: _,
            path: _,
            dirty,
            clipboard: _,
            clipboard_text: _,
            clip_history: _,
            find: _,
            goal_x: _,
            page_hint: _,
            registry: _,
            author: _,
            painter: _,
            status: _,
            history,
            undo_evicted,
            redo,
            typing_open,
            join_next,
            rev: _,
            cache: _,
            layout: _,
            originals: _,
            last_command: _,
            previous_command: _,
            recording: _,
            macros: _,
            autocorrect_on: _,
            autocorrect_user: _,
            versions: _,
            building_blocks: _,
            autosave: _,
            bib_style: _,
            merge: _,
            ui_requests: _,
            document_id: _,
            read_aloud: _,
            prefs: _,
            // Equation editing mode, like `view`: not part of the document or its history.
            math: _,
            math_latex: _,
            // Where files come from and who may run what: the session's setup, not an edit.
            host_files: _,
            agent_gate: _,
            stable_ids: _,
            stamp_edits: _,
            math_normal_text: _,
            // The save password, like the file path: not an edit to the document.
            password: _,
            also_selected: _,
            // Column selection, like `sel`'s shape: valid only while `sel` matches it.
            column: _,
            column_mode: _,
            // What the last save wrote: not an edit (a document taken back to it saves the same).
            saved: _,
            undo_limit,
            // The replica bridge: what `restore` must take back of it is in `kdm`.
            pushed: _,
            depth: _,
            run_pushed: _,
            link,
            origin: _,
            // Who may edit: fixed for the session, not an edit.
            comment_only: _,
        } = self;
        let head = if history.len() + SNAPSHOT_HEAD > *undo_limit { history.iter().take(SNAPSHOT_HEAD).cloned().collect() } else { Vec::new() };
        EditSnapshot {
            doc: doc.clone(),
            sel: sel.clone(),
            history_len: history.len(),
            evicted: *undo_evicted,
            head,
            redo: redo.clone(),
            typing_open: *typing_open,
            join_next: *join_next,
            dirty: *dirty,
            kdm: link.mark(history.last().map(|u| u.steps)),
        }
    }

    /// Put a snapshot back: the commands run since leave no trace in the document, the
    /// selection or undo/redo (undo steps the limit pushed out meanwhile come back). The layout
    /// is recomputed.
    ///
    /// On a document with a replica, the replica undoes the transactions the commands took (a
    /// collaborator's edit since stays, and then the document is the replica's result).
    pub fn restore(&mut self, snap: EditSnapshot) {
        self.sync_kdm();
        let EditSnapshot { doc, sel, history_len, evicted, head, redo, typing_open, join_next, dirty, kdm } = snap;
        let gone = usize::try_from(self.undo_evicted.saturating_sub(evicted)).unwrap_or(usize::MAX);
        if gone > 0 {
            let back = gone.min(head.len());
            self.history.splice(0..0, head.into_iter().take(back));
        }
        self.undo_evicted = evicted;
        self.doc = doc;
        self.sel = sel;
        self.also_selected.clear();
        self.history.truncate(history_len);
        self.redo = redo;
        self.typing_open = typing_open;
        self.join_next = join_next;
        self.restore_kdm(&kdm);
        self.touch();
        self.dirty = dirty;
        if self.depth == 0 {
            self.sync_kdm();
        }
    }

    /// Make the selection valid for the current document.
    pub fn clamp_selection(&mut self) {
        self.sel.anchor = self.doc.clamp(&self.sel.anchor);
        self.sel.focus = self.doc.clamp(&self.sel.focus);
        // The equation being edited must still be there (undo, edits elsewhere).
        if let Some(m) = &self.math {
            let eq = self.doc.para_at(&m.at).and_then(|p| match p.object_at(m.at.off) {
                Some(kition_doc_doc::InlineObject::Equation { math, .. }) => Some(kition_office_core::math::edit::clamp(&math.nodes, &m.pos)),
                _ => None,
            });
            match eq {
                Some(pos) => {
                    if let Some(m) = self.math.as_mut() {
                        m.pos = pos;
                    }
                }
                None => self.math = None,
            }
        }
    }

    /// Opens the document for commenting only (a commenter of a shared document): every command
    /// that changes the document but the comment commands is refused, typing included; undo and
    /// redo take back one's own comments. Only comment objects change then (what a sync server
    /// lets a commenter push).
    pub fn set_comment_only(&mut self, comment_only: bool) {
        self.comment_only = comment_only;
    }

    /// Whether only comments may be written ([`Session::set_comment_only`]).
    pub fn comment_only(&self) -> bool {
        self.comment_only
    }

    /// Run a command by id with JSON params (the single entry point for the UI, CLI, MCP and
    /// control channel). Mutating commands are undoable; a failed command leaves the document as
    /// it was; a panic inside a command becomes an error. After a top-level command, what it
    /// changed is written to the document's replica ([`crate::kdm_bridge`]).
    pub fn run(&mut self, id: &str, params: &Value) -> Result<Value, CmdError> {
        // A hosted session's command, and every command it runs, reaches files only through the
        // host and starts no program.
        let _files = crate::disk::enter(self.host_files);
        if self.depth == 0 {
            self.run_pushed = false;
        }
        let rev = self.rev;
        self.depth = self.depth.saturating_add(1);
        let result = self.run_command(id, params);
        self.depth = self.depth.saturating_sub(1);
        if self.depth == 0 {
            if self.stable_ids && self.doc.ensure_tool_ids() {
                self.rev = self.rev.wrapping_add(1);
            }
            if self.stamp_edits && self.rev != rev {
                let stamped = self.stamp_of(&self.doc);
                if stamped.core != self.doc.core {
                    self.doc.core = stamped.core;
                    self.rev = self.rev.wrapping_add(1);
                }
            }
            self.sync_kdm();
        }
        result
    }

    fn run_command(&mut self, id: &str, params: &Value) -> Result<Value, CmdError> {
        let join = std::mem::take(&mut self.join_next);
        let reg = self.registry.clone();
        let Some(spec) = reg.get(id) else { return Err(CmdError::Unknown(id.to_string())) };
        // What the guards below go by (`cmd::family`): a command's namespace does not say it.
        let family = crate::cmd::family::family(id);
        // Anyone but a person passes the kit's gate before every command, nested ones included
        // (KHP A29, A45): a batch's or a macro's step gets the same check as a direct call.
        if let Some(gate) = self.agent_gate
            && self.origin.as_ref().is_some_and(|origin| origin.kind != kdm_model::kition_kit::kdm::OriginKind::Person)
        {
            gate(id).map_err(CmdError::Disabled)?;
        }
        if let Some(why) = (spec.enabled)(self) {
            return Err(CmdError::Disabled(format!("{id}: {why}")));
        }
        // Open for commenting only: comments, and taking back one's own (rule M23).
        if spec.mutates && self.comment_only && !is_comment_command(id) && !matches!(id, "edit.undo" | "edit.redo") {
            return Err(CmdError::Disabled(format!("{id}: the document is open for commenting only")));
        }
        // Restrict Editing.
        if spec.mutates
            && let Some(mode) = self.doc.settings.protection.clone()
        {
            let allowed = id == "file.restrictEditing" || family == "file" || id == "edit.undo" || id == "edit.redo";
            // The commands on the current comment (the Review tab's); not the ones naming a comment by id.
            let comment_ok =
                matches!(id, "comment.new" | "comment.replyCurrent" | "comment.edit" | "comment.deleteCurrent" | "comment.resolveCurrent");
            match mode.as_str() {
                "readOnly" | "forms" if !allowed => return Err(CmdError::Disabled(format!("{id}: the document is protected (read only)"))),
                "comments" if !allowed && !comment_ok => return Err(CmdError::Disabled(format!("{id}: only comments are allowed in this document"))),
                "trackedChanges" => self.doc.settings.track_changes = true,
                _ => {}
            }
        }
        // Macro recording and Repeat.
        let record = !matches!(id, "macro.record" | "macro.list" | "edit.undo" | "edit.redo" | "edit.repeat")
            && (spec.mutates || family == "caret" || family == "select");
        if record && let Some((_, steps)) = self.recording.as_mut() {
            steps.push((id.to_string(), params.clone()));
        }
        if spec.mutates && !matches!(id, "edit.undo" | "edit.redo" | "edit.repeat") {
            self.previous_command = self.last_command.replace((id.to_string(), params.clone()));
        }
        // Typing, caret movement and selection outside the equation commands leave the equation.
        if self.math.is_some()
            && (matches!(family, "text" | "caret" | "select") || id.starts_with("edit.paste"))
            && let Some(m) = self.math.take()
        {
            let off = m.at.off + kition_doc_doc::para::OBJ.len_utf8();
            self.sel = Selection::caret(Pos { off, ..m.at });
        }
        // The undo stacks are snapshotted whole, not as a length: commands run nested commands,
        // whose checkpoints, undos and redos (and evictions at the limit) must be put back too.
        let before_doc = if spec.mutates {
            Some((
                self.doc.clone(),
                self.sel.clone(),
                self.history.clone(),
                self.redo.clone(),
                self.typing_open,
                self.undo_evicted,
                self.column.clone(),
                (self.pushed, self.run_pushed, self.link.replaced),
            ))
        } else {
            None
        };
        let typing = spec.id == "text.insert" || spec.id == "equation.type";
        if spec.mutates && !typing {
            self.typing_open = false;
        }
        if spec.mutates {
            let label = if typing { "Typing" } else { spec.label };
            if !join || self.history.is_empty() {
                self.checkpoint(label);
            }
        } else if !spec.id.starts_with("caret.") && !spec.id.starts_with("view.") {
            // Non-mutating commands other than caret movement keep the typing group.
        } else if spec.id.starts_with("caret.") {
            self.typing_open = false;
        }
        let sel_before = self.sel.clone();
        let return_before = self.body_return.clone();
        let run = spec.run;
        let mutates = spec.mutates;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::cmd::column::dispatch(self, id, mutates, run, params)));
        let result = match result {
            Ok(r) => r,
            Err(p) => {
                let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
                Err(CmdError::Failed(format!("{id} panicked: {msg}")))
            }
        };
        match &result {
            Ok(_) => {
                if spec.mutates {
                    // Formatting changed while tracking becomes a formatting revision.
                    if let Some((before, ..)) = &before_doc {
                        crate::cmd::fmt_revisions::record(self, id, before);
                    }
                    self.touch();
                    self.doc.ensure_nonempty();
                    self.doc.prune_text_boxes();
                    // A command that can make or copy shapes leaves each with an id of its own
                    // (KitDrawing DR2); typing and formatting cannot, so they skip the scan.
                    if self.depth == 1 && (matches!(family, "insert" | "ink" | "drawing" | "file") || id.starts_with("edit.paste")) {
                        self.doc.assign_object_ids();
                    }
                    // Connectors follow what any other edit moved, in the same step (`drawing.*`
                    // commands reroute their own; undo and redo put back what was; typing, which
                    // runs per key, is left out of the document scan).
                    if self.depth == 1
                        && !matches!(family, "drawing" | "text")
                        && !matches!(id, "edit.undo" | "edit.redo")
                        && self.doc.shapes().iter().any(|(_, o)| matches!(o, kition_doc_doc::para::InlineObject::Shape { connection: Some(_), .. }))
                    {
                        let _ = crate::cmd::drawing::reroute_connectors(self);
                    }
                }
                self.clamp_selection();
                // Caret formatting stays in its story; a caret leaving the body remembers its place.
                if self.sel.focus.story != sel_before.focus.story {
                    self.pending = None;
                    // A command that chose the place itself (a new note: its reference) keeps it.
                    if sel_before.focus.story == StoryRef::Body && self.body_return == return_before {
                        self.body_return = Some(sel_before.focus.clone());
                    }
                }
                // Extra selected objects last until something else is edited or selected.
                if !matches!(id, "select.addObject" | "arrange.ungroup") && (spec.mutates || self.sel != sel_before) {
                    self.also_selected.clear();
                }
            }
            Err(e) => {
                if let Some((d, s, h, r, t, ev, c, (pushed, run_pushed, replaced))) = before_doc {
                    // The steps it pushed are gone again, and a replacement it made is undone.
                    self.pushed = pushed;
                    self.run_pushed = run_pushed;
                    self.link.replaced = replaced;
                    self.doc = d;
                    self.sel = s;
                    self.history = h;
                    self.redo = r;
                    self.typing_open = t;
                    // The steps the command pushed out are back, so they no longer count as gone.
                    self.undo_evicted = ev;
                    self.column = c;
                }
                self.status = e.to_string();
            }
        }
        result
    }

    /// The paragraph path the caret is in.
    pub fn caret_path(&self) -> &Path {
        &self.sel.focus.path
    }
    pub fn story(&self) -> StoryRef {
        self.sel.focus.story
    }

    /// Whether the mutating command before the running one typed one character, as a key does
    /// (`text.insert` of a single character): AutoFormat as you type acts on typing, not on
    /// text an agent or a paste put in whole.
    pub fn typed_just_before(&self) -> bool {
        self.previous_command
            .as_ref()
            .is_some_and(|(id, params)| id == "text.insert" && params.get("text").and_then(Value::as_str).is_some_and(|t| t.chars().count() == 1))
    }

    /// The preceding command for a declaration asked before the next command starts.
    pub fn last_typed_one_char(&self) -> bool {
        self.last_command
            .as_ref()
            .is_some_and(|(id, params)| id == "text.insert" && params.get("text").and_then(Value::as_str).is_some_and(|t| t.chars().count() == 1))
    }

    /// Formatting typing would use at the caret.
    pub fn typing_props(&self) -> CharProps {
        if let Some(p) = &self.pending {
            return p.clone();
        }
        let f = &self.sel.focus;
        self.doc.para_at(f).map(|p| p.continuing_props(f.off)).unwrap_or_default()
    }

    /// Words in the document, as the status bar shows them (see [`Prefs::count_notes`]).
    pub fn word_count(&self) -> usize {
        if self.prefs.count_notes { self.doc.word_count_including_notes() } else { self.doc.word_count() }
    }

    /// Selected plain text.
    pub fn selected_text(&self) -> String {
        let (a, b) = self.sel.ordered();
        self.doc.copy_range(&a, &b).plain_text()
    }
}

/// The save stamp's fields of `from` into `to`.
pub(crate) fn copy_stamp(from: &kition_doc_doc::CoreProps, to: &mut kition_doc_doc::CoreProps) {
    to.modified.clone_from(&from.modified);
    to.created.clone_from(&from.created);
    to.last_modified_by.clone_from(&from.last_modified_by);
    to.revision = from.revision;
}

/// Word's save stamp: modified time, last modified by, revision; created when there is none.
pub(crate) fn stamp_core(core: &mut kition_doc_doc::CoreProps, author: &str) {
    core.modified = crate::cmd::now_iso();
    if core.created.is_empty() {
        core.created = core.modified.clone();
    }
    core.last_modified_by = author.to_owned();
    core.revision = core.revision.saturating_add(1);
}

/// The commands that write only comments (rule M23): what a commenter may run.
pub fn is_comment_command(id: &str) -> bool {
    id.starts_with("comment.")
}
