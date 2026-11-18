//! Comment anchors for a host that keeps comment threads itself (Kit UI protocol 2.6 section 7k,
//! capability `comment-anchors`): the selection as an anchor the host stores with a new thread,
//! and where each stored anchor is in this view now, after any edits since.
//!
//! An anchor is `{ ids, range?, quote?, page?, label? }`. Here `ids` are the paragraph objects the
//! selection covers in the document's replica (the first holds `range.start`), `range` is in
//! replica text offsets of the first id's text field (comment anchors left out, as presence's
//! positions) and runs on past its end, a paragraph break counting one, and `quote` is the selected
//! text (paragraphs joined by a line break, objects and control characters left out, at most 500
//! characters). Without a replica the only id is [`BODY`] (or [`PART`] and the story's number) and
//! `range` counts in the story's text read the same way.
//!
//! The host keeps the anchor as it was made; the document moves on. So an anchor is found again
//! from all it says: the quote where the range says (the replica's ids follow their paragraph
//! through edits elsewhere), else the occurrence of the quote nearest there, else its start and
//! end around an edit inside it, else the range in its paragraph while the paragraph is there.

use kdm_model::kition_kit::kdm::ObjectId;
use kition_doc_doc::para::OBJ;
use kition_doc_doc::{Path, Pos, StoryRef};
use serde_json::{Value, json};

use super::presence::{part_of, to_bytes, to_chars};
use crate::{CmdResult, CommandSpec, Session};

/// The id of the body's text when the document has no replica.
pub const BODY: &str = "doc:body";
/// The prefix of a header's, footer's or text box's text when the document has no replica.
pub const PART: &str = "doc:part:";
/// At most this many ids, and characters of a quote (the protocol's limits).
const MAX_IDS: usize = 256;
const MAX_QUOTE: usize = 500;
/// At most this many anchors are found in one call.
const MAX_ANCHORS: usize = 1024;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("comment.anchor", "Selection as Comment Anchor", "Comments", anchor)
            .params(
                r#"{} → {ids: [object], range: {start, end}, quote, page, at: {page, x, y, height}} | null (the selection as a host's comment anchor, Kit UI protocol section 7k; page counts from 1, at is where the selection starts in this view in page points; null for a caret)"#,
            )
            .pure(),
        CommandSpec::new("comment.locate", "Locate Comment Anchors", "Comments", locate)
            .params(
                r#"{"anchors": [{ids: [object], range?: {start, end}, quote?, page?}]} → [{found, exact, text, page, at: {page, x, y, height} | null, rects: [{page, x, y, width, height}]}] (where each host comment anchor is in this view now, after edits: exact when its quote is still where its range says; page counts from 1)"#,
            )
            .pure(),
    ]
}

/// Whether `c` stays out of a quote: control characters but tabs and line breaks, and the
/// bidirectional formatting characters the protocol refuses (UI-V6).
fn left_out(c: char) -> bool {
    (c.is_control() && c != '\t' && c != '\n')
        || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{061c}')
}

/// A story's text as quotes read it: each character with its paragraph (an index into `paths`)
/// and byte offset there, a line break (at the paragraph's end) between paragraphs.
struct Flow {
    story: StoryRef,
    paths: Vec<Path>,
    chars: Vec<char>,
    at: Vec<(usize, usize)>,
}

impl Flow {
    fn new(s: &Session, story: StoryRef) -> Self {
        let paths = s.doc.para_paths(story);
        let mut chars = Vec::new();
        let mut at = Vec::new();
        let mut before = 0usize;
        for (i, path) in paths.iter().enumerate() {
            if i > 0 {
                // The break is the end of the paragraph before it.
                chars.push('\n');
                at.push((i - 1, before));
            }
            let Some(p) = s.doc.para(story, path) else {
                before = 0;
                continue;
            };
            before = p.text.len();
            for (off, c) in p.text.char_indices() {
                if c == OBJ || left_out(c) {
                    continue;
                }
                chars.push(c);
                at.push((i, off));
            }
        }
        // The end of the story, so a range may end there.
        let last = paths.len().saturating_sub(1);
        at.push((last, paths.last().and_then(|path| s.doc.para(story, path)).map_or(0, |p| p.text.len())));
        Self { story, paths, chars, at }
    }

    /// The flow index of byte offset `off` of paragraph `para`: its first character at or after it.
    fn index(&self, para: usize, off: usize) -> usize {
        self.at.partition_point(|&(p, o)| (p, o) < (para, off)).min(self.chars.len())
    }

