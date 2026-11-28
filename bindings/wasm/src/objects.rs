//! Pictures, shapes and text boxes under the text tool: a press on one selects it (layout's own
//! hit test, so a text box is grabbed by its border and typed into inside), a drag moves it, and
//! its handles resize and turn it, and a shape's adjust handles (the yellow dots) reshape it,
//! through the gestures Kition's kits share (`kition_office_core::edit`). The preview is an overlay;
//! the release runs one command (one undo step): `drawing.transform` for a shape,
//! `drawing.format` for an adjust handle, `arrange.bounds` for a picture, `arrange.rotation` for a
//! picture turned. Escape drops it.
//!
//! Pictures by hand (plan "Doc usable by hand", stage 5A; WPS W-O1-W-O7): the selected object shows
//! its frame and eight round handles (paper inside, the brand's ring) with the rotation handle
//! above, never the text selection's tint; a handle takes a press within [`HANDLE_REACH_PX`] of
//! its centre, the nearest one winning; the pointer says what a press there does (a double arrow
//! along the handle, turned with the object; move over an object). A picture's corner keeps its
//! ratio unless Shift is held; Cmd (or Alt) sizes it about its centre. An inline picture dragged
//! stays inline: a dashed bar shows where in the text it lands, and the release is one
//! `text.move` there.

use kition_doc_doc::Pos;
use kition_doc_doc::para::{InlineObject, OBJ, Wrap};
use kition_doc_engine::cmd::draw::DrawMode;
use kition_doc_layout::hit::ObjectHit;
use kition_kit::tools::{PointerAction, ToolMods, ToolOverlay, ToolPointer};
use kition_office_core::edit::{Gesture, HandleHit, Mods, Preview, adjust_handles, box_handles, hit_adjust};
use kition_office_core::ink::{EraserGesture, InkGesture, InkTool};
use kition_office_core::model::Geometry;
use kition_office_core::ops::{DrawingOp, FrameChange};
use kition_office_core::{Point, Xfrm};
use serde_json::json;

use crate::{DocDocument, PageArea};

/// How near an object's edge (screen pixels) a press still takes it, and its drag threshold.
const REACH_PX: f64 = 4.0;
/// A resize handle takes a press this far from its centre (screen pixels; the nearest wins), as
/// Slide's handles do (kition-slide stage 0B); the rotation handle a little farther.
pub(crate) const HANDLE_REACH_PX: f64 = 7.0;
const ROTATE_REACH_PX: f64 = 8.0;
/// Frames and handles take the kit's brand: a handle is a ring of it around paper (the editor
/// draws anchors round, `web/src/App.tsx`), so it shows on the white page.
const ACCENT: [u8; 4] = [crate::BRAND[0], crate::BRAND[1], crate::BRAND[2], 255];
/// A resize handle's size, screen pixels.
pub(crate) const HANDLE_PX: f32 = 9.0;
/// Adjust handles are yellow, as in Office.
const ADJUST: [u8; 4] = [255, 214, 0, 255];
/// The pointer on a rotation handle: a turning arrow, drawn here (CSS has none).
pub(crate) const ROTATE_CURSOR: &str = "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='20' height='20' viewBox='0 0 20 20'%3E%3Cpath d='M15.5 10a5.5 5.5 0 1 1-2-4.24' fill='none' stroke='white' stroke-width='4' stroke-linecap='round'/%3E%3Cpath d='M15.5 10a5.5 5.5 0 1 1-2-4.24' fill='none' stroke='black' stroke-width='1.6' stroke-linecap='round'/%3E%3Cpath d='M11.4 3.2l4.6.6-1.6 4.3z' fill='black' stroke='white' stroke-width='.8'/%3E%3C/svg%3E\") 10 10, crosshair";

/// A pen stroke or an eraser drag under way, on a page (`KitDrawing 1.2`, DR20): previewed while
/// the pointer moves, one `drawing.ink` or `drawing.erase` on release, nothing on Escape.
pub(crate) enum InkDrag {
    Pen { gesture: InkGesture, page: usize, zoom: f64 },
    Eraser { gesture: EraserGesture, page: usize },
}

impl InkDrag {
    fn page(&self) -> usize {
        match self {
            InkDrag::Pen { page, .. } | InkDrag::Eraser { page, .. } => *page,
        }
    }
}

/// A gesture on an object under way.
pub(crate) struct ObjectDrag {
    gesture: Gesture,
    preview: Preview,
    page: usize,
    /// The shape's id in the document; 0 for a picture.
    uid: u32,
    /// A connector's end being dragged (instead of `gesture`), and its frame now.
    end: Option<(kition_office_core::connect::EndDrag, Xfrm)>,
    /// A shape from Insert › Shapes being drawn (instead of `gesture`), and its frame now.
    create: Option<(kition_office_core::connect::CreateDrag, Xfrm)>,
    /// The object is a picture: its corners keep the ratio unless Shift is held.
    picture: bool,
    /// The object is inline: moved, it lands at a place in the text (`to`: page and place),
    /// resized, it stays where it is.
    inline: bool,
    to: Option<(usize, Pos)>,
}

impl ObjectDrag {
    fn new(gesture: Gesture, page: usize, uid: u32) -> ObjectDrag {
        ObjectDrag { gesture, preview: Preview { frames: Vec::new() }, page, uid, end: None, create: None, picture: false, inline: false, to: None }
    }
}

/// The handle of `frame` under `p` (page points; `zoom` screen pixels a point): the rotation
/// handle when `turns` and within [`ROTATE_REACH_PX`], else the nearest resize handle within
/// [`HANDLE_REACH_PX`]. The handles are where `box_handles` (the kits' shared geometry) puts them.
pub(crate) fn handle_at(frame: &Xfrm, p: Point, zoom: f64, turns: bool) -> Option<HandleHit> {
    let zoom = zoom.max(0.05);
    let (pts, rot) = box_handles(frame);
    if turns && (rot - p).hypot() * zoom <= ROTATE_REACH_PX {
        return Some(HandleHit::Rotate);
    }
    pts.iter()
        .enumerate()
        .map(|(i, q)| (i, (*q - p).hypot() * zoom))
        .filter(|(_, d)| *d <= HANDLE_REACH_PX)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .and_then(|(i, _)| u8::try_from(i).ok())
        .map(HandleHit::Resize)
}

/// How far above the top edge the rotation handle is, points (the kits' shared geometry).
#[cfg(test)]
pub(crate) fn kition_office_core_offset() -> f64 {
    kition_office_core::edit::ROTATE_OFFSET
}

/// The pointer on resize handle `index` (NW, N, NE, E, SE, S, SW, W) of `frame`: the double arrow
/// along the way it pulls, turned with the object and its flips.
pub(crate) fn resize_cursor(frame: &Xfrm, index: u8) -> &'static str {
    let (dx, dy) = match index {
        0 => (-1.0, -1.0),
        1 => (0.0, -1.0),
        2 => (1.0, -1.0),
        3 => (1.0, 0.0),
        4 => (1.0, 1.0),
        5 => (0.0, 1.0),
        6 => (-1.0, 1.0),
        _ => (-1.0, 0.0),
    };
    let (dx, dy) = (if frame.flip_h { -dx } else { dx }, if frame.flip_v { -dy } else { dy });
    let turn = frame.rot.to_radians();
    let (sx, sy) = (dx * turn.cos() - dy * turn.sin(), dx * turn.sin() + dy * turn.cos());
    // The way it points, in eighths of a turn (the screen's y grows downwards), folded to a half turn.
    let eighth = ((sy.atan2(sx).to_degrees() / 45.0).round() as i64).rem_euclid(4);
    match eighth {
        0 => "ew-resize",
        1 => "nwse-resize",
        2 => "ns-resize",
        _ => "nesw-resize",
    }
}

fn frame_of(hit: &ObjectHit) -> Xfrm {
    Xfrm {
        x: f64::from(hit.rect.x),
        y: f64::from(hit.rect.y),
        w: f64::from(hit.rect.w),
        h: f64::from(hit.rect.h),
        rot: f64::from(hit.spin.deg),
        flip_h: hit.spin.flip_h,
        flip_v: hit.spin.flip_v,
    }
}

impl DocDocument {
    /// The object the selection is exactly (its one U+FFFC), with where it is laid out.
    pub(crate) fn selected_object(&self) -> Option<(ObjectHit, u32, bool)> {
        let layout = self.layout();
        let session = self.session.borrow();
        let (a, b) = session.sel.ordered();
        if a.story != b.story || a.path != b.path || b.off != a.off + OBJ.len_utf8() {
            return None;
        }
        let o = session.doc.para_at(&a)?.object_at(a.off)?;
        o.frame()?;
        let uid = o.object_uid().unwrap_or(0);
        // A shape and a picture turn; a chart or another object does not.
        let turns = (uid > 0 && matches!(o, InlineObject::Shape { .. })) || matches!(o, InlineObject::Image { .. });
        Some((layout.object(&a, session.page_hint)?, uid, turns))
    }

