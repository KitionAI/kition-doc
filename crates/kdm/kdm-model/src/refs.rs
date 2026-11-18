//! Ids stored as data, inside the JSON of a `json` register (a paragraph, a section, the styles,
//! table, row and cell properties): written as the object ids they name, read back as the
//! reader's local ids.
//!
//! [`walk`] finds them by where the model's serde form puts them (`tests/id_fields.rs` lists every
//! model field that holds an id, so a new one fails a test until it is handled here or explicitly):
//! - a revision index at `ins` / `del` (character properties), and at `rev` of a `fmtChange` or
//!   `numChange` object (tracked formatting and numbering changes);
//! - a list id at `num` of a `numbering` object (`NumRef`);
//! - part ids at `default` / `first` / `even` of a `headers` or `footers` object (a section's
//!   headers and footers), at `id` of a `{"type": "noteRef"}` object and at `story` of a
//!   `{"type": "shape"}` object (a text box);
//! - a comment id at `id` of a `{"type": "commentStart"}` or `{"type": "commentEnd"}` object;
//! - a media key at `media` of a `{"type": "image"}` object (a picture, or a picture in a chart).
//!
//! Comments (`part`, `parent`) and lists (`Num::id`) are translated explicitly by their writers and
//! readers.
//!
//! A written id names its object as a JSON string; a number that names nothing (a file's dangling
//! reference) stays the number it was. Media keys are strings already: one that names no media is
//! written as `"=<key>"` (object ids never start with `=`). A string whose object the reader does
//! not hold (deleted by a merge, not yet repaired) reads as "none": `null` where the field is
//! optional (a whole tracked change or list reference where the id is required), `0` for a note
//! anchor, [`NO_COMMENT`] for a comment anchor (comment ids start at 0), `""` for a media key.

use std::collections::HashMap;

use kition_doc_doc::InlineObject;
use kition_doc_doc::graphic::{Embedded, EmbeddedRef};
use kition_doc_doc::para::Paragraph;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{KdmResult, kdm_err};

const MAX_DEPTH: usize = 200;

/// The comment id a comment anchor naming a comment the reader does not hold reads as.
pub(crate) const NO_COMMENT: u32 = u32::MAX;

/// Prefix of a media key that names no media.
pub(crate) const UNRESOLVED_MEDIA: &str = "=";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ref {
    Part,
    Comment,
    Revision,
    Num,
    Media,
}

pub(crate) enum Fix {
    Keep,
    Set(Value),
    /// The reference names nothing (see the module docs for what that leaves).
    Clear,
}

fn scalar(value: &Value) -> bool {
    value.is_number() || value.is_string()
}

/// Calls `f` on every id stored as data in `value` (see the module docs) and applies its answer.
pub(crate) fn walk(value: &mut Value, f: &mut dyn FnMut(Ref, &Value) -> Fix) {
    walk_at(value, f, 0);
}

fn walk_at(value: &mut Value, f: &mut dyn FnMut(Ref, &Value) -> Fix, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    match value {
        Value::Array(items) => {
            for item in items {
                walk_at(item, f, depth + 1);
            }
        }
        Value::Object(map) => {
            let ty = map.get("type").and_then(Value::as_str).map(str::to_owned);
            for (key, child) in map.iter_mut() {
                match (key.as_str(), ty.as_deref()) {
                    ("ins" | "del", _) if scalar(child) => apply(child, f(Ref::Revision, child), Value::Null),
                    ("fmtChange" | "numChange", _) if child.is_object() => {
                        let fix = child.get("rev").filter(|r| scalar(r)).map(|rev| f(Ref::Revision, rev));
                        match fix {
                            Some(Fix::Clear) => *child = Value::Null,
                            Some(Fix::Set(rev)) => {
                                if let Some(slot) = child.get_mut("rev") {
                                    *slot = rev;
                                }
                            }
                            _ => {}
                        }
                        if let Some(old) = child.get_mut("old") {
                            walk_at(old, f, depth + 1);
                        }
                    }
                    ("numbering", _) if child.get("num").is_some_and(scalar) && child.get("level").is_some() => {
                        let fix = child.get("num").map(|num| f(Ref::Num, num));
                        match fix {
                            Some(Fix::Clear) => *child = Value::Null,
                            Some(Fix::Set(num)) => {
                                if let Some(slot) = child.get_mut("num") {
                                    *slot = num;
                                }
                            }
                            _ => {}
                        }
                    }
                    ("headers" | "footers", _) if child.is_object() => {
                        for which in ["default", "first", "even"] {
                            if let Some(slot) = child.get_mut(which)
                                && scalar(slot)
                            {
                                let fix = f(Ref::Part, slot);
                                apply(slot, fix, Value::Null);
                            }
                        }
                    }
                    ("id", Some("noteRef")) if scalar(child) => apply(child, f(Ref::Part, child), Value::from(0)),
                    ("id", Some("commentStart" | "commentEnd")) if scalar(child) => apply(child, f(Ref::Comment, child), Value::from(NO_COMMENT)),
                    ("media", Some("image")) if child.is_string() => apply(child, f(Ref::Media, child), Value::from("")),
                    ("story", Some("shape")) if scalar(child) => apply(child, f(Ref::Part, child), Value::Null),
                    _ => walk_at(child, f, depth + 1),
                }
            }
        }
        _ => {}
    }
}

