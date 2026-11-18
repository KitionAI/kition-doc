//! [`Ids`]: which replica object each piece of the view is, so [`crate::write_changes`] can find
//! the objects a command changed. Blocks, rows and cells have no ids of their own in the engine:
//! their objects are kept here in trees parallel to the view (a block list's `Vec<BlockIds>` has
//! one entry per block, in order), rebuilt by every read and every write.
//!
//! [`Known`] remembers the local id (part id, comment id, list id) every object a view ever gave
//! one, deleted ones included, so a reload keeps them: a caret in a header stays in that header.

use std::collections::{BTreeMap, HashMap};

use kition_doc_doc::Document;
use kition_kit::kdm::ObjectId;

use crate::text::ParaText;

/// One block's object, and its table's objects if it is a table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockIds {
    pub object: ObjectId,
    pub table: Option<TableIds>,
    /// A paragraph in the text form: its text field's runs as the replica holds them, which the
    /// next write diffs from (`text`). `None` for a table, and for a paragraph in the register
    /// form 0.3.x wrote, whose object has no text field: writing it makes a new object
    /// (migration, K6).
    pub text: Option<ParaText>,
    /// The paragraphs merged into this one (`pieces`), listed right after it: not in the view,
    /// their text shown at the end of this paragraph's.
    pub merged: Vec<BlockIds>,
}

