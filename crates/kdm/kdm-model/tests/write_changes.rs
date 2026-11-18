//! `write_changes` writes only what differs: one paragraph's text (characters, not the
//! paragraph), one new block and its list
//! entry (fast, in a long document), one table row, a paragraph split keeping the first object.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use common::{SharedBlobs, para, person, rich};
use kdm_model::{Ids, write_changes};
use kition_doc_doc::table::{Cell, Row};
use kition_doc_doc::{Block, Document, Path, Pos, StoryRef};
use kition_kit::kdm::{ObjectId, Replica};
use serde_json::Value;

fn root() -> ObjectId {
    ObjectId::from_string("document")
}

fn write(replica: &mut Replica, before: &Document, after: &Document, ids: &mut Ids, blobs: &SharedBlobs) -> bool {
    let mut sink = blobs.clone();
    replica.transact(&person("t"), |tx| write_changes(tx, before, after, ids, &mut sink)).unwrap_or_else(|error| panic!("write: {error}"))
}

#[test]
fn editing_one_paragraph_writes_only_its_text() {
    let blobs = SharedBlobs::default();
    let before = rich();
    let (mut replica, mut ids) = common::new_replica(&before, &blobs);
    replica.take_changed();
    let object = ids.body[2].object.clone();
    let kind = replica.get(&object, "kind").unwrap_or(Value::Null);

    let mut after = before.clone();
    common::body_para(&mut after, 2).insert_text(0, "Edited ", &Default::default()).unwrap_or_else(|e| panic!("{e}"));
    assert!(write(&mut replica, &before, &after, &mut ids, &blobs));
    assert_eq!(replica.take_changed(), BTreeSet::from([object.clone()]), "only the edited paragraph's object changed");
    assert_eq!(replica.get(&object, "kind").unwrap_or(Value::Null), kind);
    assert!(replica.text(&object, "text").unwrap_or_default().starts_with("Edited Paragraph 2"));
    assert_eq!(ids.body[2].object, object, "the paragraph keeps its object");

    // Nothing more to write.
    assert!(!write(&mut replica, &after, &after, &mut ids, &blobs));
}

#[test]
fn inserting_a_paragraph_in_a_long_document_writes_one_block() {
    let blobs = SharedBlobs::default();
    let mut before = Document::new();
    before.body = (0..2000).map(|i| para(&format!("Paragraph number {i} of a long document."))).collect();
    let (mut replica, mut ids) = common::new_replica(&before, &blobs);
    replica.take_changed();
    let order = replica.list(&root(), "body").unwrap_or_default();

    let mut after = before.clone();
    after.body.insert(1000, para("A new paragraph"));
    let mut sink = blobs.clone();
    let mut elapsed = std::time::Duration::ZERO;
    replica
        .transact(&person("t"), |tx| {
            let start = std::time::Instant::now();
            let wrote = write_changes(tx, &before, &after, &mut ids, &mut sink);
            elapsed = start.elapsed();
            wrote
        })
        .unwrap_or_else(|error| panic!("write: {error}"));
    eprintln!("diff of a 2,000-paragraph body with one insert: {elapsed:?}");
    if !cfg!(debug_assertions) {
        assert!(elapsed < std::time::Duration::from_millis(20), "the diff took {elapsed:?}");
    }
    let changed = replica.take_changed();
    let new = ids.body[1000].object.clone();
    assert_eq!(changed, BTreeSet::from([root(), new.clone()]), "one new block and the body list");
    let mut expected = order;
    expected.insert(1000, new);
    assert_eq!(replica.list(&root(), "body").unwrap_or_default(), expected, "one list insert");
}

#[test]
fn inserting_a_table_row_adds_one_row() {
    let blobs = SharedBlobs::default();
    let before = rich();
    let (mut replica, mut ids) = common::new_replica(&before, &blobs);
    replica.take_changed();
    let table = ids.body[3].table.clone().unwrap_or_else(|| panic!("block 3 is a table"));
    let rows_before: Vec<ObjectId> = table.rows.iter().map(|r| r.object.clone()).collect();

    let mut after = before.clone();
    if let Block::Table(t) = Arc::make_mut(&mut after.body[3]) {
        t.rows.insert(1, Row { props: Default::default(), cells: (0..3).map(|_| Cell::with_text("new")).collect() });
    }
    assert!(write(&mut replica, &before, &after, &mut ids, &blobs));
    let changed = replica.take_changed();
    let table_after = ids.body[3].table.clone().unwrap_or_else(|| panic!("still a table"));
    let rows_after: Vec<ObjectId> = replica.list(&table.object, "rows").unwrap_or_default();
    assert_eq!(rows_after.len(), 4);
    assert_eq!(rows_after[0], rows_before[0]);
    assert_eq!(&rows_after[2..], &rows_before[1..], "the other rows keep their objects");
    let new_row = &table_after.rows[1];
    let mut expected = BTreeSet::from([table.object.clone(), new_row.object.clone()]);
    for cell in &new_row.cells {
        expected.insert(cell.object.clone());
        expected.extend(cell.blocks.iter().map(|b| b.object.clone()));
    }
    assert_eq!(changed, expected, "only the table's row list and the new row (its cells and paragraphs) changed");
}

#[test]
fn a_paragraph_split_keeps_the_first_half_object() {
    let blobs = SharedBlobs::default();
    let before = rich();
    let (mut replica, mut ids) = common::new_replica(&before, &blobs);
    replica.take_changed();
    let object = ids.body[2].object.clone();
    let next = ids.body[3].object.clone();

    let mut after = before.clone();
    after.split_paragraph(&Pos::new(StoryRef::Body, Path::top(2), 4)).unwrap_or_else(|e| panic!("split: {e}"));
    assert!(write(&mut replica, &before, &after, &mut ids, &blobs));
    assert_eq!(ids.body[2].object, object, "the first half keeps the paragraph's object");
    assert_ne!(ids.body[3].object, object);
    assert_eq!(ids.body[4].object, next);
    let changed = replica.take_changed();
    assert_eq!(changed, BTreeSet::from([root(), object.clone(), ids.body[3].object.clone()]), "the first half, the new second half, the body list");
    // The moved text left the first half's field; a split anchor naming the second half stays at
    // the split point, so text typed there concurrently follows the second half.
    assert_eq!(replica.text(&object, "text").unwrap_or_default(), "Para\u{fffc}", "the moved text left the first half's field");
    let runs = replica.text_delta(&object, "text").unwrap_or_default();
    let mark = runs.last().and_then(|r| r.attributes.get("object")).cloned();
    assert_eq!(mark, Some(serde_json::json!({ "splitTo": ids.body[3].object.as_str() })), "the anchor names the second half");
}

#[test]
fn moving_a_paragraph_is_a_list_move() {
    let blobs = SharedBlobs::default();
    let before = rich();
    let (mut replica, mut ids) = common::new_replica(&before, &blobs);
    replica.take_changed();
    let mut after = before.clone();
    let moved = after.body.remove(0);
    after.body.push(moved);
    assert!(write(&mut replica, &before, &after, &mut ids, &blobs));
    assert_eq!(replica.take_changed(), BTreeSet::from([root()]), "only the body list changed");
    let listed = replica.list(&root(), "body").unwrap_or_default();
    assert_eq!(listed, ids.body.iter().map(|b| b.object.clone()).collect::<Vec<_>>());
}
