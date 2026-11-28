//! The text tool's pointer (plan "Doc usable by hand", stage 1; WPS W-A1-W-A8, W-B1-W-B5,
//! W-C1-W-C7, W-K1-W-K4): what a press on the pages does is decided when it goes down, from how
//! many clicks it is, where it is (the text, the selection bar, the selected text) and the keys
//! held; the drag that follows grows the selection by the same unit, or carries the selected text,
//! and the release finishes it. Every step is a command (`caret.set`, `edit.selectAll`,
//! `text.move`), as the keys are; selecting stays `pure` (no revision, no repaint of the pages).
//!
//! Pictures, shapes and ink take their presses first (`objects.rs`); tables' own gestures come
//! later (stage 4).

use kition_doc_engine::cmd::draw::DrawMode;
use kition_doc_engine::doc::Pos;
use kition_doc_engine::layout::DocLayout;
use kition_kit::tools::{PointerAction, ToolMods, ToolOverlay, ToolPointer};
use serde_json::{Value, json};

use crate::{CARET, DocDocument, PageArea};

/// Two presses at most this far apart in time (milliseconds) and place (screen pixels) are one
/// more click of the same press (Word for Mac; WPS untested).
const MULTI_CLICK_MS: f64 = 500.0;
const MULTI_CLICK_PX: f64 = 6.0;
/// The selection bar starts this far (screen pixels) left of the text (WPS W-A8: 5 to 12).
const BAR_REACH_PX: f64 = 6.0;
/// A press on the selected text becomes a drag of it past this many screen pixels (WPS W-K4).
const DRAG_PX: f64 = 3.0;

/// The pointer in the selection bar: an arrow leaning right, drawn here (CSS has none), the
/// usual arrow where an image cursor is not shown.
const BAR_CURSOR: &str = "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='20' height='24' viewBox='0 0 20 24'%3E%3Cpath d='M16 2v16l-4-4-3 7-2.2-1 3-7H5z' fill='black' stroke='white' stroke-width='1.2' stroke-linejoin='round'/%3E%3C/svg%3E\") 16 2, default";

/// What a press selects, and the drag after it grows by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Char,
    Word,
    Line,
    Paragraph,
    /// A paragraph's text without its mark (Cmd+click, WPS W-C7).
    ParagraphText,
    All,
}

impl Unit {
    /// `caret.set`'s `unit` for it.
    fn param(self) -> Option<&'static str> {
        match self {
            Unit::Word => Some("word"),
            Unit::Line => Some("line"),
            Unit::Paragraph => Some("paragraph"),
            Unit::ParagraphText => Some("paragraphText"),
            Unit::Char | Unit::All => None,
        }
    }

    /// What `clicks` presses select: in the text a caret, a word, then the paragraph (a fourth
    /// press too); in the selection bar a line, the paragraph, then the whole story; Cmd with
    /// one press the paragraph's text.
    fn of(clicks: u32, bar: bool, command: bool) -> Unit {
        match (bar, clicks) {
            (true, 1) => Unit::Line,
            (true, 2) => Unit::Paragraph,
            (true, _) => Unit::All,
            (false, 1) if command => Unit::ParagraphText,
            (false, 1) => Unit::Char,
            (false, 2) => Unit::Word,
            (false, _) => Unit::Paragraph,
        }
    }
}

/// A press on the pages not let go yet.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Gesture {
    /// Selecting: by `unit`, the unit the press selected (`start`..`end`) always kept.
    Select { unit: Unit, start: Pos, end: Pos },
    /// A press on the selected text (`press` is where): past [`DRAG_PX`] it carries the text,
    /// which lands at `to` (page, place) when let go.
    Text { from: (f64, f64), press: Pos, moved: bool, to: Option<(usize, Pos)> },
}

/// The text tool's own state: the gesture under way, the last press (for counting clicks), and
/// where the pointer is (for the cursor). Session state, never in the file.
#[derive(Debug, Default)]
pub(crate) struct TextTool {
    pub(crate) gesture: Option<Gesture>,
    /// The last press: when (ms), where (document pixels), and its click count.
    last: Option<(f64, f64, f64, u32)>,
    /// The pointer's last place (document pixels), its keys and the zoom.
    at: Option<(f64, f64, ToolMods, f64)>,
    /// The clock a test sets (ms); the wall clock otherwise.
    pub(crate) clock: Option<f64>,
}

impl TextTool {
    fn now(&self) -> f64 {
        self.clock.unwrap_or_else(kition_doc_engine::cmd::clock_ms)
    }

    /// Counts a press at (`x`, `y`): one more click of the last press when it is near enough, in
    /// time and place.
    fn count(&mut self, x: f64, y: f64, zoom: f64) -> u32 {
        let now = self.now();
        let clicks = match self.last {
            Some((then, lx, ly, n)) if now >= then && now - then <= MULTI_CLICK_MS && (x - lx).hypot(y - ly) * zoom <= MULTI_CLICK_PX => {
                n.saturating_add(1)
            }
            _ => 1,
        };
        self.last = Some((now, x, y, clicks));
        clicks
    }

