//! Paragraph text as a KDM text field with marks: undoing a merge keeps typing recorded after it,
//! an emptied paragraph merged away is deleted, inline comment anchors keep their place and are
//! not in the paragraph's field, revision marks name objects, and edits write characters, not the
//! paragraph.

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

/// The replica's text field of body block `block`.
fn delta(s: &Session, block: usize) -> Vec<TextRun> {
    let object = s.model().ids().body[block].object.clone();
    s.replica().text_delta(&object, "text").unwrap_or_else(|e| panic!("{e}"))
}

/// The view is exactly what a fresh read of the replica gives.
fn assert_fresh(s: &Session) {
    let fresh = kdm_model::read_document(s.replica(), s.blobs()).unwrap_or_else(|e| panic!("read: {e}")).0;
    assert_same("fresh read", &fresh, &s.model().document);
}

/// A joins paragraphs 0 and 1 (Backspace at 1's start).
fn merge_first_two(d: &mut Document) {
    let end = Pos::new(StoryRef::Body, Path::top(0), "Hello world".len());
    d.delete_range(&end, &Pos::new(StoryRef::Body, Path::top(1), 0)).unwrap_or_else(|e| panic!("merge: {e}"));
}

#[test]
fn undoing_a_merge_keeps_the_typing_made_meanwhile() {
    let (blobs, mut a, mut b) = pair();
    let second = a.model().ids().body[1].object.clone();
    edit(&mut a, &blobs, merge_first_two);
    assert!(a.replica().exists(&second), "the merged paragraph keeps its object");
    // B, having taken the merge, types `RR` inside what was paragraph 1; A takes that, then
    // undoes its merge.
    sync(&mut a, &mut b);
    edit(&mut b, &blobs, |d| insert(d, 0, "Hello worldSecond".len(), "RR"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_fresh(&a);
    assert_fresh(&b);
    assert_eq!(text(&a.model().document, 0), "Hello worldSecondRR paragraph");
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
fn comment_anchors_keep_their_place_through_typing_around_them() {
    let blobs = SharedBlobs::default();
    let mut a = first(&common::rich(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    // Paragraph 1 holds a comment's start and end anchors.
    let objects = para_of(&a.model().document, 1).objects.len();
    assert_eq!(objects, 2);
    edit(&mut a, &blobs, |d| insert(d, 1, 0, "a"));
    sync(&mut a, &mut b);
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
