//! Two replicas through `KitSession<DocModel>`: concurrent edits of different paragraphs both
//! survive, one paragraph edited on both keeps both edits (its text merges by character), and the
//! repair after a merge clears references to what the other side deleted.

mod common;

use std::sync::Arc;

use common::{Session, SharedBlobs, assert_no_dangling, assert_replica_clean, assert_same, body_para, edit, first, peer, rich, sync};
use kition_doc_doc::props::{CharProps, NumRef};
use kition_doc_doc::{Comment, Document, InlineObject, ListKind, PartKind, Path, StoryRef, para_block};

fn pair() -> (SharedBlobs, Session, Session) {
    let blobs = SharedBlobs::default();
    let a = first(&rich(), &blobs);
    let b = peer(&a, 2, &blobs);
    (blobs, a, b)
}

fn assert_converged(a: &Session, b: &Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_same("views", &a.model().document, &b.model().document);
    assert_no_dangling(&a.model().document);
    assert_no_dangling(&b.model().document);
    assert_replica_clean(a.replica());
}

fn text(doc: &Document, block: usize) -> String {
    doc.para(StoryRef::Body, &Path::top(block)).map(|p| p.plain_text()).unwrap_or_default()
}

fn type_at_start(doc: &mut Document, block: usize, s: &str) {
    body_para(doc, block).insert_text(0, s, &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn edits_of_different_paragraphs_are_both_kept() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| type_at_start(d, 0, "from a "));
    edit(&mut b, &blobs, |d| type_at_start(d, 2, "from b "));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert!(text(d, 0).starts_with("from a "));
    assert!(text(d, 2).starts_with("from b "));
}

#[test]
fn inserts_at_one_place_and_edits_around_them_merge() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| d.body.insert(1, common::para("inserted by a")));
    edit(&mut b, &blobs, |d| {
        d.body.insert(1, common::para("inserted by b"));
        type_at_start(d, 0, "b: ");
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let all = common::texts(&a.model().document.body);
    assert!(all.iter().any(|t| t == "inserted by a") && all.iter().any(|t| t == "inserted by b"));
    assert!(all[0].starts_with("b: "));
}

#[test]
fn one_paragraph_edited_on_both_keeps_both_edits() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| type_at_start(d, 2, "A "));
    edit(&mut b, &blobs, |d| type_at_start(d, 2, "B "));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let t = text(&a.model().document, 2);
    assert!(t.starts_with("A B Paragraph") || t.starts_with("B A Paragraph"), "both edits survive: {t}");
}

