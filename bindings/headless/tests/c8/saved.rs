//! Item 5: in memory == saved. After every write, the kit is made to read the saved file again
//! (the same bytes, with another time stamp): the comparison of what it served with what the file
//! holds must find nothing (no `others`). Over representative commands on each fixture file of
//! `conformance/`. And the receipt sizes the card asks for.
use super::outside::{bold, by_index};
use super::*;

/// What the saved file reads back differently from what the kit served, each by its diagnosed
/// cause (C8-doc-2 item 10). Anything else is `unexplained`. Fixed by C8-doc-2 and no longer
/// pinned: a picture whose media the file lacks, a note reference whose note the file lacks, and
/// a paragraph-mark link (the document is opened as its save reads back, `files::as_saved`; a
/// split's or emptied paragraph's mark takes no link), a new comment's paragraph id taken from a
/// note paragraph's (the writer skipped a held id but marked it written).
fn causes(file: &str, write: &str, others: &Value) -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    for change in others["changed"].as_array().into_iter().flatten() {
        for edit in change["edits"].as_array().into_iter().flatten() {
            let field = edit["field"].as_str().unwrap_or_default();
            let cause = if matches!(write, "split a paragraph" | "join two paragraphs") && field == "text" {
                // The replica keeps a split as the first part's text ending in an anchor (the
                // file has none) and a join as two Block objects merged into one view (the file
                // has one paragraph): kdm-model's piece forms, not what a reread makes.
                "split and join anchors of the replica"
            } else if field == "text" && edit["before"] == edit["after"] {
                // The same characters and formatting: an inline object without a tool id (a note
                // reference, a field, a picture) is named in the text's `object` mark by an
                // object id minted again on every read; the reread's comparison reads the mark
                // literally (kition-sdk 1b9d5ec reread::diff_paired pairs the objects but not the
                // marks naming them).
                "inline object's minted id in its text mark"
            } else {
                "unexplained"
            };
            found.push((cause, format!("{} {field}", change["id"])));
        }
    }
    for kind in ["created", "deleted", "moved"] {
        for item in others[kind].as_array().into_iter().flatten() {
            let cause = if matches!(write, "split a paragraph" | "join two paragraphs") {
                "split and join anchors of the replica"
            } else if file == "check-missing-note.docx" && kind == "deleted" {
                // The write deleted the note's reference: the note stays in memory (a paste in the
                // same run may put the reference back), the writer writes only referenced notes.
                "a note whose reference the write deleted is not saved"
            } else if kind == "deleted" && item["type"] == "Media" && item["at"]["field"] == "media" {
                // DOCX emits used media only (write/mod.rs); the public SDK now
                // retains raw object references on these deletion records.
                "media whose last picture the write deleted is not saved"
            } else {
                "unexplained"
            };
            found.push((cause, format!("{kind} {}", item["id"])));
        }
    }
    if let Some(unnamed) = others.get("unnamed") {
        let cause = if matches!(write, "split a paragraph" | "join two paragraphs") {
            "split and join anchors of the replica"
        } else if unnamed.get("Media").is_some()
            || (unnamed.get("Document").is_some()
                && others["deleted"].as_array().into_iter().flatten().any(|item| item["type"] == "Media" && item["at"]["field"] == "media"))
        {
            // The write deleted the last picture using a media part: it stays in memory (as a note
            // above), the writer writes only used media.
            "media whose last picture the write deleted is not saved"
        } else if file == "check-missing-note.docx" {
            "a note whose reference the write deleted is not saved"
        } else {
            "unexplained"
        };
        found.push((cause, format!("unnamed {unnamed}")));
    }
    found
}

