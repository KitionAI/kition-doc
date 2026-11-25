//! Comments beside the content (Kition platform rule M23): the schema declares the
//! reserved `CommentThread` and `Comment` types; a comment, a reply, a resolve or a delete touches
//! only those (what a sync server lets a commenter push).

mod common;

use std::collections::BTreeSet;

use common::{body_para, para, rich};
use kdm_model::{DocModel, DocSession, normalize};
use kition_doc_doc::props::CharProps;
use kition_doc_doc::{Comment, Document, InlineObject, Path, StoryRef};
use kition_kit::kdm::comment::{COMMENT_THREAD_TYPE_JSON, COMMENT_TYPE_JSON};
use kition_kit::kdm::{ActorId, Origin, Replica};
use kition_kit::session::KitModel;
use serde_json::{Value, json};

fn session(doc: &Document) -> DocSession {
    DocSession::create(doc).unwrap_or_else(|e| panic!("create: {e}"))
}

fn copy(of: &DocSession) -> DocSession {
    DocSession::open_snapshot(&of.snapshot().unwrap_or_else(|e| panic!("{e}")), of.blobs().clone()).unwrap_or_else(|e| panic!("open: {e}"))
}

fn exchange(a: &mut DocSession, b: &mut DocSession) {
    for _ in 0..4 {
        b.add_blobs(&a.blobs().clone()).unwrap_or_else(|e| panic!("{e}"));
        b.import_changes(&a.export_changes(&b.version()).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("{e}"));
        a.add_blobs(&b.blobs().clone()).unwrap_or_else(|e| panic!("{e}"));
        a.import_changes(&b.export_changes(&a.version()).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("{e}"));
    }
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "the replicas converge");
    assert_eq!(serde_json::to_value(normalize(a.document())).ok(), serde_json::to_value(normalize(b.document())).ok(), "the views converge");
}

fn record(s: &mut DocSession, person: &str, f: impl FnOnce(&mut Document)) {
    let mut after = s.document().clone();
    f(&mut after);
    s.record(&Origin::person(person), &after).unwrap_or_else(|e| panic!("record: {e}"));
}

/// Adds a reply by `author` (a principal, shown as `name`) to comment `to`; answers its id.
fn reply(d: &mut Document, to: u32, author: &str, name: &str, text: &str) -> u32 {
    let part = d.add_part(kition_doc_doc::PartKind::Comment, vec![para(text)]);
    let id = d.comments.keys().next_back().map_or(0, |k| k + 1);
    d.comments.insert(id, Comment { author: name.into(), author_id: author.into(), parent: Some(to), part, ..Default::default() });
    id
}

