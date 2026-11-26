//! The text repair: what the reader drops or clears in a paragraph's text field, written to the
//! replica instead of waiting for the paragraph's next write:
//!
//! - a U+FFFC anchor whose `Inline` object is gone (a merge deleted its comment or note, or the
//!   repair of the merged state, `validate`, deleted a note reference naming a deleted
//!   part) is deleted (a split anchor, `pieces`, names no `Inline` object and stays);
//! - a reference in a run's marks to an object that is gone (an `ins`/`del` or `fmtChange`
//!   revision) is cleared.
//!
//! It runs in a transaction ([`repair`], reading the text with the transaction's own edits): inside
//! a merge's and a restore's one change and undo step, after `validate` (`DocModel`'s
//! `KitModel::validate` runs both), and as a `system` transaction (not an undo step) after an import, undo or redo and
//! when the session opens a replica.
//!
//! Both are deterministic and idempotent (deleting a character twice, or clearing a mark twice,
//! is the same as once), so replicas repairing the same state at once converge. Marks this build
//! cannot read (another build's) are left as they are.

use std::collections::BTreeSet;

use kition_doc_doc::para::OBJ;
use kition_kit::kdm::{ObjectId, TextRun, Tx};
use kition_kit::session::write_rich_text;
use serde_json::Value;

use crate::pieces::anchor_target;
use crate::refs::{Fix, walk};
use crate::text::{INLINE, OBJECT_MARK, TEXT, merged};
use crate::{KdmResult, kdm_err};

/// Repairs every paragraph's text field in `tx`. `changed`: what the transaction's merge, import,
/// undo or redo touched; when every one of them still exists nothing was deleted, so nothing can
/// dangle (`None` checks everything, as after a restore or when opening). Answers whether it wrote.
pub(crate) fn repair(tx: &mut Tx<'_>, changed: Option<&BTreeSet<ObjectId>>) -> KdmResult<bool> {
    if changed.is_some_and(|changed| changed.iter().all(|o| tx.exists(o))) {
        return Ok(false);
    }
    let mut repairs = Vec::new();
    for object in tx.objects_of_type("Block") {
        let old = tx.text_delta(&object, TEXT)?;
        if old.is_empty() {
            continue;
        }
        let new = repaired(tx, &old);
        if new != old {
            repairs.push((object, old, new));
        }
    }
    let wrote = !repairs.is_empty();
    for (object, old, new) in repairs {
        write_rich_text(tx, &object, TEXT, &old, &new).map_err(kdm_err)?;
    }
    Ok(wrote)
}

/// `runs` without dangling anchors, references to gone objects cleared from their marks.
fn repaired(tx: &Tx<'_>, runs: &[TextRun]) -> Vec<TextRun> {
    let gone = |v: &Value| matches!(v, Value::String(id) if !tx.exists(&ObjectId::from_string(id.as_str())));
    let mut out = Vec::with_capacity(runs.len());
    for run in runs {
        let mut run = run.clone();
        if run.insert.contains(OBJ) {
            // A split anchor (`pieces`) stays, whatever its target.
            let live = anchor_target(&run).is_some()
                || run.attributes.get(OBJECT_MARK).and_then(Value::as_str).is_some_and(|id| {
                    let object = ObjectId::from_string(id);
                    tx.object_type(&object).is_ok_and(|ty| ty == INLINE)
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
