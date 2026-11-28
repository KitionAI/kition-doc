//! The right-click menu's question to the engine (plan "Doc usable by hand", stage 2A; walkthrough
//! R1; WPS W-S1-W-S4): before the menu opens, the press settles the selection, and the answer says
//! what the menu is for. The menu itself is the editor's (`web/src/ContextMenu.tsx`); each of its
//! rows runs a command the ribbon runs too.
//!
//! The rules, in one place (WPS W-S2; where WPS is untested, Word for Mac, W-S6):
//! - a press on a picture, shape or chart selects it, unless it is selected already (with the
//!   others Shift added, which stay);
//! - a press on the selected text keeps the selection (its highlight, or in the selection bar a
//!   line inside it);
//! - any other press puts the caret there (`caret.set`, a pure command: no revision, no repaint
//!   of the pages);
//! - with no point (Shift+F10, the menu key) the selection stays as it is.
//!
//! A press held with the left button, an object being dragged or ink being drawn changes nothing.

use kition_doc_doc::para::{InlineObject, OBJ};
use kition_doc_doc::{Pos, StoryRef};
use serde_json::{Value, json};

use crate::DocDocument;

/// How near an object's frame (screen pixels) a press is on the object, as a left press is.
const OBJECT_REACH_PX: f64 = 4.0;
/// The selection bar's reach left of the text (screen pixels), as the text tool's.
const BAR_REACH_PX: f64 = 6.0;
/// Spelling suggestions the menu lists at most (Word for Mac; WPS untested).
const SUGGESTIONS: usize = 5;

