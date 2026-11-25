//! C8-doc-pin: a file reread keeps resolving editor state and retained undo history.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use base64::Engine;
use kition_doc_kit::DocDocument;
use kition_kit::KitDocument;
use serde_json::json;

fn fixture() -> DocDocument {
    let mut doc = DocDocument::create("reread.docx", 612, 792).unwrap();
    for (index, text) in ["First paragraph", "Second paragraph", "Third paragraph"].into_iter().enumerate() {
        if index > 0 {
            doc.execute("text.newParagraph", json!({})).unwrap();
        }
        doc.execute("text.insert", json!({ "text": text })).unwrap();
    }
    DocDocument::open("reread.docx", &doc.save(None).unwrap()).unwrap()
}

fn select(doc: &mut DocDocument, block: usize, off: usize) {
    let pos = json!({ "block": block, "off": off });
    doc.execute("select.range", json!({ "anchor": pos, "focus": pos })).unwrap();
}

#[test]
fn a_call_restores_the_exact_editor_snapshot() {
    let mut doc = fixture();
    let state = doc.editor_state();
    select(&mut doc, 2, 4);
    doc.execute("text.insert", json!({ "text": "Peer" })).unwrap();
    doc.restore_editor_state(&state).unwrap();
    assert_eq!(doc.editor_state(), state);
}

fn reconcile(doc: &mut DocDocument, peer: &DocDocument) {
    let mut fresh = DocDocument::open("reread.docx", &peer.save(None).unwrap()).unwrap();
    fresh.carry_replica_objects(doc).unwrap();
    fresh.restore_editor_state(&doc.editor_state()).unwrap();
    assert!(doc.reconcile_reread(&fresh).unwrap());
    doc.mark_saved();
}

#[test]
fn a_reread_follows_the_selected_paragraph_and_preserves_pending_formatting() {
    let mut doc = fixture();
    select(&mut doc, 1, 3);
    doc.execute("format.bold", json!({ "value": true })).unwrap();
    let pending = doc.editor_state()["pending"].clone();
    let mut peer = DocDocument::open("reread.docx", &doc.save(None).unwrap()).unwrap();
    peer.execute("paragraph.insert", json!({ "before": 0, "text": "Outside" })).unwrap();
    reconcile(&mut doc, &peer);
    let state = doc.editor_state();
    assert_eq!(state["positions"]["anchor"]["path"], json!([2]));
    assert_eq!(state["positions"]["anchor"]["off"], 3);
    assert_eq!(state["pending"], pending);
}

#[test]
fn a_reread_follows_text_inserted_before_the_caret() {
    let mut doc = fixture();
    select(&mut doc, 1, 4);
    let mut peer = DocDocument::open("reread.docx", &doc.save(None).unwrap()).unwrap();
    select(&mut peer, 1, 0);
    peer.execute("text.insert", json!({ "text": "New " })).unwrap();
    reconcile(&mut doc, &peer);
    assert_eq!(doc.editor_state()["positions"]["anchor"]["off"], 8);
}

#[test]
fn a_reread_retains_new_picture_bytes() {
    let mut doc = fixture();
    let mut peer = DocDocument::open("reread.docx", &doc.save(None).unwrap()).unwrap();
    let picture = image::RgbaImage::from_pixel(2, 2, image::Rgba([40, 120, 200, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    let data = bytes.into_inner();
    select(&mut peer, 2, 0);
    peer.execute("image.insert", json!({ "data": base64::engine::general_purpose::STANDARD.encode(&data), "width": 40 })).unwrap();
    reconcile(&mut doc, &peer);
    let saved = doc.save(None).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(saved)).unwrap();
    let mut found = false;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        if entry.name().starts_with("word/media/") {
            let mut stored = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut stored).unwrap();
            if stored == data {
                found = true;
            }
        }
    }
    assert!(found, "the reread lost the new picture's bytes");
}

#[test]
fn a_deleted_active_paragraph_refuses_restoration_without_changing_the_live_session() {
    let mut doc = fixture();
    select(&mut doc, 1, 2);
    let state = doc.editor_state();
    let before = doc.save(Some("reread.json")).unwrap();
    let mut peer = DocDocument::open("reread.docx", &doc.save(None).unwrap()).unwrap();
    peer.execute("select.range", json!({ "anchor": { "block": 0, "off": 15 }, "focus": { "block": 1, "off": 16 } })).unwrap();
    peer.execute("text.delete", json!({})).unwrap();
    let mut fresh = DocDocument::open("reread.docx", &peer.save(None).unwrap()).unwrap();
    fresh.carry_replica_objects(&doc).unwrap();
    assert!(fresh.restore_editor_state(&state).unwrap_err().contains("deleted active paragraph"));
    assert_eq!(doc.editor_state(), state);
    assert_eq!(doc.save(Some("reread.json")).unwrap(), before);
}
