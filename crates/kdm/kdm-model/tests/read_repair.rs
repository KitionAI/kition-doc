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
