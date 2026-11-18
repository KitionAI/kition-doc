//! The read repair: what the reader drops or clears in a paragraph's text field, which a
//! transaction cannot read (so [`kition_kit::session::KitModel::validate`] cannot), written by the
//! session as a `system` transaction (not an undo step) after an import, undo or redo and when it
//! opens a replica, instead of waiting for the paragraph's next write:
//!
//! - a U+FFFC anchor whose `Inline` object is gone (a merge deleted its comment or note) is
//!   deleted (a split anchor, `pieces`, names no `Inline` object and stays);
//! - a reference in a run's marks to an object that is gone (an `ins`/`del` or `fmtChange`
//!   revision) is cleared.
//!
//! Both are deterministic and idempotent (deleting a character twice, or clearing a mark twice,
//! is the same as once), so replicas repairing the same state at once converge. Marks this build
//! cannot read (another build's) are left as they are.

use std::collections::BTreeSet;

use kition_doc_doc::para::OBJ;
use kition_kit::kdm::{ObjectId, Replica, TextRun, Tx};
use kition_kit::session::write_rich_text;
use serde_json::Value;

use crate::ids::{BlockIds, Ids};
use crate::pieces::anchor_target;
use crate::refs::{Fix, walk};
use crate::text::{INLINE, OBJECT_MARK, TEXT, merged};
use crate::write::MAX_DEPTH;
use crate::{KdmResult, kdm_err};

/// One paragraph's field: what it holds, and what it should.
pub(crate) struct Repair {
    object: ObjectId,
    old: Vec<TextRun>,
    new: Vec<TextRun>,
}

/// The repairs the view's paragraphs (`ids`, as just read from `replica`) need. `changed`: what the
/// last import, undo or redo touched (only a deletion can leave something dangling); `None` checks
/// everything.
pub(crate) fn plan(replica: &Replica, ids: &Ids, changed: Option<&BTreeSet<ObjectId>>) -> Vec<Repair> {
    if changed.is_some_and(|changed| changed.iter().all(|o| replica.exists(o))) {
        return Vec::new();
    }
    let mut out = Vec::new();
    collect(replica, &ids.body, &mut out, 0);
    for part in ids.parts.values() {
        collect(replica, &part.blocks, &mut out, 0);
    }
    out
}

fn collect(replica: &Replica, blocks: &[BlockIds], out: &mut Vec<Repair>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for block in blocks {
        if let Some(table) = &block.table {
            for row in &table.rows {
                for cell in &row.cells {
                    collect(replica, &cell.blocks, out, depth + 1);
                }
            }
        }
        collect(replica, &block.merged, out, depth + 1);
        let Some(text) = &block.text else { continue };
        let old = text.runs();
        let new = repaired(replica, old);
        if new.as_slice() != old {
            out.push(Repair { object: block.object.clone(), old: old.to_vec(), new });
        }
    }
}

/// `runs` without dangling anchors, references to gone objects cleared from their marks.
fn repaired(replica: &Replica, runs: &[TextRun]) -> Vec<TextRun> {
    let gone = |v: &Value| matches!(v, Value::String(id) if !replica.exists(&ObjectId::from_string(id.as_str())));
    let mut out = Vec::with_capacity(runs.len());
    for run in runs {
        let mut run = run.clone();
        if run.insert.contains(OBJ) {
            // A split anchor (`pieces`) stays, whatever its target.
            let live = anchor_target(&run).is_some()
                || run.attributes.get(OBJECT_MARK).and_then(Value::as_str).is_some_and(|id| {
                    let object = ObjectId::from_string(id);
                    replica.object_type(&object).is_ok_and(|ty| ty == INLINE)
                });
            if !live {
                run.insert.retain(|c| c != OBJ);
            }
        }
        let mut marks =
            Value::Object(run.attributes.iter().filter(|(key, _)| key.as_str() != OBJECT_MARK).map(|(k, v)| (k.clone(), v.clone())).collect());
        let mut cleared = false;
        walk(&mut marks, &mut |_, value| {
            if gone(value) {
                cleared = true;
                Fix::Clear
            } else {
                Fix::Keep
            }
        });
        if cleared && let Value::Object(map) = marks {
            for (key, value) in map {
                if value.is_null() {
                    run.attributes.remove(&key);
                } else {
                    run.attributes.insert(key, value);
                }
            }
        }
        out.push(run);
    }
    merged(out)
}

/// Writes `repairs`. Answers whether it wrote anything.
pub(crate) fn apply(tx: &mut Tx<'_>, repairs: &[Repair]) -> KdmResult<bool> {
    let mut wrote = false;
    for repair in repairs {
        if tx.exists(&repair.object) {
            write_rich_text(tx, &repair.object, TEXT, &repair.old, &repair.new).map_err(kdm_err)?;
            wrote = true;
        }
    }
    Ok(wrote)
}
