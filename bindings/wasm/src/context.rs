//! Call context (C8-land-14-doc item 3): the editor state a command may read besides its
//! parameters, which a host saves before an agent's call, sets for each step (`on`) and puts back
//! after it, so an agent's call never depends on, nor leaves behind, where the person's caret,
//! selection or page is.
//!
//! The editor state is the selection (anchor and focus), the drawing objects added to it with
//! Shift, the caret's pending character formatting, where the caret returns to from a header,
//! the caret's goal column and the page the caret was last seen on. `on` names paragraphs by
//! their tool ids (`p` and the `w14:paraId`, as `doc_query` lists them) or a drawing object by
//! its number: the selection becomes the whole of each named paragraph (from the first one's
//! start to the last one's end, in one story), or that object. Doc has no parts and no time.
use kition_doc_engine::doc::{Document, Pos, StoryRef};
use kition_doc_engine::{Selection, Session};
use kition_kit::CallContext;
use serde_json::{Value, json};

use crate::DocDocument;

/// The keys and JSON Pointers of `inspect()` that are the editing session, not the file: the
/// kit's editor-state list (kition-headless's volatile keys and `testing::check_context`).
pub const EDITOR_STATE: &[&str] = &[
    "/page",
    "/caret",
    "/preedit",
    "/inputAt",
    "/selected",
    "/story",
    "/equation",
    "/shape",
    "/shapes",
    "/picture",
    "/chart",
    "/escape",
    "/all",
    "/painter",
    "/cells",
    "/selectsTable",
];

/// Where paragraph `id` (`p` and eight hex digits) is: its story and path.
pub(crate) fn paragraph(doc: &Document, id: &str) -> Option<Pos> {
    let hex = id.strip_prefix('p')?;
    if hex.len() != 8 {
        return None;
    }
    let want = u32::from_str_radix(hex, 16).ok()?;
    let stories = std::iter::once(StoryRef::Body).chain(doc.parts.keys().map(|k| StoryRef::Part(*k)));
    for story in stories {
        for path in doc.para_paths(story) {
            let pos = Pos::new(story, path, 0);
            if doc.para_at(&pos).is_some_and(|p| p.props.para_id == Some(want)) {
                return Some(pos);
            }
        }
    }
    None
}

/// Where the drawing object, picture or equation numbered `uid` is (the position of its anchor).
fn drawing(doc: &Document, uid: u64) -> Option<Pos> {
    let uid = u32::try_from(uid).ok().filter(|u| *u > 0)?;
    doc.numbered().into_iter().find(|(_, o)| o.tool_uid() == Some(uid)).map(|(pos, _)| pos)
}

impl DocDocument {
    pub(crate) fn state_of(session: &Session) -> Value {
        let identity = |pos: &Pos| {
            let active = session.also_selected.contains(pos)
                || session.math.as_ref().is_some_and(|math| math.at == *pos)
                || (session.sel.anchor == *pos
                    && session.sel.anchor.story == session.sel.focus.story
                    && session.sel.anchor.path == session.sel.focus.path
                    && session.sel.focus.off == session.sel.anchor.off.saturating_add(kition_doc_engine::doc::para::OBJ.len_utf8()));
            json!({
                "paragraph": session.doc.para_at(pos).and_then(|p| p.props.para_id).map(|id| format!("p{id:08X}")),
                "object": active.then(|| session.doc.para_at(pos).and_then(|p| p.object_at(pos.off)).and_then(|object| object.tool_uid())).flatten(),
            })
        };
        let selected = |pos: &Pos| json!({ "target": identity(pos), "off": pos.off });
        json!({
            "selection": { "anchor": selected(&session.sel.anchor), "focus": selected(&session.sel.focus) },
            "positions": session.sel,
            "also": session.also_selected,
            "pending": session.pending,
            "bodyReturn": session.body_return,
            "goalX": session.goal_x,
            "page": session.page_hint,
            // The editing mode (C8-doc-4): typing in an equation, a column selection.
            "mode": { "math": session.math, "column": session.column, "columnMode": session.column_mode },
            "identities": {
                "anchor": identity(&session.sel.anchor), "focus": identity(&session.sel.focus),
                "also": session.also_selected.iter().map(identity).collect::<Vec<_>>(),
                "bodyReturn": session.body_return.as_ref().map(identity),
                "math": session.math.as_ref().map(|math| identity(&math.at)),
                "column": session.column.as_ref().map(|column| column.segments.iter().map(|(a, b)| [identity(a), identity(b)]).collect::<Vec<_>>()),
            },
        })
    }

