//! Comments beside the content (Kition platform rule M23): the schema declares the
//! reserved `CommentThread` and `Comment` types; a comment, a reply, a resolve or a delete touches
//! only those (what a sync server lets a commenter push); concurrent comments on one thread are
//! all kept; a replica schema 2 wrote, with its comments in the content, is migrated by the first
//! write of someone who may edit, as a `system` change that loses no comment, and two replicas
//! migrating offline converge; a build that keeps comments in the content refuses it then.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use common::{body_para, para, rich};
use kdm_model::{DocModel, DocSession, SCHEMA_VERSION, normalize};
use kition_doc_doc::props::CharProps;
use kition_doc_doc::{Comment, Document, InlineObject, Paragraph, Path, StoryRef};
use kition_kit::kdm::comment::{COMMENT_THREAD_TYPE_JSON, COMMENT_TYPE_JSON};
use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema, TextRun};
use kition_kit::session::{KitModel, MemoryBlobs, write_rich_text};
use serde_json::{Map, Value, json};

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

    // B may only comment: a new comment on a paragraph, a reply, a resolve, and deleting its own
    // reply touch only `CommentThread` and `Comment` objects, and write B's principal as author.
    let mut b = copy(&a);
    b.set_comment_only(true);
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
fn concurrent_comments_on_one_thread_are_all_kept() {
    let mut a = session(&rich());
    let mut b = copy(&a);
    record(&mut a, "user:ann", |d| {
        reply(d, 1, "user:ann", "Ann", "From A");
    });
    record(&mut b, "user:bob", |d| {
        reply(d, 1, "user:bob", "Bob", "From B");
    });
    // And both open a thread on the same words at once.
    record(&mut a, "user:ann", |d| {
        comment_on(d, 0, (0, 9), "user:ann", "Ann", "A's thread");
    });
    record(&mut b, "user:bob", |d| {
        comment_on(d, 0, (0, 9), "user:bob", "Bob", "B's thread");
    });
    exchange(&mut a, &mut b);
    let d = a.document();
    assert_eq!(bodies(d), ["A comment", "A reply", "A's thread", "B's thread", "From A", "From B"]);
    // The replies are in the first thread, after the reply it had.
    let thread = &a.model().ids().threads[&1];
    let listed = a.replica().list(thread, "comments").unwrap_or_default();
    assert_eq!(listed.len(), 4);
    let mut replies: Vec<String> = listed[2..].iter().map(|o| a.replica().text(o, "body").unwrap_or_default()).collect();
    replies.sort();
    assert_eq!(replies, ["From A", "From B"]);
    for (id, c) in &d.comments {
        if c.parent.is_some() {
            assert_eq!(c.parent, Some(1), "comment {id} replies to the thread's first comment");
        }
    }
    // Both new threads are anchored on paragraph 0's first word.
    let p = d.para(StoryRef::Body, &Path::top(0)).unwrap_or_else(|| panic!("paragraph"));
    assert_eq!(p.objects.iter().filter(|o| matches!(o, InlineObject::CommentStart { .. })).count(), 2);
    assert_eq!(p.text.chars().filter(|c| *c != '\u{fffc}').collect::<String>(), "Paragraph 0");
}

