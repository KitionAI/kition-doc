//! Comments beside the content (Kition platform rule M23).
//!
//! The view keeps comments as Kition Doc always has: `Document::comments`, each comment's text a
//! part of kind `Comment`, and its range two inline objects in the paragraphs (`CommentStart`,
//! `CommentEnd`). The replica keeps them beside the content, so a change that only comments
//! touches nothing else (what a sync server lets a commenter push):
//!
//! - every comment that opens a thread (no parent, or a parent that is gone) is a `CommentThread`
//!   object; it and its replies, in order, are the thread's `Comment` objects. A comment's
//!   `author` is the principal id of who wrote it (a file's author name for comments read from a
//!   file), `name` the name people see, `body` its plain text, `created` when it was made (the
//!   file's UTC date when it has one); `initials` and the `props` register keep what the file
//!   format has besides (local date, paragraph and durable ids, a reply's own range and its
//!   resolved flag, the parent of a reply to a reply, and the formatted text when the plain text
//!   does not say it all), and the thread it was written in (`thread`); a comment's `id` is its
//!   durable id and a thread's its first comment's, their tool ids (`comment:<id>`, `thread:<id>`,
//!   C8-doc-6): a comment no thread lists
//!   (two replicas made one thread at once and one's list won, or the thread was emptied while
//!   it was written) still shows, in that thread or opening one, so no comment is ever lost;
//! - the content holds no comment anchor: paragraphs are written without their `CommentStart`
//!   and `CommentEnd` objects ([`Stripped`]), and comment texts are no parts. A thread's `anchor`
//!   says where its range is instead: the paragraph object the range starts in, its `text` field,
//!   the range in it (characters of the paragraph's text as read, comment anchors left out) and
//!   the text it held (`quote`), and kit keys: `end` (`{object, offset}`) for a range ending in
//!   another paragraph, `only` (`start` or `end`) for a file's range with one end only, `ranks`
//!   (the order of comment anchors at one place), `startMarks`/`endMarks` (the anchors' run
//!   properties). A comment with no range is anchored on the document (`{object: "document"}`).
//!
//! Reading places each anchor back into its paragraph: at its range while the text there still
//! reads its quote, else at the occurrence of the quote nearest it (as `@kition/kdm`'s
//! `relocateAnchorRange`), else nowhere (the comment stays, with no range). A write rewrites an
//! anchor only when the view's range is not where the stored anchor places it, so typing around a
//! comment does not rewrite it.
//!
//! Character offsets count Unicode scalar values, as everywhere in this crate.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use kition_doc_doc::para::OBJ;
use kition_doc_doc::props::CharProps;
use kition_doc_doc::{Block, Blocks, Comment, Document, InlineObject, Paragraph, Part, PartKind, Path, StoryRef, para_block};
use kition_kit::kdm::{ObjectId, Replica, Tx};
use kition_kit::session::write_text;
use serde_json::{Map, Value, json};

use crate::ids::{BlockIds, Ids, Known, Minter};
use crate::list::{sync_list, sync_set};
use crate::refs::{Locals, Objects};
use crate::text::{TEXT, char_marks, char_props};
use crate::write::{MAX_DEPTH, same_json};
use crate::{KdmResult, kdm_err, root_id};

/// The reserved type of comment threads (rule M23).
pub const THREAD: &str = "CommentThread";
/// The reserved type of comments (rule M23).
pub const COMMENT: &str = "Comment";

/// Most comments of one thread followed up a reply chain.
const MAX_CHAIN: usize = 64;

/// The comment a comment anchor names, and whether it starts the range.
fn marker(o: &InlineObject) -> Option<(u32, bool)> {
    match o {
        InlineObject::CommentStart { id } => Some((*id, true)),
        InlineObject::CommentEnd { id } => Some((*id, false)),
        _ => None,
    }
}

// --- the content without comments ------------------------------------------------------------

/// The content-only form of documents: paragraphs without their comment anchors, no comment
/// texts, no comments. A block without anchors stays the same `Arc` (so the writer still pairs
/// untouched blocks by identity), and one block is stripped into the same `Arc` every time (so a
/// block both `before` and `after` hold stays one).
#[derive(Default)]
pub(crate) struct Stripped(HashMap<usize, Arc<Block>>);

impl Stripped {
    pub(crate) fn doc(&mut self, doc: &Document) -> Document {
        let body = self.blocks(&doc.body, 0);
        let parts = doc
            .parts
            .iter()
            .filter(|(_, part)| part.kind != PartKind::Comment)
            .map(|(id, part)| (*id, Part { kind: part.kind, blocks: self.blocks(&part.blocks, 0) }))
            .collect();
        Document { body, parts, comments: BTreeMap::new(), ..doc.clone() }
    }

    fn blocks(&mut self, blocks: &Blocks, depth: usize) -> Blocks {
        blocks.iter().map(|b| self.block(b, depth)).collect()
    }

