//! # kition-doc-kit
//!
//! The Doc engine for Kition's in-page editor, on the Kition SDK. [`DocDocument`] implements
//! [`KitDocument`] for a Kition Doc document (Word `.docx`, Kition Doc's own lossless format) and
//! [`KitTools`] for typing in it: a click places the caret, a drag selects, typed text and editing
//! keys run Kition Doc's own commands (`text.insert`, `caret.*`, the registry's shortcuts), and
//! the caret and selection come back as engine overlays.
//!
//! Document pixels are points, with the laid-out pages stacked top to bottom on one canvas,
//! centred, on a light neutral pasteboard.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod acts;
mod check;
mod collab;
mod composing;
pub mod context;
mod context_menu;
mod describe;
mod family;
pub mod files;
mod margin_guides;
mod objects;
mod params;
mod place;
mod settle;
mod text_tool;

use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::sync::Arc;

use kition_doc_engine::layout::DocLayout;
use kition_doc_engine::{Selection, Session};
use kition_kit::raster::{self, Frame};
use kition_kit::tools::{KitTools, ToolInfo, ToolMods, ToolOverlay, ToolPointer, ToolRequest, ToolResponse};
use kition_kit::{KitCommand, KitDocument, MAX_RENDER_SIDE};
use serde_json::{Value, json};

/// Canvas around the pages, in points.
pub const MARGIN: f64 = 40.0;
/// Space between two pages, in points.
pub const GAP: f64 = 24.0;
/// Body blocks `inspect()` lists (the rest through `doc_query`).
const INSPECTED_BLOCKS: usize = 2000;
/// 1:1 page images kept for zoomed-out views.
const CACHED_PAGES: usize = 24;

/// Registry locations that are not menus: caret movement and editing (keys), the Quick Access
/// Toolbar, and panes this editor does not have. Their commands still run by shortcut and for
/// agents.
const NOT_MENUS: &[&str] = &["Editing", "Navigation", "Quick Access Toolbar", "Agents", "Zotero"];
const PASTEBOARD: [u8; 4] = [238, 238, 242, 255];
/// Kition Doc's brand colour (`ui.brand.light` in `kit.json`, #2b5cd9); pages are always drawn light.
pub(crate) const BRAND: [u8; 3] = [43, 92, 217];
const SELECTION: [u8; 4] = [BRAND[0], BRAND[1], BRAND[2], 64];
const CARET: [u8; 4] = [20, 20, 20, 255];

/// Where a page lies on the canvas, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PageArea {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// One open Kition Doc document.
pub struct DocDocument {
    /// In a cell: laying out needs `&mut` (Kition Doc caches the layout per revision), and
    /// rendering and inspecting take `&self`.
    session: RefCell<Session>,
    /// The text tool's press under way, last press and pointer place (`text_tool`).
    text_tool: text_tool::TextTool,
    cache: RefCell<HashMap<usize, (u64, kition_doc_render::Rendered)>>,
    /// The canvas around the pages (an editor's canvas color).
    pasteboard: [u8; 4],
    /// A picture's or shape's move, resize or turn under way (`objects`).
    object_drag: Option<objects::ObjectDrag>,
    /// A pen stroke or an eraser drag under way (Draw tab, `objects`).
    ink_drag: Option<objects::InkDrag>,
    /// Whose keyboard the keys come from (the editor's `setup` says a Mac's): which command a
    /// shortcut runs, and the one the catalogue shows.
    platform: kition_doc_engine::Platform,
    /// Pages drawn as the editor shows them while editing (its `setup` turns it on): headers
    /// and footers dimmed, spelling and grammar squiggles. Otherwise they are drawn as printed,
    /// for thumbnails and agents' screenshots.
    editing: bool,
    /// The paragraphs (their tool ids) a step's `on` selected, while the call context is set
    /// (C8-doc-2): what `under` rests a step with `on` on. Cleared when the state is put back.
    context_on: Vec<String>,
    context_named: bool,
    /// What the guard's questions about one run's steps found so far (see [`place::Pass`]).
    pass: place::Pass,
    /// Whether a copy (`edit.copy`, `edit.cut`, `edit.copyFormat`) ran in this call: an agent's
    /// paste takes only what its own call copied (C8-doc-5, [`acts::reads_clipboard`]). Cleared
    /// when the editor state is put back after a call.
    pub(crate) copied: std::cell::Cell<bool>,
    validating_reread: bool,
    planning: bool,
    selection_active: bool,
    /// The chart the last `drawing.insert` made, with the selection it left: the caret after the
    /// chart (so typing follows it), where the chart is still the object at the caret and the
    /// editor shows its Chart Design until the selection moves.
    pub(crate) inserted_chart: Option<(u32, kition_doc_engine::Selection)>,
    /// What an input method composes at the caret (`composing`): session state, never the
    /// document. The pages are drawn with it typed in.
    preedit: Option<composing::Preedit>,
    /// How many times the pages changed without the document changing (a composition shown,
    /// changed or taken away): part of the revision the canvas repaints on.
    drawn: u64,
}

impl DocDocument {
    /// Remembers the chart `command` inserted (`drawing.insert` of `kind: "chart"`), if it did.
    pub(crate) fn note_inserted_chart(&mut self, command: &str, params: &Value, answer: &Value) {
        let chart = command == "drawing.insert" && params.get("kind").and_then(Value::as_str) == Some("chart");
        if let Some(uid) = answer.get("id").and_then(Value::as_str).and_then(|id| id.parse::<u32>().ok()).filter(|_| chart) {
            self.inserted_chart = Some((uid, self.session.borrow().sel.clone()));
        }
    }

    /// The chart just inserted while the caret is still where its insertion left it.
    fn chart_just_inserted(&self) -> Option<u32> {
        let (uid, sel) = self.inserted_chart.as_ref()?;
        let session = self.session.borrow();
        (session.sel == *sel && session.doc.find_shape(*uid).is_some()).then_some(*uid)
    }

