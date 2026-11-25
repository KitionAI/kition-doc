//! Stories: blocks, paragraphs, runs, fields, drawings, tables.

use std::sync::Arc;

/// The `Document::parts` id of a note reference whose note the package does not have: no part is
/// ever given it (`Document::add_part` counts up from 1).
const MISSING_NOTE: u32 = u32::MAX;

use kition_doc_doc::effects::{Glow, Shadow, ShapeEffects};
use kition_doc_doc::graphic::{Embedded, Graphic, GraphicItem, GraphicKind};
use kition_doc_doc::para::{Anchor, CodeField, Float, FloatAlign, NoteKind, RelSize, ShapeKind, Wrap, WrapPolygon};
use kition_doc_doc::props::{CharProps, ControlMarks, NumChange, PropChange, Rgb, TextColor};
use kition_doc_doc::table::{Cell, MAX_COLS, MAX_ROWS, Row, Table};
use kition_doc_doc::{Block, Blocks, InlineObject, Paragraph, PartKind, RevisionKind, Run, para_block};

use super::Reader;
use super::props::{sectpr, tcpr, trpr};
use crate::package::{Rels, rt};
use crate::units::{int, measure};
use crate::xml::El;

/// Deepest table nesting we build (deeper tables become plain paragraphs).
const MAX_TABLE_DEPTH: usize = 24;
/// Deepest field nesting we track.
const MAX_FIELD_DEPTH: usize = 32;
/// Deepest text-box-in-text-box nesting.
const MAX_STORY_DEPTH: usize = 4;
/// Longest field code we keep (Zotero's citation codes embed item data as JSON).
const MAX_INSTR: usize = 2 * 1024 * 1024;
/// Longest custom note mark read from the run after its reference (Word allows ten characters).
const MAX_CUSTOM_MARK: usize = 16;

/// Inherited run context (from `w:hyperlink`, `w:ins`, `w:del` and `w:sdt` wrappers).
#[derive(Clone, Default)]
pub struct RunCtx {
    link: Option<String>,
    ins: Option<u32>,
    del: Option<u32>,
    control: Option<ControlMarks>,
}

#[derive(PartialEq)]
enum Phase {
    Instr,
    Result,
}

enum Item {
    Text(String),
    Obj(Box<InlineObject>),
}

struct FieldState {
    instr: String,
    phase: Phase,
    spilled: bool,
    buf: Vec<(Item, CharProps)>,
    props: CharProps,
    locked: bool,
    /// Kept as a range field: its result flows into the document as ordinary content between
    /// `FieldStart` and `FieldEnd` markers instead of being buffered.
    range: bool,
    /// The formatting of each piece of `instr`, as read.
    code_runs: Vec<Run>,
    /// Fields nested in the code, over `instr` as read (untrimmed).
    code_fields: Vec<CodeField>,
}

impl FieldState {
    /// Appends a piece of the field code in its run's formatting.
    fn push_code(&mut self, text: &str, props: &CharProps) {
        if text.is_empty() {
            return;
        }
        self.instr.push_str(text);
        match self.code_runs.last_mut() {
            Some(r) if r.props == *props => r.len += text.len(),
            _ => self.code_runs.push(Run { len: text.len(), props: props.clone() }),
        }
    }

    /// A field nested in the code: its cached result stands in the code where it was.
    fn push_nested(&mut self, instr: String, result: &str, props: &CharProps) {
        let start = self.instr.len();
        self.push_code(result, props);
        self.code_fields.push(CodeField { start, len: self.instr.len() - start, instr });
    }

    /// The trimmed field code, its runs (none when the whole code has the field's formatting)
    /// and the fields nested in it.
    fn code(&self) -> (String, Vec<Run>, Vec<CodeField>) {
        let code = self.instr.trim();
        let from = self.instr.len() - self.instr.trim_start().len();
        let to = from + code.len();
        let code_fields = self
            .code_fields
            .iter()
            .filter(|f| f.start >= from && f.start + f.len <= to)
            .map(|f| CodeField { start: f.start - from, len: f.len, instr: f.instr.clone() })
            .collect();
        let mut runs = Vec::new();
        let mut at = 0usize;
        for r in &self.code_runs {
            let (a, b) = (at.max(from), (at + r.len).min(to));
            at += r.len;
            if a < b {
                runs.push(Run { len: b - a, props: r.props.clone() });
            }
        }
        if runs.iter().all(|r| r.props == self.props) {
            runs.clear();
        }
        (code.to_string(), runs, code_fields)
    }
}

/// Fields kept as ranges: those whose result is formatted text in its own right, often several
/// paragraphs (citation managers' `ADDIN` fields: Zotero, Mendeley, EndNote), and hyperlinks,
/// whose result keeps its runs' formatting (Word's Hyperlink style) and wraps like text.
fn is_range_instr(instr: &str) -> bool {
    let code = instr.trim_start();
    let keyword =
        |k: &str| code.get(..k.len()).is_some_and(|c| c.eq_ignore_ascii_case(k)) && code[k.len()..].chars().next().is_none_or(char::is_whitespace);
    keyword("ADDIN") || keyword("HYPERLINK")
}

/// The field that buffers content now: the innermost one that isn't a range field, unless it
/// has spilled (then content goes straight into the paragraph).
fn sink(sc: &mut StoryCtx) -> Option<&mut FieldState> {
    match sc.fields.iter_mut().rev().find(|f| !f.range) {
        Some(f) if !f.spilled => Some(f),
        _ => None,
    }
}

/// Per-story parsing state (fields may span paragraphs; block-level markers wait for the next
/// paragraph).
#[derive(Default)]
pub struct StoryCtx {
    fields: Vec<FieldState>,
    ignored_begins: usize,
    pending: Vec<InlineObject>,
    /// Text-box nesting depth.
    pub story_depth: usize,
    /// A note reference with a custom mark whose mark is not in its own run: the next run's text
    /// is the mark (as some writers save it).
    custom_note: Option<(NoteKind, u32, CharProps)>,
}

/// Paragraph content under construction.
#[derive(Default)]
struct PB {
    text: String,
    runs: Vec<Run>,
    objects: Vec<InlineObject>,
    /// Whether another piece of the current source run has already been emitted.
    in_run: bool,
}

impl PB {
    fn push_text(&mut self, s: &str, props: &CharProps) {
        if s.is_empty() {
            return;
        }
        self.text.push_str(s);
        self.push_run(s.len(), props);
    }
    fn push_obj(&mut self, o: InlineObject, props: &CharProps) {
        self.text.push(kition_doc_doc::para::OBJ);
        self.objects.push(o);
        self.push_run(kition_doc_doc::para::OBJ.len_utf8(), props);
    }
    /// Give the last object, when it is `o` and ends the text, formatting `props`.
    fn restyle_last(&mut self, o: &InlineObject, props: &CharProps) {
        let len = kition_doc_doc::para::OBJ.len_utf8();
        if self.objects.last() != Some(o) || !self.text.ends_with(kition_doc_doc::para::OBJ) {
            return;
        }
        let Some(last) = self.runs.last_mut().filter(|r| r.len >= len) else { return };
        if last.props == *props {
            return;
        }
        last.len -= len;
        if last.len == 0 {
            self.runs.pop();
        }
        self.push_run(len, props);
    }
    fn push_run(&mut self, len: usize, props: &CharProps) {
        match self.runs.last_mut() {
            Some(r) if r.props.can_merge(props) || (self.in_run && r.props == *props) => r.len += len,
            _ => self.runs.push(Run { len, props: props.clone() }),
        }
        self.in_run = true;
    }
}

/// Clean text from `w:t`: drop characters with special meaning in the model and control
/// characters XML producers shouldn't have put there.
fn clean_text(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\n' | '\r' => Some(' '),
            '\t' => Some('\t'),
            kition_doc_doc::para::OBJ => None,
            c if (c as u32) < 0x20 => None,
            c => Some(c),
        })
        .collect()
}

fn children_of_choice(ac: &El) -> Option<&El> {
    // Prefer a Choice whose requirements we understand; else the Fallback; else any Choice.
    const KNOWN: &[&str] = &["wps", "wpg", "w14", "w15", "wp14", "a14", "w16se", "w16cid", "w16", "w16cex", "w16sdtdh", "v"];
    let ok = |c: &El| c.attr("Requires").unwrap_or("").split_whitespace().all(|r| KNOWN.contains(&r));
    ac.children("mc:Choice").find(|c| ok(c)).or_else(|| ac.child("mc:Fallback")).or_else(|| ac.child("mc:Choice"))
}

