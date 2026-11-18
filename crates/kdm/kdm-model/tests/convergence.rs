//! Seeded random multi-session convergence: ~200 random model-level edits (block insert, delete
//! and move; paragraph edit, split and merge; in-paragraph typing, deletion and formatting, which
//! merge by character; table row and cell edits; comments and replies;
//! tracked changes added and accepted; lists; header and footer edits; pictures) on two or three
//! sessions with random exchanges, and runs weighted toward splits and typing at paragraphs' ends
//! (split anchors). After a final exchange every replica has the same state hash and
//! the same (id-blind) view, and no view holds a dangling reference.

mod common;

use std::sync::Arc;

use common::{Session, SharedBlobs, assert_no_dangling, assert_replica_clean, assert_same, edit, first, peer, rich, sync_all};
use kition_doc_doc::props::{CharProps, NumRef};
use kition_doc_doc::table::{Cell, Row};
use kition_doc_doc::{Block, Comment, Document, InlineObject, ListKind, Paragraph, PartKind, Path, Pos, Revision, RevisionKind, StoryRef};

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// Top-level body paragraphs (their indexes).
fn paras(d: &Document) -> Vec<usize> {
    d.body.iter().enumerate().filter(|(_, b)| matches!(***b, Block::Para(_))).map(|(i, _)| i).collect()
}

fn tables(d: &Document) -> Vec<usize> {
    d.body.iter().enumerate().filter(|(_, b)| matches!(***b, Block::Table(_))).map(|(i, _)| i).collect()
}

/// A random character boundary of `p`.
fn offset(p: &Paragraph, rng: &mut Rng) -> usize {
    p.clamp(rng.below(p.len() + 1))
}

fn body_para(d: &mut Document, i: usize) -> &mut Paragraph {
    common::body_para(d, i)
}

/// The engine keeps a story from ending with a table, and never empty.
fn tidy(d: &mut Document) {
    d.ensure_nonempty();
}

/// Removes every anchor of comment `id` and its replies, the comments and their parts.
fn delete_comment(d: &mut Document, id: u32) {
    let mut doomed = vec![id];
    let mut i = 0;
    while let Some(c) = doomed.get(i).copied() {
        doomed.extend(d.comments.iter().filter(|(_, r)| r.parent == Some(c)).map(|(k, _)| *k));
        i += 1;
    }
    for c in &doomed {
        if let Some(comment) = d.comments.remove(c) {
            d.parts.remove(&comment.part);
        }
    }
    let stories: Vec<StoryRef> = std::iter::once(StoryRef::Body).chain(d.parts.keys().map(|k| StoryRef::Part(*k))).collect();
    for story in stories {
        for path in d.para_paths(story) {
            let Ok(p) = d.para_mut(story, &path) else { continue };
            let offs = p.object_offsets();
            for (k, off) in offs.into_iter().enumerate().rev() {
                if matches!(p.objects.get(k), Some(InlineObject::CommentStart { id } | InlineObject::CommentEnd { id }) if doomed.contains(id)) {
                    let _ = p.delete(off, off + kition_doc_doc::para::OBJ.len_utf8());
                }
            }
        }
    }
}