    /// What the browser's double click event, which follows its second press, still has to do:
    /// nothing when that press was counted as a second click (it selected the word, and a drag
    /// since may have grown the selection); otherwise (a host that sends only the event, or two
    /// presses too far apart for this clock) select the word where it is.
    fn double_click_place(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        match self.last {
            Some((_, _, _, n)) if n >= 2 => None,
            _ => Some((x, y)),
        }
    }
}

fn pos_value(pos: &Pos) -> Value {
    kition_doc_engine::cmd::pos_json(pos)
}

impl DocDocument {
    /// The text tool's pointer event (see the module docs), after the objects have declined it.
    pub(crate) fn text_pointer(&mut self, event: &ToolPointer, layout: &DocLayout, areas: &[PageArea]) -> Result<(), String> {
        let page = Self::page_at(areas, event.y);
        let Some(area) = areas.get(page).copied() else { return Ok(()) };
        let (x, y) = ((event.x - area.x) as f32, (event.y - area.y) as f32);
        let story = self.session.borrow().sel.focus.story;
        let zoom = event.zoom.max(0.05);
        match event.action {
            PointerAction::Down => {
                self.text_tool.gesture = None;
                let clicks = self.text_tool.count(event.x, event.y, zoom);
                let bar = layout.selection_bar(page, x, y, story, (BAR_REACH_PX / zoom) as f32);
                let Some(pos) = bar.clone().or_else(|| layout.hit(page, x, y, story)) else { return Ok(()) };
                self.session().page_hint = page;
                let mods = event.mods;
                // A press on the selected text may carry it away (WPS W-K1); it decides on the drag.
                if bar.is_none() && clicks == 1 && !mods.shift && !mods.command && self.on_selection(layout, page, x, y) {
                    self.text_tool.gesture = Some(Gesture::Text { from: (event.x, event.y), press: pos, moved: false, to: None });
                    return Ok(());
                }
                let unit = Unit::of(clicks, bar.is_some(), mods.command);
                if unit == Unit::All {
                    self.run_tool("edit.selectAll", &json!({}))?;
                } else {
                    let mut params = json!({ "pos": pos_value(&pos), "extend": mods.shift });
                    if let Some(name) = unit.param() {
                        params["unit"] = json!(name);
                    }
                    self.run_tool("caret.set", &params)?;
                }
                // The unit kept while the drag grows the selection: the one the press selected, or
                // the anchor a Shift press grows from.
                let (start, end) = {
                    let session = self.session.borrow();
                    if mods.shift { (session.sel.anchor.clone(), session.sel.anchor.clone()) } else { session.sel.ordered() }
                };
                self.text_tool.gesture = Some(Gesture::Select { unit, start, end });
            }
            PointerAction::Drag => match self.text_tool.gesture.clone() {
                Some(Gesture::Select { unit: Unit::All, .. }) | None => {}
                Some(Gesture::Select { unit, start, end }) => {
                    // The page under the pointer now, whichever the press was on (a drag across pages).
                    let Some(pos) = layout.hit(page, x, y, story) else { return Ok(()) };
                    self.session().page_hint = page;
                    match unit.param() {
                        None if pos == self.session.borrow().sel.focus => {}
                        None => {
                            self.run_tool("caret.set", &json!({ "pos": pos_value(&pos), "extend": true }))?;
                        }
                        Some(name) => {
                            let params =
                                json!({ "pos": pos_value(&pos), "extend": true, "unit": name, "start": pos_value(&start), "end": pos_value(&end) });
                            self.run_tool("caret.set", &params)?;
                        }
                    }
                }
                Some(Gesture::Text { from, press, moved, .. }) => {
                    let moved = moved || (event.x - from.0).hypot(event.y - from.1) * zoom > DRAG_PX;
                    let to = if moved { layout.hit(page, x, y, story).map(|pos| (page, pos)) } else { None };
                    self.text_tool.gesture = Some(Gesture::Text { from, press, moved, to });
                }
            },
            PointerAction::Up => match self.text_tool.gesture.take() {
                Some(Gesture::Select { .. }) => {
                    // The format painter, held, gives what was just selected its formatting.
                    let painting = {
                        let session = self.session.borrow();
                        session.painter.is_some() && !session.sel.is_collapsed()
                    };
                    if painting {
                        self.run_tool("edit.pasteFormat", &json!({}))?;
                    }
                }
                Some(Gesture::Text { press, moved: false, .. }) => {
                    // Let go where it was pressed: the caret goes there (as a click off the selection).
                    self.run_tool("caret.set", &json!({ "pos": pos_value(&press) }))?;
                }
                Some(Gesture::Text { moved: true, .. }) => {
                    let Some(to) = layout.hit(page, x, y, story) else { return Ok(()) };
                    let inside = {
                        let session = self.session.borrow();
                        let (a, b) = session.sel.ordered();
                        to >= a && to <= b
                    };
                    // Dropped on itself it stays; elsewhere it moves there, or with Cmd a copy goes
                    // (WPS W-K2, W-K3: Alt moves too), read as it is let go.
                    if !inside {
                        self.session().page_hint = page;
                        self.run_tool("text.move", &json!({ "to": pos_value(&to), "copy": event.mods.command }))?;
                    }
                }
                None => {}
            },
            PointerAction::DoubleClick => {
                let Some((px, py)) = self.text_tool.double_click_place(event.x, event.y) else { return Ok(()) };
                let page = Self::page_at(areas, py);
                let Some(area) = areas.get(page).copied() else { return Ok(()) };
                if let Some(pos) = layout.hit(page, (px - area.x) as f32, (py - area.y) as f32, story) {
                    self.session().page_hint = page;
                    self.run_tool("caret.set", &json!({ "pos": pos_value(&pos), "unit": "word" }))?;
                }
            }
            PointerAction::Move => {}
        }
        Ok(())
    }

