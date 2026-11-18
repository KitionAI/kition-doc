//! Text positions between the view and the replica, for presence (Kit UI protocol section 7e): a
//! person's caret travels as `{ object, field: "text", offset }`, the paragraph object whose text
//! field holds it and a character offset in that field, so every replica of the document places
//! it in its own view.
//!
//! A view paragraph shows its own field's visible runs, then the tails of anchors that show there
//! (`pieces`): text typed concurrently after a split point, and the paragraphs merged into it. So
//! a view offset is in one of those **segments**, each a range of one field: [`Ids::text_position`]
//! names that field and the offset in it, and [`Ids::view_position`] finds the segment holding a
//! field offset and the view offset there. Offsets count Unicode scalar values of the paragraph's
//! text as the replica reads it: comment anchors, which live beside the content, are not counted
//! (the engine's side leaves them out, as `comments` does).

use std::collections::HashMap;

use kition_kit::kdm::ObjectId;

use crate::ids::{BlockIds, Ids};
use crate::pieces::{Composition, Piece, char_len, compose, has_anchor, layout};
use crate::write::MAX_DEPTH;

/// One range of a field that a view paragraph shows: `len` characters from `start` of `object`'s
/// text field.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Segment {
    object: ObjectId,
    start: usize,
    len: usize,
}

/// Where piece `tail` of `c` starts in its field and how long it is: the visible runs first, then
/// each anchor (one character) and its tail.
fn piece_range(c: &Composition, tail: Option<usize>) -> (usize, usize) {
    let visible = char_len(&c.visible);
    let Some(t) = tail else { return (0, visible) };
    let mut start = visible;
    for (k, (_, runs)) in c.tails.iter().enumerate() {
        start = start.saturating_add(1);
        if k == t {
            return (start, char_len(runs));
        }
        start = start.saturating_add(char_len(runs));
    }
    (start, 0)
}

/// How one block list shows its paragraphs' fields (`pieces`). Without anchors anywhere in the
/// list (nearly always) every paragraph shows its own field and nothing is composed.
struct ListShape<'a> {
    /// The list's text paragraphs (merged ones included) by object, when some field has anchors.
    fields: HashMap<&'a ObjectId, &'a BlockIds>,
    pieces: HashMap<ObjectId, Vec<Piece>>,
}

fn is_text(b: &BlockIds) -> bool {
    b.table.is_none() && b.text.is_some()
}

impl<'a> ListShape<'a> {
    fn new(blocks: &'a [BlockIds]) -> Self {
        // The list as read: each block, then the paragraphs merged into it.
        let flat: Vec<&BlockIds> = blocks.iter().flat_map(|b| std::iter::once(b).chain(b.merged.iter())).collect();
        if !flat.iter().any(|b| b.text.as_ref().is_some_and(|t| has_anchor(t.runs()))) {
            return Self { fields: HashMap::new(), pieces: HashMap::new() };
        }
        let mut fields = HashMap::new();
        let mut anchored = HashMap::new();
        for b in flat.iter().filter(|b| is_text(b)) {
            fields.entry(&b.object).or_insert(*b);
            if let Some(t) = b.text.as_ref().filter(|t| has_anchor(t.runs())) {
                anchored.entry(b.object.clone()).or_insert_with(|| compose(t.runs()));
            }
        }
        let list: Vec<(ObjectId, bool)> = flat.iter().map(|b| (b.object.clone(), is_text(b))).collect();
        Self { pieces: layout(&list, &anchored).pieces, fields }
    }

    /// Whether some paragraph of the list shows more than its own field.
    fn composed(&self) -> bool {
        !self.pieces.is_empty()
    }

    /// The segments shown paragraph `b` of the list shows (none for a table, or a paragraph in
    /// the register form, whose object has no text field).
    fn segments(&self, b: &BlockIds) -> Vec<Segment> {
        if !is_text(b) {
            return Vec::new();
        }
        let range = |owner: &BlockIds, tail: Option<usize>| -> (usize, usize) {
            let runs = owner.text.as_ref().map(|t| t.runs()).unwrap_or_default();
            if tail.is_none() && !has_anchor(runs) {
                return (0, char_len(runs));
            }
            piece_range(&compose(runs), tail)
        };
        match self.pieces.get(&b.object) {
            None => {
                let (start, len) = range(b, None);
                vec![Segment { object: b.object.clone(), start, len }]
            }
            Some(pieces) => pieces
                .iter()
                .filter_map(|piece| {
                    let owner = self.fields.get(&piece.object)?;
                    let (start, len) = range(owner, piece.tail);
                    Some(Segment { object: piece.object.clone(), start, len })
                })
                .collect(),
        }
    }
}

