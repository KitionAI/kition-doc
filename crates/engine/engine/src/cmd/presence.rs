//! Presence (Kit UI protocol section 7e): where the person's caret and selection are, as replica
//! text positions every replica shares (`{object, field: "text", offset}`: the paragraph object
//! whose text field holds it and a character offset there), and where other people's are in this
//! view. Engine positions (story, path, byte offset) are local to a view, so only replica
//! positions travel between people; `kdm-model`'s `caret` maps between the two.

use kition_doc_doc::para::OBJ;
use kition_doc_doc::{InlineObject, Paragraph, Path, Pos, StoryRef};
use serde_json::{Value, json};

use crate::{CmdResult, CommandSpec, Session};

/// The replica's field that holds a paragraph's text.
pub(crate) const TEXT_FIELD: &str = "text";
/// At most this many selected objects travel (the protocol's limit).
const MAX_SELECTION: usize = 256;
/// At most this many people's places are found in one call.
const MAX_PLACES: usize = 64;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("presence.caret", "Caret for Presence", "Navigation", caret)
            .params(
                r#"{} → {caret: {object, field, offset} | null, anchor: {object, field, offset} | null, selection: [object], page: n} (the caret, the selection's other end when something is selected, and the paragraphs it covers, as replica positions and object ids; null and [] before the document has a replica)"#,
            )
            .pure(),
        CommandSpec::new("presence.locate", "Locate Carets", "Navigation", locate)
            .params(
                r#"{"places": [{"caret": {object, field, offset}, "anchor"?: {object, field, offset}}]} → [{caret: {page, x, y, height} | null, rects: [{page, x, y, width, height}]}] (each person's caret and selection in this view, in page points; null and [] for what this view does not show)"#,
            )
            .pure(),
    ]
}

/// Whether the inline object a paragraph holds at the `k`-th U+FFFC is a comment anchor, which
/// the replica keeps beside the content and does not count in the text.
fn is_comment_anchor(p: &Paragraph, k: usize) -> bool {
    matches!(p.objects.get(k), Some(InlineObject::CommentStart { .. } | InlineObject::CommentEnd { .. }))
}

/// Byte offset `off` of `p`'s text as a replica offset: characters before it, comment anchors
/// left out.
pub(crate) fn to_chars(p: &Paragraph, off: usize) -> usize {
    let mut k = 0usize;
    let mut n = 0usize;
    for (at, c) in p.text.char_indices() {
        if at >= off {
            break;
        }
        if c == OBJ {
            k += 1;
            if is_comment_anchor(p, k - 1) {
                continue;
            }
        }
        n += 1;
    }
    n
}

/// Replica offset `chars` of `p`'s text (comment anchors left out) as a byte offset: right after
/// the `chars`-th counted character, at the end when there are fewer.
pub(crate) fn to_bytes(p: &Paragraph, chars: usize) -> usize {
    if chars == 0 {
        return 0;
    }
    let mut k = 0usize;
    let mut n = 0usize;
    for (at, c) in p.text.char_indices() {
        if c == OBJ {
            k += 1;
            if is_comment_anchor(p, k - 1) {
                continue;
            }
        }
        n += 1;
        if n == chars {
            return at + c.len_utf8();
        }
    }
    p.text.len()
}

pub(crate) fn part_of(story: StoryRef) -> Option<u32> {
    match story {
        StoryRef::Body => None,
        StoryRef::Part(id) => Some(id),
    }
}

/// `pos` as a replica text position, when the document has a replica that holds it.
fn shared(s: &Session, pos: &Pos) -> Option<Value> {
    let ids = s.kdm()?.model().ids();
    let p = s.doc.para_at(pos)?;
    let (object, offset) = ids.text_position(part_of(pos.story), &pos.path.0, to_chars(p, pos.off))?;
    Some(json!({ "object": object.as_str(), "field": TEXT_FIELD, "offset": offset }))
}