    /// The selected shape's geometry, for its adjust handles (none for other objects).
    fn selected_geometry(&self) -> Option<Geometry> {
        let mut session = self.session.borrow_mut();
        let (a, b) = session.sel.ordered();
        if a.story != b.story || a.path != b.path || b.off != a.off + OBJ.len_utf8() {
            return None;
        }
        let o = session.doc.para_at(&a)?.object_at(a.off)?.clone();
        if !matches!(o, InlineObject::Shape { .. }) || o.object_uid().unwrap_or(0) == 0 {
            return None;
        }
        Some(kition_doc_engine::cmd::drawing::to_drawing(&mut session, &a, &o).geometry)
    }

    /// The ids of the drawing objects added to the selection with Shift.
    pub(crate) fn also_selected_ids(&self) -> Vec<String> {
        let session = self.session.borrow();
        session
            .also_selected
            .iter()
            .filter_map(|p| session.doc.para_at(p).and_then(|q| q.object_at(p.off)).and_then(InlineObject::object_uid))
            .filter(|u| *u > 0)
            .map(|u| u.to_string())
            .collect()
    }

    /// Is the selected object a connector or a line, whose ends drag?
    fn selected_is_connector(&self) -> bool {
        let session = self.session.borrow();
        let (a, _) = session.sel.ordered();
        match session.doc.para_at(&a).and_then(|p| p.object_at(a.off)) {
            Some(InlineObject::Shape { connection, geom, kind, .. }) => {
                connection.is_some()
                    || *kind == kition_doc_doc::para::ShapeKind::Line
                    || geom.as_ref().is_some_and(|g| kition_office_core::connect::CONNECTORS.contains(&g.name.as_str()))
            }
            _ => false,
        }
    }

    /// Every shape on `page`, described.
    fn objects_on(&self, page: usize) -> Vec<kition_office_core::model::DrawingObject> {
        let mut session = self.session();
        let shapes: Vec<(Pos, InlineObject)> = session.doc.shapes().into_iter().map(|(p, o)| (p, o.clone())).collect();
        shapes
            .iter()
            .map(|(pos, o)| kition_doc_engine::cmd::drawing::to_drawing(&mut session, pos, o))
            .filter(|o| o.placement.extra.get("page").and_then(serde_json::Value::as_u64).is_none_or(|p| p == page as u64))
            .collect()
    }

    /// Shape `uid` described, and every shape on `page` (a connector's ends glue to them).
    fn page_objects(
        &self,
        uid: u32,
        page: usize,
    ) -> Option<(kition_office_core::model::DrawingObject, Vec<kition_office_core::model::DrawingObject>)> {
        let objects = self.objects_on(page);
        let me = objects.iter().find(|o| o.id == uid.to_string())?.clone();
        Some((me, objects))
    }

    /// The page an object gesture under way started on.
    pub(crate) fn object_drag_page(&self) -> Option<usize> {
        self.object_drag.as_ref().map(|d| d.page).or_else(|| self.ink_drag.as_ref().map(InkDrag::page))
    }

    /// A pointer event while the Draw tab holds a pen or the eraser: it draws or erases ink
    /// instead of selecting. `None` when no pen or eraser is in use.
    fn ink_pointer(&mut self, event: &ToolPointer, page: usize, p: Point) -> Result<Option<bool>, String> {
        let mode = self.session.borrow().view.draw.mode;
        let pen = mode.pen();
        if pen.is_none() && mode != DrawMode::Eraser && self.ink_drag.is_none() {
            return Ok(None);
        }
        let pressure = f64::from(event.pressure);
        match event.action {
            PointerAction::Down => {
                self.ink_drag = Some(match pen {
                    Some(tool) => {
                        let set = self.session.borrow().view.draw.settings(tool);
                        let color = kition_office_core::model::Color::rgb(kition_office_core::model::Rgb(set.color.0, set.color.1, set.color.2));
                        let shared = match tool {
                            kition_doc_doc::freeform::InkTool::Pen => InkTool::Pen,
                            kition_doc_doc::freeform::InkTool::Pencil => InkTool::Pencil,
                            kition_doc_doc::freeform::InkTool::Highlighter => InkTool::Highlighter,
                        };
                        InkDrag::Pen { gesture: InkGesture::start(shared, Some(color), f64::from(set.width), p, pressure), page, zoom: event.zoom }
                    }
                    None => InkDrag::Eraser { gesture: EraserGesture::start(p, kition_doc_engine::cmd::ink::ERASER_RADIUS), page },
                });
                Ok(Some(true))
            }
            PointerAction::Drag => {
                match self.ink_drag.as_mut() {
                    Some(InkDrag::Pen { gesture, .. }) => gesture.drag(p, pressure),
                    Some(InkDrag::Eraser { gesture, .. }) => gesture.drag(p),
                    None => return Ok(Some(false)),
                }
                Ok(Some(true))
            }
            PointerAction::Up => {
                let result = match self.ink_drag.take() {
                    Some(InkDrag::Pen { gesture, page, .. }) => {
                        self.session().run("drawing.ink", &json!({ "placement": { "page": page }, "strokes": [gesture.release()] }))
                    }
                    Some(InkDrag::Eraser { gesture, page }) => {
                        let path: Vec<[f64; 2]> = gesture.path().iter().map(|q| [q.x, q.y]).collect();
                        self.session().run("drawing.erase", &json!({ "placement": { "page": page }, "path": path, "radius": gesture.radius }))
                    }
                    None => return Ok(Some(false)),
                };
                result.map_err(|e| e.to_string())?;
                Ok(Some(true))
            }
            _ => Ok(Some(self.ink_drag.is_some())),
        }
    }

    /// The stroke being drawn, or the eraser's path, over the page it is on.
    pub(crate) fn ink_overlays(&self, areas: &[PageArea], out: &mut Vec<ToolOverlay>) {
        let Some(drag) = &self.ink_drag else { return };
        let Some(area) = areas.get(drag.page()) else { return };
        let path = |pts: &mut dyn Iterator<Item = Point>| {
            let mut d = String::new();
            for (i, q) in pts.enumerate() {
                d.push_str(&format!("{}{:.1},{:.1} ", if i == 0 { "M" } else { "L" }, area.x + q.x, area.y + q.y));
            }
            d
        };
        match drag {
            InkDrag::Pen { gesture, zoom, .. } => {
                let s = gesture.preview();
                let rgb = s.color.as_ref().and_then(|c| match &c.base {
                    kition_office_core::model::ColorBase::Rgb { rgb } => Some(*rgb),
                    _ => None,
                });
                let (r, g, b) = rgb.map_or((0, 0, 0), |c| (c.0, c.1, c.2));
                let alpha = (s.tool.alpha() * 255.0).round() as u8;
                let mut pts = s.path().into_iter();
                out.push(ToolOverlay::Path { d: path(&mut pts), color: [r, g, b, alpha], width: (s.width * zoom).max(1.0) as f32, dashed: false });
            }
            InkDrag::Eraser { gesture, .. } => {
                let mut pts = gesture.path().iter().copied();
                out.push(ToolOverlay::Path { d: path(&mut pts), color: [120, 120, 120, 160], width: 2.0, dashed: true });
            }
        }
    }

    /// The inline picture just before the caret (none with a selection), selected: the first
    /// Backspace after a picture (WPS W-F4). Answers whether it was.
    pub(crate) fn select_picture_before_caret(&self) -> bool {
        let at = {
            let session = self.session.borrow();
            if !session.sel.is_collapsed() || session.math.is_some() {
                return false;
            }
            let focus = &session.sel.focus;
            let Some(off) = focus.off.checked_sub(OBJ.len_utf8()) else { return false };
            let at = Pos { off, ..focus.clone() };
            let inline_picture = session
                .doc
                .para_at(&at)
                .and_then(|para| para.object_at(off))
                .is_some_and(|o| matches!(o, InlineObject::Image { .. }) && o.frame().is_some_and(|(_, _, float)| float.wrap == Wrap::Inline));
            if !inline_picture {
                return false;
            }
            at
        };
        self.select_object(&at);
        true
    }

    /// With a picture selected, the caret goes just before it (what is typed then goes there, and
    /// the picture stays: WPS W-O6).
    pub(crate) fn caret_before_selected_picture(&self) {
        let Some((hit, _, _)) = self.selected_object() else { return };
        if !self.object_is_picture(&hit.pos()) {
            return;
        }
        let start = self.session.borrow().sel.ordered().0;
        self.session().sel = kition_doc_engine::Selection::caret(start);
    }

    fn select_object(&self, pos: &Pos) {
        let end = Pos { off: pos.off + OBJ.len_utf8(), ..pos.clone() };
        self.session().sel = kition_doc_engine::Selection { anchor: pos.clone(), focus: end };
    }