/// Adds a comment by `author` on body paragraph `block`'s characters `range` (bytes).
fn comment_on(d: &mut Document, block: usize, range: (usize, usize), author: &str, name: &str, text: &str) -> u32 {
    let part = d.add_part(kition_doc_doc::PartKind::Comment, vec![para(text)]);
    let id = d.comments.keys().next_back().map_or(0, |k| k + 1);
    d.comments.insert(id, Comment { author: name.into(), author_id: author.into(), part, date: "2026-10-19T10:00:00Z".into(), ..Default::default() });
    let p = body_para(d, block);
    p.insert_object(range.1, InlineObject::CommentEnd { id }, &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
    p.insert_object(range.0, InlineObject::CommentStart { id }, &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
    id
}

/// The comment texts of `d`, sorted.
fn bodies(d: &Document) -> Vec<String> {
    let mut out: Vec<String> = d.comments.values().map(|c| common::texts(&d.parts[&c.part].blocks).join("\n")).collect();
    out.sort();
    out
}

/// The types of the objects `bytes` (a change export) touches, applied to a replica at `before`.
fn touched_types(before: &[u8], bytes: &[u8]) -> BTreeSet<String> {
    let mut replica = Replica::load(DocModel::schema(), before, ActorId::random()).unwrap_or_else(|e| panic!("{e}"));
    replica.take_changed();
    replica.import_changes(bytes).unwrap_or_else(|e| panic!("{e}"));
    let after = replica.state_json().unwrap_or_else(|e| panic!("{e}"));
    let old =
        Replica::load(DocModel::schema(), before, ActorId::random()).unwrap_or_else(|e| panic!("{e}")).state_json().unwrap_or_else(|e| panic!("{e}"));
    replica
        .take_changed()
        .iter()
        .map(|o| {
            let ty = |state: &Value| state["objects"][o.as_str()]["$type"].as_str().map(str::to_owned);
            ty(&after).or_else(|| ty(&old)).unwrap_or_else(|| format!("unknown {o}"))
        })
        .collect()
}

#[test]
fn the_schema_declares_the_reserved_comment_types() {
    let schema = DocModel::schema();
    for (ty, reserved) in [("CommentThread", COMMENT_THREAD_TYPE_JSON), ("Comment", COMMENT_TYPE_JSON)] {
        let reserved: Value = serde_json::from_str(reserved).unwrap_or_else(|e| panic!("{e}"));
        let ours: Value = serde_json::from_str::<Value>(include_str!("../model/schema.json")).unwrap_or_else(|e| panic!("{e}"))["types"][ty].clone();
        for (field, def) in reserved["fields"].as_object().unwrap_or_else(|| panic!("fields")) {
            assert_eq!(&ours["fields"][field], def, "{ty}.{field} is the reserved definition");
        }
        for (field, def) in ours["fields"].as_object().unwrap_or_else(|| panic!("fields")) {
            if reserved["fields"].get(field).is_none() {
                assert_eq!(def["optional"], json!(true), "{ty}.{field}, the kit's own, is optional");
            }
        }
        assert!(schema.types.contains_key(ty));
    }
}

#[test]
fn the_content_holds_no_comment_and_a_comment_touches_only_comments() {
    let mut a = session(&rich());
    // The rich document's comment and reply are one thread; no paragraph names them.
    let ids = a.model().ids().clone();
    assert_eq!(ids.threads.len(), 1);
    let state = a.replica().state_json().unwrap_or_else(|e| panic!("{e}"));
    let text = state.to_string();
    assert!(!text.contains("commentStart") && !text.contains("commentEnd"), "no comment anchor in the content");
    assert!(state["objects"].as_object().into_iter().flatten().all(|(_, o)| o["kind"] != json!("comment")), "no comment text is a part");

    // B comments: a new comment on a paragraph, a reply, a resolve, and deleting its own
    // reply touch only `CommentThread` and `Comment` objects, and write B's principal as author.
    let mut b = copy(&a);
    let before = b.snapshot().unwrap_or_else(|e| panic!("{e}"));
    let version = b.version();
    let mut new = 0;
    record(&mut b, "user:bob", |d| {
        new = comment_on(d, 2, (0, 4), "user:bob", "Bob", "Is this right?");
    });
    let mut answer = 0;
    record(&mut b, "user:bob", |d| answer = reply(d, 1, "user:bob", "Bob", "Agreed"));
    record(&mut b, "user:bob", |d| {
        if let Some(c) = d.comments.get_mut(&1) {
            c.resolved = true;
        }
    });
    let changes = b.export_changes(&version).unwrap_or_else(|e| panic!("{e}"));
    let types = touched_types(&before, &changes);
    assert_eq!(types, ["Comment", "CommentThread"].into_iter().map(str::to_owned).collect(), "a commenter's change touches only comments");
    let comment = &b.model().ids().comments[&new];
    assert_eq!(b.replica().get(comment, "author").ok(), Some(Value::from("user:bob")), "the author is the principal");
    assert_eq!(b.replica().get(comment, "name").ok(), Some(Value::from("Bob")));
    let thread = &b.model().ids().threads[&new];
    let anchor = b.replica().get(thread, "anchor").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(anchor["object"], json!(b.model().ids().body[2].object.as_str()));
    assert_eq!(
        (anchor["field"].clone(), anchor["range"].clone(), anchor["quote"].clone()),
        (json!("text"), json!({ "start": 0, "end": 4 }), json!("Para"))
    );

    let version = b.version();
    let before = b.snapshot().unwrap_or_else(|e| panic!("{e}"));
    record(&mut b, "user:bob", |d| {
        if let Some(c) = d.comments.remove(&answer) {
            d.parts.remove(&c.part);
        }
    });
    let types = touched_types(&before, &b.export_changes(&version).unwrap_or_else(|e| panic!("{e}")));
    assert!(types.iter().all(|t| t == "Comment" || t == "CommentThread"), "{types:?}");

    // A sees all of it, anchored where B put it.
    exchange(&mut a, &mut b);
    let d = a.document();
    assert!(d.comments[&1].resolved);
    let p = d.para(StoryRef::Body, &Path::top(2)).unwrap_or_else(|| panic!("paragraph"));
    assert!(matches!(p.objects.first(), Some(InlineObject::CommentStart { .. })) && p.text.starts_with('\u{fffc}'), "{:?}", p.text);
    assert_eq!(bodies(d), ["A comment", "A reply", "Is this right?"]);
}

#[test]
fn anchors_follow_typing_without_rewriting_the_thread() {
    let mut a = session(&rich());
    let thread = a.model().ids().threads[&1].clone();
    let anchor = a.replica().get(&thread, "anchor").unwrap_or_else(|e| panic!("{e}"));
    let version = a.version();
    let before = a.snapshot().unwrap_or_else(|e| panic!("{e}"));
    record(&mut a, "user:ann", |d| {
        // Typed before the commented range, inside it, and after it.
        body_para(d, 1).insert_text(0, "Intro ", &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
    });
    let types = touched_types(&before, &a.export_changes(&version).unwrap_or_else(|e| panic!("{e}")));
    assert_eq!(types, ["Block"].into_iter().map(str::to_owned).collect(), "typing outside the range writes only the paragraph");
    assert_eq!(a.replica().get(&thread, "anchor").ok(), Some(anchor));
    let fresh = kdm_model::read_document(a.replica(), a.blobs()).unwrap_or_else(|e| panic!("{e}")).0;
    let p = fresh.para(StoryRef::Body, &Path::top(1)).unwrap_or_else(|| panic!("paragraph"));
    assert!(p.text.starts_with("Intro \u{fffc}Paragraph 1\u{fffc}"), "{:?}", p.text);
}
