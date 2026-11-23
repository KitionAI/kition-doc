//! An edited chart's description as its own `Chart` object (`KitDrawing` DR14):
//! an `Inline` holding a chart edited in Kition Doc names it in its `chart` ref, and each member
//! of the description (type, title, legend, categories, series…) is an entry of its `parts` map,
//! so two people changing different members of one chart at once both keep their change. The
//! chart's part (as read or made) stays in the inline's `value` and the package; its drawing is not
//! stored: every read draws the chart again from its part and description ([`redraw`]).

use kition_doc_doc::{Document, InlineObject};
use kition_kit::kdm::{ObjectId, Replica, Tx};
use serde_json::{Map, Value};

use crate::KdmResult;

/// The object type.
const CHART: &str = "Chart";

/// A serialized inline object split into its `value` and, for an edited chart, its description's
/// members (the drawing, made from them, is left out).
pub(crate) fn split(mut value: Value) -> (Value, Option<Map<String, Value>>) {
    let Some(graphic) = value.get_mut("graphic").and_then(Value::as_object_mut) else { return (value, None) };
    let Some(Value::Object(parts)) = graphic.remove("spec") else { return (value, None) };
    graphic.remove("items");
    (value, Some(parts))
}

/// The id an inline's chart object is made under: the same in every replica.
fn chart_id(inline: &ObjectId) -> ObjectId {
    let base: String = inline.as_str().chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-')).take(120).collect();
    ObjectId::from_string(format!("{base}-chart"))
}

/// Makes inline `inline`'s chart object hold `parts` (none: no object), writing only the members
/// that differ. Answers whether it wrote.
pub(crate) fn write(tx: &mut Tx<'_>, inline: &ObjectId, parts: Option<&Map<String, Value>>) -> KdmResult<bool> {
    let stored = match tx.get(inline, "chart")? {
        Value::String(s) => Some(ObjectId::from_string(s)).filter(|o| tx.exists(o)),
        _ => None,
    };
    let Some(parts) = parts else {
        let Some(object) = stored else { return Ok(false) };
        tx.delete(&object)?;
        tx.set(inline, "chart", Value::Null)?;
        return Ok(true);
    };
    let Some(object) = stored else {
        let object = match tx.create_with_id(CHART, &chart_id(inline), Map::new()) {
            Ok(id) => id,
            Err(_) => tx.create(CHART, Map::new())?,
        };
        for (k, v) in parts {
            tx.map_set(&object, "parts", k, v.clone())?;
        }
        tx.set(inline, "chart", Value::String(object.as_str().to_owned()))?;
        return Ok(true);
    };
    let have: Map<String, Value> = tx.map_entries(&object, "parts")?.into_iter().collect();
    let mut wrote = false;
    for (k, v) in parts {
        if have.get(k) != Some(v) {
            tx.map_set(&object, "parts", k, v.clone())?;
            wrote = true;
        }
    }
    for k in have.keys().filter(|k| !parts.contains_key(*k)) {
        tx.map_delete(&object, "parts", k)?;
        wrote = true;
    }
    Ok(wrote)
}

/// Deletes inline `inline`'s chart object, if it has one.
pub(crate) fn delete(tx: &mut Tx<'_>, inline: &ObjectId) -> KdmResult<()> {
    if let Value::String(object) = tx.get(inline, "chart")? {
        let object = ObjectId::from_string(object);
        if tx.exists(&object) {
            tx.delete(&object)?;
        }
    }
    Ok(())
}

/// Inline `inline`'s serialized value with its chart's description put back.
pub(crate) fn join(replica: &Replica, inline: &ObjectId, mut value: Value) -> Value {
    let Ok(Value::String(object)) = replica.get(inline, "chart") else { return value };
    let object = ObjectId::from_string(object);
    let Ok(entries) = replica.map_entries(&object, "parts") else { return value };
    if let Some(graphic) = value.get_mut("graphic").and_then(Value::as_object_mut) {
        graphic.insert("spec".into(), Value::Object(entries.into_iter().collect()));
    }
    value
}

/// Draws every edited chart of `doc` again from its part and description (the replica stores no
/// drawing for them).
pub(crate) fn redraw(doc: &mut Document) {
    let theme = doc.settings.theme_colors.clone();
    let mut todo = Vec::new();
    for (pos, o) in doc.shapes() {
        if let InlineObject::Graphic { w, h, graphic, .. } = o
            && graphic.spec.is_some()
        {
            let items =
                kition_doc_docx::chart_part(doc, graphic).map(|part| kition_doc_docx::chart_part_items(&part, &theme, *w, *h)).unwrap_or_default();
            todo.push((pos, items, *w, *h));
        }
    }
    for (pos, items, w, h) in todo {
        if let Ok(para) = doc.para_mut(pos.story, &pos.path)
            && let Some(InlineObject::Graphic { graphic, .. }) = para.object_at_mut(pos.off)
        {
            let g = std::sync::Arc::make_mut(graphic);
            g.items = items;
            (g.w, g.h) = (w, h);
        }
    }
}