    fn block(&mut self, b: &Arc<Block>, depth: usize) -> Arc<Block> {
        let key = Arc::as_ptr(b) as usize;
        if let Some(done) = self.0.get(&key) {
            return done.clone();
        }
        let out = match &**b {
            Block::Para(p) => match strip_para(p) {
                Some(p) => Arc::new(Block::Para(p)),
                None => b.clone(),
            },
            Block::Table(t) if depth < MAX_DEPTH => {
                let mut table = t.clone();
                let mut changed = false;
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        let blocks = self.blocks(&cell.blocks, depth + 1);
                        changed |= blocks.iter().zip(&cell.blocks).any(|(x, y)| !Arc::ptr_eq(x, y));
                        cell.blocks = blocks;
                    }
                }
                if changed { Arc::new(Block::Table(table)) } else { b.clone() }
            }
            Block::Table(_) => b.clone(),
        };
        self.0.insert(key, out.clone());
        out
    }
}

/// `p` without its comment anchors, or `None` when it has none.
fn strip_para(p: &Paragraph) -> Option<Paragraph> {
    if !p.objects.iter().any(|o| marker(o).is_some()) {
        return None;
    }
    let mut out = p.clone();
    out.text = String::with_capacity(p.text.len());
    out.objects = Vec::with_capacity(p.objects.len());
    // Bytes each run loses.
    let mut cut = vec![0usize; p.runs.len()];
    let (mut k, mut run, mut run_end) = (0usize, 0usize, p.runs.first().map_or(0, |r| r.len));
    for (at, c) in p.text.char_indices() {
        while run + 1 < p.runs.len() && at >= run_end {
            run += 1;
            run_end = run_end.saturating_add(p.runs.get(run).map_or(0, |r| r.len));
        }
        if c == OBJ {
            let object = p.objects.get(k);
            k += 1;
            if object.and_then(marker).is_some() {
                if let Some(slot) = cut.get_mut(run) {
                    *slot += c.len_utf8();
                }
                continue;
            }
            if let Some(o) = object {
                out.objects.push(o.clone());
            }
        }
        out.text.push(c);
    }
    out.runs = p
        .runs
        .iter()
        .zip(cut)
        .filter_map(|(r, cut)| {
            let len = r.len.saturating_sub(cut);
            (len > 0).then(|| kition_doc_doc::para::Run { len, props: r.props.clone() })
        })
        .collect();
    Some(out)
}

// --- where the view's anchors are --------------------------------------------------------------

/// A comment anchor in the view: its comment, which end, its paragraph's object, its place in
/// the paragraph's text without comment anchors, its rank among the comment anchors at that
/// place, and its run properties.
#[derive(Clone, Debug)]
struct Marker {
    comment: u32,
    start: bool,
    object: ObjectId,
    offset: usize,
    rank: u32,
    props: CharProps,
}

/// The view's comment anchors, and the text (comment anchors left out) of each paragraph that
/// holds one, by object.
#[derive(Default)]
struct Placed {
    markers: Vec<Marker>,
    texts: HashMap<ObjectId, Vec<char>>,
}

impl Placed {
    fn of(doc: &Document, ids: &Ids) -> Self {
        let mut placed = Self::default();
        placed.blocks(&doc.body, &ids.body, 0);
        for (id, part) in &doc.parts {
            if let Some(part_ids) = ids.parts.get(id) {
                placed.blocks(&part.blocks, &part_ids.blocks, 0);
            }
        }
        placed
    }

    fn blocks(&mut self, blocks: &Blocks, ids: &[BlockIds], depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        for (block, block_ids) in blocks.iter().zip(ids) {
            match &**block {
                Block::Para(p) => self.para(p, &block_ids.object),
                Block::Table(t) => {
                    let Some(table) = &block_ids.table else { continue };
                    for (row, row_ids) in t.rows.iter().zip(&table.rows) {
                        for (cell, cell_ids) in row.cells.iter().zip(&row_ids.cells) {
                            self.blocks(&cell.blocks, &cell_ids.blocks, depth + 1);
                        }
                    }
                }
            }
        }
    }

    fn para(&mut self, p: &Paragraph, object: &ObjectId) {
        if !p.objects.iter().any(|o| marker(o).is_some()) {
            return;
        }
        let mut text = Vec::with_capacity(p.text.len());
        let (mut k, mut last): (usize, Option<(usize, u32)>) = (0, None);
        for (at, c) in p.text.char_indices() {
            if c == OBJ {
                let found = p.objects.get(k).and_then(marker);
                k += 1;
                if let Some((comment, start)) = found {
                    let offset = text.len();
                    let rank = match last {
                        Some((o, r)) if o == offset => r + 1,
                        _ => 0,
                    };
                    last = Some((offset, rank));
                    self.markers.push(Marker { comment, start, object: object.clone(), offset, rank, props: props_at(p, at) });
                    continue;
                }
            }
            text.push(c);
        }
        self.texts.insert(object.clone(), text);
    }

