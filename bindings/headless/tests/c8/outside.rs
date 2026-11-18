//! Item 6: scenarios with an independent writer that really rewrites the file between the
//! agent's observation and its write. The body is [`TEXTS`]: `Alpha one two`, `Beta three four`,
//! `Gamma five six`, `Delta seven eight`, `Epsilon nine ten`.
use super::*;

/// A selection of `[from, to)` in body block `block`, by index.
pub fn by_index(block: usize, from: usize, to: usize) -> Value {
    json!({ "command": "select.range", "params": { "anchor": { "block": block, "off": from }, "focus": { "block": block, "off": to } } })
}

/// A selection of `[from, to)` in paragraph `id`, by its id.
pub fn by_id(id: &Value, from: usize, to: usize) -> Value {
    json!({ "command": "select.range", "params": { "anchor": { "para": id, "off": from }, "focus": { "para": id, "off": to } } })
}

pub fn bold() -> Value {
    json!({ "command": "format.bold", "params": {} })
}

/// The colleague's steps to type `text` at `off` of block `block`.
pub fn type_at(block: usize, off: usize, text: &str) -> Vec<(&'static str, Value)> {
    vec![
        ("select.range", json!({ "anchor": { "block": block, "off": off }, "focus": { "block": block, "off": off } })),
        ("text.insert", json!({ "text": text })),
    ]
}

/// Whether the saved file's block `block` has bold text.
pub fn bold_in(f: &Fixture, block: usize) -> bool {
    let mut doc = f.disk();
    let answer = doc.execute("document.paragraph", json!({ "path": [block] })).unwrap();
    answer.to_string().contains("\"bold\":true")
}

#[tokio::test]
async fn an_unrelated_paragraph_changed_applies_and_others_names_only_it() {
    let f = Fixture::new(Guard::Premise);
    let ids = f.blocks().await;
    let basis = f.basis().await;
    f.colleague(&type_at(3, 0, "New "));
    let receipt = f.run(Some(basis), json!([by_index(1, 0, 4), bold()])).await.unwrap();
    // The other program's save stamps the root (`core.modified`, `core.revision`); of the
    // paragraphs, only the one it edited is named.
    let changed = receipt["others"]["changed"].as_array().unwrap();
    let blocks: Vec<&Value> = changed.iter().filter(|c| c["type"] == "Block").map(|c| &c["id"]).collect();
    assert_eq!(blocks, vec![&ids[3].0], "{receipt}");
    for root in changed.iter().filter(|c| c["id"] == "document") {
        assert!(root["fields"].as_array().unwrap().iter().all(|f| f.as_str().unwrap().starts_with("core.")), "{receipt}");
    }
    assert!(bold_in(&f, 1));
    assert_eq!(f.texts()[3], "New Delta seven eight");
    f.close().await;
}

#[tokio::test]
async fn the_same_text_changed_inside_the_range_is_refused_changed() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    let before = std::fs::read(f.root.join(FILE)).unwrap();
    f.colleague(&type_at(1, 2, "X"));
    let after = std::fs::read(f.root.join(FILE)).unwrap();
    let error = f.run(Some(basis), json!([by_index(1, 0, 4), bold()])).await.unwrap_err();
    conflict(&error, "changed");
    assert_ne!(before, after);
    assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), after, "a refused write saves nothing");
    f.close().await;
}

#[tokio::test]
async fn an_edit_before_the_range_relocates_it_and_one_after_it_applies() {
    for before in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        // `three` is [5, 10) of `Beta three four`.
        f.colleague(&type_at(1, if before { 0 } else { 15 }, "Z"));
        let result = f.run(Some(basis), json!([by_index(1, 5, 10), bold()])).await;
        if before {
            conflict(&result.unwrap_err(), "relocated");
        } else {
            result.unwrap();
            assert!(bold_in(&f, 1));
        }
        f.close().await;
    }
}

#[tokio::test]
async fn a_deleted_target_is_refused_by_index_and_by_id() {
    for id_form in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let ids = f.blocks().await;
        let basis = f.basis().await;
        // Delete paragraph 1 whole: from the end of paragraph 0 to the end of paragraph 1.
        f.colleague(&[
            ("select.range", json!({ "anchor": { "block": 0, "off": TEXTS[0].len() }, "focus": { "block": 1, "off": TEXTS[1].len() } })),
            ("text.delete", json!({})),
        ]);
        assert_eq!(f.texts().len(), TEXTS.len() - 1);
        let step = if id_form { by_id(&ids[1].0, 0, 4) } else { by_index(1, 0, 4) };
        let error = f.run(Some(basis), json!([step, bold()])).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "{error}");
        if id_form {
            conflict(&error, "deleted");
        }
        f.close().await;
    }
}

