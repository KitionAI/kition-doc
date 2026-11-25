//! Shared test helpers: a document exercising every replicated reference, a blob store shared
//! between replicas, and a multi-replica harness over `KitSession<DocModel>`. Each `tests/*.rs`
//! binary uses only some.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use kdm_model::{DocModel, Ids, normalize, write_changes, write_document};
use kition_doc_doc::graphic::{Embedded, EmbeddedRef};
use kition_doc_doc::para::{NoteKind, ShapeKind};
use kition_doc_doc::props::{CharProps, NumRef, PropChange};
use kition_doc_doc::table::Table;
use kition_doc_doc::{Block, Comment, Document, InlineObject, ListKind, Paragraph, PartKind, Pos, Revision, RevisionKind, StoryRef, para_block};
use kition_kit::kdm::{ActorId, DocId, ObjectId, Origin, Replica, Version};
use kition_kit::session::{BlobRef, BlobSink, BlobSource, KitError, KitModel, KitSession, sha256_hex};

pub type Session = KitSession<DocModel>;

/// One blob store every replica of a test shares (blobs live outside the replica).
#[derive(Clone, Default)]
pub struct SharedBlobs(pub Arc<Mutex<HashMap<String, Arc<[u8]>>>>);

impl BlobSource for SharedBlobs {
    fn get(&self, hash: &str) -> Option<Arc<[u8]>> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(hash).cloned()
    }
}

impl BlobSink for SharedBlobs {
    fn put(&mut self, bytes: &[u8], mime: &str) -> BlobRef {
        let hash = sha256_hex(bytes);
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).entry(hash.clone()).or_insert_with(|| Arc::from(bytes));
        BlobRef { hash, mime: mime.to_owned(), size: bytes.len() as u64 }
    }
}

pub fn actor(value: u64) -> ActorId {
    ActorId::from_u64(value).unwrap_or_else(|_| panic!("actor {value}"))
}

pub fn person(name: &str) -> Origin {
    Origin::person(format!("user:{name}"))
}

pub fn para(text: &str) -> Arc<Block> {
    para_block(Paragraph::with_text(text, CharProps::default()))
}

/// The text of every paragraph of a block list, tables descended.
pub fn texts(blocks: &[Arc<Block>]) -> Vec<String> {
    let mut d = Document::new();
    d.body = blocks.to_vec();
    d.para_paths(StoryRef::Body).iter().filter_map(|p| d.para(StoryRef::Body, p).map(Paragraph::plain_text)).collect()
}

/// Appends `obj` at the end of paragraph `block` of `story`.
pub fn append_object(doc: &mut Document, story: StoryRef, block: usize, obj: InlineObject) {
    let path = kition_doc_doc::Path::top(block);
    let off = doc.para(story, &path).map_or(0, Paragraph::len);
    doc.insert_object(&Pos::new(story, path, off), obj, &CharProps::default()).unwrap_or_else(|e| panic!("insert object: {e}"));
}

/// Paragraph `block` of the body, to change.
pub fn body_para(doc: &mut Document, block: usize) -> &mut Paragraph {
    doc.para_mut(StoryRef::Body, &kition_doc_doc::Path::top(block)).unwrap_or_else(|e| panic!("paragraph {block}: {e}"))
}

pub const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 1, 2, 3, 4];