    /// `params` with every position that names its paragraph by id (`{para, off}`,
    /// C8-land-14-doc) written as the engine reads positions.
    /// An agent (the headless kit: [`files::serve_hosted`]) cannot see the session's clipboard: a
    /// paste of it is refused unless its own call copied (C8-doc-5, [`acts::reads_clipboard`]).
    /// People in the editor are not refused.
    pub(crate) fn clipboard_gate(&self, command: &str, params: &Value) -> Result<(), String> {
        if acts::reads_clipboard(command, params) && files::served() && !self.copied.get() {
            return Err(format!(
                "invalid_params: `{command}` pastes the session's clipboard, which an agent cannot see: give the content (`text`, or `html` for edit.pasteSpecial), use edit.pasteText, or copy in the same run first"
            ));
        }
        Ok(())
    }

    /// Notes a copy this call made (see [`Self::clipboard_gate`]).
    pub(crate) fn copied_by(&self, command: &str) {
        if matches!(command, "edit.copy" | "edit.cut" | "edit.copyFormat") {
            self.copied.set(true);
        }
    }

    pub(crate) fn resolved(&self, command: &str, mut params: Value) -> Value {
        // A command runs: the guard's questions about the run before it are over.
        if self.planning {
            self.pass.executing(command, !self.context_on.is_empty());
        } else {
            self.pass.reset();
        }
        let session = self.session.borrow();
        place::resolve(&session.doc, command, &mut params);
        params
    }

    /// Refuses a parameter `command` does not take (a typo or a guessed name) before it runs,
    /// naming the one it resembles, instead of running without it (engine rule E7a). The schemas
    /// are the ones the catalogue offers agents, read once.
    pub(crate) fn refuse_unknown(&self, command: &str, params: &Value) -> Result<(), String> {
        static SCHEMAS: std::sync::OnceLock<std::collections::HashMap<String, Value>> = std::sync::OnceLock::new();
        let schemas = SCHEMAS.get_or_init(|| self.commands().into_iter().filter_map(|command| Some((command.id, command.schema?))).collect());
        schemas.get(command).map_or(Ok(()), |schema| kition_kit::schema::refuse_unknown(command, schema, params))
    }

    fn from_session(session: Session) -> Self {
        Self {
            session: RefCell::new(session),
            text_tool: text_tool::TextTool::default(),
            cache: RefCell::new(HashMap::new()),
            pasteboard: PASTEBOARD,
            object_drag: None,
            ink_drag: None,
            platform: kition_doc_engine::Platform::Default,
            editing: false,
            context_on: Vec::new(),
            context_named: false,
            copied: std::cell::Cell::new(false),
            pass: place::Pass::default(),
            validating_reread: false,
            planning: false,
            selection_active: false,
            inserted_chart: None,
            preedit: None,
            drawn: 0,
        }
    }

    /// How pages are drawn: as printed, or with the editor's editing look (see `editing`).
    fn render_options(&self) -> kition_doc_render::RenderOptions {
        let display = kition_doc_engine::layout::display::DisplayOptions { dim_header: self.editing, proofing: self.editing, ..Default::default() };
        kition_doc_render::RenderOptions { display, ..Default::default() }
    }

