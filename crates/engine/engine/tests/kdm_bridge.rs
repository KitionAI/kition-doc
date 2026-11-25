//! The command bridge to the document's KDM replica (Kition phase 3e, Task 2): every top-level
//! command replicates as the transactions of its undo steps and reproduces the document in a peer,
//! typing coalesced into one undo step is one transaction per keystroke, undo after a remote edit
//! keeps the remote edit, selection-only commands write nothing, the history limit never loses an
//! edit, a remote deletion of the caret's paragraph leaves a valid selection, two sessions taking
//! turns at random commands agree with the engine's document equal to its own replica after every
//! step, a remote keystroke in a 2,000-paragraph document keeps every other paragraph's layout
//! cache key, an undo past the replica's own step limit is exact and keeps the replica, and an undo
//! whose replica step has nothing left to change takes the step below with it.

use std::sync::Arc;
use std::time::Instant;

use kition_doc_engine::doc::{Block, Document, Path, Pos, StoryRef};
use kition_doc_engine::{MemoryBlobs, Origin, Session, Version, kdm_model};
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.run(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn pos(block: usize, off: usize) -> Value {
    serde_json::to_value(Pos::body(block, off)).unwrap()
}

/// A session with four paragraphs and a 2x2 table, the caret at the start.
fn new_session() -> Session {
    let mut s = Session::new(Document::new());
    for (i, t) in ["Alpha paragraph one.", "Beta paragraph two.", "Gamma paragraph three.", "Delta paragraph four."].iter().enumerate() {
        if i > 0 {
            run(&mut s, "text.newParagraph", json!({}));
        }
        run(&mut s, "text.insert", json!({ "text": t }));
    }
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "table.insert", json!({"rows": 2, "cols": 2}));
    run(&mut s, "caret.docStart", json!({}));
    s
}

/// A second session holding a replica of `a`'s document.
fn peer(a: &mut Session) -> Session {
    let mut b = Session::new(Document::new());
    let blobs = a.blobs_since(&Version::empty()).unwrap();
    b.open_snapshot(&a.snapshot().unwrap(), blobs).unwrap();
    b
}

/// Sends `from`'s changes (and the blobs they need) that `to` lacks.
fn send(from: &mut Session, to: &mut Session) {
    let since = to.version().unwrap();
    let blobs = from.blobs_since(&since).unwrap();
    to.add_blobs(&blobs).unwrap();
    let bytes = from.export_changes(&since).unwrap();
    to.import_changes(&bytes).unwrap();
}

fn sync(a: &mut Session, b: &mut Session) {
    for _ in 0..8 {
        send(a, b);
        send(b, a);
        if a.version().unwrap() == b.version().unwrap() {
            return;
        }
    }
    panic!("the replicas did not converge");
}

fn canonical(d: &Document) -> Value {
    serde_json::to_value(kdm_model::normalize(d)).unwrap()
}

/// `a` and `b` hold the same replicated state and the same document (ids aside).
fn assert_same(a: &mut Session, b: &mut Session, what: &str) {
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap(), "{what}: replicas differ");
    let (x, y) = (canonical(&a.doc), canonical(&b.doc));
    assert!(x == y, "{what}: documents differ:\n{:#?}", differences(&x, &y));
}

/// The document as read back fresh from the session's replica, without writing anything.
fn replica_doc(s: &Session) -> Document {
    let kdm = s.kdm().expect("an edit made the replica");
    kdm_model::read_document(kdm.replica(), kdm.blobs()).unwrap().0
}

/// The engine's document is exactly what its replica holds.
fn assert_holds(s: &Session, what: &str) {
    let (mine, held) = (canonical(&s.doc), canonical(&replica_doc(s)));
    assert!(mine == held, "{what}: the document is not what its replica holds:\n{:#?}", differences(&mine, &held));
}

/// The replica's own undo steps (one per undoable transaction).
fn replica_steps(s: &Session) -> usize {
    s.kdm().map_or(0, |k| k.steps().0)
}

fn last_steps(s: &Session) -> Option<u32> {
    s.kdm_undo_steps().last().copied().flatten()
}

fn para_text(s: &Session, block: usize) -> String {
    s.doc.para_at(&Pos::body(block, 0)).map(|p| p.plain_text()).unwrap_or_default()
}

fn table_index(s: &Session) -> usize {
    s.doc.body.iter().position(|b| matches!(**b, Block::Table(_))).expect("a table")
}