#[test]
fn a_reply_to_a_deleted_comment_becomes_a_comment_of_its_own() {
    let (blobs, mut a, mut b) = pair();
    // A deletes comment 1 (its anchors and its text); its existing reply goes with it.
    edit(&mut a, &blobs, |d| {
        let p = body_para(d, 1);
        let keep: Vec<usize> =
            (0..p.objects.len()).filter(|k| !matches!(p.objects[*k], InlineObject::CommentStart { .. } | InlineObject::CommentEnd { .. })).collect();
        assert!(keep.is_empty());
        let offsets = p.object_offsets();
        for off in offsets.into_iter().rev() {
            p.delete(off, off + kition_doc_doc::para::OBJ.len_utf8()).unwrap_or_else(|e| panic!("{e}"));
        }
        for id in [1, 2] {
            if let Some(c) = d.comments.remove(&id) {
                d.parts.remove(&c.part);
            }
        }
    });
    // B replies to comment 1.
    edit(&mut b, &blobs, |d| {
        let part = d.add_part(PartKind::Comment, vec![common::para("Another reply")]);
        d.comments.insert(3, Comment { author: "Di".into(), parent: Some(1), part, ..Default::default() });
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(d.comments.len(), 1, "only B's reply is left");
    let reply = d.comments.values().next().unwrap_or_else(|| panic!("the reply"));
    assert_eq!(reply.author, "Di");
    assert_eq!(reply.parent, None, "attached to the root");
    assert!(d.parts.get(&reply.part).is_some_and(|p| common::texts(&p.blocks) == ["Another reply"]));
}

#[test]
fn a_deleted_footnote_loses_its_reference_in_an_edited_paragraph() {
    let (blobs, mut a, mut b) = pair();
    let note = |d: &Document| d.parts.iter().find(|(_, p)| p.kind == PartKind::Footnote).map(|(id, _)| *id);
    // A deletes the footnote: its story and its reference.
    edit(&mut a, &blobs, |d| {
        let id = note(d).unwrap_or_else(|| panic!("a footnote"));
        d.parts.remove(&id);
        let p = body_para(d, 0);
        let off = p.object_offsets()[0];
        p.delete(off, off + kition_doc_doc::para::OBJ.len_utf8()).unwrap_or_else(|e| panic!("{e}"));
    });
    // B types in the paragraph holding the reference. Whichever paragraph wins, no reference to
    // the deleted story is left (if B's does, the repair removes it).
    edit(&mut b, &blobs, |d| type_at_start(d, 0, "typed "));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert!(note(d).is_none(), "the footnote is gone");
    let p = d.para(StoryRef::Body, &Path::top(0)).unwrap_or_else(|| panic!("paragraph 0"));
    assert!(!p.objects.iter().any(|o| matches!(o, InlineObject::NoteRef { .. })), "the dangling reference was removed");
}

#[test]
fn accepting_all_changes_against_typing_converges() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |d| {
        // Accept all: drop deleted text, clear every mark, forget the revisions.
        let p = body_para(d, 2);
        for r in p.deleted_ranges().into_iter().rev() {
            p.delete(r.start, r.end).unwrap_or_else(|e| panic!("{e}"));
        }
        p.format(0, p.len(), &|c| {
            c.ins = None;
            c.del = None;
            c.fmt_change = None;
        })
        .unwrap_or_else(|e| panic!("{e}"));
        d.revisions.clear();
    });
    edit(&mut b, &blobs, |d| {
        // Typing with tracked changes on, in another paragraph and in the marked one.
        let rev = u32::try_from(d.revisions.len()).unwrap_or(0);
        d.revisions.push(kition_doc_doc::Revision { kind: kition_doc_doc::RevisionKind::Insert, author: "Bo".into(), date: "d9".into() });
        let props = CharProps { ins: Some(rev), ..Default::default() };
        body_para(d, 0).insert_text(0, "tracked ", &props).unwrap_or_else(|e| panic!("{e}"));
        type_at_start(d, 2, "x");
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(d.revisions.len(), 1, "B's new revision survives the accept");
    assert!(text(d, 0).starts_with("tracked "));
}

#[test]
fn an_emptied_body_gets_one_paragraph() {
    let blobs = SharedBlobs::default();
    let mut doc = Document::new();
    doc.body = vec![common::para("one"), common::para("two")];
    let mut a = first(&doc, &blobs);
    let mut b = peer(&a, 2, &blobs);
    edit(&mut a, &blobs, |d| d.body = vec![Arc::clone(&d.body[1])]);
    edit(&mut b, &blobs, |d| d.body = vec![Arc::clone(&d.body[0])]);
    sync(&mut a, &mut b);
    assert_eq!(a.state_hash().ok(), b.state_hash().ok());
    assert_eq!(common::texts(&a.model().document.body), [""], "one empty paragraph");
    assert_same("views", &a.model().document, &b.model().document);
    // And the views edit on from there.
    edit(&mut a, &blobs, |d| d.body.push(para_block(kition_doc_doc::Paragraph::with_text("again", CharProps::default()))));
    sync(&mut a, &mut b);
    assert_eq!(common::texts(&b.model().document.body), ["", "again"]);
}

#[test]
fn the_repair_removes_a_reference_to_a_deleted_story() {
    let (blobs, mut a, mut b) = pair();
    // A deletes the footnote's story only; B, unaware, types in the paragraph holding its
    // reference: B's paragraph is the only write of it, so only the repair can drop the reference.
    edit(&mut a, &blobs, |d| {
        let id = d.parts.iter().find(|(_, p)| p.kind == PartKind::Footnote).map(|(id, _)| *id).unwrap_or_else(|| panic!("a footnote"));
        d.parts.remove(&id);
    });
    edit(&mut b, &blobs, |d| type_at_start(d, 0, "typed "));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let p = a.model().document.para(StoryRef::Body, &Path::top(0)).cloned().unwrap_or_else(|| panic!("paragraph 0"));
    assert!(p.plain_text().starts_with("typed Paragraph 0"), "B's typing is kept: {}", p.plain_text());
    assert!(p.objects.is_empty(), "the reference is gone");
}

#[test]
fn lists_created_concurrently_are_both_kept() {
    let (blobs, mut a, mut b) = pair();
    let make = |kind: ListKind, block: usize| {
        move |d: &mut Document| {
            let num = d.numbering.add_list(kind);
            body_para(d, block).props.numbering = Some(NumRef { num, level: 0 });
        }
    };
    // `rich` already has a list; each side makes another, with the same local ids.
    edit(&mut a, &blobs, make(ListKind::Bullet, 0));
    edit(&mut b, &blobs, make(ListKind::UpperLetter, 2));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().document;
    assert_eq!(d.numbering.nums.len(), 3, "the old list and both new ones");
    assert_eq!(d.numbering.abstracts.len(), 3, "each with its own definition");
    let level = |block: usize| {
        let n = d.para(StoryRef::Body, &Path::top(block)).and_then(|p| p.props.numbering).unwrap_or_else(|| panic!("paragraph {block} is in a list"));
        d.numbering.level(n.num, 0).map(|l| l.format).unwrap_or_else(|| panic!("list {} has a level 0", n.num))
    };
    assert_ne!(level(0), level(2), "each paragraph keeps the list its side made");
    // A list shared by two `Num`s (restart numbering) keeps one definition.
    edit(&mut a, &blobs, |d| {
        let n = d.para(StoryRef::Body, &Path::top(0)).and_then(|p| p.props.numbering).map(|n| n.num).unwrap_or(0);
        let restarted = d.numbering.restart(n).unwrap_or_else(|| panic!("restart"));
        body_para(d, 1).props.numbering = Some(NumRef { num: restarted, level: 0 });
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &b.model().document;
    assert_eq!((d.numbering.nums.len(), d.numbering.abstracts.len()), (4, 3), "a restart shares its definition");
}