fn random_edit(d: &mut Document, rng: &mut Rng, blobs_seed: u64) {
    let ps = paras(d);
    let Some(&pi) = ps.get(rng.below(ps.len())) else {
        tidy(d);
        return;
    };
    match rng.below(23) {
        // Block insert, delete, move.
        0 | 1 => {
            let at = rng.below(d.body.len() + 1);
            d.body.insert(at, common::para(&format!("new {}", rng.below(1000))));
        }
        2 if d.body.len() > 2 => {
            d.body.remove(rng.below(d.body.len()));
        }
        3 if d.body.len() > 1 => {
            let moved = d.body.remove(rng.below(d.body.len()));
            let to = rng.below(d.body.len() + 1);
            d.body.insert(to, moved);
        }
        // Paragraph edits.
        4..=6 => {
            let p = body_para(d, pi);
            let off = offset(p, rng);
            let _ = p.insert_text(off, &format!("t{}", rng.below(100)), &CharProps::default());
        }
        7 => {
            let p = body_para(d, pi);
            let (a, b) = (offset(p, rng), offset(p, rng));
            let _ = p.delete(a.min(b), a.max(b));
        }
        8 => {
            let off = offset(body_para(d, pi), rng);
            let _ = d.split_paragraph(&Pos::new(StoryRef::Body, Path::top(pi), off));
        }
        9 => {
            if let Some(Block::Para(next)) = d.body.get(pi + 1).map(|b| (**b).clone()) {
                body_para(d, pi).append(next);
                d.body.remove(pi + 1);
            }
        }
        // Tables.
        10 => {
            let ts = tables(d);
            if let Some(&ti) = ts.get(rng.below(ts.len()))
                && let Block::Table(t) = Arc::make_mut(&mut d.body[ti])
            {
                let cols = t.rows.first().map_or(1, |r| r.cells.len());
                let at = rng.below(t.rows.len() + 1);
                if rng.chance(70) || t.rows.len() < 2 {
                    t.rows.insert(at, Row { props: Default::default(), cells: (0..cols).map(|c| Cell::with_text(&format!("n{c}"))).collect() });
                } else {
                    t.rows.remove(at.min(t.rows.len() - 1));
                }
            } else {
                let mut t = kition_doc_doc::Table::new(2, 2, 200.0);
                t.rows[0].cells[0] = Cell::with_text("cell");
                d.body.insert(pi, Arc::new(Block::Table(t)));
            }
        }
        11 => {
            let ts = tables(d);
            if let Some(&ti) = ts.get(rng.below(ts.len()))
                && let Block::Table(t) = Arc::make_mut(&mut d.body[ti])
                && !t.rows.is_empty()
            {
                let r = rng.below(t.rows.len());
                let row = &mut t.rows[r];
                if !row.cells.is_empty() {
                    let c = rng.below(row.cells.len());
                    let tag = rng.below(100);
                    if let Some(Block::Para(p)) = row.cells[c].blocks.first_mut().map(Arc::make_mut) {
                        let _ = p.insert_text(0, &format!("e{tag} "), &CharProps::default());
                    }
                }
            }
        }
        // Comments and replies.
        12 => {
            let id = d.comments.keys().next_back().map_or(1, |k| k + 1);
            let part = d.add_part(PartKind::Comment, vec![common::para(&format!("comment {id}"))]);
            let parent = if rng.chance(50) { d.comments.keys().nth(rng.below(d.comments.len())).copied() } else { None };
            d.comments.insert(id, Comment { author: format!("a{}", rng.below(3)), parent, part, ..Default::default() });
            if parent.is_none() {
                let p = body_para(d, pi);
                let a = offset(p, rng);
                let _ = p.insert_object(a, InlineObject::CommentStart { id }, &CharProps::default());
                let end = p.len();
                let _ = p.insert_object(end, InlineObject::CommentEnd { id }, &CharProps::default());
            }
        }
        13 => {
            if let Some(id) = d.comments.keys().nth(rng.below(d.comments.len())).copied() {
                delete_comment(d, id);
            }
        }
        // Tracked changes: add one, or accept all.
        14 => {
            let rev = u32::try_from(d.revisions.len()).unwrap_or(0);
            d.revisions.push(Revision { kind: RevisionKind::Insert, author: format!("r{}", rng.below(3)), date: format!("d{}", rng.below(9)) });
            let props = CharProps { ins: Some(rev), ..Default::default() };
            let p = body_para(d, pi);
            let off = offset(p, rng);
            let _ = p.insert_text(off, "ins", &props);
        }
        15 => {
            let stories: Vec<StoryRef> = std::iter::once(StoryRef::Body).chain(d.parts.keys().map(|k| StoryRef::Part(*k))).collect();
            for story in stories {
                for path in d.para_paths(story) {
                    let Ok(p) = d.para_mut(story, &path) else { continue };
                    for r in p.deleted_ranges().into_iter().rev() {
                        let _ = p.delete(r.start, r.end);
                    }
                    let len = p.len();
                    let _ = p.format(0, len, &|c| {
                        c.ins = None;
                        c.del = None;
                        c.fmt_change = None;
                    });
                }
            }
            d.revisions.clear();
        }
        // Lists.
        16 => {
            let num = if d.numbering.nums.is_empty() || rng.chance(30) {
                d.numbering.add_list(if rng.chance(50) { ListKind::Bullet } else { ListKind::Numbered })
            } else {
                d.numbering.nums[rng.below(d.numbering.nums.len())].id
            };
            let level = rng.below(3) as u8;
            body_para(d, pi).props.numbering = if rng.chance(80) { Some(NumRef { num, level }) } else { None };
        }
        // Headers and footers.
        17 => {
            let hf: Vec<u32> = d.parts.iter().filter(|(_, p)| matches!(p.kind, PartKind::Header | PartKind::Footer)).map(|(k, _)| *k).collect();
            if let Some(&id) = hf.get(rng.below(hf.len())) {
                let story = StoryRef::Part(id);
                let paths = d.para_paths(story);
                if let Some(path) = paths.get(rng.below(paths.len())).cloned() {
                    if rng.chance(70) {
                        let _ = d.insert_text(&Pos::new(story, path, 0), &format!("h{} ", rng.below(100)), &CharProps::default());
                    } else if let Ok(blocks) = d.story_mut(story) {
                        blocks.push(common::para("another header line"));
                    }
                }
            }
        }
        // Formatting over a random range, and typing inside a formatted run.
        19 | 20 => {
            let p = body_para(d, pi);
            let (a, b) = (offset(p, rng), offset(p, rng));
            let on = rng.chance(70).then_some(true);
            let _ = match rng.below(4) {
                0 => p.format(a.min(b), a.max(b), &|c| c.bold = on),
                1 => p.format(a.min(b), a.max(b), &|c| c.italic = on),
                2 => p.format(a.min(b), a.max(b), &|c| c.size = on.map(|_| 14.0)),
                _ => p.format(a.min(b), a.max(b), &|c| c.link = on.map(|_| "https://example.com".to_owned())),
            };
        }
        21 => {
            let p = body_para(d, pi);
            let off = offset(p, rng);
            let props = CharProps { bold: Some(rng.chance(50)), ..Default::default() };
            let _ = p.insert_text(off, &format!("m{}", rng.below(100)), &props);
        }
        // Pictures.
        18 => {
            let bytes: Vec<u8> = common::PNG.iter().copied().chain((blobs_seed ^ rng.next()).to_le_bytes()).collect();
            let media = d.add_media(bytes, "png");
            let image = InlineObject::Image {
                edge: Default::default(),
                geom: None,
                media,
                w: 20.0,
                h: 20.0,
                alt: String::new(),
                float: Default::default(),
                crop: [0.0; 4],
                ole: None,
                adjust: Default::default(),
                uid: 0,
            };
            let p = body_para(d, pi);
            let off = offset(p, rng);
            let _ = p.insert_object(off, image, &CharProps::default());
        }
        _ => {
            // A footnote.
            let note = d.add_part(PartKind::Footnote, vec![common::para("note")]);
            let p = body_para(d, pi);
            let end = p.len();
            let _ = p.insert_object(
                end,
                InlineObject::NoteRef { kind: kition_doc_doc::para::NoteKind::Footnote, id: note, custom: String::new() },
                &CharProps::default(),
            );
        }
    }
    tidy(d);
}

