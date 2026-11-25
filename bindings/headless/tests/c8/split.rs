//! C8-doc-2 item 1: a paragraph's id follows its text. Enter at the very start of a paragraph
//! makes the new, empty paragraph before it; the paragraph with the text keeps its id. Enter in
//! the middle or at the end keeps the id on the first part (which keeps the text's start). A
//! join keeps the id of the paragraph whose text remains.
use super::outside::{bold, bold_in, by_id, type_at};
use super::*;

/// Enter at `off` of body block `block`, as the colleague's or the agent's steps.
fn enter_at(block: usize, off: usize) -> Vec<(&'static str, Value)> {
    vec![
        ("select.range", json!({ "anchor": { "block": block, "off": off }, "focus": { "block": block, "off": off } })),
        ("text.newParagraph", json!({})),
    ]
}

fn steps(steps: &[(&str, Value)]) -> Value {
    Value::Array(steps.iter().map(|(command, params)| json!({ "command": command, "params": params })).collect())
}

#[tokio::test]
async fn enter_at_the_start_leaves_the_id_on_the_text_same_server() {
    let f = Fixture::new(Guard::Premise);
    let before = f.blocks().await;
    let receipt = f.run(None, steps(&enter_at(1, 0))).await.unwrap();
    let after = f.blocks().await;
    assert_eq!(after.len(), TEXTS.len() + 1);
    // The text `Beta three four` is block 2 now, and keeps the id it had.
    assert_eq!(f.texts()[2], TEXTS[1]);
    assert_eq!(after[2], before[1], "{after:?}");
    assert!(!before.contains(&after[1]), "the empty paragraph is new: {after:?}");
    let created: Vec<&Value> = receipt["changes"]["created"].as_array().unwrap().iter().map(|c| &c["id"]).collect();
    assert_eq!(created, vec![&after[1].0], "{receipt}");
    // A write by the id lands on the text, not on the empty paragraph.
    f.run(None, json!([by_id(&before[1].0, 0, 4), bold()])).await.unwrap();
    assert!(bold_in(&f, 2));
    assert!(!bold_in(&f, 1));
    // The saved file reads back with the same ids.
    let reread = Fixture::with(Guard::Premise, &std::fs::read(f.root.join(FILE)).unwrap());
    assert_eq!(reread.blocks().await, after);
    reread.close().await;
    f.close().await;
}

#[tokio::test]
async fn enter_at_the_end_and_in_the_middle_keep_the_id_on_the_first_part() {
    for off in [4, TEXTS[1].len()] {
        let f = Fixture::new(Guard::Premise);
        let before = f.blocks().await;
        f.run(None, steps(&enter_at(1, off))).await.unwrap();
        let after = f.blocks().await;
        assert_eq!(after[1], before[1], "off {off}");
        assert!(!before.contains(&after[2]), "off {off}");
        f.close().await;
    }
}

#[tokio::test]
async fn para_insert_before_gives_the_new_paragraph_a_new_id() {
    let f = Fixture::new(Guard::Premise);
    let before = f.blocks().await;
    f.run(None, json!([{ "command": "paragraph.insert", "params": { "before": 1, "text": "Above" } }])).await.unwrap();
    let after = f.blocks().await;
    assert_eq!(f.texts()[1], "Above");
    assert_eq!(after[2], before[1]);
    assert!(!before.contains(&after[1]));
    f.close().await;
}

#[tokio::test]
async fn a_colleagues_enter_at_the_start_does_not_refuse_a_write_by_id_and_a_typed_word_inside_does() {
    // Unrelated: the colleague's Enter at the start of the paragraph moves it, its text stays.
    let f = Fixture::new(Guard::Premise);
    let ids = f.blocks().await;
    let basis = f.basis().await;
    f.colleague(&enter_at(1, 0));
    f.run(Some(basis), json!([by_id(&ids[1].0, 0, 4), bold()])).await.unwrap();
    assert!(bold_in(&f, 2), "the text that was block 1 is block 2");
    assert!(!bold_in(&f, 1));
    f.close().await;
    // Related: the colleague types inside the range the write holds.
    let f = Fixture::new(Guard::Premise);
    let ids = f.blocks().await;
    let basis = f.basis().await;
    f.colleague(&type_at(1, 2, "X"));
    conflict(&f.run(Some(basis), json!([by_id(&ids[1].0, 0, 4), bold()])).await.unwrap_err(), "changed");
    f.close().await;
}

#[tokio::test]
async fn deleting_from_a_paragraphs_start_into_the_next_keeps_the_next_ones_id() {
    let f = Fixture::new(Guard::Premise);
    let before = f.blocks().await;
    // From the start of block 1 into block 2: what is left is the end of block 2's text.
    f.run(
        None,
        json!([
            { "command": "select.range", "params": { "anchor": { "block": 1, "off": 0 }, "focus": { "block": 2, "off": 6 } } },
            { "command": "text.delete", "params": {} }
        ]),
    )
    .await
    .unwrap();
    assert_eq!(f.texts()[1], "five six");
    let after = f.blocks().await;
    assert_eq!(after[1], before[2], "{after:?}");
    f.close().await;
}

#[tokio::test]
async fn pasting_paragraphs_at_the_start_keeps_the_id_on_the_text_and_copies_get_new_ids() {
    let f = Fixture::new(Guard::Premise);
    let before = f.blocks().await;
    // Copy blocks 3 and 4 (the whole of both), paste them at the start of block 1.
    f.run(
        None,
        json!([
            { "command": "select.range", "params": { "anchor": { "block": 3, "off": 0 }, "focus": { "block": 4, "off": TEXTS[4].len() } } },
            { "command": "edit.copy", "params": {} },
            { "command": "select.range", "params": { "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 0 } } },
            { "command": "edit.paste", "params": {} }
        ]),
    )
    .await
    .unwrap();
    let texts = f.texts();
    let after = f.blocks().await;
    let at = |text: String| texts.iter().position(|t| *t == text).unwrap();
    // The paragraph that holds `Beta three four` keeps its id; the copies' sources keep theirs.
    assert_eq!(after[at(format!("{}{}", TEXTS[4], TEXTS[1]))].0, before[1].0, "{texts:?} {after:?}");
    let delta: Vec<usize> = texts.iter().enumerate().filter(|(_, t)| *t == TEXTS[3]).map(|(i, _)| i).collect();
    assert_eq!(delta.len(), 2, "{texts:?}");
    assert_eq!(after[delta[1]].0, before[3].0, "the source keeps its id: {after:?}");
    assert_ne!(after[delta[0]].0, before[3].0, "the copy gets a new one: {after:?}");
    f.close().await;
}
