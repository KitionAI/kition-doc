//! C8-land-14-doc item 3: the call context. The editor state the kit lists is all a command reads
//! besides its parameters: setting and restoring it changes nothing in the document; no command
//! depends on the page the caret was last seen on; the commands that declare no `reads` give the
//! same result whatever the selection is.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_doc_kit::DocDocument;
use kition_doc_kit::context::EDITOR_STATE;
use kition_kit::{CallContext, KitDocument};
use serde_json::{Value, json};

/// The clock the page test runs on (`SOURCE_DATE_EPOCH`): a comment's or revision's date, and the
/// durable comment id minted from it, are then the same in runs that fall in different seconds.
const EPOCH: &str = "1700000000";

/// Ten pages of numbered paragraphs, each with a bold word, and a 2×2 table.
fn fixture() -> Vec<u8> {
    let mut doc = DocDocument::create("a.docx", 612, 792).unwrap();
    for n in 0..120 {
        if n > 0 {
            doc.execute("text.newParagraph", json!({})).unwrap();
        }
        doc.execute("text.insert", json!({ "text": format!("Paragraph {n} says something about the matter at hand, at length.") })).unwrap();
    }
    doc.execute("select.range", json!({ "anchor": { "block": 3, "off": 0 }, "focus": { "block": 3, "off": 9 } })).unwrap();
    doc.execute("format.bold", json!({})).unwrap();
    doc.execute("select.range", json!({ "anchor": { "block": 5, "off": 0 }, "focus": { "block": 5, "off": 0 } })).unwrap();
    doc.execute("table.insert", json!({ "rows": 2, "cols": 2 })).unwrap();
    doc.save(None).unwrap()
}

fn open(bytes: &[u8]) -> DocDocument {
    DocDocument::open("a.docx", bytes).unwrap()
}

fn body_ids(doc: &DocDocument) -> Vec<String> {
    doc.inspect().unwrap()["blocks"].as_array().unwrap().iter().filter_map(|b| b["id"].as_str().map(str::to_owned)).collect()
}

/// The document as data, and what the command answered.
fn outcome(doc: &mut DocDocument, command: &str, params: &Value) -> (String, String) {
    let answer = match doc.execute(command, params.clone()) {
        Ok(value) => value.to_string(),
        Err(error) => format!("error: {error}"),
    };
    // The document without its save stamp and dates (a save writes the time).
    fn undated(value: &mut Value) {
        match value {
            Value::Object(map) => {
                map.retain(|key, _| !matches!(key.as_str(), "core" | "date" | "dateUtc" | "created" | "modified"));
                map.values_mut().for_each(undated);
            }
            Value::Array(items) => items.iter_mut().for_each(undated),
            _ => {}
        }
    }
    let mut data: Value = serde_json::from_slice(&doc.save(Some("a.json")).unwrap()).unwrap();
    undated(&mut data);
    (answer, data.to_string())
}

fn state(selection: Value, page: usize) -> Value {
    json!({ "positions": selection, "also": [], "pending": null, "bodyReturn": null, "goalX": null, "page": page })
}

fn sel(block: u32, from: usize, to: usize) -> Value {
    json!({ "anchor": { "story": "body", "path": [block], "off": from }, "focus": { "story": "body", "path": [block], "off": to } })
}

#[test]
fn setting_and_restoring_the_context_changes_nothing_in_the_document() {
    let bytes = fixture();
    let mut doc = open(&bytes);
    doc.restore_editor_state(&state(sel(7, 2, 9), 3)).unwrap();
    let ids = body_ids(&doc);
    for on in [vec![json!(ids[10])], vec![json!(ids[1]), json!(ids[2])]] {
        kition_kit::testing::check_context(&mut doc, &CallContext { on, part: None, time: None }, EDITOR_STATE).unwrap();
    }
}

#[test]
fn on_selects_the_named_paragraphs_whole() {
    let bytes = fixture();
    let mut doc = open(&bytes);
    let ids = body_ids(&doc);
    doc.set_context(&CallContext { on: vec![json!(ids[2]), json!(ids[4])], part: None, time: None }).unwrap();
    let selection = doc.execute("file.selection", json!({})).unwrap();
    assert_eq!(selection["anchor"]["path"], json!([2]), "{selection}");
    assert_eq!(selection["anchor"]["off"], 0);
    assert_eq!(selection["focus"]["path"], json!([4]));
    assert!(doc.set_context(&CallContext { on: vec![json!("pFFFFFFF0")], part: None, time: None }).is_err());
    assert!(doc.set_context(&CallContext { on: vec![], part: Some(json!({ "page": 2 })), time: None }).is_err());
}