fn png() -> String {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 30, 30, 255]))
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    kition_doc_engine::cmd::insert::base64_encode(&bytes)
}

fn caret_at(s: &mut Session, block: usize, off: usize) {
    run(s, "caret.set", json!({ "pos": pos(block, off) }));
}

fn select(s: &mut Session, from: (usize, usize), to: (usize, usize)) {
    run(s, "select.range", json!({ "anchor": pos(from.0, from.1), "focus": pos(to.0, to.1) }));
}

#[test]
fn each_edit_replicates_and_reproduces_in_a_peer() {
    let cases: Vec<(&str, fn(&mut Session))> = vec![
        ("paste of several paragraphs", |s| {
            select(s, (0, 0), (2, 5));
            run(s, "edit.copy", json!({}));
            caret_at(s, 3, 0);
            run(s, "edit.paste", json!({}));
        }),
        ("replace all", |s| {
            run(s, "edit.replaceAll", json!({"text": "paragraph", "with": "para"}));
        }),
        ("accept all tracked changes", |s| {
            run(s, "change.track", json!({"value": true}));
            caret_at(s, 1, 0);
            run(s, "text.insert", json!({"text": "tracked "}));
            select(s, (2, 0), (2, 5));
            run(s, "text.delete", json!({}));
            run(s, "change.track", json!({"value": false}));
            run(s, "change.acceptAll", json!({}));
        }),
        ("table row insert", |s| {
            let t = table_index(s);
            let at = serde_json::to_value(Pos::new(StoryRef::Body, Path(vec![t as u32, 0, 0, 0]), 0)).unwrap();
            run(s, "caret.set", json!({ "pos": at }));
            run(s, "table.insertRowBelow", json!({}));
        }),
        ("style modify", |s| {
            run(s, "style.set", json!({"style": "Normal", "chr": {"bold": true}}));
        }),
        ("header edit", |s| {
            run(s, "header.insert", json!({"text": "A header"}));
        }),
        ("image insert", |s| {
            caret_at(s, 2, 0);
            run(s, "image.insert", json!({ "data": png(), "alt": "red" }));
        }),
        ("new list", |s| {
            caret_at(s, 1, 0);
            run(s, "paragraph.numbering", json!({}));
        }),
        ("file.versions restore", |s| {
            run(s, "file.versions", json!({"save": "v1"}));
            caret_at(s, 0, 0);
            run(s, "text.insert", json!({"text": "Later "}));
            run(s, "file.versions", json!({"restore": 0}));
        }),
    ];
    for (name, edit) in cases {
        let mut a = new_session();
        let mut b = peer(&mut a);
        let steps = replica_steps(&a);
        let entries = a.undo_depth();
        edit(&mut a);
        assert!(a.undo_depth() > entries, "{name}: an undo step");
        let new: Vec<Option<u32>> = a.kdm_undo_steps()[entries..].to_vec();
        assert!(new.iter().all(Option::is_some), "{name}: every new step is held: {new:?}");
        let total: u32 = new.iter().flatten().sum();
        assert_eq!(replica_steps(&a) - steps, total as usize, "{name}: one replica step per transaction, all the steps'");
        assert!(last_steps(&a) >= Some(1), "{name}: the last step wrote");
        assert_eq!(a.kdm_resets(), 0, "{name}");
        assert_holds(&a, name);
        send(&mut a, &mut b);
        assert_same(&mut a, &mut b, name);
    }
}

#[test]
fn typing_a_word_is_one_undo_step_of_one_transaction_per_keystroke() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    caret_at(&mut a, 1, 0);
    let entries = a.undo_depth();
    let steps = replica_steps(&a);
    for (k, c) in ["W", "o", "r", "d"].iter().enumerate() {
        run(&mut a, "text.insert", json!({ "text": c }));
        // Each keystroke is live for collaborators.
        send(&mut a, &mut b);
        assert!(para_text(&b, 1).starts_with(&format!("{}Beta", &"Word"[..=k])), "keystroke {k} arrived");
    }
    assert_eq!(a.undo_depth(), entries + 1, "one undo step");
    assert_eq!(last_steps(&a), Some(4), "four transactions, one per keystroke");
    assert_eq!(replica_steps(&a) - steps, 4);
    assert!(para_text(&a, 1).starts_with("WordBeta"));
    assert_same(&mut a, &mut b, "typed");
    run(&mut a, "edit.undo", json!({}));
    assert_eq!(para_text(&a, 1), "Beta paragraph two.", "one undo takes the whole word back");
    assert_holds(&a, "after the undo");
    run(&mut a, "edit.redo", json!({}));
    assert!(para_text(&a, 1).starts_with("WordBeta"));
    assert_holds(&a, "after the redo");
}