#[tokio::test]
async fn a_paragraph_inserted_before_the_index_relocates_and_after_it_applies() {
    for before in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        f.colleague(&[("para.insert", if before { json!({ "before": 0, "text": "Inserted" }) } else { json!({ "after": 3, "text": "Inserted" }) })]);
        let result = f.run(Some(basis), json!([by_index(2, 0, 5), bold()])).await;
        if before {
            conflict(&result.unwrap_err(), "relocated");
        } else {
            result.unwrap();
            assert!(bold_in(&f, 2));
        }
        f.close().await;
    }
}

#[tokio::test]
async fn a_moved_paragraph_refuses_a_write_by_index_and_takes_one_by_id() {
    for id_form in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let ids = f.blocks().await;
        let basis = f.basis().await;
        // A paragraph put above the target: its index moves, its id does not.
        f.colleague(&[("para.insert", json!({ "before": 1, "text": "Above" }))]);
        let step = if id_form { by_id(&ids[2].0, 0, 5) } else { by_index(2, 0, 5) };
        let result = f.run(Some(basis), json!([step, bold()])).await;
        if id_form {
            result.unwrap();
            assert!(bold_in(&f, 3), "the paragraph that was block 2 is block 3 now");
        } else {
            conflict(&result.unwrap_err(), "relocated");
        }
        f.close().await;
    }
}

#[tokio::test]
async fn a_cell_addressed_by_path_follows_its_table() {
    // The table holding the cell is the parent: a paragraph put above it moves the path.
    for moved in [true, false] {
        let mut doc = document();
        doc.execute("select.range", json!({ "anchor": { "block": 4, "off": 0 }, "focus": { "block": 4, "off": 0 } })).unwrap();
        doc.execute("insert.table", json!({ "rows": 2, "cols": 2 })).unwrap();
        doc.execute(
            "select.range",
            json!({ "anchor": { "story": "body", "path": [4, 0, 1, 0], "off": 0 }, "focus": { "story": "body", "path": [4, 0, 1, 0], "off": 0 } }),
        )
        .unwrap();
        doc.execute("text.insert", json!({ "text": "Cell text" })).unwrap();
        let f = Fixture::with(Guard::Premise, &doc.save(None).unwrap());
        let basis = f.basis().await;
        f.colleague(&[("para.insert", if moved { json!({ "before": 0, "text": "Above" }) } else { json!({ "after": 5, "text": "Below" }) })]);
        let cell = |off: usize| json!({ "story": "body", "path": [4, 0, 1, 0], "off": off });
        let result = f.run(Some(basis), json!([{ "command": "select.range", "params": { "anchor": cell(0), "focus": cell(4) } }, bold()])).await;
        if moved {
            // The path now runs into a paragraph: what it named at basis cannot be told.
            assert_eq!(result.unwrap_err()["error"]["code"], "conflict");
        } else {
            result.unwrap();
        }
        f.close().await;
    }
}

#[tokio::test]
async fn a_style_renamed_away_refuses_a_write_by_its_name() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("styles.modify", json!({ "style": "Heading1", "name": "Chapter" }))]);
    let error = f.run(Some(basis), json!([by_index(1, 0, 0), { "command": "para.style", "params": { "style": "Heading1" } }])).await.unwrap_err();
    conflict(&error, "changed");
    f.close().await;
}

#[tokio::test]
async fn epoch_refuses_an_unrelated_change_premise_and_off_apply() {
    for (guard, applies) in [(Guard::Premise, true), (Guard::Epoch, false), (Guard::Off, true)] {
        let f = Fixture::new(guard);
        let basis = f.basis().await;
        f.colleague(&type_at(4, 0, "Q "));
        let result = f.run(Some(basis), json!([by_index(1, 0, 4), bold()])).await;
        assert_eq!(result.is_ok(), applies, "{guard:?}: {result:?}");
        f.close().await;
    }
}

#[tokio::test]
async fn a_run_with_on_leaves_the_selection_as_it_was() {
    let f = Fixture::new(Guard::Premise);
    let ids = f.blocks().await;
    let receipt = f.run(None, json!([{ "command": "format.italic", "params": {}, "on": [ids[3].0] }])).await.unwrap();
    assert!(receipt["changes"]["changed"].as_array().unwrap().iter().any(|c| c["id"] == ids[3].0), "{receipt}");
    let after = f.run(None, json!([{ "command": "document.selection", "params": {} }])).await.unwrap();
    assert_eq!(after["completed"][0]["result"]["anchor"]["path"], json!([0]), "{after}");
    f.close().await;
}
