//! Item 2: every paragraph and table row has a stable tool id, persisted in the file
//! (`w14:paraId`), the same on every read, kept by a save, new for a new paragraph.
use super::*;

fn is_tool_id(id: &Value, prefix: char) -> bool {
    id.as_str().is_some_and(|id| id.len() == 9 && id.starts_with(prefix) && id[1..].chars().all(|c| c.is_ascii_hexdigit()))
}

#[tokio::test]
async fn a_file_without_paragraph_ids_reads_with_the_same_ids_every_time() {
    let file = include_bytes!("../../../../conformance/basic.docx");
    let a = Fixture::with(Guard::Premise, file);
    let b = Fixture::with(Guard::Premise, file);
    let (first, second) = (a.blocks().await, b.blocks().await);
    assert!(!first.is_empty());
    for (id, kind) in &first {
        assert!(is_tool_id(id, if kind == "table" { 't' } else { 'p' }), "{id} {kind}");
    }
    assert_eq!(first, second);
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn a_save_keeps_every_id_and_a_split_gives_the_second_half_a_new_one() {
    let f = Fixture::new(Guard::Premise);
    let before = f.blocks().await;
    assert_eq!(before.len(), TEXTS.len());
    // Enter in the middle of block 1: its first half keeps its id, the second half is new.
    let receipt = f
        .run(
            None,
            json!([
                { "command": "select.range", "params": { "anchor": { "block": 1, "off": 4 }, "focus": { "block": 1, "off": 4 } } },
                { "command": "text.newParagraph", "params": {} }
            ]),
        )
        .await
        .unwrap();
    let after = f.blocks().await;
    assert_eq!(after.len(), TEXTS.len() + 1);
    assert_eq!(after[..2], before[..2]);
    assert_eq!(after[3..], before[2..]);
    assert!(!before.contains(&after[2]));
    let created = receipt["changes"]["created"].as_array().unwrap();
    assert_eq!(created.len(), 1, "{receipt}");
    assert_eq!(created[0]["id"], after[2].0, "{receipt}");
    // Another program reads the saved file with the same ids.
    let reread = Fixture::with(Guard::Premise, &std::fs::read(f.root.join(FILE)).unwrap());
    assert_eq!(reread.blocks().await, after);
    reread.close().await;
    f.close().await;
}

#[tokio::test]
async fn joining_two_paragraphs_keeps_the_first_ones_id_in_the_saved_file() {
    let f = Fixture::new(Guard::Premise);
    let before = f.blocks().await;
    f.run(
        None,
        json!([
            { "command": "select.range", "params": { "anchor": { "block": 2, "off": 0 }, "focus": { "block": 2, "off": 0 } } },
            { "command": "text.backspace", "params": {} }
        ]),
    )
    .await
    .unwrap();
    // The joined paragraph is the first one, with its id; the second one's id is gone.
    let reread = Fixture::with(Guard::Premise, &std::fs::read(f.root.join(FILE)).unwrap());
    let saved = reread.blocks().await;
    assert_eq!(saved.len(), TEXTS.len() - 1);
    assert_eq!(saved[..2], before[..2]);
    assert_eq!(saved[2..], before[3..]);
    reread.close().await;
    f.close().await;
}

#[tokio::test]
async fn table_rows_have_ids_and_the_table_is_named_by_its_first_row() {
    let f = Fixture::new(Guard::Premise);
    f.run(
        None,
        json!([
            { "command": "select.range", "params": { "anchor": { "block": 4, "off": 0 }, "focus": { "block": 4, "off": 0 } } },
            { "command": "insert.table", "params": { "rows": 2, "cols": 2 } }
        ]),
    )
    .await
    .unwrap();
    let blocks = f.blocks().await;
    let table = blocks.iter().find(|(_, kind)| kind == "table").unwrap().0.clone();
    assert!(is_tool_id(&table, 't'), "{blocks:?}");
    let rows = f.call("query", json!({ "objects": [table], "follow": "table.rows", "fields": ["id"] })).await.unwrap();
    let rows: Vec<Value> = rows["objects"].as_array().unwrap().iter().map(|row| row["id"].clone()).collect();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows.iter().all(|row| is_tool_id(row, 'r')), "{rows:?}");
    assert_eq!(&table.as_str().unwrap()[1..], &rows[0].as_str().unwrap()[1..]);
    let reread = Fixture::with(Guard::Premise, &std::fs::read(f.root.join(FILE)).unwrap());
    assert_eq!(reread.blocks().await, blocks);
    reread.close().await;
    f.close().await;
}

#[tokio::test]
async fn a_drawing_object_is_named_by_its_number_in_the_model_and_after_a_save() {
    let f = Fixture::new(Guard::Premise);
    let receipt = f
        .run(
            None,
            json!([
                { "command": "select.range", "params": { "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 0 } } },
                { "command": "drawing.insert", "params": { "geometry": { "preset": "rect" }, "frame": { "w": 72, "h": 36 } } }
            ]),
        )
        .await
        .unwrap();
    // The command answers the number as text; the object model names it by the number.
    let uid = json!(receipt["completed"][1]["result"]["id"].as_str().and_then(|id| id.parse::<u64>().ok()).unwrap());
    let created: Vec<&Value> = receipt["changes"]["created"].as_array().unwrap().iter().map(|c| &c["id"]).collect();
    assert!(created.contains(&&uid), "{receipt}");
    let reread = Fixture::with(Guard::Premise, &std::fs::read(f.root.join(FILE)).unwrap());
    let answer = reread.call("query", json!({ "objects": [uid], "fields": ["uid"] })).await.unwrap();
    assert_eq!(answer["objects"][0]["type"], "Inline", "{answer}");
    reread.close().await;
    f.close().await;
}

#[tokio::test]
async fn a_paragraph_a_batch_makes_gets_its_id_in_the_receipt_and_keeps_it_in_the_file() {
    let f = Fixture::new(Guard::Premise);
    let receipt = f
        .call(
            "batch",
            json!({ "ops": [
                { "op": "create", "type": "Block", "ref": "$b", "fields": { "kind": "para" }, "in": "document.body", "at": 1 },
                { "op": "text.set", "target": "$b", "field": "text", "text": "Made by a batch" }
            ] }),
        )
        .await
        .unwrap();
    let created = receipt["changes"]["created"].as_array().unwrap();
    assert_eq!(created.len(), 1, "{receipt}");
    let id = created[0]["id"].clone();
    assert!(is_tool_id(&id, 'p'), "{receipt}");
    let reread = Fixture::with(Guard::Premise, &std::fs::read(f.root.join(FILE)).unwrap());
    assert_eq!(reread.blocks().await[1].0, id);
    reread.close().await;
    f.close().await;
}