impl Ids {
    /// The block list holding the block at `path` of the body (`None`) or part `id`, and the
    /// block's index in it.
    fn list_at(&self, part: Option<u32>, path: &[u32]) -> Option<(&[BlockIds], usize)> {
        let mut blocks = self.story_blocks(part)?;
        let mut rest = path;
        for _ in 0..=MAX_DEPTH {
            let (&b, tail) = rest.split_first()?;
            match tail {
                [] => return Some((blocks, b as usize)),
                [r, c, more @ ..] => {
                    let row = blocks.get(b as usize)?.table.as_ref()?.rows.get(*r as usize)?;
                    blocks = &row.cells.get(*c as usize)?.blocks;
                    rest = more;
                }
                _ => return None,
            }
        }
        None
    }

    /// The replica's text position of view offset `offset` (characters, comment anchors left
    /// out) in the paragraph at `path` of the body (`None`) or part `part`: the paragraph object
    /// whose text field holds it, and the offset in that field. At the boundary of two segments
    /// it is the end of the first (where typed text would go). `None` for no paragraph in the text
    /// form there.
    pub fn text_position(&self, part: Option<u32>, path: &[u32], offset: usize) -> Option<(ObjectId, usize)> {
        let (blocks, index) = self.list_at(part, path)?;
        let segments = ListShape::new(blocks).segments(blocks.get(index)?);
        let mut at = 0usize;
        for segment in &segments {
            if offset <= at.saturating_add(segment.len) {
                return Some((segment.object.clone(), segment.start.saturating_add(offset.saturating_sub(at))));
            }
            at = at.saturating_add(segment.len);
        }
        // Past the end (a view that has more than the replica read): the end of the last segment.
        segments.last().map(|s| (s.object.clone(), s.start.saturating_add(s.len)))
    }

