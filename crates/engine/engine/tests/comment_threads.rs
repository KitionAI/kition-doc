//! Comment threads through the engine's commands (Kition platform rule M23): a thread on
//! a selection, replies, resolving, deleting a reply or the whole thread, and going to one; two
//! people commenting on one thread at once keep both comments; a session open for commenting
//! only refuses every edit of the content, writes only comment objects, and takes its own
//! comments back with Undo.

use std::collections::BTreeSet;

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
    // Made now, so a peer opens the same replica.
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

fn select(s: &mut Session, block: usize, from: usize, to: usize) {
    let anchor = serde_json::to_value(Pos::body(block, from)).unwrap_or_default();
    let focus = serde_json::to_value(Pos::body(block, to)).unwrap_or_default();
    run(s, "caret.set", json!({ "pos": anchor }));
    run(s, "caret.set", json!({ "pos": focus, "extend": true }));
}

fn threads(s: &mut Session) -> Vec<Value> {
    run(s, "comment.threads", json!({})).as_array().cloned().unwrap_or_default()
}

fn bodies(thread: &Value) -> Vec<String> {
    thread["comments"].as_array().into_iter().flatten().map(|c| c["body"].as_str().unwrap_or("").to_owned()).collect()
}

/// The types of the objects the replica changed between `before` (a snapshot) and now.
fn touched(s: &mut Session, before: &[u8], since: &Version) -> BTreeSet<String> {
    let schema = <kition_doc_engine::kdm_model::DocModel as kition_kit_model::KitModel>::schema();
    let mut replica = kition_kit_kdm::Replica::load(schema, before, kition_kit_kdm::ActorId::random()).unwrap_or_else(|e| panic!("{e}"));
    let old = replica.state_json().unwrap_or_else(|e| panic!("{e}"));
    replica.take_changed();
    replica.import_changes(&s.export_changes(since).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("{e}"));
    let new = replica.state_json().unwrap_or_else(|e| panic!("{e}"));
    replica
        .take_changed()
        .iter()
        .map(|o| {
            let ty = |state: &Value| state["objects"][o.as_str()]["$type"].as_str().map(str::to_owned);
            ty(&new).or_else(|| ty(&old)).unwrap_or_default()
        })
        .collect()
}

use kition_doc_engine::kdm_model::kition_kit::kdm as kition_kit_kdm;
use kition_doc_engine::kdm_model::kition_kit::session as kition_kit_model;

#[test]
fn a_thread_on_a_selection_with_replies_resolve_delete_and_go_to() {
    let mut s = new_session();
    select(&mut s, 1, 0, 4);
    let made = run(&mut s, "comment.add", json!({ "text": "Why Beta?", "author": "user:ann", "name": "Ann", "created": "2026-10-19T10:00:00Z" }));
    let thread = made["thread"].as_str().unwrap_or_default().to_owned();
    let found = threads(&mut s);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["id"], json!(thread));
    assert_eq!(found[0]["anchor"]["quote"], json!("Beta"));
    assert!(found[0]["anchor"]["place"]["page"].is_number(), "placed on its page");
    assert_eq!(found[0]["comments"][0]["author"], json!("user:ann"));
    assert_eq!(found[0]["comments"][0]["name"], json!("Ann"));
    assert_eq!(found[0]["comments"][0]["created"], json!("2026-10-19T10:00:00Z"));

    let answer = run(&mut s, "comment.reply", json!({ "thread": thread, "text": "Second", "author": "user:bob", "name": "Bob" }));
    run(&mut s, "comment.reply", json!({ "thread": thread, "text": "Third" }));
    run(&mut s, "comment.resolve", json!({ "thread": thread, "resolved": true }));
    let found = threads(&mut s);
    assert_eq!(bodies(&found[0]), ["Why Beta?", "Second", "Third"]);
    assert_eq!(found[0]["resolved"], json!(true));

    run(&mut s, "comment.delete", json!({ "thread": thread, "comment": answer["comment"] }));
    assert_eq!(bodies(&threads(&mut s)[0]), ["Why Beta?", "Third"]);

    run(&mut s, "caret.docStart", json!({}));
    run(&mut s, "comment.goto", json!({ "thread": thread }));
    assert_eq!(s.selected_text(), "Beta");

    run(&mut s, "comment.delete", json!({ "thread": thread }));
    assert!(threads(&mut s).is_empty());
    assert!(s.doc.comments.is_empty() && !s.doc.parts.values().any(|p| p.kind == kition_doc_engine::doc::PartKind::Comment));
    assert_eq!(s.doc.plain_text(kition_doc_engine::doc::StoryRef::Body).trim_end(), "Alpha paragraph one.\nBeta paragraph two.");
}

