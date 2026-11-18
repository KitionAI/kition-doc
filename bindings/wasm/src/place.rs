//! What one call rests on (C8-land-14-doc item 4): `KitDocument::under` and `covered` for the
//! commands that address text by position, and the premises of the families whose write rests on
//! less than every field of its target. Read from each handler; every command not named here keeps
//! the SDK's default (every field and every position of each object it names).
//!
//! **Text by position** (`select.range`, `caret.set`, `select.column`, `review.suggestions`,
//! `equation.edit`): a position names a paragraph (by its body index, by a path of indexes into a
//! table cell or a story, or by its id) and a byte offset in it. A later step of the run acts on
//! that place through the selection. The write rests on:
//! - the paragraph being the one the address named: by index, its place in its list
//!   (`positions: ["body"]` or `["blocks"]`; a table's place in the body and a row's in its table
//!   for a cell path, and the row's `cells`), by id nothing about its place;
//! - its text before and inside the range (`covers: text`, answered by [`DocDocument::covered_call`]):
//!   an edit before the range moves the offsets (`relocated`), one inside it changes what the range
//!   holds (`changed`, `Covered.content`), one after it changes nothing the write rests on;
//! - for a range across paragraphs, the whole text of every paragraph but the last.
//!
//! **Paragraph by index** (`para.text`, `para.insert`): the paragraph's place in the body; its
//! characters for `para.text` (replaced whole) and the formatting of its last character (which the
//! new text takes), its paragraph properties for `para.insert` (the new paragraph
//! inherits them).
//!
//! **Styles by name**: a command given a `style` (or `basedOn`, `next`) rests on the root's
//! `styles` register, any change of which refuses it (styles are no objects).
use kition_doc_engine::doc::{Block, Document, Pos, StoryRef};
use kition_kit::{ContentAsk, ContentPremise, Covered, CoveredAsk, CoveredAt, CoveredContent, Covers, Premise, Under, Within};
use serde_json::{Value, json};

use crate::DocDocument;
use crate::context::paragraph;

/// The character an inline object holds in a paragraph's text.
const OBJECT: char = kition_doc_engine::doc::para::OBJ;

/// The object model's root, as change sets name it.
const ROOT: &str = "document";

/// The commands whose positions name text, with the parameters holding them.
fn positions(command: &str) -> &'static [&'static str] {
    match command {
        "select.range" | "select.column" => &["anchor", "focus"],
        "caret.set" | "review.suggestions" | "equation.edit" => &["pos"],
        _ => &[],
    }
}

/// A position as a command reads it: `{block, off}`, `{story, path, off}`, or (C8-land-14-doc)
/// `{para: <id>, off}`, the paragraph by its tool id.
pub(crate) fn parse(doc: &Document, value: &Value) -> Option<(Pos, bool)> {
    if let Some(id) = value.get("para").and_then(Value::as_str) {
        let at = paragraph(doc, id)?;
        let off = value.get("off").and_then(Value::as_u64).and_then(|off| usize::try_from(off).ok()).unwrap_or(0);
        return Some((Pos { off, ..at }, false));
    }
    kition_doc_engine::cmd::parse_pos(value).map(|pos| (pos, true))
}

/// Rewrites every `{para: <id>, off}` position of `params` into the `{story, path, off}` the
/// engine reads. A position naming no paragraph is left for the command to refuse.
pub(crate) fn resolve(doc: &Document, command: &str, params: &mut Value) {
    for name in positions(command) {
        let Some(value) = params.get_mut(*name) else { continue };
        if value.get("para").is_none() {
            continue;
        }
        if let Some((pos, _)) = parse(doc, value) {
            *value = kition_doc_engine::cmd::pos_json(&pos);
        }
    }
}

/// The guard asks `under` for every step of a run in order before the run, then `covered` for
/// the steps it reads as places (C8-doc-2 item 3). A step that acts on the selection rests on the
/// selection the call started with only when no earlier step of the run set it: `Pass` keeps
/// whether a step asked about set the selection, and what was answered for each step (so
/// `covered` agrees with `under`). Cleared when a command runs. A run the guard refused leaves it
/// set until the next command runs: a following run's steps on the selection are then guarded by
/// the state premise only, as before C8-doc-2.
#[derive(Default)]
pub(crate) struct Pass {
    selection_set: std::cell::Cell<bool>,
    holds: std::cell::RefCell<std::collections::HashMap<String, bool>>,
    /// The range the last step asked about that names positions selected (C8-doc-2 item 7: the
    /// paragraphs a later step's family premises name).
    last: std::cell::RefCell<Option<(Pos, Pos)>>,
    math: std::cell::RefCell<Option<kition_doc_engine::MathEdit>>,
    insert: std::cell::RefCell<Option<(Pos, Pos, String)>>,
}

impl Pass {
    pub(crate) fn executing(&self, command: &str, on: bool) {
        if sets_selection(command, on) || matches!(command, "para.text" | "para.insert") {
            self.selection_set.set(true);
        }
        self.actual_state();
    }

    pub(crate) fn actual_state(&self) {
        *self.last.borrow_mut() = None;
        *self.math.borrow_mut() = None;
        *self.insert.borrow_mut() = None;
    }

    pub(crate) fn reset(&self) {
        self.selection_set.set(false);
        self.holds.borrow_mut().clear();
        *self.last.borrow_mut() = None;
        *self.math.borrow_mut() = None;
        *self.insert.borrow_mut() = None;
    }

    pub(crate) fn last_range(&self) -> Option<(Pos, Pos)> {
        self.last.borrow().clone()
    }

    pub(crate) fn equation_edit(&self) -> Option<kition_doc_engine::MathEdit> {
        self.math.borrow().clone()
    }

    pub(crate) fn last_insert(&self) -> Option<(Pos, Pos, String)> {
        self.insert.borrow().clone()
    }
}

/// Whether a step of `command` sets the selection a later step acts on.
fn sets_selection(command: &str, on: bool) -> bool {
    on || !positions(command).is_empty()
        || !crate::acts::places(command).is_empty()
        || ["select.", "caret.", "nav."].iter().any(|prefix| command.starts_with(prefix))
        || matches!(command, "edit.selectAll" | "edit.goto" | "edit.find" | "edit.findNext" | "edit.findPrevious" | "comment.goto")
}

