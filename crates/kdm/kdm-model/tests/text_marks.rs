//! Paragraph text as a KDM text field with marks: two
//! replicas typing in one paragraph keep both texts, bold and italic over overlapping ranges both
//! apply, text typed concurrently at a formatted run's end converges, a split or merge
//! concurrent with typing keeps the typing outside the moved text, inline objects keep their
//! anchors, and edits write characters, not the paragraph.

mod common;

use common::{Session, SharedBlobs, assert_no_dangling, assert_replica_clean, assert_same, body_para, edit, first, peer, sync};
use kition_doc_doc::props::CharProps;
use kition_doc_doc::{Document, InlineObject, Paragraph, Path, Pos, StoryRef};
use kition_kit::kdm::TextRun;
use serde_json::Value;

fn doc() -> Document {
    let mut d = Document::new();
    d.body = vec![common::para("Hello world"), common::para("Second paragraph"), common::para("Third")];
    d
}

fn pair() -> (SharedBlobs, Session, Session) {
    let blobs = SharedBlobs::default();
    let a = first(&doc(), &blobs);
    let b = peer(&a, 2, &blobs);
    (blobs, a, b)
}

fn assert_converged(a: &Session, b: &Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_same("views", &a.model().document, &b.model().document);
    assert_no_dangling(&a.model().document);
    assert_replica_clean(a.replica());
}

fn para_of(d: &Document, block: usize) -> Paragraph {
    d.para(StoryRef::Body, &Path::top(block)).cloned().unwrap_or_else(|| panic!("paragraph {block}"))
}

fn text(d: &Document, block: usize) -> String {
    para_of(d, block).text
}

