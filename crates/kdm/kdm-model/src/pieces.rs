//! What a paragraph's `text` field holds beyond its own characters: split anchors, so text typed
//! at the end of a paragraph while someone else splits it lands at the end of the second half.
//!
//! A paragraph split keeps the first half's object (`write`): the moved text is deleted from its
//! field and copied into a new paragraph. Characters another person types concurrently after the
//! split point were placed next to the deleted ones, so on their own they would surface at the end
//! of the first half. The split therefore first inserts a **split anchor** at the split point, a
//! U+FFFC whose `object` mark is `{"splitTo": <new paragraph's object>}` (an inline object's
//! anchor names its `Inline` object with a string instead), and only then deletes the moved text:
//! every character typed concurrently anywhere after the split point sits after the anchor, while
//! text typed later at the end of the first half goes before it.
//!
//! So a field is its **visible** runs, then for each anchor the runs after it up to the next one
//! (its **tail**). [`layout`] shows each tail at the end of the paragraph its anchor names, when
//! that paragraph is a text paragraph of the same block list, else (it was deleted or moved away)
//! where it is, at the end of the anchor's own paragraph. A view paragraph is thus a list of
//! [`Piece`]s, its own visible runs first; [`assign`] maps an edit of the whole view paragraph back
//! onto its pieces, so a write puts every character into the field it lives in.
//!
//! A 0.4.x build reads an anchor as an inline anchor without its object, which it drops, and its
//! tail as text at the end of the first half: nothing is lost (its read repair then deletes the
//! anchor, and the tail stays in the first half for everyone).
//!
//! A **paragraph merge** is the same idea the other way round. Backspace at the start of a
//! paragraph keeps the second paragraph's object and text where they are (so text typed into it
//! concurrently, anywhere, stays with it) and inserts a **merge anchor** at the start of its field:
//! a split anchor (`{"splitTo": <the first paragraph>, "merge": true}`) naming the paragraph it was
//! merged into. A paragraph whose field holds a merge anchor naming a text paragraph listed before
//! it is **merged** when the block shown right before it is a text paragraph (`layout`): it is not
//! shown on its own, and everything it shows (its visible runs, its merge anchors' tails, the tails
//! of split anchors naming it) shows at the end of that paragraph, its **holder**, after the
//! holder's own pieces and those of the paragraphs merged into it before. Its target gone (deleted
//! concurrently) or a table inserted before it concurrently, it shows on its own again: nothing is
//! lost. Undoing the merge deletes the anchor. A 0.4.x build reads a merge anchor as a split
//! anchor: the merged paragraph's text shows at the end of the first paragraph, and the merged
//! paragraph as an empty paragraph after it.

use std::collections::{BTreeMap, HashMap};

use kition_doc_doc::para::OBJ;
use kition_kit::kdm::{ObjectId, TextRun};
use kition_kit::session::diff_text;
use serde_json::{Map, Value};

use crate::text::{OBJECT_MARK, merged};

/// The key of a split anchor's `object` mark value.
pub(crate) const SPLIT_TO: &str = "splitTo";
/// The key that makes a split anchor a merge anchor.
pub(crate) const MERGE: &str = "merge";

/// What an anchor says: the paragraph it names, and whether it is a merge anchor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mark {
    pub target: ObjectId,
    pub merge: bool,
}

impl Mark {
    pub fn split(target: &ObjectId) -> Self {
        Self { target: target.clone(), merge: false }
    }

    pub fn merge(target: &ObjectId) -> Self {
        Self { target: target.clone(), merge: true }
    }
}

/// One part of a view paragraph: a field's visible runs (`tail: None`) or the tail of its
/// `tail`-th anchor.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Piece {
    pub object: ObjectId,
    pub tail: Option<usize>,
}

impl Piece {
    pub fn own(object: &ObjectId) -> Self {
        Self { object: object.clone(), tail: None }
    }
}