/// A document with every kind of replicated reference: paragraphs, a 3x3 table, a header and a
/// footer, a footnote, a comment with a reply, tracked changes (insert, delete, formatting), a
/// list, a picture (with an OLE object kept from a file), a text box, and a package part kept
/// verbatim.
pub fn rich() -> Document {
    let mut d = Document::new();
    d.body = (0..6).map(|i| para(&format!("Paragraph {i}"))).collect();
    let mut t = Table::new(3, 3, 300.0);
    for (r, row) in t.rows.iter_mut().enumerate() {
        for (c, cell) in row.cells.iter_mut().enumerate() {
            cell.blocks = vec![para(&format!("r{r}c{c}"))];
        }
    }
    d.body.insert(3, Arc::new(Block::Table(t)));

    let header = d.add_part(PartKind::Header, vec![para("Header")]);
    let footer = d.add_part(PartKind::Footer, vec![para("Footer")]);
    d.last_section.headers.default = Some(header);
    d.last_section.footers.default = Some(footer);
    let note = d.add_part(PartKind::Footnote, vec![para("A footnote")]);
    append_object(&mut d, StoryRef::Body, 0, InlineObject::NoteRef { kind: NoteKind::Footnote, id: note, custom: String::new() });

    let text = d.add_part(PartKind::Comment, vec![para("A comment")]);
    d.comments
        .insert(1, Comment { author: "Ann".into(), initials: "A".into(), date: "2026-10-17T10:00:00Z".into(), part: text, ..Default::default() });
    let reply = d.add_part(PartKind::Comment, vec![para("A reply")]);
    d.comments.insert(2, Comment { author: "Bo".into(), parent: Some(1), part: reply, ..Default::default() });
    {
        let p = body_para(&mut d, 1);
        p.insert_object(0, InlineObject::CommentStart { id: 1 }, &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
        let end = p.len();
        p.insert_object(end, InlineObject::CommentEnd { id: 1 }, &CharProps::default()).unwrap_or_else(|e| panic!("{e}"));
    }

    d.revisions = vec![
        Revision { kind: RevisionKind::Insert, author: "Ann".into(), date: "d1".into() },
        Revision { kind: RevisionKind::Delete, author: "Bo".into(), date: "d2".into() },
        Revision { kind: RevisionKind::Format, author: "Cy".into(), date: "d3".into() },
    ];
    {
        let p = body_para(&mut d, 2);
        p.format(0, 4, &|c| c.ins = Some(0)).unwrap_or_else(|e| panic!("{e}"));
        p.format(5, 8, &|c| c.del = Some(1)).unwrap_or_else(|e| panic!("{e}"));
        p.format(8, 10, &|c| c.fmt_change = PropChange::boxed(2, CharProps::default())).unwrap_or_else(|e| panic!("{e}"));
    }

    let list = d.numbering.add_list(ListKind::Numbered);
    body_para(&mut d, 4).props.numbering = Some(NumRef { num: list, level: 0 });
    body_para(&mut d, 5).props.numbering = Some(NumRef { num: list, level: 1 });

    let media = d.add_media(PNG.to_vec(), "png");
    let ole = Embedded {
        xml: "<w:object/>".into(),
        refs: vec![EmbeddedRef { id: "rId9".into(), kind: "oleObject".into(), target: "word/embeddings/x.bin".into(), external: false }],
    };
    let image = InlineObject::Image {
        edge: Default::default(),
        geom: None,
        media,
        w: 40.0,
        h: 30.0,
        alt: "picture".into(),
        float: Default::default(),
        crop: [0.0; 4],
        ole: Some(Arc::new(ole)),
        adjust: Default::default(),
        uid: 0,
    };
    append_object(&mut d, StoryRef::Body, 4, image);
    d.passthrough.insert("word/embeddings/x.bin".into(), Arc::new(vec![1, 2, 3]));

    let story = d.add_part(PartKind::TextBox, vec![para("In the box")]);
    let shape = InlineObject::Shape {
        vml: None,
        kind: ShapeKind::TextBox,
        w: 100.0,
        h: 50.0,
        fill: None,
        stroke: None,
        stroke_width: 1.0,
        float: Default::default(),
        story: Some(story),
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    append_object(&mut d, StoryRef::Body, 5, shape);
    d
}

/// A fresh replica of `doc` (its creating transaction, not an undo step) and its objects.
pub fn new_replica(doc: &Document, blobs: &SharedBlobs) -> (Replica, Ids) {
    let mut ids = Ids::default();
    let mut sink = blobs.clone();
    let (replica, ()) = Replica::create_with(DocModel::schema(), DocId::new(), actor(1), &Origin::system("create"), |tx| {
        write_document(tx, doc, &mut ids, &mut sink).map(|_| ())
    })
    .unwrap_or_else(|error| panic!("create: {error}"));
    (replica, ids)
}

pub fn first(doc: &Document, blobs: &SharedBlobs) -> Session {
    let (replica, _) = new_replica(doc, blobs);
    KitSession::from_replica(replica, Box::new(blobs.clone())).unwrap_or_else(|error| panic!("attach: {error}"))
}

pub fn peer(of: &Session, value: u64, blobs: &SharedBlobs) -> Session {
    let snapshot = of.snapshot().unwrap_or_else(|error| panic!("snapshot: {error}"));
    KitSession::open_snapshot(&snapshot, Box::new(blobs.clone()), actor(value)).unwrap_or_else(|error| panic!("open_snapshot: {error}"))
}

/// Edits `session`'s view with `f` and records what changed, the way the engine bridge will.
pub fn edit(session: &mut Session, blobs: &SharedBlobs, f: impl FnOnce(&mut Document)) {
    let before = session.model().document.clone();
    let mut after = before.clone();
    f(&mut after);
    let mut ids = session.model().ids().clone();
    let mut sink = blobs.clone();
    session
        .record(&person("test"), |tx| write_changes(tx, &before, &after, &mut ids, &mut sink).map(|_| ()).map_err(KitError::from))
        .unwrap_or_else(|error| panic!("record: {error}"));
    session.model_mut().adopt(after, ids);
}

/// [`edit`] recorded with `origin` (an agent's edit, its own undo scope).
pub fn edit_as(session: &mut Session, blobs: &SharedBlobs, origin: &Origin, f: impl FnOnce(&mut Document)) {
    let before = session.model().document.clone();
    let mut after = before.clone();
    f(&mut after);
    let mut ids = session.model().ids().clone();
    let mut sink = blobs.clone();
    session
        .record(origin, |tx| write_changes(tx, &before, &after, &mut ids, &mut sink).map(|_| ()).map_err(KitError::from))
        .unwrap_or_else(|error| panic!("record: {error}"));
    session.model_mut().adopt(after, ids);
}

/// A draft of `of` (a fork with actor `value`, as an agent's draft is made) and the version it
/// follows.
pub fn draft(of: &Session, value: u64, blobs: &SharedBlobs) -> (Session, Version) {
    let draft = of.fork(actor(value), Box::new(blobs.clone())).unwrap_or_else(|error| panic!("fork: {error}"));
    (draft, of.version())
}

/// Takes `draft`'s changes since `base` into `into` (`mergeChanges`) with `origin`.
pub fn merge(into: &mut Session, draft: &Session, base: &Version, origin: &Origin) {
    let bytes = draft.export_changes(base).unwrap_or_else(|error| panic!("export: {error}"));
    into.merge_changes(&bytes, origin).unwrap_or_else(|error| panic!("merge: {error}"));
}

/// The view is exactly what a fresh read of the replica gives.
pub fn assert_fresh(s: &Session) {
    let fresh = kdm_model::read_document(s.replica(), s.blobs()).unwrap_or_else(|error| panic!("read: {error}")).0;
    assert_same("fresh read", &fresh, &s.model().document);
}

/// Writing a fresh read of the replica over a copy of itself sharing no block writes nothing.
pub fn assert_unchanged_writes_nothing(s: &mut Session, blobs: &SharedBlobs) {
    let (read, mut ids) = kdm_model::read_document(s.replica(), s.blobs()).unwrap_or_else(|error| panic!("read: {error}"));
    let mut copy = read.clone();
    copy.body = copy.body.iter().map(|b| Arc::new((**b).clone())).collect();
    for part in copy.parts.values_mut() {
        part.blocks = part.blocks.iter().map(|b| Arc::new((**b).clone())).collect();
    }
    let mut sink = blobs.clone();
    let version = s.version();
    let wrote = s
        .record(&person("test"), |tx| write_changes(tx, &read, &copy, &mut ids, &mut sink).map_err(KitError::from))
        .unwrap_or_else(|error| panic!("record: {error}"));
    assert!(!wrote, "an unchanged view wrote changes");
    assert_eq!(s.version(), version, "an unchanged view wrote changes");
}

/// `theirs` made in a draft of `a` while `mine` is made in `a`, then the draft merged in as the
/// person's step. What the merge wrote reads back exactly; one Undo gives back the exact state and
/// view before the merge, and Redo the merged ones. The draft is an agent's, as the host merges
/// one.
pub fn merge_draft(a: &mut Session, blobs: &SharedBlobs, theirs: impl FnOnce(&mut Document), mine: impl FnOnce(&mut Document)) {
    let (mut d, base) = draft(a, 2, blobs);
    edit(&mut d, blobs, theirs);
    edit(a, blobs, mine);
    let (hash, view) = (a.state_hash().ok(), a.model().document.clone());
    merge(a, &d, &base, &Origin::agent("agent:test"));
    assert_fresh(a);
    let (merged_hash, merged_view) = (a.state_hash().ok(), a.model().document.clone());
    assert!(a.undo().unwrap_or_else(|error| panic!("undo: {error}")).0, "the merge is the person's step");
    assert_eq!(a.state_hash().ok(), hash, "the undo gives back the state before the merge");
    assert_same("undone", &a.model().document, &view);
    assert!(a.redo().unwrap_or_else(|error| panic!("redo: {error}")).0);
    assert_eq!(a.state_hash().ok(), merged_hash, "the redo gives back the merged state");
    assert_same("redone", &a.model().document, &merged_view);
    assert_fresh(a);
}

pub fn send(from: &Session, to: &mut Session) {
    let bytes = from.export_changes(&to.version()).unwrap_or_else(|error| panic!("export: {error}"));
    to.import_changes(&bytes).unwrap_or_else(|error| panic!("import: {error}"));
}

/// Exchanges until every session holds the same version (repairs are changes too).
pub fn sync_all(sessions: &mut [Session]) {
    for _ in 0..8 {
        for i in 0..sessions.len() {
            for j in 0..sessions.len() {
                if i != j {
                    let bytes = sessions[i].export_changes(&sessions[j].version()).unwrap_or_else(|error| panic!("export: {error}"));
                    sessions[j].import_changes(&bytes).unwrap_or_else(|error| panic!("import: {error}"));
                }
            }
        }
        if sessions.windows(2).all(|w| w[0].version() == w[1].version()) {
            return;
        }
    }
    panic!("the replicas did not converge");
}

pub fn sync(a: &mut Session, b: &mut Session) {
    for _ in 0..8 {
        send(a, b);
        send(b, a);
        if a.version() == b.version() {
            return;
        }
    }
    panic!("the replicas did not converge");
}

/// Panics with where `a` and `b` first differ unless they normalize to the same document.
pub fn assert_same(what: &str, a: &Document, b: &Document) {
    let (x, y) = (normalize(a), normalize(b));
    if x == y {
        return;
    }
    let (sa, sb) = (serde_json::to_string(&x).unwrap_or_default(), serde_json::to_string(&y).unwrap_or_default());
    if sa == sb {
        assert_eq!(x.media, y.media, "{what}: media");
        assert_eq!(x.passthrough, y.passthrough, "{what}: passthrough");
        panic!("{what}: the documents differ in what serde skips (embedded objects)");
    }
    let at = sa.bytes().zip(sb.bytes()).position(|(p, q)| p != q).unwrap_or(sa.len().min(sb.len()));
    let from = at.saturating_sub(200);
    let cut = |s: &str| s.get(from..(at + 200).min(s.len())).unwrap_or("").to_owned();
    panic!("{what}: the documents differ near byte {at}:\n  left  {}\n  right {}", cut(&sa), cut(&sb));
}

/// Every reference in `doc` resolves: note references, text boxes, headers and footers, comment
/// anchors, comment parts and parents, tracked changes, list references and pictures.
pub fn assert_no_dangling(doc: &Document) {
    let part = |id: u32, what: &str| assert!(doc.parts.contains_key(&id), "{what} names part {id}, which is gone");
    let rev = |id: u32, what: &str| assert!((id as usize) < doc.revisions.len(), "{what} names revision {id}, which is gone");
    let num = |id: u32| assert!(id == 0 || doc.numbering.nums.iter().any(|n| n.id == id), "a paragraph names list {id}, which is gone");
    let mut sections = vec![doc.last_section.clone()];
    let mut stories = vec![StoryRef::Body];
    stories.extend(doc.parts.keys().map(|k| StoryRef::Part(*k)));
    for story in stories {
        for path in doc.para_paths(story) {
            let Some(p) = doc.para(story, &path) else { continue };
            if let Some(s) = &p.section {
                sections.push((**s).clone());
            }
            for run in &p.runs {
                if let Some(i) = run.props.ins {
                    rev(i, "an insertion");
                }
                if let Some(i) = run.props.del {
                    rev(i, "a deletion");
                }
                if let Some(c) = &run.props.fmt_change {
                    rev(c.rev, "a formatting change");
                }
            }
            if let Some(n) = p.props.numbering {
                num(n.num);
            }
            assert_eq!(p.text.matches(kition_doc_doc::para::OBJ).count(), p.objects.len(), "paragraph {path} objects match its text");
            assert_eq!(p.runs.iter().map(|r| r.len).sum::<usize>(), p.text.len(), "paragraph {path} runs cover its text");
            for o in &p.objects {
                match o {
                    InlineObject::NoteRef { id, .. } => part(*id, "a note reference"),
                    InlineObject::CommentStart { id } | InlineObject::CommentEnd { id } => {
                        assert!(doc.comments.contains_key(id), "an anchor names comment {id}, which is gone");
                    }
                    InlineObject::Image { media, .. } => assert!(doc.media.contains_key(media), "a picture names media {media}, which is gone"),
                    o => {
                        for id in o.text_boxes() {
                            part(id, "a text box");
                        }
                    }
                }
            }
        }
    }
    for s in sections {
        for id in [s.headers.default, s.headers.first, s.headers.even, s.footers.default, s.footers.first, s.footers.even].into_iter().flatten() {
            part(id, "a section");
        }
    }
    for (id, c) in &doc.comments {
        part(c.part, &format!("comment {id}"));
        if let Some(parent) = c.parent {
            assert!(doc.comments.contains_key(&parent), "comment {id} replies to {parent}, which is gone");
        }
    }
}

/// The replica itself holds no list entry naming a deleted object.
pub fn assert_replica_clean(replica: &Replica) {
    let root = ObjectId::from_string("document");
    for field in ["body", "parts", "revisions", "media"] {
        for item in replica.list(&root, field).unwrap_or_default() {
            assert!(replica.exists(&item), "document.{field} lists deleted {item}");
        }
    }
    for (ty, field) in [("Table", "cols"), ("Table", "rows"), ("Row", "cells"), ("Cell", "blocks"), ("Part", "blocks"), ("CommentThread", "comments")]
    {
        for object in replica.objects_of_type(ty) {
            for item in replica.list(&object, field).unwrap_or_default() {
                assert!(replica.exists(&item), "{ty} {object}.{field} lists deleted {item}");
            }
        }
    }
}
