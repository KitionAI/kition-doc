//! A drawing object's frame (its size and placement: `w`, `h`, `float`) as its own register of its
//! `Inline` object, beside the `value` register holding the rest of the object, so one person
//! moving or resizing a shape or chart and another formatting or editing it both keep their
//! change (Kit Harness Protocol `KitDrawing` DR14). Charts and diagrams split the same way. A
//! shape's other members are entries of its `look` map ([`write_look`]).

use kition_kit::kdm::Tx;
use serde_json::{Map, Value};

use crate::KdmResult;

/// The members of a serialized shape that make its frame.
const FRAME: [&str; 3] = ["w", "h", "float"];

/// A shape, or a chart or diagram (`graphic`): the objects with a frame of their own.
fn is_shape(v: &Value) -> bool {
    matches!(v.get("type").and_then(Value::as_str), Some("shape" | "graphic"))
}

/// A serialized inline object split into the register `value` holds and, for a shape or graphic,
/// its frame.
pub(crate) fn split(mut value: Value) -> (Value, Option<Value>) {
    if !is_shape(&value) {
        return (value, None);
    }
    let mut frame = Map::new();
    if let Some(map) = value.as_object_mut() {
        for key in FRAME {
            if let Some(v) = map.remove(key) {
                frame.insert(key.to_owned(), v);
            }
        }
    }
    (value, Some(Value::Object(frame)))
}

/// The serialized inline object from its two registers (a `frame` that is not an object, as an
/// object without a frame has, leaves `value` as it is).
pub(crate) fn join(mut value: Value, frame: &Value) -> Value {
    if let (Some(map), Some(frame)) = (value.as_object_mut(), frame.as_object()) {
        for key in FRAME {
            if let Some(v) = frame.get(key) {
                map.insert(key.to_owned(), v.clone());
            }
        }
    }
    value
}

/// A shape's members other than its frame as entries of its `look` map, one per member, so two
/// people changing two different ones of one shape at once both keep theirs (DR14); its `value`
/// register keeps only its type. `true` when it wrote.
pub(crate) fn write_look(tx: &mut Tx, inline: &kition_kit::kdm::ObjectId, value: &Value) -> KdmResult<bool> {
    let Some(members) = value.as_object() else { return Ok(false) };
    let kept = tx.get(inline, "value")?;
    let now: Map<String, Value> = tx.map_entries(inline, "look").unwrap_or_default().into_iter().collect();
    let mut wrote = false;
    for (k, v) in members.iter().filter(|(k, _)| k.as_str() != "type") {
        if !now.get(k.as_str()).is_some_and(|h| crate::write::same_json(h, v)) {
            tx.map_set(inline, "look", k, v.clone())?;
            wrote = true;
        }
    }
    for k in now.keys().filter(|k| !members.contains_key(k.as_str())) {
        tx.map_delete(inline, "look", k)?;
        wrote = true;
    }
    let marker = serde_json::json!({ "type": "shape" });
    if !crate::write::same_json(&kept, &marker) {
        tx.set(inline, "value", marker)?;
        wrote = true;
    }
    Ok(wrote)
}

/// Clears the `look` map of an object that is not a plain shape (one that was a shape before).
/// `true` when it wrote.
pub(crate) fn clear_look(tx: &mut Tx, inline: &kition_kit::kdm::ObjectId) -> KdmResult<bool> {
    let keys: Vec<String> = tx.map_entries(inline, "look").unwrap_or_default().into_iter().map(|(k, _)| k).collect();
    for k in &keys {
        tx.map_delete(inline, "look", k)?;
    }
    Ok(!keys.is_empty())
}

/// A shape's `value` with its `look` entries over it.
pub(crate) fn join_look(mut value: Value, look: Vec<(String, Value)>) -> Value {
    if !is_plain_shape(&value) {
        return value;
    }
    if let Some(map) = value.as_object_mut() {
        for (k, v) in look {
            map.insert(k, v);
        }
    }
    value
}

/// Whether a serialized object is a shape (its members go in `look`).
pub(crate) fn is_plain_shape(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("shape")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_shape_splits_and_joins_back() {
        let v = json!({ "type": "shape", "kind": "ellipse", "w": 10.0, "h": 5.0, "float": { "wrap": "square" }, "fill": "FF0000" });
        let (rest, frame) = split(v.clone());
        assert_eq!(rest, json!({ "type": "shape", "kind": "ellipse", "fill": "FF0000" }));
        assert_eq!(join(rest, &frame.unwrap()), v);
        // Other objects stay whole; a missing frame leaves the value as it is.
        let img = json!({ "type": "image", "w": 1.0 });
        assert_eq!(split(img.clone()), (img.clone(), None));
        assert_eq!(join(img.clone(), &Value::Null), img);
    }
}