/// These handlers format or resolve paragraph properties through `paths_between`;
/// character offsets and marks at the implicit caret do not choose their result.
/// Keep text-dependent paragraph commands (sort, styles, list scans) on their own path.
fn paragraph_properties_only(command: &str, params: &Value) -> bool {
    matches!(
        command,
        "references.addText"
            | "para.align"
            | "para.alignCenter"
            | "para.alignLeft"
            | "para.alignRight"
            | "para.justify"
            | "para.distribute"
            | "para.spacing"
            | "para.addSpaceBefore"
            | "para.removeSpaceAfter"
            | "para.asianTypography"
            | "para.set"
            | "para.indents"
            | "para.hangingIndent"
            | "para.removeHanging"
            | "para.rtl"
            | "para.ltr"
            | "para.lineSpacing"
            | "para.single"
            | "para.double"
            | "para.oneAndHalf"
            | "para.keepNext"
            | "para.keepLines"
            | "para.pageBreakBefore"
            | "para.widowControl"
            | "para.outlineLevel"
            | "para.shading"
            | "para.borders"
            | "para.indent"
            | "para.outdent"
            | "para.tabs"
            | "para.bullets"
            | "para.numbering"
            | "para.multilevel"
            | "para.defineBullet"
            | "para.defineNumber"
            | "list.define"
    ) || (command == "styles.inspectorClear" && matches!(params.get("level").and_then(Value::as_str), Some("paragraphStyle" | "paragraphFormatting")))
}

/// Section properties use paragraph indexes and section breaks, without text
/// offsets. Forward columns insert a break at an offset, so retain their text hold.
fn section_properties_only(command: &str, params: &Value) -> bool {
    matches!(
        command,
        "layout.orientation"
            | "layout.margins"
            | "layout.size"
            | "layout.lineNumbers"
            | "layout.pageNumberFormat"
            | "layout.pageSetup"
            | "layout.verticalAlign"
            | "layout.differentFirstPage"
            | "layout.linkToPrevious"
            | "design.pageBorders"
    ) || (command == "layout.columns" && params.get("apply").and_then(Value::as_str) != Some("forward"))
}

/// What of a paragraph's text a call rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hold {
    /// Characters `[0, end)` and their marks (and the marks of the one before `start`):
    /// `start..end` is the range itself, an edit before it moves it.
    Range,
    /// The whole text and its marks (a paragraph a range runs through).
    Whole,
    /// Enter at the end (`para.insert after`, or before an empty paragraph): whether the text
    /// is empty (an empty list item leaves its list), what AutoCorrect does to its last word,
    /// and the marks of its last character (the new paragraph's text continues them); not the
    /// other characters, not where they are.
    Enter,
    /// The characters and the marks of the last one (`para.text` replaces the text whole).
    LastMarks,
    /// The set of formats its characters carry, not which characters (a character command
    /// with `on`: the whole text is selected when it runs; a toggle reads whether all of it has
    /// the format).
    Formats,
}

/// One paragraph a call addresses by position, and the characters of it the call rests on.
struct Held {
    id: String,
    start: usize,
    end: usize,
    hold: Hold,
}

fn char_index(text: &str, off: usize) -> usize {
    let mut at = off.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    text.get(..at).map_or(0, |prefix| prefix.chars().count())
}

fn hex_id(prefix: char, id: Option<u32>) -> Option<String> {
    id.map(|v| format!("{prefix}{v:08X}"))
}

/// The premises of a paragraph addressed at `pos`, by index (`by_index`) or by id: where it is,
/// and where the table and row are that a cell path counts in.
fn place_premises(doc: &Document, stored: Option<&StoredBody>, pos: &Pos, by_index: bool, id: &str, under: &mut Under) {
    if !by_index {
        under.premises.push(Premise::on(id).fields(Vec::<String>::new()).positions(Vec::<String>::new()));
        return;
    }
    let list = if pos.path.depth() == 0 && pos.story == StoryRef::Body { "body" } else { "blocks" };
    under.premises.push(Premise::on(id).fields(Vec::<String>::new()).positions([list]));
    // A body paragraph by index: its window in the body too (C8-doc-2 item 8: a paragraph
    // joined into the one before moves the next one to its index, which its own place does not
    // tell in a same-server change).
    if list == "body" {
        let i = pos.path.last();
        // The object model keeps a paragraph joined into the one before (Backspace at its start)
        // in the body list, merged into it in the view (C8-doc-3 item 6): the view's index then
        // counts fewer blocks than the stored list. The window is where the paragraph is stored,
        // and each paragraph before it shown merged is held by its text: one merged since basis
        // (its merge anchor) means the index names another paragraph now.
        let (at, merged) = stored.map_or((i, Vec::new()), |body| body.before(id, i));
        under.within.push(Within { parent: json!(ROOT), field: "body".into(), from: at, to: at + 1, time: None });
        for merged in merged {
            under.premises.push(Premise::on(merged.as_str()).fields(["text"]).positions(Vec::<String>::new()));
        }
    }
    // A header, footer, note or text box by its part number (C8-doc-2 item 4): the number is
    // the part's place in the root's `parts` (in number order), which another program's save may
    // renumber (a text box read before a footer). Its window there holds that the number still
    // names the same part.
    if let StoryRef::Part(n) = pos.story
        && let Some(k) = doc.parts.keys().position(|key| *key == n)
    {
        under.within.push(Within { parent: json!(ROOT), field: "parts".into(), from: k, to: k + 1, time: None });
    }
    // A cell path: the table's place in its list, the row's in the table, and the row's cells.
    let path = &pos.path.0;
    let mut blocks: Option<&kition_doc_engine::doc::Blocks> = match pos.story {
        StoryRef::Body => Some(&doc.body),
        StoryRef::Part(n) => doc.parts.get(&n).map(|part| &part.blocks),
    };
    let mut depth = 0usize;
    let mut i = 0usize;
    while i + 3 < path.len() {
        let Some(Block::Table(table)) = blocks.and_then(|b| b.get(path[i] as usize)).map(|b| &**b) else { break };
        let list = if depth == 0 && pos.story == StoryRef::Body { "body" } else { "blocks" };
        if let Some(tid) = hex_id('t', table.rows.first().and_then(|r| r.props.para_id)) {
            under.premises.push(Premise::on(tid.as_str()).fields(Vec::<String>::new()).positions([list]));
            // A body table by index: its window in the body, where the table is stored (not the
            // cell paragraph's index inside its cell), and the paragraphs before it shown merged.
            if list == "body" {
                let (at, merged) = stored.map_or((path[i] as usize, Vec::new()), |body| body.before(&tid, path[i] as usize));
                under.within.push(Within { parent: json!(ROOT), field: "body".into(), from: at, to: at + 1, time: None });
                for merged in merged {
                    under.premises.push(Premise::on(merged.as_str()).fields(["text"]).positions(Vec::<String>::new()));
                }
            }
        }
        let Some(row) = table.rows.get(path[i + 1] as usize) else { break };
        if let Some(rid) = hex_id('r', row.props.para_id) {
            under.premises.push(Premise::on(rid.as_str()).fields(["~tableOrder"]).positions(Vec::<String>::new()));
            // The row's place in its table, and its cells up to the addressed one (C8-doc-2: a
            // cell put before it, or one of them deleted or merged, moves the path; an edit in
            // another cell's text does not). Cells have no ids of their own: the row's id and the
            // cells' places name the cell. A cell's place is held through its properties (a cell
            // put in or taken out is told as a change of the whole cell at its index).
            let cells: Vec<String> = (0..=path[i + 2] as usize).map(|k| format!("cells.{k}.props")).collect();
            under.premises.push(Premise::on(rid.as_str()).fields(cells).positions(["rows"]));
        }
        blocks = row.cells.get(path[i + 2] as usize).map(|cell| &cell.blocks);
        depth += 1;
        i += 3;
    }
}

