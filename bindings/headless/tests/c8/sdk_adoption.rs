//! C8-doc-pin: saved-file identities, mark-only inventories and retained editor history.
use super::outside::{bold, by_index};
use super::*;

#[tokio::test]
async fn an_unrelated_reread_keeps_the_observed_selection_state() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    let view = f.kits.open(kition_kits::Caller::Agent, FILE).await.unwrap();
    let state = f.kits.context(kition_kits::Caller::Agent, view.kit_id, &view.document_id, "context.get", Default::default()).await.unwrap().unwrap()
        ["state"]
        .clone();
    f.colleague(&outside::type_at(3, 0, "Peer "));
    let view = f.kits.open(kition_kits::Caller::Agent, FILE).await.unwrap();
    let after = f.kits.context(kition_kits::Caller::Agent, view.kit_id, &view.document_id, "context.get", Default::default()).await.unwrap().unwrap()
        ["state"]
        .clone();
    assert_eq!(after["selection"], state["selection"], "before {state}, after {after}");
    f.run(Some(basis), json!([{ "command": "text.insert", "params": { "text": "Agent " } }])).await.unwrap();
    assert_eq!(f.texts()[3], format!("Peer {}", TEXTS[3]));
    f.close().await;
}

/// Fresh replica IDs inside inline marks must not turn an unchanged file into a text edit.
#[tokio::test]
async fn an_unchanged_inline_reference_reread_reports_no_changes() {
    let file = concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/check-missing-note.docx");
    let f = Fixture::with(Guard::Premise, &std::fs::read(file).unwrap());
    let receipt = f.run(None, json!([by_index(0, 0, 2), bold()])).await.unwrap();
    f.rewrite(&std::fs::read(f.root.join(FILE)).unwrap());
    let answer = f.call("inspect", json!({ "basis": receipt["revision"] })).await.unwrap();
    let same_text = answer["others"]["changed"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|change| change["edits"].as_array().into_iter().flatten())
        .any(|edit| edit["field"] == "text" && edit["before"] == edit["after"]);
    assert!(!same_text, "{answer}");
    for list in ["changed", "created", "deleted", "moved"] {
        assert!(answer["others"][list].as_array().is_none_or(Vec::is_empty), "{answer}");
    }
    f.close().await;
}

/// Formatting by stable ID consumes formatting, independently of body indexes.
#[tokio::test]
async fn a_character_command_by_id_applies_after_an_unrelated_insertion_above() {
    let f = Fixture::new(Guard::Premise);
    let ids = f.blocks().await;
    let basis = f.basis().await;
    f.colleague(&[("paragraph.insert", json!({ "before": 0, "text": "Above" }))]);
    f.run(Some(basis), json!([{ "command": "format.italic", "params": {}, "on": [ids[2].0] }])).await.unwrap();
    assert_eq!(f.texts(), ["Above", TEXTS[0], TEXTS[1], TEXTS[2], TEXTS[3], TEXTS[4]]);
    let target = f.disk().execute("file.paragraph", json!({ "path": [3] })).unwrap();
    assert!(target["paragraph"]["runs"].as_array().unwrap().iter().all(|run| run["props"]["italic"] == true), "{target}");
    f.close().await;
}

/// Mark-only objects belong to the complete private inventory used for outside comparison.
#[tokio::test]
async fn an_outside_rotation_reports_the_inline_frame_change() {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 0 } })).unwrap();
    let shape = doc.execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "w": 50, "h": 20 } })).unwrap();
    let uid = shape["id"].as_str().and_then(|id| id.parse::<u64>().ok()).or_else(|| shape["id"].as_u64()).unwrap();
    let f = Fixture::with(Guard::Premise, &doc.save(None).unwrap());
    let basis = f.basis().await;
    f.colleague(&[("drawing.transform", json!({ "ids": [uid], "rotateBy": 45 }))]);
    let answer = f.call("inspect", json!({ "basis": basis })).await.unwrap();
    let others = &answer["others"];
    let told = ["changed", "created", "deleted"]
        .iter()
        .flat_map(|list| others[*list].as_array().into_iter().flatten())
        .any(|change| change["type"] == "Inline");
    assert!(told, "{others}");
    assert!(
        others["changed"].as_array().unwrap().iter().any(|change| {
            change["type"] == "Inline"
                && change["edits"].as_array().unwrap().iter().any(|edit| edit["field"] == "frame.float.rot" && edit["after"] == 45)
        }),
        "{others}"
    );
    f.close().await;
}