    fn session(&self) -> RefMut<'_, Session> {
        self.session.borrow_mut()
    }

    fn layout(&self) -> Arc<DocLayout> {
        self.session().layout()
    }

    /// Every page's place on the canvas: stacked, each centred on the widest.
    fn areas(layout: &DocLayout) -> Vec<PageArea> {
        let widest = layout.pages.iter().map(|page| f64::from(page.w)).fold(0.0, f64::max);
        let mut y = MARGIN;
        layout
            .pages
            .iter()
            .map(|page| {
                let (w, h) = (f64::from(page.w), f64::from(page.h.min(1e6)));
                let area = PageArea { x: MARGIN + (widest - w) / 2.0, y, w, h };
                y += h + GAP;
                area
            })
            .collect()
    }

    /// The page nearest canvas height `y`.
    fn page_at(areas: &[PageArea], y: f64) -> usize {
        areas.iter().rposition(|area| y >= area.y - GAP / 2.0).unwrap_or(0)
    }

    /// Premultiplied RGBA of the canvas area at (`x`, `y`) at `scale` pixels per point.
    fn compose(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Vec<u8> {
        let mut out = self.pasteboard.repeat(width as usize * height as usize);
        let layout = self.drawn_layout();
        let session = self.session.borrow();
        let doc = self.shown_preedit().map_or(&session.doc, |preedit| &preedit.doc);
        let options = self.render_options();
        for (index, area) in Self::areas(&layout).iter().enumerate() {
            // The part of the page inside the output, in output pixels.
            let px0 = ((area.x - x) * scale).floor().max(0.0);
            let py0 = ((area.y - y) * scale).floor().max(0.0);
            let px1 = ((area.x + area.w - x) * scale).ceil().min(f64::from(width));
            let py1 = ((area.y + area.h - y) * scale).ceil().min(f64::from(height));
            if px1 <= px0 || py1 <= py0 {
                continue;
            }
            let Some(page) = layout.pages.get(index) else { continue };
            let (w, h) = ((px1 - px0) as u32, (py1 - py0) as u32);
            // Page points → canvas points → output pixels → this piece's pixels.
            let view = kurbo::Affine::translate((-px0, -py0)) * kurbo::Affine::scale(scale) * kurbo::Affine::translate((area.x - x, area.y - y));
            let rendered = kition_doc_render::render_region(doc, page, w, h, view, &options);
            blit(&mut out, width, &rendered.pixels, w, px0 as u32, py0 as u32);
        }
        out
    }

    /// Straight-alpha 1:1 pixels of the integer area (`x0`, `y0`) `w` × `h`, from cached pages.
    fn full(&self, x0: i32, y0: i32, w: u32, h: u32) -> Vec<u8> {
        let mut out = self.pasteboard.repeat(w as usize * h as usize);
        let layout = self.drawn_layout();
        // The pages as drawn: a composition shown changes them, not the document's revision.
        let revision = self.revision();
        for (index, area) in Self::areas(&layout).iter().enumerate() {
            let (sx, sy) = (area.x.round() as i32, area.y.round() as i32);
            let (pw, ph) = (area.w.ceil() as i32, area.h.ceil() as i32);
            let (ox0, oy0) = (sx.max(x0), sy.max(y0));
            let (ox1, oy1) = ((sx + pw).min(x0 + w as i32), (sy + ph).min(y0 + h as i32));
            if ox0 >= ox1 || oy0 >= oy1 {
                continue;
            }
            let fresh = self.cache.borrow().get(&index).is_some_and(|(rev, _)| *rev == revision);
            if !fresh {
                let Some(page) = layout.pages.get(index) else { continue };
                let session = self.session.borrow();
                let doc = self.shown_preedit().map_or(&session.doc, |preedit| &preedit.doc);
                let rendered = kition_doc_render::render_page(doc, page, 1.0, &self.render_options());
                drop(session);
                let mut cache = self.cache.borrow_mut();
                if cache.len() >= CACHED_PAGES {
                    cache.clear();
                }
                cache.insert(index, (revision, rendered));
            }
            let cache = self.cache.borrow();
            let Some((_, image)) = cache.get(&index) else { continue };
            let iw = image.width as i32;
            for row in oy0..oy1.min(sy + image.height as i32) {
                let from = (((row - sy) * iw + (ox0 - sx)) * 4) as usize;
                let to = (((row - y0) * w as i32 + (ox0 - x0)) * 4) as usize;
                let len = ((ox1.min(sx + iw) - ox0).max(0) * 4) as usize;
                if let (Some(source), Some(target)) = (image.pixels.get(from..from + len), out.get_mut(to..to + len)) {
                    target.copy_from_slice(source);
                }
            }
        }
        straight(&mut out);
        out
    }

    /// The layout the pages are drawn from: with what an input method composes typed in
    /// (`composing`), else the document's.
    fn drawn_layout(&self) -> Arc<DocLayout> {
        match self.shown_preedit() {
            Some(preedit) => preedit.layout.clone(),
            None => self.layout(),
        }
    }

    /// The caret and the selection (while an input method composes, the caret in the
    /// composition and the line under it).
    fn overlays(&self) -> Vec<ToolOverlay> {
        let layout = self.layout();
        let areas = Self::areas(&layout);
        let preedit = self.preedit_overlays();
        let session = self.session.borrow();
        let mut out = Vec::new();
        let at = |page: usize, x: f32, y: f32| areas.get(page).map(|area| (area.x + f64::from(x), area.y + f64::from(y)));
        let (a, b) = session.sel.ordered();
        if let Some(preedit) = preedit {
            out.extend(preedit);
        } else if !session.sel.is_collapsed() {
            for (page, rect) in layout.selection_rects(&session.doc, &a, &b, session.page_hint) {
                let (Some((x0, y0)), Some((x1, y1))) = (at(page, rect.x, rect.y), at(page, rect.x + rect.w, rect.y + rect.h)) else { continue };
                out.push(ToolOverlay::Highlight { quad: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], color: SELECTION });
            }
        } else if let Some(caret) = layout.caret_on(&session.sel.focus, session.page_hint)
            && let (Some((ax, ay)), Some((bx, by))) = (at(caret.page, caret.x, caret.top), at(caret.page, caret.x, caret.top + caret.height))
        {
            out.push(ToolOverlay::Line { ax, ay, bx, by, color: CARET, dashed: false });
        }
        drop(session);
        self.text_tool_overlays(&layout, &areas, &mut out);
        margin_guides::append(&layout, &areas, &mut out);
        self.object_overlays(&areas, &mut out);
        self.ink_overlays(&areas, &mut out);
        out
    }

    fn respond(&self) -> ToolResponse {
        let requests = std::mem::take(&mut self.session().status);
        ToolResponse {
            requests: if requests.is_empty() { Vec::new() } else { vec![ToolRequest::Status { text: requests }] },
            overlays: self.overlays(),
            cursor: {
                let layout = self.layout();
                self.text_cursor(&layout, &Self::areas(&layout))
            },
            // A word processor always types: editing keys and text come here.
            busy: true,
            wants_text: true,
        }
    }

    /// The registry's name for a key the host sends (`ArrowLeft` → `Left`).
    fn key_name(key: &str) -> Option<String> {
        let name = match key {
            "ArrowLeft" => "Left",
            "ArrowRight" => "Right",
            "ArrowUp" => "Up",
            "ArrowDown" => "Down",
            "Home" | "End" | "PageUp" | "PageDown" | "Enter" | "Tab" | "Backspace" | "Delete" | "Escape" => key,
            " " => "Space",
            other if other.chars().count() == 1 => return Some(other.to_ascii_uppercase()),
            other if other.starts_with('F') && other.len() <= 3 => other,
            _ => return None,
        };
        Some(name.to_owned())
    }

    fn combo(mods: ToolMods, key: &str, shift: bool) -> String {
        let mut combo = String::new();
        if mods.command || mods.control {
            combo.push_str("Mod+");
        }
        if mods.alt {
            combo.push_str("Alt+");
        }
        if mods.shift && shift {
            combo.push_str("Shift+");
        }
        combo.push_str(key);
        combo
    }
}

/// Copies a `w`-wide premultiplied image into `out` (`width` wide) at (`x`, `y`).
/// Commands the engine marks pure that change the document all the same: the history, opening a
/// header or footer for editing (which makes one when there is none), and the `file` commands that
/// make, open, reload, close, save, or encrypt the document (`file.autosave` saves it from then on).
const CHANGES_DOCUMENT: &[&str] = &[
    "edit.undo",
    "edit.redo",
    "header.edit",
    "footer.edit",
    "file.new",
    "file.newFromTemplate",
    "file.open",
    "file.save",
    "file.saveAs",
    "file.close",
    "file.encrypt",
    "file.versions",
    "file.autosave",
];

/// Whether running a command changes the document (the catalog's `mutates`).
fn changes_document(spec: &kition_doc_engine::CommandSpec) -> bool {
    spec.mutates || CHANGES_DOCUMENT.contains(&spec.id)
}

fn blit(out: &mut [u8], width: u32, src: &[u8], w: u32, x: u32, y: u32) {
    let row_len = w as usize * 4;
    for (row, source) in src.chunks_exact(row_len).enumerate() {
        let to = ((y as usize + row) * width as usize + x as usize) * 4;
        if let Some(target) = out.get_mut(to..to + row_len) {
            target.copy_from_slice(source);
        }
    }
}