#[test]
fn no_command_depends_on_the_page_the_caret_was_last_seen_on() {
    // A test cannot set an environment variable in its own process (that is `unsafe`), so it runs
    // this test binary again with the clock pinned and only this test selected.
    if std::env::var("SOURCE_DATE_EPOCH").as_deref() != Ok(EPOCH) {
        let name = "no_command_depends_on_the_page_the_caret_was_last_seen_on";
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env("SOURCE_DATE_EPOCH", EPOCH)
            .output()
            .unwrap();
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success(), "{stdout}\n{stderr}");
        assert!(stdout.contains("1 passed"), "the inner test ran: {stdout}");
        println!("{stdout}");
        return;
    }
    let bytes = fixture();
    let commands: Vec<String> = open(&bytes).commands().into_iter().map(|c| c.id).collect();
    let mut differ = Vec::new();
    let mut unstable = Vec::new();
    // `file.layout` answers where the caret is drawn (a read, no edit): for a caret at a page
    // break the page it was last seen on picks which page's line it is drawn on.
    let reads_page = ["file.layout"];
    for command in commands
        .iter()
        .filter(|id| kition_doc_engine::cmd::family::family(id) != "file" && !id.starts_with("view.") && !reads_page.contains(&id.as_str()))
    {
        for params in [json!({}), json!({ "text": "x" })] {
            let run = |page: usize| {
                let mut doc = open(&bytes);
                doc.restore_editor_state(&state(sel(60, 3, 3), page)).unwrap();
                outcome(&mut doc, command, &params)
            };
            // A command whose two runs on the same page differ (fresh random list or revision
            // ids) is compared by what it does not mint: skipped here, listed apart.
            let (a, again, b) = (run(0), run(0), run(6));
            if a != again {
                unstable.push(format!("{command} {params}"));
            } else if a != b {
                differ.push(format!("{command} {params}"));
            }
        }
    }
    assert!(differ.is_empty(), "these read the current page:\n{}", differ.join("\n"));
    println!("not deterministic, not compared: {unstable:?}");
}

