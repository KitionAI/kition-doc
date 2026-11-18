//! Migration from 0.3.x: a replica 0.3.x wrote (schema
//! version 1, each paragraph whole in its `para` register) reads as before, is rewritten into the
//! text form by a `system` transaction (not an undo step) before the first write, and two
//! replicas that migrate it concurrently converge; the migration raises the replica's minimum
//! schema version to 2 (rule M20), so a 0.3.x build refuses a migrated replica, its changes, and
//! a replica made in the text form.

use std::sync::Arc;

use kdm_model::{DocModel, DocSession, SCHEMA_VERSION, normalize, read_document};
use kition_doc_doc::props::CharProps;
use kition_doc_doc::{Document, Paragraph, StoryRef, para_block};
use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema};
use kition_kit::session::{KitModel, MemoryBlobs};
use serde_json::{Map, Value, json};

/// `model/schema.json` as 0.3.x had it: schema version 1, no text field, no `Inline` type.
fn schema_v1() -> Arc<Schema> {
    let mut v: Value = serde_json::from_str(include_str!("../model/schema.json")).unwrap();
    v["schemaVersion"] = json!(1);
    v["types"].as_object_mut().unwrap().remove("Inline");
    v["types"]["Block"]["fields"].as_object_mut().unwrap().remove("text");
    Arc::new(Schema::parse(&v.to_string()).unwrap())
}

fn paragraphs() -> Vec<Paragraph> {
    let bold = CharProps { bold: Some(true), ..Default::default() };
    let mut first = Paragraph::with_text("Hello ", CharProps::default());
    first.insert_text(first.len(), "bold world", &bold).unwrap();
    vec![first, Paragraph::with_text("Second paragraph", CharProps::default()), Paragraph::with_text("Third", CharProps::default())]
}

/// A snapshot of a replica as 0.3.x wrote it: the paragraphs in the register form.
fn old_snapshot() -> Vec<u8> {
    old_replica(&paragraphs()).snapshot().unwrap()
}

/// A replica as 0.3.x wrote it, holding `paragraphs` in the register form.
fn old_replica(paragraphs: &[Paragraph]) -> Replica {
    let root = ObjectId::from_string("document");
    let (replica, ()) = Replica::create_with(schema_v1(), DocId::new(), ActorId::random(), &Origin::system("create"), |tx| {
        tx.create_with_id("Document", &root, Map::new())?;
        for (i, p) in paragraphs.iter().enumerate() {
            let fields: Map<String, Value> =
                [("kind".to_owned(), json!("para")), ("para".to_owned(), serde_json::to_value(p).unwrap())].into_iter().collect();
            let block = tx.create("Block", fields)?;
            tx.list_insert(&root, "body", i, &block)?;
        }
        Ok(())
    })
    .unwrap();
    replica
}

fn texts(d: &Document) -> Vec<String> {
    d.body.iter().filter_map(|b| b.as_para()).map(|p| p.text.clone()).collect()
}

fn typed(s: &DocSession, block: usize, text: &str) -> Document {
    let mut d = s.document().clone();
    d.para_mut(StoryRef::Body, &kition_doc_doc::Path::top(block)).unwrap().insert_text(0, text, &CharProps::default()).unwrap();
    d
}

fn expected() -> Document {
    let mut d = Document::new();
    d.body = paragraphs().into_iter().map(para_block).collect();
    d
}

#[test]
fn an_old_replica_reads_as_before() {
    let s = DocSession::open_snapshot(&old_snapshot(), MemoryBlobs::default()).unwrap();
    assert!(s.model().ids().has_register_form());
    assert_eq!(serde_json::to_value(normalize(s.document())).unwrap(), serde_json::to_value(normalize(&expected())).unwrap());
}

/// Which body paragraphs are in the register form 0.3.x wrote.
fn register_form(s: &DocSession) -> Vec<bool> {
    s.model().ids().body.iter().map(|b| s.replica().get(&b.object, "para").unwrap().get("text").is_some()).collect()
}