    /// The position of flow index `i` (the story's end past the last character).
    fn pos(&self, s: &Session, i: usize) -> Option<Pos> {
        let &(para, off) = self.at.get(i).or(self.at.last())?;
        let path = self.paths.get(para)?.clone();
        Some(s.doc.clamp(&Pos { story: self.story, path, off }))
    }

    fn text(&self, from: usize, to: usize) -> String {
        self.chars.get(from.min(to)..to.min(self.chars.len())).map(|c| c.iter().collect()).unwrap_or_default()
    }

    /// Every flow index where `needle` starts.
    fn find_all(&self, needle: &[char]) -> Vec<usize> {
        let Some(first) = needle.first() else { return Vec::new() };
        if needle.len() > self.chars.len() {
            return Vec::new();
        }
        (0..=self.chars.len() - needle.len())
            .filter(|&i| self.chars.get(i) == Some(first) && self.chars.get(i..i + needle.len()) == Some(needle))
            .collect()
    }

    /// Characters from `from` to `to`, counted as a range counts them: replica characters (objects
    /// included, comment anchors not) and one per paragraph break.
    fn range_len(&self, s: &Session, from: &Pos, to: &Pos) -> usize {
        let Some(a) = self.paths.iter().position(|p| *p == from.path) else { return 0 };
        let Some(b) = self.paths.iter().position(|p| *p == to.path) else { return 0 };
        let para = |i: usize| self.paths.get(i).and_then(|path| s.doc.para(self.story, path));
        if a == b {
            return para(a).map_or(0, |p| to_chars(p, to.off).saturating_sub(to_chars(p, from.off)));
        }
        let mut n = para(a).map_or(0, |p| to_chars(p, p.text.len()).saturating_sub(to_chars(p, from.off)));
        for i in a + 1..b {
            n = n.saturating_add(1).saturating_add(para(i).map_or(0, |p| to_chars(p, p.text.len())));
        }
        n.saturating_add(1).saturating_add(para(b).map_or(0, |p| to_chars(p, to.off)))
    }
}

fn story_id(story: StoryRef) -> String {
    match story {
        StoryRef::Body => BODY.to_owned(),
        StoryRef::Part(id) => format!("{PART}{id}"),
    }
}

/// The story an id without a replica names.
fn story_of(id: &str) -> Option<StoryRef> {
    if id == BODY {
        return Some(StoryRef::Body);
    }
    id.strip_prefix(PART)?.parse().ok().map(StoryRef::Part)
}

/// Replica offset `chars` of `story`'s text read as one (paragraphs joined by a break) as a place.
fn story_offset(s: &Session, flow: &Flow, chars: usize) -> Option<(usize, usize)> {
    let mut left = chars;
    for (i, path) in flow.paths.iter().enumerate() {
        let p = s.doc.para(flow.story, path)?;
        let len = to_chars(p, p.text.len());
        if left <= len {
            return Some((i, to_bytes(p, left)));
        }
        left = left.saturating_sub(len + 1);
    }
    None
}

/// `comment.anchor`: the selection as an anchor for a host's new comment thread.
fn anchor(s: &mut Session, _: &Value) -> CmdResult {
    if s.sel.is_collapsed() {
        return Ok(Value::Null);
    }
    let (a, b) = s.sel.ordered();
    if a.story != b.story {
        return Ok(Value::Null);
    }
    let flow = Flow::new(s, a.story);
    let Some(pa) = flow.paths.iter().position(|p| *p == a.path) else { return Ok(Value::Null) };
    let Some(pb) = flow.paths.iter().position(|p| *p == b.path) else { return Ok(Value::Null) };
    let (fa, fb) = (flow.index(pa, a.off), flow.index(pb, b.off));
    let quote: String = flow.text(fa, fb).chars().take(MAX_QUOTE).collect();
    let length = flow.range_len(s, &a, &b);
    let mut ids: Vec<String> = Vec::new();
    let start;
    let replica = s.kdm().map(|kdm| kdm.model().ids());
    let first = replica.and_then(|ids| ids.text_position(part_of(a.story), &a.path.0, s.doc.para_at(&a).map_or(0, |p| to_chars(p, a.off))));
    if let (Some(model), Some((object, offset))) = (replica, first) {
        start = offset;
        ids.push(object.as_str().to_owned());
        for path in s.doc.paths_between(&a, &b) {
            if ids.len() >= MAX_IDS {
                break;
            }
            if let Some(object) = model.block_at(part_of(a.story), &path.0) {
                let object = object.as_str().to_owned();
                if !ids.contains(&object) {
                    ids.push(object);
                }
            }
        }
    } else {
        // No replica: the story's text read as one.
        let before = Pos { story: a.story, path: flow.paths.first().cloned().unwrap_or_else(|| Path::top(0)), off: 0 };
        start = flow.range_len(s, &before, &a);
        ids.push(story_id(a.story));
    }
    let layout = s.layout();
    let caret = layout.caret_on(&a, s.page_hint);
    let page = caret.as_ref().map_or(s.page_hint, |c| c.page);
    let at = caret.map(|c| json!({ "page": c.page, "x": c.x, "y": c.top, "height": c.height }));
    Ok(json!({
        "ids": ids,
        "range": { "start": start, "end": start.saturating_add(length) },
        "quote": quote,
        "page": page + 1,
        "at": at,
    }))
}