/// Splits and typing at paragraphs' ends, the edits split anchors serve (`pieces`), else any edit.
fn split_edit(d: &mut Document, rng: &mut Rng, blobs_seed: u64) {
    let ps = paras(d);
    let Some(&pi) = ps.get(rng.below(ps.len())) else {
        tidy(d);
        return;
    };
    match rng.below(8) {
        0 | 1 => {
            let off = offset(body_para(d, pi), rng);
            let _ = d.split_paragraph(&Pos::new(StoryRef::Body, Path::top(pi), off));
        }
        2 | 3 => {
            let p = body_para(d, pi);
            let end = p.len();
            let _ = p.insert_text(end, &format!("e{}", rng.below(100)), &CharProps::default());
        }
        4 => {
            if let Some(Block::Para(next)) = d.body.get(pi + 1).map(|b| (**b).clone()) {
                body_para(d, pi).append(next);
                d.body.remove(pi + 1);
            }
        }
        5 if d.body.len() > 2 => {
            d.body.remove(pi);
        }
        _ => random_edit(d, rng, blobs_seed),
    }
    tidy(d);
}

fn run(seed: u64, sessions: usize, ops: usize) {
    run_with(seed, sessions, ops, random_edit);
}

fn run_with(seed: u64, sessions: usize, ops: usize, random_edit: fn(&mut Document, &mut Rng, u64)) {
    let blobs = SharedBlobs::default();
    let a = first(&rich(), &blobs);
    let mut all: Vec<Session> = vec![a];
    for k in 1..sessions {
        let next = peer(&all[0], 2 + k as u64, &blobs);
        all.push(next);
    }
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    for _ in 0..ops {
        let s = rng.below(all.len());
        let mut step = Rng(rng.next() | 1);
        edit(&mut all[s], &blobs, |d| random_edit(d, &mut step, seed));
        if rng.chance(25) {
            let (i, j) = (rng.below(all.len()), rng.below(all.len()));
            if i != j {
                let bytes = all[i].export_changes(&all[j].version()).unwrap_or_else(|e| panic!("export: {e}"));
                all[j].import_changes(&bytes).unwrap_or_else(|e| panic!("seed {seed}: import: {e}"));
            }
        }
    }
    sync_all(&mut all);
    let hash = all[0].state_hash().unwrap_or_else(|e| panic!("hash: {e}"));
    for (k, s) in all.iter().enumerate() {
        assert_eq!(s.state_hash().unwrap_or_else(|e| panic!("hash: {e}")), hash, "seed {seed}: session {k} has another state");
        assert_same(&format!("seed {seed}: session {k}"), &all[0].model().document, &s.model().document);
        assert_no_dangling(&s.model().document);
        assert_replica_clean(s.replica());
    }
    let anchors = all[0]
        .replica()
        .objects_of_type("Block")
        .iter()
        .flat_map(|b| all[0].replica().text_delta(b, "text").unwrap_or_default())
        .filter(|r| r.attributes.get("object").is_some_and(|v| v.get("splitTo").is_some()))
        .count();
    let d = &all[0].model().document;
    eprintln!(
        "seed {seed}: {anchors} split anchors, {} blocks, {} parts, {} comments ({} replies), {} revisions, {} lists, {} media, {} tables",
        d.body.len(),
        d.parts.len(),
        d.comments.len(),
        d.comments.values().filter(|c| c.parent.is_some()).count(),
        d.revisions.len(),
        d.numbering.nums.len(),
        d.media.len(),
        tables(d).len()
    );
    // And the converged view still edits: the next write of an unchanged view is empty.
    let s = &mut all[0];
    let view = s.model().document.clone();
    let fresh = kdm_model::read_document(s.replica(), &blobs).unwrap_or_else(|e| panic!("read: {e}")).0;
    assert_same(&format!("seed {seed}: fresh read"), &view, &fresh);
}

#[test]
fn two_sessions_converge() {
    for seed in 1..=4 {
        run(seed, 2, 200);
    }
}

#[test]
fn three_sessions_converge() {
    for seed in 11..=14 {
        run(seed, 3, 200);
    }
}

#[test]
fn splits_and_typing_at_paragraph_ends_converge() {
    for seed in 21..=24 {
        run_with(seed, 2, 200, split_edit);
    }
    for seed in 31..=33 {
        run_with(seed, 3, 200, split_edit);
    }
}