    /// Remembers where the pointer is (and its keys) for the cursor.
    pub(crate) fn track_pointer(&mut self, event: &ToolPointer) {
        self.text_tool.at = Some((event.x, event.y, event.mods, event.zoom.max(0.05)));
    }

    /// Escape while the text tool holds a press: the press is dropped, the text left where it is
    /// and the selection as it is. Answers whether there was one.
    pub(crate) fn cancel_text_gesture(&mut self) -> bool {
        self.text_tool.gesture.take().is_some()
    }

    fn run_tool(&self, command: &str, params: &Value) -> Result<Value, String> {
        self.session().run(command, params).map_err(|error| error.to_string())
    }

    /// Whether page point (`x`, `y`) is on the selected text (its highlight).
    fn on_selection(&self, layout: &DocLayout, page: usize, x: f32, y: f32) -> bool {
        let session = self.session.borrow();
        if session.sel.is_collapsed() {
            return false;
        }
        let (a, b) = session.sel.ordered();
        layout
            .selection_rects(&session.doc, &a, &b, session.page_hint)
            .into_iter()
            .any(|(p, r)| p == page && x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h)
    }

    /// The text being carried: a dashed bar where it would land (WPS W-K1).
    pub(crate) fn text_tool_overlays(&self, layout: &DocLayout, areas: &[PageArea], out: &mut Vec<ToolOverlay>) {
        let Some(Gesture::Text { moved: true, to: Some((page, to)), .. }) = &self.text_tool.gesture else { return };
        let Some(caret) = layout.caret_on(to, *page) else { return };
        let Some(area) = areas.get(caret.page) else { return };
        let x = area.x + f64::from(caret.x);
        let top = area.y + f64::from(caret.top);
        out.push(ToolOverlay::Line { ax: x, ay: top, bx: x, by: top + f64::from(caret.height), color: CARET, dashed: true });
    }

    /// The pointer's shape for what a press there would do (the one place that decides it): the
    /// I-beam over text; the selection bar's arrow; the arrow over the selected text (it can be
    /// dragged) and off the pages; `copy` while the format painter is held or a Cmd drag copies;
    /// the crosshair while a pen, the eraser or a shape to draw is held.
    pub(crate) fn text_cursor(&self, layout: &DocLayout, areas: &[PageArea]) -> String {
        {
            // A pen, the eraser or a shape to draw held (the Draw tab, Insert › Shapes): a press
            // draws, and a drag at the window's edge does not scroll (`web/src/edgeScroll.ts`).
            let session = self.session.borrow();
            let mode = session.view.draw.mode;
            if mode.pen().is_some() || mode == DrawMode::Eraser || session.view.draw.shape.is_some() || self.ink_drag.is_some() {
                return "crosshair".into();
            }
        }
        match &self.text_tool.gesture {
            Some(Gesture::Text { moved: true, .. }) => {
                let copy = self.text_tool.at.is_some_and(|(_, _, mods, _)| mods.command);
                return if copy { "copy" } else { "default" }.into();
            }
            Some(Gesture::Text { .. }) => return "default".into(),
            Some(Gesture::Select { unit: Unit::Line | Unit::All, .. }) => return BAR_CURSOR.into(),
            Some(Gesture::Select { .. }) => return "text".into(),
            None => {}
        }
        let Some((x, y, _, zoom)) = self.text_tool.at else { return "text".into() };
        if self.session.borrow().painter.is_some() {
            return "copy".into();
        }
        let page = Self::page_at(areas, y);
        let Some(area) = areas.get(page) else { return "default".into() };
        let (px, py) = ((x - area.x) as f32, (y - area.y) as f32);
        let story = self.session.borrow().sel.focus.story;
        if layout.selection_bar(page, px, py, story, (BAR_REACH_PX / zoom) as f32).is_some() {
            return BAR_CURSOR.into();
        }
        if x < area.x || x > area.x + area.w || y < area.y || y > area.y + area.h {
            return "default".into();
        }
        if self.on_selection(layout, page, px, py) {
            return "default".into();
        }
        "text".into()
    }
}