impl Reader<'_> {
    /// Read a story's blocks from a container element (`w:body`, `w:hdr`, `w:tc`, `w:txbxContent`…).
    pub fn read_blocks(&mut self, sc: &mut StoryCtx, parent: &El, rels: &Rels, out: &mut Blocks, depth: usize) {
        if depth > crate::xml::MAX_DEPTH {
            return;
        }
        for e in parent.els() {
            match e.name.as_str() {
                "w:p" => {
                    let p = self.read_para(sc, e, rels, depth);
                    // A framed drop-cap paragraph merges into the paragraph it starts.
                    if let Some(Block::Para(prev)) = out.last().map(|b| &**b)
                        && prev.props.drop_cap.is_some()
                        && prev.section.is_none()
                        && prev.text.len() <= 16
                        && p.props.drop_cap.is_none()
                        && let Some(Block::Para(mut first)) = out.pop().map(|b| (*b).clone())
                    {
                        let (lines, space) = (first.props.drop_cap, first.props.drop_cap_space);
                        let mut props = p.props.clone();
                        first.append(p);
                        props.drop_cap = lines;
                        props.drop_cap_space = space;
                        first.props = props;
                        out.push(para_block(first));
                        continue;
                    }
                    out.push(para_block(p));
                }
                "w:tbl" => {
                    if depth / 4 >= MAX_TABLE_DEPTH {
                        out.push(para_block(Paragraph::with_text(&clean_text(&e.deep_text()), CharProps::default())));
                    } else if let Some(t) = self.read_table(sc, e, rels, depth) {
                        out.push(Arc::new(Block::Table(t)));
                    }
                }
                "w:sdt" => {
                    if let Some(c) = e.child("w:sdtContent") {
                        let before = out.len();
                        self.read_blocks(sc, c, rels, out, depth + 1);
                        // The control's start mark goes with its first paragraph, its end mark with its last.
                        let marks = control_marks(e);
                        if let Some(Block::Para(p)) = out.get_mut(before).map(Arc::make_mut) {
                            let start = ControlMarks { start: marks.start, end: None };
                            p.props.control = Some(p.props.control.map_or(start, |m| m.with(start)));
                        }
                        if out.len() > before
                            && let Some(Block::Para(p)) = out.last_mut().map(Arc::make_mut)
                        {
                            let end = ControlMarks { start: None, end: marks.end };
                            p.props.control = Some(p.props.control.map_or(end, |m| m.with(end)));
                        }
                    }
                }
                "w:customXml" | "w:smartTag" | "w:ins" | "w:moveTo" => self.read_blocks(sc, e, rels, out, depth + 1),
                "mc:AlternateContent" => {
                    if let Some(c) = children_of_choice(e) {
                        self.read_blocks(sc, c, rels, out, depth + 1);
                    }
                }
                "w:bookmarkStart" | "w:bookmarkEnd" | "w:commentRangeStart" | "w:commentRangeEnd" => {
                    if let Some(o) = self.marker(e) {
                        sc.pending.push(o);
                    }
                }
                _ => {}
            }
        }
    }

    /// Markers left over at the end of a story go to its last paragraph.
    pub fn flush_pending(&mut self, sc: &mut StoryCtx, out: &mut Blocks) {
        if sc.pending.is_empty() {
            return;
        }
        if let Some(Block::Para(p)) = out.last_mut().map(Arc::make_mut) {
            for o in sc.pending.drain(..) {
                let props = p.runs.last().map(|r| r.props.clone()).unwrap_or_default();
                p.text.push(kition_doc_doc::para::OBJ);
                p.objects.push(o);
                p.runs.push(Run { len: kition_doc_doc::para::OBJ.len_utf8(), props });
            }
            p.normalize();
        }
        sc.pending.clear();
    }

    fn marker(&mut self, e: &El) -> Option<InlineObject> {
        let id = e.attr("w:id").unwrap_or("").to_string();
        match e.name.as_str() {
            "w:bookmarkStart" => {
                let name = e.attr("w:name").unwrap_or("").to_string();
                if name.is_empty() || name.len() > 256 {
                    return None;
                }
                self.bookmarks.insert(id, name.clone());
                Some(InlineObject::BookmarkStart { name })
            }
            "w:bookmarkEnd" => self.bookmarks.get(&id).map(|n| InlineObject::BookmarkEnd { name: n.clone() }),
            "w:commentRangeStart" => self.comment_id(&id).map(|id| InlineObject::CommentStart { id }),
            "w:commentRangeEnd" => {
                let cid = self.comment_id(&id)?;
                self.comments_ended.insert(cid);
                Some(InlineObject::CommentEnd { id: cid })
            }
            _ => None,
        }
    }

    fn comment_id(&self, file_id: &str) -> Option<u32> {
        self.comment_map.get(file_id).copied()
    }

    fn read_para(&mut self, sc: &mut StoryCtx, e: &El, rels: &Rels, depth: usize) -> Paragraph {
        let mut pb = PB::default();
        let mut para = Paragraph::new();
        for o in std::mem::take(&mut sc.pending) {
            pb.push_obj(o, &CharProps::default());
        }
        if let Some(ppr) = e.child("w:pPr") {
            let (props, mark) = self.pc.ppr(ppr);
            para.props = props;
            para.mark = mark;
            // A tracked paragraph mark (ECMA-376 §17.13.5.16 / §17.13.5.15): `w:ins` / `w:del`
            // in the mark's `w:rPr`.
            if let Some(rpr) = ppr.child("w:rPr") {
                for k in rpr.els() {
                    match k.name.as_str() {
                        "w:ins" | "w:moveTo" => para.mark.ins = Some(self.revision(RevisionKind::Insert, k)),
                        "w:del" | "w:moveFrom" => para.mark.del = Some(self.revision(RevisionKind::Delete, k)),
                        _ => {}
                    }
                }
                para.mark.fmt_change = self.rpr_change(rpr);
            }
            // Tracked formatting changes (§17.13.5.29, §17.13.5.19): the paragraph properties
            // before the change, and a change of the list numbering.
            if let Some(ch) = ppr.child("w:pPrChange") {
                let old = ch.child("w:pPr").map(|o| self.pc.ppr(o).0.formatting()).unwrap_or_default();
                para.props.fmt_change = PropChange::boxed(self.revision(RevisionKind::Format, ch), old);
            }
            if para.props.numbering.is_some()
                && let Some(ch) = ppr.child("w:numPr").and_then(|n| n.child("w:numberingChange"))
            {
                let original = ch.attr("w:original").unwrap_or("").chars().take(256).collect();
                para.props.num_change = Some(Box::new(NumChange { rev: self.revision(RevisionKind::Format, ch), original }));
            }
            if let Some(s) = ppr.child("w:sectPr") {
                para.section = Some(Box::new(self.read_section(s, rels)));
            }
        }
        para.props.para_id = e.attr("w14:paraId").and_then(super::para_id);
        let ctx = RunCtx::default();
        self.read_inline_children(sc, &mut pb, e, rels, &ctx, depth + 1);
        self.flush_custom_note(sc, &mut pb);
        self.spill_fields(sc, &mut pb);
        paragraph.text = pb.text;
        para.runs = pb.runs;
        para.objects = pb.objects;
        para.normalize();
        para
    }

    /// A tracked run formatting change in `rpr` (`w:rPrChange`, §17.13.5.31): the formatting
    /// before it. A change nested inside it is ignored.
    fn rpr_change(&mut self, rpr: &El) -> Option<Box<PropChange<CharProps>>> {
        let ch = rpr.child("w:rPrChange")?;
        let old = ch.child("w:rPr").map(|o| self.pc.rpr(o).formatting()).unwrap_or_default();
        PropChange::boxed(self.revision(RevisionKind::Format, ch), old)
    }

    pub fn read_section(&mut self, s: &El, rels: &Rels) -> kition_doc_doc::SectionProps {
        let (mut sp, refs) = sectpr(s);
        // §17.13.5.32: the section's properties before a tracked change (no header references).
        if let Some(ch) = s.child("w:sectPrChange") {
            let old = ch.child("w:sectPr").map(|o| sectpr(o).0).unwrap_or_default();
            sp.fmt_change = PropChange::boxed(self.revision(RevisionKind::Format, ch), old);
        }
        for r in refs {
            let Some(rel) = rels.by_id(&r.rid) else { continue };
            if rel.external {
                continue;
            }
            let target = rel.target.clone();
            let Some(id) = self.load_header_footer(&target, r.footer) else { continue };
            let set = if r.footer { &mut sp.footers } else { &mut sp.headers };
            match r.kind.as_str() {
                "first" => set.first = Some(id),
                "even" => set.even = Some(id),
                _ => set.default = Some(id),
            }
        }
        sp
    }

    fn read_inline_children(&mut self, sc: &mut StoryCtx, pb: &mut PB, e: &El, rels: &Rels, ctx: &RunCtx, depth: usize) {
        if depth > crate::xml::MAX_DEPTH {
            return;
        }
        for k in e.els() {
            self.read_inline(sc, pb, k, rels, ctx, depth);
        }
    }

    fn read_inline(&mut self, sc: &mut StoryCtx, pb: &mut PB, k: &El, rels: &Rels, ctx: &RunCtx, depth: usize) {
        if k.name != "w:r" {
            self.flush_custom_note(sc, pb);
        }
        match k.name.as_str() {
            "w:r" => self.read_run(sc, pb, k, rels, ctx, depth),
            "w:hyperlink" => {
                let mut c = ctx.clone();
                let url = k.attr("r:id").and_then(|id| rels.by_id(id)).map(|r| r.target.clone());
                let anchor = k.attr("w:anchor").filter(|a| !a.is_empty());
                c.link = match (url, anchor) {
                    (Some(u), Some(a)) => Some(format!("{u}#{a}")),
                    (Some(u), None) => Some(u),
                    (None, Some(a)) => Some(format!("#{a}")),
                    (None, None) => ctx.link.clone(),
                };
                self.read_inline_children(sc, pb, k, rels, &c, depth + 1);
            }
            "w:ins" | "w:moveTo" => {
                let mut c = ctx.clone();
                c.ins = Some(self.revision(RevisionKind::Insert, k));
                self.read_inline_children(sc, pb, k, rels, &c, depth + 1);
            }
            "w:del" | "w:moveFrom" => {
                let mut c = ctx.clone();
                c.del = Some(self.revision(RevisionKind::Delete, k));
                self.read_inline_children(sc, pb, k, rels, &c, depth + 1);
            }
            "w:fldSimple" => {
                let instr = k.attr("w:instr").unwrap_or("").to_string();
                let props = k.child("w:r").and_then(|r| r.child("w:rPr")).map(|p| self.run_props(p, ctx)).unwrap_or_else(|| self.run_props_none(ctx));
                self.begin_field(sc, props, instr, on_off_attr(k, "w:fldLock"));
                self.separate_field(sc, pb);
                self.read_inline_children(sc, pb, k, rels, ctx, depth + 1);
                self.end_field(sc, pb);
            }
            "w:bookmarkStart" | "w:bookmarkEnd" | "w:commentRangeStart" | "w:commentRangeEnd" => {
                if let Some(o) = self.marker(k) {
                    let props = self.run_props_none(ctx);
                    // Markers go straight into the paragraph (never into a field's cached result).
                    pb.push_obj(o, &props);
                }
            }
            "w:sdt" => {
                if let Some(c) = k.child("w:sdtContent") {
                    let mut c2 = ctx.clone();
                    c2.control = Some(control_marks(k));
                    self.read_inline_children(sc, pb, c, rels, &c2, depth + 1);
                }
            }
            "w:smartTag" | "w:customXml" | "w:dir" | "w:bdo" => self.read_inline_children(sc, pb, k, rels, ctx, depth + 1),
            "m:oMath" => {
                let props = self.run_props_none(ctx);
                let math = super::math::read_omath(k, Default::default());
                let linear = kition_office_core::math::to_linear(&math.nodes);
                self.emit_obj(sc, pb, InlineObject::Equation { linear, display: false, math, uid: 0 }, &props);
            }
            "m:oMathPara" => {
                let props = self.run_props_none(ctx);
                let jc = super::math::para_jc(k);
                for m in k.children("m:oMath") {
                    let math = super::math::read_omath(m, jc);
                    let linear = kition_office_core::math::to_linear(&math.nodes);
                    self.emit_obj(sc, pb, InlineObject::Equation { linear, display: true, math, uid: 0 }, &props);
                }
            }
            "mc:AlternateContent" => {
                if let Some(c) = children_of_choice(k) {
                    self.read_inline_children(sc, pb, c, rels, ctx, depth + 1);
                }
            }
            // Some generators write a break directly under the paragraph instead of inside a run.
            "w:br" | "w:cr" => {
                let props = self.run_props_none(ctx);
                self.read_run_child(sc, pb, k, &props, rels, &mut None, depth + 1);
            }
            _ => {}
        }
    }

    fn run_props(&mut self, rpr: &El, ctx: &RunCtx) -> CharProps {
        let mut p = self.pc.rpr(rpr);
        p.link = ctx.link.clone();
        p.ins = ctx.ins;
        p.del = ctx.del;
        p.control = ctx.control;
        p.fmt_change = self.rpr_change(rpr);
        p
    }
    fn run_props_none(&self, ctx: &RunCtx) -> CharProps {
        CharProps { link: ctx.link.clone(), ins: ctx.ins, del: ctx.del, control: ctx.control, ..Default::default() }
    }

    fn read_run(&mut self, sc: &mut StoryCtx, pb: &mut PB, r: &El, rels: &Rels, ctx: &RunCtx, depth: usize) {
        // The custom mark of the reference just before: this run's text, when it holds only text.
        if let Some((kind, id, props)) = sc.custom_note.take() {
            let mut mark = String::new();
            let only_text = r.els().all(|k| match k.name.as_str() {
                "w:rPr" => true,
                "w:t" => {
                    mark.push_str(&clean_text(&k.text()));
                    true
                }
                _ => false,
            });
            if only_text && !mark.is_empty() && mark.chars().count() <= MAX_CUSTOM_MARK {
                self.emit_obj(sc, pb, InlineObject::NoteRef { kind, id, custom: mark }, &props);
                return;
            }
            self.emit_obj(sc, pb, InlineObject::NoteRef { kind, id, custom: String::new() }, &props);
        }
        let props = match r.child("w:rPr") {
            Some(p) => self.run_props(p, ctx),
            None => self.run_props_none(ctx),
        };
        pb.in_run = false;
        let mut note: Option<(NoteKind, u32, Option<String>)> = None;
        for k in r.els() {
            self.read_run_child(sc, pb, k, &props, rels, &mut note, depth + 1);
        }
        match note.take() {
            // Its mark follows in the next run.
            Some((kind, id, Some(custom))) if custom.is_empty() => sc.custom_note = Some((kind, id, props)),
            Some((kind, id, custom)) => self.emit_obj(sc, pb, InlineObject::NoteRef { kind, id, custom: custom.unwrap_or_default() }, &props),
            None => {}
        }
    }

    /// A custom-marked note reference still waiting for its mark: without one, it is numbered.
    fn flush_custom_note(&mut self, sc: &mut StoryCtx, pb: &mut PB) {
        if let Some((kind, id, props)) = sc.custom_note.take() {
            self.emit_obj(sc, pb, InlineObject::NoteRef { kind, id, custom: String::new() }, &props);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn read_run_child(
        &mut self,
        sc: &mut StoryCtx,
        pb: &mut PB,
        k: &El,
        props: &CharProps,
        rels: &Rels,
        note: &mut Option<(NoteKind, u32, Option<String>)>,
        depth: usize,
    ) {
        if depth > crate::xml::MAX_DEPTH {
            return;
        }
        // A custom footnote mark: the text right after the reference is the mark.
        if let Some((_, _, Some(custom))) = note.as_mut()
            && k.name == "w:t"
        {
            custom.push_str(&clean_text(&k.text()));
            return;
        }
        if k.name != "w:rPr"
            && let Some((kind, id, custom)) = note.take()
        {
            self.emit_obj(sc, pb, InlineObject::NoteRef { kind, id, custom: custom.unwrap_or_default() }, props);
        }
        match k.name.as_str() {
            "w:t" | "w:delText" => {
                let t = clean_text(&k.text());
                self.emit_text(sc, pb, &t, props);
            }
            "w:instrText" | "w:delInstrText" => {
                if let Some(f) = sc.fields.last_mut()
                    && f.phase == Phase::Instr
                    && !f.spilled
                    && f.instr.len() < MAX_INSTR
                {
                    f.push_code(&k.text(), props);
                }
            }
            "w:tab" | "w:ptab" => self.emit_text(sc, pb, "\t", props),
            "w:br" => {
                let c = match k.attr("w:type") {
                    Some("page") => "\u{000C}",
                    Some("column") => "\u{000E}",
                    _ => "\n",
                };
                self.emit_text(sc, pb, c, props);
            }
            "w:cr" => self.emit_text(sc, pb, "\n", props),
            "w:noBreakHyphen" => self.emit_text(sc, pb, "\u{2011}", props),
            "w:softHyphen" => self.emit_text(sc, pb, "\u{00AD}", props),
            "w:sym" => {
                let ch = k.attr("w:char").and_then(|h| u32::from_str_radix(h.trim(), 16).ok()).and_then(char::from_u32).filter(|c| *c as u32 >= 0x20);
                if let Some(ch) = ch {
                    let mut p = props.clone();
                    if let Some(f) = k.attr("w:font").filter(|f| !f.is_empty()) {
                        p.font = Some(f.to_string());
                    }
                    self.emit_text(sc, pb, &ch.to_string(), &p);
                }
            }
            "w:fldChar" => match k.attr("w:fldCharType") {
                Some("begin") => self.begin_field(sc, props.clone(), String::new(), on_off_attr(k, "w:fldLock")),
                Some("separate") => self.separate_field(sc, pb),
                Some("end") => self.end_field(sc, pb),
                _ => {}
            },
            "w:drawing" => {
                if let Some(o) = self.read_drawing(sc, k, rels) {
                    self.emit_obj(sc, pb, o, props);
                }
            }
            "w:pict" => {
                if let Some(o) = self.read_pict(sc, k, rels) {
                    self.emit_obj(sc, pb, o, props);
                }
            }
            "w:object" => {
                if let Some(o) = self.read_object(sc, k, rels) {
                    self.emit_obj(sc, pb, o, props);
                }
            }
            "w:footnoteReference" | "w:endnoteReference" => {
                let foot = k.name == "w:footnoteReference";
                let fid = k.attr("w:id").and_then(int).unwrap_or(i64::MIN);
                let map = if foot { &self.footnotes } else { &self.endnotes };
                // A reference whose note the package does not have keeps its mark, as Word shows it,
                // pointing at a part id no part has (`MISSING_NOTE`), so the check reports it.
                let id = map.get(&fid).copied().unwrap_or(MISSING_NOTE);
                let kind = if foot { NoteKind::Footnote } else { NoteKind::Endnote };
                let custom = on_off_attr(k, "w:customMarkFollows").then(String::new);
                *note = Some((kind, id, custom));
            }
            // The note's own number at the start of its text: a reference to the note being read.
            "w:footnoteRef" | "w:endnoteRef" => {
                let kind = if k.name == "w:footnoteRef" { NoteKind::Footnote } else { NoteKind::Endnote };
                if let Some((nk, id)) = self.current_note
                    && nk == kind
                {
                    *note = Some((kind, id, None));
                }
            }
            "w:commentReference" => {
                if let Some(cid) = k.attr("w:id").and_then(|i| self.comment_id(i)) {
                    if self.comments_ended.insert(cid) {
                        pb.push_obj(InlineObject::CommentEnd { id: cid }, props);
                    } else {
                        // The range ended just before: the end takes the reference run's formatting.
                        pb.restyle_last(&InlineObject::CommentEnd { id: cid }, props);
                    }
                }
            }
            "w:pgNum" => self.emit_obj(
                sc,
                pb,
                InlineObject::Field { instr: "PAGE".into(), result: String::new(), locked: false, code_runs: Vec::new(), code_fields: Vec::new() },
                props,
            ),
            "w:ruby" => {
                if let Some(base) = k.child("w:rubyBase") {
                    for r in base.children("w:r") {
                        for c in r.els() {
                            self.read_run_child(sc, pb, c, props, rels, note, depth + 1);
                        }
                    }
                }
            }
            "mc:AlternateContent" => {
                if let Some(o) = self.read_ink(sc, k, rels) {
                    self.emit_obj(sc, pb, o, props);
                    return;
                }
                if let Some(c) = children_of_choice(k) {
                    for x in c.els() {
                        self.read_run_child(sc, pb, x, props, rels, note, depth + 1);
                    }
                }
            }
            _ => {}
        }
    }

    /// Ink (a `wordprocessingInk` drawing holding a `w14:contentPart`), which Kition Doc does not
    /// draw: shown as the picture of its VML fallback, and the whole alternate content kept (with
    /// the ink part and the picture) so saving writes the ink back.
    fn read_ink(&mut self, sc: &mut StoryCtx, ac: &El, rels: &Rels) -> Option<InlineObject> {
        let ink = |c: &El| {
            c.child("w:drawing").and_then(|d| d.find("a:graphicData")).and_then(|g| g.attr("uri")).is_some_and(|u| u.ends_with("/wordprocessingInk"))
        };
        if !ac.children("mc:Choice").any(ink) {
            return None;
        }
        let pict = ac.child("mc:Fallback")?.child("w:pict")?;
        let mut o = self.read_vml(sc, pict, rels)?;
        let InlineObject::Image { ole, .. } = &mut o else { return None };
        *ole = self.keep_embedded_showing(ac, rels, &[], vml_image_id(pict));
        Some(o)
    }

    // ---- fields ----

    fn begin_field(&mut self, sc: &mut StoryCtx, props: CharProps, instr: String, locked: bool) {
        if sc.fields.len() >= MAX_FIELD_DEPTH || sc.ignored_begins > 0 {
            sc.ignored_begins += 1;
            return;
        }
        // Fields nested deeper than the cap are ignored (their content flows to the enclosing field).
        let mut f = FieldState {
            instr: String::new(),
            phase: Phase::Instr,
            spilled: false,
            buf: Vec::new(),
            props,
            locked,
            range: false,
            code_runs: Vec::new(),
            code_fields: Vec::new(),
        };
        let props = f.props.clone();
        f.push_code(&instr, &props);
        sc.fields.push(f);
    }

    /// Whether the field below the innermost one is still collecting its code (a field nested
    /// in another's code contributes text to that code, never content).
    fn parent_in_instr(sc: &StoryCtx) -> bool {
        let n = sc.fields.len();
        n >= 2 && sc.fields.get(n - 2).is_some_and(|p| !p.spilled && !p.range && p.phase == Phase::Instr)
    }

    /// `separate`: the code is complete; a range field starts its result here.
    fn separate_field(&mut self, sc: &mut StoryCtx, pb: &mut PB) {
        if sc.ignored_begins > 0 {
            return;
        }
        let nested_in_instr = Self::parent_in_instr(sc);
        let Some(f) = sc.fields.last_mut() else { return };
        if f.range {
            return;
        }
        f.phase = Phase::Result;
        if !f.spilled && !nested_in_instr && is_range_instr(&f.instr) {
            f.range = true;
            let (obj, props) = (InlineObject::FieldStart { instr: f.instr.trim().to_string(), locked: f.locked }, f.props.clone());
            self.emit_obj(sc, pb, obj, &props);
        }
    }

    fn end_field(&mut self, sc: &mut StoryCtx, pb: &mut PB) {
        if sc.ignored_begins > 0 {
            sc.ignored_begins -= 1;
            return;
        }
        let nested_in_instr = Self::parent_in_instr(sc);
        let Some(f) = sc.fields.pop() else { return };
        if f.range {
            self.emit_obj(sc, pb, InlineObject::FieldEnd, &f.props);
            return;
        }
        if f.spilled {
            return;
        }
        if f.phase == Phase::Instr && !nested_in_instr && is_range_instr(&f.instr) {
            // A range field with no result yet.
            self.emit_obj(sc, pb, InlineObject::FieldStart { instr: f.instr.trim().to_string(), locked: f.locked }, &f.props);
            self.emit_obj(sc, pb, InlineObject::FieldEnd, &f.props);
            return;
        }
        let mut result = String::new();
        for (it, _) in &f.buf {
            match it {
                Item::Text(t) => result.push_str(t),
                Item::Obj(o) => result.push_str(o.plain_text()),
            }
        }
        let (instr, code_runs, code_fields) = f.code();
        if let Some(parent) = sc.fields.last_mut()
            && !parent.spilled
            && parent.phase == Phase::Instr
        {
            parent.push_nested(instr, &result, &f.props);
            return;
        }
        let obj = InlineObject::Field { instr, result, locked: f.locked, code_runs, code_fields };
        self.emit_obj(sc, pb, obj, &f.props);
    }

    /// At a paragraph end, open fields can't keep buffering: emit them (empty result) and
    /// let the rest of their result flow as ordinary text.
    fn spill_fields(&mut self, sc: &mut StoryCtx, pb: &mut PB) {
        for f in sc.fields.iter_mut() {
            if f.spilled || f.range {
                continue;
            }
            f.spilled = true;
            let (instr, code_runs, code_fields) = f.code();
            pb.push_obj(InlineObject::Field { instr, result: String::new(), locked: f.locked, code_runs, code_fields }, &f.props);
            for (it, p) in f.buf.drain(..) {
                match it {
                    Item::Text(t) => pb.push_text(&t, &p),
                    Item::Obj(o) => pb.push_obj(*o, &p),
                }
            }
        }
    }

    fn emit_text(&mut self, sc: &mut StoryCtx, pb: &mut PB, s: &str, props: &CharProps) {
        match sink(sc) {
            Some(f) => {
                if f.phase == Phase::Result && f.buf.len() < 100_000 {
                    f.buf.push((Item::Text(s.to_string()), props.clone()));
                }
            }
            _ => pb.push_text(s, props),
        }
    }

    fn emit_obj(&mut self, sc: &mut StoryCtx, pb: &mut PB, o: InlineObject, props: &CharProps) {
        match sink(sc) {
            Some(f) => {
                if f.phase == Phase::Result && f.buf.len() < 100_000 {
                    f.buf.push((Item::Obj(Box::new(o)), props.clone()));
                }
            }
            _ => pb.push_obj(o, props),
        }
    }

    // ---- drawings ----

    fn read_drawing(&mut self, sc: &mut StoryCtx, d: &El, rels: &Rels) -> Option<InlineObject> {
        let (c, anchored) = match d.child("wp:inline") {
            Some(c) => (c, false),
            None => (d.child("wp:anchor")?, true),
        };
        let ext = c.child("wp:extent");
        let dim = |n: &str| ext.and_then(|e| e.attr(n)).and_then(|v| measure(v, 12_700.0)).unwrap_or(0.0).clamp(0.0, crate::units::MAX_LEN_PT);
        let (w, h) = (dim("cx"), dim("cy"));
        let alt = c.child("wp:docPr").and_then(|p| p.attr("descr").filter(|s| !s.is_empty()).or_else(|| p.attr("title"))).unwrap_or("").to_string();
        let mut float = if anchored { anchor_float(c) } else { Float::default() };
        if let Some(e) = c.child("wp:effectExtent") {
            for (slot, n) in float.effect.iter_mut().zip(["l", "t", "r", "b"]) {
                // Negative for a rotated object narrower than its frame; settled below.
                *slot = e.attr(n).and_then(|v| measure(v, 12_700.0)).unwrap_or(0.0).clamp(-1584.0, 1584.0);
            }
        }
        let gd = c.child("a:graphic").and_then(|g| g.child("a:graphicData"))?;
        // Checked by URI before the blip search: a diagram's data can hold a blip further down.
        let uri = gd.attr("uri").unwrap_or("");
        let graphic_kind = if uri.ends_with("/chart") {
            Some(GraphicKind::Chart)
        } else if uri.ends_with("/diagram") {
            Some(GraphicKind::Diagram)
        } else {
            None
        };
        if let Some(kind) = graphic_kind {
            // Kept so saving writes the chart or diagram back: the frame around it is written
            // from the object's (possibly moved or resized) size and position.
            let extra: Vec<String> = match kind {
                GraphicKind::Chart => Vec::new(),
                GraphicKind::Diagram => self.diagram_drawing_rel(gd, rels).into_iter().collect(),
            };
            let source = c.child("a:graphic").and_then(|g| self.keep_embedded(g, rels, &extra));
            // A chart's drawing id is its id in the document (DR2), as for shapes.
            let uid = if kind == GraphicKind::Chart {
                c.child("wp:docPr").and_then(|p| p.attr("id")).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0)
            } else {
                0
            };
            let graphic = self.graphic(kind, gd, rels, w, h, source, uid);
            float.effect = float.effect.map(|e| e.max(0.0));
            return Some(InlineObject::Graphic { w, h, alt, float, graphic });
        }
        let mut obj = if let Some(g) = gd.child("wpg:wgp") {
            self.read_group(sc, g, rels, w, h, float)?
        } else if find_outside_text(gd, "a:blip", 0).is_some() {
            self.read_pic(gd, rels, w, h, alt, float)?
        } else {
            let wsp = gd.find("wps:wsp")?;
            let mut obj = self.read_wsp(sc, wsp, rels, w, h, float);
            // Only the outer drawing carries the name identifying Kition Doc ink.
            if let InlineObject::Shape { freeform: Some(f), .. } = &mut obj {
                Arc::make_mut(f).ink = c.child("wp:docPr").and_then(|p| p.attr("name")).and_then(super::freeform::ink_tool);
            }
            obj
        };
        // The drawing's id is the shape's id in the document (DR2), when it is a number.
        // A picture's too (C8-doc-6: its tool id).
        if let InlineObject::Shape { uid, .. } | InlineObject::Group { uid, .. } | InlineObject::Image { uid, .. } = &mut obj {
            *uid = c.child("wp:docPr").and_then(|p| p.attr("id")).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0);
        }
        // Word's effect extent also covers a rotated object's overhang; the model keeps that
        // apart (it follows from the angle), so take it back out.
        if let Some(f) = obj.float_mut() {
            let (px, py) = f.spin_pad(w, h);
            for (e, p) in f.effect.iter_mut().zip([px, py, px, py]) {
                *e = (*e - p).clamp(0.0, 1584.0);
            }
        }
        Some(obj)
    }

    /// The chart or smart graphic in an `a:graphicData`, as items in points from its top-left
    /// corner. Charts are drawn from their cached data, smart graphics from the drawing Word
    /// stores beside them. Built once per part and size; once the file's graphic budget is spent,
    /// later ones are empty.
    /// `uid`: the object's id in the document (a chart's drawing id), part of what is shared.
    #[allow(clippy::too_many_arguments)]
    fn graphic(&mut self, kind: GraphicKind, gd: &El, rels: &Rels, w: f32, h: f32, source: Option<Arc<Embedded>>, uid: u32) -> Arc<Graphic> {
        let part = match kind {
            GraphicKind::Chart => gd.child("c:chart").and_then(|c| c.attr("r:id")).and_then(|id| super::part_of(rels, id, rt::CHART)),
            GraphicKind::Diagram => self.diagram_part(gd, rels),
        };
        let Some(path) = part else { return Arc::new(Graphic { kind, items: Vec::new(), w, h, source, uid, ..Default::default() }) };
        let key = (kind, path, w.to_bits(), h.to_bits(), uid);
        if let Some(g) = self.graphics.get(&key) {
            return g.clone();
        }
        let data = match kind {
            GraphicKind::Chart => None,
            GraphicKind::Diagram => self.diagram_data(gd, rels),
        };
        let items = if self.graphic_budget == 0 { Vec::new() } else { self.graphic_items(kind, &key.1, data.as_deref(), w, h) };
        self.graphic_budget = self.graphic_budget.saturating_sub(graphic_work(&items));
        let g = Arc::new(Graphic { kind, items, w, h, source, uid, ..Default::default() });
        self.graphics.insert(key, g.clone());
        g
    }

    /// The items of the chart part or diagram drawing part at `path` (`data`: a diagram's data
    /// model part).
    fn graphic_items(&mut self, kind: GraphicKind, path: &str, data: Option<&str>, w: f32, h: f32) -> Vec<GraphicItem> {
        match kind {
            GraphicKind::Chart => match self.graphic_part(path) {
                Some(space) => super::chart::chart_items(&space, &self.doc.settings.theme_colors, w, h),
                None => Vec::new(),
            },
            GraphicKind::Diagram => self.diagram_drawing(path, data, w, h),
        }
    }

    /// A picture: the first `a:blip` in `pic` and its crop.
    fn read_pic(&mut self, pic: &El, rels: &Rels, w: f32, h: f32, alt: String, mut float: Float) -> Option<InlineObject> {
        let blip = pic.find("a:blip")?;
        float.set_spin(xfrm_spin(pic.find("pic:spPr")));
        let embed = blip.attr("r:embed")?;
        // A picture whose image part is missing from the package keeps its frame, as Word shows it,
        // under a media name the document does not hold, so the check reports the missing image.
        let media = match self.media_for(rels, embed) {
            Some(media) => media,
            None => {
                let rel = rels.by_id(embed).filter(|rel| !rel.external)?;
                rel.target.rsplit('/').next().filter(|name| !name.is_empty()).unwrap_or("image").to_owned()
            }
        };
        let mut crop = [0.0f32; 4];
        if let Some(sr) = pic.find("a:srcRect") {
            for (i, n) in ["l", "t", "r", "b"].iter().enumerate() {
                if let Some(v) = sr.attr(n).and_then(int)
                    && let Some(slot) = crop.get_mut(i)
                {
                    *slot = (v.clamp(0, 100_000) as f32 / 100_000.0).clamp(0.0, 1.0);
                }
            }
        }
        // The preset outline it is cut to, unless a plain rectangle.
        let geom = pic.find("pic:spPr").and_then(|s| s.child("a:prstGeom")).and_then(|g| {
            let name = g.attr("prst").filter(|n| !n.is_empty() && n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric()))?;
            let adj = preset_adjust(g);
            (name != "rect" || !adj.is_empty()).then(|| kition_doc_doc::graphic::PresetGeom { name: name.to_string(), adj })
        });
        // Its line (none without a fill; 0.75 pt when the width isn't given) and effects.
        let sppr = pic.find("pic:spPr");
        let ln = sppr.and_then(|s| s.child("a:ln"));
        let edge = kition_doc_doc::graphic::PictureEdge {
            line: ln.and_then(|l| super::drawing_color::fill_colour(l, &self.doc.settings.theme_colors)),
            line_width: ln.and_then(|l| l.attr("w")).and_then(|v| measure(v, 12_700.0)).unwrap_or(0.75).clamp(0.0, 100.0),
            effects: sppr.and_then(|s| s.child("a:effectLst")).map(effect_list).unwrap_or_default(),
        };
        let edge = if edge.line.is_none() { kition_doc_doc::graphic::PictureEdge { line_width: 0.0, ..edge } } else { edge };
        Some(InlineObject::Image { edge, geom, media, w, h, alt, float, crop, ole: None, adjust: image_adjust(blip), uid: 0 })
    }

    /// A `wpg:wgp` group (`w` × `h`): its pictures, shapes and text boxes, nested groups
    /// flattened into it. `None` when it has no member we can show.
    fn read_group(&mut self, sc: &mut StoryCtx, g: &El, rels: &Rels, w: f32, h: f32, mut float: Float) -> Option<InlineObject> {
        let x = group_xfrm(g.child("wpg:grpSpPr"));
        float.set_spin(xfrm_spin(g.child("wpg:grpSpPr")));
        // The members' space; without `a:chExt` (or in a direction it has no size, as a group of
        // level lines) it is the group's own size.
        let (cw, ch) = x.ch_ext.unwrap_or((0.0, 0.0));
        let ch_w = if cw > 0.0 { cw } else { w.max(1.0) };
        let ch_h = if ch > 0.0 { ch } else { h.max(1.0) };
        let (cx, cy) = x.ch_off.unwrap_or((0.0, 0.0));
        let mut children = Vec::new();
        self.group_members(sc, g, rels, (-cx, -cy, 1.0, 1.0), 0, &mut children);
        if children.is_empty() {
            return None;
        }
        Some(InlineObject::Group { w, h, float, ch_w, ch_h, children, uid: 0 })
    }

    /// The members of group `g` into `out`, mapped into the outermost group's space by `t`
    /// (x0, y0, sx, sy: outer = x0 + inner × sx).
    fn group_members(
        &mut self,
        sc: &mut StoryCtx,
        g: &El,
        rels: &Rels,
        t: (f32, f32, f32, f32),
        depth: usize,
        out: &mut Vec<kition_doc_doc::para::GroupChild>,
    ) {
        let (x0, y0, sx, sy) = t;
        for e in g.els() {
            if out.len() >= kition_doc_doc::para::MAX_GROUP_CHILDREN {
                return;
            }
            let e = match e.name.as_str() {
                "mc:AlternateContent" => match children_of_choice(e).and_then(|c| c.els().next()) {
                    Some(c) => c,
                    None => continue,
                },
                _ => e,
            };
            let x = match e.name.as_str() {
                "wps:wsp" => group_xfrm(e.child("wps:spPr")),
                "pic:pic" => group_xfrm(e.child("pic:spPr")),
                "wpg:grpSp" => group_xfrm(e.child("wpg:grpSpPr")),
                // A chart or smart graphic in the group: its frame has its own `wpg:xfrm`.
                "wpg:graphicFrame" => xfrm_of(e.child("wpg:xfrm")),
                _ => continue,
            };
            let ((ox, oy), (ew, eh)) = (x.off, x.ext);
            if e.name == "wpg:grpSp" {
                if depth >= 8 {
                    continue;
                }
                // Its members' space maps onto its own box.
                let (cox, coy) = x.ch_off.unwrap_or((0.0, 0.0));
                let (cw, ch) = x.ch_ext.filter(|(cw, ch)| *cw > 0.0 && *ch > 0.0).unwrap_or((ew.max(1.0), eh.max(1.0)));
                let (kx, ky) = (ew / cw, eh / ch);
                let nt = (x0 + (ox - cox * kx) * sx, y0 + (oy - coy * ky) * sy, sx * kx, sy * ky);
                self.group_members(sc, e, rels, nt, depth + 1, out);
                continue;
            }
            let max = crate::units::MAX_LEN_PT;
            let fit = |v: f32, lo: f32| kition_doc_geom::finite(v).clamp(lo, max);
            let (w, h) = (fit(ew * sx, 0.0), fit(eh * sy, 0.0));
            let obj = if e.name == "pic:pic" {
                let alt = e.find("pic:cNvPr").and_then(|p| p.attr("descr")).unwrap_or("").to_string();
                self.read_pic(e, rels, w, h, alt, Float::default())
            } else if e.name == "wpg:graphicFrame" {
                self.group_graphic(e, rels, w, h)
            } else {
                let mut obj = self.read_wsp(sc, e, rels, w, h, Float::default());
                // A member's id in the document (DR2), as a shape on its own has.
                if let InlineObject::Shape { uid, .. } = &mut obj {
                    *uid = e.child("wps:cNvPr").and_then(|p| p.attr("id")).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0);
                }
                Some(obj)
            };
            if let Some(obj) = obj {
                out.push(kition_doc_doc::para::GroupChild { x: fit(x0 + ox * sx, -max), y: fit(y0 + oy * sy, -max), obj });
            }
        }
    }

    /// A chart or smart graphic member of a group (`wpg:graphicFrame`), kept like one in the
    /// text (see `read_drawing`); `None` for other graphic frames.
    fn group_graphic(&mut self, frame: &El, rels: &Rels, w: f32, h: f32) -> Option<InlineObject> {
        let g = frame.child("a:graphic")?;
        let gd = g.child("a:graphicData")?;
        let uri = gd.attr("uri").unwrap_or("");
        let kind = if uri.ends_with("/chart") {
            GraphicKind::Chart
        } else if uri.ends_with("/diagram") {
            GraphicKind::Diagram
        } else {
            return None;
        };
        let extra: Vec<String> = match kind {
            GraphicKind::Chart => Vec::new(),
            GraphicKind::Diagram => self.diagram_drawing_rel(gd, rels).into_iter().collect(),
        };
        let source = self.keep_embedded(g, rels, &extra)?;
        let graphic = self.graphic(kind, gd, rels, w, h, Some(source), 0);
        let alt = frame.child("wpg:cNvPr").and_then(|p| p.attr("descr")).unwrap_or("").to_string();
        Some(InlineObject::Graphic { w, h, alt, float: Float::default(), graphic })
    }

    fn read_wsp(&mut self, sc: &mut StoryCtx, wsp: &El, rels: &Rels, w: f32, h: f32, mut float: Float) -> InlineObject {
        let sppr = wsp.child("wps:spPr");
        float.set_spin(xfrm_spin(sppr));
        let prst = sppr.and_then(|s| s.child("a:prstGeom")).and_then(|g| g.attr("prst")).unwrap_or("rect");
        let txbx = wsp.child("wps:txbx").and_then(|t| t.child("w:txbxContent"));
        let mut kind = match prst {
            "roundRect" => ShapeKind::RoundedRectangle,
            "ellipse" => ShapeKind::Ellipse,
            "triangle" | "rtTriangle" => ShapeKind::Triangle,
            "diamond" => ShapeKind::Diamond,
            "line" | "straightConnector1" => ShapeKind::Line,
            "rightArrow" | "leftArrow" | "upArrow" | "downArrow" => ShapeKind::Arrow,
            "star5" | "star4" | "star6" => ShapeKind::Star,
            "heart" => ShapeKind::Heart,
            _ => ShapeKind::Rectangle,
        };
        // A preset the kind does not name keeps its own outline and adjust values.
        let canonical = matches!(
            (kind, prst),
            (ShapeKind::Rectangle, "rect")
                | (ShapeKind::RoundedRectangle, "roundRect")
                | (ShapeKind::Ellipse, "ellipse")
                | (ShapeKind::Triangle, "triangle")
                | (ShapeKind::Diamond, "diamond")
                | (ShapeKind::Line, "line")
                | (ShapeKind::Arrow, "rightArrow")
                | (ShapeKind::Star, "star5")
                | (ShapeKind::Heart, "heart")
        );
        let adj = sppr.and_then(|s| s.child("a:prstGeom")).map(preset_adjust).unwrap_or_default();
        let geom = (!canonical || !adj.is_empty()).then(|| kition_doc_doc::graphic::PresetGeom { name: prst.to_string(), adj });
        let is_tb = wsp.child("wps:cNvSpPr").and_then(|c| c.attr("txBox")).is_some_and(|v| v == "1" || v == "true");
        if is_tb || (txbx.is_some() && kind == ShapeKind::Rectangle) {
            kind = ShapeKind::TextBox;
        }
        let theme = &self.doc.settings.theme_colors;
        // What the shape does not say itself comes from its style's theme references (`wps:style`):
        // a shape Word inserts gives its fill, outline and text colour only there.
        let style = wsp.child("wps:style");
        let style_ref = |name: &str| style.and_then(|s| s.child(name)).filter(|r| r.attr("idx").and_then(int).unwrap_or(0) > 0);
        let fills = |e: &El| {
            e.els().any(|c| matches!(c.name.as_str(), "a:noFill" | "a:solidFill" | "a:gradFill" | "a:pattFill" | "a:blipFill" | "a:grpFill"))
        };
        let gradient = sppr.and_then(|s| s.child("a:gradFill")).and_then(|g| super::drawing_color::gradient(g, theme)).map(Arc::new);
        // A gradient's first colour stands in where one colour is drawn.
        let fill = match sppr.filter(|s| fills(s)) {
            Some(s) => super::drawing_color::fill_colour(s, theme),
            None => style_ref("a:fillRef").and_then(|r| super::drawing_color::color(r, theme)),
        };
        let ln = sppr.and_then(|s| s.child("a:ln"));
        let line_ref = style_ref("a:lnRef");
        let stroke = match ln.filter(|l| fills(l)) {
            Some(l) => super::drawing_color::fill_colour(l, theme),
            None => line_ref.and_then(|r| super::drawing_color::color(r, theme)),
        };
        // The theme's line widths (Office's: 0.5, 1 and 1.5 points) for a referenced outline.
        let ref_width = line_ref.and_then(|r| r.attr("idx").and_then(int)).map_or(0.0, |idx| match idx {
            1 => 0.5,
            2 => 1.0,
            _ => 1.5,
        });
        let stroke_width = ln.and_then(|l| l.attr("w")).and_then(|v| measure(v, 12_700.0)).unwrap_or(ref_width).clamp(0.0, 100.0);
        let effects = sppr.and_then(|s| s.child("a:effectLst")).map(effect_list).unwrap_or_default();
        let text_color = style.and_then(|s| s.child("a:fontRef")).and_then(|r| super::drawing_color::color(r, theme));
        let story = match txbx {
            Some(t) if sc.story_depth < MAX_STORY_DEPTH => Some(self.read_textbox_colored(sc, t, rels, text_color)),
            _ => None,
        };
        // A connector's ends: other drawings' ids (`wp:docPr@id`, the shapes' `uid`) and sites.
        let ends = wsp.child("wps:cNvCnPr");
        let end = |name: &str| {
            ends.and_then(|c| c.child(name)).and_then(|e| {
                Some((e.attr("id")?.trim().parse::<u32>().ok()?, e.attr("idx").and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0)))
            })
        };
        let connection = Some(kition_doc_doc::graphic::Connection { start: end("a:stCxn"), end: end("a:endCxn") })
            .filter(|c| c.start.is_some() || c.end.is_some());
        let warp = wsp.child("wps:bodyPr").and_then(|b| b.child("a:prstTxWarp")).and_then(|t| {
            let name = t.attr("prst").filter(|p| *p != "textNoShape" && *p != "textPlain")?;
            let adj = t
                .child("a:avLst")
                .map(|av| {
                    av.els()
                        .filter(|gd| gd.name == "a:gd")
                        .filter_map(|gd| {
                            Some((
                                gd.attr("name")?.to_string(),
                                gd.attr("fmla")?.strip_prefix("val ")?.trim().parse::<f64>().ok().filter(|v| v.is_finite())?,
                            ))
                        })
                        .take(8)
                        .collect()
                })
                .unwrap_or_default();
            Some(kition_doc_doc::graphic::PresetGeom { name: name.to_string(), adj })
        });
        let body = wsp.child("wps:bodyPr").and_then(text_frame);
        // Shapes in groups use this same reader; parsing geometry only in the
        // outer drawing reader would replace their custom outlines with rectangles.
        let freeform = sppr.and_then(|s| {
            let mut f = super::freeform::cust_geom(s, w, h)?;
            f.alpha = super::freeform::line_alpha(s.child("a:ln"));
            kind = ShapeKind::Freeform;
            Some(Arc::new(f))
        });
        InlineObject::Shape {
            vml: None,
            kind,
            w,
            h,
            fill,
            stroke,
            stroke_width,
            float,
            story,
            freeform,
            effects,
            geom,
            uid: 0,
            gradient,
            connection,
            warp,
            body,
            dash: ln.and_then(|l| l.child("a:prstDash")).and_then(|d| d.attr("val")).and_then(kition_doc_doc::graphic::LineDash::parse),
        }
    }

    fn read_textbox(&mut self, sc: &StoryCtx, content: &El, rels: &Rels) -> u32 {
        self.read_textbox_colored(sc, content, rels, None)
    }

    /// `read_textbox`, its text in `color` (the shape style's font colour) where neither the text
    /// nor its paragraph style gives one, written then with the text.
    fn read_textbox_colored(&mut self, sc: &StoryCtx, content: &El, rels: &Rels, color: Option<Rgb>) -> u32 {
        let mut inner = StoryCtx { story_depth: sc.story_depth + 1, ..Default::default() };
        let mut blocks = Blocks::new();
        self.read_blocks(&mut inner, content, rels, &mut blocks, 0);
        self.flush_pending(&mut inner, &mut blocks);
        if let Some(c) = color {
            let styles = &self.doc.styles;
            // Whether the style `id` or a style it is based on sets a text colour.
            let colors = |id: &str| {
                let mut at = styles.get(id);
                for _ in 0..16 {
                    let Some(st) = at else { return false };
                    if st.chr.color.is_some() {
                        return true;
                    }
                    at = st.based_on.as_deref().and_then(|b| styles.get(b));
                }
                false
            };
            for block in &mut blocks {
                let Block::Para(p) = Arc::make_mut(block) else { continue };
                if colors(p.props.style.as_deref().unwrap_or("Normal")) {
                    continue;
                }
                for props in p.runs.iter_mut().map(|r| &mut r.props).chain(std::iter::once(&mut p.mark)) {
                    if props.color.is_none() && props.style.is_none() {
                        props.color = Some(TextColor::Rgb(c));
                    }
                }
            }
        }
        // A story ends with a paragraph (Word adds one after a last table, as writing does).
        if blocks.last().is_none_or(|b| matches!(**b, Block::Table(_))) {
            blocks.push(para_block(Paragraph::new()));
        }
        self.doc.add_part(PartKind::TextBox, blocks)
    }

    /// A legacy VML drawing (`w:pict`). A picture is shown, and written back as the VML it was
    /// (its own markup and parts kept, as for an OLE object); a text box is read into a text box
    /// (its text edited, written back in its VML drawing while that still fits it, else as
    /// DrawingML); a drawing Kition Doc does not model (lines,
    /// rectangles, groups of them) is kept as it was, unseen, and written back.
    fn read_pict(&mut self, sc: &mut StoryCtx, pict: &El, rels: &Rels) -> Option<InlineObject> {
        match self.read_vml(sc, pict, rels) {
            Some(mut o) => {
                match &mut o {
                    InlineObject::Image { ole, .. } => *ole = self.keep_embedded_showing(pict, rels, &[], vml_image_id(pict)),
                    // A text box keeps its drawing without its text (the text is the story's);
                    // Art text keeps its drawing whole (its string is written from the story).
                    InlineObject::Shape { vml, warp, .. } => {
                        let mut shell = pict.clone();
                        if empty_first(&mut shell, "w:txbxContent", 0) {
                            *vml = self.keep_embedded(&shell, rels, &[]);
                        } else if warp.is_some() {
                            *vml = self.keep_embedded(pict, rels, &[]);
                        }
                    }
                    _ => {}
                }
                Some(o)
            }
            None => super::embed::kept_run(pict),
        }
    }

    /// Legacy VML pictures and text boxes (best effort).
    fn read_vml(&mut self, sc: &mut StoryCtx, pict: &El, rels: &Rels) -> Option<InlineObject> {
        let shape = vml_shape(pict)?;
        let (w, h, float) = vml_frame(shape);
        if let Some(id) = vml_image_id(pict) {
            let media = self.media_for(rels, id)?;
            let alt = shape
                .attr("alt")
                .or_else(|| find_outside_text(shape, "v:imagedata", 0).and_then(|img| img.attr("o:title")))
                .unwrap_or("")
                .to_string();
            return Some(InlineObject::Image {
                edge: Default::default(),
                geom: None,
                media,
                w,
                h,
                alt,
                float,
                crop: [0.0; 4],
                ole: None,
                adjust: Default::default(),
                uid: 0,
            });
        }
        if let Some(t) = shape.find("w:txbxContent")
            && sc.story_depth < MAX_STORY_DEPTH
        {
            let story = Some(self.read_textbox(sc, t, rels));
            let (fill, stroke, stroke_width) = vml_paint(shape);
            return Some(InlineObject::Shape {
                vml: None,
                kind: ShapeKind::TextBox,
                w,
                h,
                fill,
                stroke,
                stroke_width,
                float,
                story,
                freeform: None,
                effects: Default::default(),
                geom: None,
                uid: 0,
                gradient: None,
                connection: None,
                warp: None,
                body: None,
                dash: None,
            });
        }
        self.read_vml_art_text(shape, w, h, float)
    }

    /// A VML art text drawing (`v:textpath`, as Word's and WPS's text watermarks are): a text box
    /// holding its string in its font and fill colour (over the page at the fill's opacity),
    /// stretched to fill the shape as art text's plain text warp is. `None` without a string or a
    /// fill to see.
    fn read_vml_art_text(&mut self, shape: &El, w: f32, h: f32, float: Float) -> Option<InlineObject> {
        let path = shape.child("v:textpath").filter(|t| !t.attr("on").is_some_and(vml_off))?;
        let string: String = path.attr("string")?.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        let string = string.trim();
        if string.is_empty() {
            return None;
        }
        let (fill, _, _) = vml_paint(shape);
        let opacity = shape.child("v:fill").and_then(|f| f.attr("opacity")).map_or(1.0, vml_fraction);
        let color = fill.filter(|_| opacity > 0.0).map(|c| super::drawing_color::over_white(c, opacity))?;
        let style = path.attr("style").unwrap_or("");
        let font = vml_style(style, "font-family").map(|f| f.trim().trim_matches(|c| c == '"' || c == '\'').to_string()).filter(|f| !f.is_empty());
        let bold = vml_style(style, "font-weight").map(|v| v.eq_ignore_ascii_case("bold") || v.parse::<u32>().is_ok_and(|n| n >= 600));
        let italic = vml_style(style, "font-style").map(|v| v.eq_ignore_ascii_case("italic"));
        // The text is stretched to the shape, so its size only has to keep it on one line.
        let size = (w / string.chars().count().max(1) as f32 * 0.8).clamp(1.0, 12.0);
        let props =
            CharProps { font: font.clone(), font_ea: font, size: Some(size), bold, italic, color: Some(TextColor::Rgb(color)), ..Default::default() };
        let mut p = Paragraph::with_text(string, props.clone());
        p.mark = props;
        p.props = kition_doc_doc::props::ParaProps {
            align: Some(kition_doc_doc::props::Align::Center),
            space_before: Some(0.0),
            space_after: Some(0.0),
            line_spacing: Some(kition_doc_doc::props::LineSpacing::Multiple(1.0)),
            snap_to_grid: Some(false),
            ..Default::default()
        };
        let story = self.doc.add_part(PartKind::TextBox, vec![para_block(p)]);
        Some(InlineObject::Shape {
            vml: None,
            kind: ShapeKind::TextBox,
            w,
            h,
            fill: None,
            stroke: None,
            stroke_width: 0.0,
            float,
            story: Some(story),
            freeform: None,
            effects: Default::default(),
            geom: None,
            uid: 0,
            gradient: None,
            connection: None,
            warp: Some(kition_doc_doc::graphic::PresetGeom { name: "textPlain".to_string(), adj: Vec::new() }),
            body: Some(kition_doc_doc::graphic::TextFrame {
                anchor: kition_doc_doc::graphic::TextAnchor::Middle,
                insets: [0.0; 4],
                fit: false,
                no_wrap: false,
                fit_height: false,
            }),
            dash: None,
        })
    }

    /// An OLE object (`w:object`, ECMA-376 §17.3.3.19): shown as its picture (VML, or a DrawingML
    /// picture in `w:drawing`), and kept whole so saving writes the object back.
    fn read_object(&mut self, sc: &mut StoryCtx, obj: &El, rels: &Rels) -> Option<InlineObject> {
        let mut o = match self.read_vml(sc, obj, rels) {
            Some(o) => o,
            None => {
                let d = obj.child("w:drawing")?;
                self.read_drawing(sc, d, rels)?
            }
        };
        if let InlineObject::Image { ole, .. } = &mut o {
            // Its picture is the image's media file, written once (not carried as a part too).
            *ole = self.keep_embedded_showing(obj, rels, &[], vml_image_id(obj));
        }
        Some(o)
    }

    // ---- tables ----

    fn read_table(&mut self, sc: &mut StoryCtx, t: &El, rels: &Rels, depth: usize) -> Option<Table> {
        let mut table = Table::default();
        if let Some(p) = t.child("w:tblPr") {
            table.props = self.pc.tblpr(p);
            if let Some(ch) = p.child("w:tblPrChange") {
                let old = ch.child("w:tblPr").map(|o| self.pc.tblpr(o)).unwrap_or_default();
                table.props.fmt_change = PropChange::boxed(self.revision(RevisionKind::Format, ch), old);
            }
        }
        if let Some(g) = t.child("w:tblGrid") {
            table.grid = g
                .children("w:gridCol")
                .take(MAX_COLS)
                .map(|c| crate::units::tw(c, "w:w").unwrap_or(0.0).clamp(0.0, crate::units::MAX_LEN_PT))
                .collect();
        }
        let mut rows = Vec::new();
        collect(t, "w:tr", &mut rows, 0);
        for tr in rows.into_iter().take(MAX_ROWS) {
            let mut row = Row { props: tr.child("w:trPr").map(trpr).unwrap_or_default(), cells: Vec::new() };
            if let Some(ch) = tr.child("w:trPr").and_then(|p| p.child("w:trPrChange")) {
                let old = ch.child("w:trPr").map(trpr).unwrap_or_default();
                row.props.fmt_change = PropChange::boxed(self.revision(RevisionKind::Format, ch), old);
            }
            row.props.para_id = tr.attr("w14:paraId").and_then(super::para_id);
            row.props.exceptions = tr.child("w:tblPrEx").map(El::to_xml).filter(|x| x.len() <= crate::write::MAX_KEPT_PROPS);
            let mut tcs = Vec::new();
            collect(tr, "w:tc", &mut tcs, 0);
            for tc in tcs.into_iter().take(MAX_COLS) {
                let (mut props, hcont) =
                    tc.child("w:tcPr").map(tcpr).unwrap_or_else(|| (kition_doc_doc::props::CellProps { span: 1, ..Default::default() }, false));
                if let Some(ch) = tc.child("w:tcPr").and_then(|p| p.child("w:tcPrChange")) {
                    let old = ch.child("w:tcPr").map(|o| tcpr(o).0).unwrap_or_default();
                    props.fmt_change = PropChange::boxed(self.revision(RevisionKind::Format, ch), old);
                }
                if hcont && let Some(prev) = row.cells.last_mut() {
                    prev.props.span = (prev.props.span + props.span.max(1)).min(63);
                    continue;
                }
                let mut blocks = Blocks::new();
                self.read_blocks(sc, tc, rels, &mut blocks, depth + 4);
                self.flush_pending(sc, &mut blocks);
                row.cells.push(Cell { props, blocks });
            }
            if !row.cells.is_empty() {
                table.rows.push(row);
            }
        }
        if table.rows.is_empty() {
            return None;
        }
        if table.grid.is_empty() {
            let cols = table.cols().clamp(1, MAX_COLS);
            table.grid = vec![72.0; cols];
        }
        Some(table)
    }
}

