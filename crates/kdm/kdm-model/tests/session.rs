//! `DocSession`: the engine bridge's replica (phase 3e Task 2): `Send`, record/undo/redo and the
//! exchange with a peer, picture blobs shipped through `blobs_since` and garbage-collected once no
//! reachable undo step needs them, and unchanged blocks keeping their `Arc` (and `rev`) across an
//! import.

mod common;

use std::sync::Arc;

use common::{PNG, assert_same, body_para, person, rich};
use kdm_model::DocSession;
use kition_doc_doc::props::CharProps;
use kition_doc_doc::{Block, Document};
use kition_kit::kdm::Version;
use kition_kit::session::MemoryBlobs;

fn send(from: &DocSession, to: &mut DocSession) {
    let since = to.version();
    to.add_blobs(&MemoryBlobs(from.blobs_since(&since).into_iter().collect())).unwrap();
    to.import_changes(&from.export_changes(&since).unwrap()).unwrap();
}

fn edited(s: &DocSession, f: impl FnOnce(&mut Document)) -> Document {
    let mut d = s.document().clone();
    f(&mut d);
    d
}

fn peer(a: &DocSession) -> DocSession {
    DocSession::open_snapshot(&a.snapshot().unwrap(), MemoryBlobs(a.blobs_since(&Version::empty()).into_iter().collect())).unwrap()
}

fn type_at_start(d: &mut Document, block: usize, text: &str) {
    body_para(d, block).insert_text(0, text, &CharProps::default()).unwrap();
}

#[test]
fn a_session_is_send() {
    fn send_able<T: Send>() {}
    send_able::<DocSession>();
}

#[test]
fn record_undo_redo_and_a_peer_agree() {
    let mut a = DocSession::create(&rich()).unwrap();
    let mut b = peer(&a);
    assert_same("opened", a.document(), b.document());

    let after = edited(&a, |d| type_at_start(d, 0, "Typed "));
    assert_eq!(a.record(&person("a"), &after).unwrap(), 1, "one undo step");
    assert_eq!(a.record(&person("a"), &after).unwrap(), 0, "nothing changed, no step");
    send(&a, &mut b);
    assert!(b.document().plain_text(kition_doc_doc::StoryRef::Body).starts_with("Typed "));
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());
    assert_same("exchanged", a.document(), b.document());

    assert!(a.undo().unwrap().0);
    assert!(!a.document().plain_text(kition_doc_doc::StoryRef::Body).starts_with("Typed "));
    assert!(a.redo().unwrap().0);
    assert!(a.document().plain_text(kition_doc_doc::StoryRef::Body).starts_with("Typed "));
    assert!(!a.redo().unwrap().0, "nothing more to redo");
}

#[test]
fn a_new_picture_ships_its_blob_and_only_that() {
    let mut a = DocSession::create(&rich()).unwrap();
    let mut b = peer(&a);
    let since = b.version();
    let after = edited(&a, |d| {
        d.add_media(vec![0x89, b'P', b'N', b'G', 9, 9, 9], "png");
    });
    a.record(&person("a"), &after).unwrap();
    assert_eq!(a.blobs_since(&since).len(), 1, "only the new picture's bytes");
    send(&a, &mut b);
    assert_eq!(b.document().media.len(), 2);
    assert!(a.blobs_since(&a.version()).is_empty(), "an up-to-date peer needs nothing");
}

#[test]
fn a_deleted_pictures_blob_lives_while_its_undo_step_does() {
    let mut a = DocSession::create(&rich()).unwrap();
    let blobs = a.blobs().0.len();
    let png = kition_kit::session::sha256_hex(PNG);
    assert!(a.blobs().0.contains_key(&png), "the picture and the kept package part");
    let after = edited(&a, |d| d.media.clear());
    a.record(&person("a"), &after).unwrap();
    assert!(a.blobs().0.contains_key(&png), "the undo step can bring it back");
    assert!(a.undo().unwrap().0);
    assert_eq!(a.document().media.len(), 1, "undo brought the picture back");
    assert!(a.redo().unwrap().0);
    a.retain_steps(0, 0);
    assert!(!a.blobs().0.contains_key(&png), "no reachable step needs it any more");
    assert_eq!(a.blobs().0.len(), blobs - 1, "the package part kept verbatim stays");
    assert!(!a.can_undo());
}