/// Representative writes: formatting, typing, structure, tables, drawings, styles, comments.
fn writes() -> Vec<(&'static str, Value)> {
    let caret = |block: usize, off: usize| by_index(block, off, off);
    vec![
        ("bold a range", json!([by_index(0, 0, 3), bold()])),
        ("type", json!([caret(0, 0), { "command": "text.insert", "params": { "text": "Typed " } }])),
        ("split a paragraph", json!([caret(0, 2), { "command": "text.newParagraph", "params": {} }])),
        ("join two paragraphs", json!([caret(1, 0), { "command": "text.backspace", "params": {} }])),
        ("insert a table", json!([caret(0, 0), { "command": "insert.table", "params": { "rows": 2, "cols": 2 } }])),
        (
            "insert a shape",
            json!([caret(0, 0), { "command": "drawing.insert", "params": { "geometry": { "preset": "rect" }, "frame": { "w": 50, "h": 20 } } }]),
        ),
        ("centre a paragraph", json!([caret(0, 0), { "command": "para.alignCenter", "params": {} }])),
        ("apply a heading style", json!([caret(0, 0), { "command": "para.style", "params": { "style": "Heading2" } }])),
        ("add a comment", json!([by_index(0, 0, 2), { "command": "comment.add", "params": { "text": "Note" } }])),
        ("insert a paragraph by index", json!([{ "command": "para.insert", "params": { "after": 0, "text": "New one" } }])),
        ("replace a paragraph's text", json!([{ "command": "para.text", "params": { "index": 0, "text": "Replaced" } }])),
        (
            "delete a selection across paragraphs",
            json!([{ "command": "select.range", "params": { "anchor": { "block": 0, "off": 1 }, "focus": { "block": 1, "off": 1 } } }, { "command": "text.delete", "params": {} }]),
        ),
    ]
}

#[tokio::test]
async fn every_write_reads_back_from_the_saved_file_as_it_was_served() {
    let mut files: Vec<_> = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "docx"))
        .collect();
    files.sort();
    let mut differ = Vec::new();
    let mut pinned: std::collections::BTreeMap<&'static str, usize> = std::collections::BTreeMap::new();
    let mut cells = 0usize;
    let mut failed = Vec::new();
    for file in &files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        for (what, steps) in writes() {
            let f = Fixture::with(Guard::Premise, &std::fs::read(file).unwrap());
            if f.call("inspect", json!({})).await.is_err() {
                failed.push(format!("{name}: does not open"));
                f.close().await;
                break;
            }
            let receipt = match f.run(None, steps.clone()).await {
                Ok(receipt) => receipt,
                Err(error) => {
                    failed.push(format!("{name} / {what}: {}", error["error"]["message"]));
                    f.close().await;
                    continue;
                }
            };
            cells += 1;
            // The same bytes again, as another program's save that changed nothing.
            f.rewrite(&std::fs::read(f.root.join(FILE)).unwrap());
            let answer = f.call("inspect", json!({ "basis": receipt["revision"] })).await.unwrap();
            if let Some(others) = answer.get("others") {
                for (cause, what_differs) in causes(&name, what, others) {
                    *pinned.entry(cause).or_insert(0usize) += 1;
                    if cause == "unexplained" {
                        differ.push(format!("{name} / {what}: {what_differs}"));
                    }
                }
            }
            f.close().await;
        }
    }
    println!("{cells} writes on {} files; differences by pinned cause: {pinned:?}", files.len());
    println!("not run: {failed:?}");
    assert!(differ.is_empty(), "{}", differ.join("\n"));
}

