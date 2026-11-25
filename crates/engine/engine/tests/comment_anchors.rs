//! Comment anchors for a host that keeps comment threads itself (Kit UI protocol 2.6 section 7k,
//! capability `comment-anchors`; feature `doc.review.host-comments`): the selection becomes an
//! anchor the host stores (`comment.anchor`), and the host's anchor is found again in this view
//! (`comment.locate`) through typing before it and inside it, new and deleted paragraphs, page
//! breaks, Word's own comments, a co-editor's edits, and without a replica.

use kition_doc_engine::doc::{Document, Pos};
use kition_doc_engine::{Session, Version};
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.run(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A document of three paragraphs, with its replica, as a host session has.
fn new_session() -> Session {
    let mut s = Session::new(Document::new());
    for (i, t) in ["Alpha paragraph one.", "Beta paragraph two, with a quoted passage here.", "Gamma paragraph three."].iter().enumerate() {
        if i > 0 {
            run(&mut s, "text.newParagraph", json!({}));
        }
        run(&mut s, "text.insert", json!({ "text": t }));
    }
    s.version().unwrap_or_else(|e| panic!("{e}"));
    s
}

fn caret_at(s: &mut Session, block: usize, off: usize) {
    let pos = serde_json::to_value(Pos::body(block, off)).unwrap_or_default();
    run(s, "caret.set", json!({ "pos": pos }));
}

fn type_at(s: &mut Session, block: usize, off: usize, text: &str) {
    caret_at(s, block, off);
    run(s, "text.insert", json!({ "text": text }));
}

/// The anchor a host stores for a comment on `text`: what `comment.anchor` answers, without the
/// view's own `at`.
fn anchor_on(s: &mut Session, text: &str) -> Value {
    run(s, "select.text", json!({ "text": text }));
    let mut anchor = run(s, "comment.anchor", json!({}));
    assert!(anchor.is_object(), "an anchor for {text:?}: {anchor}");
    assert!(anchor["at"].is_object(), "the view places the selection: {anchor}");
    if let Some(map) = anchor.as_object_mut() {
        map.remove("at");
    }
    anchor
}

fn locate(s: &mut Session, anchor: &Value) -> Value {
    let mut found = run(s, "comment.locate", json!({ "anchors": [anchor] }));
    found.as_array_mut().map(|list| list.remove(0)).unwrap_or(Value::Null)
}

fn peer(a: &mut Session) -> Session {
    let mut b = Session::new(Document::new());
    let blobs = a.blobs_since(&Version::empty()).unwrap_or_else(|e| panic!("{e}"));
    b.open_snapshot(&a.snapshot().unwrap_or_else(|e| panic!("{e}")), blobs).unwrap_or_else(|e| panic!("{e}"));
    b
}

fn send(from: &mut Session, to: &mut Session) {
    let since = to.version().unwrap_or_else(|e| panic!("{e}"));
    to.add_blobs(&from.blobs_since(&since).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("{e}"));
    to.import_changes(&from.export_changes(&since).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_selection_becomes_an_anchor_of_replica_ids_a_range_and_its_quote() {
    let mut s = new_session();
    let anchor = anchor_on(&mut s, "quoted passage");
    let ids = anchor["ids"].as_array().cloned().unwrap_or_default();
    assert_eq!(ids.len(), 1, "one paragraph: {anchor}");
    assert!(ids[0].as_str().is_some_and(|id| !id.starts_with("doc:")), "a replica id: {anchor}");
    assert_eq!(anchor["quote"], "quoted passage");
    let start = anchor["range"]["start"].as_u64().unwrap_or(u64::MAX);
    assert_eq!(start, "Beta paragraph two, with a ".chars().count() as u64);
    assert_eq!(anchor["range"]["end"].as_u64(), Some(start + "quoted passage".len() as u64));
    assert_eq!(anchor["page"], 1, "pages count from 1");
    let found = locate(&mut s, &anchor);
    assert_eq!(found["found"], true);
    assert_eq!(found["exact"], true);
    assert_eq!(found["text"], "quoted passage");
    assert_eq!(found["page"], 1);
    assert!(!found["rects"].as_array().cloned().unwrap_or_default().is_empty(), "a highlight: {found}");
    assert!(found["at"]["height"].as_f64().unwrap_or(0.0) > 0.0, "a marker's place: {found}");
}

#[test]
fn a_caret_is_no_anchor() {
    let mut s = new_session();
    caret_at(&mut s, 1, 3);
    assert_eq!(run(&mut s, "comment.anchor", json!({})), Value::Null);
}

#[test]
fn the_anchor_follows_typing_before_it_in_its_paragraph() {
    let mut s = new_session();
    let anchor = anchor_on(&mut s, "quoted passage");
    let before = locate(&mut s, &anchor)["at"]["x"].as_f64().unwrap_or(0.0);
    type_at(&mut s, 1, 0, "Some words typed first. ");
    let found = locate(&mut s, &anchor);
    assert_eq!(found["found"], true, "{found}");
    assert_eq!(found["text"], "quoted passage", "the same words: {found}");
    assert_ne!(found["at"]["x"].as_f64().unwrap_or(0.0), before, "the marker moved with the text");
}

#[test]
fn the_anchor_follows_paragraphs_added_and_deleted_before_it() {
    let mut s = new_session();
    let anchor = anchor_on(&mut s, "quoted passage");
    caret_at(&mut s, 0, 0);
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({ "text": "A new first paragraph." }));
    let found = locate(&mut s, &anchor);
    assert_eq!(
        (found["found"].clone(), found["exact"].clone(), found["text"].clone()),
        (json!(true), json!(true), json!("quoted passage")),
        "{found}"
    );
    // Delete the first two paragraphs ("A new first paragraph." and "Alpha paragraph one.").
    run(&mut s, "select.text", json!({ "text": "A new first paragraph." }));
    let pos = serde_json::to_value(Pos::body(2, 0)).unwrap_or_default();
    run(&mut s, "caret.set", json!({ "pos": pos, "extend": true }));
    run(&mut s, "text.delete", json!({}));
    let found = locate(&mut s, &anchor);
    assert_eq!((found["found"].clone(), found["text"].clone()), (json!(true), json!("quoted passage")), "{found}");
}

#[test]
fn an_edit_inside_the_quote_keeps_the_anchor_around_it() {
    let mut s = new_session();
    let anchor = anchor_on(&mut s, "quoted passage");
    run(&mut s, "select.text", json!({ "text": "quoted" }));
    run(&mut s, "select.collapse", json!({ "end": true }));
    run(&mut s, "text.insert", json!({ "text": " and much longer" }));
    let found = locate(&mut s, &anchor);
    assert_eq!(found["found"], true, "{found}");
    assert_eq!(found["exact"], false);
    assert_eq!(found["text"], "quoted and much longer passage", "{found}");
}

#[test]
fn deleted_text_leaves_the_anchor_where_it_was_and_a_deleted_paragraph_loses_it() {
    let mut s = new_session();
    let anchor = anchor_on(&mut s, "quoted passage");
    run(&mut s, "select.text", json!({ "text": "quoted passage" }));
    run(&mut s, "text.delete", json!({}));
    let found = locate(&mut s, &anchor);
    assert_eq!(found["found"], true, "its paragraph is still there: {found}");
    assert_eq!(found["exact"], false);
    assert!(!found["text"].as_str().unwrap_or("").contains("quoted"), "{found}");

    let gone = anchor_on(&mut s, "Gamma paragraph three.");
    // From the end of the paragraph before it to its end.
    caret_at(&mut s, 1, "Beta paragraph two, with a  here.".len());
    let end = serde_json::to_value(Pos::body(2, "Gamma paragraph three.".len())).unwrap_or_default();
    run(&mut s, "caret.set", json!({ "pos": end, "extend": true }));
    run(&mut s, "text.delete", json!({}));
    assert_eq!(locate(&mut s, &gone)["found"], false, "its paragraph and its words are gone");
}

#[test]
fn the_anchor_moves_to_the_next_page_with_its_text() {
    let mut s = new_session();
    let anchor = anchor_on(&mut s, "Gamma paragraph three.");
    assert_eq!(locate(&mut s, &anchor)["page"], 1);
    caret_at(&mut s, 0, 0);
    run(&mut s, "insert.pageBreak", json!({}));
    let found = locate(&mut s, &anchor);
    assert_eq!(found["exact"], true, "{found}");
    assert_eq!(found["page"], 2, "{found}");
    let pages: Vec<u64> = found["rects"].as_array().cloned().unwrap_or_default().iter().filter_map(|r| r["page"].as_u64()).collect();
    assert!(!pages.is_empty() && pages.iter().all(|&p| p == 1), "highlighted on the second page (index 1): {found}");
}

#[test]
fn a_selection_across_paragraphs_anchors_all_of_them() {
    let mut s = new_session();
    run(&mut s, "select.text", json!({ "text": "one." }));
    let first = run(&mut s, "presence.caret", json!({}))["caret"].clone();
    let end = serde_json::to_value(Pos::body(1, "Beta".len())).unwrap_or_default();
    run(&mut s, "caret.set", json!({ "pos": end, "extend": true }));
    let anchor = run(&mut s, "comment.anchor", json!({}));
    assert_eq!(anchor["quote"], "one.\nBeta");
    assert_eq!(anchor["ids"].as_array().map(Vec::len), Some(2), "{anchor}");
    assert_eq!(anchor["ids"][0], first["object"]);
    let start = anchor["range"]["start"].as_u64().unwrap_or(0);
    assert_eq!(anchor["range"]["end"].as_u64(), Some(start + "one.\nBeta".chars().count() as u64));
    type_at(&mut s, 0, 0, "Before. ");
    let found = locate(&mut s, &anchor);
    assert_eq!(found["text"], "one.\nBeta", "{found}");
    let rects = found["rects"].as_array().cloned().unwrap_or_default();
    assert!(rects.len() >= 2, "both lines highlighted: {found}");
}

#[test]
fn native_word_comments_leave_the_anchor_alone() {
    let mut s = new_session();
    let anchor = anchor_on(&mut s, "quoted passage");
    // A Word comment on a word inside the quote: its anchors are not text.
    run(&mut s, "select.text", json!({ "text": "passage" }));
    run(&mut s, "review.newComment", json!({ "text": "A native comment" }));
    let found = locate(&mut s, &anchor);
    assert_eq!((found["found"].clone(), found["text"].clone()), (json!(true), json!("quoted passage")), "{found}");
    // And a hosted anchor made over a Word comment quotes only the text.
    run(&mut s, "select.text", json!({ "text": "quoted" }));
    let end = s.doc.para(kition_doc_engine::doc::StoryRef::Body, &kition_doc_engine::doc::Path::top(1)).map_or(0, |p| p.text.len() - " here.".len());
    let end = serde_json::to_value(Pos::body(1, end)).unwrap_or_default();
    run(&mut s, "caret.set", json!({ "pos": end, "extend": true }));
    let over = run(&mut s, "comment.anchor", json!({}));
    assert_eq!(over["quote"], "quoted passage", "{over}");
    assert_eq!(locate(&mut s, &over)["exact"], true);
}

#[test]
fn a_co_editor_finds_the_anchor_after_both_edited() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    let anchor = anchor_on(&mut a, "quoted passage");
    // One after the other: the co-editor types, then this view.
    type_at(&mut b, 1, 0, "Their words. ");
    send(&mut b, &mut a);
    type_at(&mut a, 2, 0, "Mine. ");
    send(&mut a, &mut b);
    for s in [&mut a, &mut b] {
        let found = locate(s, &anchor);
        assert_eq!((found["found"].clone(), found["text"].clone()), (json!(true), json!("quoted passage")), "{found}");
    }
}

#[test]
fn without_a_replica_the_anchor_counts_in_the_body_text() {
    // A document as opened, before any edit made its replica.
    let mut s = Session::new(Document::from_text("Alpha paragraph one.\nBeta paragraph two, with a quoted passage here.\nGamma paragraph three."));
    assert!(s.kdm().is_none(), "no replica yet");
    let anchor = anchor_on(&mut s, "paragraph three");
    assert_eq!(anchor["ids"], json!(["doc:body"]));
    let start = ("Alpha paragraph one.\nBeta paragraph two, with a quoted passage here.\nGamma ").chars().count() as u64;
    assert_eq!(anchor["range"]["start"].as_u64(), Some(start), "{anchor}");
    assert_eq!(locate(&mut s, &anchor)["exact"], true);
    type_at(&mut s, 0, 0, "More. ");
    let found = locate(&mut s, &anchor);
    assert_eq!((found["found"].clone(), found["text"].clone()), (json!(true), json!("paragraph three")), "{found}");
    // An edit right in its middle, with offsets that no longer say where it is.
    run(&mut s, "select.text", json!({ "text": "Gamma paragraph" }));
    run(&mut s, "select.collapse", json!({ "end": true }));
    run(&mut s, "text.insert", json!({ "text": ", fast" }));
    let found = locate(&mut s, &anchor);
    assert_eq!((found["found"].clone(), found["text"].clone()), (json!(true), json!("paragraph, fast three")), "{found}");
}

#[test]
fn an_anchor_of_another_document_or_malformed_is_not_found() {
    let mut s = new_session();
    let list = run(
        &mut s,
        "comment.locate",
        json!({ "anchors": [
            { "ids": ["no-such-object"], "range": { "start": 0, "end": 4 } },
            { "ids": [], "quote": "nowhere in this text" },
            { "ids": ["doc:body"], "range": { "start": 99999, "end": 3 }, "quote": "zzz" },
            "junk",
        ] }),
    );
    let found: Vec<Value> = list.as_array().cloned().unwrap_or_default().iter().map(|f| f["found"].clone()).collect();
    assert_eq!(found, vec![json!(false), json!(false), json!(false), json!(false)]);
    // Ids this replica does not have (a replica made again, without the one the anchor was made
    // on): the quote is found where it is.
    let quoted = locate(&mut s, &json!({ "ids": ["elsewhere"], "quote": "Gamma" }));
    assert_eq!((quoted["found"].clone(), quoted["text"].clone()), (json!(true), json!("Gamma")), "{quoted}");
}