#[test]
fn two_people_commenting_on_one_thread_at_once_keep_both() {
    let mut a = new_session();
    select(&mut a, 0, 0, 5);
    let thread = run(&mut a, "comment.add", json!({ "text": "Open", "author": "user:ann", "name": "Ann" }))["thread"].clone();
    let mut b = peer(&mut a);
    let theirs = threads(&mut b)[0]["id"].clone();
    run(&mut a, "comment.reply", json!({ "thread": thread, "text": "From Ann", "author": "user:ann", "name": "Ann" }));
    run(&mut b, "comment.reply", json!({ "thread": theirs, "text": "From Bob", "author": "user:bob", "name": "Bob" }));
    sync(&mut a, &mut b);
    for s in [&mut a, &mut b] {
        let found = threads(s);
        assert_eq!(found.len(), 1);
        let mut all = bodies(&found[0]);
        assert_eq!(all.remove(0), "Open");
        all.sort();
        assert_eq!(all, ["From Ann", "From Bob"]);
    }
}

#[test]
fn a_commenter_comments_but_cannot_edit_and_writes_only_comments() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    b.set_comment_only(true);
    let (before, since) = (b.snapshot().unwrap_or_else(|e| panic!("{e}")), b.version().unwrap_or_else(|e| panic!("{e}")));
    let text = b.doc.plain_text(kition_doc_engine::doc::StoryRef::Body);
    for (id, params) in [
        ("text.insert", json!({ "text": "x" })),
        ("format.bold", json!({})),
        ("text.newParagraph", json!({})),
        ("insert.table", json!({ "rows": 1, "cols": 1 })),
        ("review.trackChanges", json!({})),
    ] {
        let error = b.run(id, &params).err().unwrap_or_else(|| panic!("{id} ran"));
        assert!(error.to_string().contains("commenting only"), "{id}: {error}");
    }
    assert_eq!(b.doc.plain_text(kition_doc_engine::doc::StoryRef::Body), text, "the content is as it was");
    // Selecting still works, and a comment goes on the selection.
    select(&mut b, 0, 6, 15);
    let thread = run(&mut b, "comment.add", json!({ "text": "Nice", "author": "user:bob", "name": "Bob" }))["thread"].clone();
    run(&mut b, "comment.reply", json!({ "thread": thread, "text": "And this", "author": "user:bob", "name": "Bob" }));
    run(&mut b, "comment.resolve", json!({ "thread": thread }));
    let types = touched(&mut b, &before, &since);
    assert_eq!(types, ["Comment", "CommentThread"].into_iter().map(str::to_owned).collect(), "only comment objects");
    // Undo takes the commenter's own last step back.
    run(&mut b, "edit.undo", json!({}));
    assert_eq!(threads(&mut b)[0]["resolved"], json!(false));
    sync(&mut a, &mut b);
    let found = threads(&mut a);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["anchor"]["quote"], json!("paragraph"));
    assert_eq!(bodies(&found[0]), ["Nice", "And this"]);
    assert_eq!(found[0]["comments"][0]["author"], json!("user:bob"));
}