    /// A pointer event for objects; `true` when it was theirs. (`px`, `py`): the point on `page`.
    pub(crate) fn object_pointer(&mut self, event: &ToolPointer, page: usize, px: f64, py: f64) -> Result<bool, String> {
        let p = Point::new(px, py);
        if let Some(handled) = self.ink_pointer(event, page, p)? {
            return Ok(handled);
        }
        let zoom = event.zoom.max(0.05);
        let tol = REACH_PX / zoom;
        // Cmd sizes about the centre (WPS W-O3), as Alt does.
        let mods = Mods { shift: event.mods.shift, alt: event.mods.alt || event.mods.command };
        match event.action {
            PointerAction::Down => {
                // A shape chosen in Insert › Shapes: this drag draws it.
                let armed = self.session.borrow().view.draw.shape.clone();
                if let Some(preset) = armed {
                    let drag = kition_office_core::connect::CreateDrag::start(&preset, p, &self.objects_on(page), tol * 1.5);
                    let frame = drag.frame();
                    self.object_drag = Some(ObjectDrag { create: Some((drag, frame)), ..ObjectDrag::new(Gesture::moving(Vec::new(), p), page, 0) });
                    return Ok(true);
                }
                // A selected connector's end: drag it onto another shape's site, or off.
                if let Some((_, uid, _)) = self.selected_object().filter(|(h, u, _)| h.page == page && *u > 0)
                    && let Some((connector, objects)) = self.page_objects(uid, page)
                    && let Some(end) = kition_office_core::connect::hit_end(&connector, p, tol)
                {
                    let drag = kition_office_core::connect::EndDrag::start(&connector, end, &objects);
                    let frame = drag.frame();
                    self.object_drag = Some(ObjectDrag { end: Some((drag, frame)), ..ObjectDrag::new(Gesture::moving(Vec::new(), p), page, uid) });
                    return Ok(true);
                }
                if let Some((hit, uid, _)) = self.selected_object().filter(|(h, _, _)| h.page == page)
                    && let Some(geometry) = self.selected_geometry()
                    && let Some(index) = hit_adjust(&frame_of(&hit), &geometry, p, tol)
                {
                    let gesture = Gesture::adjusting(uid.to_string(), frame_of(&hit), geometry, index);
                    self.object_drag = Some(ObjectDrag::new(gesture, page, uid));
                    return Ok(true);
                }
                if let Some((hit, uid, turns)) = self.selected_object().filter(|(h, _, _)| h.page == page)
                    && let Some(h) = handle_at(&frame_of(&hit), p, zoom, turns)
                {
                    let id = uid.to_string();
                    let frame = frame_of(&hit);
                    let gesture = match h {
                        HandleHit::Resize(i) => Gesture::resizing(id, i, frame, p),
                        HandleHit::Rotate => Gesture::rotating(vec![(id, frame)], &frame, p),
                    };
                    let picture = self.object_is_picture(&hit.pos());
                    self.object_drag = Some(ObjectDrag { picture, inline: hit.wrap == Wrap::Inline, ..ObjectDrag::new(gesture, page, uid) });
                    return Ok(true);
                }
                let layout = self.layout();
                let Some(hit) = layout.object_at(page, px as f32, py as f32, tol as f32) else {
                    self.session().also_selected.clear();
                    return Ok(false);
                };
                // Shift adds the object to the selection (for Group); it does not drag.
                if event.mods.shift && self.selected_object().is_some() {
                    let pos = hit.pos();
                    let mut session = self.session();
                    let (a, _) = session.sel.ordered();
                    if a != pos && !session.also_selected.contains(&pos) {
                        session.also_selected.push(pos);
                    }
                    return Ok(true);
                }
                self.session().also_selected.clear();
                let uid = {
                    let session = self.session.borrow();
                    session.doc.para_at(&hit.pos()).and_then(|para| para.object_at(hit.off)).and_then(InlineObject::object_uid).unwrap_or(0)
                };
                self.select_object(&hit.pos());
                let picture = self.object_is_picture(&hit.pos());
                // An inline picture moves in the text (WPS W-O5); a drawing object as it did.
                let inline = hit.wrap == Wrap::Inline && uid == 0;
                self.object_drag =
                    Some(ObjectDrag { picture, inline, ..ObjectDrag::new(Gesture::moving(vec![(uid.to_string(), frame_of(&hit))], p), page, uid) });
                Ok(true)
            }
            PointerAction::Drag => {
                let Some(drag) = self.object_drag.as_mut() else { return Ok(false) };
                if let Some((end, frame)) = drag.end.as_mut() {
                    *frame = end.drag(p, tol * 1.5).0;
                    return Ok(true);
                }
                if let Some((create, frame)) = drag.create.as_mut() {
                    *frame = create.drag(p, tol * 1.5, mods.shift);
                    return Ok(true);
                }
                // A picture's corner keeps its ratio, and Shift lets it go (WPS W-O3; Word, R-O20).
                let mods = match drag.gesture {
                    Gesture::Resize { handle, .. } if drag.picture && matches!(handle, 0 | 2 | 4 | 6) => Mods { shift: !mods.shift, ..mods },
                    _ => mods,
                };
                drag.preview = drag.gesture.drag(p, mods, tol, None);
                let dropping = drag.inline && matches!(drag.gesture, Gesture::Move { .. }) && !drag.preview.frames.is_empty();
                if dropping {
                    let to = self.drop_place(event);
                    if let Some(drag) = self.object_drag.as_mut() {
                        drag.to = to;
                    }
                }
                Ok(true)
            }
            PointerAction::Up => {
                let Some(drag) = self.object_drag.take() else { return Ok(false) };
                if let Some((end, _)) = &drag.end {
                    self.session().run("drawing.reconnect", &end.release()).map_err(|e| e.to_string())?;
                    return Ok(true);
                }
                if let Some((create, _)) = &drag.create {
                    self.session().view.draw.shape = None;
                    let (command, mut params, ends) = create.release();
                    if command == "drawing.insert" {
                        params["placement"] = json!({ "kind": "float", "page": drag.page });
                    }
                    let made = self.session().run(command, &params).map_err(|e| e.to_string())?;
                    let id = made.get("id").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
                    for (end, at) in ends {
                        let which = if end == 0 { "start" } else { "end" };
                        self.session()
                            .run("drawing.reconnect", &json!({ "id": id, "end": which, "to": at.id, "site": at.site }))
                            .map_err(|e| e.to_string())?;
                    }
                    // The new object is selected.
                    let found = id.parse::<u32>().ok().and_then(|u| self.session.borrow().doc.find_shape(u));
                    if let Some(pos) = found {
                        self.select_object(&pos);
                    }
                    return Ok(true);
                }
                // An inline picture let go in the text: it moves there, still inline (one step).
                if drag.inline && matches!(drag.gesture, Gesture::Move { .. }) {
                    if drag.preview.frames.is_empty() {
                        return Ok(true);
                    }
                    let to = self.drop_place(event).or(drag.to.clone()).map(|(_, to)| to);
                    let onto_itself = {
                        let session = self.session.borrow();
                        let (a, b) = session.sel.ordered();
                        to.as_ref().is_none_or(|to| *to >= a && *to <= b)
                    };
                    if let Some(to) = to.filter(|_| !onto_itself) {
                        self.session().run("text.move", &json!({ "to": kition_doc_engine::cmd::pos_json(&to) })).map_err(|e| e.to_string())?;
                    }
                    return Ok(true);
                }
                let frames = match drag.gesture.release(&drag.preview) {
                    Some(DrawingOp::Transform(FrameChange::Set(frames))) => frames,
                    Some(DrawingOp::Format { ids, format }) => {
                        self.session().run("drawing.format", &json!({ "ids": ids, "geometry": format.geometry })).map_err(|e| e.to_string())?;
                        return Ok(true);
                    }
                    _ => return Ok(true),
                };
                let Some((_, f)) = frames.first() else { return Ok(true) };
                let moved = drag.gesture.frames().first().is_some_and(|(_, o)| (o.x, o.y) != (f.x, f.y));
                let result = if drag.uid > 0 {
                    self.session().run("drawing.transform", &json!({ "ids": [drag.uid.to_string()], "frame": f }))
                } else if matches!(drag.gesture, Gesture::Rotate { .. }) {
                    // A picture turned by its handle: its angle (one step).
                    self.session().run("arrange.rotation", &json!({ "degrees": f.rot }))
                } else if moved && !drag.inline {
                    self.session().run("arrange.bounds", &json!({ "x": f.x, "y": f.y, "width": f.w, "height": f.h, "page": drag.page }))
                } else {
                    // Resized: an inline picture keeps its place in the text.
                    self.session().run("arrange.bounds", &json!({ "width": f.w, "height": f.h }))
                };
                result.map_err(|e| e.to_string())?;
                Ok(true)
            }
            // A double click on an object keeps it selected (the editor shows its tab, WPS W-W4);
            // it does not select the word under it.
            PointerAction::DoubleClick => match self.layout().object_at(page, px as f32, py as f32, tol as f32) {
                Some(hit) => {
                    self.select_object(&hit.pos());
                    Ok(true)
                }
                None => Ok(false),
            },
            _ => Ok(false),
        }
    }