#[test]
fn undo_after_a_remote_edit_keeps_it() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    caret_at(&mut a, 0, 0);
    run(&mut a, "text.insert", json!({"text": "Local "}));
    send(&mut a, &mut b);
    caret_at(&mut b, 2, 0);
    run(&mut b, "text.insert", json!({"text": "Remote "}));
    run(&mut b, "text.newParagraph", json!({}));
    send(&mut b, &mut a);
    assert_same(&mut a, &mut b, "exchanged");
    run(&mut a, "edit.undo", json!({}));
    assert_eq!(para_text(&a, 0), "Alpha paragraph one.", "the local typing is undone");
    assert!(canonical(&a.doc).to_string().contains("Remote "), "the remote edit survives the local undo");
    assert_eq!(a.kdm_resets(), 0);
    assert_holds(&a, "after the undo");
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "after the undo");
}

#[test]
fn selection_only_commands_write_nothing() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    let version = a.version().unwrap();
    let sel = b.sel.clone();
    for (id, params) in [
        ("caret.docEnd", json!({})),
        ("caret.left", json!({})),
        ("edit.selectAll", json!({})),
        ("caret.docStart", json!({})),
        ("select.word", json!({})),
        ("view.zoom", json!({"value": 1.5})),
        ("edit.copy", json!({})),
    ] {
        let _ = a.run(id, &params);
    }
    caret_at(&mut a, 2, 3);
    assert_eq!(a.version().unwrap(), version, "no transaction");
    send(&mut a, &mut b);
    assert_eq!(b.sel, sel, "the peer's own selection is untouched");
    assert_same(&mut a, &mut b, "nothing to send");
}

#[test]
fn every_edit_replicates_at_the_history_limit() {
    let mut s = new_session();
    s.set_undo_limit(3);
    for i in 0..8 {
        caret_at(&mut s, 0, 0);
        run(&mut s, "text.insert", json!({ "text": format!("{i}") }));
        assert!(replica_doc(&s).plain_text(StoryRef::Body).starts_with(&format!("{i}")), "edit {i} replicated");
        assert_eq!(last_steps(&s), Some(1), "edit {i}: one replica step");
    }
    assert_eq!(s.kdm().unwrap().steps().0, 3, "the replica keeps only the steps the history can take");
    run(&mut s, "edit.undo", json!({}));
    assert!(para_text(&s, 0).starts_with("6543210"), "undo reverts the last edit only: {}", para_text(&s, 0));
    assert_holds(&s, "after the undo");
    for _ in 0..5 {
        let _ = s.run("edit.undo", &json!({}));
    }
    assert!(para_text(&s, 0).starts_with("43210"), "undo stops where the history does: {}", para_text(&s, 0));
    assert_holds(&s, "at the bottom of the history");
    assert_eq!(s.kdm_resets(), 0);
}

#[test]
fn a_remote_deletion_of_the_caret_paragraph_leaves_a_valid_selection() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    caret_at(&mut a, 3, 10);
    select(&mut b, (2, 0), (4, 0));
    run(&mut b, "text.delete", json!({}));
    send(&mut b, &mut a);
    assert!(a.doc.para_at(&a.sel.focus).is_some_and(|p| a.sel.focus.off <= p.text.len()), "the caret is in a paragraph: {:?}", a.sel);
    assert!(a.doc.para_at(&a.sel.anchor).is_some());
    run(&mut a, "text.insert", json!({"text": "still typing "}));
    assert_holds(&a, "an edit after the import");
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "exchanged");

    // A paragraph added above the caret: the caret stays on its paragraph and its character.
    let mut a = new_session();
    let mut b = peer(&mut a);
    caret_at(&mut a, 2, 6);
    caret_at(&mut b, 0, 0);
    run(&mut b, "text.newParagraph", json!({}));
    caret_at(&mut b, 3, 0);
    run(&mut b, "text.insert", json!({"text": ">> "}));
    send(&mut b, &mut a);
    assert_eq!(a.sel.focus.path, Path::top(3), "the caret follows its paragraph");
    assert_eq!(a.sel.focus.off, 9, "and its character, past the remote typing before it");
}