    pub(crate) fn restore_selection_state(&mut self, saved: &Value) -> Result<(), String> {
        let strict = std::mem::take(&mut self.validating_reread);
        let selection_active = saved.get("activeSelection").and_then(Value::as_bool).unwrap_or(false);
        let mut selection: Selection =
            serde_json::from_value(saved.get("positions").cloned().unwrap_or(Value::Null)).map_err(|e| format!("selection: {e}"))?;
        let mut also: Vec<Pos> = serde_json::from_value(saved.get("also").cloned().unwrap_or_else(|| json!([]))).map_err(|e| format!("also: {e}"))?;
        let pending = serde_json::from_value(saved.get("pending").cloned().unwrap_or(Value::Null)).map_err(|e| format!("pending: {e}"))?;
        let mut body_return: Option<Pos> =
            serde_json::from_value(saved.get("bodyReturn").cloned().unwrap_or(Value::Null)).map_err(|e| format!("bodyReturn: {e}"))?;
        let goal_x = saved.get("goalX").and_then(Value::as_f64).map(|x| x as f32);
        let page = saved.get("page").and_then(Value::as_u64).and_then(|p| usize::try_from(p).ok()).unwrap_or(0);
        let mode = saved.get("mode").cloned().unwrap_or(Value::Null);
        let mut math: Option<kition_doc_engine::MathEdit> =
            serde_json::from_value(mode.get("math").cloned().unwrap_or(Value::Null)).map_err(|e| format!("mode.math: {e}"))?;
        let mut column: Option<kition_doc_engine::ColumnBlock> =
            serde_json::from_value(mode.get("column").cloned().unwrap_or(Value::Null)).map_err(|e| format!("mode.column: {e}"))?;
        let column_mode = mode.get("columnMode").and_then(Value::as_bool).unwrap_or(false);
        let doc = self.session.borrow().doc.clone();
        let identities = saved.get("identities").unwrap_or(&Value::Null);
        let named = |name: &str| identities.get(name).unwrap_or(&Value::Null);
        let restore = |pos: &mut Pos, identity: &Value| -> Result<(), String> {
            if let Some(id) = identity.get("paragraph").and_then(Value::as_str) {
                if let Some(at) = paragraph(&doc, id) {
                    *pos = doc.clamp(&Pos { off: pos.off, ..at });
                } else if strict && selection_active {
                    return Err(format!("deleted active paragraph {id}"));
                }
            }
            if let Some(uid) = identity.get("object").and_then(Value::as_u64) {
                if let Some(at) = drawing(&doc, uid) {
                    *pos = at;
                } else if strict && selection_active {
                    return Err(format!("deleted active drawing object {uid}"));
                }
            }
            Ok(())
        };
        restore(&mut selection.anchor, named("anchor"))?;
        restore(&mut selection.focus, named("focus"))?;
        for (i, pos) in also.iter_mut().enumerate() {
            restore(pos, named("also").get(i).unwrap_or(&Value::Null))?;
        }
        if let Some(pos) = &mut body_return {
            restore(pos, named("bodyReturn"))?;
        }
        if let Some(math) = &mut math {
            restore(&mut math.at, named("math"))?;
        }
        if let Some(column) = &mut column {
            column.sel = selection.clone();
            for (i, (a, b)) in column.segments.iter_mut().enumerate() {
                let segment = named("column").get(i).unwrap_or(&Value::Null);
                restore(a, segment.get(0).unwrap_or(&Value::Null))?;
                restore(b, segment.get(1).unwrap_or(&Value::Null))?;
            }
        }
        self.context_on.clear();
        self.context_named = false;
        self.selection_active = selection_active;
        if !self.planning {
            self.copied.set(false);
        }
        let mut session = self.session();
        // What the run deleted is dropped, as an undo would: a position that no longer is falls
        // back to the document's start.
        let doc = session.doc.clone();
        let valid = |p: &Pos| doc.para_at(p).is_some_and(|q| p.off <= q.len());
        let start = Pos::new(StoryRef::Body, kition_doc_engine::doc::Path::top(0), 0);
        session.sel = if valid(&selection.anchor) && valid(&selection.focus) { selection } else { Selection::caret(start) };
        session.also_selected = also.into_iter().filter(|p| valid(p)).collect();
        session.pending = pending;
        session.body_return = body_return.filter(|p| valid(p));
        session.goal_x = goal_x;
        session.page_hint = page;
        // An equation the run deleted is no longer edited; a column selection lasts while the
        // selection is the one it was made with.
        session.math = math.filter(|m| {
            doc.para_at(&m.at).is_some_and(|p| matches!(p.object_at(m.at.off), Some(kition_doc_engine::doc::InlineObject::Equation { .. })))
        });
        session.column = column.filter(|c| c.sel == session.sel && c.segments.iter().all(|(a, b)| valid(a) && valid(b)));
        session.column_mode = column_mode;
        Ok(())
    }