#[test]
fn a_deleted_thread_keeps_a_reply_made_to_it_meanwhile() {
    let mut a = session(&rich());
    let mut b = copy(&a);
    record(&mut a, "user:ann", |d| {
        let p = body_para(d, 1);
        for off in p.object_offsets().into_iter().rev() {
            p.delete(off, off + kition_doc_doc::para::OBJ.len_utf8()).unwrap_or_else(|e| panic!("{e}"));
        }
        for id in [1, 2] {
            if let Some(c) = d.comments.remove(&id) {
                d.parts.remove(&c.part);
            }
        }
    });
    record(&mut b, "user:bob", |d| {
        reply(d, 1, "user:bob", "Bob", "Wait");
    });
    exchange(&mut a, &mut b);
    assert_eq!(bodies(a.document()), ["Wait"], "the reply is kept, opening the thread");
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

// --- schema 2 -----------------------------------------------------------------------------------

/// `model/schema.json` as schema version 2 had it: comments in the content.
fn schema_v2() -> Arc<Schema> {
    let mut v: Value = serde_json::from_str(include_str!("../model/schema.json")).unwrap_or_else(|e| panic!("{e}"));
    v["schemaVersion"] = json!(2);
    v["types"].as_object_mut().unwrap_or_else(|| panic!("types")).remove("CommentThread");
    v["types"]["Comment"] = json!({ "fields": {
        "props": { "kind": "json", "optional": true },
        "part": { "kind": "ref", "of": "Part", "optional": true },
        "parent": { "kind": "ref", "of": "Comment", "optional": true }
    } });
    Arc::new(Schema::parse(&v.to_string()).unwrap_or_else(|e| panic!("{e}")))
}

fn meta() -> Value {
    let mut v = serde_json::to_value(Paragraph::new()).unwrap_or_else(|e| panic!("{e}"));
    if let Some(map) = v.as_object_mut() {
        map.remove("text");
        map.remove("runs");
        map.remove("objects");
    }
    v
}

fn fields(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
}

/// A replica as schema 2 wrote it: "Hello commented world", "commented" commented by Ann
/// ("Check this", resolved), with Ben's reply ("Done") on the same range.
fn old_snapshot() -> Vec<u8> {
    let root = ObjectId::from_string("document");
    let (replica, ()) = Replica::create_with(schema_v2(), DocId::new(), ActorId::random(), &Origin::system("create"), |tx| {
        tx.create_with_id("Document", &root, Map::new())?;
        let text_part = |tx: &mut kition_kit::kdm::Tx<'_>, text: &str| -> Result<ObjectId, KdmError> {
            let part = tx.create("Part", fields(&[("kind", json!("comment"))]))?;
            let block = tx.create("Block", fields(&[("kind", json!("para")), ("para", meta())]))?;
            tx.text_insert(&block, "text", 0, text)?;
            tx.list_insert(&part, "blocks", 0, &block)?;
            Ok(part)
        };
        let (first, second) = (text_part(tx, "Check this")?, text_part(tx, "Done")?);
        tx.list_insert(&root, "parts", 0, &first)?;
        tx.list_insert(&root, "parts", 1, &second)?;
        let ann = tx.create(
            "Comment",
            fields(&[
                ("props", json!({ "author": "Ann", "initials": "A", "date": "2026-03-01T09:00:00Z", "resolved": true, "paraId": "1A2B3C4D" })),
                ("part", json!(first.as_str())),
            ]),
        )?;
        let ben = tx.create(
            "Comment",
            fields(&[
                ("props", json!({ "author": "Ben", "date": "2026-03-01T10:00:00Z" })),
                ("part", json!(second.as_str())),
                ("parent", json!(ann.as_str())),
            ]),
        )?;
        tx.list_insert(&root, "comments", 0, &ann)?;
        tx.list_insert(&root, "comments", 1, &ben)?;
        let inline = |tx: &mut kition_kit::kdm::Tx<'_>, ty: &str, of: &ObjectId| {
            tx.create("Inline", fields(&[("value", json!({ "type": ty, "id": of.as_str() }))]))
        };
        let marks: Vec<ObjectId> = vec![
            inline(tx, "commentStart", &ann)?,
            inline(tx, "commentStart", &ben)?,
            inline(tx, "commentEnd", &ann)?,
            inline(tx, "commentEnd", &ben)?,
        ];
        let block = tx.create("Block", fields(&[("kind", json!("para")), ("para", meta())]))?;
        let run = |text: &str| TextRun { insert: text.into(), attributes: Default::default() };
        let anchor =
            |o: &ObjectId| TextRun { insert: "\u{fffc}".into(), attributes: [("object".to_owned(), json!(o.as_str()))].into_iter().collect() };
        let runs = vec![run("Hello "), anchor(&marks[0]), anchor(&marks[1]), run("commented"), anchor(&marks[2]), anchor(&marks[3]), run(" world")];
        write_rich_text(tx, &block, "text", &[], &runs).map_err(|e| KdmError::Engine(e.to_string()))?;
        tx.list_insert(&root, "body", 0, &block)?;
        Ok(())
    })
    .unwrap_or_else(|e| panic!("old replica: {e}"));
    replica.snapshot().unwrap_or_else(|e| panic!("{e}"))
}