impl BlockIds {
    /// A block with no merged paragraphs.
    pub fn new(object: ObjectId, table: Option<TableIds>, text: Option<ParaText>) -> Self {
        Self { object, table, text, merged: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableIds {
    pub object: ObjectId,
    pub rows: Vec<RowIds>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowIds {
    pub object: ObjectId,
    pub cells: Vec<CellIds>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellIds {
    pub object: ObjectId,
    pub blocks: Vec<BlockIds>,
}

/// A part's object and its blocks' objects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartIds {
    pub object: ObjectId,
    pub blocks: Vec<BlockIds>,
}

/// The objects of a view (see the module docs). `body` and each part's `blocks` are parallel to
/// the view's block lists; `revisions` to `Document::revisions`; the maps are keyed by the view's
/// local ids. A list (`Num`) and an abstract list (`AbstractNum`) have no object: each is named
/// in the replica by a UUID string, the list's the key of its `lists` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ids {
    pub body: Vec<BlockIds>,
    pub parts: BTreeMap<u32, PartIds>,
    /// Each comment's `Comment` object (rule M23), by the view's comment id.
    pub comments: BTreeMap<u32, ObjectId>,
    /// Each thread's `CommentThread` object, by the view's id of the comment that opened it.
    pub threads: BTreeMap<u32, ObjectId>,
    /// Each comment's `body` text as last read or written, which the next write diffs from.
    pub comment_bodies: BTreeMap<u32, String>,
    /// The replica holds comments in its content, as schema 2 wrote them (`Comment` objects the
    /// document lists, their texts as parts, their anchors in the text): writes are refused until
    /// [`crate::migrate_document`] moves them out.
    pub legacy_comments: bool,
    pub revisions: Vec<ObjectId>,
    pub nums: BTreeMap<u32, String>,
    pub abstracts: BTreeMap<u32, String>,
    pub media: BTreeMap<String, ObjectId>,
}

impl Ids {
    /// Whether any paragraph is still in the register form 0.3.x wrote (see [`BlockIds::text`]).
    pub fn has_register_form(&self) -> bool {
        fn any(blocks: &[BlockIds], depth: usize) -> bool {
            depth <= crate::write::MAX_DEPTH
                && blocks.iter().any(|b| match &b.table {
                    None => b.text.is_none(),
                    Some(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| any(&c.blocks, depth + 1))),
                })
        }
        any(&self.body, 0) || self.parts.values().any(|p| any(&p.blocks, 0))
    }

    /// The objects of every paragraph still in the register form 0.3.x wrote.
    pub fn register_form_objects(&self) -> Vec<ObjectId> {
        fn collect(blocks: &[BlockIds], out: &mut Vec<ObjectId>, depth: usize) {
            if depth > crate::write::MAX_DEPTH {
                return;
            }
            for b in blocks {
                match &b.table {
                    None if b.text.is_none() => out.push(b.object.clone()),
                    None => {}
                    Some(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| collect(&c.blocks, out, depth + 1)),
                }
            }
        }
        let mut out = Vec::new();
        collect(&self.body, &mut out, 0);
        for part in self.parts.values() {
            collect(&part.blocks, &mut out, 0);
        }
        out
    }

    /// The object of part (story) `id`.
    pub fn part_object(&self, id: u32) -> Option<&ObjectId> {
        self.parts.get(&id).map(|p| &p.object)
    }

    /// The local id of the part whose object is `object`.
    pub fn part_of(&self, object: &ObjectId) -> Option<u32> {
        self.parts.iter().find(|(_, p)| p.object == *object).map(|(id, _)| *id)
    }

    /// The objects of a story's top-level blocks: the body's (`None`) or part `id`'s.
    pub fn story_blocks(&self, part: Option<u32>) -> Option<&[BlockIds]> {
        match part {
            None => Some(&self.body),
            Some(id) => self.parts.get(&id).map(|p| p.blocks.as_slice()),
        }
    }

    /// The object of the block at `path` (`[block]`, `[block, row, cell, block]`, …, as the
    /// engine's paths) of the body (`None`) or part `id`.
    pub fn block_at(&self, part: Option<u32>, path: &[u32]) -> Option<&ObjectId> {
        let mut blocks = self.story_blocks(part)?;
        let mut rest = path;
        loop {
            let (&b, tail) = rest.split_first()?;
            let block = blocks.get(b as usize)?;
            match tail {
                [] => return Some(&block.object),
                [r, c, more @ ..] => {
                    let row = block.table.as_ref()?.rows.get(*r as usize)?;
                    blocks = &row.cells.get(*c as usize)?.blocks;
                    rest = more;
                }
                _ => return None,
            }
        }
    }

    /// Where block `object` is: its story (`None`: the body) and path, as [`Ids::block_at`] takes.
    pub fn find_block(&self, object: &ObjectId) -> Option<(Option<u32>, Vec<u32>)> {
        fn search(blocks: &[BlockIds], object: &ObjectId, path: &mut Vec<u32>, depth: usize) -> bool {
            if depth > crate::write::MAX_DEPTH {
                return false;
            }
            for (i, block) in blocks.iter().enumerate() {
                path.push(u32::try_from(i).unwrap_or(u32::MAX));
                // A merged paragraph is where its holder is.
                if block.object == *object || block.merged.iter().any(|m| m.object == *object) {
                    return true;
                }
                for (r, row) in block.table.iter().flat_map(|t| t.rows.iter().enumerate()) {
                    for (c, cell) in row.cells.iter().enumerate() {
                        path.push(u32::try_from(r).unwrap_or(u32::MAX));
                        path.push(u32::try_from(c).unwrap_or(u32::MAX));
                        if search(&cell.blocks, object, path, depth + 1) {
                            return true;
                        }
                        path.truncate(path.len().saturating_sub(2));
                    }
                }
                path.pop();
            }
            false
        }
        let mut path = Vec::new();
        if search(&self.body, object, &mut path, 0) {
            return Some((None, path));
        }
        self.parts.iter().find_map(|(id, part)| {
            path.clear();
            search(&part.blocks, object, &mut path, 0).then(|| (Some(*id), path.clone()))
        })
    }
}

/// The local ids objects had in a view. See the module docs.
#[derive(Clone, Debug, Default)]
pub struct Known {
    pub(crate) parts: HashMap<ObjectId, u32>,
    pub(crate) comments: HashMap<ObjectId, u32>,
    pub(crate) nums: HashMap<String, u32>,
    pub(crate) abstracts: HashMap<String, u32>,
    /// The largest id of each kind ever given: fresh ones go above, so an id the engine may still
    /// hold never comes back naming another object.
    pub(crate) max_part: u32,
    pub(crate) max_comment: u32,
    pub(crate) max_num: u32,
    pub(crate) max_abstract: u32,
}

impl Known {
    /// Remembers the local ids of `doc`, whose objects are `ids`.
    pub fn remember(&mut self, doc: &Document, ids: &Ids) {
        for (id, part) in &ids.parts {
            self.parts.insert(part.object.clone(), *id);
        }
        for (id, object) in &ids.comments {
            self.comments.insert(object.clone(), *id);
            // A comment's text is a part of the view but no object of its own (rule M23): its
            // part id is remembered by the comment's object.
            if let Some(c) = doc.comments.get(id)
                && !ids.parts.contains_key(&c.part)
            {
                self.parts.insert(object.clone(), c.part);
            }
        }
        for (id, uuid) in &ids.nums {
            self.nums.insert(uuid.clone(), *id);
        }
        for (id, uuid) in &ids.abstracts {
            self.abstracts.insert(uuid.clone(), *id);
        }
        let max = |keys: &mut dyn Iterator<Item = u32>| keys.max().unwrap_or(0);
        self.max_part = self.max_part.max(max(&mut doc.parts.keys().copied()));
        self.max_comment = self.max_comment.max(max(&mut doc.comments.keys().copied()));
        self.max_num = self.max_num.max(max(&mut doc.numbering.nums.iter().map(|n| n.id)));
        self.max_abstract = self.max_abstract.max(max(&mut doc.numbering.abstracts.iter().map(|a| a.id)));
    }
}

/// Local ids for one read: the remembered one if no object of this read has it yet, else a fresh
/// one above every id given (never 0, which names nothing, except for comments and abstract lists,
/// whose ids start at 0).
pub(crate) struct Minter {
    used: std::collections::HashSet<u32>,
    next: u32,
    zero: bool,
}

impl Minter {
    pub(crate) fn new(floor: u32) -> Self {
        Self { used: std::collections::HashSet::new(), next: floor, zero: false }
    }

    /// A minter that keeps a remembered 0 (comment and abstract list ids).
    pub(crate) fn with_zero(floor: u32) -> Self {
        Self { zero: true, ..Self::new(floor) }
    }

    pub(crate) fn mint(&mut self, known: Option<u32>) -> u32 {
        if let Some(id) = known
            && (id > 0 || self.zero)
            && self.used.insert(id)
        {
            return id;
        }
        loop {
            self.next = self.next.saturating_add(1);
            if self.used.insert(self.next) || self.next == u32::MAX {
                return self.next;
            }
        }
    }
}
