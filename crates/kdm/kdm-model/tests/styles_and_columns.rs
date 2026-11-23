//! The styles and a table's columns as containers (schema-freeze): each style is an entry of the
//! root's `styles` map keyed by its style id, the document defaults the `styleDefaults` register,
//! and each grid column a `Col` object of its table's `cols` list. Two replicas changing two
//! styles, adding two styles, changing the defaults and a style, or resizing two columns of one
//! table at once both keep their change, and the replicas end identical.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::{Session, SharedBlobs, assert_no_dangling, assert_replica_clean, assert_same, edit, first, para, peer, sync};
use kition_doc_doc::table::Table;
use kition_doc_doc::{Block, Document, Style};
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

fn assert_converged(a: &Session, b: &Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_same("views", &a.model().document, &b.model().document);
    assert_no_dangling(&a.model().document);
    assert_replica_clean(a.replica());
}

fn style_mut<'d>(d: &'d mut Document, id: &str) -> &'d mut Style {
    d.styles.styles.iter_mut().find(|s| s.id == id).unwrap_or_else(|| panic!("style {id}"))
}

fn style<'d>(d: &'d Document, id: &str) -> &'d Style {
    d.styles.styles.iter().find(|s| s.id == id).unwrap_or_else(|| panic!("style {id}"))
}

fn table(d: &Document) -> &Table {
    match d.body.get(1).map(|b| &**b) {
        Some(Block::Table(t)) => t,
        _ => panic!("the table"),
    }
}

fn table_mut(d: &mut Document) -> &mut Table {
    match d.body.get_mut(1).map(Arc::make_mut) {
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
fn changes_to_two_styles_are_both_kept() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| style_mut(d, "Heading1").chr.bold = Some(false));
    edit(&mut b, &blobs, |d| style_mut(d, "Normal").chr.italic = Some(true));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(style(d, "Heading1").chr.bold, Some(false), "A's change");
    assert_eq!(style(d, "Normal").chr.italic, Some(true), "B's change");
}

#[test]
fn two_styles_added_at_once_are_both_kept() {
    let (blobs, mut a, mut b) = pair();
    let add = |id: &str| {
        let id = id.to_owned();
        move |d: &mut Document| d.styles.styles.push(Style { id: id.clone(), name: id.clone(), ..Default::default() })
    };
    edit(&mut a, &blobs, add("FromA"));
    edit(&mut b, &blobs, add("FromB"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(d.styles.styles.len(), doc().styles.styles.len() + 2);
    let ids: Vec<&str> = d.styles.styles.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.ends_with(&["FromA", "FromB"]), "both after the built-in ones: {ids:?}");
}

#[test]
fn a_change_of_the_defaults_and_of_a_style_are_both_kept() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| d.styles.default_para.space_after = Some(4.0));
    edit(&mut b, &blobs, |d| style_mut(d, "Heading2").chr.size = Some(15.0));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(d.styles.default_para.space_after, Some(4.0), "A's change");
    assert_eq!(style(d, "Heading2").chr.size, Some(15.0), "B's change");
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

#[test]
fn resizing_two_columns_at_once_keeps_both() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| table_mut(d).grid[0] = 60.0);
    edit(&mut b, &blobs, |d| table_mut(d).grid[2] = 140.0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(table(&a.model().document).grid, vec![60.0, 100.0, 140.0], "A's and B's widths");
}

#[test]
fn a_column_added_and_another_resized_at_once_are_both_kept() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| table_mut(d).grid.push(50.0));
    edit(&mut b, &blobs, |d| table_mut(d).grid[1] = 120.0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(table(&a.model().document).grid, vec![100.0, 120.0, 100.0, 50.0]);
}
