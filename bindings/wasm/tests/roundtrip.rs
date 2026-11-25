//! Core editing survives save and reopen: each test edits a new document only through engine
//! commands (as the editor and the agent do), checks the edit took, saves it as .docx, opens the
//! saved bytes again and compares the document before and after.
//!
//! The comparison is `file.inspect` (blocks, table cells, header and footer parts, sections)
//! plus `file.paragraph` for every body paragraph and every table cell's paragraphs. Left out
//! as volatile: `canUndo` and `canRedo` (a reopened document has no history), `properties` (save
//! stamps the core properties) and `selection` (a file does not keep the caret).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_doc_kit::DocDocument;
use kition_kit::KitDocument;
use serde_json::{Value, json};

const NAME: &str = "Round trip.docx";

fn exec(doc: &mut DocDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn new_document() -> DocDocument {
    DocDocument::create(NAME, 0, 0).unwrap_or_else(|error| panic!("{error}"))
}

fn at(path: &[usize], off: usize) -> Value {
    json!({ "story": "body", "path": path, "off": off })
}

/// Selects characters `from..to` of the paragraph at `path`.
fn select(doc: &mut DocDocument, path: &[usize], from: usize, to: usize) {
    exec(doc, "select.range", json!({ "anchor": at(path, from), "focus": at(path, to) }));
}

/// Puts the caret at the end of the text of the paragraph at `path`.
fn caret(doc: &mut DocDocument, path: &[usize], off: usize) {
    exec(doc, "caret.set", json!({ "pos": at(path, off) }));
}

fn paragraph(doc: &mut DocDocument, path: &[usize]) -> Value {
    exec(doc, "file.paragraph", json!({ "path": path }))
}

/// The paths of every body paragraph and every table cell's first paragraph.
fn paragraph_paths(inspect: &Value) -> Vec<Vec<usize>> {
    let mut paths = Vec::new();
    for block in inspect["blocks"].as_array().into_iter().flatten() {
        let index = usize::try_from(block["index"].as_u64().unwrap()).unwrap();
        match block["type"].as_str() {
            Some("paragraph") => paths.push(vec![index]),
            Some("table") => {
                for (row, cells) in block["cells"].as_array().into_iter().flatten().enumerate() {
                    for col in 0..cells.as_array().map_or(0, Vec::len) {
                        paths.push(vec![index, row, col, 0]);
                    }
                }
            }
            _ => {}
        }
    }
    paths
}

/// What a round trip must keep (see the module comment for what is left out).
fn state(doc: &mut DocDocument) -> Value {
    let mut inspect = exec(doc, "file.inspect", json!({}));
    let paragraphs: Vec<Value> = paragraph_paths(&inspect).iter().map(|path| json!({ "path": path, "details": paragraph(doc, path) })).collect();
    let object = inspect.as_object_mut().unwrap();
    for volatile in ["canUndo", "canRedo", "properties", "selection"] {
        object.remove(volatile);
    }
    json!({ "inspect": inspect, "paragraphs": paragraphs })
}

/// Saves `doc` as .docx, opens the bytes again and asserts nothing changed; answers the state.
fn assert_survives_save_and_reopen(doc: &mut DocDocument) -> Value {
    let before = state(doc);
    let bytes = doc.save(Some(NAME)).unwrap_or_else(|error| panic!("save: {error}"));
    let mut reopened = DocDocument::open(NAME, &bytes).unwrap_or_else(|error| panic!("reopen: {error}"));
    let after = state(&mut reopened);
    let mut changes = Vec::new();
    differences(&before, &after, "", &mut changes);
    assert!(changes.is_empty(), "the document changed across save and reopen:\n{}", changes.join("\n"));
    before
}

/// Each JSON pointer where `before` and `after` differ, with both values.
fn differences(before: &Value, after: &Value, at: &str, out: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().chain(b.keys().filter(|key| !a.contains_key(*key))) {
                differences(a.get(key).unwrap_or(&Value::Null), b.get(key).unwrap_or(&Value::Null), &format!("{at}/{key}"), out);
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (index, (x, y)) in a.iter().zip(b).enumerate() {
                differences(x, y, &format!("{at}/{index}"), out);
            }
        }
        _ if before != after => out.push(format!("{at}: {before} -> {after}")),
        _ => {}
    }
}