fn apply(slot: &mut Value, fix: Fix, cleared: Value) {
    match fix {
        Fix::Keep => {}
        Fix::Set(value) => *slot = value,
        Fix::Clear => *slot = cleared,
    }
}

pub(crate) fn local_number(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|n| u32::try_from(n).ok())
}

/// The view's local ids to object ids, for a write.
#[derive(Default)]
pub(crate) struct Objects {
    pub parts: HashMap<u32, String>,
    pub comments: HashMap<u32, String>,
    pub revisions: Vec<String>,
    pub nums: HashMap<u32, String>,
    pub media: HashMap<String, String>,
}

impl Objects {
    pub(crate) fn translate(&self, value: &mut Value) {
        walk(value, &mut |kind, v| {
            if kind == Ref::Media {
                let Some(key) = v.as_str() else { return Fix::Keep };
                return Fix::Set(Value::String(match self.media.get(key) {
                    Some(object) => object.clone(),
                    None => format!("{UNRESOLVED_MEDIA}{key}"),
                }));
            }
            let Some(n) = local_number(v) else { return Fix::Keep };
            let object = match kind {
                Ref::Part => self.parts.get(&n),
                Ref::Comment => self.comments.get(&n),
                Ref::Revision => usize::try_from(n).ok().and_then(|i| self.revisions.get(i)),
                Ref::Num => self.nums.get(&n),
                Ref::Media => None,
            };
            object.map_or(Fix::Keep, |o| Fix::Set(Value::String(o.clone())))
        });
    }

    /// `value` with its references written as object ids.
    pub(crate) fn translated(&self, mut value: Value) -> Value {
        self.translate(&mut value);
        value
    }
}

/// Object ids to the reader's local ids.
#[derive(Default)]
pub(crate) struct Locals {
    pub parts: HashMap<String, u32>,
    pub comments: HashMap<String, u32>,
    pub revisions: HashMap<String, u32>,
    pub nums: HashMap<String, u32>,
    pub media: HashMap<String, String>,
}

impl Locals {
    pub(crate) fn translate(&self, value: &mut Value) {
        walk(value, &mut |kind, v| {
            let Value::String(s) = v else { return Fix::Keep };
            if kind == Ref::Media {
                if let Some(key) = s.strip_prefix(UNRESOLVED_MEDIA) {
                    return Fix::Set(Value::from(key));
                }
                return self.media.get(s).map_or(Fix::Clear, |key| Fix::Set(Value::from(key.as_str())));
            }
            let local = match kind {
                Ref::Part => self.parts.get(s),
                Ref::Comment => self.comments.get(s),
                Ref::Revision => self.revisions.get(s),
                Ref::Num => self.nums.get(s),
                Ref::Media => None,
            };
            local.map_or(Fix::Clear, |n| Fix::Set(Value::from(*n)))
        });
    }
}

// --- what serde skips: embedded objects (OLE objects, charts and diagrams kept from a file) ---