/// The stories' flows a call has read, each read once.
#[derive(Default)]
struct Flows(Vec<Flow>);

impl Flows {
    fn of(&mut self, s: &Session, story: StoryRef) -> &Flow {
        let i = match self.0.iter().position(|f| f.story == story) {
            Some(i) => i,
            None => {
                self.0.push(Flow::new(s, story));
                self.0.len() - 1
            }
        };
        &self.0[i]
    }
}

/// Where an anchor's range says it is now: its story, and where the range starts and ends in
/// that story's flow (`None` for an anchor whose ids name nothing shown here).
fn expected(s: &Session, flows: &mut Flows, value: &Value) -> Option<(StoryRef, Option<usize>, Option<usize>)> {
    let ids: Vec<&str> = value.get("ids").and_then(Value::as_array).map(|ids| ids.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let first = *ids.first()?;
    let range = value.get("range");
    let bound = |key: &str| range.and_then(|r| r.get(key)).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok());
    let (start, end) = (bound("start"), bound("end"));
    let length = match (start, end) {
        (Some(start), Some(end)) => Some(end.saturating_sub(start)),
        _ => None,
    };
    if let Some(story) = story_of(first) {
        let flow = flows.of(s, story);
        let at = story_offset(s, flow, start.unwrap_or(0)).map(|(p, off)| flow.index(p, off));
        return Some((story, at, at.map(|i| i.saturating_add(length.unwrap_or(0)))));
    }
    let model = s.kdm()?.model().ids();
    let (part, path, chars) = model.view_position(&ObjectId::from_string(first), start.unwrap_or(0))?;
    let story = part.map_or(StoryRef::Body, StoryRef::Part);
    let flow = flows.of(s, story);
    let para = flow.paths.iter().position(|p| p.0 == path)?;
    let p = s.doc.para(story, flow.paths.get(para)?)?;
    let at = flow.index(para, to_bytes(p, chars));
    let to = match length {
        Some(length) => at.saturating_add(length),
        // No range: the paragraphs the ids name, whole.
        None => {
            let last = ids
                .last()
                .and_then(|id| model.view_position(&ObjectId::from_string(*id), usize::MAX))
                .and_then(|(_, path, _)| flow.paths.iter().position(|p| p.0 == path))
                .filter(|&last| last >= para)
                .unwrap_or(para);
            let len = flow.paths.get(last).and_then(|path| s.doc.para(story, path)).map_or(0, |p| p.text.len());
            flow.index(last, len)
        }
    };
    let at = if start.is_some() { at } else { flow.index(para, 0) };
    Some((story, Some(at), Some(to.min(flow.chars.len()))))
}

/// The occurrence in `found` nearest `near` (the first without it).
fn nearest(found: &[usize], near: Option<usize>) -> Option<usize> {
    match near {
        Some(near) => found.iter().copied().min_by_key(|&i| i.abs_diff(near)),
        None => found.first().copied(),
    }
}