    /// Where the replica's text position (`object`'s text field, offset `offset`) shows in the
    /// view: the paragraph's story (`None`: the body) and path, and the view offset (characters,
    /// comment anchors left out). An offset on an anchor, or past what the field shows, is the
    /// nearest place before it that shows. `None` for an object that is no shown paragraph.
    pub fn view_position(&self, object: &ObjectId, offset: usize) -> Option<(Option<u32>, Vec<u32>, usize)> {
        let (part, path) = self.find_block(object)?;
        let (&last, parent) = path.split_last()?;
        let (blocks, _) = self.list_at(part, &path)?;
        let shape = ListShape::new(blocks);
        // A field's text shows in its own paragraph, or (its tails) in others of its list.
        let shown: Vec<(usize, Vec<Segment>)> = if shape.composed() {
            blocks.iter().enumerate().map(|(index, b)| (index, shape.segments(b))).collect()
        } else {
            blocks.get(last as usize).map(|b| vec![(last as usize, shape.segments(b))]).unwrap_or_default()
        };
        // The nearest segment of `object` starting at or before `offset`, as (shown paragraph,
        // view offset of the segment's start, segment); the first of its segments otherwise.
        let mut best: Option<(usize, usize, &Segment)> = None;
        let mut first: Option<(usize, usize, &Segment)> = None;
        for (index, segments) in &shown {
            let index = *index;
            let mut at = 0usize;
            for segment in segments {
                if segment.object == *object {
                    first.get_or_insert((index, at, segment));
                    if segment.start <= offset && best.is_none_or(|(_, _, b)| segment.start >= b.start) {
                        best = Some((index, at, segment));
                    }
                }
                at = at.saturating_add(segment.len);
            }
        }
        let place = |index: usize| {
            let mut p = parent.to_vec();
            p.push(u32::try_from(index).unwrap_or(u32::MAX));
            p
        };
        match best.or(first) {
            Some((index, at, segment)) => {
                let inside = offset.saturating_sub(segment.start).min(segment.len);
                Some((part, place(index), at.saturating_add(inside)))
            }
            // A paragraph that shows nothing of its own field (none in the text form): its start.
            None => Some((part, place(last as usize), 0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kition_kit::kdm::TextRun;

    use super::*;
    use crate::ParaText;
    use crate::ids::{CellIds, RowIds, TableIds};
    use crate::pieces::{Mark, anchor};

    fn run(s: &str) -> TextRun {
        TextRun { insert: s.into(), attributes: BTreeMap::new() }
    }

    fn para(id: &str, runs: Vec<TextRun>) -> BlockIds {
        BlockIds::new(ObjectId::from_string(id), None, Some(ParaText::new(runs)))
    }

    fn o(id: &str) -> ObjectId {
        ObjectId::from_string(id)
    }

    #[test]
    fn a_plain_paragraph_maps_offsets_one_to_one() {
        let ids = Ids { body: vec![para("a", vec![run("Hello")]), para("b", vec![run("world")])], ..Ids::default() };
        assert_eq!(ids.text_position(None, &[1], 3), Some((o("b"), 3)));
        assert_eq!(ids.text_position(None, &[1], 9), Some((o("b"), 5)), "past the end: the end");
        assert_eq!(ids.view_position(&o("b"), 3), Some((None, vec![1], 3)));
        assert_eq!(ids.view_position(&o("b"), 40), Some((None, vec![1], 5)));
        assert_eq!(ids.view_position(&o("missing"), 0), None);
        assert_eq!(ids.text_position(None, &[2], 0), None);
    }

    #[test]
    fn a_tail_shown_elsewhere_maps_to_its_own_field() {
        // "Hello |" split into "Hello " and "world", "!" typed at the old end concurrently: it
        // shows at the end of "world", but lives in the first paragraph's field after the anchor.
        let ids = Ids {
            body: vec![para("a", vec![run("Hello "), anchor(&Mark::split(&o("n"))), run("!")]), para("n", vec![run("world")])],
            ..Ids::default()
        };
        assert_eq!(ids.text_position(None, &[0], 6), Some((o("a"), 6)));
        assert_eq!(ids.text_position(None, &[1], 2), Some((o("n"), 2)));
        assert_eq!(ids.text_position(None, &[1], 5), Some((o("n"), 5)), "between the segments: the end of the first");
        assert_eq!(ids.text_position(None, &[1], 6), Some((o("a"), 8)), "after the '!', past the anchor");
        assert_eq!(ids.view_position(&o("a"), 8), Some((None, vec![1], 6)));
        assert_eq!(ids.view_position(&o("a"), 7), Some((None, vec![1], 5)), "just after the anchor: before the '!'");
        assert_eq!(ids.view_position(&o("a"), 3), Some((None, vec![0], 3)));
        assert_eq!(ids.view_position(&o("n"), 1), Some((None, vec![1], 1)));
    }

    #[test]
    fn a_merged_paragraph_maps_into_its_holder() {
        // "ab" and "cd" merged by Backspace: "cd" keeps its object, shown at the end of "ab".
        let mut a = para("a", vec![run("ab")]);
        a.merged.push(para("b", vec![anchor(&Mark::merge(&o("a"))), run("cd")]));
        let ids = Ids { body: vec![a, para("c", vec![run("ef")])], ..Ids::default() };
        assert_eq!(ids.text_position(None, &[0], 1), Some((o("a"), 1)));
        assert_eq!(ids.text_position(None, &[0], 3), Some((o("b"), 2)));
        assert_eq!(ids.view_position(&o("b"), 2), Some((None, vec![0], 3)));
        assert_eq!(ids.view_position(&o("b"), 0), Some((None, vec![0], 2)), "on the anchor: the start of its text");
        assert_eq!(ids.view_position(&o("c"), 1), Some((None, vec![1], 1)));
    }

    #[test]
    fn positions_in_tables_and_parts_are_found() {
        let cell = CellIds { object: o("cell"), blocks: vec![para("x", vec![run("in a cell")])] };
        let table = TableIds { object: o("t"), rows: vec![RowIds { object: o("row"), cells: vec![cell] }] };
        let mut ids = Ids { body: vec![para("a", vec![run("before")]), BlockIds::new(o("t"), Some(table), None)], ..Ids::default() };
        ids.parts.insert(7, crate::ids::PartIds { object: o("header"), blocks: vec![para("h", vec![run("Header")])] });
        assert_eq!(ids.text_position(None, &[1, 0, 0, 0], 4), Some((o("x"), 4)));
        assert_eq!(ids.view_position(&o("x"), 4), Some((None, vec![1, 0, 0, 0], 4)));
        assert_eq!(ids.text_position(Some(7), &[0], 2), Some((o("h"), 2)));
        assert_eq!(ids.view_position(&o("h"), 2), Some((Some(7), vec![0], 2)));
        assert_eq!(ids.text_position(None, &[1], 0), None, "a table holds no text");
    }
}