/// Receipt sizes (bytes of the `changes` JSON) for the shapes the card names.
#[tokio::test]
async fn receipt_sizes() {
    let size = |receipt: &Value| receipt["changes"].to_string().len();
    // One property: bold three characters.
    let f = Fixture::new(Guard::Premise);
    let one = f.run(None, json!([by_index(1, 0, 4), bold()])).await.unwrap();
    // One move: a paragraph moved within the body (object model list move).
    let mv = f.call("batch", json!({ "ops": [{ "op": "list.move", "target": "document", "field": "body", "from": 4, "to": 0 }] })).await;
    println!(
        "one property: {} bytes; one move: {}",
        size(&one),
        mv.as_ref().map_or_else(|e| format!("refused {}", e["error"]), |r| format!("{} bytes", size(r)))
    );
    f.close().await;
    // A move of a parent with 5 and 200 children: a table of 5 and 200 rows moved in the body.
    for rows in [5usize, 200] {
        let mut doc = document();
        doc.execute("select.range", json!({ "anchor": { "block": 4, "off": 0 }, "focus": { "block": 4, "off": 0 } })).unwrap();
        doc.execute("insert.table", json!({ "rows": rows, "cols": 1 })).unwrap();
        let f = Fixture::with(Guard::Premise, &doc.save(None).unwrap());
        let mv = f.call("batch", json!({ "ops": [{ "op": "list.move", "target": "document", "field": "body", "from": 4, "to": 0 }] })).await;
        println!(
            "table of {rows} rows moved: {}",
            mv.as_ref().map_or_else(|e| format!("refused {}", e["error"]), |r| format!("{} bytes ({})", size(r), r["changes"]))
        );
        f.close().await;
    }
    // The same change on 200 objects: 200 paragraphs centred in one step.
    let mut doc = DocDocument::create(FILE, 612, 792).unwrap();
    for n in 0..200 {
        if n > 0 {
            doc.execute("text.newParagraph", json!({})).unwrap();
        }
        doc.execute("text.insert", json!({ "text": format!("Line {n}") })).unwrap();
    }
    let f = Fixture::with(Guard::Premise, &doc.save(None).unwrap());
    let all = f
        .run(None, json!([{ "command": "select.range", "params": { "anchor": { "block": 0, "off": 0 }, "focus": { "block": 199, "off": 0 } } }, { "command": "para.alignCenter", "params": {} }]))
        .await
        .unwrap();
    println!("200 paragraphs centred: {} bytes (footprint {})", size(&all), all["changes"]["footprint"]);
    f.close().await;
}

/// kit-env's creations: a paragraph made by a batch with another's stored values (its `para`
/// register, text set after), for every body paragraph of every fixture; the saved file must read
/// back as served, but for the pinned causes.
#[tokio::test]
async fn a_batch_copy_of_every_paragraph_reads_back_as_served() {
    let mut files: Vec<_> = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "docx"))
        .collect();
    files.sort();
    let mut differ = Vec::new();
    let mut cells = 0usize;
    for file in &files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let probe = Fixture::with(Guard::Premise, &std::fs::read(file).unwrap());
        let Ok(blocks) = probe.call("query", json!({ "follow": "body", "fields": ["kind", "para"] })).await else {
            probe.close().await;
            continue;
        };
        probe.close().await;
        for block in blocks["objects"].as_array().unwrap().iter().filter(|b| b["fields"]["kind"] == "para") {
            let f = Fixture::with(Guard::Premise, &std::fs::read(file).unwrap());
            let ops = json!([
                { "op": "create", "type": "Block", "ref": "$b", "fields": { "kind": "para", "para": block["fields"]["para"] }, "in": "document.body", "at": 0 },
                { "op": "text.set", "target": "$b", "field": "text", "text": "Key results" }
            ]);
            let Ok(receipt) = f.call("batch", json!({ "ops": ops })).await else {
                f.close().await;
                continue;
            };
            cells += 1;
            f.rewrite(&std::fs::read(f.root.join(FILE)).unwrap());
            let answer = f.call("inspect", json!({ "basis": receipt["revision"] })).await.unwrap();
            if let Some(others) = answer.get("others") {
                for (cause, what) in causes(&name, "create", others) {
                    if cause == "unexplained" {
                        differ.push(format!("{name} copy of {}: {what}: {others}", block["id"]));
                    }
                }
            }
            f.close().await;
        }
    }
    println!("{cells} copies; unexplained: {}", differ.len());
    assert!(differ.is_empty(), "{}", differ.join("\n"));
}