/// Where `value` (an anchor) is in this view: its story, and the start and end in the story's
/// flow, and whether its quote is still exactly where its range says.
fn resolve(s: &Session, flows: &mut Flows, value: &Value) -> Option<(StoryRef, usize, usize, bool)> {
    let quote: Vec<char> = value.get("quote").and_then(Value::as_str).unwrap_or("").chars().filter(|&c| !left_out(c) && c != OBJ).collect();
    let place = expected(s, flows, value);
    let story = place.map_or(StoryRef::Body, |(story, _, _)| story);
    let (at, to) = place.map_or((None, None), |(_, at, to)| (at, to));
    let flow = flows.of(s, story);
    let len = flow.chars.len();
    if quote.is_empty() {
        let at = at?.min(len);
        return Some((story, at, to.unwrap_or(at).clamp(at, len), true));
    }
    let n = quote.len();
    // Where the range says.
    if let Some(at) = at
        && flow.chars.get(at..at.saturating_add(n)) == Some(quote.as_slice())
    {
        return Some((story, at, at + n, true));
    }
    // The quote moved (edits before it, or offsets from a replica that has seen other edits):
    // its occurrence nearest where the range says.
    if let Some(i) = nearest(&flow.find_all(&quote), at) {
        return Some((story, i, i + n, false));
    }
    // Edited inside: its start and its end, around the edit, near where it was: the longest of
    // the quote's first and last characters (half of it, then fewer, at least 3) still there.
    if n >= 6 && at.is_some() {
        let mut head: Option<(usize, usize)> = None;
        let mut tail: Option<(usize, usize)> = None;
        let mut k = (n / 2).min(32);
        while k >= 3 && (head.is_none() || tail.is_none()) {
            if head.is_none() {
                head = nearest(&flow.find_all(quote.get(..k).unwrap_or_default()), at).map(|h| (h, k));
            }
            if tail.is_none() {
                let from = n - k;
                tail = nearest(&flow.find_all(quote.get(from..).unwrap_or_default()), at.map(|a| a + from)).map(|t| (t, k));
            }
            k /= 2;
        }
        match (head, tail) {
            (Some((h, _)), Some((t, l))) if t >= h && t + l <= h + 2 * n + 32 => return Some((story, h, t + l, false)),
            (Some((h, l)), _) => return Some((story, h, h + l, false)),
            (None, Some((t, l))) => return Some((story, t, t + l, false)),
            (None, None) => {}
        }
    }
    // The quoted text is gone, but its paragraph is there: where it was, to its paragraph's end.
    let at = at?.min(len);
    let to = to.unwrap_or(at).clamp(at, len);
    let end = flow.chars.get(at..to).and_then(|c| c.iter().position(|&c| c == '\n')).map_or(to, |p| at + p);
    Some((story, at, end, false))
}

/// `comment.locate`: where each host comment anchor is in this view now.
fn locate(s: &mut Session, v: &Value) -> CmdResult {
    let anchors = v.get("anchors").and_then(Value::as_array).cloned().unwrap_or_default();
    let layout = s.layout();
    let mut flows = Flows::default();
    let mut out = Vec::with_capacity(anchors.len().min(MAX_ANCHORS));
    let missing = || json!({ "found": false, "exact": false, "text": "", "page": null, "at": null, "rects": [] });
    for value in anchors.iter().take(MAX_ANCHORS) {
        let Some((story, from, to, exact)) = resolve(s, &mut flows, value) else {
            out.push(missing());
            continue;
        };
        let flow = flows.of(s, story);
        let (Some(a), Some(b)) = (flow.pos(s, from), flow.pos(s, to)) else {
            out.push(missing());
            continue;
        };
        let text = flow.text(from, to);
        let caret = layout.caret_on(&a, s.page_hint);
        let rects: Vec<Value> = if a == b {
            Vec::new()
        } else {
            layout
                .selection_rects(&s.doc, &a, &b, s.page_hint)
                .into_iter()
                .map(|(page, r)| json!({ "page": page, "x": r.x, "y": r.y, "width": r.w, "height": r.h }))
                .collect()
        };
        out.push(json!({
            "found": true,
            "exact": exact,
            "text": text,
            "page": caret.as_ref().map(|c| c.page + 1),
            "at": caret.map(|c| json!({ "page": c.page, "x": c.x, "y": c.top, "height": c.height })),
            "rects": rects,
        }));
    }
    Ok(Value::Array(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_leave_out_controls_and_bidi_formatting() {
        assert!(left_out('\u{0}'));
        assert!(left_out('\u{202e}'));
        assert!(left_out('\u{200f}'));
        assert!(!left_out('\t'));
        assert!(!left_out('\n'));
        assert!(!left_out('é'));
    }

    #[test]
    fn ids_without_a_replica_name_their_story() {
        assert_eq!(story_of(&story_id(StoryRef::Body)), Some(StoryRef::Body));
        assert_eq!(story_of(&story_id(StoryRef::Part(7))), Some(StoryRef::Part(7)));
        assert_eq!(story_of("doc:part:x"), None);
        assert_eq!(story_of("4f0c"), None);
    }
}
