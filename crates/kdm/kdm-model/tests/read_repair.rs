//! The read repair (`repair`): what the reader drops or clears in a paragraph's text field reaches
//! the replica in a `system` transaction after an import and when a replica is opened, not only
//! on the paragraph's next write: marks naming a deleted revision. (Comment anchors are never in
//! a paragraph's field, rule M23: deleting a comment leaves nothing there to repair.)

mod common;

use common::{body_para, rich};
use kdm_model::{DocModel, DocSession, normalize, read_document};
use kition_doc_doc::Document;
use kition_doc_doc::props::CharProps;
use kition_kit::kdm::{ActorId, ObjectId, Origin, Replica};
use kition_kit::session::{KitModel, MemoryBlobs};

fn session(doc: &Document) -> DocSession {
    DocSession::create(doc).unwrap_or_else(|e| panic!("create: {e}"))
}

fn copy(of: &DocSession) -> DocSession {
    DocSession::open_snapshot(&of.snapshot().unwrap_or_else(|e| panic!("{e}")), of.blobs().clone()).unwrap_or_else(|e| panic!("open: {e}"))
}

fn exchange(a: &mut DocSession, b: &mut DocSession) {
    for _ in 0..4 {
        b.add_blobs(&a.blobs().clone()).unwrap_or_else(|e| panic!("{e}"));
        b.import_changes(&a.export_changes(&b.version()).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("{e}"));
        a.add_blobs(&b.blobs().clone()).unwrap_or_else(|e| panic!("{e}"));
        a.import_changes(&b.export_changes(&a.version()).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("{e}"));
    }
    assert_eq!(a.state_hash().ok(), b.state_hash().ok());
}

fn record(s: &mut DocSession, f: impl FnOnce(&mut Document)) {
    let mut after = s.document().clone();
    f(&mut after);
    s.record(&Origin::person("user:t"), &after).unwrap_or_else(|e| panic!("record: {e}"));
}

/// The view is exactly what a fresh read of the replica gives.
fn assert_fresh(s: &DocSession) {
    let fresh = read_document(s.replica(), s.blobs()).unwrap_or_else(|e| panic!("read: {e}")).0;
    assert_eq!(serde_json::to_value(normalize(&fresh)).ok(), serde_json::to_value(normalize(s.document())).ok());
}

#[test]
fn a_deleted_comments_anchors_leave_a_concurrently_edited_paragraphs_field() {
    let mut a = session(&rich());
    let mut b = copy(&a);
    let object = a.model().ids().body[1].object.clone();
    // The comment's anchors are in the view, not in the paragraph's field (rule M23).
    assert_eq!(a.document().para(kition_doc_doc::StoryRef::Body, &kition_doc_doc::Path::top(1)).map(|p| p.objects.len()), Some(2));
    assert_eq!(a.replica().text(&object, "text").unwrap_or_default().matches('\u{fffc}').count(), 0);
    // A deletes the comment (its anchors and its parts); B types into the paragraph holding them.
    record(&mut a, |d| {
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
    record(&mut b, |d| {
        body_para(d, 1).insert_text(3, "typed", &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
    });
    exchange(&mut a, &mut b);
    for s in [&a, &b] {
        let text = s.replica().text(&object, "text").unwrap_or_default();
        assert!(!text.contains('\u{fffc}'), "the replica no longer holds the anchors: {text:?}");
        assert!(text.contains("typed"), "{text:?}");
        assert_fresh(s);
    }
}

#[test]
fn opening_a_replica_clears_marks_naming_a_deleted_revision() {
    let a = session(&rich());
    let revision = a.model().ids().revisions[0].clone();
    let object = a.model().ids().body[2].object.clone();
    let names = |replica: &Replica| {
        replica
            .text_delta(&object, "text")
            .unwrap_or_default()
            .iter()
            .any(|r| r.attributes.get("ins").and_then(|v| v.as_str()) == Some(revision.as_str()))
    };
    assert!(names(a.replica()));
    // Another build (or a merge without a session) deleted the revision, leaving the marks.
    let mut stored =
        Replica::load(DocModel::schema(), &a.snapshot().unwrap_or_else(|e| panic!("{e}")), ActorId::random()).unwrap_or_else(|e| panic!("{e}"));
    stored.transact(&Origin::system("test"), |tx| tx.delete(&ObjectId::from_string(revision.as_str()))).unwrap_or_else(|e| panic!("{e}"));
    assert!(names(&stored));
    let blobs: MemoryBlobs = a.blobs().clone();
    let s = DocSession::open_snapshot(&stored.snapshot().unwrap_or_else(|e| panic!("{e}")), blobs).unwrap_or_else(|e| panic!("open: {e}"));
    assert!(!names(s.replica()), "the repair cleared the marks in the replica");
    assert_eq!(s.steps(), (0, 0), "the repair is no undo step");
    assert_fresh(&s);
}

/// A script's object-model batch may make a paragraph with only its text: no `para` register (or
/// `para: null`). It reads as a paragraph with the default properties and its text, and a person's
/// edit of it writes the text form.
#[test]
fn a_paragraph_without_a_register_keeps_its_text() {
    use kition_doc_doc::{Block, StoryRef};
    use kition_kit::kdm::batch::{self, ReplicaTarget};

    let mut a = session(&rich());
    let mut scripted =
        Replica::load(DocModel::schema(), &a.snapshot().unwrap_or_else(|e| panic!("{e}")), ActorId::random()).unwrap_or_else(|e| panic!("{e}"));
    let ops = serde_json::json!({ "ops": [
        { "op": "create", "type": "Block", "in": "root.body", "at": 0, "ref": "$n", "fields": { "kind": "para", "para": null } },
        { "op": "text.insert", "target": "$n", "field": "text", "index": 0, "text": "Scripted null" },
        { "op": "create", "type": "Block", "in": "root.body", "at": 1, "ref": "$m", "fields": { "kind": "para" } },
        { "op": "text.insert", "target": "$m", "field": "text", "index": 0, "text": "Scripted missing" },
    ] });
    batch::run(&mut ReplicaTarget(&mut scripted), &ops).unwrap_or_else(|e| panic!("batch: {e}"));
    a.import_changes(&scripted.export_changes(&a.version()).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("import: {e}"));

    let text = |d: &Document, block: usize| d.para(StoryRef::Body, &kition_doc_doc::Path::top(block)).map(|p| p.plain_text()).unwrap_or_default();
    for (block, expected) in [(0, "Scripted null"), (1, "Scripted missing")] {
        assert_eq!(text(a.document(), block), expected);
        let Block::Para(p) = a.document().body[block].as_ref() else { panic!("block {block} is a paragraph") };
        assert_eq!(p.props, Default::default(), "default paragraph properties");
        assert_eq!(p.runs.iter().map(|r| r.len).sum::<usize>(), expected.len(), "the runs cover the text");
    }
    assert_fresh(&a);

    // A person types into it: the text field is edited in place, and the register written.
    let object = a.model().ids().body[0].object.clone();
    record(&mut a, |d| {
        let p = body_para(d, 0);
        let end = p.len();
        p.insert_text(end, "!", &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
    });
    assert_eq!(a.model().ids().body[0].object, object, "the paragraph keeps its object");
    assert_eq!(a.replica().text(&object, "text").unwrap_or_default(), "Scripted null!");
    assert!(a.replica().get(&object, "para").is_ok_and(|v| v.is_object()), "the register is written");
    assert_fresh(&a);
}