    /// Each comment's first start and first end anchor.
    fn ends(&self) -> HashMap<u32, (Option<&Marker>, Option<&Marker>)> {
        let mut out: HashMap<u32, (Option<&Marker>, Option<&Marker>)> = HashMap::new();
        for m in &self.markers {
            let entry = out.entry(m.comment).or_default();
            if m.start {
                entry.0.get_or_insert(m);
            } else {
                entry.1.get_or_insert(m);
            }
        }
        out
    }
}

/// The run properties at byte `at` of `p`.
fn props_at(p: &Paragraph, at: usize) -> CharProps {
    let mut start = 0usize;
    for r in &p.runs {
        if at < start + r.len {
            return r.props.clone();
        }
        start += r.len;
    }
    p.runs.last().map(|r| r.props.clone()).unwrap_or_default()
}

// --- anchors -----------------------------------------------------------------------------------

fn text_of(chars: &[char], start: usize, end: usize) -> String {
    chars.get(start.min(chars.len())..end.min(chars.len())).map(|s| s.iter().collect()).unwrap_or_default()
}

/// The anchor of a range between `start` and `end` (either may be missing).
fn anchor_of(start: Option<&Marker>, end: Option<&Marker>, texts: &HashMap<ObjectId, Vec<char>>, objects: &Objects) -> KdmResult<Value> {
    let empty = Vec::new();
    let text = |o: &ObjectId| texts.get(o).unwrap_or(&empty);
    let mut a = Map::new();
    let (first, range) = match (start, end) {
        (None, None) => return Ok(json!({ "object": root_id().as_str() })),
        (Some(s), Some(e)) if s.object == e.object => (s, (s.offset, e.offset)),
        (Some(s), Some(e)) => {
            a.insert("end".into(), json!({ "object": e.object.as_str(), "offset": e.offset }));
            (s, (s.offset, text(&s.object).len().max(s.offset)))
        }
        (Some(s), None) => {
            a.insert("only".into(), json!("start"));
            (s, (s.offset, s.offset))
        }
        (None, Some(e)) => {
            a.insert("only".into(), json!("end"));
            (e, (e.offset, e.offset))
        }
    };
    a.insert("object".into(), json!(first.object.as_str()));
    a.insert("field".into(), json!(TEXT));
    a.insert("range".into(), json!({ "start": range.0, "end": range.1 }));
    if range.0 < range.1 {
        let quote = text_of(text(&first.object), range.0, range.1);
        if !quote.is_empty() {
            a.insert("quote".into(), json!(quote));
        }
    }
    let ranks = (start.map_or(0, |m| m.rank), end.map_or(0, |m| m.rank));
    if ranks != (0, 0) {
        a.insert("ranks".into(), json!([ranks.0, ranks.1]));
    }
    for (key, m) in [("startMarks", start), ("endMarks", end)] {
        if let Some(m) = m
            && !m.props.is_empty()
        {
            let marks = char_marks(&m.props, objects)?;
            a.insert(key.into(), Value::Object(marks.into_iter().collect()));
        }
    }
    Ok(Value::Object(a))
}

/// Where `range` is in `text` now: itself while the text there still reads `quote`, else the
/// occurrence of `quote` nearest its start, else `None`. Without a quote, the range as it is if
/// it still fits. Kition platform rule M23; `@kition/kdm`'s `relocateAnchorRange`.
pub(crate) fn relocate(start: usize, end: usize, quote: Option<&str>, text: &[char]) -> Option<(usize, usize)> {
    let quote: Vec<char> = quote.unwrap_or("").chars().collect();
    if quote.is_empty() {
        return (start.max(end) <= text.len()).then_some((start, end));
    }
    if text.get(start..end).is_some_and(|s| s == quote.as_slice()) {
        return Some((start, end));
    }
    let mut best: Option<usize> = None;
    if quote.len() <= text.len() {
        for at in 0..=text.len() - quote.len() {
            if text.get(at..at + quote.len()) == Some(quote.as_slice()) && best.is_none_or(|b| at.abs_diff(start) < b.abs_diff(start)) {
                best = Some(at);
            }
        }
    }
    best.map(|at| (at, at + quote.len()))
}

fn range_of(anchor: &Value) -> Option<(usize, usize)> {
    let r = anchor.get("range")?;
    let n = |key: &str| r.get(key).and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok());
    Some((n("start")?, n("end")?))
}

/// `stored` with its range moved to where it is in `texts` now (unchanged when it cannot be
/// placed): what [`write`] compares a freshly made anchor with.
fn relocated(stored: &Value, texts: &HashMap<ObjectId, Vec<char>>) -> Value {
    let mut out = stored.clone();
    let Some(object) = stored.get("object").and_then(Value::as_str) else { return out };
    let (Some((start, end)), Some(text)) = (range_of(stored), texts.get(&ObjectId::from_string(object))) else { return out };
    if let Some((s, e)) = relocate(start, end, stored.get("quote").and_then(Value::as_str), text)
        && let Some(map) = out.as_object_mut()
    {
        map.insert("range".into(), json!({ "start": s, "end": e }));
    }
    out
}

