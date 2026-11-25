//! The styles and a table's columns as containers (schema-freeze): each style is an entry of the
//! root's `styles` map keyed by its style id, the document defaults the `styleDefaults` register,
//! and each grid column a `Col` object of its table's `cols` list; deleting a style rewrites no
//! other entry.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::{Session, SharedBlobs, edit, first, para, peer};
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