#[test]
fn agent_edits_record_their_origin_and_undo_on_this_replica() {
    let mut s = new_session();
    let agent = Origin::agent("test");
    s.execute_as("edit.replaceAll", &json!({"text": "Alpha", "with": "Agent"}), &agent).unwrap();
    assert_eq!(s.origin(), kition_doc_engine::kdm_bridge::local_person(), "the origin is restored after the call");
    assert!(para_text(&s, 0).starts_with("Agent"));
    // KitModel M12a: the person's Undo takes back the person's own last step, never the agent's.
    let person_steps = s.undo_depth();
    run(&mut s, "edit.undo", json!({}));
    assert!(para_text(&s, 0).starts_with("Agent"), "the person's undo keeps the agent's edit");
    assert_eq!(s.undo_depth(), person_steps - 1);
    run(&mut s, "edit.redo", json!({}));
    // E14a: the agent's scope by its own origin, or by `scope`.
    s.execute_as("edit.undo", &json!({}), &agent).unwrap();
    assert!(para_text(&s, 0).starts_with("Alpha"), "the agent's undo takes back the agent's edit");
    s.execute_as("edit.redo", &json!({}), &agent).unwrap();
    assert!(para_text(&s, 0).starts_with("Agent"));
    run(&mut s, "edit.undo", json!({"scope": "agent"}));
    assert!(para_text(&s, 0).starts_with("Alpha"));
    assert!(s.run("edit.undo", &json!({"scope": "everyone"})).is_err(), "an unknown scope is refused");
}

#[test]
fn an_agent_undo_under_the_persons_later_step_keeps_the_persons_edit() {
    let mut s = new_session();
    let agent = Origin::agent("test");
    s.execute_as("edit.replaceAll", &json!({"text": "Alpha", "with": "Agent"}), &agent).unwrap();
    caret_at(&mut s, 0, 0);
    run(&mut s, "text.insert", json!({"text": "Person: "}));
    assert!(para_text(&s, 0).starts_with("Person: Agent"));
    s.execute_as("edit.undo", &json!({}), &agent).unwrap();
    assert!(para_text(&s, 0).starts_with("Person: Alpha"), "the agent's step goes, the person's later typing stays: {:?}", para_text(&s, 0));
    run(&mut s, "edit.undo", json!({}));
    assert!(para_text(&s, 0).starts_with("Alpha"), "then the person's own undo takes back the typing");
    s.execute_as("edit.redo", &json!({}), &agent).unwrap();
    assert!(para_text(&s, 0).starts_with("Agent"));
}

#[test]
fn a_file_newer_than_its_stored_replica_wins_on_open() {
    let mut a = new_session();
    let snapshot = a.snapshot().unwrap();
    let blobs = a.blobs_since(&Version::empty()).unwrap();
    let doc_id = a.doc_id().unwrap();
    run(&mut a, "edit.replaceAll", json!({"text": "Alpha", "with": "Newer"}));
    let file = kdm_model::encode("x.docx", &a.doc).unwrap();

    let mut b = Session::new(Document::new());
    b.open_with_replica(&file, &snapshot, blobs.clone()).unwrap();
    assert!(para_text(&b, 0).starts_with("Newer"), "the file's content");
    assert_holds(&b, "recorded onto the replica");
    assert_eq!(b.doc_id().unwrap(), doc_id, "the replica's own identity");
    assert!(!b.dirty, "opening is clean");
    assert_eq!(b.undo_depth(), 0, "opening is not an undo step");

    let mut c = Session::new(Document::new());
    c.open_with_replica(&[], &snapshot, blobs).unwrap();
    send(&mut b, &mut c);
    assert_same(&mut b, &mut c, "a peer of the older replica gets the file's content");
}

#[test]
fn passthroughs_answer_before_any_edit() {
    let mut s = Session::new(Document::new());
    assert!(s.doc_id().is_none(), "no replica before anything asks for one");
    let version = s.version().unwrap();
    assert!(s.doc_id().is_some());
    assert_eq!(s.kdm_resets(), 0);
    assert!(s.export_changes(&Version::empty()).unwrap().len() > 1);
    assert_eq!(s.version().unwrap(), version, "asking writes nothing");
    let blobs: MemoryBlobs = s.blobs().unwrap();
    assert!(blobs.0.is_empty(), "a new document has no pictures");
}