/// `fresh`, or `stored` when that places the range at the same place.
fn keep_stored(stored: &Value, fresh: Value, texts: &HashMap<ObjectId, Vec<char>>) -> Value {
    if !stored.is_null() && (same_json(stored, &fresh) || same_json(&relocated(stored, texts), &fresh)) { stored.clone() } else { fresh }
}

// --- plain text of a comment -------------------------------------------------------------------

/// A comment's text as plain text: its paragraphs' text (inline objects left out), one per line.
fn plain(blocks: &Blocks) -> String {
    fn walk(blocks: &Blocks, out: &mut Vec<String>, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        for b in blocks {
            match &**b {
                Block::Para(p) => out.push(p.text.chars().filter(|c| *c != OBJ).collect()),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out, depth + 1)),
            }
        }
    }
    let mut lines = Vec::new();
    walk(blocks, &mut lines, 0);
    lines.join("\n")
}

/// The text Kition Doc makes a comment of from plain text: a paragraph per line.
fn blocks_of(text: &str) -> Blocks {
    text.split('\n').map(|line| para_block(Paragraph::with_text(line, CharProps::default()))).collect()
}

// --- write -------------------------------------------------------------------------------------

/// The comment that opens `id`'s thread: up its parents while they are comments of `doc`.
fn root_of(doc: &Document, id: u32) -> u32 {
    let mut at = id;
    for _ in 0..MAX_CHAIN {
        match doc.comments.get(&at).and_then(|c| c.parent) {
            Some(parent) if parent != at && doc.comments.contains_key(&parent) => at = parent,
            _ => break,
        }
    }
    at
}

fn object_str(o: &ObjectId) -> Value {
    Value::String(o.as_str().to_owned())
}

/// Sets `object.field` to `v` unless it holds it. Answers whether it wrote.
fn set(tx: &mut Tx<'_>, object: &ObjectId, field: &str, v: Value) -> KdmResult<bool> {
    if same_json(&tx.get(object, field)?, &v) {
        return Ok(false);
    }
    tx.set(object, field, v)?;
    Ok(true)
}

/// A comment's registers (rule M23 and this kit's own), as written.
fn registers(
    c: &Comment,
    thread: Option<&ObjectId>,
    parent: Option<&ObjectId>,
    own_anchor: Option<Value>,
    blocks: &Blocks,
    objects: &Objects,
    root: bool,
) -> KdmResult<Map<String, Value>> {
    let author = if c.author_id.is_empty() { c.author.clone() } else { c.author_id.clone() };
    let created = if c.date_utc.is_empty() { c.date.clone() } else { c.date_utc.clone() };
    let mut props = Map::new();
    if c.date != created {
        props.insert("date".into(), json!(c.date));
    }
    if !c.date_utc.is_empty() {
        props.insert("dateUtc".into(), json!(c.date_utc));
    }
    if !c.para_id.is_empty() {
        props.insert("paraId".into(), json!(c.para_id));
    }
    if !c.durable_id.is_empty() {
        props.insert("durableId".into(), json!(c.durable_id));
    }
    if !root && c.resolved {
        props.insert("resolved".into(), json!(true));
    }
    if let Some(thread) = thread {
        props.insert("thread".into(), object_str(thread));
    }
    if let Some(parent) = parent {
        props.insert("replyTo".into(), object_str(parent));
    }
    if let Some(anchor) = own_anchor {
        props.insert("anchor".into(), anchor);
    }
    if *blocks != blocks_of(&plain(blocks)) {
        let v = serde_json::to_value(blocks).map_err(kdm_err)?;
        props.insert("blocks".into(), objects.translated(v));
    }
    let mut out = Map::new();
    // Its tool id (C8-doc-6): its durable id, which the file keeps.
    out.insert("id".into(), if c.durable_id.is_empty() { Value::Null } else { json!(c.durable_id) });
    out.insert("author".into(), json!(author));
    out.insert("name".into(), if c.author.is_empty() { Value::Null } else { json!(c.author) });
    out.insert("created".into(), json!(created));
    out.insert("initials".into(), if c.initials.is_empty() { Value::Null } else { json!(c.initials) });
    out.insert("props".into(), if props.is_empty() { Value::Null } else { Value::Object(props) });
    Ok(out)
}