/// Gather `name` children, looking through `w:sdt`/`w:customXml` wrappers.
fn collect<'a>(e: &'a El, name: &str, out: &mut Vec<&'a El>, depth: usize) {
    if depth > 8 {
        return;
    }
    for k in e.els() {
        if k.name == name {
            out.push(k);
        } else if k.name == "w:sdt" {
            if let Some(c) = k.child("w:sdtContent") {
                collect(c, name, out, depth + 1);
            }
        } else if k.name == "w:customXml" {
            collect(k, name, out, depth + 1);
        }
    }
}

fn on_off_attr(e: &El, name: &str) -> bool {
    e.attr(name).is_some_and(|v| !matches!(v, "0" | "false" | "off"))
}

/// The bounds of a wrap polygon (`wp:start`, `wp:lineTo`, in 21600ths of the object's frame) as
/// shares of the frame (left, top, right, bottom). `None` without points, or for the whole frame.
/// A wrap polygon's points, as shares of the frame (21600 units); `None` for the whole frame.
fn wrap_polygon(p: &El) -> Option<WrapPolygon> {
    let share = |e: &El, a: &str| e.attr(a).and_then(int).map(|v| (v as f32 / 21_600.0).clamp(-1.0, 2.0));
    let points: Vec<(f32, f32)> = p
        .els()
        .filter(|e| matches!(e.name.as_str(), "wp:start" | "wp:lineTo"))
        .take(10_000)
        .filter_map(|e| share(e, "x").zip(share(e, "y")))
        .collect();
    WrapPolygon::new(&points).filter(|polygon| !polygon.is_frame())
}