    pub(crate) fn apply_context(&mut self, context: &CallContext) -> Result<(), String> {
        if context.part.is_some() {
            return Err(
                "invalid_params: a Doc document has no parts: name what a step acts on with `on` (paragraph ids, drawing object numbers)".into()
            );
        }
        if context.time.is_some() {
            return Err("invalid_params: a Doc document has no time".into());
        }
        self.context_on.clear();
        self.context_named = false;
        if context.on.is_empty() {
            return Ok(());
        }
        // Everything is checked before anything changes.
        let doc = self.session.borrow().doc.clone();
        let mut paragraphs: Vec<Pos> = Vec::new();
        let mut objects: Vec<Pos> = Vec::new();
        for id in &context.on {
            if let Some(text) = id.as_str()
                && let Some(pos) = paragraph(&doc, text)
            {
                paragraphs.push(pos);
                continue;
            }
            let number = id.as_u64().or_else(|| id.as_str().and_then(|text| text.parse().ok()));
            match number.and_then(|uid| drawing(&doc, uid)) {
                Some(pos) => objects.push(pos),
                None => {
                    return Err(format!(
                        "invalid_params: `{id}` names no paragraph (its id as doc_query lists it, `p` and eight hex digits) and no drawing object of the document"
                    ));
                }
            }
        }
        if !paragraphs.is_empty() && !objects.is_empty() {
            return Err("invalid_params: `on` names paragraphs and drawing objects together; give one kind per step".into());
        }
        self.context_on = context.on.iter().filter_map(Value::as_str).filter(|id| paragraph(&doc, id).is_some()).map(str::to_owned).collect();
        self.context_named = true;
        let mut session = self.session();
        // A step naming what it acts on is in no editing mode: not typing in an equation, no
        // column selection (C8-doc-4).
        session.math = None;
        session.column = None;
        session.column_mode = false;
        if !paragraphs.is_empty() {
            paragraphs.sort();
            let (Some(first), Some(last)) = (paragraphs.first().cloned(), paragraphs.last().cloned()) else { return Ok(()) };
            if first.story != last.story {
                return Err("invalid_params: `on` names paragraphs of different stories; give one story per step".into());
            }
            let end = doc.para_at(&last).map_or(0, kition_doc_engine::doc::Paragraph::len);
            session.sel = Selection { anchor: first, focus: Pos { off: end, ..last } };
            session.also_selected.clear();
            return Ok(());
        }
        objects.sort();
        let first = objects.remove(0);
        let len = kition_doc_engine::doc::para::OBJ.len_utf8();
        session.sel = Selection { anchor: first.clone(), focus: Pos { off: first.off + len, ..first } };
        session.also_selected = objects;
        Ok(())
    }
}