/// The run of `paragraph` (a `file.paragraph` answer) covering character `at`.
fn run_at(paragraph: &Value, at: usize) -> Value {
    let mut start = 0;
    for run in paragraph["paragraph"]["runs"].as_array().into_iter().flatten() {
        let len = usize::try_from(run["len"].as_u64().unwrap()).unwrap();
        if at < start + len {
            return run["props"].clone();
        }
        start += len;
    }
    Value::Null
}

#[test]
fn character_formatting_survives_save_and_reopen() {
    let mut doc = new_document();
    let text = "plain bold italic under strike big serif marked sup red";
    exec(&mut doc, "text.insert", json!({ "text": text }));
    let word = |name: &str| {
        let from = text.find(name).unwrap();
        (from, from + name.len())
    };
    let edits: [(&str, &str, Value); 9] = [
        ("bold", "format.bold", json!({})),
        ("italic", "format.italic", json!({})),
        ("under", "format.underline", json!({})),
        ("strike", "format.strikethrough", json!({})),
        ("big", "format.fontSize", json!({ "size": 20 })),
        ("serif", "format.fontName", json!({ "name": "Georgia" })),
        ("marked", "format.highlight", json!({ "color": "yellow" })),
        ("sup", "format.superscript", json!({})),
        ("red", "format.color", json!({ "color": "C00000" })),
    ];
    for (name, command, params) in &edits {
        let (from, to) = word(name);
        select(&mut doc, &[0], from, to);
        exec(&mut doc, command, params.clone());
    }
    let details = paragraph(&mut doc, &[0]);
    let props = |name: &str| run_at(&details, word(name).0);
    assert_eq!(props("bold")["bold"], json!(true));
    assert_eq!(props("italic")["italic"], json!(true));
    assert!(props("under")["underline"].is_string() || props("under")["underline"] == json!(true), "{details}");
    assert!(props("strike").to_string().contains("strike"), "{details}");
    assert_eq!(props("big")["size"], json!(20.0));
    assert!(props("serif").to_string().contains("Georgia"), "{details}");
    assert!(props("marked").to_string().contains("yellow"), "{details}");
    assert!(props("sup").to_string().contains("super"), "{details}");
    assert_eq!(props("red")["color"], json!({ "Rgb": [192, 0, 0] }), "{details}");
    assert_eq!(props("plain"), Value::Null);
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn paragraph_formatting_survives_save_and_reopen() {
    let mut doc = new_document();
    for (index, text) in ["Centred title", "Right-aligned date", "A justified, indented paragraph with wider lines."].iter().enumerate() {
        if index > 0 {
            exec(&mut doc, "text.newParagraph", json!({}));
        }
        exec(&mut doc, "text.insert", json!({ "text": text }));
    }
    caret(&mut doc, &[0], 0);
    exec(&mut doc, "format.alignCenter", json!({}));
    caret(&mut doc, &[1], 0);
    exec(&mut doc, "format.alignRight", json!({}));
    caret(&mut doc, &[2], 0);
    exec(&mut doc, "format.justify", json!({}));
    exec(&mut doc, "paragraph.indent", json!({}));
    exec(&mut doc, "paragraph.lineSpacing", json!({ "value": 1.5 }));
    let resolved = |doc: &mut DocDocument, block: usize| paragraph(doc, &[block])["resolved"].clone();
    assert_eq!(resolved(&mut doc, 0)["align"], json!("center"));
    assert_eq!(resolved(&mut doc, 1)["align"], json!("right"));
    let third = resolved(&mut doc, 2);
    assert_eq!(third["align"], json!("justify"), "{third}");
    assert!(third["indentLeft"].as_f64().unwrap() > 0.0, "{third}");
    assert_eq!(third["lineSpacing"]["value"].as_f64().map(|value| (value * 100.0).round()), Some(150.0), "{third}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn bulleted_and_numbered_lists_survive_save_and_reopen() {
    let mut doc = new_document();
    exec(&mut doc, "text.insert", json!({ "text": "Shopping" }));
    exec(&mut doc, "text.newParagraph", json!({}));
    exec(&mut doc, "paragraph.bullets", json!({}));
    exec(&mut doc, "text.insert", json!({ "text": "Apples" }));
    exec(&mut doc, "text.newParagraph", json!({}));
    exec(&mut doc, "text.insert", json!({ "text": "Pears" }));
    exec(&mut doc, "text.newParagraph", json!({}));
    exec(&mut doc, "paragraph.bullets", json!({}));
    exec(&mut doc, "text.insert", json!({ "text": "Steps" }));
    exec(&mut doc, "text.newParagraph", json!({}));
    exec(&mut doc, "paragraph.numbering", json!({}));
    exec(&mut doc, "text.insert", json!({ "text": "First step" }));
    exec(&mut doc, "text.newParagraph", json!({}));
    exec(&mut doc, "text.insert", json!({ "text": "A detail of the first step" }));
    exec(&mut doc, "paragraph.listLevel", json!({ "level": 1 }));
    exec(&mut doc, "text.newParagraph", json!({}));
    exec(&mut doc, "text.insert", json!({ "text": "Second step" }));
    exec(&mut doc, "paragraph.listLevel", json!({ "level": 0 }));
    let inspect = exec(&mut doc, "file.inspect", json!({}));
    let list = |index: usize| inspect["blocks"][index]["list"].clone();
    assert!(list(0).is_null());
    assert_eq!(list(1), list(2), "the two bullets are one list");
    assert!(list(1).is_object(), "{inspect}");
    assert!(list(4).is_object() && list(4)["num"] != list(1)["num"], "the numbered list is its own list: {inspect}");
    assert_eq!(list(5)["level"], json!(1), "{inspect}");
    assert_eq!(list(6)["level"], json!(0), "{inspect}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn heading_and_custom_styles_survive_save_and_reopen() {
    let mut doc = new_document();
    for (index, text) in ["Report", "Background", "Body text that stays Normal.", "A quoted remark."].iter().enumerate() {
        if index > 0 {
            exec(&mut doc, "text.newParagraph", json!({}));
        }
        exec(&mut doc, "text.insert", json!({ "text": text }));
    }
    caret(&mut doc, &[0], 0);
    exec(&mut doc, "paragraph.heading1", json!({}));
    caret(&mut doc, &[1], 0);
    exec(&mut doc, "paragraph.heading2", json!({}));
    caret(&mut doc, &[3], 0);
    exec(&mut doc, "paragraph.style", json!({ "style": "Quote" }));
    let inspect = exec(&mut doc, "file.inspect", json!({}));
    let style = |index: usize| inspect["blocks"][index]["style"].clone();
    assert_eq!(style(0), json!("Heading1"), "{inspect}");
    assert_eq!(style(1), json!("Heading2"), "{inspect}");
    assert_eq!(style(2), json!("Normal"), "{inspect}");
    assert_eq!(style(3), json!("Quote"), "{inspect}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn tables_survive_save_and_reopen() {
    let mut doc = new_document();
    exec(&mut doc, "text.insert", json!({ "text": "Prices" }));
    exec(&mut doc, "text.newParagraph", json!({}));
    exec(&mut doc, "table.insert", json!({ "rows": 2, "cols": 2 }));
    let table =
        exec(&mut doc, "file.inspect", json!({}))["blocks"].as_array().unwrap().iter().position(|block| block["type"] == json!("table")).unwrap();
    for (row, col, text) in [(0, 0, "Item"), (0, 1, "Price"), (1, 0, "Tea"), (1, 1, "3.50")] {
        caret(&mut doc, &[table, row, col, 0], 0);
        exec(&mut doc, "text.insert", json!({ "text": text }));
    }
    caret(&mut doc, &[table, 1, 1, 0], 0);
    exec(&mut doc, "table.insertRowBelow", json!({}));
    caret(&mut doc, &[table, 2, 0, 0], 0);
    exec(&mut doc, "text.insert", json!({ "text": "Coffee" }));
    caret(&mut doc, &[table, 0, 1, 0], 0);
    exec(&mut doc, "table.insertColumnRight", json!({}));
    caret(&mut doc, &[table, 0, 2, 0], 0);
    exec(&mut doc, "text.insert", json!({ "text": "Note" }));
    caret(&mut doc, &[table, 0, 0, 0], 0);
    exec(&mut doc, "format.bold", json!({}));
    let inspect = exec(&mut doc, "file.inspect", json!({}));
    let block = &inspect["blocks"][table];
    assert_eq!((block["rows"].clone(), block["cols"].clone()), (json!(3), json!(3)), "{block}");
    assert_eq!(block["cells"][0], json!(["Item", "Price", "Note"]), "{block}");
    assert_eq!(block["cells"][1], json!(["Tea", "3.50", ""]), "{block}");
    assert_eq!(block["cells"][2], json!(["Coffee", "", ""]), "{block}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn headers_footers_and_page_numbers_survive_save_and_reopen() {
    let mut doc = new_document();
    exec(&mut doc, "text.insert", json!({ "text": "A page with a header and a numbered footer." }));
    exec(&mut doc, "header.insert", json!({}));
    exec(&mut doc, "text.insert", json!({ "text": "Quarterly report" }));
    exec(&mut doc, "header.close", json!({}));
    // Like Word's Page Number gallery, pageNumber.insert makes the footer the page number.
    exec(&mut doc, "footer.insert", json!({}));
    exec(&mut doc, "pageNumber.insert", json!({}));
    exec(&mut doc, "header.close", json!({}));
    let inspect = exec(&mut doc, "file.inspect", json!({}));
    let part =
        |kind: &str| inspect["parts"].as_array().into_iter().flatten().find(|part| part["kind"] == json!(kind)).map(|part| part["text"].clone());
    assert_eq!(part("header"), Some(json!("Quarterly report")), "{inspect}");
    assert_eq!(part("footer"), Some(json!("1")), "{inspect}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn page_setup_survives_save_and_reopen() {
    let mut doc = new_document();
    exec(&mut doc, "text.insert", json!({ "text": "Wide pages in two columns with narrow margins." }));
    exec(&mut doc, "page.size", json!({ "width": 595, "height": 842 }));
    exec(&mut doc, "page.orientation", json!({ "value": "landscape" }));
    exec(&mut doc, "page.margins", json!({ "top": 36, "bottom": 36, "left": 54, "right": 54 }));
    exec(&mut doc, "page.columns", json!({ "count": 2 }));
    let inspect = exec(&mut doc, "file.inspect", json!({}));
    let section = &inspect["sections"][0]["props"];
    assert_eq!(section["landscape"], json!(true), "{section}");
    assert_eq!((section["pageW"].clone(), section["pageH"].clone()), (json!(842.0), json!(595.0)), "{section}");
    assert_eq!(
        [&section["marginTop"], &section["marginBottom"], &section["marginLeft"], &section["marginRight"]].map(Value::clone),
        [json!(36.0), json!(36.0), json!(54.0), json!(54.0)],
        "{section}"
    );
    assert_eq!(section["columns"]["count"], json!(2), "{section}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn typed_breaks_and_tabs_survive_save_and_reopen() {
    let mut doc = new_document();
    exec(&mut doc, "text.insert", json!({ "text": "Name" }));
    exec(&mut doc, "text.tab", json!({}));
    exec(&mut doc, "text.insert", json!({ "text": "Value" }));
    exec(&mut doc, "text.lineBreak", json!({}));
    exec(&mut doc, "text.insert", json!({ "text": "Second line of the same paragraph" }));
    exec(&mut doc, "text.newParagraph", json!({}));
    exec(&mut doc, "text.insert", json!({ "text": "Before the break" }));
    exec(&mut doc, "text.pageBreak", json!({}));
    exec(&mut doc, "text.insert", json!({ "text": "On the next page" }));
    let inspect = exec(&mut doc, "file.inspect", json!({}));
    let first = inspect["blocks"][0]["text"].as_str().unwrap().to_owned();
    assert!(first.contains('\t') && first.contains('\n'), "{first:?}");
    assert!(inspect["pages"].as_u64().unwrap() >= 2, "{inspect}");
    assert_survives_save_and_reopen(&mut doc);
}