fn anchor_float(c: &El) -> Float {
    let mut f = Float {
        wrap: Wrap::Square,
        z: c.attr("relativeHeight").and_then(|v| v.trim().parse::<i64>().ok()),
        keep_apart: c.attr("allowOverlap").is_some_and(|v| matches!(v, "0" | "false" | "off")),
        ..Float::default()
    };
    let behind = on_off_attr(c, "behindDoc");
    // Word 2010 writes percentage positions (`wp14:pctPosHOffset`) as alternate content with an
    // absolute fallback, the position Word laid the object out at; the fallback is read.
    let fallback = c.children("mc:AlternateContent").filter_map(|ac| ac.child("mc:Fallback")).flat_map(El::els);
    for k in c.els().chain(fallback) {
        match k.name.as_str() {
            "wp:wrapSquare" => f.wrap = Wrap::Square,
            "wp:wrapTight" | "wp:wrapThrough" => {
                f.wrap = if k.name == "wp:wrapTight" { Wrap::Tight } else { Wrap::Through };
                f.wrap_polygon = k.child("wp:wrapPolygon").and_then(wrap_polygon);
            }
            "wp:wrapTopAndBottom" => f.wrap = Wrap::TopAndBottom,
            "wp14:sizeRelH" => {
                let from = match k.attr("relativeFrom").unwrap_or("") {
                    "page" => Anchor::Page,
                    "leftMargin" => Anchor::LeftMargin,
                    "rightMargin" => Anchor::RightMargin,
                    "insideMargin" => Anchor::InsideMargin,
                    "outsideMargin" => Anchor::OutsideMargin,
                    _ => Anchor::Margin,
                };
                let share = k.child("wp14:pctWidth").and_then(|p| int(p.text().trim())).map(|v| (v as f32 / 100_000.0).clamp(0.0, 10.0));
                f.rel_width = share.filter(|s| *s > 0.0).map(|share| RelSize { from, share });
            }
            "wp:wrapNone" => f.wrap = if behind { Wrap::BehindText } else { Wrap::InFrontOfText },
            "wp:positionH" | "wp:positionV" => {
                let horiz = k.name == "wp:positionH";
                let rel = match (k.attr("relativeFrom").unwrap_or(""), horiz) {
                    ("page", _) => Anchor::Page,
                    ("margin", _) => Anchor::Margin,
                    ("leftMargin", true) => Anchor::LeftMargin,
                    ("rightMargin", true) => Anchor::RightMargin,
                    ("insideMargin", _) => Anchor::InsideMargin,
                    ("outsideMargin", _) => Anchor::OutsideMargin,
                    ("character", true) => Anchor::Character,
                    ("topMargin", false) => Anchor::TopMargin,
                    ("bottomMargin", false) => Anchor::BottomMargin,
                    ("line", false) => Anchor::Line,
                    ("paragraph", false) => Anchor::Paragraph,
                    (_, true) => Anchor::Column,
                    (_, false) => Anchor::Paragraph,
                };
                let align = k.child("wp:align").and_then(|a| FloatAlign::from_ooxml(a.text().trim()));
                let off = k.child("wp:posOffset").and_then(|o| measure(&o.text(), 12_700.0)).unwrap_or(0.0);
                if horiz {
                    f.h_rel = rel;
                    f.h_align = align;
                    f.x = off;
                } else {
                    f.v_rel = rel;
                    f.v_align = align;
                    f.y = off;
                }
            }
            _ => {}
        }
    }
    let dist = |n: &str| c.attr(n).and_then(|v| measure(v, 12_700.0)).unwrap_or(0.0).clamp(0.0, 1584.0);
    f.dist = dist("distL").max(dist("distR"));
    f.dist_top = dist("distT");
    f.dist_bottom = dist("distB");
    f
}

