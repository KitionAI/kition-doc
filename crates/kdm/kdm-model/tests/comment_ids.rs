//! A comment's file identity (phase 7 fidelity: its paragraph id, durable id and UTC date, which
//! `commentsExtended`, `commentsIds` and `commentsExtensible` name it by) and the people part
//! replicate, and a replica keeps them across edits, so a saved file's comment parts stay
//! consistent. They are the comment's own `props` register (rule M23 lets a kit add optional
//! fields to `Comment`), and the people part a blob.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, assert_same, edit, first, new_replica, para, peer, sync};
use kdm_model::{decode, encode, read_document};
use kition_doc_doc::{Comment, Document, PartKind};
use serde_json::Value;

const PEOPLE: &str = r#"<w15:people xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml"><w15:person w15:author="Ann"><w15:presenceInfo w15:providerId="None" w15:userId="Ann"/></w15:person></w15:people>"#;

fn document() -> Document {
    let mut d = Document::new();
    d.body = vec![para("Commented")];
    let text = d.add_part(PartKind::Comment, vec![para("Check this")]);
    d.comments.insert(
        1,
        Comment {
            author: "Ann".into(),
            date: "2026-03-01T09:00:00Z".into(),
            resolved: true,
            part: text,
            para_id: "1A2B3C4D".into(),
            durable_id: "5E6F7081".into(),
            date_utc: "2026-03-01T01:00:00Z".into(),
            ..Default::default()
        },
    );
    let reply = d.add_part(PartKind::Comment, vec![para("Done")]);
    d.comments.insert(2, Comment { author: "Ben".into(), parent: Some(1), part: reply, para_id: "0000BBB2".into(), ..Default::default() });
    d.passthrough.insert("word/people.xml".into(), Arc::new(PEOPLE.as_bytes().to_vec()));
    d
}

#[test]
fn comment_ids_and_people_replicate_and_save() {
    let doc = document();
    let blobs = SharedBlobs::default();
    let (replica, ids) = new_replica(&doc, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_same("kept", &doc, &read);
    assert_eq!(read.comments, doc.comments);
    assert_eq!(read.passthrough.get("word/people.xml"), doc.passthrough.get("word/people.xml"));

    // The comments are one thread beside the content (rule M23): the comment that opened it and
    // its reply, each with its file identity in its `props`.
    let thread = &ids.threads[&1];
    assert_eq!(replica.object_type(thread).ok().as_deref(), Some("CommentThread"));
    assert_eq!(replica.list(thread, "comments").ok(), Some(vec![ids.comments[&1].clone(), ids.comments[&2].clone()]));
    assert_eq!(replica.get(thread, "resolved").ok(), Some(Value::Bool(true)));
    let first = &ids.comments[&1];
    assert_eq!(replica.get(first, "author").ok(), Some(Value::from("Ann")));
    assert_eq!(replica.get(first, "created").ok(), Some(Value::from("2026-03-01T01:00:00Z")), "the file's UTC date");
    assert_eq!(replica.text(first, "body").ok().as_deref(), Some("Check this"));
    let props: Value = replica.get(first, "props").unwrap_or_else(|error| panic!("props: {error}"));
    assert_eq!(props.get("durableId").and_then(Value::as_str), Some("5E6F7081"));
    assert_eq!(props.get("paraId").and_then(Value::as_str), Some("1A2B3C4D"));
    assert_eq!(props.get("date").and_then(Value::as_str), Some("2026-03-01T09:00:00Z"));

    // Saved from the replica's view, the comment parts carry the same ids.
    let bytes = encode("Comments.docx", &read).unwrap_or_else(|error| panic!("encode: {error}"));
    let again = decode("Comments.docx", &bytes).unwrap_or_else(|error| panic!("decode: {error}"));
    assert_eq!(again.comments[&1].durable_id, "5E6F7081");
    assert_eq!(again.comments[&1].date_utc, "2026-03-01T01:00:00Z");
    assert_eq!(again.comments[&2].parent, Some(1));
    assert_eq!(again.comments[&2].para_id, "0000BBB2");
    assert!(!again.comments[&2].durable_id.is_empty(), "the reply gets a durable id");
    assert_eq!(again.passthrough.get("word/people.xml").map(|b| b.as_slice()), Some(PEOPLE.as_bytes()));
}

#[test]
fn comment_ids_survive_edits_on_two_replicas() {
    let blobs = SharedBlobs::default();
    let mut a = first(&document(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    // One person resolves the reply, the other edits the first comment's text.
    edit(&mut a, &blobs, |d| d.comments.get_mut(&2).unwrap_or_else(|| panic!("reply")).resolved = true);
    sync(&mut a, &mut b);
    edit(&mut b, &blobs, |d| {
        let part = d.comments[&1].part;
        d.parts.get_mut(&part).unwrap_or_else(|| panic!("part")).blocks = vec![para("Checked")];
    });
    sync(&mut a, &mut b);
    for s in [&a, &b] {
        let d = &s.model().document;
        assert!(d.comments[&2].resolved);
        assert_eq!((d.comments[&1].para_id.as_str(), d.comments[&1].durable_id.as_str()), ("1A2B3C4D", "5E6F7081"));
    }
}
