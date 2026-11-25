//! Presence (Kit UI protocol section 7e): a person's caret and selection travel as replica text
//! positions (`{object, field, offset}`), and a co-editor places them in their own view, also
//! after a peer's typing, around comment anchors (kept beside the content), and in a paragraph
//! merged into another.

use kition_doc_engine::doc::{Document, Pos};
use kition_doc_engine::{Session, Version};
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.run(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn new_session() -> Session {
    let mut s = Session::new(Document::new());
    for (i, t) in ["Alpha paragraph one.", "Beta paragraph two."].iter().enumerate() {
        if i > 0 {
            run(&mut s, "text.newParagraph", json!({}));
        }
        run(&mut s, "text.insert", json!({ "text": t }));
    }
    s.version().unwrap_or_else(|e| panic!("{e}"));
    s
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

fn sync(a: &mut Session, b: &mut Session) {
    for _ in 0..8 {
        send(a, b);
        send(b, a);
        if a.version().ok() == b.version().ok() {
            return;
        }
    }
    panic!("the replicas did not converge");
}

fn caret_at(s: &mut Session, block: usize, off: usize) {
    let pos = serde_json::to_value(Pos::body(block, off)).unwrap_or_default();
    run(s, "caret.set", json!({ "pos": pos }));
}

/// Puts the caret right after `text`.
fn after(s: &mut Session, text: &str) {
    run(s, "select.text", json!({ "text": text }));
    run(s, "select.collapse", json!({ "end": true }));
}

/// Where `s` places the carets (and selections) shared as `places`.
fn locate(s: &mut Session, places: Value) -> Vec<Value> {
    run(s, "presence.locate", json!({ "places": places })).as_array().cloned().unwrap_or_default()
}

/// `s`'s own caret, and where `s` itself places it: what a peer's must equal at the same spot.
fn own(s: &mut Session) -> (Value, Value) {
    let shared = run(s, "presence.caret", json!({}));
    let placed = locate(s, json!([{ "caret": shared["caret"] }])).remove(0)["caret"].clone();
    (shared["caret"].clone(), placed)
}

#[test]
fn a_caret_travels_as_a_text_position_a_peer_places_on_its_view() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    caret_at(&mut a, 1, 5);
    let shared = run(&mut a, "presence.caret", json!({}));
    assert_eq!(shared["caret"]["field"], "text");
    assert_eq!(shared["caret"]["offset"], 5);
    assert!(shared["caret"]["object"].is_string());
    assert_eq!(shared["anchor"], Value::Null);
    assert_eq!(shared["selection"], json!([]));
    assert_eq!(shared["page"], 0);

    // The peer, its own caret at the same spot, shares the same position and places both alike.
    caret_at(&mut b, 1, 5);
    let (theirs, placed) = own(&mut b);
    assert_eq!(theirs, shared["caret"]);
    let found = locate(&mut b, json!([{ "caret": shared["caret"] }]));
    assert_eq!(found[0]["caret"], placed);
    assert!(found[0]["caret"]["height"].as_f64().unwrap_or_default() > 0.0, "{found:?}");
    assert_eq!(found[0]["rects"], json!([]));

    // Something it does not have is not placed.
    let missing = locate(&mut b, json!([{ "caret": { "object": "nothing", "field": "text", "offset": 0 } }, { "caret": 7 }, {}]));
    assert_eq!(missing.len(), 3);
    assert!(missing.iter().all(|place| place["caret"].is_null()), "{missing:?}");
}

#[test]
fn a_selection_travels_as_its_ends_and_the_paragraphs_it_covers() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    let anchor = serde_json::to_value(Pos::body(0, 6)).unwrap_or_default();
    let focus = serde_json::to_value(Pos::body(1, 4)).unwrap_or_default();
    run(&mut a, "caret.set", json!({ "pos": anchor }));
    run(&mut a, "caret.set", json!({ "pos": focus, "extend": true }));
    let shared = run(&mut a, "presence.caret", json!({}));
    assert_eq!((shared["anchor"]["offset"].as_u64(), shared["caret"]["offset"].as_u64()), (Some(6), Some(4)));
    assert_eq!(shared["selection"].as_array().map(Vec::len), Some(2), "{shared}");
    assert_ne!(shared["anchor"]["object"], shared["caret"]["object"]);

    let found = locate(&mut b, json!([{ "caret": shared["caret"], "anchor": shared["anchor"] }]));
    let rects = found[0]["rects"].as_array().cloned().unwrap_or_default();
    assert!(rects.len() >= 2, "a rectangle on each line: {found:?}");
    assert!(rects.iter().all(|r| r["width"].as_f64().unwrap_or_default() >= 0.0 && r["page"] == 0));
}

