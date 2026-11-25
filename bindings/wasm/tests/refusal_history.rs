//! C8-doc-refusals: coverage reconstructs a joined view only from complete history.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_doc_kit::DocDocument;
use kition_kit::{CoveredAsk, KitDocument};
use serde_json::{Value, json};

fn joined() -> (DocDocument, Value, CoveredAsk, Value) {
    let mut doc = DocDocument::create("join.docx", 612, 792).unwrap();
    for (i, text) in ["Alpha one two", "Beta three four", "Gamma five six", "Delta seven eight", "Epsilon nine ten"].iter().enumerate() {
        if i > 0 {
            doc.execute("text.newParagraph", json!({})).unwrap();
        }
        doc.execute("text.insert", json!({ "text": text })).unwrap();
    }
    doc.execute("text.newParagraph", json!({})).unwrap();
    doc.execute("table.insert", json!({ "rows": 2, "cols": 2 })).unwrap();
    let cell = json!({ "path": [5, 0, 1, 0], "story": "body", "off": 0 });
    doc.execute("select.range", json!({ "anchor": cell, "focus": cell })).unwrap();
    doc.execute("text.insert", json!({ "text": "Cell 01" })).unwrap();
    let id = |doc: &mut DocDocument, path: Value| {
        let p = doc.execute("file.paragraph", json!({ "path": path })).unwrap();
        json!(format!("p{:08X}", p["paragraph"]["props"]["paraId"].as_u64().unwrap()))
    };
    let paragraph = id(&mut doc, json!([2]));
    let target = id(&mut doc, json!([5, 0, 1, 0]));
    doc.snapshot().unwrap();
    let at = json!({ "block": 2, "off": 0 });
    doc.execute("select.range", json!({ "anchor": at, "focus": at })).unwrap();
    doc.execute("text.backspace", json!({})).unwrap();
    let ask = CoveredAsk {
        id: paragraph,
        before: json!({ "text": "Gamma five six" }).as_object().unwrap().clone(),
        created: false,
        deleted: false,
        unknown: vec![],
    };
    let params = json!({ "anchor": cell, "focus": { "path": [5, 0, 1, 0], "story": "body", "off": 4 } });
    (doc, params, ask, target)
}

#[test]
fn complete_join_history_recovers_the_previous_cell_without_editing() {
    let (mut doc, params, ask, target) = joined();
    let before = doc.snapshot().unwrap();
    let covered = doc.covered("select.range", &params, &[ask]).unwrap().unwrap();
    assert_eq!(covered.under, [target]);
    assert_eq!(doc.snapshot().unwrap(), before, "the reconstruction must not edit the live replica");
}

#[test]
fn unavailable_join_history_never_becomes_an_empty_hit_test() {
    let (mut doc, params, ask, _) = joined();
    let before = doc.snapshot().unwrap();
    for missing in [true, false] {
        let mut ask = ask.clone();
        if missing {
            ask.before.clear();
        } else {
            ask.unknown.push("text".into());
        }
        assert!(doc.covered("select.range", &params, &[ask]).unwrap().is_none());
        assert_eq!(doc.snapshot().unwrap(), before);
    }
}