#[test]
fn a_picture_whose_blob_arrives_after_its_changes_reads_empty_then_whole() {
    let mut doc = Document::new();
    doc.body = vec![common::para("One"), common::para("Two")];
    let mut a = DocSession::create(&doc).unwrap();
    let mut b = peer(&a);
    let since = b.version();
    let after = edited(&a, |d| {
        d.add_media(PNG.to_vec(), "png");
    });
    a.record(&person("a"), &after).unwrap();
    // The changes arrive first: the view loads, the picture's bytes empty, nothing resets.
    b.import_changes(&a.export_changes(&since).unwrap()).unwrap();
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());
    let key = b.document().media.keys().next().cloned().expect("the picture's media");
    assert!(b.document().media[&key].is_empty());
    assert!(!b.model().missing_blobs().is_empty());
    // The blob arrives: the view reads it.
    assert!(b.add_blobs(&MemoryBlobs(a.blobs_since(&since).into_iter().collect())).unwrap());
    assert_eq!(b.document().media[&key].as_slice(), PNG);
    assert!(b.model().missing_blobs().is_empty());
    assert_same("whole", a.document(), b.document());
}

#[test]
fn an_import_keeps_the_arc_of_every_block_it_did_not_change() {
    let mut doc = rich();
    doc.body.extend((0..50).map(|i| common::para(&format!("Filler {i}"))));
    let mut a = DocSession::create(&doc).unwrap();
    let mut b = peer(&a);
    let before: Vec<Arc<Block>> = b.document().body.clone();
    let after = edited(&a, |d| type_at_start(d, 10, "x"));
    a.record(&person("a"), &after).unwrap();
    send(&a, &mut b);
    let now = &b.document().body;
    assert_eq!(now.len(), before.len());
    let kept = now.iter().zip(&before).filter(|(x, y)| Arc::ptr_eq(x, y)).count();
    assert_eq!(kept, before.len() - 1, "only the edited paragraph is new");
    assert!(!Arc::ptr_eq(&now[10], &before[10]));
}

#[test]
fn comment_zero_and_abstract_list_zero_keep_their_ids_across_a_reload() {
    let mut doc = Document::new();
    doc.body = vec![common::para("Commented text")];
    let part = doc.add_part(kition_doc_doc::PartKind::Comment, vec![common::para("Note")]);
    doc.comments.insert(0, kition_doc_doc::Comment { author: "Ann".into(), part, ..Default::default() });
    {
        let p = body_para(&mut doc, 0);
        p.insert_object(0, kition_doc_doc::InlineObject::CommentStart { id: 0 }, &CharProps::default()).unwrap();
        let end = p.len();
        p.insert_object(end, kition_doc_doc::InlineObject::CommentEnd { id: 0 }, &CharProps::default()).unwrap();
    }
    let list = doc.numbering.add_list(kition_doc_doc::ListKind::Bullet);
    assert_eq!(doc.numbering.abstracts[0].id, 0, "the engine's first abstract list is 0");
    let mut a = DocSession::create(&doc).unwrap();
    let after = edited(&a, |d| type_at_start(d, 0, "x"));
    a.record(&person("a"), &after).unwrap();
    assert!(a.undo().unwrap().0);
    let d = a.document();
    assert!(d.comments.contains_key(&0), "comment 0 is still 0: {:?}", d.comments.keys().collect::<Vec<_>>());
    assert_eq!(d.numbering.abstracts[0].id, 0, "abstract list 0 is still 0");
    assert_eq!(d.numbering.nums[0].id, list);
}
