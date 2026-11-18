//! After a reload, every block that reads back as it was keeps the old view's `Arc`: unchanged
//! paragraphs keep their `rev` (the engine's layout cache key) and pointer identity, so a remote
//! keystroke re-lays out one paragraph, not the document, and the next diff finds the untouched
//! blocks by `Arc` identity.

use std::collections::HashMap;
use std::sync::Arc;

use kition_doc_doc::{Block, Document};
use kition_kit::kdm::ObjectId;

use crate::ids::{BlockIds, Ids};
use crate::write::MAX_DEPTH;

/// Puts `old`'s `Arc` back into `doc` for every block (at any depth) whose object is the same and
/// whose content is equal; a changed table keeps the `Arc`s of its unchanged cell blocks.
pub(crate) fn reuse(old: &Document, old_ids: &Ids, doc: &mut Document, ids: &Ids) {
    let mut by_object: HashMap<&ObjectId, &Arc<Block>> = HashMap::new();
    collect(&old.body, &old_ids.body, &mut by_object, 0);
    for (id, part) in &old.parts {
        if let Some(p) = old_ids.parts.get(id) {
            collect(&part.blocks, &p.blocks, &mut by_object, 0);
        }
    }
    if by_object.is_empty() {
        return;
    }
    put_back(&mut doc.body, &ids.body, &by_object, 0);
    for (id, part) in &mut doc.parts {
        if let Some(p) = ids.parts.get(id) {
            put_back(&mut part.blocks, &p.blocks, &by_object, 0);
        }
    }
}

fn collect<'a>(blocks: &'a [Arc<Block>], ids: &'a [BlockIds], out: &mut HashMap<&'a ObjectId, &'a Arc<Block>>, depth: usize) {
    if depth > MAX_DEPTH || blocks.len() != ids.len() {
        return;
    }
    for (block, block_ids) in blocks.iter().zip(ids) {
        out.insert(&block_ids.object, block);
        if let (Block::Table(t), Some(table)) = (&**block, &block_ids.table) {
            for (row, row_ids) in t.rows.iter().zip(&table.rows) {
                for (cell, cell_ids) in row.cells.iter().zip(&row_ids.cells) {
                    collect(&cell.blocks, &cell_ids.blocks, out, depth + 1);
                }
            }
        }
    }
}

/// Whether two paragraphs differ only in where equal runs split (the text form keeps no
/// boundary between equal runs, so a view's split runs read back merged).
fn same_merged(a: &Block, b: &Block) -> bool {
    let (Block::Para(x), Block::Para(y)) = (a, b) else { return false };
    if x.text != y.text || x.objects != y.objects || x.props != y.props || x.mark != y.mark || x.section != y.section {
        return false;
    }
    let (mut x, mut y) = (x.clone(), y.clone());
    x.normalize();
    y.normalize();
    x.runs == y.runs
}

fn put_back(blocks: &mut [Arc<Block>], ids: &[BlockIds], old: &HashMap<&ObjectId, &Arc<Block>>, depth: usize) {
    if depth > MAX_DEPTH || blocks.len() != ids.len() {
        return;
    }
    for (block, block_ids) in blocks.iter_mut().zip(ids) {
        if let Some(was) = old.get(&block_ids.object)
            && (***was == **block || same_merged(was, block))
        {
            *block = Arc::clone(was);
            continue;
        }
        if let Some(table) = &block_ids.table
            && let Some(Block::Table(t)) = Arc::get_mut(block)
        {
            for (row, row_ids) in t.rows.iter_mut().zip(&table.rows) {
                for (cell, cell_ids) in row.cells.iter_mut().zip(&row_ids.cells) {
                    put_back(&mut cell.blocks, &cell_ids.blocks, old, depth + 1);
                }
            }
        }
    }
}