#[test]
fn opening_another_document_replaces_it_on_the_replica() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    run(&mut a, "file.new", json!({"template": "letter"}));
    assert_eq!(a.undo_depth(), 0);
    assert_holds(&a, "the new document");
    send(&mut a, &mut b);
    assert_same(&mut a, &mut b, "replaced");
}

/// A tiny deterministic generator (no extra dependency).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % (n.max(1) as u64)) as usize
    }
}

/// A random place in a body paragraph (start, end or middle), or `None` if the block is a table.
fn random_pos(s: &Session, rng: &mut Rng) -> Option<(usize, usize)> {
    let block = rng.below(s.doc.body.len());
    let p = s.doc.para_at(&Pos::body(block, 0))?;
    let len = p.text.len();
    let off = match rng.below(3) {
        0 => 0,
        1 => len,
        _ => p.clamp(rng.below(len + 1)),
    };
    Some((block, off))
}

/// One random command: caret moves and selections, typing (with AutoCorrect), Enter, deletions,
/// paste, replace all, formatting, lists, tables and rows, styles, headers, pictures, tracked
/// changes, versions, undo and redo.
fn random_command(s: &mut Session, rng: &mut Rng, picture: &str) -> String {
    let (id, params): (&str, Value) = match rng.below(24) {
        0 | 1 => match random_pos(s, rng) {
            Some((b, o)) => ("caret.set", json!({ "pos": pos(b, o) })),
            None => ("caret.docEnd", json!({})),
        },
        2 => match (random_pos(s, rng), random_pos(s, rng)) {
            (Some(x), Some(y)) => ("select.range", json!({ "anchor": pos(x.0, x.1), "focus": pos(y.0, y.1) })),
            _ => ("select.word", json!({})),
        },
        3..=6 => {
            let text = ["a", "bc ", "Hello world. ", "teh ", "x"][rng.below(5)];
            ("text.insert", json!({ "text": text }))
        }
        7 => ("text.newParagraph", json!({})),
        8 => ("text.backspace", json!({})),
        9 => ("text.delete", json!({})),
        10 => {
            let _ = s.run("edit.copy", &json!({}));
            ("edit.paste", json!({}))
        }
        11 => {
            let (text, with) = (["a", "paragraph", "Hello"][rng.below(3)], ["A", "para", ""][rng.below(3)]);
            ("edit.replaceAll", json!({ "text": text, "with": with }))
        }
        12 => (["format.bold", "format.italic"][rng.below(2)], json!({})),
        13 => (["paragraph.bullets", "paragraph.numbering"][rng.below(2)], json!({})),
        14 => ("table.insert", json!({"rows": 1 + rng.below(2), "cols": 1 + rng.below(2)})),
        15 => {
            if let Some(t) = s.doc.body.iter().position(|b| matches!(**b, Block::Table(_))) {
                let at = serde_json::to_value(Pos::new(StoryRef::Body, Path(vec![t as u32, 0, 0, 0]), 0)).unwrap();
                let _ = s.run("caret.set", &json!({ "pos": at }));
                ("table.insertRowBelow", json!({}))
            } else {
                ("caret.docStart", json!({}))
            }
        }
        16 => ("style.set", json!({"style": "Normal", "chr": {"italic": rng.below(2) == 0}})),
        17 => ("header.insert", json!({ "text": format!("Header {}", rng.below(100)) })),
        18 => ("image.insert", json!({ "data": picture, "alt": "p" })),
        19 => match rng.below(3) {
            0 => ("change.track", json!({ "value": rng.below(2) == 0 })),
            1 => ("change.acceptAll", json!({})),
            _ => ("change.rejectAll", json!({})),
        },
        20 => match rng.below(2) {
            0 => ("file.versions", json!({"save": "v"})),
            _ => ("file.versions", json!({"restore": 0})),
        },
        21 => ("edit.redo", json!({})),
        _ => ("edit.undo", json!({})),
    };
    let outcome = match s.run(id, &params) {
        Ok(_) => "ok".to_string(),
        Err(e) => format!("refused: {e}"),
    };
    format!("{id} {params} -> {outcome}")
}