/// A layout hit names the paragraph in its containing lists just as an indexed position does.
pub(crate) fn hit_premises(doc: &Document, pos: &Pos, under: &mut Under) {
    if let Some(id) = hex_id('p', doc.para_at(pos).and_then(|p| p.props.para_id)) {
        place_premises(doc, None, pos, true, &id, under);
    }
}

/// The body as the object model stores it: each block's tool id in list order, and the ids the
/// view shows (a paragraph merged into the one before is stored, not shown).
pub(crate) struct StoredBody {
    ids: Vec<String>,
    shown: std::collections::HashSet<String>,
}

impl StoredBody {
    pub(crate) fn of(session: &kition_doc_engine::Session) -> Option<Self> {
        let replica = session.kdm().map(kition_doc_engine::kdm_model::DocSession::replica)?;
        let root = replica.objects_of_type("Document").into_iter().next()?;
        let ids = replica
            .list(&root, "body")
            .ok()?
            .iter()
            .map(|object| replica.get(object, "id").ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default())
            .collect();
        let shown = session
            .doc
            .body
            .iter()
            .filter_map(|block| match &**block {
                Block::Para(p) => hex_id('p', p.props.para_id),
                Block::Table(t) => hex_id('t', t.rows.first().and_then(|r| r.props.para_id)),
            })
            .collect();
        Some(Self { ids, shown })
    }

    /// Where paragraph `id` (shown at `index`) is stored, and the paragraphs stored before it that
    /// the view shows merged into another.
    fn before(&self, id: &str, index: usize) -> (usize, Vec<String>) {
        let Some(at) = self.ids.iter().position(|stored| stored == id) else { return (index, Vec::new()) };
        let merged = self.ids.iter().take(at).filter(|stored| !stored.is_empty() && !self.shown.contains(*stored)).cloned().collect();
        (at, merged)
    }
}

/// Whether `pos` names a paragraph exactly: every step of its path a table, a row and a cell
/// where it says, the last a paragraph, the offset inside its text. (The engine reads a path that
/// runs past a paragraph as that paragraph.)
fn exact(doc: &Document, pos: &Pos) -> bool {
    let mut blocks = match pos.story {
        StoryRef::Body => &doc.body,
        StoryRef::Part(n) => match doc.parts.get(&n) {
            Some(part) => &part.blocks,
            None => return false,
        },
    };
    let path = &pos.path.0;
    let mut i = 0usize;
    while i + 3 < path.len() {
        let Some(Block::Table(table)) = blocks.get(path[i] as usize).map(|b| &**b) else { return false };
        let Some(cell) = table.rows.get(path[i + 1] as usize).and_then(|row| row.cells.get(path[i + 2] as usize)) else { return false };
        blocks = &cell.blocks;
        i += 3;
    }
    if i + 1 != path.len() {
        return false;
    }
    matches!(blocks.get(path[i] as usize).map(|b| &**b), Some(Block::Para(p)) if pos.off <= p.text.len() && p.text.is_char_boundary(pos.off))
}