#[tokio::test]
async fn an_outside_reread_keeps_agent_undo_and_the_outside_edit() {
    let f = Fixture::new(Guard::Premise);
    let ids = f.blocks().await;
    f.run(None, json!([outside::by_id(&ids[0].0, TEXTS[0].len(), TEXTS[0].len()), { "command": "text.insert", "params": { "text": " Agent" } }]))
        .await
        .unwrap();
    f.colleague(&[("paragraph.text", json!({ "index": 3, "text": "Colleague" }))]);
    let basis = f.basis().await;
    f.run(Some(basis), json!([{ "command": "edit.undo", "params": {} }])).await.unwrap();
    assert_eq!(f.texts(), [TEXTS[0], TEXTS[1], TEXTS[2], "Colleague", TEXTS[4]]);
    f.close().await;
}

#[tokio::test]
async fn character_formats_by_id_still_refuse_a_changed_format() {
    let f = Fixture::new(Guard::Premise);
    let ids = f.blocks().await;
    let basis = f.basis().await;
    f.colleague(&[("select.range", json!({ "anchor": { "block": 2, "off": 0 }, "focus": { "block": 2, "off": 5 } })), ("format.bold", json!({}))]);
    let before = std::fs::read(f.root.join(FILE)).unwrap();
    let error = f.run(Some(basis), json!([{ "command": "format.italic", "params": {}, "on": [ids[2].0] }])).await.unwrap_err();
    conflict(&error, "changed");
    assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
    f.close().await;
}

#[tokio::test]
async fn character_formats_by_id_refuse_a_changed_distribution_of_existing_marks() {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 2, "off": 0 }, "focus": { "block": 2, "off": 5 } })).unwrap();
    doc.execute("format.italic", json!({ "value": true })).unwrap();
    let f = Fixture::with(Guard::Premise, &doc.save(None).unwrap());
    let ids = f.blocks().await;
    let basis = f.basis().await;
    f.colleague(&[
        ("select.range", json!({ "anchor": { "block": 2, "off": 5 }, "focus": { "block": 2, "off": 6 } })),
        ("format.italic", json!({ "value": true })),
    ]);
    let before = std::fs::read(f.root.join(FILE)).unwrap();
    let error = f.run(Some(basis), json!([{ "command": "format.italic", "params": { "value": false }, "on": [ids[2].0] }])).await.unwrap_err();
    conflict(&error, "changed");
    assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
    f.close().await;
}

#[tokio::test]
async fn raw_text_assignments_merge_unrelated_edits_and_refuse_same_field_edits() {
    for same in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let ids = f.blocks().await;
        let basis = f.basis().await;
        f.colleague(&[("paragraph.text", json!({ "index": if same { 2 } else { 3 }, "text": "Colleague" }))]);
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let answer =
            f.call("batch", json!({ "basis": basis, "ops": [{ "op": "text.set", "target": ids[2].0, "field": "text", "text": "Agent" }] })).await;
        if same {
            conflict(&answer.unwrap_err(), "changed");
            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
        } else {
            answer.unwrap();
            assert_eq!(f.texts(), [TEXTS[0], TEXTS[1], "Agent", "Colleague", TEXTS[4]]);
        }
        f.close().await;
    }
}

#[tokio::test]
async fn shortening_a_selected_endpoint_reports_changed_and_preserves_the_file() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("caret.set", json!({ "pos": { "block": 2, "off": 8 } })), ("text.newParagraph", json!({}))]);
    let before = std::fs::read(f.root.join(FILE)).unwrap();
    let error = f
        .run(
            Some(basis),
            json!([
                { "command": "select.range", "params": { "anchor": { "block": 1, "off": 5 }, "focus": { "block": 2, "off": 10 } } },
                bold(),
            ]),
        )
        .await
        .unwrap_err();
    conflict(&error, "changed");
    assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
    f.close().await;
}

#[tokio::test]
async fn formatting_by_id_ignores_split_anchors_but_preserves_same_field_protection() {
    for changed_format in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let ids = f.blocks().await;
        let basis = f.basis().await;
        if changed_format {
            f.colleague(&[
                ("select.range", json!({ "anchor": { "block": 2, "off": 0 }, "focus": { "block": 2, "off": 5 } })),
                ("format.italic", json!({ "value": true })),
            ]);
        } else {
            f.colleague(&[("caret.set", json!({ "pos": { "block": 2, "off": 8 } })), ("text.newParagraph", json!({}))]);
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let answer = f.run(Some(basis), json!([{ "on": [ids[2].0], "command": "format.italic", "params": { "value": false } }])).await;
        if changed_format {
            conflict(&answer.unwrap_err(), "changed");
            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
        } else {
            answer.unwrap();
            assert_eq!(f.texts(), [TEXTS[0], TEXTS[1], "Gamma fi", "ve six", TEXTS[3], TEXTS[4]]);
            let paragraph = f.disk().execute("file.paragraph", json!({ "path": [2] })).unwrap();
            assert!(paragraph["paragraph"]["runs"].as_array().unwrap().iter().all(|run| run["props"]["italic"] != true));
        }
        f.close().await;
    }
}