impl DocDocument {
    /// A right-click at canvas point (`x`, `y`) (document pixels), or with none (Shift+F10) for the
    /// selection as it is: settles the selection as the module docs say, then answers what the menu
    /// is for: `target` (`text`, `picture`, `shape`, `chart`), whether text is `selected`, whether
    /// the caret is in a `table`, the `spelling` issue under the press (its word, kind, message,
    /// first suggestions and its ends, when checking as you type is on), and `at`, the canvas point
    /// the menu opens at (the press, else the caret's foot, else the selected object's middle).
    pub fn context_menu(&mut self, point: Option<(f64, f64)>, zoom: f64) -> Result<Value, String> {
        let zoom = if zoom.is_finite() && zoom > 0.0 { zoom.max(0.05) } else { 1.0 };
        let point = point.filter(|(x, y)| x.is_finite() && y.is_finite());
        let busy = self.text_tool.gesture.is_some() || self.object_drag.is_some() || self.ink_drag.is_some();
        let layout = self.layout();
        let areas = Self::areas(&layout);
        // The place pressed in the text (for its spelling), when the press is on text.
        let mut pressed: Option<Pos> = None;
        if let Some((x, y)) = point
            && !busy
        {
            let page = Self::page_at(&areas, y);
            if let Some(area) = areas.get(page).copied() {
                let (px, py) = ((x - area.x) as f32, (y - area.y) as f32);
                let story = self.session.borrow().sel.focus.story;
                if let Some(hit) = layout.object_at(page, px, py, (OBJECT_REACH_PX / zoom) as f32) {
                    let pos = Pos::new(hit.story, hit.path.clone(), hit.off);
                    let selected = self.selected_object().is_some_and(|(o, _, _)| o.story == hit.story && o.path == hit.path && o.off == hit.off)
                        || self.session.borrow().also_selected.contains(&pos);
                    if !selected {
                        self.session().also_selected.clear();
                        let end = Pos { off: pos.off + OBJ.len_utf8(), ..pos.clone() };
                        let params = json!({ "anchor": kition_doc_engine::cmd::pos_json(&pos), "focus": kition_doc_engine::cmd::pos_json(&end) });
                        self.session().run("select.range", &params).map_err(|error| error.to_string())?;
                    }
                    self.session().page_hint = page;
                } else {
                    let bar = layout.selection_bar(page, px, py, story, (BAR_REACH_PX / zoom) as f32);
                    if let Some(pos) = bar.clone().or_else(|| layout.hit(page, px, py, story)) {
                        let keep = {
                            let session = self.session.borrow();
                            let (a, b) = session.sel.ordered();
                            !session.sel.is_collapsed() && if bar.is_some() { pos >= a && pos < b } else { self.on_selection(&layout, page, px, py) }
                        };
                        self.session().page_hint = page;
                        if !keep {
                            self.session().also_selected.clear();
                            self.session()
                                .run("caret.set", &json!({ "pos": kition_doc_engine::cmd::pos_json(&pos) }))
                                .map_err(|error| error.to_string())?;
                        }
                        if bar.is_none() {
                            pressed = Some(pos);
                        }
                    }
                }
            }
        }
        let layout = self.layout();
        let areas = Self::areas(&layout);
        let object = self.selected_object();
        let mut session = self.session();
        let target = match &object {
            Some((hit, _, _)) => match session.doc.para_at(&Pos::new(hit.story, hit.path.clone(), hit.off)).and_then(|p| p.object_at(hit.off)) {
                Some(InlineObject::Image { .. }) => "picture",
                Some(InlineObject::Graphic { .. }) => "chart",
                _ => "shape",
            },
            None => "text",
        };
        let selected = object.is_none() && !session.sel.is_collapsed();
        let table = session.sel.focus.path.depth() > 0 && session.sel.focus.story == session.sel.anchor.story;
        // The spelling (or grammar) issue under the press, or at the caret from the keyboard.
        let spelling = if object.is_none() && session.view.proofing {
            let at = pressed.clone().or_else(|| point.is_none().then(|| session.sel.focus.clone()));
            match at {
                Some(at) => {
                    let answer = session
                        .run("spelling.suggestions", &json!({ "pos": kition_doc_engine::cmd::pos_json(&at) }))
                        .map_err(|error| error.to_string())?;
                    issue(answer)
                }
                None => Value::Null,
            }
        } else {
            Value::Null
        };
        let at = match point {
            Some((x, y)) => Some((x, y)),
            None => match &object {
                Some((hit, _, _)) => areas.get(hit.page).map(|area| {
                    (area.x + f64::from(hit.rect.x) + f64::from(hit.rect.w) / 2.0, area.y + f64::from(hit.rect.y) + f64::from(hit.rect.h) / 2.0)
                }),
                None => layout.caret_on(&session.sel.focus, session.page_hint).and_then(|caret| {
                    areas.get(caret.page).map(|area| (area.x + f64::from(caret.x), area.y + f64::from(caret.top) + f64::from(caret.height)))
                }),
            },
        };
        let story = match session.sel.focus.story {
            StoryRef::Body => "body",
            StoryRef::Part(_) => "headerFooter",
        };
        Ok(json!({
            "target": target,
            "selected": selected,
            "table": table,
            "story": story,
            "spelling": spelling,
            "at": at.map(|(x, y)| json!([x, y])),
        }))
    }
}

/// The menu's part of `spelling.suggestions`' answer: the word, its kind, the message, the first
/// suggestions, and the issue's ends (the menu selects them before a suggestion replaces the word).
fn issue(answer: Value) -> Value {
    let Some(found) = answer.as_object() else { return Value::Null };
    let suggestions: Vec<Value> =
        found.get("suggestions").and_then(Value::as_array).map(|list| list.iter().take(SUGGESTIONS).cloned().collect()).unwrap_or_default();
    json!({
        "word": found.get("text").cloned().unwrap_or(Value::Null),
        "kind": found.get("kind").cloned().unwrap_or(Value::Null),
        "message": found.get("message").cloned().unwrap_or(Value::Null),
        "suggestions": suggestions,
        "start": found.get("start").cloned().unwrap_or(Value::Null),
        "end": found.get("end").cloned().unwrap_or(Value::Null),
    })
}