/// A paragraph field split at its anchors.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Composition {
    pub visible: Vec<TextRun>,
    /// Each anchor and its tail, in field order.
    pub tails: Vec<(Mark, Vec<TextRun>)>,
}

impl Composition {
    /// The field again: the visible runs, then each anchor and its tail.
    pub fn runs(&self) -> Vec<TextRun> {
        let mut out = self.visible.clone();
        for (mark, tail) in &self.tails {
            out.push(anchor(mark));
            out.extend(tail.iter().cloned());
        }
        merged(out)
    }

    /// The runs of one piece of this field.
    pub fn piece(&self, tail: Option<usize>) -> &[TextRun] {
        match tail {
            None => &self.visible,
            Some(t) => self.tails.get(t).map_or(&[][..], |(_, runs)| runs.as_slice()),
        }
    }
}

/// The anchor `mark` says.
pub(crate) fn anchor(mark: &Mark) -> TextRun {
    let mut value = Map::new();
    value.insert(SPLIT_TO.into(), Value::String(mark.target.as_str().to_owned()));
    if mark.merge {
        value.insert(MERGE.into(), Value::Bool(true));
    }
    let mut attributes = BTreeMap::new();
    attributes.insert(OBJECT_MARK.to_owned(), Value::Object(value));
    TextRun { insert: OBJ.to_string(), attributes }
}

/// The paragraph a run's `object` mark names as a split anchor's target, if it is one.
pub(crate) fn anchor_target(run: &TextRun) -> Option<&str> {
    run.attributes.get(OBJECT_MARK)?.get(SPLIT_TO)?.as_str()
}

/// Whether a split anchor's run is a merge anchor.
fn is_merge(run: &TextRun) -> bool {
    run.attributes.get(OBJECT_MARK).and_then(|v| v.get(MERGE)).and_then(Value::as_bool).unwrap_or(false)
}

/// Whether a field holds a split anchor.
pub(crate) fn has_anchor(runs: &[TextRun]) -> bool {
    runs.iter().any(|run| anchor_target(run).is_some())
}

/// Splits a field's runs at its anchors.
pub(crate) fn compose(runs: &[TextRun]) -> Composition {
    let mut c = Composition::default();
    for run in runs {
        let Some(target) = anchor_target(run) else {
            push(&mut c, run.clone());
            continue;
        };
        let mark = Mark { target: ObjectId::from_string(target), merge: is_merge(run) };
        // Each anchor character is an anchor; anything else in the run (no writer makes it) is
        // text, without the mark.
        let mut text = String::new();
        for ch in run.insert.chars() {
            if ch == OBJ {
                if !text.is_empty() {
                    push(&mut c, plain_part(run, std::mem::take(&mut text)));
                }
                c.tails.push((mark.clone(), Vec::new()));
            } else {
                text.push(ch);
            }
        }
        if !text.is_empty() {
            push(&mut c, plain_part(run, text));
        }
    }
    c
}

fn plain_part(run: &TextRun, insert: String) -> TextRun {
    let mut attributes = run.attributes.clone();
    attributes.remove(OBJECT_MARK);
    TextRun { insert, attributes }
}

fn push(c: &mut Composition, run: TextRun) {
    match c.tails.last_mut() {
        Some((_, tail)) => tail.push(run),
        None => c.visible.push(run),
    }
}

pub(crate) fn char_len(runs: &[TextRun]) -> usize {
    runs.iter().map(|r| r.insert.chars().count()).sum()
}

pub(crate) fn text_of(runs: &[TextRun]) -> String {
    runs.iter().map(|r| r.insert.as_str()).collect()
}

/// The runs covering characters `start..end` of `runs`.
pub(crate) fn slice(runs: &[TextRun], start: usize, end: usize) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut at = 0;
    for run in runs {
        let len = run.insert.chars().count();
        let (from, to) = (start.max(at), end.min(at + len));
        if from < to {
            let insert: String = run.insert.chars().skip(from - at).take(to - from).collect();
            out.push(TextRun { insert, attributes: run.attributes.clone() });
        }
        at += len;
    }
    out
}