/// First descendant named `name` outside text box content (a picture inside a text box is the
/// text's, not the drawing's).
fn find_outside_text<'a>(e: &'a El, name: &str, depth: usize) -> Option<&'a El> {
    if depth > crate::xml::MAX_DEPTH {
        return None;
    }
    e.els().filter(|c| c.name != "w:txbxContent").find_map(|c| if c.name == name { Some(c) } else { find_outside_text(c, name, depth + 1) })
}

/// Empty the first `name` element in `e` (depth first). False when there is none.
fn empty_first(e: &mut El, name: &str, depth: usize) -> bool {
    if depth > crate::xml::MAX_DEPTH {
        return false;
    }
    for k in e.kids.iter_mut() {
        if let crate::xml::Node::El(c) = k {
            if c.name == name {
                c.kids.clear();
                return true;
            }
            if empty_first(c, name, depth + 1) {
                return true;
            }
        }
    }
    false
}

/// The adjust values of a preset outline (`a:prstGeom/a:avLst`, `a:gd` with `val N`), at most 16.
fn preset_adjust(g: &El) -> Vec<(String, f64)> {
    g.child("a:avLst")
        .map(|av| {
            av.els()
                .filter(|gd| gd.name == "a:gd")
                .filter_map(|gd| {
                    Some((gd.attr("name")?.to_string(), gd.attr("fmla")?.strip_prefix("val ")?.trim().parse::<f64>().ok().filter(|v| v.is_finite())?))
                })
                .take(16)
                .collect()
        })
        .unwrap_or_default()
}

