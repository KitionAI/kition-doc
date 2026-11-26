//! Drafts merged into a document through `KitSession<DocModel>` (data model 6.6, D1 to D9): lists
//! an agent's draft and the person make meanwhile are both kept and a restart shares its list's
//! definition; and the repair after a merge (`validate`) clears what the merged state left behind:
//! a note reference to a part the draft deleted, a comment anchor naming a comment that is gone, a
//! body left without paragraphs, and an object a draft listed twice.

mod common;

use common::{Session, SharedBlobs, assert_fresh, assert_no_dangling, assert_replica_clean, body_para, draft, edit, first, merge, person, rich};
use kition_doc_doc::props::{CharProps, NumRef};
use kition_doc_doc::{Document, ListKind, PartKind, Path, StoryRef};
use kition_kit::kdm::{ObjectId, Origin};
use serde_json::{Value, json};

fn agent() -> Origin {
    Origin::agent("agent:test")
}

fn assert_clean(s: &Session) {
    assert_no_dangling(&s.model().document);
    assert_replica_clean(s.replica());
    assert_fresh(s);
}

fn text(doc: &Document, block: usize) -> String {
    doc.para(StoryRef::Body, &Path::top(block)).map(|p| p.plain_text()).unwrap_or_default()
}

fn type_at_start(doc: &mut Document, block: usize, s: &str) {
    body_para(doc, block).insert_text(0, s, &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn lists_made_in_a_draft_and_meanwhile_are_both_kept() {
    let blobs = SharedBlobs::default();
    let mut a = first(&rich(), &blobs);
    let (mut d, base) = draft(&a, 2, &blobs);
    let make = |kind: ListKind, block: usize| {
        move |d: &mut Document| {
            let num = d.numbering.add_list(kind);
            body_para(d, block).props.numbering = Some(NumRef { num, level: 0 });
        }
    };
    // `rich` already has a list; each side makes another, with the same local ids.
    edit(&mut d, &blobs, make(ListKind::UpperLetter, 2));
    edit(&mut a, &blobs, make(ListKind::Bullet, 0));
    merge(&mut a, &d, &base, &agent());
    assert_clean(&a);
    let doc = &a.model().document;
    assert_eq!(doc.numbering.nums.len(), 3, "the old list and both new ones");
    assert_eq!(doc.numbering.abstracts.len(), 3, "each with its own definition");
    let level = |block: usize| {
        let n =
            doc.para(StoryRef::Body, &Path::top(block)).and_then(|p| p.props.numbering).unwrap_or_else(|| panic!("paragraph {block} is in a list"));
        doc.numbering.level(n.num, 0).map(|l| l.format).unwrap_or_else(|| panic!("list {} has a level 0", n.num))
    };
    assert_ne!(level(0), level(2), "each paragraph keeps the list its side made");
    // A list shared by two `Num`s (restart numbering) keeps one definition.
    let root = ObjectId::from_string("document");
    let entries = a.replica().map_entries(&root, "lists").unwrap_or_else(|e| panic!("{e}"));
    edit(&mut a, &blobs, |d| {
        let n = d.para(StoryRef::Body, &Path::top(0)).and_then(|p| p.props.numbering).map(|n| n.num).unwrap_or(0);
        let restarted = d.numbering.restart(n).unwrap_or_else(|| panic!("restart"));
        body_para(d, 1).props.numbering = Some(NumRef { num: restarted, level: 0 });
    });
    assert_clean(&a);
    let doc = &a.model().document;
    assert_eq!((doc.numbering.nums.len(), doc.numbering.abstracts.len()), (4, 3), "a restart shares its definition");
    let now = a.replica().map_entries(&root, "lists").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(now.len(), entries.len() + 1, "the restart is one new entry");
    // The other lists keep their definitions (their `order` may be renumbered: the merge left two
    // lists made at one place with one order).
    let held = |e: &[(String, Value)]| -> Vec<(String, Value, Value)> {
        e.iter().map(|(k, v)| (k.clone(), v["num"].clone(), v["abstract"].clone())).collect()
    };
    let now_held = held(&now);
    assert!(held(&entries).iter().all(|entry| now_held.contains(entry)), "the other lists keep their definitions");
    assert!(a.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    assert_eq!(a.replica().map_entries(&root, "lists").unwrap_or_else(|e| panic!("{e}")), entries, "undo takes the restart back");
    assert_eq!((a.model().document.numbering.nums.len(), a.model().document.numbering.abstracts.len()), (3, 3));
}

#[test]
fn a_note_reference_to_a_part_the_draft_deleted_goes() {
    let blobs = SharedBlobs::default();
    let mut a = first(&rich(), &blobs);
    let (mut d, base) = draft(&a, 2, &blobs);
    // The draft deletes the footnote's story only; the person types in the paragraph holding its
    // reference meanwhile: only the repair can drop the reference.
    edit(&mut d, &blobs, |doc| {
        let id = doc.parts.iter().find(|(_, p)| p.kind == PartKind::Footnote).map(|(id, _)| *id).unwrap_or_else(|| panic!("a footnote"));
        doc.parts.remove(&id);
    });
    edit(&mut a, &blobs, |doc| type_at_start(doc, 0, "typed "));
    merge(&mut a, &d, &base, &agent());
    assert_clean(&a);
    let p = a.model().document.para(StoryRef::Body, &Path::top(0)).cloned().unwrap_or_else(|| panic!("paragraph 0"));
    assert_eq!(p.plain_text(), "typed Paragraph 0", "the typing is kept");
    assert!(p.objects.is_empty(), "the reference is gone");
    assert!(
        a.replica()
            .objects_of_type("Inline")
            .iter()
            .all(|o| a.replica().get(o, "value").ok().and_then(|v| v.get("type").cloned()) != Some(json!("noteRef"))),
        "its inline object is deleted"
    );
}

#[test]
fn a_comment_anchor_naming_a_comment_that_is_gone_goes() {
    let blobs = SharedBlobs::default();
    let mut a = first(&rich(), &blobs);
    let (mut d, base) = draft(&a, 2, &blobs);
    // An agent's draft writes an inline object as a comment anchor naming no comment.
    let inline = d
        .replica()
        .objects_of_type("Inline")
        .into_iter()
        .find(|o| d.replica().get(o, "value").ok().and_then(|v| v.get("type").cloned()) == Some(json!("noteRef")))
        .unwrap_or_else(|| panic!("the note reference's inline object"));
    let target = inline.clone();
    d.record(&agent(), move |tx| Ok(tx.set(&target, "value", json!({"type": "commentStart", "id": "gone-comment"}))?))
        .unwrap_or_else(|e| panic!("record: {e}"));
    merge(&mut a, &d, &base, &agent());
    assert!(!a.replica().exists(&inline), "the repair deletes the anchor");
    assert_clean(&a);
    let p = a.model().document.para(StoryRef::Body, &Path::top(0)).cloned().unwrap_or_else(|| panic!("paragraph 0"));
    assert_eq!((p.plain_text(), p.objects.len()), ("Paragraph 0".to_owned(), 0));
}

#[test]
fn a_body_left_without_paragraphs_gets_an_empty_one() {
    let blobs = SharedBlobs::default();
    let mut doc = Document::new();
    doc.body = vec![common::para("First"), common::para("Second")];
    let mut a = first(&doc, &blobs);
    let (mut d, base) = draft(&a, 2, &blobs);
    edit(&mut d, &blobs, |doc| {
        doc.body.remove(0);
    });
    edit(&mut a, &blobs, |doc| {
        doc.body.remove(1);
    });
    merge(&mut a, &d, &base, &agent());
    assert_clean(&a);
    let root = ObjectId::from_string("document");
    let body = a.replica().list(&root, "body").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(body.len(), 1, "one paragraph: {body:?}");
    assert_eq!(a.replica().get(&body[0], "kind").ok(), Some(Value::from("para")));
    assert_eq!(a.model().document.body.len(), 1);
    assert_eq!(text(&a.model().document, 0), "");
}

#[test]
fn an_object_a_draft_lists_twice_is_listed_once() {
    let blobs = SharedBlobs::default();
    let mut a = first(&rich(), &blobs);
    let root = ObjectId::from_string("document");
    let body = a.replica().list(&root, "body").unwrap_or_else(|e| panic!("{e}"));
    let view = a.model().document.clone();
    let (mut d, base) = draft(&a, 2, &blobs);
    let (first_block, len) = (body[0].clone(), body.len());
    let at = root.clone();
    d.record(&agent(), move |tx| Ok(tx.list_insert(&at, "body", len, &first_block)?)).unwrap_or_else(|e| panic!("record: {e}"));
    // The repair takes the later entry back, so the merge changes nothing: it records no change
    // and no step (kition-sdk 6a57d68, regressions/issue-merge-cancelled-by-repair).
    let version = a.version();
    let bytes = d.export_changes(&base).unwrap_or_else(|error| panic!("export: {error}"));
    let (scope, _) = a.merge_changes(&bytes, &person("test")).unwrap_or_else(|error| panic!("merge: {error}"));
    assert_eq!(scope, None, "a merge that changes nothing is no step");
    assert_eq!(a.version(), version, "and logs no change");
    assert_eq!(a.replica().list(&root, "body").unwrap_or_else(|e| panic!("{e}")), body, "the later entry goes");
    common::assert_same("the view", &a.model().document, &view);
    assert_clean(&a);
}
