//! `kition-doc` opens a document from its replica (`model.open`, which `kit.json` lists): what
//! `model.export` answers for an open document, its changes and the media they reference, opens
//! again as the same document, with no file. An agent's headless draft is such a replica (KHP
//! A12); the kit listed `model.open` but refused it, so its drafts fell back to copies of the file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use kition_doc_kit::DocDocument;
use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use serde_json::{Map, Value, json};

const MANIFEST: &str = include_str!("../../../kit.json");

/// A 2x2 PNG, one red, one green, one blue and one white pixel.
const PICTURE: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEklEQVR4nGP4z8DAAMIM/4EAAB/uBfsL2WiLAAAAAElFTkSuQmCC";

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// The document's blocks as an agent reads them: text, runs and the objects in each paragraph.
fn blocks(kit: &mut Headless<DocDocument>, id: &str) -> Value {
    let inspected = kit.execute(id, "document.inspect", params(json!({ "text": true })), Origin::Agent).unwrap();
    let result = inspected.result.unwrap_or_default();
    result.get("blocks").cloned().unwrap_or(result)
}

/// The first page as a PNG, drawn by the kit's own export (the picture's pixels included).
fn page(kit: &mut Headless<DocDocument>, id: &str) -> Value {
    let exported = kit.execute(id, "file.export", params(json!({ "format": "png" })), Origin::Agent).unwrap();
    exported.result.unwrap_or_default()["data"].clone()
}

#[test]
fn a_document_opens_from_its_exported_replica_with_its_media() {
    let root = std::env::temp_dir().join(format!("kition-doc-replica-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    // A picture comes in a file (the agent's tools name no files, so it cannot insert one).
    let mut document =
        DocDocument::open("basic.docx", &std::fs::read(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.docx"))).unwrap())
            .unwrap();
    document.execute("text.insert", json!({ "text": "Before the picture. " })).unwrap();
    document.execute("insert.picture", json!({ "data": PICTURE, "width": 72, "alt": "Four dots" })).unwrap();
    std::fs::write(root.join("basic.docx"), document.save(None).unwrap()).unwrap();

    let mut kit = Headless::<DocDocument>::with_replica_port(KitInfo::from_manifest(MANIFEST, &[], &[]).unwrap());
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let file = kit.open(OpenRequest { file: &folder.file("basic.docx").unwrap(), read_only: false, create: None }).unwrap().id;
    kit.execute(&file, "text.insert", params(json!({ "text": "Typed by the agent. " })), Origin::Agent).unwrap();
    let before = blocks(&mut kit, &file);
    let text = before.to_string();
    assert!(text.contains("Before the picture.") && text.contains("Typed by the agent.") && text.contains("Four dots"), "{before}");
    let pixels = page(&mut kit, &file);
    assert!(pixels.as_str().is_some_and(|data| !data.is_empty()), "{pixels}");

    let exported = kit.model("model.export", json!({ "documentId": file })).unwrap();
    let fields: Vec<&String> = exported.as_object().map(|answer| answer.keys().collect()).unwrap_or_default();
    assert!(exported["blobs"].is_string(), "the picture travels with the changes, but `model.export` answered only {fields:?}");
    let opened = kit
        .model("model.open", json!({ "name": "basic.docx", "snapshot": exported["changes"], "blobs": exported["blobs"] }))
        .unwrap_or_else(|error| panic!("model.open: {}", error.message));
    let replica = opened["documentId"].as_str().unwrap().to_owned();
    assert_ne!(replica, file);
    assert_eq!(blocks(&mut kit, &replica), before, "the replica is the document");
    assert_eq!(page(&mut kit, &replica), pixels, "the replica draws the same page, picture and all");

    // The replica takes edits of its own and leaves the file's document alone.
    kit.execute(&replica, "text.insert", params(json!({ "text": "Only in the draft. " })), Origin::Agent).unwrap();
    assert_eq!(blocks(&mut kit, &file), before);
    assert_ne!(blocks(&mut kit, &replica), before);
    let saved = kit.model("model.save", json!({ "documentId": replica })).unwrap();
    assert!(saved["data"].as_str().is_some_and(|data| !data.is_empty()), "{saved}");
    let _ = std::fs::remove_dir_all(root);
}