/// Writes the comments of `after` (the view; `before` the view `ids` described) as threads, with
/// `ids`' content objects already those of `after`; leaves `ids`' comment maps describing `after`.
/// `migrating`: `ids.comments` are the comments as schema 2 wrote them, deleted here, and every
/// comment is made anew under an id derived from its old object's. Answers whether it wrote.
pub(crate) fn write(
    tx: &mut Tx<'_>,
    before: Option<&Document>,
    after: &Document,
    ids: &mut Ids,
    objects: &Objects,
    migrating: bool,
) -> KdmResult<bool> {
    let placed = Placed::of(after, ids);
    let ends = placed.ends();
    let old_comments = std::mem::take(&mut ids.comments);
    let old_threads = std::mem::take(&mut ids.threads);
    let old_bodies = std::mem::take(&mut ids.comment_bodies);
    let mut wrote = false;
    let anchor = |id: u32| -> KdmResult<Option<Value>> {
        match ends.get(&id) {
            Some((s, e)) => anchor_of(*s, *e, &placed.texts, objects).map(Some),
            None => Ok(None),
        }
    };

    if migrating {
        for object in old_comments.values() {
            if tx.exists(object) {
                tx.delete(object)?;
                wrote = true;
            }
        }
        wrote |= sync_set(tx, &root_id(), "comments", &[])? > 0;
    }
    // The id a migrated comment's or thread's object gets: derived from its old one, so replicas
    // migrating one replica offline make the same objects.
    let derived = |prefix: &str, id: u32| old_comments.get(&id).map(|o| ObjectId::from_string(format!("{prefix}-{}", o.as_str())));

    // Comments: the same object while the comment is.
    let mut comments: BTreeMap<u32, ObjectId> = BTreeMap::new();
    let mut fresh: Vec<u32> = Vec::new();
    for id in after.comments.keys() {
        let existed = !migrating && before.is_some_and(|b| b.comments.contains_key(id));
        match old_comments.get(id) {
            Some(object) if existed && tx.exists(object) => {
                comments.insert(*id, object.clone());
            }
            _ => fresh.push(*id),
        }
    }

    // Threads: one per comment that opens one, holding it and its replies.
    let mut members: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for id in after.comments.keys() {
        members.entry(root_of(after, *id)).or_default().push(*id);
    }
    let mut threads: BTreeMap<u32, ObjectId> = BTreeMap::new();
    for root in members.keys() {
        let Some(c) = after.comments.get(root) else { continue };
        let fresh_anchor = anchor(*root)?.unwrap_or_else(|| json!({ "object": root_id().as_str() }));
        let kept = old_threads.get(root).filter(|t| comments.contains_key(root) && tx.exists(t)).cloned();
        // Its tool id (C8-doc-6): its first comment's durable id.
        let thread_id = if c.durable_id.is_empty() { Value::Null } else { json!(c.durable_id) };
        let thread = match kept {
            Some(thread) => {
                let stored = tx.get(&thread, "anchor")?;
                wrote |= set(tx, &thread, "anchor", keep_stored(&stored, fresh_anchor, &placed.texts))?;
                wrote |= set(tx, &thread, "resolved", json!(c.resolved))?;
                wrote |= set(tx, &thread, "id", thread_id)?;
                thread
            }
            None => {
                let mut fields = Map::new();
                fields.insert("anchor".into(), fresh_anchor);
                fields.insert("resolved".into(), json!(c.resolved));
                if !thread_id.is_null() {
                    fields.insert("id".into(), thread_id);
                }
                wrote = true;
                match migrating.then(|| derived("cth", *root)).flatten() {
                    Some(chosen) => tx.create_with_id(THREAD, &chosen, fields)?,
                    None => tx.create(THREAD, fields)?,
                }
            }
        };
        threads.insert(*root, thread);
    }

    let empty: Blocks = Vec::new();
    let blocks_for = |c: &Comment| after.parts.get(&c.part).map_or(&empty, |p| &p.blocks);
    let thread_of = |id: u32| threads.get(&root_of(after, id));
    // New comments first, so a reply can name its parent's object.
    for id in &fresh {
        let Some(c) = after.comments.get(id) else { continue };
        let mut fields = registers(c, thread_of(*id), None, None, blocks_for(c), objects, true)?;
        fields.retain(|_, v| !v.is_null());
        let object = match migrating.then(|| derived("cmt", *id)).flatten() {
            Some(chosen) => tx.create_with_id(COMMENT, &chosen, fields)?,
            None => tx.create(COMMENT, fields)?,
        };
        comments.insert(*id, object);
        wrote = true;
    }
    let mut bodies: BTreeMap<u32, String> = BTreeMap::new();
    for (id, c) in &after.comments {
        let Some(object) = comments.get(id).cloned() else { continue };
        let is_new = fresh.contains(id);
        let root = root_of(after, *id);
        let parent = c.parent.filter(|p| *p != root && after.comments.contains_key(p)).and_then(|p| comments.get(&p));
        let blocks = blocks_for(c);
        let mut fields = registers(c, thread_of(*id), parent, if root == *id { None } else { anchor(*id)? }, blocks, objects, root == *id)?;
        // A reply's own range stays as stored while it is where the stored one places it.
        if let Some(Value::Object(props)) = fields.get_mut("props")
            && let Some(fresh_anchor) = props.get("anchor").cloned()
        {
            let stored = if is_new { Value::Null } else { tx.get(&object, "props")?.get("anchor").cloned().unwrap_or(Value::Null) };
            props.insert("anchor".into(), keep_stored(&stored, fresh_anchor, &placed.texts));
        }
        for (field, v) in fields {
            if !(is_new && v.is_null()) {
                wrote |= set(tx, &object, &field, v)?;
            }
        }
        let body = plain(blocks);
        let old = if is_new { "" } else { old_bodies.get(id).map_or("", String::as_str) };
        if body != old {
            write_text(tx, &object, "body", old, &body).map_err(kdm_err)?;
            wrote = true;
        }
        bodies.insert(*id, body);
    }
    let live: HashSet<&ObjectId> = comments.values().collect();
    for object in old_comments.values() {
        if !migrating && !live.contains(object) && tx.exists(object) {
            tx.delete(object)?;
            wrote = true;
        }
    }

    // Each thread lists its comments: in the order it lists them, then the new ones in the
    // view's order; the comment that opens it first.
    for (root, members) in &members {
        let Some(thread) = threads.get(root) else { continue };
        let wanted: Vec<ObjectId> = members.iter().filter_map(|id| comments.get(id).cloned()).collect();
        let wanted_set: HashSet<&ObjectId> = wanted.iter().collect();
        let opener = comments.get(root).cloned();
        let mut order: Vec<ObjectId> = opener.iter().cloned().collect();
        let mut listed: HashSet<ObjectId> = order.iter().cloned().collect();
        for o in tx.list_items(thread, "comments")? {
            if wanted_set.contains(&o) && listed.insert(o.clone()) {
                order.push(o);
            }
        }
        for o in wanted {
            if listed.insert(o.clone()) {
                order.push(o);
            }
        }
        wrote |= sync_list(tx, thread, "comments", &order)? > 0;
    }
    // A thread whose opening comment is gone is emptied, not deleted: a reply someone made to it
    // concurrently still lands in its list, and then opens it (a comment is never lost to a
    // concurrent delete of its thread). A reader skips a thread with no comment.
    let live: HashSet<&ObjectId> = threads.values().collect();
    for thread in old_threads.values() {
        if !live.contains(thread) && tx.exists(thread) {
            wrote |= sync_list(tx, thread, "comments", &[])? > 0;
        }
    }
    ids.comments = comments;
    ids.threads = threads;
    ids.comment_bodies = bodies;
    ids.legacy_comments = false;
    Ok(wrote)
}