/// Two sessions take turns at ~250 random commands with a history limit small enough to wrap,
/// each sending its changes to the other after its command (one line of changes). After every
/// command the document is exactly what its own replica holds and the selection is valid; every
/// few commands both hold the same replicated state and the same document.
#[test]
fn random_commands_keep_document_and_replica_equal_and_peers_agree() {
    let picture = png();
    for seed in [3u64, 11, 2026] {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let mut a = new_session();
        a.set_undo_limit(4);
        let mut b = peer(&mut a);
        b.set_undo_limit(4);
        let mut log: Vec<String> = Vec::new();
        for step in 0..63 {
            for name in ["a", "b"] {
                let (s, other) = if name == "a" { (&mut a, &mut b) } else { (&mut b, &mut a) };
                let what = random_command(s, &mut rng, &picture);
                log.push(format!("{name}: {what}"));
                let context = || format!("seed {seed}, step {step}, {name}; last commands:\n{}", log[log.len().saturating_sub(6)..].join("\n"));
                assert_eq!(s.kdm_resets(), 0, "{}", context());
                let (mine, held) = (canonical(&s.doc), canonical(&replica_doc(s)));
                assert!(mine == held, "the document is not what its replica holds: {}\n{:#?}", context(), differences(&mine, &held));
                assert!(s.doc.para_at(&s.sel.focus).is_some_and(|p| s.sel.focus.off <= p.text.len()), "caret out of range: {}", context());
                send(s, other);
            }
            if step % 4 == 3 {
                sync(&mut a, &mut b);
                assert_same(&mut a, &mut b, &format!("seed {seed}, step {step}"));
            }
        }
        sync(&mut a, &mut b);
        assert_same(&mut a, &mut b, &format!("seed {seed}, final"));
        let mut c = peer(&mut a);
        assert_same(&mut a, &mut c, &format!("seed {seed}: a fresh peer matches"));
    }
}

/// A remote keystroke in a 2,000-paragraph document: the import's time, and that every other
/// paragraph keeps its `Arc` (so its `rev`, the layout cache key) and the relayout is quick.
#[test]
fn a_remote_keystroke_in_a_long_document_reloads_one_paragraph() {
    let mut doc = Document::new();
    doc.body = (0..2000)
        .map(|i| {
            kition_doc_engine::doc::para_block(kition_doc_engine::doc::Paragraph::with_text(
                &format!("Paragraph number {i} of a long document, with some words in it."),
                Default::default(),
            ))
        })
        .collect();
    let mut a = Session::new(doc);
    let mut b = peer(&mut a);
    b.layout();
    let before: Vec<Arc<Block>> = b.doc.body.clone();
    caret_at(&mut a, 1000, 0);
    let t = Instant::now();
    run(&mut a, "text.insert", json!({"text": "x"}));
    let local = t.elapsed();
    let since = b.version().unwrap();
    let bytes = a.export_changes(&since).unwrap();
    let t = Instant::now();
    b.import_changes(&bytes).unwrap();
    let import = t.elapsed();
    let t = Instant::now();
    b.layout();
    let relayout = t.elapsed();
    let kept = b.doc.body.iter().zip(&before).filter(|(x, y)| Arc::ptr_eq(x, y)).count();
    eprintln!("2,000 paragraphs: local keystroke + write {local:?}, remote import {import:?}, relayout {relayout:?}");
    assert_eq!(kept, 1999, "only the typed paragraph is new");
    assert!(para_text(&b, 1000).starts_with("xParagraph"));
}

fn indent(s: &Session, block: usize) -> Option<f32> {
    s.doc.para_at(&Pos::body(block, 0)).and_then(|p| p.props.indent_left)
}

/// More steps than the replica keeps in one scope (100): the newest undo through the replica, the
/// older ones from the history's snapshots. The document is exactly what it was at every step, it
/// stays what its replica holds, and the replica is kept; redo comes back the same way.
#[test]
fn an_undo_past_the_replicas_step_limit_is_exact_and_keeps_the_replica() {
    let mut s = new_session();
    let doc_id = s.doc_id();
    assert!(doc_id.is_some(), "the edits made the replica");
    caret_at(&mut s, 0, 0);
    let mut states = vec![canonical(&s.doc)];
    for i in 1..=120u16 {
        run(&mut s, "paragraph.indents", json!({ "left": f64::from(i) }));
        states.push(canonical(&s.doc));
    }
    assert_eq!(indent(&s, 0), Some(120.0));
    assert!(s.undo_depth() >= 120, "the history holds every step: {}", s.undo_depth());
    for k in (0..120).rev() {
        assert!(s.undo(), "undo to step {k}");
        assert!(canonical(&s.doc) == states[k], "undone to step {k}:\n{:#?}", differences(&canonical(&s.doc), &states[k]));
        assert_holds(&s, &format!("undone to step {k}"));
        assert_eq!((s.kdm_resets(), s.doc_id()), (0, doc_id.clone()), "the replica is kept at step {k}");
    }
    assert_eq!(indent(&s, 0), None);
    for k in 1..=120 {
        assert!(s.redo(), "redo to step {k}");
        assert!(canonical(&s.doc) == states[k], "redone to step {k}:\n{:#?}", differences(&canonical(&s.doc), &states[k]));
        assert_holds(&s, &format!("redone to step {k}"));
        assert_eq!((s.kdm_resets(), s.doc_id()), (0, doc_id.clone()), "the replica is kept at step {k}");
    }
    assert!(!s.can_redo());
    // An edit after it all is written on the same replica.
    run(&mut s, "paragraph.indents", json!({ "left": 7.0 }));
    assert_holds(&s, "an edit after the redos");
    assert_eq!(s.kdm_resets(), 0);
}