#[test]
fn the_first_write_migrates_what_it_changes_as_a_system_change() {
    let mut s = DocSession::open_snapshot(&old_snapshot(), MemoryBlobs::default()).unwrap();
    let after = typed(&s, 1, "My ");
    assert_eq!(s.record(&Origin::person("user:a"), &after).unwrap(), 1, "the edit is one undo step");
    assert_eq!(s.steps().0, 1, "the migration is not an undo step");
    assert_eq!(register_form(&s), [true, false, true], "only the paragraph the write changed is rewritten");
    assert!(s.model().ids().has_register_form());
    assert_eq!(s.replica().min_schema_version(), SCHEMA_VERSION);
    let object = s.model().ids().body[1].object.clone();
    assert_eq!(s.replica().text(&object, "text").unwrap(), "My Second paragraph");
    assert_eq!(texts(&read_document(s.replica(), s.blobs()).unwrap().0), ["Hello bold world", "My Second paragraph", "Third"]);

    // The bold run of the first paragraph becomes a mark when it is written.
    let after = typed(&s, 0, "Oh ");
    s.record(&Origin::person("user:a"), &after).unwrap();
    assert_eq!(register_form(&s), [false, false, true]);
    let first = s.replica().text_delta(&s.model().ids().body[0].object, "text").unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(first[1].insert, "bold world");
    assert_eq!(first[1].attributes.get("bold"), Some(&json!(true)));

    // Undo takes back the edits, not the migrations.
    assert!(s.undo().unwrap().0);
    assert!(s.undo().unwrap().0);
    assert_eq!(texts(s.document()), ["Hello bold world", "Second paragraph", "Third"]);
    assert_eq!(register_form(&s), [false, false, true]);
    assert!(!s.undo().unwrap().0, "nothing else to undo");
}

#[test]
fn an_unchanged_paragraph_rebuilt_by_a_command_stays_in_the_register_form() {
    let mut s = DocSession::open_snapshot(&old_snapshot(), MemoryBlobs::default()).unwrap();
    let mut after = typed(&s, 1, "My ");
    // A command that rebuilt every block without changing paragraphs 0 and 2.
    after.body = after.body.iter().map(|b| std::sync::Arc::new((**b).clone())).collect();
    s.record(&Origin::person("user:a"), &after).unwrap();
    assert_eq!(register_form(&s), [true, false, true]);
    assert_eq!(texts(&read_document(s.replica(), s.blobs()).unwrap().0), ["Hello bold world", "My Second paragraph", "Third"]);
}

fn exchange(a: &mut DocSession, b: &mut DocSession) {
    for _ in 0..4 {
        b.import_changes(&a.export_changes(&b.version()).unwrap()).unwrap();
        a.import_changes(&b.export_changes(&a.version()).unwrap()).unwrap();
    }
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());
    assert_eq!(serde_json::to_value(normalize(a.document())).unwrap(), serde_json::to_value(normalize(b.document())).unwrap());
}

#[test]
fn two_replicas_migrating_concurrently_keep_both_sides_edits() {
    let snapshot = old_snapshot();
    let mut a = DocSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
    let mut b = DocSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
    let edit_a = typed(&a, 0, "A ");
    a.record(&Origin::person("user:a"), &edit_a).unwrap();
    let edit_b = typed(&b, 2, "B ");
    b.record(&Origin::person("user:b"), &edit_b).unwrap();
    exchange(&mut a, &mut b);
    assert_eq!(texts(a.document()), ["A Hello bold world", "Second paragraph", "B Third"], "each side's edit survives");
}

#[test]
fn a_paragraph_both_replicas_changed_while_migrating_is_kept_twice() {
    let snapshot = old_snapshot();
    let mut a = DocSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
    let mut b = DocSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
    let edit_a = typed(&a, 1, "A ");
    a.record(&Origin::person("user:a"), &edit_a).unwrap();
    let edit_b = typed(&b, 1, "B ");
    b.record(&Origin::person("user:b"), &edit_b).unwrap();
    exchange(&mut a, &mut b);
    let shown = texts(a.document());
    assert_eq!(shown.len(), 4, "{shown:?}");
    assert!(
        shown[1..3].contains(&"A Second paragraph".to_owned()) && shown[1..3].contains(&"B Second paragraph".to_owned()),
        "both versions: {shown:?}"
    );
    // Each version goes on merging by character.
    let edit = typed(&b, 1, "x");
    b.record(&Origin::person("user:b"), &edit).unwrap();
    exchange(&mut a, &mut b);
    assert!(texts(a.document())[1].starts_with('x'));
}

#[test]
fn undoing_a_write_that_migrated_keeps_what_another_person_typed_since() {
    let snapshot = old_snapshot();
    let mut a = DocSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
    let mut b = DocSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
    let edit_a = typed(&a, 1, "A ");
    a.record(&Origin::person("user:a"), &edit_a).unwrap();
    exchange(&mut a, &mut b);
    let mut edit_b = b.document().clone();
    let end = "A Second paragraph".len();
    edit_b.para_mut(StoryRef::Body, &kition_doc_doc::Path::top(1)).unwrap().insert_text(end, " by B", &CharProps::default()).unwrap();
    b.record(&Origin::person("user:b"), &edit_b).unwrap();
    exchange(&mut a, &mut b);
    assert!(a.undo().unwrap().0);
    exchange(&mut a, &mut b);
    assert_eq!(texts(b.document()), ["Hello bold world", "Second paragraph by B", "Third"], "A's typing undone, B's kept");
}