/// How one block list shows: each shown text paragraph's pieces (only those showing more than
/// their own visible runs) and each merged paragraph's holder.
#[derive(Clone, Debug, Default)]
pub(crate) struct Layout {
    pub pieces: HashMap<ObjectId, Vec<Piece>>,
    pub holder: HashMap<ObjectId, ObjectId>,
}

impl Layout {
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty() && self.holder.is_empty()
    }
}

/// How one block list shows (`list`: its blocks in order, each with whether it is a paragraph in
/// the text form; `fields` holds the composed field of each that has anchors). A paragraph is
/// merged (see the module docs) when its field holds a merge anchor naming a text paragraph listed
/// before it and the block shown right before it is a text paragraph, its holder. Each text
/// paragraph's **section** is its visible runs, its merge anchors' tails, then the tails of split
/// anchors naming it (by the anchors' paragraphs in list order, its own whose target is not a
/// listed text paragraph among them). A shown paragraph shows its section, then the sections of
/// the paragraphs merged into it, in list order.
pub(crate) fn layout(list: &[(ObjectId, bool)], fields: &HashMap<ObjectId, Composition>) -> Layout {
    let mut out = Layout::default();
    if fields.values().all(|c| c.tails.is_empty()) {
        return out;
    }
    let mut index: HashMap<&ObjectId, usize> = HashMap::new();
    for (at, (object, text)) in list.iter().enumerate() {
        if *text {
            index.entry(object).or_insert(at);
        }
    }
    let mut last: Option<(&ObjectId, bool)> = None;
    for (at, (object, text)) in list.iter().enumerate() {
        let merges =
            *text && fields.get(object).is_some_and(|c| c.tails.iter().any(|(m, _)| m.merge && index.get(&m.target).is_some_and(|&t| t < at)));
        if merges && let Some((holder, true)) = last {
            out.holder.insert(object.clone(), holder.clone());
            continue;
        }
        last = Some((object, *text));
    }
    let mut sections: HashMap<&ObjectId, Vec<Piece>> = HashMap::new();
    for (object, text) in list {
        if !*text || sections.contains_key(object) {
            continue;
        }
        let mut section = vec![Piece::own(object)];
        if let Some(c) = fields.get(object) {
            section.extend(c.tails.iter().enumerate().filter(|(_, (m, _))| m.merge).map(|(t, _)| Piece { object: object.clone(), tail: Some(t) }));
        }
        sections.insert(object, section);
    }
    for (source, text) in list {
        let Some(c) = fields.get(source).filter(|_| *text) else { continue };
        for (t, (mark, _)) in c.tails.iter().enumerate() {
            if mark.merge {
                continue;
            }
            let holder = if mark.target != *source && index.contains_key(&mark.target) { &mark.target } else { source };
            if let Some(section) = sections.get_mut(holder) {
                section.push(Piece { object: source.clone(), tail: Some(t) });
            }
        }
    }
    let mut shown: HashMap<&ObjectId, Vec<Piece>> = HashMap::new();
    for (object, text) in list {
        if !*text || shown.contains_key(object) {
            continue;
        }
        let Some(section) = sections.get(object) else { continue };
        match out.holder.get(object) {
            Some(holder) => {
                if let Some(pieces) = shown.get_mut(holder) {
                    pieces.extend(section.iter().cloned());
                }
            }
            None => {
                shown.insert(object, section.clone());
            }
        }
    }
    for (object, pieces) in shown {
        if pieces.len() > 1 || pieces.first().is_some_and(|p| p.object != *object || p.tail.is_some()) {
            out.pieces.insert(object.clone(), pieces);
        }
    }
    out
}

