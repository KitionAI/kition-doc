//! Comment threads between two replicas as a host drives them (Kition platform rule M23):
//! comments two people add to one thread at once are both kept, and a replica open for commenting
//! only (`setCommentOnly`) refuses edits of the content, answers no unsaved changes, and sends
//! changes that touch only `CommentThread` and `Comment` objects (what a sync server takes from a
//! commenter, sync protocol 1.6).

mod common;

use std::collections::BTreeSet;

use common::NAME;
use kdm_model::DocModel;
use kition_doc_kit::DocDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, Replica};
use kition_kit::session::KitModel;
use serde_json::{Value, json};

fn exec(doc: &mut DocDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn first() -> DocDocument {
    let mut doc = DocDocument::create(NAME, 0, 0).unwrap_or_else(|error| panic!("{error}"));
    exec(&mut doc, "text.insert", json!({ "text": "Shared words to talk about." }));
    doc
}

fn join(of: &mut DocDocument) -> DocDocument {
    let (snapshot, blobs) = (of.snapshot().unwrap_or_else(|e| panic!("{e}")), of.blobs().unwrap_or_else(|e| panic!("{e}")));
    DocDocument::open_with_replica(NAME, &[], &snapshot, &blobs).unwrap_or_else(|e| panic!("{e}"))
}

fn send(from: &mut DocDocument, to: &mut DocDocument) {
    let since = to.version().unwrap_or_else(|e| panic!("{e}"));
    to.add_blobs(&from.blobs_since(&since).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("{e}"));
    to.import_changes(&from.export_changes(&since).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("{e}"));
}

fn exchange(a: &mut DocDocument, b: &mut DocDocument) {
    for _ in 0..4 {
        send(a, b);
        send(b, a);
    }
    assert_eq!(a.version().ok(), b.version().ok(), "the replicas converge");
}

fn threads(doc: &mut DocDocument) -> Vec<Value> {
    exec(doc, "comment.threads", json!({})).as_array().cloned().unwrap_or_default()
}

fn select_words(doc: &mut DocDocument, from: usize, to: usize) {
    exec(doc, "caret.set", json!({ "pos": { "story": "body", "path": [0], "off": from } }));
    exec(doc, "caret.set", json!({ "pos": { "story": "body", "path": [0], "off": to }, "extend": true }));
}

#[test]
fn comments_added_to_one_thread_at_once_are_both_kept() {
    let mut a = first();
    select_words(&mut a, 0, 6);
    let thread = exec(&mut a, "comment.add", json!({ "text": "Shared how?", "author": "user:ann", "name": "Ann" }))["thread"].clone();
    let mut b = join(&mut a);
    let theirs = threads(&mut b)[0]["id"].clone();
    exec(&mut a, "comment.reply", json!({ "thread": thread, "text": "Ann again", "author": "user:ann", "name": "Ann" }));
    exec(&mut b, "comment.reply", json!({ "thread": theirs, "text": "Bob here", "author": "user:bob", "name": "Bob" }));
    exchange(&mut a, &mut b);
    for doc in [&mut a, &mut b] {
        let found = threads(doc);
        assert_eq!(found.len(), 1);
        let mut bodies: Vec<String> =
            found[0]["comments"].as_array().into_iter().flatten().map(|c| c["body"].as_str().unwrap_or("").to_owned()).collect();
        assert_eq!(bodies.remove(0), "Shared how?");
        bodies.sort();
        assert_eq!(bodies, ["Ann again", "Bob here"]);
        assert_eq!(found[0]["anchor"]["quote"], json!("Shared"));
    }
}

#[test]
fn a_commenters_changes_touch_only_comment_objects() {
    let mut a = first();
    let mut b = join(&mut a);
    b.set_comment_only(true);
    let (before, since) = (b.snapshot().unwrap_or_else(|e| panic!("{e}")), b.version().unwrap_or_else(|e| panic!("{e}")));
    assert!(b.execute("text.insert", json!({ "text": "no" })).is_err(), "typing is refused");
    assert!(b.execute("format.bold", json!({})).is_err(), "formatting is refused");
    select_words(&mut b, 7, 12);
    let thread = exec(&mut b, "comment.add", json!({ "text": "Which words?", "author": "user:bob", "name": "Bob" }))["thread"].clone();
    exec(&mut b, "comment.reply", json!({ "thread": thread, "text": "These", "author": "user:bob", "name": "Bob" }));
    exec(&mut b, "comment.resolve", json!({ "thread": thread }));
    assert!(!b.dirty(), "a commenter has nothing to save");

    // The changes as the server sees them: every object they touch is a comment's.
    let changes = b.export_changes(&since).unwrap_or_else(|e| panic!("{e}"));
    let mut replica = Replica::load(DocModel::schema(), &before, ActorId::random()).unwrap_or_else(|e| panic!("{e}"));
    let old = replica.state_json().unwrap_or_else(|e| panic!("{e}"));
    replica.take_changed();
    replica.import_changes(&changes).unwrap_or_else(|e| panic!("{e}"));
    let new = replica.state_json().unwrap_or_else(|e| panic!("{e}"));
    let types: BTreeSet<String> = replica
        .take_changed()
        .iter()
        .map(|o| {
            let ty = |state: &Value| state["objects"][o.as_str()]["$type"].as_str().map(str::to_owned);
            ty(&new).or_else(|| ty(&old)).unwrap_or_default()
        })
        .collect();
    assert_eq!(types, ["Comment", "CommentThread"].into_iter().map(str::to_owned).collect());
    // Its comments carry Bob's principal as their author.
    let authors: BTreeSet<String> = new["objects"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, o)| o["$type"] == json!("Comment"))
        .filter_map(|(_, o)| o["author"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(authors, ["user:bob".to_owned()].into_iter().collect());

    exchange(&mut a, &mut b);
    let found = threads(&mut a);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["resolved"], json!(true));
    assert_eq!(found[0]["anchor"]["quote"], json!("words"));
}