fn insert(d: &mut Document, block: usize, off: usize, s: &str) {
    body_para(d, block).insert_text(off, s, &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
}

/// The character properties at byte `off` of paragraph `block`.
fn props_at(d: &Document, block: usize, off: usize) -> CharProps {
    let p = para_of(d, block);
    let mut start = 0;
    for run in &p.runs {
        if off < start + run.len {
            return run.props.clone();
        }
        start += run.len;
    }
    panic!("offset {off} is past paragraph {block}'s runs");
}

fn format(d: &mut Document, block: usize, a: usize, b: usize, f: &dyn Fn(&mut CharProps)) {
    body_para(d, block).format(a, b, f).unwrap_or_else(|e| panic!("{e}"));
}

/// The replica's text field of body block `block`.
fn delta(s: &Session, block: usize) -> Vec<TextRun> {
    let object = s.model().ids().body[block].object.clone();
    s.replica().text_delta(&object, "text").unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn typing_at_different_places_of_one_paragraph_keeps_both() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| insert(d, 0, 0, "A: "));
    edit(&mut b, &blobs, |d| insert(d, 0, "Hello world".len(), " (b)"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(text(&a.model().document, 0), "A: Hello world (b)");
}

#[test]
fn typing_at_the_same_place_of_one_paragraph_keeps_both() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| insert(d, 0, 5, " there"));
    edit(&mut b, &blobs, |d| insert(d, 0, 5, ", dear"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let t = text(&a.model().document, 0);
    assert!(t == "Hello there, dear world" || t == "Hello, dear there world", "both texts survive, each whole: {t}");
}

#[test]
fn typing_and_deleting_in_one_paragraph_merge() {
    let (blobs, mut a, mut b) = pair();
    // A deletes "Second "; B types at the paragraph's end.
    edit(&mut a, &blobs, |d| body_para(d, 1).delete(0, "Second ".len()).unwrap_or_else(|e| panic!("{e}")));
    edit(&mut b, &blobs, |d| insert(d, 1, "Second paragraph".len(), " here"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(text(&a.model().document, 1), "paragraph here");
}

#[test]
fn bold_and_italic_on_overlapping_ranges_both_apply() {
    let (blobs, mut a, mut b) = pair();
    // "Hello world": A bolds "Hello" (0..5), B italicises "lo wo" (3..8).
    edit(&mut a, &blobs, |d| format(d, 0, 0, 5, &|c| c.bold = Some(true)));
    edit(&mut b, &blobs, |d| format(d, 0, 3, 8, &|c| c.italic = Some(true)));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    let at = |off| props_at(d, 0, off);
    assert_eq!((at(0).bold, at(0).italic), (Some(true), None), "bold only");
    assert_eq!((at(3).bold, at(3).italic), (Some(true), Some(true)), "both");
    assert_eq!((at(4).bold, at(4).italic), (Some(true), Some(true)), "both");
    assert_eq!((at(5).bold, at(5).italic), (None, Some(true)), "italic only");
    assert_eq!((at(8).bold, at(8).italic), (None, None), "neither");
}

#[test]
fn one_property_set_on_both_resolves_per_character() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| format(d, 0, 0, 8, &|c| c.size = Some(14.0)));
    edit(&mut b, &blobs, |d| format(d, 0, 3, 11, &|c| c.size = Some(18.0)));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(props_at(d, 0, 0).size, Some(14.0), "only A set it there");
    assert_eq!(props_at(d, 0, 10).size, Some(18.0), "only B set it there");
    let middle = props_at(d, 0, 5).size;
    assert!(middle == Some(14.0) || middle == Some(18.0), "one value wins in the overlap: {middle:?}");
}

#[test]
fn text_typed_concurrently_at_a_formatted_run_end_converges() {
    let (blobs, mut a, mut b) = pair();
    let len = "Hello world".len();
    // A makes the whole paragraph bold and a link; B, not having seen that, types at its end.
    edit(&mut a, &blobs, |d| {
        format(d, 0, 0, len, &|c| {
            c.bold = Some(true);
            c.link = Some("https://example.com".into());
        });
    });
    edit(&mut b, &blobs, |d| insert(d, 0, len, "!"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(text(d, 0), "Hello world!");
    for off in [0, len - 1] {
        assert_eq!((props_at(d, 0, off).bold, props_at(d, 0, off).link.as_deref()), (Some(true), Some("https://example.com")));
    }
    // Whether "!" took the marks is not the schema's `expand`: Loro orders an insertion and a
    // mark's end made concurrently at one position by their peers, for every expand (both marks
    // or neither). `expand` governs text typed at a mark already seen (kition-kdm `marks.rs`).
    let typed = props_at(d, 0, len);
    assert_eq!(typed.bold.is_some(), typed.link.is_some(), "both marks or neither: {typed:?}");
}

#[test]
fn a_split_concurrent_with_typing_keeps_the_typing_before_the_split() {
    let (blobs, mut a, mut b) = pair();
    let object = a.model().ids().body[1].object.clone();
    // A presses Enter after "Second"; B types at the paragraph's start.
    edit(&mut a, &blobs, |d| {
        d.split_paragraph(&Pos::new(StoryRef::Body, Path::top(1), "Second".len())).unwrap_or_else(|e| panic!("split: {e}"));
    });
    assert_eq!(a.model().ids().body[1].object, object, "the first half keeps its object and text field");
    edit(&mut b, &blobs, |d| insert(d, 1, 0, "My "));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(d.body.len(), 4);
    assert_eq!(text(d, 1), "My Second");
    assert_eq!(text(d, 2), " paragraph");
}

#[test]
fn text_typed_at_a_paragraphs_end_during_a_split_follows_the_second_half() {
    let (blobs, mut a, mut b) = pair();
    // A presses Enter after "Second"; B types at the paragraph's end.
    edit(&mut a, &blobs, |d| {
        d.split_paragraph(&Pos::new(StoryRef::Body, Path::top(1), "Second".len())).unwrap_or_else(|e| panic!("split: {e}"));
    });
    edit(&mut b, &blobs, |d| insert(d, 1, "Second paragraph".len(), " ends here"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(d.body.len(), 4);
    assert_eq!(text(d, 1), "Second");
    assert_eq!(text(d, 2), " paragraph ends here");
    // Text typed later at the first half's end stays there; at the second half's end, there.
    edit(&mut b, &blobs, |d| insert(d, 1, "Second".len(), "!"));
    edit(&mut a, &blobs, |d| insert(d, 2, " paragraph ends here".len(), "."));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(text(d, 1), "Second!");
    assert_eq!(text(d, 2), " paragraph ends here.");
    assert_fresh(&a);
    assert_fresh(&b);
}

#[test]
fn the_second_half_keeps_typing_and_objects_through_more_splits_and_merges() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| {
        d.split_paragraph(&Pos::new(StoryRef::Body, Path::top(0), "Hello".len())).unwrap_or_else(|e| panic!("split: {e}"));
    });
    // B types at the end and puts a comment anchor there too.
    edit(&mut b, &blobs, |d| {
        insert(d, 0, "Hello world".len(), "!");
        let end = para_of(d, 0).len();
        body_para(d, 0).insert_object(end, InlineObject::CommentEnd { id: 1 }, &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
        d.comments.insert(1, kition_doc_doc::Comment::default());
        let part = d.add_part(kition_doc_doc::PartKind::Comment, vec![common::para("Note")]);
        if let Some(c) = d.comments.get_mut(&1) {
            c.part = part;
        }
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(text(&a.model().document, 0), "Hello");
    assert_eq!(text(&a.model().document, 1), format!(" world!{}", kition_doc_doc::para::OBJ));
    assert_eq!(para_of(&a.model().document, 1).objects.len(), 1, "the anchor's object shows with the tail");
    // B types inside the shown tail and splits the second half; A merges the halves back.
    edit(&mut b, &blobs, |d| insert(d, 1, " world!".len(), "?"));
    edit(&mut b, &blobs, |d| {
        d.split_paragraph(&Pos::new(StoryRef::Body, Path::top(1), " wor".len())).unwrap_or_else(|e| panic!("split: {e}"));
    });
    edit(&mut a, &blobs, |d| {
        let end = Pos::new(StoryRef::Body, Path::top(0), "Hello".len());
        d.delete_range(&end, &Pos::new(StoryRef::Body, Path::top(1), 0)).unwrap_or_else(|e| panic!("merge: {e}"));
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_fresh(&a);
    let all: String = (0..a.model().document.body.len()).map(|i| text(&a.model().document, i)).collect();
    for piece in ["Hello", " wor", "ld!?"] {
        assert!(all.contains(piece), "{piece:?} is kept: {all:?}");
    }
    assert!(all.matches(kition_doc_doc::para::OBJ).count() >= 1, "the comment anchor is kept: {all:?}");
    // Deleting a paragraph that shows a tail deletes the tail with it.
    let last = a.model().document.body.len() - 1;
    edit(&mut a, &blobs, |d| {
        d.body.remove(last - 1);
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_fresh(&a);
}

#[test]
fn a_split_anchor_a_04_build_repairs_away_keeps_the_text() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| {
        d.split_paragraph(&Pos::new(StoryRef::Body, Path::top(1), "Second".len())).unwrap_or_else(|e| panic!("split: {e}"));
    });
    edit(&mut b, &blobs, |d| insert(d, 1, "Second paragraph".len(), "!"));
    sync(&mut a, &mut b);
    let object = a.model().ids().body[1].object.clone();
    assert_eq!(a.replica().text(&object, "text").unwrap_or_default(), format!("Second{}!", kition_doc_doc::para::OBJ));
    // A 0.4.x build reads the anchor as an anchor without its inline object: its read repair
    // deletes it, and the tail is text at the end of the first half.
    let mut old = kition_kit::kdm::Replica::load(
        <kdm_model::DocModel as kition_kit::session::KitModel>::schema(),
        &a.replica().snapshot().unwrap_or_else(|e| panic!("{e}")),
        common::actor(9),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    old.transact(&kition_kit::kdm::Origin::system("repair"), |tx| tx.text_delete(&object, "text", "Second".chars().count(), 1))
        .unwrap_or_else(|e| panic!("{e}"));
    a.import_changes(&old.export_changes(&a.version()).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("import: {e}"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(text(d, 1), "Second!");
    assert_eq!(text(d, 2), " paragraph");
    assert_fresh(&a);
}

/// The view is exactly what a fresh read of the replica gives.
fn assert_fresh(s: &Session) {
    let fresh = kdm_model::read_document(s.replica(), s.blobs()).unwrap_or_else(|e| panic!("read: {e}")).0;
    assert_same("fresh read", &fresh, &s.model().document);
}

#[test]
fn a_merge_concurrent_with_typing_keeps_the_typing_in_the_first_paragraph() {
    let (blobs, mut a, mut b) = pair();
    // A joins paragraphs 0 and 1 (Backspace at 1's start); B types at 0's start.
    edit(&mut a, &blobs, |d| {
        let end = Pos::new(StoryRef::Body, Path::top(0), "Hello world".len());
        d.delete_range(&end, &Pos::new(StoryRef::Body, Path::top(1), 0)).unwrap_or_else(|e| panic!("merge: {e}"));
    });
    edit(&mut b, &blobs, |d| insert(d, 0, 0, "Oh. "));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(d.body.len(), 2);
    assert_eq!(text(d, 0), "Oh. Hello worldSecond paragraph");
}

/// A joins paragraphs 0 and 1 (Backspace at 1's start).
fn merge_first_two(d: &mut Document) {
    let end = Pos::new(StoryRef::Body, Path::top(0), "Hello world".len());
    d.delete_range(&end, &Pos::new(StoryRef::Body, Path::top(1), 0)).unwrap_or_else(|e| panic!("merge: {e}"));
}

/// A merges paragraphs 0 and 1 while B types `RR` at byte `off` of paragraph 1: both apply.
fn merge_against_typing_at(off: usize) -> (SharedBlobs, Session, Session) {
    let (blobs, mut a, mut b) = pair();
    let second = a.model().ids().body[1].object.clone();
    edit(&mut a, &blobs, merge_first_two);
    assert!(a.replica().exists(&second), "the merged paragraph keeps its object");
    edit(&mut b, &blobs, |d| insert(d, 1, off, "RR"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_fresh(&a);
    assert_fresh(&b);
    let d = &a.model().document;
    assert_eq!(d.body.len(), 2, "one paragraph break less");
    let mut expected = String::from("Second paragraph");
    expected.insert_str(off, "RR");
    assert_eq!(text(d, 0), format!("Hello world{expected}"));
    assert_eq!(text(d, 1), "Third");
    (blobs, a, b)
}

#[test]
fn a_merge_keeps_typing_at_the_start_of_the_merged_paragraph() {
    let (blobs, mut a, mut b) = merge_against_typing_at(0);
    // Deleting the joined paragraph deletes the merged one's object with it.
    let merged: Vec<_> = a.model().ids().body[0].merged.iter().map(|m| m.object.clone()).collect();
    assert_eq!(merged.len(), 1);
    edit(&mut b, &blobs, |d| {
        d.body.remove(0);
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert!(merged.iter().all(|m| !a.replica().exists(m)));
    assert_eq!(a.model().document.body.len(), 1);
    assert_eq!(text(&a.model().document, 0), "Third");
}

#[test]
fn a_merge_keeps_typing_in_the_middle_of_the_merged_paragraph() {
    merge_against_typing_at("Second".len());
}

#[test]
fn a_merge_keeps_typing_at_the_end_of_the_merged_paragraph() {
    let (blobs, mut a, mut b) = merge_against_typing_at("Second paragraph".len());
    // Typing later at the joined paragraph's end, inside it, and merging the next one in too.
    let len = text(&a.model().document, 0).len();
    edit(&mut a, &blobs, |d| insert(d, 0, len, "."));
    edit(&mut b, &blobs, |d| insert(d, 0, "Hello world".len(), "|"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(text(&a.model().document, 0), "Hello world|Second paragraphRR.");
    edit(&mut b, &blobs, |d| {
        let end = Pos::new(StoryRef::Body, Path::top(0), len + 2);
        d.delete_range(&end, &Pos::new(StoryRef::Body, Path::top(1), 0)).unwrap_or_else(|e| panic!("merge: {e}"));
    });
    edit(&mut a, &blobs, |d| insert(d, 1, 0, ">"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_fresh(&a);
    let d = &a.model().document;
    assert_eq!(d.body.len(), 1);
    assert_eq!(text(d, 0), "Hello world|Second paragraphRR.>Third");
}

#[test]
fn a_merge_and_a_split_of_the_merged_paragraph_both_apply() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, merge_first_two);
    edit(&mut b, &blobs, |d| {
        d.split_paragraph(&Pos::new(StoryRef::Body, Path::top(1), "Second".len())).unwrap_or_else(|e| panic!("split: {e}"));
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_fresh(&a);
    let d = &a.model().document;
    let all: Vec<String> = (0..d.body.len()).map(|i| text(d, i)).collect();
    assert_eq!(all, ["Hello worldSecond", " paragraph", "Third"]);
}

#[test]
fn a_merge_and_a_split_of_the_first_paragraph_both_apply() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, merge_first_two);
    edit(&mut b, &blobs, |d| {
        d.split_paragraph(&Pos::new(StoryRef::Body, Path::top(0), "Hello".len())).unwrap_or_else(|e| panic!("split: {e}"));
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_fresh(&a);
    let d = &a.model().document;
    let all: Vec<String> = (0..d.body.len()).map(|i| text(d, i)).collect();
    assert_eq!(all, ["Hello", " worldSecond paragraph", "Third"]);
}

#[test]
fn a_merged_paragraph_whose_first_paragraph_is_deleted_shows_on_its_own() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, merge_first_two);
    edit(&mut b, &blobs, |d| {
        d.body.remove(0);
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_fresh(&a);
    let d = &a.model().document;
    let all: Vec<String> = (0..d.body.len()).map(|i| text(d, i)).collect();
    assert_eq!(all, ["Second paragraph", "Third"]);
}

#[test]
fn undoing_a_merge_keeps_the_typing_made_meanwhile() {
    let (blobs, mut a, mut b) = merge_against_typing_at("Second".len());
    assert!(a.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_fresh(&a);
    let d = &a.model().document;
    let all: Vec<String> = (0..d.body.len()).map(|i| text(d, i)).collect();
    assert_eq!(all, ["Hello world", "SecondRR paragraph", "Third"]);
    assert!(a.redo().unwrap_or_else(|e| panic!("redo: {e}")).0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(text(&a.model().document, 0), "Hello worldSecondRR paragraph");
    let _ = blobs;
}

#[test]
fn merging_an_empty_paragraph_deletes_it() {
    let (blobs, mut a, _b) = pair();
    edit(&mut a, &blobs, |d| {
        d.split_paragraph(&Pos::new(StoryRef::Body, Path::top(0), "Hello world".len())).unwrap_or_else(|e| panic!("split: {e}"));
    });
    let empty = a.model().ids().body[1].object.clone();
    edit(&mut a, &blobs, |d| {
        let end = Pos::new(StoryRef::Body, Path::top(0), "Hello world".len());
        d.delete_range(&end, &Pos::new(StoryRef::Body, Path::top(1), 0)).unwrap_or_else(|e| panic!("merge: {e}"));
    });
    assert!(!a.replica().exists(&empty));
    assert!(a.model().ids().body.iter().all(|b| b.merged.is_empty()));
    assert_fresh(&a);
}

#[test]
fn comment_anchors_keep_their_place_through_concurrent_typing() {
    let blobs = SharedBlobs::default();
    let mut a = first(&common::rich(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    // Paragraph 1 holds a comment's start and end anchors.
    let objects = para_of(&a.model().document, 1).objects.len();
    assert_eq!(objects, 2);
    edit(&mut a, &blobs, |d| insert(d, 1, 0, "a"));
    edit(&mut b, &blobs, |d| {
        let end = body_para(d, 1).len();
        insert(d, 1, end, "b");
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = para_of(&a.model().document, 1);
    assert_eq!(p.objects.len(), 2, "both anchors survive");
    assert!(matches!(p.objects[0], InlineObject::CommentStart { .. }));
    assert!(p.text.starts_with('a') && p.text.ends_with('b'), "{:?}", p.text);
    // The anchors are the comment thread's (rule M23), not the paragraph's: its field holds no
    // anchor character, and both replicas place them from the thread.
    let runs = delta(&a, 1);
    assert!(runs.iter().all(|r| !r.insert.contains(kition_doc_doc::para::OBJ) && !r.attributes.contains_key("object")), "{runs:?}");
    assert_eq!(para_of(&b.model().document, 1), p);
}

#[test]
fn a_deleted_comment_drops_its_anchors_from_a_concurrently_edited_paragraph() {
    let blobs = SharedBlobs::default();
    let mut a = first(&common::rich(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    edit(&mut a, &blobs, |d| {
        let p = body_para(d, 1);
        for off in p.object_offsets().into_iter().rev() {
            p.delete(off, off + kition_doc_doc::para::OBJ.len_utf8()).unwrap_or_else(|e| panic!("{e}"));
        }
        for id in [1, 2] {
            if let Some(c) = d.comments.remove(&id) {
                d.parts.remove(&c.part);
            }
        }
    });
    edit(&mut b, &blobs, |d| insert(d, 1, 3, "typed"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = para_of(&a.model().document, 1);
    assert!(p.objects.is_empty());
    assert!(p.text.contains("typed"));
}

#[test]
fn a_revision_mark_is_an_object_id() {
    let blobs = SharedBlobs::default();
    let a = first(&common::rich(), &blobs);
    // Paragraph 2's first characters are revision 0's insertion.
    let revision = a.model().ids().revisions[0].as_str().to_owned();
    let runs = delta(&a, 2);
    assert_eq!(runs[0].attributes.get("ins"), Some(&Value::String(revision)));
}

#[test]
fn typing_writes_characters_not_the_paragraph() {
    let (blobs, mut a, _) = pair();
    let object = a.model().ids().body[0].object.clone();
    let para = a.replica().get(&object, "para").unwrap_or(Value::Null);
    assert!(para.get("text").is_none(), "the register holds no text");
    edit(&mut a, &blobs, |d| insert(d, 0, 5, ","));
    assert_eq!(a.replica().text(&object, "text").unwrap_or_default(), "Hello, world");
    assert_eq!(a.replica().get(&object, "para").unwrap_or(Value::Null), para, "the register is unchanged");
}

#[test]
fn split_runs_read_back_merged_and_write_nothing() {
    let (blobs, mut a, _) = pair();
    // Two runs with the same properties: the text form keeps no boundary between them.
    edit(&mut a, &blobs, |d| {
        let p = body_para(d, 0);
        p.runs = vec![
            kition_doc_doc::para::Run { len: 5, props: CharProps::default() },
            kition_doc_doc::para::Run { len: 6, props: CharProps::default() },
        ];
    });
    assert_eq!(delta(&a, 0).len(), 1);
    let fresh = kdm_model::read_document(a.replica(), &blobs).unwrap_or_else(|e| panic!("{e}")).0;
    assert_same("merged runs", &a.model().document, &fresh);
}
