//! Deterministic repair after a merge, an undo or a redo ([`kition_kit::session::KitModel::validate`]):
//! state only, so every replica writes the same repair for the same merged state.
//!
//! - List entries naming deleted objects are removed, and so are later entries of an object
//!   listed twice (replicas that repaired the same state concurrently both listed the paragraph
//!   below): the reader reads it once, at its first place, so the view is unchanged, and list
//!   positions stay the view's.
//! - A note reference naming a deleted part is removed: its `Inline` object is deleted (its U+FFFC
//!   in the paragraph's text goes in the text repair, `crate::repair`, which `DocModel`'s
//!   `validate` runs next in the same transaction); a text box story, a section's header or
//!   footer, a tracked change (`ins`/`del`, `fmtChange`, `numChange`) or a list reference naming a
//!   deleted part, revision or list is cleared (in a run's marks, the text repair clears it); a
//!   picture of deleted media loses its media key.
//! - A body, part or table cell left without blocks (or a body or cell ending with a table) gets
//!   an empty paragraph, under an id derived from the state, so replicas repairing the same state
//!   create the same object (and readers skip the repeated list entry).

use std::collections::{BTreeSet, HashSet};

use kition_doc_doc::Paragraph;
use kition_kit::kdm::{ObjectId, Tx};
use kition_kit::session::sha256_hex;
use serde_json::{Map, Value, json};

use crate::refs::{Fix, Ref, UNRESOLVED_MEDIA, para_meta, walk};
use crate::text::INLINE;
use crate::{KdmResult, root_id};

pub(crate) fn validate(tx: &mut Tx<'_>, _changed: &BTreeSet<ObjectId>) -> KdmResult<bool> {
    let root = root_id();
    if !tx.exists(&root) {
        return Ok(false);
    }
    let mut wrote = false;

    // Dangling list entries.
    for field in ["body", "parts", "revisions", "media"] {
        wrote |= clean_dangling(tx, &root, field)?;
    }
    // A thread listing a deleted comment, or one twice.
    for (ty, field) in [("Table", "cols"), ("Table", "rows"), ("Row", "cells"), ("Cell", "blocks"), ("Part", "blocks"), ("CommentThread", "comments")]
    {
        for object in tx.objects_of_type(ty) {
            wrote |= clean_dangling(tx, &object, field)?;
        }
    }

    // References stored as data.
    let lists = tx.map_entries(&root, "lists")?;
    let nums: HashSet<String> = lists.iter().map(|(key, _)| key.clone()).collect();
    for (key, mut v) in lists {
        if clear_refs(&mut v, tx, &nums) {
            tx.map_set(&root, "lists", &key, v)?;
            wrote = true;
        }
    }
    for (key, mut v) in tx.map_entries(&root, "styles")? {
        if clear_refs(&mut v, tx, &nums) {
            tx.map_set(&root, "styles", &key, v)?;
            wrote = true;
        }
    }
    for inline in tx.objects_of_type(INLINE) {
        let mut v = tx.get(&inline, "value")?;
        if dangling_anchor(&v, tx) {
            tx.delete(&inline)?;
            wrote = true;
        } else if clear_refs(&mut v, tx, &nums) {
            tx.set(&inline, "value", v)?;
            wrote = true;
        }
    }
    for block in tx.objects_of_type("Block") {
        let mut para = tx.get(&block, "para")?;
        if para.is_null() {
            continue;
        }
        if clear_refs(&mut para, tx, &nums) {
            tx.set(&block, "para", para)?;
            wrote = true;
        }
    }
    let mut registers: Vec<(ObjectId, &str)> = vec![(root.clone(), "lastSection"), (root.clone(), "styleDefaults"), (root.clone(), "numbering")];
    for ty in ["Table", "Row", "Cell"] {
        registers.extend(tx.objects_of_type(ty).into_iter().map(|o| (o, "props")));
    }
    for (object, field) in registers {
        let mut v = tx.get(&object, field)?;
        if clear_refs(&mut v, tx, &nums) {
            tx.set(&object, field, v)?;
            wrote = true;
        }
    }

    // Stories and cells left without a paragraph.
    wrote |= ensure_paragraph(tx, &root, "body", true)?;
    for part in tx.objects_of_type("Part") {
        wrote |= ensure_paragraph(tx, &part, "blocks", false)?;
    }
    for cell in tx.objects_of_type("Cell") {
        wrote |= ensure_paragraph(tx, &cell, "blocks", true)?;
    }
    Ok(wrote)
}