// --- read --------------------------------------------------------------------------------------

/// The local id minters a read shares with the rest of the reader.
pub(crate) struct Minters {
    pub comments: Minter,
    pub parts: Minter,
}

/// A comment anchor to place in a paragraph: where, which end of which comment, its rank, its
/// run properties.
struct Placement {
    offset: usize,
    rank: u32,
    /// The comment's object: orders anchors at one place the same in every replica.
    key: String,
    comment: u32,
    start: bool,
    props: CharProps,
}

fn string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Reads the comment threads into `doc` (its comments, their texts as parts) and places their
/// anchors in its paragraphs (`ids` describes `doc`'s content).
pub(crate) fn read(
    replica: &Replica,
    doc: &mut Document,
    ids: &mut Ids,
    minters: &mut Minters,
    known: &Known,
    locals: &mut Locals,
    seen: &mut HashSet<ObjectId>,
) -> KdmResult<()> {
    let types = &replica.schema().types;
    if !types.contains_key(THREAD) {
        return Ok(());
    }
    // Every thread with its comments: each comment once (a concurrent delete may leave a deleted
    // one listed, and two replicas that migrated one thread offline list its comments twice).
    let mut threads: Vec<(ObjectId, Vec<ObjectId>)> = Vec::new();
    let mut listed: HashSet<ObjectId> = HashSet::new();
    let mut found = replica.objects_of_type(THREAD);
    found.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    for thread in found {
        let mut items = Vec::new();
        for item in replica.list(&thread, "comments")? {
            if replica.exists(&item) && replica.object_type(&item).is_ok_and(|ty| ty == COMMENT) && listed.insert(item.clone()) {
                items.push(item);
            }
        }
        threads.push((thread, items));
    }
    // A comment no thread lists (its thread's list was replaced by a concurrent one: two
    // replicas made one thread at once; or it was deleted while the comment was made) is never
    // lost: it shows in the thread it was made in (its `props.thread`), else opens one.
    let mut orphans: Vec<(String, ObjectId)> = replica
        .objects_of_type(COMMENT)
        .into_iter()
        .filter(|o| !listed.contains(o) && !seen.contains(o))
        .filter_map(|o| Some((replica.get(&o, "created").ok()?.as_str()?.to_owned(), o)))
        .collect();
    orphans.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.as_str().cmp(b.1.as_str())));
    for (_, orphan) in orphans {
        let home = replica.get(&orphan, "props").ok().and_then(|p| string(&p, "thread")).map(ObjectId::from_string);
        match home.as_ref().and_then(|h| threads.iter_mut().find(|(t, _)| t == h)) {
            Some((_, items)) => items.push(orphan),
            None => threads.push((home.filter(|h| replica.exists(h)).unwrap_or_else(|| orphan.clone()), vec![orphan])),
        }
    }
    // In the order they were opened (their first comment's `created`, then id).
    let mut threads: Vec<(String, ObjectId, Vec<ObjectId>)> = threads
        .into_iter()
        .filter_map(|(thread, items)| {
            let opened = replica.get(items.first()?, "created").ok()?.as_str().unwrap_or("").to_owned();
            Some((opened, thread, items))
        })
        .collect();
    threads.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.as_str().cmp(b.1.as_str())));

    let mut placements: Vec<(ObjectId, Value, u32)> = Vec::new();
    for (_, thread, items) in threads {
        let local: Vec<u32> = items
            .iter()
            .map(|item| {
                let id = minters.comments.mint(known.comments.get(item).copied());
                locals.comments.insert(item.as_str().to_owned(), id);
                id
            })
            .collect();
        let root = local.first().copied().unwrap_or(0);
        // A comment that opens a thread no object holds (see above) has no thread's fields.
        let is_thread = replica.exists(&thread) && replica.object_type(&thread).is_ok_and(|ty| ty == THREAD);
        let resolved = is_thread && replica.get(&thread, "resolved")?.as_bool().unwrap_or(false);
        for (n, (item, id)) in items.iter().zip(&local).enumerate() {
            seen.insert(item.clone());
            let props = replica.get(item, "props")?;
            let author = replica.get(item, "author")?.as_str().unwrap_or("").to_owned();
            let name = replica.get(item, "name")?.as_str().map(str::to_owned);
            let created = replica.get(item, "created")?.as_str().unwrap_or("").to_owned();
            let body = replica.text(item, "body")?;
            let blocks = props
                .get("blocks")
                .cloned()
                .and_then(|mut v| {
                    locals.translate(&mut v);
                    serde_json::from_value::<Blocks>(v).ok()
                })
                .filter(|b| plain(b) == body)
                .unwrap_or_else(|| blocks_of(&body));
            let part = minters.parts.mint(known.parts.get(item).copied());
            doc.parts.insert(part, Part { kind: PartKind::Comment, blocks });
            let (author, author_id) = match name {
                Some(name) if name != author => (name, author),
                Some(name) => (name, String::new()),
                None => (author, String::new()),
            };
            let parent = (n > 0).then(|| {
                string(&props, "replyTo").and_then(|o| locals.comments.get(&o).copied()).filter(|p| local.contains(p) && p != id).unwrap_or(root)
            });
            let comment = Comment {
                author,
                author_id,
                initials: replica.get(item, "initials")?.as_str().unwrap_or("").to_owned(),
                date: string(&props, "date").unwrap_or_else(|| created.clone()),
                parent,
                resolved: if n == 0 { resolved } else { props.get("resolved").and_then(Value::as_bool).unwrap_or(false) },
                part,
                para_id: string(&props, "paraId").unwrap_or_default(),
                durable_id: string(&props, "durableId").unwrap_or_default(),
                date_utc: string(&props, "dateUtc").unwrap_or_default(),
            };
            doc.comments.insert(*id, comment);
            ids.comments.insert(*id, item.clone());
            ids.comment_bodies.insert(*id, body);
            let anchor = match n {
                0 if is_thread => replica.get(&thread, "anchor")?,
                0 => Value::Null,
                _ => props.get("anchor").cloned().unwrap_or(Value::Null),
            };
            if anchor.is_object() {
                placements.push((item.clone(), anchor, *id));
            }
        }
        if is_thread {
            ids.threads.insert(root, thread);
        }
    }
    place(doc, ids, &placements, locals);
    Ok(())
}

