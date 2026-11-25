//! C8-doc-2 items 2, 3 and 6: what a write rests on, each with one colleague change that must
//! refuse it and one unrelated change that must not. Another program really rewrites the file
//! between the agent's observation and its write. The body is [`TEXTS`].
use super::outside::{bold, bold_in, by_index, type_at};
use super::*;

fn insert_after(i: usize) -> Value {
    json!([{ "command": "paragraph.insert", "params": { "after": i, "text": "New" } }])
}

/// Item 2: `paragraph.insert after` reads its paragraph's last word (Enter's AutoCorrect): another
/// program's `teh` typed at its end would be corrected by the agent's write, unseen.
#[tokio::test]
async fn para_insert_holds_what_autocorrect_reads_at_the_end() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&type_at(2, TEXTS[2].len(), " teh"));
    let error = f.run(Some(basis), insert_after(2)).await.unwrap_err();
    conflict(&error, "changed");
    assert_eq!(f.texts()[2], format!("{} teh", TEXTS[2]), "a refused write changes nothing");
    f.close().await;
    // Unrelated: formatting at its start.
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("select.range", json!({ "anchor": { "block": 2, "off": 0 }, "focus": { "block": 2, "off": 2 } })), ("format.italic", json!({}))]);
    f.run(Some(basis), insert_after(2)).await.unwrap();
    assert_eq!(f.texts()[3], "New");
    f.close().await;
}

/// Item 2: the new paragraph's style is its style's `next`.
#[tokio::test]
async fn para_insert_holds_its_styles_next_and_not_another_style() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("style.set", json!({ "style": "Normal", "next": "Heading1" }))]);
    conflict(&f.run(Some(basis), insert_after(2)).await.unwrap_err(), "changed");
    f.close().await;
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("style.set", json!({ "style": "Heading2", "next": "Heading3" }))]);
    f.run(Some(basis), insert_after(2)).await.unwrap();
    f.close().await;
}

/// Item 3: a step with no `on` and no position of its own acts on the selection the call started
/// with (here the caret at the start of block 0): a paragraph put above it moves it.
#[tokio::test]
async fn a_step_on_the_calls_selection_holds_it_as_a_place() {
    let typed = json!([{ "command": "text.insert", "params": { "text": "X" } }]);
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("paragraph.insert", json!({ "before": 0, "text": "Above" }))]);
    conflict(&f.run(Some(basis), typed.clone()).await.unwrap_err(), "relocated");
    assert_eq!(f.texts()[0], "Above", "a refused write changes nothing");
    f.close().await;
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&type_at(3, 0, "Q "));
    f.run(Some(basis), typed).await.unwrap();
    assert_eq!(f.texts()[0], format!("X{}", TEXTS[0]));
    f.close().await;
}

/// Item 3: a step after one that set the selection rests on that one, not on the selection the
/// call started with.
#[tokio::test]
async fn a_step_after_a_selection_rests_on_that_selection() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("paragraph.insert", json!({ "before": 0, "text": "Above" }))]);
    let ids = f.blocks().await;
    // Block 3 at basis is `Delta`, block 4 now; by id it is the same paragraph.
    f.run(Some(basis), json!([super::outside::by_id(&ids[4].0, 0, 5), bold()])).await.unwrap();
    assert!(bold_in(&f, 4));
    f.close().await;
}

/// Item 6: a character command with `on` rests on the formats of the paragraph's characters,
/// not on which characters it has.
#[tokio::test]
async fn on_with_a_character_command_holds_the_formats_not_the_characters() {
    let f = Fixture::new(Guard::Premise);
    let ids = f.blocks().await;
    let basis = f.basis().await;
    f.colleague(&type_at(2, 6, "more "));
    f.run(Some(basis), json!([{ "command": "format.bold", "params": {}, "on": [ids[2].0] }])).await.unwrap();
    assert!(bold_in(&f, 2));
    f.close().await;
    // Related: a word of it bolded; the agent's toggle would now unbold it.
    let f = Fixture::new(Guard::Premise);
    let ids = f.blocks().await;
    let basis = f.basis().await;
    f.colleague(&[("select.range", json!({ "anchor": { "block": 2, "off": 0 }, "focus": { "block": 2, "off": 5 } })), ("format.bold", json!({}))]);
    conflict(&f.run(Some(basis), json!([{ "command": "format.bold", "params": {}, "on": [ids[2].0] }])).await.unwrap_err(), "changed");
    f.close().await;
}