fn old_session() -> DocSession {
    open(&old_snapshot())
}

fn open(snapshot: &[u8]) -> DocSession {
    DocSession::open_snapshot(snapshot, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open: {e}"))
}

/// What the old replica holds, as the view shows it (comment ids and parts normalized).
fn expected_view(s: &DocSession) -> Value {
    serde_json::to_value(normalize(s.document())).unwrap_or_else(|e| panic!("{e}"))
}

fn assert_old_comments(d: &Document) {
    assert_eq!(bodies(d), ["Check this", "Done"]);
    let ann = d.comments.values().find(|c| c.author == "Ann").unwrap_or_else(|| panic!("Ann's comment"));
    assert!(ann.resolved && ann.initials == "A" && ann.date == "2026-03-01T09:00:00Z" && ann.para_id == "1A2B3C4D");
    let ben = d.comments.values().find(|c| c.author == "Ben").unwrap_or_else(|| panic!("Ben's reply"));
    assert!(ben.parent.is_some_and(|p| d.comments[&p].author == "Ann"));
    let p = d.para(StoryRef::Body, &Path::top(0)).unwrap_or_else(|| panic!("paragraph"));
    assert_eq!(p.text, "Hello \u{fffc}\u{fffc}commented\u{fffc}\u{fffc} world");
    assert!(matches!(
        p.objects.as_slice(),
        [InlineObject::CommentStart { .. }, InlineObject::CommentStart { .. }, InlineObject::CommentEnd { .. }, InlineObject::CommentEnd { .. }]
    ));
}

#[test]
fn a_schema_2_replica_reads_with_its_comments() {
    let s = old_session();
    assert!(s.legacy_comments());
    assert_old_comments(s.document());
}

#[test]
fn the_first_write_moves_the_comments_out_of_the_content_and_loses_none() {
    let mut s = old_session();
    let view = expected_view(&s);
    record(&mut s, "user:ann", |d| {
        body_para(d, 0).insert_text(0, "Oh ", &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
    });
    assert!(!s.legacy_comments());
    assert_eq!(s.steps().0, 1, "the migration is no undo step; the edit is one");
    assert_eq!(s.replica().min_schema_version(), SCHEMA_VERSION);
    // Nothing of the comments is left in the content.
    let state = s.replica().state_json().unwrap_or_else(|e| panic!("{e}"));
    let objects = state["objects"].as_object().cloned().unwrap_or_default();
    assert!(objects.values().all(|o| o["$type"] != json!("Inline")), "the anchors' objects are gone");
    assert!(objects.values().all(|o| o["kind"] != json!("comment")), "the comment texts are no parts");
    assert_eq!(s.replica().list(&ObjectId::from_string("document"), "comments").ok(), Some(Vec::new()));
    let block = &s.model().ids().body[0].object;
    assert_eq!(s.replica().text(block, "text").ok().as_deref(), Some("Oh Hello commented world"));
    // One thread: Ann's comment and Ben's reply, anchored on "commented".
    let threads: Vec<_> = objects.iter().filter(|(_, o)| o["$type"] == json!("CommentThread")).collect();
    assert_eq!(threads.len(), 1);
    let (_, thread) = threads[0];
    assert_eq!(thread["resolved"], json!(true));
    assert_eq!(thread["anchor"]["quote"], json!("commented"));
    assert_eq!(thread["comments"].as_array().map(Vec::len), Some(2));
    // The view is as it was (plus the edit), and so is a fresh read.
    assert_old_comments(&{
        let mut d = s.document().clone();
        body_para(&mut d, 0).delete(0, 3).unwrap_or_else(|e| panic!("{e}"));
        d
    });
    let mut undone = s.document().clone();
    body_para(&mut undone, 0).delete(0, 3).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(serde_json::to_value(normalize(&undone)).ok(), Some(view));
    let fresh = kdm_model::read_document(s.replica(), s.blobs()).unwrap_or_else(|e| panic!("{e}")).0;
    assert_eq!(serde_json::to_value(normalize(&fresh)).ok(), serde_json::to_value(normalize(s.document())).ok());
    // Undo takes back the edit, not the migration.
    assert!(s.undo().unwrap_or_else(|e| panic!("{e}")).0);
    assert!(!s.legacy_comments());
    assert_old_comments(s.document());

    // A build that keeps comments in the content refuses the migrated replica.
    let error = Replica::load(schema_v2(), &s.snapshot().unwrap_or_else(|e| panic!("{e}")), ActorId::random()).err();
    assert_eq!(error, Some(KdmError::NewerSchema { found: SCHEMA_VERSION.into(), reads: 2 }));
}

#[test]
fn two_replicas_migrating_offline_converge_without_doubling_a_comment() {
    let snapshot = old_snapshot();
    let (mut a, mut b) = (open(&snapshot), open(&snapshot));
    record(&mut a, "user:ann", |d| {
        body_para(d, 0).insert_text(0, "A ", &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
    });
    record(&mut b, "user:bob", |d| {
        let end = body_para(d, 0).len();
        body_para(d, 0).insert_text(end, " B", &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
        let ann = d.comments.iter().find(|(_, c)| c.author == "Ann").map(|(id, _)| *id).unwrap_or_else(|| panic!("Ann"));
        reply(d, ann, "user:bob", "Bob", "Seen");
    });
    exchange(&mut a, &mut b);
    let d = a.document();
    assert_eq!(bodies(d), ["Check this", "Done", "Seen"], "every comment once");
    let threads = a.replica().objects_of_type("CommentThread");
    assert_eq!(threads.len(), 1, "both made the same thread");
    // One replica's list of the thread won; B's reply, which only B's listed, shows in the thread
    // it was written in all the same, and the next write lists it there.
    let seen = d.comments.values().find(|c| c.author == "Bob").unwrap_or_else(|| panic!("Bob's reply"));
    assert!(seen.parent.is_some_and(|p| d.comments[&p].author == "Ann"), "in Ann's thread");
    let p = d.para(StoryRef::Body, &Path::top(0)).unwrap_or_else(|| panic!("paragraph"));
    assert_eq!(p.text.chars().filter(|c| *c != '\u{fffc}').collect::<String>(), "A Hello commented world B");
    assert_eq!(p.objects.len(), 4, "the range is placed once");
    record(&mut a, "user:ann", |d| {
        body_para(d, 0).insert_text(0, "!", &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
    });
    exchange(&mut a, &mut b);
    assert_eq!(a.replica().list(&threads[0], "comments").map(|l| l.len()).ok(), Some(3), "listed once each");
    assert_eq!(bodies(b.document()), ["Check this", "Done", "Seen"]);
}

#[test]
fn a_commenter_does_not_migrate_and_writes_nothing_over_a_schema_2_replica() {
    let mut s = old_session();
    s.set_comment_only(true);
    let version = s.version();
    let mut after = s.document().clone();
    reply(&mut after, 0, "user:bob", "Bob", "No");
    assert!(s.record(&Origin::person("user:bob"), &after).is_err(), "refused: comments are still in the content");
    assert_eq!(s.version(), version, "nothing written");
    assert!(s.legacy_comments());
}