    /// The selected picture's box on the canvas (around it as turned), its wrapping and its angle,
    /// for the editor's Layout Options button beside it, its tab and its Size dialog: session state.
    pub(crate) fn picture_box(&self, areas: &[PageArea]) -> Option<serde_json::Value> {
        let (hit, _, _) = self.selected_object()?;
        if !self.object_is_picture(&hit.pos()) {
            return None;
        }
        let area = areas.get(hit.page)?;
        let b = frame_of(&hit).bounds();
        Some(
            json!({ "page": hit.page, "x": area.x + b.x0, "y": area.y + b.y0, "width": b.width(), "height": b.height(), "wrap": hit.wrap, "rotation": hit.spin.deg }),
        )
    }

    /// Whether the object at `pos` is a picture.
    fn object_is_picture(&self, pos: &Pos) -> bool {
        let session = self.session.borrow();
        matches!(session.doc.para_at(pos).and_then(|para| para.object_at(pos.off)), Some(InlineObject::Image { .. }))
    }

    /// Where in the text an inline picture dragged to the pointer of `event` would land: the
    /// place under the pointer in the picture's story, on the page under it.
    fn drop_place(&self, event: &ToolPointer) -> Option<(usize, Pos)> {
        let layout = self.layout();
        let areas = Self::areas(&layout);
        let page = Self::page_at(&areas, event.y);
        let area = areas.get(page)?;
        let story = self.session.borrow().sel.focus.story;
        layout.hit(page, (event.x - area.x) as f32, (event.y - area.y) as f32, story).map(|pos| (page, pos))
    }

    /// The pointer over objects, or `None` to leave it to the text: during a gesture, its own; on
    /// a selected object's handle, the arrow along it (the turning arrow on the rotation handle);
    /// over an object, move.
    pub(crate) fn object_cursor(&self, areas: &[PageArea]) -> Option<String> {
        if let Some(drag) = &self.object_drag {
            if drag.end.is_some() || drag.create.is_some() {
                return Some("crosshair".into());
            }
            return Some(match &drag.gesture {
                Gesture::Resize { handle, orig, .. } => resize_cursor(orig, *handle).into(),
                Gesture::Rotate { .. } => ROTATE_CURSOR.into(),
                Gesture::Adjust { .. } => "pointer".into(),
                Gesture::Move { .. } => "move".into(),
            });
        }
        let (x, y, _, zoom) = self.text_tool.at?;
        let page = Self::page_at(areas, y);
        let area = areas.get(page)?;
        let p = Point::new(x - area.x, y - area.y);
        if let Some((hit, _, turns)) = self.selected_object().filter(|(h, _, _)| h.page == page) {
            let frame = frame_of(&hit);
            if let Some(geometry) = self.selected_geometry()
                && hit_adjust(&frame, &geometry, p, REACH_PX / zoom).is_some()
            {
                return Some("pointer".into());
            }
            match handle_at(&frame, p, zoom, turns) {
                Some(HandleHit::Resize(i)) => return Some(resize_cursor(&frame, i).into()),
                Some(HandleHit::Rotate) => return Some(ROTATE_CURSOR.into()),
                None => {}
            }
        }
        self.layout().object_at(page, p.x as f32, p.y as f32, (REACH_PX / zoom) as f32).map(|_| "move".into())
    }