/// A replica text position as a place in this view, when it shows here.
fn local(s: &Session, value: &Value) -> Option<Pos> {
    let ids = s.kdm()?.model().ids();
    if value.get("field").and_then(Value::as_str).is_some_and(|field| field != TEXT_FIELD) {
        return None;
    }
    let object = value.get("object").and_then(Value::as_str)?;
    let offset = value.get("offset").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok())?;
    let (part, path, chars) = ids.view_position(&kdm_model::kition_kit::kdm::ObjectId::from_string(object), offset)?;
    let story = part.map_or(StoryRef::Body, StoryRef::Part);
    let path = Path(path);
    let p = s.doc.para(story, &path)?;
    Some(s.doc.clamp(&Pos { story, off: to_bytes(p, chars), path }))
}

/// `presence.caret`: the caret, the selection's other end, and the paragraphs the selection
/// covers, as the other people in the document read them.
fn caret(s: &mut Session, _: &Value) -> CmdResult {
    let page = s.layout().caret_on(&s.sel.focus, s.page_hint).map_or(s.page_hint, |c| c.page);
    let focus = shared(s, &s.sel.focus);
    let collapsed = s.sel.is_collapsed();
    let anchor = if collapsed { None } else { shared(s, &s.sel.anchor) };
    let mut selection: Vec<String> = Vec::new();
    if !collapsed && let Some(kdm) = s.kdm() {
        let ids = kdm.model().ids();
        let (a, b) = s.sel.ordered();
        if a.story == b.story {
            for path in s.doc.paths_between(&a, &b) {
                if selection.len() >= MAX_SELECTION {
                    break;
                }
                if let Some(object) = ids.block_at(part_of(a.story), &path.0) {
                    let object = object.as_str().to_owned();
                    if !selection.contains(&object) {
                        selection.push(object);
                    }
                }
            }
        }
    }
    Ok(json!({ "caret": focus, "anchor": anchor, "selection": selection, "page": page }))
}

/// `presence.locate`: where each person's caret and selection are in this view.
fn locate(s: &mut Session, v: &Value) -> CmdResult {
    let places = v.get("places").and_then(Value::as_array).cloned().unwrap_or_default();
    let layout = s.layout();
    let mut out = Vec::with_capacity(places.len().min(MAX_PLACES));
    for place in places.iter().take(MAX_PLACES) {
        let focus = place.get("caret").and_then(|c| local(s, c));
        let caret = focus
            .as_ref()
            .and_then(|pos| layout.caret_on(pos, s.page_hint))
            .map(|c| json!({ "page": c.page, "x": c.x, "y": c.top, "height": c.height }));
        let mut rects = Vec::new();
        if let (Some(focus), Some(anchor)) = (focus.as_ref(), place.get("anchor").and_then(|a| local(s, a)))
            && anchor.story == focus.story
            && anchor != *focus
        {
            for (page, r) in layout.selection_rects(&s.doc, &anchor, focus, s.page_hint) {
                rects.push(json!({ "page": page, "x": r.x, "y": r.y, "width": r.w, "height": r.h }));
            }
        }
        out.push(json!({ "caret": caret, "rects": rects }));
    }
    Ok(Value::Array(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn para(text: &str, objects: Vec<InlineObject>) -> Paragraph {
        Paragraph { text: text.to_owned(), objects, ..Paragraph::default() }
    }

    #[test]
    fn comment_anchors_are_not_counted() {
        let obj = OBJ.to_string();
        // "ab" [comment start] "cd" [comment end] "é"
        let p = para(&format!("ab{obj}cd{obj}é"), vec![InlineObject::CommentStart { id: 1 }, InlineObject::CommentEnd { id: 1 }]);
        let at = |chars: usize| to_bytes(&p, chars);
        assert_eq!(to_chars(&p, 2), 2);
        assert_eq!(to_chars(&p, 2 + obj.len()), 2, "after the start anchor: still 2");
        assert_eq!(to_chars(&p, p.text.len()), 5);
        assert_eq!(at(0), 0);
        assert_eq!(at(3), 2 + obj.len() + 1, "after the 'c'");
        assert_eq!(at(5), p.text.len());
        assert_eq!(at(9), p.text.len(), "past the end: the end");
        for chars in 0..=5 {
            assert_eq!(to_chars(&p, at(chars)), chars);
        }
    }
}