/// Whether `value` is an object id (a string) naming an object that no longer exists. Numbers
/// (a file's own dangling ids) and `null` are not.
fn gone(tx: &Tx<'_>, value: &Value) -> bool {
    matches!(value, Value::String(id) if !tx.exists(&ObjectId::from_string(id.as_str())))
}

fn gone_ref(tx: &Tx<'_>, nums: &HashSet<String>, kind: Ref, value: &Value) -> bool {
    let Value::String(s) = value else { return false };
    match kind {
        Ref::Num => !nums.contains(s),
        Ref::Media => !s.starts_with(UNRESOLVED_MEDIA) && !tx.exists(&ObjectId::from_string(s.as_str())),
        _ => !tx.exists(&ObjectId::from_string(s.as_str())),
    }
}

/// Clears every reference in `v` to something gone. Answers whether it cleared any.
fn clear_refs(v: &mut Value, tx: &Tx<'_>, nums: &HashSet<String>) -> bool {
    let mut cleared = false;
    walk(v, &mut |kind, value| {
        // Loads expose durable tool names inside JSON. Resolve only the typed
        // reference slots this kit owns before checking for dangling references.
        let named = match kind {
            Ref::Part => Some(("Part", "part:")),
            Ref::Comment => Some(("Comment", "comment:")),
            _ => None,
        };
        let resolved = named.and_then(|(ty, prefix)| {
            let name = value.as_str()?.strip_prefix(prefix)?;
            let mut matches = tx.objects_of_type(ty).into_iter().filter(|id| tx.get(id, "id").ok().as_ref().and_then(Value::as_str) == Some(name));
            let first = matches.next()?;
            matches.next().is_none().then_some(first)
        });
        if let Some(id) = resolved {
            cleared = true;
            Fix::Set(Value::String(id.to_string()))
        } else if gone_ref(tx, nums, kind, value) {
            cleared = true;
            Fix::Clear
        } else {
            Fix::Keep
        }
    });
    cleared
}

/// Whether an inline object is a note reference or comment anchor naming a deleted part or
/// comment.
fn dangling_anchor(o: &Value, tx: &Tx<'_>) -> bool {
    matches!(o.get("type").and_then(Value::as_str), Some("noteRef" | "commentStart" | "commentEnd")) && o.get("id").is_some_and(|id| gone(tx, id))
}

/// Lists an empty paragraph at the end of `owner.field` if it holds no block (or, `trailing`, ends
/// with a table). Answers whether it did.
fn ensure_paragraph(tx: &mut Tx<'_>, owner: &ObjectId, field: &str, trailing: bool) -> KdmResult<bool> {
    let items = tx.list_items(owner, field)?;
    let ends_with_table = match items.last() {
        Some(last) => trailing && tx.get(last, "kind")?.as_str() == Some("table"),
        None => true,
    };
    if !ends_with_table {
        return Ok(false);
    }
    let seed: Vec<&str> = std::iter::once(owner.as_str()).chain(std::iter::once(field)).chain(items.iter().map(ObjectId::as_str)).collect();
    let hash = sha256_hex(seed.join(",").as_bytes());
    let id = ObjectId::from_string(format!("fill-{}", hash.get(..40).unwrap_or(&hash)));
    let para = para_meta(&Paragraph::new())?;
    let fields: Map<String, Value> = [("kind".to_owned(), json!("para")), ("para".to_owned(), para)].into_iter().collect();
    let object = match tx.create_with_id("Block", &id, fields.clone()) {
        Ok(object) => object,
        // That id was used before (and deleted): any id will do.
        Err(_) => tx.create("Block", fields)?,
    };
    tx.list_insert(owner, field, items.len(), &object)?;
    Ok(true)
}

/// Removes list entries naming an object that no longer exists, and every entry but the first of
/// an object listed more than once.
fn clean_dangling(tx: &mut Tx<'_>, owner: &ObjectId, field: &str) -> KdmResult<bool> {
    let items = tx.list_items(owner, field)?;
    let mut seen = HashSet::new();
    let repeated: Vec<bool> = items.iter().map(|object| !seen.insert(object)).collect();
    let mut wrote = false;
    for (index, object) in items.iter().enumerate().rev() {
        if !tx.exists(object) || repeated.get(index).copied().unwrap_or(false) {
            tx.list_remove(owner, field, index)?;
            wrote = true;
        }
    }
    Ok(wrote)
}
