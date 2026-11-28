//! Caret movement and selection.

use kition_doc_doc::{Pos, StoryRef};
use serde_json::Value;

use super::{parse_pos, sel_result};
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

const EXTEND: &str = r#"{"extend"?: bool}"#;
const PAGE: &str = r#"{"extend"?: bool, "distance"?: pt}"#;
/// The farthest one Page Up or Page Down goes, in points (a screen is far less).
const MAX_PAGE_DISTANCE: f32 = 100_000.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("caret.left", "Left", "Navigation", |s, v| lr(s, v, true)).key("Left").params(EXTEND).pure(),
        CommandSpec::new("caret.right", "Right", "Navigation", |s, v| lr(s, v, false)).key("Right").params(EXTEND).pure(),
        CommandSpec::new("caret.up", "Up", "Navigation", |s, v| vert(s, v, -1)).key("Up").params(EXTEND).pure(),
        CommandSpec::new("caret.down", "Down", "Navigation", |s, v| vert(s, v, 1)).key("Down").params(EXTEND).pure(),
        // In a right-to-left paragraph the word to the left is the next one.
        CommandSpec::new("caret.wordLeft", "Word Left", "Navigation", |s, v| mv(s, v, if rtl_para(s) { word_right } else { word_left }))
            .key("Mod+Left / Alt+Left")
            .mac("Alt+Left")
            .params(EXTEND)
            .pure(),
        CommandSpec::new("caret.wordRight", "Word Right", "Navigation", |s, v| mv(s, v, if rtl_para(s) { word_left } else { word_right }))
            .key("Mod+Right / Alt+Right")
            .mac("Alt+Right")
            .params(EXTEND)
            .pure(),
        // On a Mac ⌘← and ⌘→ go to the ends of the line, as Home and End do (WPS W-D3).
        CommandSpec::new("caret.home", "Line Start", "Navigation", |s, v| line(s, v, true)).key("Home").mac("Home / Mod+Left").params(EXTEND).pure(),
        CommandSpec::new("caret.end", "Line End", "Navigation", |s, v| line(s, v, false)).key("End").mac("End / Mod+Right").params(EXTEND).pure(),
        CommandSpec::new("caret.paraUp", "Paragraph Up", "Navigation", |s, v| mv(s, v, para_up)).key("Mod+Up / Alt+Up").params(EXTEND).pure(),
        CommandSpec::new("caret.paraDown", "Paragraph Down", "Navigation", |s, v| mv(s, v, para_down))
            .key("Mod+Down / Alt+Down")
            .params(EXTEND)
            .pure(),
        CommandSpec::new("caret.docStart", "Document Start", "Navigation", |s, v| mv(s, v, |s, _| s.doc.start_of(s.sel.focus.story)))
            .key("Mod+Home")
            .params(EXTEND)
            .pure(),
        CommandSpec::new("caret.docEnd", "Document End", "Navigation", |s, v| mv(s, v, |s, _| s.doc.end_of(s.sel.focus.story)))
            .key("Mod+End")
            .params(EXTEND)
            .pure(),
        // `distance` is a screen's height, which only the editor knows (it binds PageUp and
        // PageDown with it); without it the caret goes to the start of the page before or after
        // (⌘PageUp, ⌘PageDown; WPS W-D7, W-D8).
        CommandSpec::new("caret.pageUp", "Page Up", "Navigation", |s, v| page(s, v, -1)).key("Mod+PageUp").params(PAGE).pure(),
        CommandSpec::new("caret.pageDown", "Page Down", "Navigation", |s, v| page(s, v, 1)).key("Mod+PageDown").params(PAGE).pure(),
        CommandSpec::new("caret.set", "Set Caret", "Navigation", set)
            .params(r#"{"pos": Pos, "extend"?: bool} | {"page": n, "x": pt, "y": pt}"#)
            .pure(),
        CommandSpec::new("edit.selectAll", "Select All", "Home › Editing › Select", select_all).key("Mod+A").pure(),
        CommandSpec::new("select.range", "Select Range", "Navigation", select_range).params(r#"{"anchor": Pos, "focus": Pos}"#).pure(),
        CommandSpec::new("select.word", "Select Word", "Navigation", select_word).pure(),
        CommandSpec::new("select.sentence", "Select Sentence", "Navigation", select_sentence).pure(),
        CommandSpec::new("select.paragraph", "Select Paragraph", "Navigation", select_paragraph).pure(),
        CommandSpec::new("select.line", "Select Line", "Navigation", select_line).pure(),
        CommandSpec::new("select.text", "Select Text", "Navigation", select_text).params(r#"{"text": string, "occurrence"?: n}"#).pure(),
        CommandSpec::new("select.collapse", "Collapse Selection", "Navigation", |s, v| {
            let to_end = p::bool(v, "end").unwrap_or(false);
            let (a, b) = s.sel.ordered();
            s.sel = Selection::caret(if to_end { b } else { a });
            sel_result(s)
        })
        .key("Escape")
        .pure(),
    ]
}

fn extend(v: &Value) -> bool {
    p::bool(v, "extend").unwrap_or(false)
}

fn apply(s: &mut Session, to: Pos, ext: bool) {
    if ext {
        s.sel.focus = to;
    } else {
        s.sel = Selection::caret(to);
    }
    s.pending = None;
}

fn mv(s: &mut Session, v: &Value, f: fn(&mut Session, &Pos) -> Pos) -> CmdResult {
    let ext = extend(v);
    s.goal_x = None;
    let from = s.sel.focus.clone();
    let to = f(s, &from);
    apply(s, to, ext);
    sel_result(s)
}

/// Does the caret's paragraph read right to left?
fn rtl_para(s: &Session) -> bool {
    s.doc.para_at(&s.sel.focus).is_some_and(|p| s.doc.styles.resolve_para(&p.props).bidi)
}

/// Is the caret's paragraph bidirectional (right to left, or with right-to-left text)? Arrow keys
/// move visually there.
fn bidi_para(s: &Session) -> bool {
    s.doc.para_at(&s.sel.focus).is_some_and(|p| kition_doc_doc::bidi::has_rtl(&p.text) || s.doc.styles.resolve_para(&p.props).bidi)
}

/// Left/Right: with a selection and no Shift, collapse to its edge.
fn lr(s: &mut Session, v: &Value, is_left: bool) -> CmdResult {
    if !extend(v) && !s.sel.is_collapsed() {
        let (a, b) = s.sel.ordered();
        // The left edge of a selection in right-to-left text is its logical end.
        let to_start = is_left != rtl_para(s);
        s.sel = Selection::caret(if to_start { a } else { b });
        s.goal_x = None;
        return sel_result(s);
    }
    if bidi_para(s) {
        return visual_lr(s, v, is_left);
    }
    mv(s, v, if is_left { left } else { right })
}

/// Left/Right in bidirectional text: one caret position to the left or right on the screen.
/// Past the line's visual end the caret goes to the neighbouring line in reading order (the
/// previous line when moving toward the paragraph's start edge).
fn visual_lr(s: &mut Session, v: &Value, is_left: bool) -> CmdResult {
    let ext = extend(v);
    s.goal_x = None;
    let from = s.sel.focus.clone();
    let l = s.layout();
    let to = match l.visual_step(&from, is_left, s.page_hint) {
        Some(kition_doc_layout::VisualStep::Moved(p)) => p,
        Some(kition_doc_layout::VisualStep::Edge { start, stop, last_line, rtl }) => {
            let at = |off| Pos { off, ..from.clone() };
            if is_left != rtl {
                match s.doc.para_at(&from) {
                    Some(para) if start > 0 => at(para.prev_boundary(start)),
                    _ => left(s, &at(0)),
                }
            } else if !last_line {
                at(stop)
            } else {
                let len = s.doc.para_at(&from).map_or(stop, |p| p.len());
                right(s, &at(len))
            }
        }
        None if is_left => left(s, &from),
        None => right(s, &from),
    };
    apply(s, to, ext);
    sel_result(s)
}

fn left(s: &mut Session, p: &Pos) -> Pos {
    if let Some(para) = s.doc.para_at(p)
        && p.off > 0
    {
        return Pos { off: para.prev_boundary(p.off), ..p.clone() };
    }
    match s.doc.prev_para(p.story, &p.path) {
        Some(q) => {
            let off = s.doc.para(p.story, &q).map(|x| x.len()).unwrap_or(0);
            Pos { story: p.story, path: q, off }
        }
        None => p.clone(),
    }
}

fn right(s: &mut Session, p: &Pos) -> Pos {
    if let Some(para) = s.doc.para_at(p)
        && p.off < para.len()
    {
        return Pos { off: para.next_boundary(p.off), ..p.clone() };
    }
    match s.doc.next_para(p.story, &p.path) {
        Some(q) => Pos { story: p.story, path: q, off: 0 },
        None => p.clone(),
    }
}

fn word_left(s: &mut Session, p: &Pos) -> Pos {
    match s.doc.para_at(p) {
        Some(para) if p.off > 0 => Pos { off: para.word_start(p.off), ..p.clone() },
        _ => left(s, p),
    }
}

fn word_right(s: &mut Session, p: &Pos) -> Pos {
    match s.doc.para_at(p) {
        Some(para) if p.off < para.len() => Pos { off: para.word_end(p.off), ..p.clone() },
        _ => right(s, p),
    }
}

fn para_up(s: &mut Session, p: &Pos) -> Pos {
    if p.off > 0 {
        return Pos { off: 0, ..p.clone() };
    }
    match s.doc.prev_para(p.story, &p.path) {
        Some(q) => Pos { story: p.story, path: q, off: 0 },
        None => p.clone(),
    }
}

fn para_down(s: &mut Session, p: &Pos) -> Pos {
    match s.doc.next_para(p.story, &p.path) {
        Some(q) => Pos { story: p.story, path: q, off: 0 },
        None => {
            let off = s.doc.para_at(p).map(|x| x.len()).unwrap_or(0);
            Pos { off, ..p.clone() }
        }
    }
}

fn vert(s: &mut Session, v: &Value, dir: i32) -> CmdResult {
    let ext = extend(v);
    let l = s.layout();
    let from = s.sel.focus.clone();
    let to = match l.vertical(&from, s.goal_x, dir, s.page_hint) {
        Some((p, gx)) => {
            s.goal_x = Some(gx);
            p
        }
        None => {
            // Top/bottom of the story: go to its start/end.
            if dir < 0 { s.doc.start_of(from.story) } else { s.doc.end_of(from.story) }
        }
    };
    if let Some(c) = l.caret_on(&to, s.page_hint) {
        s.page_hint = c.page;
    }
    let gx = s.goal_x;
    apply(s, to, ext);
    s.goal_x = gx;
    sel_result(s)
}

fn line(s: &mut Session, v: &Value, home: bool) -> CmdResult {
    let ext = extend(v);
    let l = s.layout();
    let from = s.sel.focus.clone();
    let to = match l.line_bounds(&from, s.page_hint) {
        Some((a, b)) => {
            if home {
                a
            } else {
                b
            }
        }
        None => from,
    };
    s.goal_x = None;
    apply(s, to, ext);
    sel_result(s)
}

/// Page Up and Page Down: by `distance` points (a screen) keeping the place across, through the
/// pages as they are stacked; without it, to the start of the first line of the page before or
/// after (the first page's, or the end of the last).
fn page(s: &mut Session, v: &Value, dir: i32) -> CmdResult {
    let ext = extend(v);
    let distance = match v.get("distance") {
        None => None,
        Some(_) => match p::f32(v, "distance") {
            Some(d) if d > 0.0 => Some(d.min(MAX_PAGE_DISTANCE)),
            _ => return Err(CmdError::Params("`distance` is a positive number of points".into())),
        },
    };
    let l = s.layout();
    let from = s.sel.focus.clone();
    let Some(c) = l.caret_on(&from, s.page_hint) else { return sel_result(s) };
    let last = l.pages.len().saturating_sub(1);
    let Some(distance) = distance else {
        let to = match (dir < 0, c.page) {
            (false, p) if p >= last => s.doc.end_of(from.story),
            (up, p) => {
                let np = if up { p.saturating_sub(1) } else { p + 1 };
                let first = l.hit(np, 0.0, 0.0, from.story);
                s.page_hint = np;
                match first {
                    Some(pos) => l.line_bounds(&pos, np).map_or(pos, |(start, _)| start),
                    None => from,
                }
            }
        };
        s.goal_x = None;
        apply(s, to, ext);
        return sel_result(s);
    };
    // Where the middle of the caret's line goes, on the pages stacked one after another; the line
    // nearest it there, on its page or the next one (a margin between them is no line).
    let goal = s.goal_x.unwrap_or(c.x);
    let mut tops = Vec::with_capacity(l.pages.len());
    let mut total = 0.0f32;
    for p in &l.pages {
        tops.push(total);
        total += p.h;
    }
    let top_of = |page: usize| tops.get(page).copied().unwrap_or(0.0);
    let start = top_of(c.page) + c.top + c.height / 2.0;
    let target = if dir < 0 { start - distance } else { start + distance };
    let to = if target > total {
        // Past the last page: the end of the text.
        s.doc.end_of(from.story)
    } else {
        let target = target.max(0.0);
        let page = tops.iter().rposition(|top| *top <= target).unwrap_or(0);
        let mut best: Option<(f32, Pos, usize)> = None;
        let neighbours = [(page.checked_sub(1), f32::MAX), (Some(page), target - top_of(page)), (Some(page + 1).filter(|p| *p <= last), 0.0)];
        for (p, y) in neighbours {
            let Some(p) = p else { continue };
            let y = y.min(l.pages.get(p).map_or(0.0, |pg| pg.h));
            let Some(pos) = l.hit(p, goal, y, from.story) else { continue };
            let Some(at) = l.caret_on(&pos, p) else { continue };
            let off = (top_of(at.page) + at.top + at.height / 2.0 - target).abs();
            if best.as_ref().is_none_or(|(b, ..)| off < *b) {
                best = Some((off, pos, at.page));
            }
        }
        match best {
            Some((_, pos, page)) => {
                s.page_hint = page;
                pos
            }
            None => from,
        }
    };
    apply(s, to, ext);
    s.goal_x = Some(goal);
    sel_result(s)
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let ext = extend(v);
    let to = if let Some(pv) = v.get("pos") {
        parse_pos(pv).ok_or_else(|| CmdError::Params("bad `pos`".into()))?
    } else if let (Some(pg), Some(x), Some(y)) = (p::u64(v, "page"), p::f32(v, "x"), p::f32(v, "y")) {
        let story = super::story_param(s, v);
        let l = s.layout();
        s.page_hint = pg as usize;
        l.hit(pg as usize, x, y, story).ok_or_else(|| CmdError::Params("no text on that page".into()))?
    } else {
        return Err(CmdError::Params("`pos` or `page`/`x`/`y` required".into()));
    };
    let to = s.doc.clamp(&to);
    s.goal_x = None;
    apply(s, to, ext);
    sel_result(s)
}

fn select_all(s: &mut Session, _: &Value) -> CmdResult {
    let st = s.sel.focus.story;
    s.sel = Selection { anchor: s.doc.start_of(st), focus: s.doc.end_of(st) };
    sel_result(s)
}

fn select_range(s: &mut Session, v: &Value) -> CmdResult {
    let a = v.get("anchor").and_then(parse_pos).ok_or_else(|| CmdError::Params("bad `anchor`".into()))?;
    let b = v.get("focus").and_then(parse_pos).ok_or_else(|| CmdError::Params("bad `focus`".into()))?;
    s.sel = Selection { anchor: s.doc.clamp(&a), focus: s.doc.clamp(&b) };
    sel_result(s)
}

fn select_word(s: &mut Session, _: &Value) -> CmdResult {
    let f = s.sel.focus.clone();
    if let Some(p) = s.doc.para_at(&f) {
        let (a, b) = p.word_at(f.off);
        s.sel = Selection { anchor: Pos { off: a, ..f.clone() }, focus: Pos { off: b, ..f } };
    }
    sel_result(s)
}

fn select_sentence(s: &mut Session, _: &Value) -> CmdResult {
    let f = s.sel.focus.clone();
    if let Some(p) = s.doc.para_at(&f) {
        let (a, b) = p.sentence_at(f.off);
        s.sel = Selection { anchor: Pos { off: a, ..f.clone() }, focus: Pos { off: b, ..f } };
    }
    sel_result(s)
}

fn select_paragraph(s: &mut Session, _: &Value) -> CmdResult {
    let f = s.sel.focus.clone();
    let len = s.doc.para_at(&f).map(|p| p.len()).unwrap_or(0);
    // Word includes the paragraph mark: extend to the start of the next paragraph when there is one.
    let end = match s.doc.next_para(f.story, &f.path) {
        Some(q) if q.parent() == f.path.parent() => Pos { story: f.story, path: q, off: 0 },
        _ => Pos { off: len, ..f.clone() },
    };
    s.sel = Selection { anchor: Pos { off: 0, ..f }, focus: end };
    sel_result(s)
}

fn select_line(s: &mut Session, _: &Value) -> CmdResult {
    let l = s.layout();
    if let Some((a, b)) = l.line_bounds(&s.sel.focus, s.page_hint) {
        s.sel = Selection { anchor: a, focus: b };
    }
    sel_result(s)
}

/// Select the n-th occurrence of `text` in the body (agents: "select the word X").
fn select_text(s: &mut Session, v: &Value) -> CmdResult {
    let needle = p::req_str(v, "text")?;
    let n = p::u64(v, "occurrence").unwrap_or(1).max(1) as usize;
    if needle.is_empty() {
        return Err(CmdError::Params("`text` is empty".into()));
    }
    let mut count = 0;
    let story = super::story_param(s, v);
    for path in s.doc.para_paths(story) {
        let Some(p) = s.doc.para(story, &path) else { continue };
        for (i, _) in p.text.match_indices(needle) {
            count += 1;
            if count == n {
                s.sel =
                    Selection { anchor: Pos { story, path: path.clone(), off: i }, focus: Pos { story, path: path.clone(), off: i + needle.len() } };
                return sel_result(s);
            }
        }
    }
    let _ = StoryRef::Body;
    Err(CmdError::Failed(format!("`{needle}` not found")))
}