/// Where each paragraph object is in `doc` (its story and path), with its text.
fn paragraphs(doc: &Document, ids: &Ids) -> HashMap<ObjectId, (StoryRef, Path)> {
    fn walk(blocks: &[BlockIds], story: StoryRef, path: &mut Vec<u32>, out: &mut HashMap<ObjectId, (StoryRef, Path)>, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        for (i, b) in blocks.iter().enumerate() {
            path.push(u32::try_from(i).unwrap_or(u32::MAX));
            match &b.table {
                None => {
                    out.insert(b.object.clone(), (story, Path(path.clone())));
                    // A merged paragraph shows in its holder.
                    for m in &b.merged {
                        out.insert(m.object.clone(), (story, Path(path.clone())));
                    }
                }
                Some(t) => {
                    for (r, row) in t.rows.iter().enumerate() {
                        for (c, cell) in row.cells.iter().enumerate() {
                            path.push(u32::try_from(r).unwrap_or(u32::MAX));
                            path.push(u32::try_from(c).unwrap_or(u32::MAX));
                            walk(&cell.blocks, story, path, out, depth + 1);
                            path.truncate(path.len().saturating_sub(2));
                        }
                    }
                }
            }
            path.pop();
        }
    }
    let mut out = HashMap::new();
    walk(&ids.body, StoryRef::Body, &mut Vec::new(), &mut out, 0);
    for (id, part) in &ids.parts {
        if doc.parts.contains_key(id) {
            walk(&part.blocks, StoryRef::Part(*id), &mut Vec::new(), &mut out, 0);
        }
    }
    out
}

