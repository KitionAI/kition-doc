//! Settling a batch of object-model operations (C8-land-14-doc item 5): a paragraph a batch made
//! (or copied, with another's `paraId` in its `para` register) gets its own `w14:paraId` and tool
//! id in the same transaction, as the saved file will hold them, so the batch's receipt names it
//! by the id it keeps, and a reread of the saved file finds nothing new.
use std::collections::HashSet;

use kition_kit::BatchSettler;
use kition_kit::kdm::{ObjectId, Tx};
use serde_json::{Value, json};

/// The `w14:paraId` a `para` register holds.
fn para_id(para: &Value) -> Option<u32> {
    para.get("props")?.get("paraId")?.as_u64().and_then(|v| u32::try_from(v).ok()).filter(|v| *v > 0 && *v < 0x8000_0000)
}

/// A fresh id derived from the object id, probed past every id in use.
fn fresh(object: &ObjectId, used: &mut HashSet<u32>) -> u32 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in object.as_str().as_bytes() {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut v = 0x1000_0000u32 | ((h ^ (h >> 32)) as u32 & 0x0FFF_FFFF);
    while !used.insert(v) {
        v = if v >= 0x7FFF_FFFE { 0x1000_0000 } else { v + 1 };
    }
    v
}

fn settle(tx: &mut Tx<'_>) -> Result<(), String> {
    let blocks = tx.objects_of_type("Block");
    // Ids kept by paragraphs already numbered (their tool id agrees with their register) first.
    let mut used: HashSet<u32> = HashSet::new();
    let mut pending: Vec<ObjectId> = Vec::new();
    for object in &blocks {
        let para = tx.get(object, "para").map_err(|e| e.to_string())?;
        let kind = tx.get(object, "kind").map_err(|e| e.to_string())?;
        if kind.as_str() != Some("para") {
            continue;
        }
        let id = tx.get(object, "id").map_err(|e| e.to_string())?;
        match para_id(&para) {
            Some(v) if id.as_str() == Some(format!("p{v:08X}").as_str()) && used.insert(v) => {}
            _ => pending.push(object.clone()),
        }
    }
    for row in tx.objects_of_type("Row") {
        if let Some(v) = tx.get(&row, "props").ok().as_ref().and_then(|props| props.get("paraId")).and_then(Value::as_u64) {
            used.insert(u32::try_from(v).unwrap_or(0));
        }
    }
    for object in pending {
        let mut para = tx.get(&object, "para").map_err(|e| e.to_string())?;
        let v = match para_id(&para) {
            Some(v) if used.insert(v) => v,
            _ => fresh(&object, &mut used),
        };
        if !para.is_object() {
            para = json!({});
        }
        if let Some(map) = para.as_object_mut() {
            let props = map.entry("props").or_insert_with(|| json!({}));
            if let Some(props) = props.as_object_mut() {
                props.insert("paraId".into(), json!(v));
            }
        }
        tx.set(&object, "para", para).map_err(|e| e.to_string())?;
        tx.set(&object, "id", json!(format!("p{v:08X}"))).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The kit's [`kition_kit::KitDocument::batch_settler`].
pub(crate) fn settler() -> BatchSettler {
    Box::new(settle)
}