impl DocDocument {
    /// The paragraphs a positional command addresses, and what of each it rests on.
    /// `None` for a command that addresses no text by position; an empty list when a position
    /// names no paragraph exactly now (a path into a table that is no longer there).
    fn held(&self, command: &str, params: &Value, under: Option<&mut Under>) -> Option<Vec<Held>> {
        let asking = under.is_some();
        if !self.context_on.is_empty() && positions(command).is_empty() && matches!(on_family(command), Some(OnFamily::Characters)) {
            return Some(self.context_on.iter().map(|id| Held { id: id.clone(), start: 0, end: 0, hold: Hold::Formats }).collect());
        }
        let session = self.session.borrow();
        let doc = &session.doc;
        let index = |name: &str| params.get(name).and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok());
        if command == "para.text" {
            // A body paragraph by index whose characters the write replaces whole: its place and
            // its characters, not their formatting (the new text takes the paragraph's style).
            let i = index("index")?;
            let Some(Block::Para(p)) = doc.body.get(i).map(|b| &**b) else { return Some(Vec::new()) };
            let id = hex_id('p', p.props.para_id)?;
            if let Some(under) = under {
                under.objects.push(json!(id));
                under.premises.push(Premise::on(id.as_str()).fields(Vec::<String>::new()).positions(["body"]));
            }
            // The new text takes the formatting the paragraph's end continues with: the marks of
            // its last character are held, not the others'.
            let len = p.text.chars().count();
            return Some(vec![Held { id, start: len.saturating_sub(1), end: len, hold: Hold::LastMarks }]);
        }
        if command == "para.insert" {
            // Enter at the end of paragraph `after` (or the start of `before`), then typing
            // (C8-doc-2 item 2). Enter reads the paragraph's text (its last word's AutoCorrect,
            // whether it is empty: an empty list item leaves its list), its paragraph properties
            // (the new paragraph's), the formatting its end continues with, and its style's
            // `next`: the paragraph's place, the whole of its text after `after`, the start of it
            // before `before` (the whole when it is empty), `para`, and that style.
            let (i, after) = match (index("after"), index("before")) {
                (Some(i), _) => (i, true),
                (None, Some(i)) => (i, false),
                (None, None) => return None,
            };
            let Some(Block::Para(p)) = doc.body.get(i).map(|b| &**b) else {
                if let Some(under) = under {
                    under.within.push(Within { parent: json!(ROOT), field: "body".into(), from: i, to: i.saturating_add(1), time: None });
                    if let Some(Block::Table(t)) = doc.body.get(i).map(|b| &**b)
                        && let Some(id) = hex_id('t', t.rows.first().and_then(|r| r.props.para_id))
                    {
                        under.premises.push(Premise::on(id.as_str()).fields(["kind"]).positions(["body"]));
                    }
                }
                return Some(Vec::new());
            };
            let id = hex_id('p', p.props.para_id)?;
            if let Some(under) = under {
                under.objects.push(json!(id));
                under.premises.push(Premise::on(id.as_str()).fields(["para"]).positions(["body"]));
                under.premises.push(style_premise(p.props.style.as_deref().unwrap_or("Normal")));
            }
            let len = p.text.chars().count();
            let hold = if after || len == 0 { Hold::Enter } else { Hold::Range };
            return Some(vec![Held { id, start: 0, end: 0, hold }]);
        }
        let names = positions(command);
        // A join can leave the replica piece alive after its paragraph disappears
        // from the editor. Keep the requested identity for coverage to report gone.
        let missing: Vec<_> = names
            .iter()
            .filter_map(|name| params.get(*name)?.get("para")?.as_str())
            .filter(|id| paragraph(doc, id).is_none())
            .map(|id| Held { id: id.to_owned(), start: 0, end: 0, hold: Hold::Range })
            .collect();
        if !missing.is_empty() {
            if let Some(under) = under {
                under.objects.extend(missing.iter().map(|item| json!(item.id)));
            }
            return Some(missing);
        }
        let mut found: Vec<(Pos, bool)> = names.iter().filter_map(|name| params.get(*name)).filter_map(|value| parse(doc, value)).collect();
        let selection = names.is_empty() && self.reads_selection(&session, command, params, asking);
        if selection {
            // A step with no `on` and no position acts on the selection the call started with
            // (C8-doc-2 item 3): that range is held exactly as an explicit one by index.
            found = vec![(session.sel.anchor.clone(), true), (session.sel.focus.clone(), true)];
        }
        if found.is_empty() {
            return None;
        }
        found.sort();
        let stored = if asking { StoredBody::of(&session) } else { None };
        let mut under = under;
        if !found.iter().all(|(pos, _)| exact(doc, pos)) {
            // A position that names no paragraph exactly now (C8-doc-2 item 8): by index, the
            // body block it counts from is another now (its window tells `relocated`); by id,
            // the paragraph's text no longer reaches the offset (`changed`).
            if let Some(under) = under.as_deref_mut() {
                for (pos, by_index) in &found {
                    if *by_index {
                        if pos.story == StoryRef::Body
                            && let Some(&first) = pos.path.0.first()
                        {
                            let i = first as usize;
                            under.within.push(Within { parent: json!(ROOT), field: "body".into(), from: i, to: i + 1, time: None });
                        }
                    } else if let Some(id) = doc.para_at(&Pos { off: 0, ..pos.clone() }).and_then(|p| hex_id('p', p.props.para_id)) {
                        under.premises.push(Premise::on(id.as_str()).fields(["text"]).positions(Vec::<String>::new()));
                    }
                }
            }
            if !found.iter().all(|(pos, _)| doc.para_at(pos).is_some_and(|p| pos.off > p.len() || p.text.is_char_boundary(pos.off))) {
                return Some(Vec::new());
            }
        }
        let (first, last) = (found.first()?.0.clone(), found.last()?.0.clone());
        let mut held = Vec::new();
        // Every paragraph from the first position's to the last's, in one story.
        let paths: Vec<_> =
            if first.story == last.story && first.path != last.path { doc.paths_between(&first, &last) } else { vec![first.path.clone()] };
        for path in paths {
            let pos = Pos::new(first.story, path.clone(), 0);
            let Some(p) = doc.para_at(&pos) else { continue };
            let Some(id) = hex_id('p', p.props.para_id) else { continue };
            let in_first = path == first.path;
            let in_last = path == last.path;
            let by_index = found.iter().filter(|(at, _)| at.path == path).all(|(_, by_index)| *by_index);
            let (start, end, hold) = match (in_first, in_last) {
                (true, true) => {
                    (char_index(&p.text, first.off), if last.off > p.len() { usize::MAX } else { char_index(&p.text, last.off) }, Hold::Range)
                }
                (false, true) => (0, if last.off > p.len() { usize::MAX } else { char_index(&p.text, last.off) }, Hold::Range),
                _ => (0, p.text.chars().count(), Hold::Whole),
            };
            if let Some(under) = under.as_deref_mut() {
                under.objects.push(json!(id));
                // Paragraphs inside the range are held by place too, so one put between is seen.
                place_premises(doc, stored.as_ref(), &pos, by_index || !(in_first || in_last), &id, under);
                if hold == Hold::Whole {
                    under.premises.push(Premise::on(id.as_str()).fields(["text"]).positions(Vec::<String>::new()));
                }
            }
            held.push(Held { id, start, end, hold });
        }
        Some(held)
    }

    /// Whether `command` acts on the selection the call started with: a command that changes the
    /// document, reads the selection (`acts`), names no position, no drawing object by id and no
    /// paragraph by index, in a step without `on`.
    /// `asking`: `under` is asking (the answer is kept for the step), else `covered` (the answer
    /// `under` gave is read).
    fn reads_selection(&self, session: &kition_doc_engine::Session, command: &str, params: &Value, asking: bool) -> bool {
        if crate::family::table_definition_only(command, params) {
            return false;
        }
        if crate::family::new_unlinked_list(command, params) && params.get("apply").and_then(Value::as_bool) == Some(false) {
            return false;
        }
        if (command == "comment.delete" && params.get("thread").is_some_and(|v| !v.is_null()))
            || (matches!(command, "review.deleteComment" | "review.resolveComment") && params.get("id").is_some_and(|v| !v.is_null()))
            || (command == "review.deleteComment" && params.get("all").and_then(Value::as_bool) == Some(true))
        {
            // The comment id or all-comments branch chooses the target without
            // the body caret. Keep selection reads on the caret fallback.
            return false;
        }
        if matches!(command, "draw.stroke" | "draw.erase" | "drawing.ink" | "drawing.erase") {
            // These adjust selection offsets after editing at a page point; they consume no
            // text from the caret's paragraph. Their actual anchors are held by the family.
            return false;
        }
        if self.context_named || crate::acts::SELECTION_FREE.contains(&command) || !crate::acts::places(command).is_empty() {
            return false;
        }
        let mut spec = kition_kit::KitCommand { id: command.to_owned(), ..Default::default() };
        crate::acts::declare(&mut spec);
        if command != "drawing.connect"
            && spec.targets.into_iter().chain(spec.objects.iter().copied()).any(|name| params.get(name).is_some_and(|value| !value.is_null()))
        {
            return false;
        }
        if !session.registry.get(command).is_some_and(crate::changes_document) {
            return false;
        }
        let key = format!("{command} {params}");
        if asking {
            let holds = !self.pass.selection_set.get();
            *self.pass.holds.borrow_mut().entry(key).or_insert(false) |= holds;
            holds
        } else {
            self.pass.holds.borrow().get(&key).copied().unwrap_or(!self.pass.selection_set.get())
        }
    }

    pub(crate) fn under_call(&self, command: &str, params: &Value) -> Result<Option<Under>, String> {
        let mut answer = self.under_answer(command, params)?;
        // What the command's family rests on besides (`family`).
        let mut more = answer.take().unwrap_or_default();
        let declared = self.family_premises(command, params, &mut more);
        answer = if declared || more != Under::default() { Some(more) } else { None };
        if sets_selection(command, !self.context_on.is_empty()) || matches!(command, "para.text" | "para.insert") {
            self.pass.selection_set.set(true);
        }
        // A step naming positions: the range a later step of the run acts on.
        let names = positions(command);
        if !names.is_empty() {
            let session = self.session.borrow();
            let mut found: Vec<Pos> =
                names.iter().filter_map(|name| params.get(*name)).filter_map(|value| parse(&session.doc, value)).map(|(pos, _)| pos).collect();
            found.sort();
            if let (Some(first), Some(last)) = (found.first(), found.last()) {
                *self.pass.last.borrow_mut() = Some((first.clone(), last.clone()));
            }
        }
        if command == "equation.edit" {
            let session = self.session.borrow();
            let focus = self.pass.last_range().map_or_else(|| session.sel.focus.clone(), |(_, b)| b);
            *self.pass.math.borrow_mut() = kition_doc_engine::cmd::equation::edit_state_at(&session, &focus, params);
        } else if command == "equation.exit" || command == "select.range" || command == "caret.set" {
            *self.pass.math.borrow_mut() = None;
        }
        let session = self.session.borrow();
        if command == "text.insert" {
            let (a, b) = self.pass.last_range().unwrap_or_else(|| session.sel.ordered());
            *self.pass.insert.borrow_mut() = params
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| text.chars().count() == 1 && !text.contains(['\r', '\n', '\u{2029}']))
                .filter(|_| a.story == b.story && a.path == b.path)
                .map(|text| (a, b, text.to_owned()));
        } else if session.registry.get(command).is_some_and(crate::changes_document) {
            *self.pass.insert.borrow_mut() = None;
        }
        Ok(answer)
    }

    fn under_answer(&self, command: &str, params: &Value) -> Result<Option<Under>, String> {
        let mut under = Under::default();
        // A step with `on`: the paragraphs it names, by id, selected whole when the step runs
        // (C8-doc-2 item 6). What of them it rests on, by the command's family: a character
        // command the formats their text carries (which characters do not matter: the whole
        // text is selected when it runs); a paragraph command their paragraph properties. Their
        // place never matters. Other commands keep the SDK default (all of each).
        if !self.context_on.is_empty()
            && positions(command).is_empty()
            && let Some(family) = on_family(command)
        {
            match family {
                OnFamily::Characters => {
                    for id in &self.context_on {
                        under.objects.push(json!(id));
                        under.premises.push(Premise::on(id.as_str()).fields(Vec::<String>::new()).positions(Vec::<String>::new()));
                        under.content.push(ContentPremise { id: json!(id), field: "text".into(), region: json!("formats") });
                    }
                    under.covers = Some(Covers { fields: vec!["text".to_owned()] });
                }
                OnFamily::Paragraph => {
                    for id in &self.context_on {
                        under.premises.push(Premise::on(id.as_str()).fields(["para"]).positions(Vec::<String>::new()));
                    }
                }
            }
            self.style_premises(command, params, &mut under);
            return Ok(Some(under));
        }
        if crate::family::table_property_fields(command, params).is_some() {
            let session = self.session.borrow();
            if self.reads_selection(&session, command, params, true)
                && let Some((path, _, _)) = session.sel.focus.path.cell()
                && let Some(Block::Table(t)) = session.doc.block(session.sel.focus.story, &path)
                && let Some(id) = hex_id('t', t.rows.first().and_then(|r| r.props.para_id))
            {
                let at = Pos::new(session.sel.focus.story, path, 0);
                place_premises(&session.doc, StoredBody::of(&session).as_ref(), &at, true, &id, &mut under);
                self.style_premises(command, params, &mut under);
                return Ok(Some(under));
            }
            // A path whose table disappeared still takes the ordinary
            // invalid-position guard below, including its containing window.
        }
        if paragraph_properties_only(command, params) || section_properties_only(command, params) {
            let session = self.session.borrow();
            if self.reads_selection(&session, command, params, true) {
                // The call's selection locates paragraphs by index, independently of
                // their character offsets. Family premises hold their properties and
                // resolved styles; these premises hold identity and containing lists.
                let stored = StoredBody::of(&session);
                let (a, b) = session.sel.ordered();
                for path in session.doc.paths_between(&a, &b) {
                    let pos = Pos::new(a.story, path, 0);
                    if let Some(id) = hex_id('p', session.doc.para_at(&pos).and_then(|p| p.props.para_id)) {
                        place_premises(&session.doc, stored.as_ref(), &pos, true, &id, &mut under);
                    }
                }
                self.style_premises(command, params, &mut under);
                return Ok(Some(under));
            }
        }
        // A paragraph named by an id the document no longer has: what the write rested on is gone.
        let missing: Vec<String> = {
            let session = self.session.borrow();
            positions(command)
                .iter()
                .filter_map(|name| params.get(*name)?.get("para")?.as_str())
                .filter(|id| paragraph(&session.doc, id).is_none())
                .map(str::to_owned)
                .collect()
        };
        for id in missing {
            under.premises.push(Premise::on(id.as_str()).fields(Vec::<String>::new()).positions(Vec::<String>::new()));
        }
        if let Some(held) = self.held(command, params, Some(&mut under)) {
            // A position naming no paragraph now: `covered` cannot tell what it named at basis
            // (refused `unknown`); its window or its paragraph's text tells more (above).
            let _ = held;
            under.covers = Some(Covers { fields: vec!["text".to_owned()] });
            self.style_premises(command, params, &mut under);
            return Ok(Some(under));
        }
        if !under.premises.is_empty() {
            under.covers = Some(Covers { fields: vec!["text".to_owned()] });
            self.style_premises(command, params, &mut under);
            return Ok(Some(under));
        }
        if command == "drawing.erase" {
            // The strokes an eraser path takes are found by where they are on the page: the
            // ones it takes now are under the place, each held whole; `covered` tells the guard
            // a stroke another writer drew, moved or changed since was not there at basis.
            let hits = kition_doc_engine::cmd::ink::erase_hits(&mut self.session.borrow_mut(), params).unwrap_or_default();
            under.objects = hits.into_iter().map(|uid| json!(uid)).collect();
            // A stroke another program drew is told only as its paragraph's text in a file read
            // again (no `created` Inline): paragraphs' text is asked about too.
            under.covers = Some(Covers { fields: vec!["frame".to_owned(), "value".to_owned(), "text".to_owned()] });
            return Ok(Some(under));
        }
        if self.style_premises(command, params, &mut under) {
            return Ok(Some(under));
        }
        Ok(None)
    }

    pub(crate) fn content_call(&self, ask: &ContentAsk) -> Option<bool> {
        if ask.premise.field != "text" || ask.premise.region != json!("formats") {
            return None;
        }
        let (was_text, was_marks) = text_and_marks(&ask.before)?;
        if paragraph(&self.session.borrow().doc, ask.premise.id.as_str()?).is_none() {
            // Covered.gone reports the removed paragraph even if its merged replica piece survives.
            return Some(false);
        }
        let (now_text, now_marks) = text_now(&self.session.borrow(), ask.premise.id.as_str()?)?;
        if was_marks.is_empty() && !was_text.is_empty() {
            return None;
        }
        Some(formats_changed(&was_text, &was_marks, &now_text, &now_marks))
    }

    /// A command given a style by name (`style`, `basedOn`, `next`, a list level's `style`)
    /// rests on that style: the entry of the root's `styles` register whose `id` or `name` it is
    /// (C8-doc-2 item 6; a change of another style applies). Answers whether it named one.
    fn style_premises(&self, command: &str, params: &Value, under: &mut Under) -> bool {
        // Underline/border styles, picture presets and citation formats are not
        // names in the document's style sheet.
        let named = command.starts_with("styles.")
            || matches!(
                command,
                "format.charStyle" | "para.style" | "insert.table" | "table.style" | "table.newStyle" | "table.modifyStyle" | "table.deleteStyle"
            );
        let mut names: Vec<&str> = if named {
            ["style", "basedOn", "next"].iter().filter_map(|name| params.get(*name).and_then(Value::as_str)).collect()
        } else {
            Vec::new()
        };
        if command == "list.define" {
            names.extend(params.get("levels").and_then(Value::as_array).into_iter().flatten().filter_map(|level| level.get("style")?.as_str()));
        }
        for name in &names {
            under.premises.push(style_premise(name));
        }
        !names.is_empty()
    }

    /// `context.covered` for a positional command: the paragraph was under the place at `basis`
    /// when its text before the range is as it was (an edit before it moved the offsets), and its
    /// content there is the same when the range's characters are too. The basis text is the full
    /// value the guard keeps privately (runs: characters and marks); a formatting change counts
    /// where its characters are. A text the guard could not rebuild cannot be told.
    pub(crate) fn covered_call(&self, command: &str, params: &Value, asks: &[CoveredAsk]) -> Result<Option<CoveredAt>, String> {
        if command == "drawing.erase" {
            // Where a stroke was at basis is not rebuilt here: one others created was not under
            // the path; any other change of a stroke, or of the text of a paragraph that holds
            // (or held) a drawing object, cannot be told (refused `unknown`). A paragraph without
            // one holds no stroke.
            let under: Vec<Value> = kition_doc_engine::cmd::ink::erase_hits(&mut self.session.borrow_mut(), params)
                .unwrap_or_default()
                .into_iter()
                .map(|uid| json!(uid))
                .collect();
            let session = self.session.borrow();
            // A paragraph whose text and marks are as they were (a reread mints its objects'
            // marks anew), or that holds no drawing object then or now, moved no stroke.
            let unmoved = |ask: &CoveredAsk| {
                let was = ask.before.get("text").and_then(text_and_marks);
                let now = ask.id.as_str().and_then(|id| text_now(&session, id));
                match (was, now) {
                    (Some(was), Some(now)) => was == now || (!was.0.contains(OBJECT) && !now.0.contains(OBJECT)),
                    _ => false,
                }
            };
            let covered = asks
                .iter()
                .map(|ask| {
                    let paragraph = ask.id.as_str().is_some_and(|id| id.starts_with('p'));
                    let covered = if ask.created || (paragraph && unmoved(ask)) { Some(false) } else { None };
                    Covered { id: ask.id.clone(), covered, content: None, gone: false }
                })
                .collect();
            return Ok(Some(CoveredAt { under, covered }));
        }
        let Some(held) = self.held(command, params, None) else { return Ok(None) };
        if held.is_empty() {
            // A preceding join can move an indexed table path out of the current
            // view without moving anything in the stored list. Its complete prior
            // text lets us rebuild that view on an isolated replica.
            return Ok(self.covered_before_join(command, params, asks));
        }
        let session = self.session.borrow();
        let now_of = |id: &str| text_now(&session, id);
        let mut under = Vec::new();
        let mut covered = Vec::new();
        for item in &held {
            let ask = asks.iter().find(|ask| ask.id.as_str() == Some(item.id.as_str()));
            let Some(ask) = ask else {
                under.push(json!(item.id));
                continue;
            };
            if ask.created {
                covered.push(Covered { id: ask.id.clone(), covered: Some(false), content: None, gone: false });
                continue;
            }
            if paragraph(&session.doc, &item.id).is_none() {
                under.push(json!(item.id));
                covered.push(Covered { id: ask.id.clone(), covered: Some(true), content: Some(CoveredContent::Deleted), gone: true });
                continue;
            }
            if ask.unknown.iter().any(|field| field == "text") {
                covered.push(Covered { id: ask.id.clone(), covered: None, content: None, gone: false });
                continue;
            }
            let Some(before) = ask.before.get("text") else {
                under.push(json!(item.id));
                covered.push(Covered { id: ask.id.clone(), covered: Some(true), content: Some(CoveredContent::Same), gone: false });
                continue;
            };
            let Some((now_text, now_marks)) = now_of(&item.id) else {
                covered.push(Covered { id: ask.id.clone(), covered: None, content: None, gone: false });
                continue;
            };
            let Some((was_text, was_marks)) = text_and_marks(before) else {
                covered.push(Covered { id: ask.id.clone(), covered: None, content: None, gone: false });
                continue;
            };
            if item.hold == Hold::Formats {
                under.push(json!(item.id));
                // A basis without formats (a plain string) cannot be told.
                let content = if was_marks.is_empty() && !was_text.is_empty() {
                    None
                } else if formats_changed(&was_text, &was_marks, &now_text, &now_marks) {
                    Some(CoveredContent::Changed)
                } else {
                    Some(CoveredContent::Same)
                };
                covered.push(Covered { id: ask.id.clone(), covered: Some(true), content, gone: false });
                continue;
            }
            if item.hold == Hold::Enter {
                // Enter at the end: what it reads of the text, not where the text is.
                under.push(json!(item.id));
                let autocorrect = |text: &str| enter_effect(session.autocorrect_on, &session.autocorrect_user, text);
                let changed = was_text.is_empty() != now_text.is_empty()
                    || (!was_marks.is_empty() && was_marks.last() != now_marks.last())
                    || (was_text != now_text && (autocorrect(&was_text) || autocorrect(&now_text)));
                covered.push(Covered {
                    gone: false,
                    id: ask.id.clone(),
                    covered: Some(true),
                    content: Some(if changed { CoveredContent::Changed } else { CoveredContent::Same }),
                });
                continue;
            }
            let now: Vec<char> = now_text.chars().collect();
            let was: Vec<char> = was_text.chars().collect();
            // Where the texts first differ, at the earliest any alignment allows.
            let prefix = was.iter().zip(&now).take_while(|(a, b)| a == b).count();
            let suffix = was.iter().rev().zip(now.iter().rev()).take_while(|(a, b)| a == b).count();
            let earliest = if was == now { was.len() } else { prefix.min(was.len().min(now.len()).saturating_sub(suffix)) };
            let (start, end) = if item.hold == Hold::Range { (item.start, item.end) } else { (0, usize::MAX) };
            // A paragraph replaced whole (`para.text`) holds the marks of its last character only.
            let marks_last = item.hold == Hold::LastMarks && !was_marks.is_empty() && was_marks.last() != now_marks.last();
            let text_changed = was != now;
            if text_changed && earliest < start {
                // Before the range: the offsets name other characters now.
                covered.push(Covered { id: ask.id.clone(), covered: Some(false), content: None, gone: false });
                continue;
            }
            under.push(json!(item.id));
            // Inside the range (a caret's range is empty: an edit at it is after it).
            let inside = text_changed && earliest < end;
            // Formatting of the held characters (and of the one before a caret or range, whose
            // formatting typing takes), where the texts still agree. A basis given as a plain
            // string carries no marks to compare.
            let agree = earliest.min(was.len()).min(now.len());
            let marks_changed = item.hold != Hold::LastMarks
                && !was_marks.is_empty()
                && (start.saturating_sub(1)..end.max(usize::from(start == 0 && end == 0)).min(agree)).any(|i| was_marks.get(i) != now_marks.get(i));
            let changed = inside || marks_changed || marks_last;
            covered.push(Covered {
                gone: false,
                id: ask.id.clone(),
                covered: Some(true),
                content: Some(if changed { CoveredContent::Changed } else { CoveredContent::Same }),
            });
        }
        // Anything else asked about (another paragraph, a deleted one) is not at this place.
        for ask in asks {
            if !covered.iter().any(|c| c.id == ask.id) {
                let mine = held.iter().any(|item| ask.id.as_str() == Some(item.id.as_str()));
                covered.push(Covered { id: ask.id.clone(), covered: Some(mine && !ask.deleted), content: None, gone: false });
            }
        }
        Ok(Some(CoveredAt { under, covered }))
    }

    fn covered_before_join(&self, command: &str, params: &Value, asks: &[CoveredAsk]) -> Option<CoveredAt> {
        use kition_kit::kdm::{ActorId, Origin, Replica};
        use kition_kit::session::KitModel;

        let session = self.session.borrow();
        let kdm = session.kdm()?;
        let replica = kdm.replica();
        let blocks = replica.objects_of_type("Block");
        let mut restores = Vec::new();
        let mut joined = false;
        for ask in asks {
            // This proof concerns text-only joins, with unchanged stored order.
            // Missing history, lifecycle changes and object-bearing prior text
            // need a different proof; they remain unavailable.
            if ask.created || ask.deleted || !ask.unknown.is_empty() || ask.before.get("$at").is_some() {
                return None;
            }
            let id = ask.id.as_str()?;
            let before = ask.before.get("text")?;
            if before.as_array().is_some_and(|runs| runs.iter().any(|run| run.get("attributes").and_then(|a| a.get("object")).is_some())) {
                return None;
            }
            let (text, _) = text_and_marks(before)?;
            if text.contains(OBJECT) {
                return None;
            }
            let mut matching = blocks.iter().filter(|object| replica.get(object, "id").ok().as_ref().and_then(Value::as_str) == Some(id));
            let object = matching.next()?.clone();
            if matching.next().is_some() {
                return None;
            }
            joined |= paragraph(&session.doc, id).is_none() && replica.text(&object, "text").ok()?.contains(OBJECT);
            restores.push((object, text));
        }
        if !joined {
            return None;
        }
        let mut before = Replica::load(kdm_model::DocModel::schema(), &replica.snapshot().ok()?, ActorId::random()).ok()?;
        before
            .transact(&Origin::system("premise-join-view"), |tx| {
                for (object, text) in &restores {
                    let len = tx.text_len(object, "text")?;
                    tx.text_delete(object, "text", 0, len)?;
                    tx.text_insert(object, "text", 0, text)?;
                }
                Ok(())
            })
            .ok()?;
        // Only occupancy is used below, so the restored plain text needs no
        // character formatting. All existing objects and media stay in the fork.
        let (doc, _) = kdm_model::read_document(&before, kdm.blobs()).ok()?;
        let mut found: Vec<_> = positions(command).iter().map(|name| parse(&doc, params.get(*name)?)).collect::<Option<_>>()?;
        if found.is_empty() || !found.iter().all(|(pos, indexed)| *indexed && pos.story == StoryRef::Body && pos.path.depth() > 0 && exact(&doc, pos))
        {
            return None;
        }
        found.sort();
        let (first, last) = (&found.first()?.0, &found.last()?.0);
        let under: Vec<_> = doc
            .paths_between(first, last)
            .iter()
            .filter_map(|path| doc.para_at(&Pos::new(first.story, path.clone(), 0)).and_then(|p| hex_id('p', p.props.para_id)))
            .map(|id| json!(id))
            .collect();
        if under.is_empty() {
            return None;
        }
        let covered =
            asks.iter().map(|ask| Covered { id: ask.id.clone(), covered: Some(under.contains(&ask.id)), content: None, gone: false }).collect();
        Some(CoveredAt { under, covered })
    }
}