/// Item 6: a paragraph command with `on` rests on the paragraph's properties.
#[tokio::test]
async fn on_with_a_paragraph_command_holds_its_properties_not_its_text() {
    let centre = |id: &Value| json!([{ "command": "format.alignCenter", "params": {}, "on": [id] }]);
    let f = Fixture::new(Guard::Premise);
    let ids = f.blocks().await;
    let basis = f.basis().await;
    f.colleague(&type_at(2, 6, "more "));
    f.run(Some(basis), centre(&ids[2].0)).await.unwrap();
    f.close().await;
    let f = Fixture::new(Guard::Premise);
    let ids = f.blocks().await;
    let basis = f.basis().await;
    f.colleague(&[
        ("select.range", json!({ "anchor": { "block": 2, "off": 0 }, "focus": { "block": 2, "off": 0 } })),
        ("paragraph.indent", json!({})),
    ]);
    conflict(&f.run(Some(basis), centre(&ids[2].0)).await.unwrap_err(), "changed");
    f.close().await;
}

/// A 2x2 table at block 4 with text in every cell.
pub fn table_document() -> Vec<u8> {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 4, "off": 0 }, "focus": { "block": 4, "off": 0 } })).unwrap();
    doc.execute("table.insert", json!({ "rows": 2, "cols": 2 })).unwrap();
    for (r, c) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
        let at = json!({ "story": "body", "path": [4, r, c, 0], "off": 0 });
        doc.execute("select.range", json!({ "anchor": at, "focus": at })).unwrap();
        doc.execute("text.insert", json!({ "text": format!("Cell {r}{c}") })).unwrap();
    }
    doc.save(None).unwrap()
}

/// Item 6: a write by a cell path rests on its row's place and the cells up to its own, not on
/// the other cells' content.
#[tokio::test]
async fn a_cell_path_holds_its_rows_cells_up_to_it_not_their_text() {
    let cell = |off: usize| json!({ "story": "body", "path": [4, 0, 1, 0], "off": off });
    let write = json!([{ "command": "select.range", "params": { "anchor": cell(0), "focus": cell(4) } }, bold()]);
    let colleague_cell = |c: usize| {
        let at = json!({ "story": "body", "path": [4, 0, c, 0], "off": 0 });
        vec![("select.range", json!({ "anchor": at, "focus": at })), ("text.insert", json!({ "text": "Z" }))]
    };
    let f = Fixture::with(Guard::Premise, &table_document());
    let basis = f.basis().await;
    f.colleague(&colleague_cell(0));
    f.run(Some(basis), write.clone()).await.unwrap();
    f.close().await;
    // A column put before the cell: the path names another cell.
    let f = Fixture::with(Guard::Premise, &table_document());
    let basis = f.basis().await;
    let at = json!({ "story": "body", "path": [4, 0, 0, 0], "off": 0 });
    f.colleague(&[("select.range", json!({ "anchor": at, "focus": at })), ("table.insertColumnLeft", json!({}))]);
    assert_eq!(f.run(Some(basis), write).await.unwrap_err()["error"]["code"], "conflict");
    f.close().await;
}

/// Item 6: a write naming a style rests on that style, not on every style.
#[tokio::test]
async fn a_style_by_name_holds_that_style_only() {
    let write = json!([by_index(1, 0, 0), { "command": "paragraph.style", "params": { "style": "Heading1" } }]);
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("style.set", json!({ "style": "Heading2", "next": "Heading3" }))]);
    f.run(Some(basis), write.clone()).await.unwrap();
    f.close().await;
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("style.set", json!({ "style": "Heading1", "next": "Heading3" }))]);
    conflict(&f.run(Some(basis), write).await.unwrap_err(), "changed");
    f.close().await;
}

/// The part number a story's first paragraph reading `text` has in `doc`, as positions name it.
fn part_reading(doc: &mut DocDocument, text: &str) -> Option<u64> {
    (1..20u64).find(|k| {
        doc.execute("file.paragraph", json!({ "path": [0], "story": { "part": k } }))
            .is_ok_and(|answer| answer["paragraph"]["text"].as_str().is_some_and(|t| t.contains(text)))
    })
}