/// Splits `new` (a view paragraph's new runs) into its pieces, whose old lengths are `lens` and
/// whose old text is `old`: kept characters stay in their piece, inserted ones join the piece of
/// the character before them (the first piece at the start). A piece in `gone` (its field is
/// being deleted) gives its characters to the closest piece before it that stays. Answers each
/// piece's new runs.
pub(crate) fn assign(old: &str, lens: &[usize], gone: &[bool], new: &[TextRun]) -> Vec<Vec<TextRun>> {
    let new_text = text_of(new);
    let mut owner_old: Vec<usize> = Vec::with_capacity(old.chars().count());
    for (k, &len) in lens.iter().enumerate() {
        let owner = (0..=k).rev().find(|&i| !gone.get(i).copied().unwrap_or(false)).unwrap_or(0);
        owner_old.extend(std::iter::repeat_n(owner, len));
    }
    let mut owner_new: Vec<usize> = Vec::with_capacity(new_text.chars().count());
    let mut at = 0;
    for edit in diff_text(old, &new_text) {
        owner_new.extend(owner_old.get(at..edit.index).unwrap_or(&[]).iter().copied());
        let last = owner_new.last().copied().unwrap_or(0);
        owner_new.extend(std::iter::repeat_n(last, edit.insert.chars().count()));
        at = edit.index + edit.delete;
    }
    owner_new.extend(owner_old.get(at..).unwrap_or(&[]).iter().copied());
    let mut out = vec![Vec::new(); lens.len().max(1)];
    let mut start = 0;
    for (k, slot) in out.iter_mut().enumerate() {
        let count = owner_new.iter().filter(|&&o| o == k).count();
        *slot = slice(new, start, start + count);
        start += count;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> TextRun {
        TextRun { insert: s.into(), attributes: Default::default() }
    }

    fn shown(pieces: &[Piece], fields: &HashMap<ObjectId, Composition>) -> String {
        pieces.iter().filter_map(|p| fields.get(&p.object).map(|c| text_of(c.piece(p.tail)))).collect()
    }

    #[test]
    fn a_field_composes_at_its_anchors_and_back() {
        let n = ObjectId::from_string("n");
        let runs = merged(vec![run("Hello "), anchor(&Mark::split(&n)), run("!")]);
        let c = compose(&runs);
        assert_eq!(text_of(&c.visible), "Hello ");
        assert_eq!(c.tails.len(), 1);
        assert_eq!(c.tails[0].0, Mark::split(&n));
        assert_eq!(text_of(&c.tails[0].1), "!");
        assert_eq!(c.runs(), runs);
        assert!(has_anchor(&runs));
        assert!(!has_anchor(&c.visible));
    }

    #[test]
    fn an_inline_objects_anchor_is_no_split_anchor() {
        let mut inline = run(&OBJ.to_string());
        inline.attributes.insert(OBJECT_MARK.to_owned(), Value::String("i1".into()));
        let runs = vec![run("a"), inline];
        assert!(!has_anchor(&runs));
        assert_eq!(compose(&runs).visible, runs);
    }

    fn texts(objects: &[&ObjectId]) -> Vec<(ObjectId, bool)> {
        objects.iter().map(|o| ((*o).clone(), true)).collect()
    }

    #[test]
    fn a_tail_shows_at_the_end_of_its_target() {
        let (o, n) = (ObjectId::from_string("o"), ObjectId::from_string("n"));
        let mut fields = HashMap::new();
        fields.insert(o.clone(), compose(&[run("Hello "), anchor(&Mark::split(&n)), run("!")]));
        fields.insert(n.clone(), compose(&[run("world")]));
        let pieces = layout(&texts(&[&o, &n]), &fields).pieces;
        assert!(!pieces.contains_key(&o));
        assert_eq!(shown(&pieces[&n], &fields), "world!");
        // Its target gone, the tail stays where it is.
        let pieces = layout(&texts(&[&o]), &fields).pieces;
        assert_eq!(shown(&pieces[&o], &fields), "Hello !");
    }

    #[test]
    fn a_merged_paragraph_shows_at_the_end_of_the_one_before_it() {
        let (a, b, c) = (ObjectId::from_string("a"), ObjectId::from_string("b"), ObjectId::from_string("c"));
        let mut fields = HashMap::new();
        fields.insert(a.clone(), compose(&[run("ab")]));
        // Typed at b's start concurrently with the merge, before the anchor: still in order.
        fields.insert(b.clone(), compose(&[run("RR"), anchor(&Mark::merge(&a)), run("cd")]));
        fields.insert(c.clone(), compose(&[run("ef")]));
        let l = layout(&texts(&[&a, &b, &c]), &fields);
        assert_eq!(l.holder.get(&b), Some(&a));
        assert!(!l.pieces.contains_key(&b) && !l.pieces.contains_key(&c));
        assert_eq!(shown(&l.pieces[&a], &fields), "abRRcd");
        // Its target gone, it shows on its own; a table before it, too.
        let l = layout(&texts(&[&b, &c]), &fields);
        assert!(l.holder.is_empty());
        let t = ObjectId::from_string("t");
        let l = layout(&[(a.clone(), true), (t, false), (b.clone(), true)], &fields);
        assert!(l.holder.is_empty());
        assert_eq!(shown(&l.pieces[&b], &fields), "RRcd");
        // A paragraph split off before it concurrently holds it instead.
        let n = ObjectId::from_string("n");
        fields.insert(a.clone(), compose(&[run("a"), anchor(&Mark::split(&n))]));
        fields.insert(n.clone(), compose(&[run("b")]));
        let l = layout(&texts(&[&a, &n, &b]), &fields);
        assert_eq!(l.holder.get(&b), Some(&n));
        assert_eq!(shown(&l.pieces[&n], &fields), "bRRcd");
    }

    #[test]
    fn a_split_merged_back_keeps_its_tail_at_the_end() {
        // "a|b" split, "X" typed at the end concurrently, then the halves merged back.
        let (a, b) = (ObjectId::from_string("a"), ObjectId::from_string("b"));
        let mut fields = HashMap::new();
        fields.insert(a.clone(), compose(&[run("a"), anchor(&Mark::split(&b)), run("X")]));
        fields.insert(b.clone(), compose(&[anchor(&Mark::merge(&a)), run("b")]));
        let l = layout(&texts(&[&a, &b]), &fields);
        assert_eq!(shown(&l.pieces[&a], &fields), "abX");
        let runs = merged(vec![anchor(&Mark::merge(&a)), run("b")]);
        assert_eq!(compose(&runs).runs(), runs);
        assert!(compose(&runs).tails[0].0.merge);
    }

    #[test]
    fn an_edit_goes_to_the_piece_it_touches() {
        let new = [run("world, and more!")];
        // "world" (own) + "!" (a tail): ", and more" typed before the "!" joins "world".
        let parts = assign("world!", &[5, 1], &[false, false], &new);
        assert_eq!(text_of(&parts[0]), "world, and more");
        assert_eq!(text_of(&parts[1]), "!");
        // Typed after the "!": the tail's.
        let parts = assign("world!", &[5, 1], &[false, false], &[run("world!?")]);
        assert_eq!((text_of(&parts[0]).as_str(), text_of(&parts[1]).as_str()), ("world", "!?"));
        // A gone piece's characters go to the piece before it.
        let parts = assign("world!", &[5, 1], &[false, true], &[run("world!")]);
        assert_eq!((text_of(&parts[0]).as_str(), text_of(&parts[1]).as_str()), ("world!", ""));
        // Typing at the start of an empty first piece stays in it.
        let parts = assign("!", &[0, 1], &[false, false], &[run("x!")]);
        assert_eq!((text_of(&parts[0]).as_str(), text_of(&parts[1]).as_str()), ("x", "!"));
    }
}