/// Premultiplied to straight alpha, in place.
fn straight(pixels: &mut [u8]) {
    for px in pixels.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        if a != 0 && a != 255 {
            for channel in px.iter_mut().take(3) {
                *channel = ((u32::from(*channel) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// `Mod+B` (Kition Doc) as `Cmd+B` (the Kition platform, where Cmd is Control off macOS). The
/// catalogue takes one key a command: of `Mod+Y / Mod+Shift+Z` the first; the editor binds the
/// others it needs itself (`web/src/keys.ts`), and the text tool takes every one (`KitTools::key`).
fn shortcut(text: &str) -> Option<String> {
    text.split(" / ").next().filter(|first| !first.is_empty()).map(|first| first.replace("Mod+", "Cmd+"))
}

impl KitDocument for DocDocument {
    /// Opens a Word document (and the other formats Kition Doc reads).
    fn open(name: &str, bytes: &[u8]) -> Result<Self, String> {
        let doc = kition_doc_engine::io::open_bytes(name, bytes)?;
        Ok(Self::from_session(files::session(files::as_saved(name, doc))))
    }

    /// A new, empty document (Letter, as Kition Doc's default).
    fn create(_name: &str, _width: u32, _height: u32) -> Result<Self, String> {
        let mut session = files::session(kition_doc_engine::doc::Document::new());
        session.dirty = false;
        Ok(Self::from_session(session))
    }

    fn width(&self) -> u32 {
        let layout = self.layout();
        let widest = layout.pages.iter().map(|page| f64::from(page.w)).fold(612.0, f64::max);
        (widest + 2.0 * MARGIN).round() as u32
    }

    fn height(&self) -> u32 {
        let layout = self.layout();
        let areas = Self::areas(&layout);
        areas.last().map_or(2.0 * MARGIN + 792.0, |area| area.y + area.h + MARGIN).round() as u32
    }

    fn revision(&self) -> u64 {
        let session = self.session.borrow();
        // The caret moves without changing the document; the overlays follow it either way. A
        // composition shown changes the pages drawn, not the document (`composing`).
        session.rev().saturating_add(self.drawn)
    }

    /// Never while the document is open for commenting only: comments reach the others through
    /// the replica, and the file is not this person's to save.
    fn dirty(&self) -> bool {
        let session = self.session.borrow();
        session.dirty && !session.comment_only()
    }

    fn render(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let (width, height) = (width.clamp(1, MAX_RENDER_SIDE), height.clamp(1, MAX_RENDER_SIDE));
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        if scale < 1.0 {
            let (x0, y0) = (x.floor() as i32, y.floor() as i32);
            let span_x = ((f64::from(width) / scale).ceil() as i32).max(1);
            let span_y = ((f64::from(height) / scale).ceil() as i32).max(1);
            return self.render_area(x0, y0, x0.saturating_add(span_x), y0.saturating_add(span_y), width, height);
        }
        if scale == 1.0 && x.fract() == 0.0 && y.fract() == 0.0 {
            return Ok(self.full(x as i32, y as i32, width, height));
        }
        let mut out = self.compose(x, y, scale, width, height);
        straight(&mut out);
        Ok(out)
    }

    fn render_area(&self, x0: i32, y0: i32, x1: i32, y1: i32, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let w = u32::try_from(x1.saturating_sub(x0).max(1)).unwrap_or(1).min(MAX_RENDER_SIDE);
        let h = u32::try_from(y1.saturating_sub(y0).max(1)).unwrap_or(1).min(MAX_RENDER_SIDE);
        let full = self.full(x0, y0, w, h);
        Ok(raster::render_area(Frame { rgba: &full, width: w, height: h }, 0, 0, w as i32, h as i32, width, height))
    }

    /// What is wrong with the document (KHP A21): see [`check`].
    fn check(&self) -> Vec<kition_kit::diagnostics::Diagnostic> {
        let layout = self.layout();
        check::check(&self.session.borrow().doc, Some(&layout))
    }

    fn execute(&mut self, command: &str, params: Value) -> Result<Value, String> {
        self.refuse_unknown(command, &params)?;
        self.clipboard_gate(command, &params)?;
        let params = self.resolved(command, params);
        let selection = self.session.borrow().sel.clone();
        let answer = self.session().run(command, &params).map_err(|error| error.to_string())?;
        self.selection_active |= self.session.borrow().sel != selection || matches!(command, "select.range" | "caret.set");
        self.copied_by(command);
        self.note_inserted_chart(command, &params, &answer);
        self.settle_preedit();
        Ok(answer)
    }

    /// The document: its pages (with where each lies on the canvas), the caret's page, words,
    /// the selection, and the history.
    fn inspect(&self) -> Result<Value, String> {
        // The selected shape or chart, for the editor's shared Shape Format and Chart Design controls.
        let selected = self.selected_object().filter(|(_, uid, _)| *uid > 0).map(|(_, uid, _)| uid).or_else(|| self.chart_just_inserted());
        let is_chart = selected.is_some_and(|uid| {
            let session = self.session.borrow();
            session
                .doc
                .find_shape(uid)
                .and_then(|p| session.doc.para_at(&p)?.object_at(p.off).cloned())
                .is_some_and(|o| matches!(o, kition_doc_engine::doc::InlineObject::Graphic { .. }))
        });
        let (shape, chart) = match selected {
            Some(uid) if is_chart => (None, Some(uid.to_string())),
            Some(uid) => (Some(uid.to_string()), None),
            None => (None, None),
        };
        // Every selected drawing object (the selected one, then those added with Shift), for
        // grouping.
        let shapes: Vec<String> = shape.iter().cloned().chain(self.also_selected_ids()).collect();
        let layout = self.layout();
        let areas = Self::areas(&layout);
        let pages: Vec<Value> = areas
            .iter()
            .enumerate()
            .map(|(index, area)| json!({ "index": index, "area": { "x": area.x, "y": area.y, "width": area.w, "height": area.h } }))
            .collect();
        let gesture = self.object_drag_page().is_some();
        // What an input method composes, and where a composition starts (the editor puts the
        // input method's text input there): session state, never in the file.
        let preedit = self.preedit_info();
        let input_at = self.input_at(&layout, &areas);
        let session = self.session.borrow();
        let caret = layout.caret_on(&session.sel.focus, session.page_hint);
        let page = caret.as_ref().map_or(session.page_hint, |caret| caret.page);
        // Where the caret is on the canvas (the selection's moving end), so the editor can keep it
        // in view: session state, never in the file.
        let caret = caret.and_then(|caret| {
            let area = areas.get(caret.page)?;
            Some(json!({ "page": caret.page, "x": area.x + f64::from(caret.x), "top": area.y + f64::from(caret.top), "height": f64::from(caret.height) }))
        });
        // Whether Escape has something to cancel: a gesture, a shape chosen to draw, or a
        // selection (text or an object). The editor gives Escape to the text tool only then, so
        // an Escape with nothing to cancel is left to the host (in Focus it leaves Focus).
        let escape = gesture
            || self.text_tool.gesture.is_some()
            || session.painter.is_some()
            || session.view.draw.shape.is_some()
            || !session.sel.is_collapsed();
        // Whether the selection is the whole story (Select All, or three clicks in the selection
        // bar): the view stays where it is (walkthrough C10).
        let all = !session.sel.is_collapsed() && {
            let (a, b) = session.sel.ordered();
            a == session.doc.start_of(a.story) && b == session.doc.end_of(a.story)
        };
        // The format painter held: once, or until put down (double-clicked).
        let painter = session.painter.as_ref().map(|(_, _, sticky)| if *sticky { "sticky" } else { "once" });
        // The body's blocks with their tool ids (C8-land-14-doc): what `on`, `doc_query` and a
        // write's receipt name them by, beside the index `select.range` takes.
        let blocks: Vec<Value> = session
            .doc
            .body
            .iter()
            .take(INSPECTED_BLOCKS)
            .enumerate()
            .map(|(index, block)| match &**block {
                kition_doc_engine::doc::Block::Para(p) => {
                    let text: String = p.text.chars().filter(|c| *c != kition_doc_engine::doc::para::OBJ).take(200).collect();
                    json!({ "index": index, "id": p.props.para_id.map(|v| format!("p{v:08X}")), "type": "paragraph", "text": text })
                }
                kition_doc_engine::doc::Block::Table(t) => json!({
                    "index": index,
                    "id": t.rows.first().and_then(|r| r.props.para_id).map(|v| format!("t{v:08X}")),
                    "type": "table",
                    "rows": t.rows.len(),
                }),
            })
            .collect();
        Ok(json!({
            "blocks": blocks,
            "pages": pages,
            "page": page,
            "caret": caret,
            "preedit": preedit,
            "inputAt": input_at,
            "words": session.word_count(),
            "selected": session.selected_text().chars().count(),
            // Where the caret is (`body`, or a header or footer part), and whether an equation is
            // being edited: the editor's contextual tabs follow them.
            "story": match session.sel.focus.story {
                kition_doc_engine::doc::StoryRef::Body => "body",
                kition_doc_engine::doc::StoryRef::Part(_) => "headerFooter",
            },
            "equation": session.math.is_some(),
            "shape": shape,
            "shapes": shapes,
            "chart": chart,
            "escape": escape,
            "all": all,
            "painter": painter,
            "canUndo": session.can_undo(),
            "canRedo": session.can_redo(),
        }))
    }

    fn commands(&self) -> Vec<KitCommand> {
        let mut commands: Vec<kition_kit::KitCommand> = {
            let session = self.session.borrow();
            session
                .registry
                .all()
                .iter()
                .map(|spec| {
                    let menu: Vec<String> = spec.location.split('›').map(|part| part.trim().to_owned()).filter(|part| !part.is_empty()).collect();
                    KitCommand {
                        id: spec.id.to_owned(),
                        title: spec.label.trim_end_matches('…').to_owned(),
                        menu: (!menu.is_empty() && !NOT_MENUS.contains(&menu[0].as_str())).then_some(menu),
                        shortcut: shortcut(spec.shortcut_on(self.platform)),
                        params: Some(spec.params.to_owned()),
                        mutates: changes_document(spec),
                        enabled: (spec.enabled)(&session).is_none(),
                        ..Default::default()
                    }
                    .with_kit_schema()
                })
                .collect()
        };
        // Served for a host, the kit starts no program: Read Aloud is not there (KHP A45).
        if files::served() {
            commands.retain(|command| !files::AGENT_EXCLUDED.contains(&command.id.as_str()));
        }
        kition_kit::files::declare(&mut commands, files::FILES);
        describe::describe(&mut commands);
        for command in &mut commands {
            if let Some(schema) = command.schema.as_mut() {
                params::complete(&command.id, schema);
            }
            acts::declare(command);
        }
        commands
    }

    fn editor_state(&self) -> Value {
        let mut state = Self::state_of(&self.session.borrow());
        state["activeSelection"] = json!(self.selection_active);
        state
    }

    fn restore_editor_state(&mut self, saved: &Value) -> Result<(), String> {
        self.restore_selection_state(saved)
    }

    fn carry_replica_objects(&mut self, _previous: &Self) -> Result<(), String> {
        self.validating_reread = true;
        Ok(())
    }

    fn fork_for_premises(&self) -> Result<Self, String> {
        let mut fork = Self::from_session(self.session.borrow().fork_for_premises().map_err(|error| error.to_string())?);
        fork.planning = true;
        fork.selection_active = self.selection_active;
        fork.copied.set(self.copied.get());
        Ok(fork)
    }

    fn reconcile_reread(&mut self, fresh: &Self) -> Result<bool, String> {
        let mut state = fresh.editor_state();
        let after = fresh.session.borrow().doc.clone();
        self.session().reread_document(&after).map_err(|error| error.to_string())?;
        // The retained replica follows text edits before the caret as well as paragraph moves.
        state["positions"] = Self::state_of(&self.session.borrow())["positions"].clone();
        self.restore_selection_state(&state)?;
        Ok(true)
    }

    fn set_context(&mut self, context: &kition_kit::CallContext) -> Result<(), String> {
        self.apply_context(context)
    }

    fn batch_settler(&self) -> Option<kition_kit::BatchSettler> {
        Some(settle::settler())
    }

    fn derived(&self, ids: &[Value], names: &[String]) -> Result<Vec<kition_kit::DerivedValues>, String> {
        Ok(self.derived_objects(ids, names))
    }

    fn under(&self, command: &str, params: &Value) -> Result<Option<kition_kit::Under>, String> {
        self.under_call(command, params)
    }

    fn under_step(&self, command: &str, params: &Value, _step: &kition_kit::PremiseStep) -> Result<Option<kition_kit::Under>, String> {
        // Prefix planning already executed earlier steps on an isolated session.
        // Read its actual selection and mode rather than predicting them again.
        if self.planning {
            self.pass.actual_state();
        }
        Ok(Some(self.under_call(command, params)?.unwrap_or_default()))
    }

    /// The session's mail merge list (C8-doc-6, Kit Process Protocol external inputs): set by a person
    /// (Select Recipients names a file or takes rows, which an agent may not run) and read by the
    /// merge commands, no stored field holds it. Its fingerprint is that of its fields and rows.
    fn external_inputs(&self) -> Result<Vec<kition_kit::ExternalInput>, String> {
        let session = self.session.borrow();
        let merge = &session.merge;
        let value = serde_json::json!([merge.kind, merge.headers, merge.rows]);
        Ok(vec![kition_kit::ExternalInput { name: acts::MERGE_SOURCE.to_owned(), fingerprint: Some(family::fingerprint_of(&value)) }])
    }

    fn covered(&self, command: &str, params: &Value, objects: &[kition_kit::CoveredAsk]) -> Result<Option<kition_kit::CoveredAt>, String> {
        self.covered_call(command, params, objects)
    }

    fn content_at(&self, ask: &kition_kit::ContentAsk, _history: &dyn kition_kit::HistoricalBlobs) -> Result<Option<bool>, String> {
        Ok(self.content_call(ask))
    }

    fn disabled_commands(&self) -> Vec<String> {
        let session = self.session.borrow();
        session.registry.all().iter().filter(|spec| (spec.enabled)(&session).is_some()).map(|spec| spec.id.to_owned()).collect()
    }

    /// The document as Word `.docx`, or in the format `name`'s extension asks for. Like Word, a
    /// save stamps the file it writes (modified time, revision) when the document changed since
    /// it was last saved or opened; saving with nothing new writes the same bytes again. The
    /// stamp goes into the bytes only, never the document or its replica: saving is not an edit
    /// of a shared document, and the replica a host stores beside the file reopens with it as
    /// the same version.
    ///
    /// Served headless for agents (`files::served`), every command that changes the document
    /// stamps it as it runs (C8-doc-2, `Session::stamp_edits`): the stamp is part of the write's
    /// receipt, and the save writes the document as it is. What a write's receipt says is then
    /// exactly what the saved file holds, who last modified it included. A batch of object-model
    /// operations (no command) is not stamped.
    fn save(&self, name: Option<&str>) -> Result<Vec<u8>, String> {
        // Served, every command stamped the document as it ran (its receipt says so): the save
        // writes it as it is. Otherwise a save stamps the bytes only (see above).
        let doc = if files::served() { self.session.borrow_mut().saved_as_is() } else { self.session.borrow_mut().save_stamped() };
        kition_doc_engine::io::save_bytes(name.unwrap_or("document.docx"), &doc)
    }

    fn mark_saved(&mut self) {
        self.session().dirty = false;
    }
}

impl KitTools for DocDocument {
    fn tools(&self) -> Vec<ToolInfo> {
        vec![ToolInfo { id: "text".into(), label: "Text".into(), shortcut: None, icon: "tool-type".into(), group: 0 }]
    }

    fn tool(&self) -> String {
        "text".into()
    }

    fn select_tool(&mut self, id: &str, _zoom: f64) -> Result<ToolResponse, String> {
        if id != "text" {
            return Err(format!("no tool {id}"));
        }
        Ok(self.respond())
    }

    fn pointer(&mut self, event: &ToolPointer) -> Result<ToolResponse, String> {
        let layout = self.layout();
        let areas = Self::areas(&layout);
        self.track_pointer(event);
        // Pictures, shapes and text box borders before the text under them (unless the text tool
        // holds a press); an object's drag stays in the coordinates of the page it started on.
        if self.text_tool.gesture.is_none() {
            let page = Self::page_at(&areas, event.y);
            if let Some((opage, oarea)) =
                self.object_drag_page().and_then(|p| areas.get(p).map(|a| (p, *a))).or_else(|| areas.get(page).map(|a| (page, *a)))
                && self.object_pointer(event, opage, event.x - oarea.x, event.y - oarea.y)?
            {
                self.settle_preedit();
                return Ok(self.respond());
            }
        }
        self.text_pointer(event, &layout, &areas)?;
        self.settle_preedit();
        Ok(self.respond())
    }

    fn key(&mut self, key: &str, mods: ToolMods, zoom: f64) -> Result<ToolResponse, String> {
        let answer = self.key_press(key, mods, zoom);
        self.settle_preedit();
        answer
    }

    fn text(&mut self, text: &str, _zoom: f64) -> Result<ToolResponse, String> {
        // What an input method chose ends its composition, a step of its own (`composing`).
        let committed = self.commit_preedit();
        if !text.is_empty() && text.chars().all(|c| !c.is_control()) {
            self.session().run("text.insert", &json!({ "text": text })).map_err(|error| error.to_string())?;
        }
        if committed {
            self.session().close_typing();
        }
        Ok(self.respond())
    }

    fn state(&mut self, _x: f64, _y: f64, _mods: ToolMods, _zoom: f64) -> ToolResponse {
        self.respond()
    }
}

impl DocDocument {
    /// A key for the text tool (see `KitTools::key`).
    fn key_press(&mut self, key: &str, mods: ToolMods, _zoom: f64) -> Result<ToolResponse, String> {
        if self.object_key(key, mods)? {
            return Ok(self.respond());
        }
        // Escape drops a press under way (the text stays put), then puts down the format painter.
        if key == "Escape" && (self.cancel_text_gesture() || self.session().painter.take().is_some()) {
            return Ok(self.respond());
        }
        let Some(name) = Self::key_name(key) else { return Ok(self.respond()) };
        let registry = self.session.borrow().registry.clone();
        if let Some(spec) = registry.by_shortcut_on(&Self::combo(mods, &name, true), self.platform) {
            self.session().run(spec.id, &json!({})).map_err(|error| error.to_string())?;
        } else if mods.shift
            && let Some(spec) = registry.by_shortcut_on(&Self::combo(mods, &name, false), self.platform)
            && spec.id.starts_with("caret.")
        {
            // Shift extends caret movement.
            self.session().run(spec.id, &json!({ "extend": true })).map_err(|error| error.to_string())?;
        } else if name == "Escape" {
            let mut session = self.session();
            let focus = session.sel.focus.clone();
            session.sel = Selection::caret(focus);
        }
        Ok(self.respond())
    }
}

kition_kit::export_kit_document!(DocDocument as "DocDocument");
kition_kit::export_kit_tools!(DocDocument as "DocDocument");

/// Adds a font file's faces (TTF, OTF, TTC) for text the embedded faces do not cover, such as
/// Chinese in a web build, which has no system fonts to fall back on. Returns the faces added.
pub fn add_font(bytes: &[u8]) -> usize {
    kition_doc_fonts::FontDb::global().add_font(bytes.to_vec())
}

impl DocDocument {
    /// Opens the document for commenting only (a commenter of a shared document, the Kit UI
    /// protocol's `document.comments`): the engine refuses every edit of the content (typing,
    /// commands, the file commands that change it) and writes only comment threads (Kition platform
    /// rule M23). The caret and selections still move, so a comment can go on a selection.
    pub fn set_comment_only(&mut self, comment_only: bool) {
        self.session().set_comment_only(comment_only);
    }

    /// Draws pages with the editor's editing look (headers and footers dimmed, spelling and
    /// grammar squiggles) instead of as printed. Only the kit's own editor turns it on.
    pub fn set_editing(&mut self, editing: bool) {
        self.editing = editing;
        self.cache.borrow_mut().clear();
    }

    /// The keys come from a Mac's keyboard (the editor's `setup` on a Mac): ⌘← and ⌘→ go to the
    /// ends of the line, ⌥← and ⌥→ a word (`CommandSpec::mac_shortcut`).
    pub fn set_mac(&mut self, mac: bool) {
        self.platform = if mac { kition_doc_engine::Platform::Mac } else { kition_doc_engine::Platform::Default };
    }

    /// The canvas around the pages in `#rrggbb` (the editor's canvas color).
    pub fn set_backdrop(&mut self, color: &str) -> Result<(), String> {
        let hex = color.trim().trim_start_matches('#');
        let channel = |at: usize| hex.get(at..at + 2).and_then(|part| u8::from_str_radix(part, 16).ok());
        match (hex.len(), channel(0), channel(2), channel(4)) {
            (6, Some(r), Some(g), Some(b)) => {
                self.pasteboard = [r, g, b, 255];
                Ok(())
            }
            _ => Err(format!("{color} is not a #rrggbb color")),
        }
    }
}

/// Doc's own additions to the exported class, for its embedded editor.
#[cfg(target_arch = "wasm32")]
pub mod web {
    use kition_kit::wasm_bindgen::prelude::*;
    use serde_json::json;

    use crate::__kition_kit_export::WebDocument;

    /// Adds a font file's faces for text the embedded faces do not cover; returns how many.
    #[wasm_bindgen(js_name = addFont, wasm_bindgen = kition_kit::wasm_bindgen)]
    pub fn add_font(bytes: &[u8]) -> u32 {
        crate::add_font(bytes) as u32
    }

    fn error(message: String) -> JsError {
        JsError::new(&message)
    }

    /// An origin in the KDM's commit form, the JSON array `[kind, principal, label]`.
    fn origin_of(origin: &str) -> Result<kition_doc_engine::Origin, JsError> {
        kition_doc_engine::Origin::decode(origin).ok_or_else(|| error(format!("origin is not [kind, principal, label]: {origin}")))
    }

    #[wasm_bindgen(js_class = "DocDocument", wasm_bindgen = kition_kit::wasm_bindgen)]
    impl WebDocument {
        /// The canvas around the pages in `#rrggbb` (the editor's canvas color).
        #[wasm_bindgen(js_name = setBackdrop)]
        pub fn set_backdrop(&mut self, color: &str) -> Result<(), JsError> {
            self.0.set_backdrop(color).map_err(error)
        }

        /// Draws pages with the editor's editing look (see `DocDocument::set_editing`).
        #[wasm_bindgen(js_name = setEditing)]
        pub fn set_editing(&mut self, editing: bool) {
            self.0.set_editing(editing);
        }

        /// The keys come from a Mac's keyboard (see `DocDocument::set_mac`).
        #[wasm_bindgen(js_name = setMac)]
        pub fn set_mac(&mut self, mac: bool) {
            self.0.set_mac(mac);
        }

        /// Opens the document for commenting only (see `DocDocument::set_comment_only`).
        #[wasm_bindgen(js_name = setCommentOnly)]
        pub fn set_comment_only(&mut self, comment_only: bool) {
            self.0.set_comment_only(comment_only);
        }

        // Kition phase 3e, Task 3: the collaboration members (engine contract 2). Shapes mirror
        // `kition-sdk`'s `session::web` (`export_kit_session!` does not apply here; see
        // `crate::collab`).

        pub fn version(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.version().map_err(error)
        }

        #[wasm_bindgen(js_name = exportChanges)]
        pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.export_changes(since).map_err(error)
        }

        #[wasm_bindgen(js_name = importChanges)]
        pub fn import_changes(&mut self, bytes: &[u8]) -> Result<String, JsError> {
            self.0.import_changes(bytes).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        pub fn snapshot(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.snapshot().map_err(error)
        }

        pub fn undo(&mut self) -> Result<String, JsError> {
            self.0.undo().map(|(done, damage)| json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        pub fn redo(&mut self) -> Result<String, JsError> {
            self.0.redo().map(|(done, damage)| json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        #[wasm_bindgen(js_name = canUndo)]
        pub fn can_undo(&self) -> bool {
            self.0.can_undo()
        }

        #[wasm_bindgen(js_name = canRedo)]
        pub fn can_redo(&self) -> bool {
            self.0.can_redo()
        }

        /// `origin` is the KDM's commit form, the JSON array `[kind, principal, label]`.
        #[wasm_bindgen(js_name = executeAs)]
        pub fn execute_as(&mut self, command: &str, params: &str, origin: &str) -> Result<String, JsError> {
            let params = kition_kit::parse_params(params).map_err(error)?;
            let origin = origin_of(origin)?;
            self.0.execute_as(command, params, &origin).map(|value| value.to_string()).map_err(error)
        }

        /// Every blob the replica references or can still bring back, for a peer's `addBlobs`.
        pub fn blobs(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.blobs().map_err(error)
        }

        /// Adds a peer's blobs (from their `blobs()`): kept until the next `importChanges`.
        #[wasm_bindgen(js_name = addBlobs)]
        pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), JsError> {
            self.0.add_blobs(bytes).map_err(error)
        }

        /// The blobs newly needed since `since` (empty bytes: everything the current state
        /// references), encoded like `blobs()`: for live sync, so a change ships only the blobs
        /// it needs.
        #[wasm_bindgen(js_name = blobsSince)]
        pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.blobs_since(since).map_err(error)
        }

        #[wasm_bindgen(js_name = kdmResets)]
        pub fn kdm_resets(&self) -> u32 {
            self.0.kdm_resets()
        }

        /// `model/describe` (the object model, `KitObjectModel 1.0`) as JSON text, through the
        /// change exchange `kition-doc` serves it with headless ([`crate::collab`]). A failure's
        /// message is the error's JSON (`{"code", "index", "message"}`).
        #[wasm_bindgen(js_name = modelDescribe)]
        pub fn model_describe(&mut self) -> Result<String, JsError> {
            kition_kit::session::model::Exchange::new()
                .describe(&mut self.0)
                .map(|answer| answer.to_string())
                .map_err(|failure| error(failure.to_json().to_string()))
        }

        /// `model/batch`: the request and the answer as JSON text, one undo step; a failure's
        /// message is the error's JSON (`{"code", "index", "message"}`).
        #[wasm_bindgen(js_name = modelBatch)]
        pub fn model_batch(&mut self, request: &str) -> Result<String, JsError> {
            let request: serde_json::Value = serde_json::from_str(request).map_err(|failure| {
                error(json!({ "code": "invalid-op", "index": null, "message": format!("the batch is not JSON: {failure}") }).to_string())
            })?;
            kition_kit::session::model::Exchange::new()
                .batch(&mut self.0, &request, false)
                .map(|answer| answer.to_string())
                .map_err(|failure| error(failure.to_json().to_string()))
        }

        /// A draft's changes taken in as one undo step, recorded with `origin` (`[kind, principal,
        /// label]`): `{"damage", "summary"}`. A refusal fails with the KDM's message.
        #[wasm_bindgen(js_name = mergeChanges)]
        pub fn merge_changes(&mut self, changes: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.merge_changes(changes, &origin).map(|answer| answer.to_string()).map_err(error)
        }

        /// What merging a draft's changes would change now, as `StateDiff` JSON; changes nothing.
        #[wasm_bindgen(js_name = compareChanges)]
        pub fn compare_changes(&self, changes: &[u8]) -> Result<String, JsError> {
            self.0.compare_changes(changes).map(|answer| answer.to_string()).map_err(error)
        }

        /// Writes the state a snapshot holds over the document as one change of `origin`;
        /// answers the damage JSON.
        #[wasm_bindgen(js_name = restoreState)]
        pub fn restore_state(&mut self, snapshot: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.restore_state(snapshot, &origin).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        /// An agent's reply (JSON text) with its object ids named as the tool interface names them.
        #[wasm_bindgen(js_name = modelReply)]
        pub fn model_reply(&self, value: &str) -> Result<String, JsError> {
            self.0.model_reply(value).map_err(error)
        }

        /// What changed since the version `since` (a receipt): `{"changes": StateDiff}`, or
        /// `{"changes": null, "unavailable"}` when the replica's log does not hold it.
        #[wasm_bindgen(js_name = changesSince)]
        pub fn changes_since(&self, since: &[u8]) -> String {
            self.0.changes_since(since).to_string()
        }

        /// Opens a file with the replica a host stored for it (the Kit UI protocol's `replica`
        /// capability); a replica that does not open is ignored and the file opens plainly.
        #[wasm_bindgen(js_name = openWithReplica)]
        pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<WebDocument, JsError> {
            crate::DocDocument::open_with_replica(name, bytes, snapshot, blobs).map(Self).map_err(error)
        }

        /// Whether the document has made its replica yet (it makes one on the first shared
        /// edit).
        #[wasm_bindgen(getter, js_name = hasReplica)]
        pub fn has_replica(&self) -> bool {
            self.0.has_replica()
        }

        /// The replica's own document id, for the Kit UI protocol's `replica/info` message.
        /// `undefined` before the document has made a replica yet.
        #[wasm_bindgen(getter, js_name = docId)]
        pub fn doc_id(&self) -> Option<String> {
            self.0.doc_id()
        }

        /// What an input method composes at the caret (`text`, its cursor `cursor` characters in),
        /// shown without changing the document; empty text takes it away. Answers whether it
        /// shows (see `DocDocument::set_preedit`).
        #[wasm_bindgen(js_name = setPreedit)]
        pub fn set_preedit(&mut self, text: &str, cursor: u32) -> bool {
            self.0.set_preedit(text, cursor as usize)
        }

        /// A right-click at canvas point (`x`, `y`), or for the selection as it is when either is
        /// not a number (Shift+F10): see `DocDocument::context_menu`. Answers its JSON.
        #[wasm_bindgen(js_name = contextMenu)]
        pub fn context_menu(&mut self, x: f64, y: f64, zoom: f64) -> Result<String, JsError> {
            let point = (x.is_finite() && y.is_finite()).then_some((x, y));
            let answer = self.0.context_menu(point, zoom).map(|answer| answer.to_string()).map_err(error);
            self.0.settle_preedit();
            answer
        }
    }
}

pub use describe::NO_OUTPUT;

#[cfg(test)]
mod tests;