    /// A key for objects; `true` when it was theirs: Escape drops a gesture, arrows nudge a
    /// selected floating object.
    pub(crate) fn object_key(&mut self, key: &str, mods: ToolMods) -> Result<bool, String> {
        if key == "Escape" && (self.object_drag.take().is_some() || self.ink_drag.take().is_some()) {
            self.session().view.draw.shape = None;
            return Ok(true);
        }
        // Escape puts down a shape chosen to draw.
        if key == "Escape" && self.session().view.draw.shape.take().is_some() {
            return Ok(true);
        }
        let Some((hit, _, _)) = self.selected_object() else { return Ok(false) };
        if hit.wrap == Wrap::Inline || mods.shift || mods.command || mods.control {
            return Ok(false);
        }
        let step = if mods.alt { 1.0 } else { 6.0 };
        let (dx, dy) = match key {
            "ArrowLeft" => (-step, 0.0),
            "ArrowRight" => (step, 0.0),
            "ArrowUp" => (0.0, -step),
            "ArrowDown" => (0.0, step),
            _ => return Ok(false),
        };
        self.session().run("arrange.nudge", &json!({ "dx": dx, "dy": dy })).map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// The selected object's frame and handles, or a gesture's preview.
    pub(crate) fn object_overlays(&self, areas: &[PageArea], out: &mut Vec<ToolOverlay>) {
        // A shape being drawn: its outline (a line, or its box) as it would be.
        if let Some(drag) = self.object_drag.as_ref()
            && let Some((create, f)) = drag.create.as_ref()
            && let Some(area) = areas.get(drag.page)
        {
            let at = |q: Point| [area.x + q.x, area.y + q.y];
            let d = if kition_office_core::connect::is_line_preset(&create.preset) {
                let (p0, p1) = kition_office_core::connect::line_ends(f);
                let ([x0, y0], [x1, y1]) = (at(p0), at(p1));
                format!("M{x0:.1},{y0:.1} L{x1:.1},{y1:.1}")
            } else {
                let o = kition_office_core::model::DrawingObject {
                    frame: *f,
                    geometry: kition_office_core::model::Geometry::preset(&create.preset),
                    ..Default::default()
                };
                let path = kition_office_core::edit::outline(&o).outline();
                (kition_office_core::kurbo::Affine::translate((area.x, area.y)) * f.affine() * path).to_svg()
            };
            out.push(ToolOverlay::Path { d, color: ACCENT, width: 1.5, dashed: false });
            return;
        }
        let Some((hit, _, turns)) = self.selected_object() else { return };
        let Some(area) = areas.get(hit.page) else { return };
        let to = |q: Point| [area.x + q.x, area.y + q.y];
        let quad = |f: &Xfrm| {
            let a = f.affine();
            let pts = [(0.0, 0.0), (f.w, 0.0), (f.w, f.h), (0.0, f.h)].map(|(x, y)| to(a * Point::new(x, y)));
            format!(
                "M{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} Z",
                pts[0][0], pts[0][1], pts[1][0], pts[1][1], pts[2][0], pts[2][1], pts[3][0], pts[3][1]
            )
        };
        // A connector's end under way: the line as it would be.
        if let Some((_, f)) = self.object_drag.as_ref().and_then(|d| d.end.as_ref()) {
            let (p0, p1) = kition_office_core::connect::line_ends(f);
            let ([x0, y0], [x1, y1]) = (to(p0), to(p1));
            out.push(ToolOverlay::Path { d: format!("M{x0:.1},{y0:.1} L{x1:.1},{y1:.1}"), color: ACCENT, width: 1.5, dashed: false });
            for [x, y] in [[x0, y0], [x1, y1]] {
                out.push(ToolOverlay::Anchor { x, y, color: ADJUST, filled: true, size: 8.0 });
            }
            return;
        }
        let frame = frame_of(&hit);
        // An adjust handle under way: the shape's new outline.
        if let Some((_, path)) = self.object_drag.as_ref().and_then(|d| d.gesture.preview_outline()) {
            let shifted = kition_office_core::kurbo::Affine::translate((area.x, area.y)) * path;
            out.push(ToolOverlay::Path { d: quad(&frame), color: ACCENT, width: 1.0, dashed: true });
            out.push(ToolOverlay::Path { d: shifted.to_svg(), color: ACCENT, width: 1.5, dashed: false });
            return;
        }
        let preview = self.object_drag.as_ref().and_then(|d| d.preview.frames.first().map(|(_, f)| *f));
        out.push(ToolOverlay::Path { d: quad(&frame), color: ACCENT, width: 1.0, dashed: preview.is_some() });
        if let Some(f) = preview {
            out.push(ToolOverlay::Path { d: quad(&f), color: ACCENT, width: 1.5, dashed: false });
            // An inline picture being moved: a dashed bar where in the text it lands (as dragged
            // text shows, `text_tool.rs`).
            if let Some((page, to)) = self.object_drag.as_ref().and_then(|d| d.to.clone())
                && let Some(caret) = self.layout().caret_on(&to, page)
                && let Some(area) = areas.get(caret.page)
            {
                let x = area.x + f64::from(caret.x);
                let top = area.y + f64::from(caret.top);
                out.push(ToolOverlay::Line { ax: x, ay: top, bx: x, by: top + f64::from(caret.height), color: crate::CARET, dashed: true });
            }
            return;
        }
        let (pts, rot) = box_handles(&frame);
        if turns {
            // The rotation handle on a stem from the top edge's middle.
            let ([x0, y0], [x1, y1]) = (to(pts[1]), to(rot));
            out.push(ToolOverlay::Path { d: format!("M{x0:.1},{y0:.1} L{x1:.1},{y1:.1}"), color: ACCENT, width: 1.0, dashed: false });
            out.push(ToolOverlay::Anchor { x: x1, y: y1, color: ACCENT, filled: false, size: HANDLE_PX });
        }
        for q in pts {
            let [x, y] = to(q);
            out.push(ToolOverlay::Anchor { x, y, color: ACCENT, filled: false, size: HANDLE_PX });
        }
        // Objects added with Shift: their frames.
        let also: Vec<Pos> = self.session.borrow().also_selected.clone();
        for p in also {
            if let Some(h) = self.layout().object(&p, hit.page).filter(|h| h.page == hit.page) {
                out.push(ToolOverlay::Path { d: quad(&frame_of(&h)), color: ACCENT, width: 1.0, dashed: false });
            }
        }
        // A connector's ends, which drag onto other shapes' sites.
        if self.selected_is_connector() {
            let (p0, p1) = kition_office_core::connect::line_ends(&frame);
            for q in [p0, p1] {
                let [x, y] = to(q);
                out.push(ToolOverlay::Anchor { x, y, color: ADJUST, filled: false, size: 9.0 });
            }
        }
        if let Some(geometry) = self.selected_geometry() {
            for (_, q) in adjust_handles(&frame, &geometry) {
                let [x, y] = to(q);
                out.push(ToolOverlay::Anchor { x, y, color: ADJUST, filled: true, size: 8.0 });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;
    use kition_kit::tools::{KitTools, PointerAction, ToolMods, ToolPointer};
    use kition_office_core::model::DrawingObject;
    use serde_json::json;

    use crate::DocDocument;

    fn shape(doc: &mut DocDocument, id: &str) -> DrawingObject {
        let d = doc.execute("drawing.describe", json!({ "ids": [id] })).unwrap();
        serde_json::from_value(d["objects"][0].clone()).unwrap()
    }

    /// The canvas point of page point (`x`, `y`) on page 0.
    fn on_page(doc: &DocDocument, x: f64, y: f64) -> (f64, f64) {
        let areas = DocDocument::areas(&doc.layout());
        (areas[0].x + x, areas[0].y + y)
    }

    fn ev(action: PointerAction, (x, y): (f64, f64)) -> ToolPointer {
        ToolPointer { action, x, y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 }
    }

    #[test]
    fn a_pen_draws_a_stroke_and_the_eraser_takes_it_back_with_the_pointer() {
        let mut doc = DocDocument::create("Doc.docx", 0, 0).unwrap();
        doc.execute("file.setText", json!({ "text": "A line of text to draw over.\nAnother line." })).unwrap();
        let inks = |doc: &mut DocDocument| -> Vec<DrawingObject> {
            let d = doc.execute("drawing.describe", json!({})).unwrap();
            serde_json::from_value::<Vec<DrawingObject>>(d["objects"].clone())
                .unwrap()
                .into_iter()
                .filter(|o| o.kind == kition_office_core::model::ObjectKind::Ink)
                .collect()
        };
        doc.execute("ink.pen", json!({ "color": "C00000", "width": 3 })).unwrap();
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 120.0, 140.0))).unwrap();
        for i in 1..=10 {
            let r = doc.pointer(&ev(PointerAction::Drag, on_page(&doc, 120.0 + f64::from(i) * 10.0, 140.0))).unwrap();
            assert!(!r.overlays.is_empty(), "the stroke shows while it is drawn");
        }
        assert!(inks(&mut doc).is_empty(), "nothing is added before the pointer lifts");
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 220.0, 140.0))).unwrap();
        let drawn = inks(&mut doc);
        assert_eq!(drawn.len(), 1, "one stroke on release");
        let st = &drawn[0].ink.as_ref().unwrap()[0];
        assert_eq!(st.width, 3.0);
        assert!((st.points[0][0] - 120.0).abs() < 1.0 && (st.points.last().unwrap()[0] - 220.0).abs() < 1.0, "{:?}", st.points);
        // Escape drops a stroke being drawn.
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 120.0, 200.0))).unwrap();
        doc.pointer(&ev(PointerAction::Drag, on_page(&doc, 200.0, 200.0))).unwrap();
        doc.key("Escape", ToolMods::default(), 1.0).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 200.0, 200.0))).unwrap();
        assert_eq!(inks(&mut doc).len(), 1);
        // The eraser's drag across the stroke takes it, in one step.
        doc.execute("ink.eraser", json!({})).unwrap();
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 170.0, 120.0))).unwrap();
        doc.pointer(&ev(PointerAction::Drag, on_page(&doc, 170.0, 160.0))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 170.0, 160.0))).unwrap();
        assert!(inks(&mut doc).is_empty());
        doc.execute("edit.undo", json!({})).unwrap();
        assert_eq!(inks(&mut doc).len(), 1, "undo brings it back");
        // Back to selecting: the pointer selects again instead of drawing.
        doc.execute("ink.select", json!({})).unwrap();
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 120.0, 260.0))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 160.0, 260.0))).unwrap();
        assert_eq!(inks(&mut doc).len(), 1);
    }

    #[test]
    fn a_shape_is_selected_moved_resized_and_turned_with_the_pointer() {
        let mut doc = DocDocument::create("Doc.docx", 0, 0).unwrap();
        doc.execute("text.insert", json!({ "text": "Text under the shape. " })).unwrap();
        let id = doc
            .execute("drawing.insert", json!({ "geometry": { "preset": "ellipse" }, "frame": { "x": 200, "y": 300, "w": 120, "h": 60 } }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        doc.execute("select.collapse", json!({ "end": true })).unwrap();
        // Press on it: it is selected, with handles.
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 260.0, 330.0))).unwrap();
        let r = doc.pointer(&ev(PointerAction::Drag, on_page(&doc, 280.0, 340.0))).unwrap();
        assert!(r.overlays.len() >= 2);
        assert_eq!(shape(&mut doc, &id).frame.x, 200.0, "the preview does not move it");
        doc.pointer(&ev(PointerAction::Drag, on_page(&doc, 300.0, 350.0))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 300.0, 350.0))).unwrap();
        let f = shape(&mut doc, &id).frame;
        assert!((f.x - 240.0).abs() < 0.5 && (f.y - 320.0).abs() < 0.5, "{f:?}");
        assert!(doc.selected_object().is_some(), "still selected");
        // The bottom-right handle resizes it.
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, f.x + f.w, f.y + f.h))).unwrap();
        doc.pointer(&ev(PointerAction::Drag, on_page(&doc, f.x + f.w + 30.0, f.y + f.h + 20.0))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, f.x + f.w + 30.0, f.y + f.h + 20.0))).unwrap();
        let g = shape(&mut doc, &id).frame;
        assert!((g.w - 150.0).abs() < 0.5 && (g.h - 80.0).abs() < 0.5, "{g:?}");
        // The rotation handle turns it a quarter.
        let (cx, cy) = (g.x + g.w / 2.0, g.y + g.h / 2.0);
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, cx, g.y - 20.0))).unwrap();
        doc.pointer(&ev(PointerAction::Drag, on_page(&doc, cx + 100.0, cy))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, cx + 100.0, cy))).unwrap();
        assert!((shape(&mut doc, &id).frame.rot - 90.0).abs() < 0.5);
        // A drag cancelled with Escape changes nothing; arrows nudge it.
        let before = shape(&mut doc, &id).frame;
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, cx, cy))).unwrap();
        doc.pointer(&ev(PointerAction::Drag, on_page(&doc, cx + 50.0, cy))).unwrap();
        doc.key("Escape", ToolMods::default(), 1.0).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, cx + 50.0, cy))).unwrap();
        assert_eq!(shape(&mut doc, &id).frame, before);
        doc.key("ArrowRight", ToolMods::default(), 1.0).unwrap();
        assert!((shape(&mut doc, &id).frame.x - before.x - 6.0).abs() < 0.5);
        // Undo takes back the nudge, then the turn.
        doc.execute("edit.undo", json!({})).unwrap();
        assert_eq!(shape(&mut doc, &id).frame.x, before.x);
        // Saving and reopening keeps it, and it moves again.
        let bytes = doc.save(Some("Doc.docx")).unwrap();
        let mut back = DocDocument::open("Doc.docx", &bytes).unwrap();
        let o = shape(&mut back, &id);
        assert!((o.frame.rot - 90.0).abs() < 0.5 && o.geometry == kition_office_core::model::Geometry::preset("ellipse"));
    }

    #[test]
    fn a_shape_chosen_in_insert_shapes_is_drawn_by_a_drag_and_a_line_between_shapes_glues() {
        let mut doc = DocDocument::create("Doc.docx", 0, 0).unwrap();
        doc.execute("text.insert", json!({ "text": "Text under the shapes. " })).unwrap();
        let count = |doc: &mut DocDocument| doc.session.borrow().doc.shapes().len();
        // An ellipse drawn from (300, 200) to (400, 260).
        doc.execute("ink.shape", json!({ "preset": "ellipse" })).unwrap();
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 300.0, 200.0))).unwrap();
        let r = doc.pointer(&ev(PointerAction::Drag, on_page(&doc, 400.0, 260.0))).unwrap();
        assert!(r.overlays.iter().any(|o| matches!(o, kition_kit::tools::ToolOverlay::Path { width, .. } if *width == 1.5)), "the outline previews");
        assert_eq!(count(&mut doc), 0, "nothing is made before the release");
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 400.0, 260.0))).unwrap();
        let d = doc.execute("drawing.describe", json!({})).unwrap();
        let e: DrawingObject = serde_json::from_value(d["objects"][0].clone()).unwrap();
        assert_eq!(e.geometry, kition_office_core::model::Geometry::preset("ellipse"));
        let f = e.frame;
        assert!((f.x - 300.0).abs() < 1.0 && (f.y - 200.0).abs() < 1.0 && (f.w - 100.0).abs() < 1.0 && (f.h - 60.0).abs() < 1.0, "{f:?}");
        assert_eq!(doc.session.borrow().view.draw.shape, None, "drawn once, the shape is put down");
        // A rectangle, then a line drawn from the ellipse's right site to it: one connector.
        let id = |d: &serde_json::Value| d["id"].as_str().unwrap().to_string();
        let ellipse = e.id.clone();
        let rect = id(&doc
            .execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "x": 300, "y": 400, "w": 100, "h": 50 } }))
            .unwrap());
        doc.execute("select.collapse", json!({ "end": true })).unwrap();
        let from = kition_office_core::connect::sites(&shape(&mut doc, &ellipse)).into_iter().max_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
        let to = kition_office_core::connect::sites(&shape(&mut doc, &rect)).into_iter().min_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
        let before = count(&mut doc);
        doc.execute("ink.shape", json!({ "preset": "line" })).unwrap();
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, from.x + 2.0, from.y))).unwrap();
        doc.pointer(&ev(PointerAction::Drag, on_page(&doc, to.x, to.y - 3.0))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, to.x, to.y - 3.0))).unwrap();
        let d = doc.execute("drawing.describe", json!({})).unwrap();
        let line: DrawingObject = serde_json::from_value(d["objects"][0].clone()).unwrap();
        let conn = line.connection.clone().expect("glued");
        assert_eq!((conn.start.unwrap().id, conn.end.unwrap().id), (ellipse, rect));
        assert_eq!(count(&mut doc), before + 1);
        // One step: undo takes the connector away.
        doc.execute("edit.undo", json!({})).unwrap();
        assert_eq!(count(&mut doc), before);
        // Escape puts down a chosen shape.
        doc.execute("ink.shape", json!({ "preset": "rect" })).unwrap();
        doc.key("Escape", ToolMods::default(), 1.0).unwrap();
        assert_eq!(doc.session.borrow().view.draw.shape, None);
        assert!(doc.execute("ink.shape", json!({ "preset": "notAShape" })).is_err());
    }

    #[test]
    fn a_connectors_end_drags_onto_another_shape_in_one_step() {
        let mut doc = DocDocument::create("Doc.docx", 0, 0).unwrap();
        doc.execute("text.insert", json!({ "text": "Text under the shapes. " })).unwrap();
        let ins = |doc: &mut DocDocument, x: f64, y: f64| -> String {
            let id = doc
                .execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "x": x, "y": y, "w": 100, "h": 50 } }))
                .unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
            doc.execute("select.collapse", json!({ "end": true })).unwrap();
            id
        };
        let (a, b, c) = (ins(&mut doc, 100.0, 300.0), ins(&mut doc, 400.0, 450.0), ins(&mut doc, 100.0, 600.0));
        let k = doc.execute("drawing.connect", json!({ "from": a, "to": b })).unwrap()["id"].as_str().unwrap().to_string();
        let o = shape(&mut doc, &k);
        let (p0, p1) = kition_office_core::connect::line_ends(&o.frame);
        // Select the connector inside its box, away from the shapes; its ends show.
        let mid = p0.midpoint(p1);
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, mid.x, mid.y))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, mid.x, mid.y))).unwrap();
        assert_eq!(doc.execute("drawing.describe", json!({})).unwrap()["objects"][0]["id"], k.as_str());
        // Drag its end from b onto c's top site.
        let target = kition_office_core::connect::sites(&shape(&mut doc, &c)).into_iter().min_by(|x, y| x.y.total_cmp(&y.y)).unwrap();
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, p1.x, p1.y))).unwrap();
        doc.pointer(&ev(PointerAction::Drag, on_page(&doc, target.x + 3.0, target.y - 2.0))).unwrap();
        assert_eq!(shape(&mut doc, &k).frame, o.frame, "the drag previews only");
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, target.x + 3.0, target.y - 2.0))).unwrap();
        let now = shape(&mut doc, &k);
        let conn = now.connection.clone().unwrap();
        assert_eq!((conn.start.unwrap().id, conn.end.unwrap().id), (a.clone(), c.clone()));
        let (q0, q1) = kition_office_core::connect::line_ends(&now.frame);
        assert!((q0 - p0).hypot() < 1.5 && (q1 - target).hypot() < 1.5, "{q0:?} {q1:?} {target:?}");
        // One step back to b.
        doc.execute("edit.undo", json!({})).unwrap();
        assert_eq!(shape(&mut doc, &k).connection.unwrap().end.unwrap().id, b);
    }

    #[test]
    fn a_shapes_adjust_handle_reshapes_it_in_one_step() {
        let mut doc = DocDocument::create("Doc.docx", 0, 0).unwrap();
        doc.execute("text.insert", json!({ "text": "Text under the shape. " })).unwrap();
        let id = doc
            .execute("drawing.insert", json!({ "geometry": { "preset": "roundRect" }, "frame": { "x": 200, "y": 300, "w": 200, "h": 100 } }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        doc.execute("select.collapse", json!({ "end": true })).unwrap();
        // Select it: its yellow handle shows on the top edge, a sixth of the short side in.
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 300.0, 350.0))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 300.0, 350.0))).unwrap();
        let o = shape(&mut doc, &id);
        let handles = kition_office_core::edit::adjust_handles(&o.frame, &o.geometry);
        let (_, at) = handles[0];
        assert!((at.x - 216.667).abs() < 0.1 && (at.y - 300.0).abs() < 0.1, "{at:?}");
        let r = doc.pointer(&ev(PointerAction::Move, on_page(&doc, 500.0, 500.0))).unwrap();
        assert!(
            r.overlays.iter().any(|o| matches!(o, kition_kit::tools::ToolOverlay::Anchor { color, .. } if *color == super::ADJUST)),
            "the adjust handle is drawn"
        );
        // Dragging it 25 pt in rounds the corners more; the preview changes nothing.
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, at.x, at.y))).unwrap();
        let r = doc.pointer(&ev(PointerAction::Drag, on_page(&doc, at.x + 25.0, at.y))).unwrap();
        assert!(
            r.overlays.iter().any(|o| matches!(o, kition_kit::tools::ToolOverlay::Path { width, .. } if *width == 1.5)),
            "the new outline previews"
        );
        assert_eq!(shape(&mut doc, &id).geometry, o.geometry);
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, at.x + 25.0, at.y))).unwrap();
        let after = shape(&mut doc, &id);
        let v = after.geometry.adjust_values()[0];
        assert!((v - 41_667.0).abs() <= 1.0, "{:?}", after.geometry);
        assert_eq!(after.frame, o.frame, "the frame stays");
        // One undo step; Escape drops a drag.
        doc.execute("edit.undo", json!({})).unwrap();
        assert_eq!(shape(&mut doc, &id).geometry.adjust_values().first().copied().unwrap_or(16_667.0), 16_667.0);
        doc.execute("edit.redo", json!({})).unwrap();
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 200.0 + 41.667, 300.0))).unwrap();
        doc.pointer(&ev(PointerAction::Drag, on_page(&doc, 200.0 + 80.0, 300.0))).unwrap();
        doc.key("Escape", ToolMods::default(), 1.0).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 200.0 + 80.0, 300.0))).unwrap();
        assert!((shape(&mut doc, &id).geometry.adjust_values()[0] - 41_667.0).abs() <= 1.0);
        // It is saved and reads back.
        let bytes = doc.save(Some("Doc.docx")).unwrap();
        let mut back = DocDocument::open("Doc.docx", &bytes).unwrap();
        assert!((shape(&mut back, &id).geometry.adjust_values()[0] - 41_667.0).abs() <= 1.0);
    }

    #[test]
    fn shift_adds_an_object_and_the_two_group() {
        let mut doc = DocDocument::create("Doc.docx", 0, 0).unwrap();
        doc.execute("text.insert", json!({ "text": "Text under the shapes. " })).unwrap();
        let a = doc
            .execute("drawing.insert", json!({ "geometry": { "preset": "ellipse" }, "frame": { "x": 200, "y": 300, "w": 120, "h": 60 } }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        doc.execute("select.collapse", json!({ "end": true })).unwrap();
        let b = doc.execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "x": 60, "y": 60, "w": 80, "h": 50 } })).unwrap()
            ["id"]
            .as_str()
            .unwrap()
            .to_string();
        doc.execute("select.collapse", json!({ "end": true })).unwrap();
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 100.0, 85.0))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 100.0, 85.0))).unwrap();
        let shift = ToolMods { shift: true, ..ToolMods::default() };
        let mut down = ev(PointerAction::Down, on_page(&doc, 260.0, 330.0));
        down.mods = shift;
        doc.pointer(&down).unwrap();
        let mut up = ev(PointerAction::Up, on_page(&doc, 260.0, 330.0));
        up.mods = shift;
        doc.pointer(&up).unwrap();
        let state = doc.inspect().unwrap();
        assert_eq!(state["shapes"], json!([b, a]), "{state}");
        let ids = state["shapes"].clone();
        let g = doc.execute("drawing.group", json!({ "ids": ids })).unwrap();
        assert!(g["id"].as_str().is_some_and(|id| !id.is_empty() && id != "0"), "{g}");
        let st = doc.inspect().unwrap();
        assert_eq!(st["shape"], g["id"]);
        doc.pointer(&ev(PointerAction::Move, on_page(&doc, 500.0, 500.0))).unwrap();
    }

    /// A document of three paragraphs with a 160 × 100 pt picture inline in the second (after
    /// "Before "), the caret at the first's start.
    fn with_picture() -> DocDocument {
        let mut doc = DocDocument::create("Doc.docx", 0, 0).unwrap();
        doc.execute("file.setText", json!({ "text": "The first paragraph of text.\nBefore \nThe third paragraph, where it lands." })).unwrap();
        let picture = image::RgbaImage::from_pixel(16, 10, image::Rgba([30, 150, 70, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        picture.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes.into_inner());
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 7) })).unwrap();
        doc.execute("image.insert", json!({ "data": data })).unwrap();
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        doc.execute("image.size", json!({ "width": 160, "height": 100, "lockAspect": false })).unwrap();
        doc.execute("caret.docStart", json!({})).unwrap();
        doc
    }

    fn at(block: u32, off: usize) -> serde_json::Value {
        json!({ "story": "body", "path": [block], "off": off })
    }

    /// The picture's frame on the canvas (x, y, w, h) and its wrapping, from `inspect()` with it selected.
    fn picture(doc: &mut DocDocument) -> (f64, f64, f64, f64, String) {
        let p = doc.inspect().unwrap()["picture"].clone();
        (
            p["x"].as_f64().unwrap(),
            p["y"].as_f64().unwrap(),
            p["width"].as_f64().unwrap(),
            p["height"].as_f64().unwrap(),
            p["wrap"].as_str().unwrap().to_string(),
        )
    }

    /// The picture's size in the document (first object of paragraph `block`).
    fn size_of(doc: &mut DocDocument, block: u32) -> Option<(f64, f64)> {
        let p = doc.execute("file.paragraph", json!({ "path": [block] })).unwrap();
        let o = p["paragraph"]["objects"].get(0)?.clone();
        Some((o["w"].as_f64()?, o["h"].as_f64()?))
    }

    fn with(mut e: ToolPointer, mods: ToolMods) -> ToolPointer {
        e.mods = mods;
        e
    }

    fn drag(doc: &mut DocDocument, from: (f64, f64), to: (f64, f64), mods: ToolMods) -> kition_kit::tools::ToolResponse {
        doc.pointer(&with(ev(PointerAction::Down, from), mods)).unwrap();
        let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        doc.pointer(&with(ev(PointerAction::Drag, mid), mods)).unwrap();
        let r = doc.pointer(&with(ev(PointerAction::Drag, to), mods)).unwrap();
        doc.pointer(&with(ev(PointerAction::Up, to), mods)).unwrap();
        r
    }

    fn click(doc: &mut DocDocument, p: (f64, f64)) {
        doc.pointer(&ev(PointerAction::Down, p)).unwrap();
        doc.pointer(&ev(PointerAction::Up, p)).unwrap();
    }

    #[test]
    fn a_selected_picture_shows_eight_ringed_handles_and_a_rotation_handle_and_no_tint() {
        use kition_kit::tools::ToolOverlay;
        let mut doc = with_picture();
        assert!(doc.inspect().unwrap()["picture"].is_null(), "no picture selected yet");
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        let (x, y, w, h, wrap) = picture(&mut doc);
        assert_eq!(wrap, "inline");
        assert!((w - 160.0).abs() < 0.5 && (h - 100.0).abs() < 0.5, "{w} {h}");
        doc.execute("caret.docStart", json!({})).unwrap();
        click(&mut doc, (x + w / 2.0, y + h / 2.0));
        assert!(doc.selected_object().is_some(), "a click selects it");
        let r = doc.state(0.0, 0.0, ToolMods::default(), 1.0);
        let rings: Vec<_> = r
            .overlays
            .iter()
            .filter_map(|o| match o {
                ToolOverlay::Anchor { x, y, color, filled: false, size } if *color == super::ACCENT && *size == super::HANDLE_PX => Some((*x, *y)),
                _ => None,
            })
            .collect();
        assert_eq!(rings.len(), 9, "eight handles and the rotation handle: {:?}", r.overlays);
        assert!(rings.contains(&(x + w, y + h)) && rings.contains(&(x, y)), "{rings:?}");
        assert!(rings.iter().any(|(rx, ry)| (*rx - (x + w / 2.0)).abs() < 0.01 && *ry < y - 10.0), "the rotation handle is above: {rings:?}");
        assert!(!r.overlays.iter().any(|o| matches!(o, ToolOverlay::Highlight { .. })), "no selection tint over it");
        assert!(!r.overlays.iter().any(|o| matches!(o, ToolOverlay::Anchor { color: [255, 255, 255, 255], .. })), "no white-on-white handles");
    }

    #[test]
    fn the_pointer_shows_what_a_press_on_a_picture_does() {
        let mut doc = with_picture();
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        let (x, y, w, h, _) = picture(&mut doc);
        let cursor = |doc: &mut DocDocument, p: (f64, f64)| doc.pointer(&ev(PointerAction::Move, p)).unwrap().cursor;
        assert_eq!(cursor(&mut doc, (x + w, y + h)), "nwse-resize");
        assert_eq!(cursor(&mut doc, (x + w + 4.0, y + h + 4.0)), "nwse-resize", "near the handle");
        assert_eq!(cursor(&mut doc, (x, y)), "nwse-resize");
        assert_eq!(cursor(&mut doc, (x + w, y)), "nesw-resize");
        assert_eq!(cursor(&mut doc, (x + w, y + h / 2.0)), "ew-resize");
        assert_eq!(cursor(&mut doc, (x + w / 2.0, y + h)), "ns-resize");
        assert_eq!(cursor(&mut doc, (x + w / 2.0, y + h / 2.0)), "move");
        assert!(cursor(&mut doc, (x + w / 2.0, y - super::kition_office_core_offset())).contains("svg"), "the turning arrow on the rotation handle");
        // Turned a quarter, the corner's arrow turns with it.
        doc.execute("arrange.rotation", json!({ "degrees": 90 })).unwrap();
        let (hit, _, _) = doc.selected_object().unwrap();
        let (pts, _) = kition_office_core::edit::box_handles(&super::frame_of(&hit));
        let area = DocDocument::areas(&doc.layout())[hit.page];
        assert_eq!(cursor(&mut doc, (area.x + pts[2].x, area.y + pts[2].y)), "nwse-resize", "NE turned a quarter points SE");
        assert_eq!(cursor(&mut doc, (area.x + pts[3].x, area.y + pts[3].y)), "ns-resize");
        // A picture not selected: move over it; the text beside it the I-beam.
        doc.execute("caret.docStart", json!({})).unwrap();
        let (hx, hy) = (area.x + f64::from(hit.rect.x + hit.rect.w / 2.0), area.y + f64::from(hit.rect.y + hit.rect.h / 2.0));
        assert_eq!(cursor(&mut doc, (hx, hy)), "move");
        let text = on_page(&doc, 100.0, 80.0);
        assert_eq!(cursor(&mut doc, text), "text");
    }

    #[test]
    fn a_handle_is_taken_seven_pixels_from_its_centre_at_any_zoom() {
        for zoom in [1.0, 2.0] {
            let mut doc = with_picture();
            doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
            let (x, y, w, h, _) = picture(&mut doc);
            // 6 screen pixels out from the corner, along its diagonal.
            let d = 6.0 / 2f64.sqrt() / zoom;
            let mut down = ev(PointerAction::Down, (x + w + d, y + h + d));
            down.zoom = zoom;
            doc.pointer(&down).unwrap();
            let mut up = ev(PointerAction::Up, (x + w + d + 30.0, y + h + d + 30.0));
            up.zoom = zoom;
            let mut mv = ev(PointerAction::Drag, (x + w + d + 30.0, y + h + d + 30.0));
            mv.zoom = zoom;
            doc.pointer(&mv).unwrap();
            doc.pointer(&up).unwrap();
            let (nw, nh) = size_of(&mut doc, 1).unwrap();
            assert!(nw > 170.0 && nh > 105.0, "zoom {zoom}: the corner was taken: {nw} {nh}");
            assert!(doc.selected_object().is_some(), "still selected");
            // 9 pixels out takes nothing of the picture: the press goes to the text.
            let (x, y, w, h, _) = picture(&mut doc);
            let far = 9.0 / zoom;
            let mut down = ev(PointerAction::Down, (x + w + far, y + h + far));
            down.zoom = zoom;
            doc.pointer(&down).unwrap();
            let mut up = ev(PointerAction::Up, (x + w + far, y + h + far));
            up.zoom = zoom;
            doc.pointer(&up).unwrap();
            assert_eq!(size_of(&mut doc, 1).unwrap(), (nw, nh), "zoom {zoom}");
            assert!(doc.selected_object().is_none(), "zoom {zoom}: a press off it lets it go");
        }
    }

    #[test]
    fn a_pictures_corner_keeps_its_ratio_unless_shift_and_cmd_sizes_about_its_centre() {
        let shift = ToolMods { shift: true, ..ToolMods::default() };
        let cmd = ToolMods { command: true, ..ToolMods::default() };
        let mut doc = with_picture();
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        let (x, y, w, h, _) = picture(&mut doc);
        drag(&mut doc, (x + w, y + h), (x + w + 60.0, y + h + 40.0), ToolMods::default());
        let (nw, nh) = size_of(&mut doc, 1).unwrap();
        assert!((nw / nh - 1.6).abs() < 0.01 && nw > 200.0, "the ratio stays: {nw} × {nh}");
        assert_eq!(picture(&mut doc).4, "inline", "resizing leaves it inline");
        doc.execute("edit.undo", json!({})).unwrap();
        assert_eq!(size_of(&mut doc, 1).unwrap(), (160.0, 100.0), "one undo step");
        // Shift lets the ratio go.
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        drag(&mut doc, (x + w, y + h), (x + w + 60.0, y + h + 10.0), shift);
        let (nw, nh) = size_of(&mut doc, 1).unwrap();
        assert!((nw - 220.0).abs() < 0.5 && (nh - 110.0).abs() < 0.5, "free: {nw} × {nh}");
        doc.execute("edit.undo", json!({})).unwrap();
        // An edge's middle changes one way only.
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        drag(&mut doc, (x + w, y + h / 2.0), (x + w + 40.0, y + h / 2.0 + 25.0), ToolMods::default());
        assert_eq!(size_of(&mut doc, 1).unwrap(), (200.0, 100.0));
        doc.execute("edit.undo", json!({})).unwrap();
        // Cmd: about the centre, both sides grow, the ratio kept.
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        drag(&mut doc, (x + w, y + h), (x + w + 20.0, y + h + 20.0), cmd);
        let (nw, nh) = size_of(&mut doc, 1).unwrap();
        // Both sides of each way grow by the drag (100 + 2 × 20 high), the ratio kept: 224 × 140.
        assert!((nw - 224.0).abs() < 0.5 && (nh - 140.0).abs() < 0.5, "about the centre: {nw} × {nh}");
        assert_eq!(picture(&mut doc).4, "inline");
    }

    #[test]
    fn an_inline_picture_dragged_into_other_text_stays_inline_in_one_step() {
        use kition_kit::tools::ToolOverlay;
        let mut doc = with_picture();
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        let (x, y, w, h, _) = picture(&mut doc);
        // Where "where" of the third paragraph is: its caret.
        doc.execute("select.range", json!({ "anchor": at(2, 21), "focus": at(2, 21) })).unwrap();
        let c = doc.inspect().unwrap()["caret"].clone();
        let target = (c["x"].as_f64().unwrap() + 1.0, c["top"].as_f64().unwrap() + c["height"].as_f64().unwrap() / 2.0);
        doc.execute("caret.docStart", json!({})).unwrap();
        let from = (x + w / 2.0, y + h / 2.0);
        doc.pointer(&ev(PointerAction::Down, from)).unwrap();
        doc.pointer(&ev(PointerAction::Drag, (from.0 + 30.0, from.1 + 30.0))).unwrap();
        let r = doc.pointer(&ev(PointerAction::Drag, target)).unwrap();
        let bar = r.overlays.iter().find_map(|o| match o {
            ToolOverlay::Line { ax, dashed: true, .. } => Some(*ax),
            _ => None,
        });
        assert!(bar.is_some_and(|ax| (ax - c["x"].as_f64().unwrap()).abs() < 1.0), "the dashed bar where it lands: {:?}", r.overlays);
        assert_eq!(r.cursor, "move");
        assert!(size_of(&mut doc, 1).is_some(), "nothing moves before the release");
        doc.pointer(&ev(PointerAction::Up, target)).unwrap();
        assert!(size_of(&mut doc, 1).is_none(), "it left its paragraph");
        let text = doc.execute("file.paragraph", json!({ "path": [2] })).unwrap()["paragraph"]["text"].as_str().unwrap().to_string();
        assert_eq!(text, "The third paragraph, \u{FFFC}where it lands.");
        assert_eq!(picture(&mut doc).4, "inline", "still inline, and still selected");
        doc.execute("edit.undo", json!({})).unwrap();
        assert!(size_of(&mut doc, 1).is_some() && size_of(&mut doc, 2).is_none(), "one undo step");
        // Dropped on itself it stays.
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        drag(&mut doc, from, (from.0 + 10.0, from.1), ToolMods::default());
        assert!(size_of(&mut doc, 1).is_some());
        // A floating picture is dragged freely.
        doc.execute("arrange.wrap", json!({ "wrap": "square" })).unwrap();
        let (fx, fy, fw, fh, _) = picture(&mut doc);
        drag(&mut doc, (fx + fw / 2.0, fy + fh / 2.0), (fx + fw / 2.0 + 40.0, fy + fh / 2.0 + 20.0), ToolMods::default());
        let (gx, gy, _, _, wrap) = picture(&mut doc);
        assert_eq!(wrap, "square");
        assert!((gx - fx - 40.0).abs() < 1.0 && (gy - fy - 20.0).abs() < 1.0, "{gx},{gy} from {fx},{fy}");
    }

    #[test]
    fn a_pictures_rotation_handle_turns_it_in_one_step() {
        let mut doc = with_picture();
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        let (x, y, w, h, _) = picture(&mut doc);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        drag(&mut doc, (cx, y - super::kition_office_core_offset()), (cx + 200.0, cy), ToolMods::default());
        let rot = doc.execute("file.paragraph", json!({ "path": [1] })).unwrap()["paragraph"]["objects"][0]["float"]["rot"].as_f64().unwrap_or(0.0);
        assert!((rot - 90.0).abs() < 0.5, "{rot}");
        doc.execute("edit.undo", json!({})).unwrap();
        let rot = doc.execute("file.paragraph", json!({ "path": [1] })).unwrap()["paragraph"]["objects"][0]["float"]["rot"].as_f64().unwrap_or(0.0);
        assert_eq!(rot, 0.0);
    }

    #[test]
    fn backspace_after_a_picture_selects_it_first_and_typing_on_it_goes_before_it() {
        let mut doc = with_picture();
        let text =
            |doc: &mut DocDocument| doc.execute("file.paragraph", json!({ "path": [1] })).unwrap()["paragraph"]["text"].as_str().unwrap().to_string();
        doc.execute("select.range", json!({ "anchor": at(1, 10), "focus": at(1, 10) })).unwrap();
        doc.key("Backspace", ToolMods::default(), 1.0).unwrap();
        assert!(doc.selected_object().is_some(), "the first Backspace selects the picture");
        assert_eq!(text(&mut doc), "Before \u{FFFC}");
        doc.key("Backspace", ToolMods::default(), 1.0).unwrap();
        assert_eq!(text(&mut doc), "Before ", "the second deletes it");
        doc.execute("edit.undo", json!({})).unwrap();
        assert_eq!(text(&mut doc), "Before \u{FFFC}", "the selecting was no step");
        // Typed on the selected picture: the letters go before it, and it stays.
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        KitTools::text(&mut doc, "ab", 1.0).unwrap();
        assert_eq!(text(&mut doc), "Before ab\u{FFFC}");
        // An agent's text.backspace still deletes it at once.
        doc.execute("select.range", json!({ "anchor": at(1, 12), "focus": at(1, 12) })).unwrap();
        doc.execute("text.backspace", json!({})).unwrap();
        assert_eq!(text(&mut doc), "Before ab");
    }

    #[test]
    fn a_double_click_on_a_picture_keeps_it_selected() {
        let mut doc = with_picture();
        doc.execute("select.range", json!({ "anchor": at(1, 7), "focus": at(1, 10) })).unwrap();
        let (x, y, w, h, _) = picture(&mut doc);
        doc.execute("caret.docStart", json!({})).unwrap();
        let p = (x + w / 2.0, y + h / 2.0);
        click(&mut doc, p);
        doc.pointer(&ev(PointerAction::DoubleClick, p)).unwrap();
        assert!(doc.selected_object().is_some());
        assert!(doc.inspect().unwrap()["picture"].is_object());
    }

    #[test]
    fn a_press_beside_objects_places_the_caret_as_before() {
        let mut doc = DocDocument::create("Doc.docx", 0, 0).unwrap();
        doc.execute("text.insert", json!({ "text": "Plain text" })).unwrap();
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 80.0, 80.0))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 80.0, 80.0))).unwrap();
        assert!(doc.selected_object().is_none());
    }
}