fn formats_changed(was_text: &str, was: &[Value], now_text: &str, now: &[Value]) -> bool {
    if was_text == now_text {
        return was != now;
    }
    // Text edits can preserve a uniform format despite changing the character count.
    // Mixed runs need an alignment proof before their distribution can be ignored.
    if let (Some(was_first), Some(now_first)) = (was.first(), now.first())
        && was.iter().all(|mark| mark == was_first)
        && now.iter().all(|mark| mark == now_first)
    {
        return was_first != now_first;
    }
    // Rejecting one inserted run removes its revision mark with its characters.
    // Prove that every surviving character kept its full marks and that the
    // removed run's formatting is still represented; accepting a revision keeps
    // the characters and therefore takes the exact comparison above.
    let before: Vec<_> = was_text.chars().collect();
    let after: Vec<_> = now_text.chars().collect();
    if before.len() > after.len() && was.len() == before.len() && now.len() == after.len() {
        let prefix = before.iter().zip(&after).take_while(|(a, b)| a == b).count();
        let suffix = before.iter().rev().zip(after.iter().rev()).take(after.len().saturating_sub(prefix)).take_while(|(a, b)| a == b).count();
        let end = before.len().saturating_sub(suffix);
        if prefix + suffix == after.len()
            && was.get(..prefix) == now.get(..prefix)
            && was.get(end..) == now.get(prefix..)
            && let Some(removed) = was.get(prefix..end)
            && let Some(first) = removed.first()
            && removed.iter().all(|mark| mark == first)
            && let Some(attributes) = first.as_object()
            && attributes.contains_key("ins")
        {
            let mut formatting = attributes.clone();
            formatting.remove("ins");
            let formatting = if formatting.is_empty() { Value::Null } else { Value::Object(formatting) };
            if now.contains(&formatting) {
                return false;
            }
        }
    }
    was != now
}