#[test]
fn an_old_build_refuses_a_replica_in_the_text_form() {
    let s = DocSession::create(&expected()).unwrap();
    let snapshot = s.snapshot().unwrap();
    let error = Replica::load(schema_v1(), &snapshot, ActorId::random()).err().unwrap();
    assert_eq!(error, KdmError::NewerSchema { found: SCHEMA_VERSION.into(), reads: 1 }, "this schema version is refused by a version 1 build");
    assert!(Replica::load(DocModel::schema(), &snapshot, ActorId::random()).is_ok());
}

#[test]
fn the_schema_version_constant_is_the_schema_files() {
    assert_eq!(SCHEMA_VERSION, DocModel::schema().schema_version);
}

#[test]
fn the_migration_raises_the_minimum_schema_version_so_an_old_build_refuses_it() {
    let snapshot = old_snapshot();
    let mut old = Replica::load(schema_v1(), &snapshot, ActorId::random()).unwrap();
    assert_eq!(old.min_schema_version(), 1, "0.3.x made it at version 1");
    let mut s = DocSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
    assert_eq!(s.replica().min_schema_version(), 1);
    assert!(s.migrate().unwrap());
    assert_eq!(s.replica().min_schema_version(), SCHEMA_VERSION, "raised in the migration's transaction");
    assert_eq!(s.steps().0, 0, "the raise is no undo step");

    // A 0.3.x build refuses the migrated replica, and its changes, rather than misread them.
    let error = Replica::load(schema_v1(), &s.snapshot().unwrap(), ActorId::random()).err().unwrap();
    assert_eq!(error, KdmError::NewerSchema { found: SCHEMA_VERSION.into(), reads: 1 });
    let before = old.state_json().unwrap();
    let error = old.import_changes(&s.export_changes(&old.version()).unwrap()).unwrap_err();
    assert_eq!(error, KdmError::NewerSchema { found: SCHEMA_VERSION.into(), reads: 1 });
    assert_eq!(old.state_json().unwrap(), before, "a refused import changes nothing");

    // Nothing left to migrate: a second call writes nothing.
    assert!(!s.migrate().unwrap());
}

#[test]
fn an_old_replica_without_paragraphs_is_raised_before_its_first_write() {
    let snapshot = old_replica(&[]).snapshot().unwrap();
    let mut s = DocSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap();
    assert!(!s.model().ids().has_register_form());
    let mut after = s.document().clone();
    after.body.push(para_block(Paragraph::with_text("New", CharProps::default())));
    assert_eq!(s.record(&Origin::person("user:a"), &after).unwrap(), 1);
    assert_eq!(s.replica().min_schema_version(), SCHEMA_VERSION);
    assert!(Replica::load(schema_v1(), &s.snapshot().unwrap(), ActorId::random()).is_err());
}

#[test]
fn a_session_refuses_changes_of_a_newer_schema_and_stays_usable() {
    let mut s = DocSession::create(&expected()).unwrap();
    let after = typed(&s, 0, "Edited ");
    assert_eq!(s.record(&Origin::person("user:a"), &after).unwrap(), 1);
    let mut newer_schema: Value = serde_json::from_str(include_str!("../model/schema.json")).unwrap();
    let next = SCHEMA_VERSION + 1;
    newer_schema["schemaVersion"] = json!(next);
    let mut newer = Replica::load(Arc::new(Schema::parse(&newer_schema.to_string()).unwrap()), &s.snapshot().unwrap(), ActorId::random()).unwrap();
    assert!(newer.transact(&Origin::system("migrate"), |tx| tx.raise_schema_version(next)).unwrap());

    let (version, hash) = (s.version(), s.state_hash().unwrap());
    let error = s.import_changes(&newer.export_changes(&version).unwrap()).unwrap_err();
    assert!(error.to_string().contains(&format!("needs schema version {next}")), "{error}");
    assert_eq!((s.version(), s.state_hash().unwrap()), (version, hash), "nothing imported");
    assert_eq!(s.steps(), (0, 0), "the refusal cleared the replica's history, and the session's with it");
    let after = typed(&s, 1, "Still ");
    assert_eq!(s.record(&Origin::person("user:a"), &after).unwrap(), 1, "still editable");
    assert!(s.undo().unwrap().0);
}