/// A person's undo whose last step's paragraph an agent deleted since: the replica passes that
/// step over and takes back the one below in the same call, so the history takes both steps back
/// together, the replica's steps and the history's stay in line (the blob index follows the
/// replica's stacks), and the redo brings the step below back.
#[test]
fn an_undo_that_passes_over_a_step_takes_the_one_below_with_it() {
    let mut s = new_session();
    caret_at(&mut s, 0, 0);
    run(&mut s, "paragraph.indents", json!({ "left": 12.0 }));
    caret_at(&mut s, 3, 0);
    run(&mut s, "paragraph.indents", json!({ "left": 24.0 }));
    assert_eq!((indent(&s, 0), indent(&s, 3)), (Some(12.0), Some(24.0)));
    // The agent deletes paragraph 3, the one the person's last step indented.
    let agent = Origin::agent("test");
    s.execute_as(
        "select.range",
        &json!({ "anchor": pos(2, "Gamma paragraph three.".len()), "focus": pos(3, "Delta paragraph four.".len()) }),
        &agent,
    )
    .unwrap();
    s.execute_as("text.delete", &json!({}), &agent).unwrap();
    assert!(!canonical(&s.doc).to_string().contains("Delta paragraph four."));
    assert_holds(&s, "after the agent's deletion");
    let after_agent = canonical(&s.doc);
    let (entries, (undo, redo)) = (s.undo_depth(), s.kdm().unwrap().steps());
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(indent(&s, 0), None, "the step below is taken back");
    assert_eq!(s.undo_depth(), entries - 2, "both history steps are taken back together");
    assert_eq!(s.kdm().unwrap().steps(), (undo - 2, redo + 1), "the replica's steps, as the index counts them");
    assert_eq!(s.kdm_resets(), 0);
    assert_holds(&s, "after the undo");
    assert!(!canonical(&s.doc).to_string().contains("Delta paragraph four."), "the agent's deletion stays");
    run(&mut s, "edit.redo", json!({}));
    assert!(canonical(&s.doc) == after_agent, "the redo brings the step below back:\n{:#?}", differences(&canonical(&s.doc), &after_agent));
    assert!(!s.can_redo(), "nothing of the person's is left to redo");
    assert_eq!(s.kdm().unwrap().steps(), (undo - 1, redo), "the one redo step brought back both");
    assert_holds(&s, "after the redo");
    // The next undo takes the brought-back step again, exactly.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(indent(&s, 0), None);
    assert_holds(&s, "after the second undo");
    assert_eq!(s.kdm_resets(), 0);
}

/// The first few paths at which two JSON values differ (for a readable failure).
fn differences(a: &Value, b: &Value) -> Vec<String> {
    fn walk(a: &Value, b: &Value, path: String, out: &mut Vec<String>) {
        if out.len() >= 5 || a == b {
            return;
        }
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for key in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                    walk(x.get(key).unwrap_or(&Value::Null), y.get(key).unwrap_or(&Value::Null), format!("{path}.{key}"), out);
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    walk(p, q, format!("{path}[{i}]"), out);
                }
            }
            _ => out.push(format!("{path}: {} vs {}", short(a), short(b))),
        }
    }
    fn short(v: &Value) -> String {
        let s = v.to_string();
        if s.len() > 300 { format!("{}…", s.chars().take(300).collect::<String>()) } else { s }
    }
    let mut out = Vec::new();
    walk(a, b, String::new(), &mut out);
    out
}