/// Whether Enter at the end of a paragraph of `text` changes it (AutoCorrect of its last word:
/// a replacement, an ordinal's superscript, a link), with AutoCorrect `on` and the person's own
/// entries `user`. Run on a scratch copy; the document is not touched.
fn enter_effect(on: bool, user: &[(String, String)], text: &str) -> bool {
    use kition_doc_engine::doc::props::CharProps;
    let mut doc = Document::new();
    let start = doc.start_of(StoryRef::Body);
    if doc.insert_text(&start, text, &CharProps::default()).is_err() {
        return true;
    }
    let before = doc.para_at(&start).cloned();
    let mut scratch = kition_doc_engine::Session::new(doc);
    scratch.autocorrect_on = on;
    scratch.autocorrect_user = user.to_vec();
    scratch.sel = kition_doc_engine::Selection::caret(Pos { off: text.len(), ..start.clone() });
    if kition_doc_engine::cmd::tools::autocorrect_word(&mut scratch, true).is_err() {
        return true;
    }
    scratch.doc.para_at(&start).cloned() != before
}

/// The premise on one style, named by its id or its name: its entry in the root's `styles`
/// register (`styles.styles`, a list of styles each with an `id` and a `name`).
pub(crate) fn style_premise(name: &str) -> Premise {
    let quoted = serde_json::to_string(name).unwrap_or_default();
    Premise::on(ROOT).fields([format!("styles.styles[id={quoted}]"), format!("styles.styles[name={quoted}]")]).positions(Vec::<String>::new())
}