/// How many paragraphs, from an anchor's own on, its range is looked for in.
const FOLLOWING: usize = 32;

/// The paragraph of `run` (paragraphs and their lengths, in order) that place `pos` of their
/// texts put together is in, and the place in it. A place between two paragraphs is the end of
/// the first, as a file puts an anchor there.
fn locate(run: &[((StoryRef, Path), usize)], pos: usize) -> ((StoryRef, Path), usize) {
    let mut at = pos;
    for (i, (place, len)) in run.iter().enumerate() {
        if at <= *len || i + 1 == run.len() {
            return (place.clone(), at.min(*len));
        }
        at -= len;
    }
    run.first().map_or(((StoryRef::Body, Path::top(0)), 0), |(place, _)| (place.clone(), 0))
}

/// Places the anchors (`(comment object, anchor, comment)`) in `doc`'s paragraphs.
fn place(doc: &mut Document, ids: &Ids, anchors: &[(ObjectId, Value, u32)], locals: &Locals) {
    if anchors.is_empty() {
        return;
    }
    let where_is = paragraphs(doc, ids);
    let text = |doc: &Document, at: &(StoryRef, Path)| -> Option<Vec<char>> { Some(doc.para(at.0, &at.1)?.text.chars().collect()) };
    let marks = |v: Option<&Value>| -> CharProps {
        match v {
            Some(Value::Object(map)) => char_props(&map.clone().into_iter().collect(), locals),
            _ => CharProps::default(),
        }
    };
    let mut by_para: BTreeMap<(StoryRef, Path), Vec<Placement>> = BTreeMap::new();
    for (key, anchor, comment) in anchors {
        let Some(object) = anchor.get("object").and_then(Value::as_str).map(ObjectId::from_string) else { continue };
        let (Some(range), Some(at)) = (range_of(anchor), where_is.get(&object).cloned()) else { continue };
        // The paragraph and the ones after it in its story or cell: a paragraph split since the
        // anchor was made (here or concurrently) holds the rest of its text in the next ones.
        let mut run: Vec<((StoryRef, Path), usize)> = Vec::new();
        let mut chars: Vec<char> = Vec::new();
        for i in 0..FOLLOWING {
            let next = (at.0, at.1.with_last(at.1.last() + i));
            let Some(t) = text(doc, &next) else { break };
            run.push((next, t.len()));
            chars.extend(t);
        }
        let Some((start, end)) = relocate(range.0, range.1, anchor.get("quote").and_then(Value::as_str), &chars) else { continue };
        let ranks = anchor.get("ranks").and_then(Value::as_array);
        let rank = |i: usize| ranks.and_then(|r| r.get(i)).and_then(Value::as_u64).and_then(|r| u32::try_from(r).ok()).unwrap_or(0);
        let only = anchor.get("only").and_then(Value::as_str);
        if only != Some("end") {
            let (place, offset) = locate(&run, start);
            by_para.entry(place).or_default().push(Placement {
                offset,
                rank: rank(0),
                key: key.as_str().to_owned(),
                comment: *comment,
                start: true,
                props: marks(anchor.get("startMarks")),
            });
        }
        if only == Some("start") {
            continue;
        }
        let other = anchor.get("end").and_then(|e| {
            let place = where_is.get(&ObjectId::from_string(e.get("object")?.as_str()?))?.clone();
            let len = text(doc, &place)?.len();
            Some((place, e.get("offset").and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).unwrap_or(len).min(len)))
        });
        let (place, offset) = match other {
            Some(found) => found,
            None => locate(&run, if only == Some("end") { start } else { end }),
        };
        by_para.entry(place).or_default().push(Placement {
            offset,
            rank: rank(1),
            key: key.as_str().to_owned(),
            comment: *comment,
            start: false,
            props: marks(anchor.get("endMarks")),
        });
    }
    for ((story, path), mut list) in by_para {
        // In order, then inserted last first, so every offset still counts in the text as read.
        list.sort_by(|a, b| (a.offset, a.rank, &a.key, !a.start).cmp(&(b.offset, b.rank, &b.key, !b.start)));
        let Ok(p) = doc.para_mut(story, &path) else { continue };
        let chars: Vec<usize> = p.text.char_indices().map(|(at, _)| at).chain(std::iter::once(p.text.len())).collect();
        for m in list.iter().rev() {
            let Some(&at) = chars.get(m.offset.min(chars.len().saturating_sub(1))) else { continue };
            let object = if m.start { InlineObject::CommentStart { id: m.comment } } else { InlineObject::CommentEnd { id: m.comment } };
            let _ = p.insert_object(at, object, &m.props);
        }
    }
}
