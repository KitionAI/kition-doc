//! `file.export`: the File tab's Export buttons and the agent's `doc_export` take the file's bytes
//! from the engine as base64 and hand them to the host, which writes the new file (Kit UI protocol
//! `host/write` mode `export`, KHP A41). The engine writes nothing and the document stays as it was.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use base64::Engine as _;
use kition_doc_kit::DocDocument;
use kition_kit::KitDocument;
use serde_json::{Value, json};

fn document() -> DocDocument {
    let mut doc = DocDocument::create("Report.docx", 612, 792).unwrap();
    doc.execute("text.insert", json!({ "text": "Quarterly report" })).unwrap();
    doc
}

fn bytes(answer: &Value) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD.decode(answer["data"].as_str().unwrap()).unwrap()
}

#[test]
fn exports_the_document_as_pdf_bytes_without_changing_it() {
    let mut doc = document();
    let (revision, dirty, saved) = (doc.revision(), doc.dirty(), doc.save(None).unwrap());
    let answer = doc.execute("file.export", json!({ "format": "pdf" })).unwrap();
    let pdf = bytes(&answer);
    assert!(pdf.starts_with(b"%PDF-"), "{:?}", &pdf[..8.min(pdf.len())]);
    assert_eq!(answer["format"], "pdf");
    assert_eq!(answer["extension"], "pdf");
    assert_eq!(answer["mime"], "application/pdf");
    assert_eq!(answer["bytes"], pdf.len());
    assert!(answer.get("path").is_none(), "the host names and writes the file");
    assert_eq!((doc.revision(), doc.dirty()), (revision, dirty));
    assert_eq!(doc.save(None).unwrap(), saved);
}

#[test]
fn exports_one_page_as_a_png_at_a_scale() {
    let mut doc = document();
    let answer = doc.execute("file.export", json!({ "format": "png", "page": 1, "scale": 1 })).unwrap();
    let png = bytes(&answer);
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!((answer["mime"].as_str(), answer["extension"].as_str()), (Some("image/png"), Some("png")));
    // A Letter page at 1 pixel per point.
    assert_eq!((answer["width"].as_u64(), answer["height"].as_u64()), (Some(612), Some(792)));
    let decoded = image::load_from_memory(&png).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (612, 792));
    let double = doc.execute("file.export", json!({ "format": "png" })).unwrap();
    assert_eq!(double["width"].as_u64(), Some(1224), "2 pixels per point by default");
}

#[test]
fn refuses_a_format_or_page_it_cannot_write() {
    let mut doc = document();
    let error = doc.execute("file.export", json!({ "format": "docx" })).unwrap_err();
    assert!(error.contains("pdf") && error.contains("png"), "{error}");
    let error = doc.execute("file.export", json!({ "format": "png", "page": 9 })).unwrap_err();
    assert!(error.contains("no page 9"), "{error}");
    assert!(doc.execute("file.export", json!({})).is_err(), "a format is required");
    assert!(doc.execute("file.export", json!({ "format": "pdf", "path": "/tmp/x.pdf" })).is_err(), "no file system");
}

/// kit.json declares what `file.export` writes, for the host's export check and the agent's
/// `doc_export` (`exports`, `agent.export`).
#[test]
fn the_manifest_declares_the_exports() {
    let manifest: Value = serde_json::from_str(include_str!("../../../kit.json")).unwrap();
    let exports: Vec<(&str, &str, &str)> = manifest["exports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| (entry["extension"].as_str().unwrap(), entry["mime"].as_str().unwrap(), entry["command"].as_str().unwrap()))
        .collect();
    assert_eq!(exports, [("pdf", "application/pdf", "file.export"), ("png", "image/png", "file.export")]);
    assert_eq!(manifest["agent"]["export"], json!({ "command": "file.export", "data": "data" }));
    let mut doc = document();
    for (extension, mime, command) in exports {
        let answer = doc.execute(command, json!({ "format": extension })).unwrap();
        assert_eq!((answer["extension"].as_str(), answer["mime"].as_str()), (Some(extension), Some(mime)));
        assert!(!bytes(&answer).is_empty());
    }
}