#[test]
fn a_caret_follows_a_peers_typing_before_it() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    after(&mut a, "Beta paragraph");
    let before = run(&mut a, "presence.caret", json!({}));
    assert_eq!(before["caret"]["offset"], 14);
    // The peer types at the paragraph's start.
    caret_at(&mut b, 1, 0);
    run(&mut b, "text.insert", json!({ "text": "New " }));
    sync(&mut a, &mut b);

    let shared = run(&mut a, "presence.caret", json!({}));
    assert_eq!(shared["caret"]["object"], before["caret"]["object"]);
    assert_eq!(shared["caret"]["offset"], 18, "moved with the text before it");
    after(&mut b, "New Beta paragraph");
    assert_eq!(locate(&mut b, json!([{ "caret": shared["caret"] }]))[0]["caret"], own(&mut b).1);
}

#[test]
fn comment_anchors_do_not_count() {
    let mut a = new_session();
    run(&mut a, "select.text", json!({ "text": "paragraph" }));
    run(&mut a, "comment.add", json!({ "text": "Which one?" }));
    let mut b = peer(&mut a);
    after(&mut a, "one");
    let shared = run(&mut a, "presence.caret", json!({}));
    assert_eq!(shared["caret"]["offset"], 19, "the comment's anchors are not characters of the text");
    after(&mut b, "one");
    let (theirs, placed) = own(&mut b);
    assert_eq!(theirs, shared["caret"]);
    assert_eq!(locate(&mut b, json!([{ "caret": shared["caret"] }]))[0]["caret"], placed);
}

#[test]
fn a_caret_in_a_merged_paragraph_names_the_paragraph_its_text_lives_in() {
    let mut a = new_session();
    after(&mut a, "two");
    let second = run(&mut a, "presence.caret", json!({}))["caret"].clone();
    // Backspace at the second paragraph's start: its text now shows at the end of the first,
    // and stays in its own object.
    caret_at(&mut a, 1, 0);
    run(&mut a, "text.backspace", json!({}));
    let mut b = peer(&mut a);
    after(&mut a, "two");
    let shared = run(&mut a, "presence.caret", json!({}));
    assert_eq!(shared["caret"]["object"], second["object"], "{shared}");
    after(&mut b, "two");
    let (theirs, placed) = own(&mut b);
    assert_eq!(theirs, shared["caret"]);
    assert_eq!(locate(&mut b, json!([{ "caret": shared["caret"] }]))[0]["caret"], placed);
    // At the joint: the end of the first paragraph's text.
    after(&mut a, "one.");
    let joint = run(&mut a, "presence.caret", json!({}));
    assert_ne!(joint["caret"]["object"], second["object"]);
    assert_eq!(joint["caret"]["offset"], 20);
}

#[test]
fn nothing_is_shared_before_the_document_has_a_replica() {
    let mut s = Session::new(Document::new());
    assert!(s.kdm().is_none());
    let shared = run(&mut s, "presence.caret", json!({}));
    assert_eq!((shared["caret"].clone(), shared["selection"].clone()), (Value::Null, json!([])));
    assert_eq!(locate(&mut s, json!([{ "caret": { "object": "x", "field": "text", "offset": 0 } }]))[0]["caret"], Value::Null);
    assert!(s.kdm().is_none(), "asking makes no replica");
}