fn embedded_value(e: &Embedded) -> Value {
    let refs: Vec<Value> = e.refs.iter().map(|r| json!({ "id": r.id, "kind": r.kind, "target": r.target, "external": r.external })).collect();
    json!({ "xml": e.xml, "refs": refs })
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct EmbeddedForm {
    xml: String,
    refs: Vec<EmbeddedRefForm>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct EmbeddedRefForm {
    id: String,
    kind: String,
    target: String,
    external: bool,
}

fn embedded_from(value: &Value) -> Option<std::sync::Arc<Embedded>> {
    let form = EmbeddedForm::deserialize(value).ok()?;
    let refs = form.refs.into_iter().map(|r| EmbeddedRef { id: r.id, kind: r.kind, target: r.target, external: r.external }).collect();
    Some(std::sync::Arc::new(Embedded { xml: form.xml, refs }))
}

fn add_extras(object: &InlineObject, value: &mut Value, depth: usize) {
    if depth > 16 {
        return;
    }
    match object {
        InlineObject::Image { ole: Some(e), .. } => {
            if let Some(map) = value.as_object_mut() {
                map.insert("ole".into(), embedded_value(e));
            }
        }
        InlineObject::Shape { vml: Some(e), .. } => {
            if let Some(map) = value.as_object_mut() {
                map.insert("vml".into(), embedded_value(e));
            }
        }
        InlineObject::Graphic { graphic, .. } => {
            if let Some(e) = &graphic.source
                && let Some(map) = value.get_mut("graphic").and_then(Value::as_object_mut)
            {
                map.insert("source".into(), embedded_value(e));
            }
        }
        InlineObject::Group { children, .. } => {
            if let Some(values) = value.get_mut("children").and_then(Value::as_array_mut) {
                for (child, v) in children.iter().zip(values) {
                    if let Some(obj) = v.get_mut("obj") {
                        add_extras(&child.obj, obj, depth + 1);
                    }
                }
            }
        }
        _ => {}
    }
}

fn take_extras(object: &mut InlineObject, value: &Value, depth: usize) {
    if depth > 16 {
        return;
    }
    match object {
        InlineObject::Image { ole, .. } => {
            if let Some(e) = value.get("ole") {
                *ole = embedded_from(e);
            }
        }
        InlineObject::Shape { vml, .. } => {
            if let Some(e) = value.get("vml") {
                *vml = embedded_from(e);
            }
        }
        InlineObject::Graphic { graphic, .. } => {
            if let Some(e) = value.get("graphic").and_then(|g| g.get("source")) {
                std::sync::Arc::make_mut(graphic).source = embedded_from(e);
            }
        }
        InlineObject::Group { children, .. } => {
            if let Some(values) = value.get("children").and_then(Value::as_array) {
                for (child, v) in children.iter_mut().zip(values) {
                    if let Some(obj) = v.get("obj") {
                        take_extras(&mut child.obj, obj, depth + 1);
                    }
                }
            }
        }
        _ => {}
    }
}

/// A paragraph's JSON, with what serde skips (embedded objects) added. Ids are still local.
pub(crate) fn para_value(p: &Paragraph) -> KdmResult<Value> {
    let mut v = serde_json::to_value(p).map_err(kdm_err)?;
    if let Some(values) = v.get_mut("objects").and_then(Value::as_array_mut) {
        for (object, value) in p.objects.iter().zip(values) {
            add_extras(object, value, 0);
        }
    }
    Ok(v)
}

/// [`para_value`] back (ids already local).
pub(crate) fn para_from(v: &Value) -> KdmResult<Paragraph> {
    let mut p = Paragraph::deserialize(v).map_err(kdm_err)?;
    if let Some(values) = v.get("objects").and_then(Value::as_array) {
        for (object, value) in p.objects.iter_mut().zip(values) {
            take_extras(object, value, 0);
        }
    }
    Ok(p)
}

/// An inline object's JSON (an `Inline` object's `value`), with what serde skips added. Ids are
/// still local.
pub(crate) fn inline_value(o: &InlineObject) -> KdmResult<Value> {
    let mut v = serde_json::to_value(o).map_err(kdm_err)?;
    add_extras(o, &mut v, 0);
    Ok(v)
}

/// [`inline_value`] back (ids already local).
pub(crate) fn inline_from(v: &Value) -> KdmResult<InlineObject> {
    let mut o = InlineObject::deserialize(v).map_err(kdm_err)?;
    take_extras(&mut o, v, 0);
    Ok(o)
}

/// A paragraph's own register in the text form (`para`): its properties, paragraph mark and
/// section, without the text, runs and objects the text field and its `Inline` objects hold.
/// Ids are still local. Never has a `text` key, which is how a reader tells it from the register
/// form 0.3.x wrote ([`is_register_form`]).
pub(crate) fn para_meta(p: &Paragraph) -> KdmResult<Value> {
    let mut v = serde_json::to_value(p).map_err(kdm_err)?;
    if let Some(map) = v.as_object_mut() {
        map.remove("text");
        map.remove("runs");
        map.remove("objects");
    }
    Ok(v)
}

/// [`para_meta`] back: a paragraph with no text (ids already local).
pub(crate) fn para_from_meta(v: &Value) -> KdmResult<Paragraph> {
    let mut map = v.as_object().cloned().unwrap_or_default();
    map.insert("text".into(), Value::String(String::new()));
    map.insert("runs".into(), Value::Array(Vec::new()));
    map.remove("objects");
    Paragraph::deserialize(Value::Object(map)).map_err(kdm_err)
}

/// Whether a `para` register holds a whole paragraph (the register form of 0.3.x, W2) rather
/// than [`para_meta`].
pub(crate) fn is_register_form(v: &Value) -> bool {
    v.get("text").is_some()
}