#[test]
fn the_commands_that_declare_no_reads_give_the_same_result_whatever_the_selection() {
    let bytes = fixture();
    let doc = open(&bytes);
    let free: Vec<String> = doc.commands().into_iter().filter(|c| c.reads.is_empty()).map(|c| c.id).collect();
    assert!(!free.is_empty());
    let calls = [
        ("select.range", json!({ "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 4 } })),
        ("caret.set", json!({ "pos": { "story": "body", "path": [2], "off": 3 } })),
        ("file.text", json!({})),
        ("file.paragraph", json!({ "path": [3] })),
        ("page.color", json!({ "color": "FFEECC" })),
        ("page.watermark", json!({ "text": "DRAFT" })),
        ("theme.effects", json!({})),
        ("theme.paragraphSpacing", json!({ "value": "double" })),
        ("theme.styleSet", json!({ "name": "lines" })),
        ("theme.set", json!({ "name": "Studio" })),
        ("theme.colors", json!({ "name": "Gallery" })),
        ("theme.fonts", json!({ "heading": "Georgia" })),
        ("style.addToGallery", json!({ "style": "Heading1", "value": true })),
        ("style.set", json!({ "style": "Heading1", "name": "Agent heading" })),
        // The commands the independent reading found read no editor state (C8-doc-4), with the
        // reading's parameters.
        ("comment.reply", json!({"thread":"c0","text":"ok"})),
        ("comment.resolve", json!({"thread":"c0","resolved":false})),
        ("file.setText", json!({"text":"x\ny"})),
        ("drawing.arrange", json!({"ids":["7"],"order":"front"})),
        ("file.encrypt", json!({"password":"pw"})),
        ("file.new", json!({"template":"letter"})),
        ("file.properties", json!({"title":"T"})),
        ("file.protect", json!({"mode":"readOnly"})),
        ("page.hyphenation", json!({})),
        ("page.differentOddEven", json!({ "value": true })),
        ("mailMerge.envelopes", json!({"delivery":"Jo\n1 St"})),
        ("mailMerge.findRecipient", json!({"text":"ada"})),
        ("mailMerge.finish", json!({})),
        ("mailMerge.labels", json!({"text":"Hello"})),
        ("mailMerge.next", json!({})),
        ("mailMerge.preview", json!({})),
        ("mailMerge.previous", json!({})),
        ("paragraph.insert", json!({"after":2,"text":"Mid"})),
        ("footnote.options", json!({"footnoteFormat":"lowerRoman"})),
        ("citation.sources", json!({})),
        ("file.compare", json!({"text":"Title\nAlpha beta delta."})),
        ("comment.edit", json!({"id":1,"text":"a\nb"})),
        ("comment.replyCurrent", json!({"id":0,"text":"ok"})),
        ("file.restrictEditing", json!({"mode":"comments"})),
        ("change.track", json!({})),
        ("style.delete", json!({"style":"Mine"})),
        ("style.setVisibility", json!({"style":"Heading 1","gallery":false})),
    ];
    for id in &free {
        let params = calls.iter().find(|(command, _)| command == id).map(|(_, params)| params.clone()).unwrap_or_else(|| panic!("{id}: no call"));
        let mut a = open(&bytes);
        let mut b = open(&bytes);
        a.restore_editor_state(&state(sel(8, 0, 12), 0)).unwrap();
        // The second run also has pending formatting at the caret (what typing takes).
        let mut other = state(
            json!({ "anchor": { "story": "body", "path": [5, 0, 1, 0], "off": 0 }, "focus": { "story": "body", "path": [5, 0, 1, 0], "off": 0 } }),
            0,
        );
        other["pending"] = json!({ "bold": true });
        b.restore_editor_state(&other).unwrap();
        let echo = kition_doc_engine::cmd::family::family(id) == "design"
            || matches!(id.as_str(), "style.addToGallery" | "style.set" | "page.differentOddEven");
        let selection_echo = |doc: &mut DocDocument| {
            let selection = doc.execute("file.selection", json!({})).unwrap();
            json!({ "anchor": selection["anchor"], "focus": selection["focus"] }).to_string()
        };
        let (echo_a, echo_b) = (selection_echo(&mut a), selection_echo(&mut b));
        let (answer_a, data_a) = outcome(&mut a, id, &params);
        let (answer_b, data_b) = outcome(&mut b, id, &params);
        assert_eq!(data_a, data_b, "{id} changed the document differently by the selection");
        // The selection a command answers is the one it set, not one it read.
        if echo {
            // C8-doc-7: metadata writes echo the unchanged selection for the UI. Check
            // both full answers exactly; their document effects read no selection.
            assert_eq!(answer_a, echo_a, "{id} must echo the first selection unchanged");
            assert_eq!(answer_b, echo_b, "{id} must echo the second selection unchanged");
        } else {
            assert_eq!(answer_a, answer_b, "{id} answered differently by the selection");
        }
    }
}

/// C8-doc-6: the session's mail merge list is the kit's external input `mergeSource`: no stored
/// field holds it, a person sets it (Select Recipients is not the agent's), and the merge commands
/// read it. Its fingerprint follows its rows, and the commands that read it name it in `under`,
/// so the guard refuses an agent's step when a person changed the list since the agent's basis.
#[test]
fn the_mail_merge_list_is_an_external_input_the_merge_commands_rest_on() {
    let mut doc = DocDocument::create("a.docx", 612, 792).unwrap();
    doc.execute("text.insert", json!({ "text": "Dear " })).unwrap();
    let fingerprint = |doc: &DocDocument| doc.external_inputs().unwrap().into_iter().find(|i| i.name == "mergeSource").and_then(|i| i.fingerprint);
    let empty = fingerprint(&doc);
    assert!(empty.is_some(), "the list is always observable");
    doc.execute("mailMerge.recipients", json!({ "rows": [{ "Name": "Ada" }, { "Name": "Bo" }] })).unwrap();
    let ada = fingerprint(&doc);
    assert_ne!(ada, empty);
    doc.execute("mailMerge.recipients", json!({ "rows": [{ "Name": "Cy" }] })).unwrap();
    assert_ne!(fingerprint(&doc), ada, "another list, another fingerprint");
    for (command, params) in [("mailMerge.next", json!({})), ("mailMerge.finish", json!({})), ("mailMerge.labels", json!({}))] {
        let under = doc.under(command, &params).unwrap().unwrap_or_default();
        assert_eq!(under.inputs, vec!["mergeSource".to_owned()], "{command}");
    }
    // Labels and envelopes given their text read no list.
    let under = doc.under("mailMerge.labels", &json!({ "text": "Hello" })).unwrap().unwrap_or_default();
    assert!(under.inputs.is_empty());
}