/// A picture's colour adjustments from its `a:blip` effects (`a:lum`, `a:grayscl`, `a:biLevel`,
/// `a:alphaModFix`; amounts in thousandths of a percent).
fn image_adjust(blip: &El) -> kition_doc_doc::graphic::ImageAdjust {
    let share = |e: &El, a: &str, def: f32| e.attr(a).and_then(int).map_or(def, |v| (v as f32 / 100_000.0).clamp(-1.0, 1.0));
    let mut adjust = kition_doc_doc::graphic::ImageAdjust::default();
    for e in blip.els() {
        match e.name.as_str() {
            "a:lum" => {
                adjust.bright = share(e, "bright", 0.0);
                adjust.contrast = share(e, "contrast", 0.0);
            }
            "a:grayscl" => adjust.gray = true,
            "a:biLevel" => adjust.bilevel = Some(share(e, "thresh", 0.5).max(0.0)),
            "a:alphaModFix" => adjust.opacity = share(e, "amt", 1.0).max(0.0),
            _ => {}
        }
    }
    adjust
}

/// A VML boolean that is off (`f`, `false`, `0`).
fn vml_off(v: &str) -> bool {
    matches!(v.trim(), "f" | "false" | "0")
}

/// A VML fraction (`opacity`): 65536ths with an `f` suffix, a percentage, or a plain number;
/// within 0 and 1 (1 when it can't be read).
fn vml_fraction(v: &str) -> f64 {
    let v = v.trim();
    let value = if let Some(f) = v.strip_suffix('f') {
        f.trim().parse::<f64>().ok().map(|f| f / 65_536.0)
    } else if let Some(p) = v.strip_suffix('%') {
        p.trim().parse::<f64>().ok().map(|p| p / 100.0)
    } else {
        v.parse::<f64>().ok()
    };
    value.filter(|f| f.is_finite()).map_or(1.0, |f| f.clamp(0.0, 1.0))
}

