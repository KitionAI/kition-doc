//! The styles and a table's columns as containers (schema-freeze): each style is an entry of the
//! root's `styles` map keyed by its style id, the document defaults the `styleDefaults` register,
//! and each grid column a `Col` object of its table's `cols` list; deleting a style rewrites no
//! other entry. A column resized or added is written as its own `Col` (the others keep their
//! objects), so an agent's draft resizing one column merges with the person resizing or adding
//! another meanwhile; each reads back, undoes and redoes exactly.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::{Session, SharedBlobs, assert_unchanged_writes_nothing, edit, first, merge_draft, para, peer};
use kition_doc_doc::table::Table;
use kition_doc_doc::{Block, Document};
use serde_json::Value;

fn doc() -> Document {
    let mut d = Document::new();
    d.body = vec![para("Before"), Arc::new(Block::Table(Table::new(2, 3, 300.0))), para("After")];
    d
}

fn pair() -> (SharedBlobs, Session, Session) {
    let blobs = SharedBlobs::default();
    let a = first(&doc(), &blobs);
    let b = peer(&a, 2, &blobs);
    (blobs, a, b)
}

fn table(d: &Document) -> &Table {
    match d.body.get(1).map(|b| &**b) {
        Some(Block::Table(t)) => t,
        _ => panic!("the table"),
    }
}

#[test]
fn the_replica_holds_each_style_and_each_column_on_its_own() {
    let (_, a, _) = pair();
    let replica = a.replica();
    let root = kition_kit::kdm::ObjectId::from_string("document");
    let styles = replica.map_entries(&root, "styles").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(styles.len(), doc().styles.styles.len(), "one entry per style");
    for (key, entry) in &styles {
        assert_eq!(entry["style"]["id"], Value::from(key.as_str()), "keyed by its style id");
    }
    let defaults = replica.get(&root, "styleDefaults").unwrap_or_else(|e| panic!("{e}"));
    assert!(defaults["defaultChr"].is_object() && defaults["defaultPara"].is_object(), "{defaults}");
    let tables = replica.objects_of_type("Table");
    assert_eq!(tables.len(), 1);
    let cols = replica.list(&tables[0], "cols").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cols.len(), 3, "one Col per grid column");
    for col in &cols {
        assert_eq!(replica.get(col, "width").ok().and_then(|w| w.as_f64()), Some(100.0));
    }
    // Read back as the engine's style sheet and grid.
    assert_eq!(a.model().document.styles, doc().styles);
    assert_eq!(table(&a.model().document).grid, vec![100.0; 3]);
}

#[test]
fn deleting_a_style_rewrites_no_other() {
    let (blobs, mut a, _) = pair();
    let root = kition_kit::kdm::ObjectId::from_string("document");
    let entries =
        |a: &Session| -> BTreeMap<String, Value> { a.replica().map_entries(&root, "styles").unwrap_or_else(|e| panic!("{e}")).into_iter().collect() };
    let mut before = entries(&a);
    edit(&mut a, &blobs, |d| d.styles.styles.retain(|s| s.id != "NoSpacing"));
    assert!(before.remove("NoSpacing").is_some());
    assert_eq!(entries(&a), before, "the other entries, orders included, stay as they were");
    assert!(!a.model().document.styles.styles.iter().any(|s| s.id == "NoSpacing"));
}

fn table_mut(d: &mut Document) -> &mut Table {
    match d.body.get_mut(1).map(Arc::make_mut) {
        Some(Block::Table(t)) => t,
        _ => panic!("the table"),
    }
}

/// The table's `Col` objects and their widths, as the replica holds them.
fn cols(a: &Session) -> Vec<(kition_kit::kdm::ObjectId, Option<f64>)> {
    let replica = a.replica();
    let tables = replica.objects_of_type("Table");
    assert_eq!(tables.len(), 1);
    let cols = replica.list(&tables[0], "cols").unwrap_or_else(|e| panic!("{e}"));
    cols.into_iter().map(|col| (col.clone(), replica.get(&col, "width").ok().and_then(|w| w.as_f64()))).collect()
}

#[test]
fn resizing_two_columns_at_once_keeps_both() {
    let (blobs, mut a, _) = pair();
    let before = cols(&a);
    merge_draft(&mut a, &blobs, |d| table_mut(d).grid[2] = 140.0, |d| table_mut(d).grid[0] = 60.0);
    assert_eq!(table(&a.model().document).grid, vec![60.0, 100.0, 140.0], "the person's and the draft's widths");
    let after = cols(&a);
    let ids = |c: &[(kition_kit::kdm::ObjectId, Option<f64>)]| c.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&after), ids(&before), "each column keeps its object");
    assert_eq!(after.iter().map(|(_, w)| *w).collect::<Vec<_>>(), [Some(60.0), Some(100.0), Some(140.0)]);
    assert_unchanged_writes_nothing(&mut a, &blobs);
}

#[test]
fn a_column_added_and_another_resized_at_once_are_both_kept() {
    let (blobs, mut a, _) = pair();
    let before = cols(&a);
    merge_draft(&mut a, &blobs, |d| table_mut(d).grid[1] = 120.0, |d| table_mut(d).grid.push(50.0));
    assert_eq!(table(&a.model().document).grid, vec![100.0, 120.0, 100.0, 50.0]);
    let after = cols(&a);
    assert_eq!(after.len(), 4);
    assert_eq!(after[..3].iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(), before.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>());
    assert!(!before.iter().any(|(id, _)| *id == after[3].0), "the added column is a new object");
    assert_eq!(after.iter().map(|(_, w)| *w).collect::<Vec<_>>(), [Some(100.0), Some(120.0), Some(100.0), Some(50.0)]);
    assert_unchanged_writes_nothing(&mut a, &blobs);
}