/// What a step with `on` rests on of the paragraphs it names, by its command.
#[derive(Clone, Copy)]
enum OnFamily {
    /// The formats of their characters (character formatting, font, case of the selection).
    Characters,
    /// Their paragraph properties (alignment, indents, spacing, style, borders, shading).
    Paragraph,
}

fn on_family(command: &str) -> Option<OnFamily> {
    let (family, _) = command.split_once('.')?;
    match family {
        "format" | "font" => Some(OnFamily::Characters),
        "para" if !matches!(command, "para.text" | "para.insert" | "para.dialog") => Some(OnFamily::Paragraph),
        _ => None,
    }
}

/// Paragraph `id`'s text and marks now, as the object model holds them (the basis value is in
/// that form): its `Block`'s text field.
fn text_now(session: &kition_doc_engine::Session, id: &str) -> Option<(String, Vec<Value>)> {
    let replica = session.kdm().map(kition_doc_engine::kdm_model::DocSession::replica)?;
    let object =
        replica.objects_of_type("Block").into_iter().find(|object| replica.get(object, "id").ok().as_ref().and_then(Value::as_str) == Some(id))?;
    let runs = replica.text_delta(&object, "text").ok()?;
    text_and_marks(&serde_json::to_value(runs).ok()?)
}

/// The text and marks per character of a text value: a string, or runs `[{insert, attributes}]`.
/// The mark naming an inline object's object id is left out: a reread of the file mints new
/// object ids, and the object itself is named by its own tool id.
fn text_and_marks(value: &Value) -> Option<(String, Vec<Value>)> {
    match value {
        Value::String(text) => Some((text.clone(), vec![])),
        Value::Array(runs) => {
            let mut text = String::new();
            let mut marks = Vec::new();
            for run in runs {
                let insert = run.get("insert")?.as_str()?;
                let mut attributes = run.get("attributes").cloned().unwrap_or(Value::Null);
                let structural = attributes.get("object").and_then(|object| object.get("splitTo")).and_then(Value::as_str).is_some();
                if let Some(map) = attributes.as_object_mut() {
                    map.remove("object");
                }
                // Replica split/merge anchors are not characters in the paragraph view.
                for character in insert.chars().filter(|character| !structural || *character != OBJECT) {
                    text.push(character);
                    marks.push(attributes.clone());
                }
            }
            Some((text, marks))
        }
        _ => None,
    }
}