/// A VML shape's fill, pen colour and pen width (points), as a text box reads them: `filled` /
/// `stroked="f"` or a `v:fill` / `v:stroke` turned off is none; VML's default pen is 0.75 pt and
/// a weight without a unit is in EMU.
pub(crate) fn vml_paint(shape: &El) -> (Option<Rgb>, Option<Rgb>, f32) {
    let on = |attr: &str, child: &str| {
        let off = |v: Option<&str>| v.is_some_and(|v| matches!(v.trim(), "f" | "false" | "0"));
        !off(shape.attr(attr)) && !off(shape.child(child).and_then(|c| c.attr("on")))
    };
    let fill = shape.attr("fillcolor").and_then(Rgb::parse).filter(|_| on("filled", "v:fill"));
    let line = shape.child("v:stroke");
    let stroke = shape
        .attr("strokecolor")
        .or_else(|| line.and_then(|l| l.attr("color")))
        .and_then(Rgb::parse)
        .or_else(|| (shape.attr("stroked").is_some_and(|v| matches!(v.trim(), "t" | "true" | "1")) || line.is_some()).then_some(Rgb::BLACK))
        .filter(|_| on("stroked", "v:stroke"));
    let stroke_width = match stroke {
        Some(_) => shape
            .attr("strokeweight")
            .or_else(|| line.and_then(|l| l.attr("weight")))
            .and_then(|v| measure(v, 12_700.0))
            .unwrap_or(0.75)
            .clamp(0.0, 100.0),
        None => 0.0,
    };
    (fill, stroke, stroke_width)
}

/// The shape a VML drawing (`w:pict`, `w:object`) draws: its first VML element but a shape type.
pub(crate) fn vml_shape(pict: &El) -> Option<&El> {
    pict.els().find(|e| e.name.starts_with("v:") && e.local() != "shapetype")
}

/// The relationship id of the picture a VML drawing shows (an empty `v:imagedata` names none: a
/// text box can carry one).
fn vml_image_id(pict: &El) -> Option<&str> {
    let img = find_outside_text(vml_shape(pict)?, "v:imagedata", 0)?;
    img.attr("r:id").or_else(|| img.attr("r:pict")).filter(|id| !id.is_empty())
}

/// A VML shape's size and placement, from its `style`.
pub(crate) fn vml_frame(shape: &El) -> (f32, f32, Float) {
    let style = shape.attr("style").unwrap_or("");
    let len = |k: &str| vml_style(style, k).and_then(|v| measure(v, 1.0)).unwrap_or(0.0).clamp(0.0, crate::units::MAX_LEN_PT);
    (len("width"), len("height"), vml_float(shape, style))
}

/// The value of property `key` in a VML `style` attribute (CSS declarations; the last one wins).
fn vml_style<'a>(style: &'a str, key: &str) -> Option<&'a str> {
    style.rsplit(';').filter_map(|d| d.split_once(':')).find(|(k, _)| k.trim().eq_ignore_ascii_case(key)).map(|(_, v)| v.trim())
}