/// Item 4: a position in a story by its part number rests on that number still naming the same
/// story. A text box (anchored in block 0) and a footer: the agent writes at the text box's
/// number; another program deletes the text box, and the footer, read after it, takes its number.
#[tokio::test]
async fn a_story_by_part_number_is_refused_when_the_number_names_another() {
    let mut doc = document();
    doc.execute("footer.insert", json!({ "text": "Footer words" })).unwrap();
    doc.execute("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 0 } })).unwrap();
    doc.execute("textBox.insert", json!({ "text": "Box words" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let part = part_reading(&mut DocDocument::open(FILE, &bytes).unwrap(), "Box words").unwrap();
    // An agent names a part's position by its number with `caret.set` (its `story` takes one).
    let write = json!([
        { "command": "caret.set", "params": { "pos": { "story": { "part": part }, "path": [0], "off": 0 } } },
        { "command": "text.insert", "params": { "text": "Mine " } }
    ]);
    // Related: the text box deleted (the character anchoring it in block 0).
    let f = Fixture::with(Guard::Premise, &bytes);
    let basis = f.basis().await;
    f.colleague(&[("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 3 } })), ("text.delete", json!({}))]);
    assert_eq!(part_reading(&mut f.disk(), "Box words"), None);
    assert_eq!(part_reading(&mut f.disk(), "Footer words"), Some(part), "the footer took the text box's number");
    let error = f.run(Some(basis), write.clone()).await.unwrap_err();
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert_eq!(part_reading(&mut f.disk(), "Mine"), None, "a refused write changes nothing");
    f.close().await;
    // Unrelated: typing in the body after the anchor.
    let f = Fixture::with(Guard::Premise, &bytes);
    let basis = f.basis().await;
    f.colleague(&type_at(3, 0, "Q "));
    f.run(Some(basis), write).await.unwrap();
    assert_eq!(part_reading(&mut f.disk(), "Mine Box words"), Some(part));
    f.close().await;
}

fn stroke(y: f64) -> Value {
    json!({ "tool": "pen", "color": "#000000", "width": 1.0, "points": [[100.0, y], [200.0, y]] })
}

/// Item 4: an agent's ink names its page and its pen: the caret's page and the editor's current
/// pen are no part of what it observed.
#[tokio::test]
async fn ink_without_its_page_or_pen_is_refused_for_an_agent() {
    let f = Fixture::new(Guard::Premise);
    let no_page = f.run(None, json!([{ "command": "drawing.ink", "params": { "strokes": [stroke(300.0)] } }])).await.unwrap_err();
    assert!(no_page.to_string().contains("placement.page"), "{no_page}");
    let no_pen = f
        .run(
            None,
            json!([{ "command": "drawing.ink", "params": { "placement": { "page": 0 }, "strokes": [{ "points": [[1.0, 1.0], [9.0, 9.0]] }] } }]),
        )
        .await
        .unwrap_err();
    assert!(no_pen.to_string().contains("width"), "{no_pen}");
    f.run(None, json!([{ "command": "drawing.ink", "params": { "placement": { "page": 0 }, "strokes": [stroke(300.0)] } }])).await.unwrap();
    f.close().await;
}

/// Item 4: an eraser path on a page rests on where every ink stroke is: another program's stroke
/// drawn under the path refuses it; typing elsewhere does not.
#[tokio::test]
async fn an_eraser_path_holds_the_strokes_it_could_take() {
    let erase = json!([{ "command": "drawing.erase", "params": { "placement": { "page": 0 }, "path": [[150.0, 290.0], [150.0, 410.0]] } }]);
    let ink = |y: f64| ("drawing.ink", json!({ "placement": { "page": 0 }, "strokes": [stroke(y)] }));
    let mut doc = document();
    doc.execute(ink(300.0).0, ink(300.0).1).unwrap();
    let bytes = doc.save(None).unwrap();
    let f = Fixture::with(Guard::Premise, &bytes);
    let basis = f.basis().await;
    f.colleague(&[ink(400.0)]);
    assert_eq!(f.run(Some(basis), erase.clone()).await.unwrap_err()["error"]["code"], "conflict");
    f.close().await;
    let f = Fixture::with(Guard::Premise, &bytes);
    let basis = f.basis().await;
    f.colleague(&type_at(3, 0, "Q "));
    let receipt = f.run(Some(basis), erase).await.unwrap();
    assert_eq!(receipt["completed"][0]["result"]["erased"], 1, "{receipt}");
    f.close().await;
}

/// Item 9: an agent's write stamps the document as Word's save does (modified time, last modified
/// by, revision), and its receipt says so: the saved file holds what the receipt said, and the
/// next call finds nothing the agent was not told.
#[tokio::test]
async fn an_agents_write_carries_the_save_stamp_in_its_receipt() {
    let f = Fixture::new(Guard::Premise);
    let was = f.call("query", json!({ "objects": ["document"], "fields": ["core"] })).await.unwrap()["objects"][0]["fields"]["core"].clone();
    // The stamp's time is in whole seconds: the write comes in a later second than the fixture's
    // save, as a person's edit does, so its modified time is a new one.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let receipt = f.run(Some(f.basis().await), json!([by_index(1, 0, 4), bold()])).await.unwrap();
    let root = receipt["changes"]["changed"].as_array().unwrap().iter().find(|c| c["id"] == "document").cloned().unwrap_or_default();
    let fields: Vec<&str> = root["fields"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    assert!(fields.contains(&"core.revision") && fields.contains(&"core.modified"), "{receipt}");
    let reread = Fixture::with(Guard::Premise, &std::fs::read(f.root.join(FILE)).unwrap());
    let core = reread.call("query", json!({ "objects": ["document"], "fields": ["core"] })).await.unwrap()["objects"][0]["fields"]["core"].clone();
    reread.close().await;
    assert_eq!(core["revision"].as_u64(), was["revision"].as_u64().map(|r| r + 1), "{core} {was}");
    assert!(!core["lastModifiedBy"].as_str().unwrap_or_default().is_empty(), "{core}");
    // The next write sees nothing it was not told.
    let next = f.run(Some(receipt["revision"].clone()), json!([by_index(2, 0, 4), bold()])).await.unwrap();
    assert!(next.get("others").is_none_or(Value::is_null), "{next}");
    f.close().await;
}

/// Item 7: every write rests on whether changes are tracked (typing and formatting become
/// revisions); not on another paragraph.
#[tokio::test]
async fn a_write_holds_whether_changes_are_tracked() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("change.track", json!({ "value": true }))]);
    conflict(&f.run(Some(basis), json!([by_index(1, 0, 4), bold()])).await.unwrap_err(), "changed");
    f.close().await;
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&type_at(3, 0, "Q "));
    f.run(Some(basis), json!([by_index(1, 0, 4), bold()])).await.unwrap();
    f.close().await;
}

/// Item 7: a table command rests on the table the selection is in (its grid), not on a body
/// paragraph.
#[tokio::test]
async fn a_table_command_holds_its_table() {
    let cell = json!({ "story": "body", "path": [4, 1, 1, 0], "off": 0 });
    let write =
        json!([{ "command": "select.range", "params": { "anchor": cell, "focus": cell } }, { "command": "table.insertRowBelow", "params": {} }]);
    let f = Fixture::with(Guard::Premise, &table_document());
    let basis = f.basis().await;
    let at = json!({ "story": "body", "path": [4, 0, 0, 0], "off": 0 });
    f.colleague(&[("select.range", json!({ "anchor": at, "focus": at })), ("table.insertColumnLeft", json!({}))]);
    assert_eq!(f.run(Some(basis), write.clone()).await.unwrap_err()["error"]["code"], "conflict");
    f.close().await;
    let f = Fixture::with(Guard::Premise, &table_document());
    let basis = f.basis().await;
    f.colleague(&type_at(1, 0, "Q "));
    f.run(Some(basis), write).await.unwrap();
    f.close().await;
}

/// Item 7: a theme change rewrites every style and the theme formatting of every run: it rests on
/// the document's content, not on its properties (title, author).
#[tokio::test]
async fn a_theme_change_holds_the_styles() {
    let write = json!([{ "command": "theme.fonts", "params": { "heading": "Georgia" } }]);
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("style.set", json!({ "style": "Heading1", "next": "Heading3" }))]);
    assert_eq!(f.run(Some(basis), write.clone()).await.unwrap_err()["error"]["code"], "conflict");
    f.close().await;
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("file.properties", json!({ "title": "Another title" }))]);
    f.run(Some(basis), write).await.unwrap();
    f.close().await;
}