/// Placement of a VML shape (ECMA-376 Part 4, VML): `position:absolute` in its style makes it
/// float, offset by `left`/`margin-left` and `top`/`margin-top` from the area named by
/// `mso-position-horizontal-relative` / `mso-position-vertical-relative` (unless aligned with
/// `mso-position-horizontal` / `mso-position-vertical`), wrapped as its `w10:wrap` says. Without
/// wrapping (`none`, or no `w10:wrap`) it's in front of the text, or behind it at a negative
/// `z-index`. Anything else stays inline.
fn vml_float(shape: &El, style: &str) -> Float {
    let get = |k: &str| vml_style(style, k);
    if !get("position").is_some_and(|p| p.eq_ignore_ascii_case("absolute")) {
        // Inline: only the turn (an OLE object turned in Kition Doc is saved so).
        let mut f = Float::default();
        f.set_spin(vml_spin(style));
        return f;
    }
    let off = |k: &str| get(k).and_then(|v| measure(v, 1.0)).unwrap_or(0.0);
    let max = crate::units::MAX_LEN_PT;
    let behind = get("z-index").and_then(int).is_some_and(|z| z < 0);
    let wrap = match shape.child("w10:wrap").and_then(|w| w.attr("type")).unwrap_or("none") {
        "square" => Wrap::Square,
        "tight" => Wrap::Tight,
        "through" => Wrap::Through,
        "topAndBottom" => Wrap::TopAndBottom,
        _ if behind => Wrap::BehindText,
        _ => Wrap::InFrontOfText,
    };
    let h_rel = match get("mso-position-horizontal-relative").unwrap_or("text") {
        "page" => Anchor::Page,
        "margin" => Anchor::Margin,
        "char" => Anchor::Character,
        "left-margin-area" => Anchor::LeftMargin,
        "right-margin-area" => Anchor::RightMargin,
        "inner-margin-area" => Anchor::InsideMargin,
        "outer-margin-area" => Anchor::OutsideMargin,
        _ => Anchor::Column,
    };
    let v_rel = match get("mso-position-vertical-relative").unwrap_or("text") {
        "page" => Anchor::Page,
        "margin" => Anchor::Margin,
        "line" => Anchor::Line,
        "top-margin-area" => Anchor::TopMargin,
        "bottom-margin-area" => Anchor::BottomMargin,
        "inner-margin-area" => Anchor::InsideMargin,
        "outer-margin-area" => Anchor::OutsideMargin,
        _ => Anchor::Paragraph,
    };
    let dist = |k: &str| get(k).and_then(|v| measure(v, 1.0)).unwrap_or(0.0).clamp(0.0, 1584.0);
    let mut f = Float {
        wrap,
        h_rel,
        v_rel,
        x: (off("left") + off("margin-left")).clamp(-max, max),
        y: (off("top") + off("margin-top")).clamp(-max, max),
        h_align: get("mso-position-horizontal").and_then(FloatAlign::from_ooxml),
        v_align: get("mso-position-vertical").and_then(FloatAlign::from_ooxml),
        dist: dist("mso-wrap-distance-left").max(dist("mso-wrap-distance-right")),
        dist_top: dist("mso-wrap-distance-top"),
        dist_bottom: dist("mso-wrap-distance-bottom"),
        ..Float::default()
    };
    f.set_spin(vml_spin(style));
    f
}

/// A VML shape's turn: `rotation` in degrees, or 65536ths of one with an `fd` or `f` suffix;
/// `flip`: `x` and/or `y`.
fn vml_spin(style: &str) -> kition_doc_geom::Spin {
    let get = |k: &str| vml_style(style, k);
    let rot = get("rotation").and_then(|v| match v.strip_suffix("fd").or_else(|| v.strip_suffix('f')) {
        Some(fd) => fd.trim().parse::<f32>().ok().map(|f| f / 65_536.0),
        None => v.trim().parse::<f32>().ok(),
    });
    let flip = get("flip").unwrap_or("");
    kition_doc_geom::Spin::new(rot.unwrap_or(0.0), flip.contains('x'), flip.contains('y'))
}

/// What a graphic's items cost to keep and draw: one per item, plus one per path segment.
fn graphic_work(items: &[GraphicItem]) -> usize {
    items.iter().map(|it| if let GraphicItem::Path { segs, .. } = it { segs.len() + 1 } else { 1 }).sum()
}

/// A shape's `a:effectLst` (ECMA-376 Part 1 §20.1.8.26): its outer shadow, glow and soft edges.
/// Other effects are dropped.
fn effect_list(l: &El) -> ShapeEffects {
    let pt = |e: &El, n: &str| e.attr(n).and_then(|v| measure(v, 12_700.0)).unwrap_or(0.0);
    let shadow = l.child("a:outerShdw").map(|e| {
        let (color, transparency) = effect_color(e);
        // `dir`: 60000ths of a degree, clockwise.
        let angle = e.attr("dir").and_then(int).map(|v| (v.rem_euclid(21_600_000) as f32) / 60_000.0).unwrap_or(0.0);
        // `rotWithShape` is true unless it says otherwise (ECMA-376 §20.1.8.49).
        let rot_with_shape = e.attr("rotWithShape").is_none_or(|v| !matches!(v, "0" | "false" | "off"));
        Shadow { color: color.unwrap_or(Rgb::BLACK), transparency, blur: pt(e, "blurRad"), distance: pt(e, "dist"), angle, rot_with_shape }
    });
    let glow = l.child("a:glow").map(|e| {
        let (color, transparency) = effect_color(e);
        Glow { color: color.unwrap_or(Glow::default().color), size: pt(e, "rad"), transparency }
    });
    let soft_edge = l.child("a:softEdge").map(|e| pt(e, "rad"));
    ShapeEffects { shadow, glow, soft_edge }.sanitized()
}

/// The colour inside an effect (`a:srgbClr`, `a:prstClr`, `a:sysClr`; theme colours aren't
/// resolved here) and its transparency, percent (from `a:alpha`, opaque when absent).
fn effect_color(e: &El) -> (Option<Rgb>, f32) {
    let Some(c) = e.els().find(|c| matches!(c.name.as_str(), "a:srgbClr" | "a:prstClr" | "a:sysClr" | "a:schemeClr" | "a:scrgbClr" | "a:hslClr"))
    else {
        return (None, 0.0);
    };
    let rgb = match c.name.as_str() {
        "a:srgbClr" => c.attr("val").and_then(Rgb::parse),
        "a:sysClr" => c.attr("lastClr").and_then(Rgb::parse),
        "a:prstClr" => match c.attr("val").unwrap_or("") {
            "black" => Some(Rgb::BLACK),
            "white" => Some(Rgb::WHITE),
            "gray" | "grey" => Some(Rgb(0x80, 0x80, 0x80)),
            _ => None,
        },
        _ => None,
    };
    // `a:alpha`: opacity in 1000ths of a percent.
    let opacity = c.child("a:alpha").and_then(|a| a.attr("val")).and_then(int).map(|v| v.clamp(0, 100_000) as f32 / 1000.0).unwrap_or(100.0);
    (rgb, 100.0 - opacity)
}

/// A DrawingML `a:xfrm` (ECMA-376 §20.1.7.5/6) in points: offset, extent and, for a group, its
/// members' offset and extent.
struct Xfrm {
    off: (f32, f32),
    ext: (f32, f32),
    ch_off: Option<(f32, f32)>,
    ch_ext: Option<(f32, f32)>,
}

/// The rotation and flips of the `a:xfrm` in shape properties `sppr` (ECMA-376 §20.1.7.6:
/// `rot` in 60000ths of a degree, clockwise; `flipH`, `flipV`). None when missing or junk.
fn xfrm_spin(sppr: Option<&El>) -> kition_doc_geom::Spin {
    let Some(x) = sppr.and_then(|s| s.child("a:xfrm")) else { return kition_doc_geom::Spin::default() };
    let rot = x.attr("rot").and_then(int).map(|v| v.rem_euclid(21_600_000) as f32 / 60_000.0).unwrap_or(0.0);
    kition_doc_geom::Spin::new(rot, on_off_attr(x, "flipH"), on_off_attr(x, "flipV"))
}

/// The `a:xfrm` in shape properties `sppr` (zeros when missing).
fn group_xfrm(sppr: Option<&El>) -> Xfrm {
    xfrm_of(sppr.and_then(|s| s.child("a:xfrm")))
}

/// An `a:xfrm` (or a frame's `wpg:xfrm`) in points.
fn xfrm_of(x: Option<&El>) -> Xfrm {
    let pair = |name: &str, a: &str, b: &str, lim: f32| {
        let e = x.and_then(|x| x.child(name))?;
        let v = |n: &str| e.attr(n).and_then(|v| measure(v, 12_700.0)).map(|v| v.clamp(-lim, lim)).unwrap_or(0.0);
        Some((v(a), v(b)))
    };
    let max = crate::units::MAX_LEN_PT;
    let pos = |p: Option<(f32, f32)>| p.map(|(a, b)| (a.max(0.0), b.max(0.0)));
    Xfrm {
        off: pair("a:off", "x", "y", max).unwrap_or((0.0, 0.0)),
        ext: pos(pair("a:ext", "cx", "cy", max)).unwrap_or((0.0, 0.0)),
        ch_off: pair("a:chOff", "x", "y", max),
        ch_ext: pos(pair("a:chExt", "cx", "cy", max)),
    }
}

/// A shape's text frame from its `wps:bodyPr`: its anchor and margins, `None` when both are the
/// defaults.
fn text_frame(body: &El) -> Option<kition_doc_doc::graphic::TextFrame> {
    use kition_doc_doc::graphic::{TextAnchor, TextFrame};
    let anchor = match body.attr("anchor") {
        Some("ctr") => TextAnchor::Middle,
        Some("b") => TextAnchor::Bottom,
        _ => TextAnchor::Top,
    };
    let mut insets = TextFrame::DEFAULT_INSETS;
    for (slot, name) in insets.iter_mut().zip(["lIns", "tIns", "rIns", "bIns"]) {
        if let Some(v) = body.attr(name).and_then(|v| measure(v, 12_700.0)) {
            *slot = v.clamp(0.0, 1584.0);
        }
    }
    let unwrapped = body.attr("wrap") == Some("none");
    let auto_fit = body.child("a:spAutoFit").is_some();
    let fit = unwrapped && auto_fit;
    let frame = TextFrame { anchor, insets, fit, no_wrap: unwrapped && !fit, fit_height: auto_fit && !unwrapped };
    (frame != TextFrame::default()).then_some(frame)
}

/// A content control's marks: their font size is `w:sz` in the `w:rPr` of its `w:sdtPr` (start)
/// or `w:sdtEndPr` (end), 0 when not set (the paragraph style's).
fn control_marks(sdt: &El) -> ControlMarks {
    let size = |pr: &str| -> f32 {
        let half = sdt.child(pr).and_then(|p| p.child("w:rPr")).and_then(|r| r.child("w:sz")).and_then(|z| z.attr("w:val"));
        half.and_then(|v| v.parse::<f32>().ok()).filter(|h| h.is_finite() && *h > 0.0).map_or(0.0, |h| (h / 2.0).min(1638.0))
    };
    ControlMarks { start: Some(size("w:sdtPr")), end: Some(size("w:sdtEndPr")) }
}
