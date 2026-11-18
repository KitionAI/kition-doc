//! Pictures, shapes and text boxes under the text tool: a press on one selects it (layout's own
//! hit test, so a text box is grabbed by its border and typed into inside), a drag moves it, and
//! its handles resize and turn it, and a shape's adjust handles (the yellow dots) reshape it,
//! through the gestures Kition's kits share (`kition_office_core::edit`). The preview is an overlay;
//! the release runs one command (one undo step): `drawing.transform` for a shape,
//! `drawing.format` for an adjust handle, `arrange.bounds` for a picture. Escape drops it.

use kition_doc_doc::Pos;
use kition_doc_doc::para::{InlineObject, OBJ, Wrap};
use kition_doc_engine::cmd::draw::DrawMode;
use kition_doc_layout::hit::ObjectHit;
use kition_kit::tools::{PointerAction, ToolMods, ToolOverlay, ToolPointer};
use kition_office_core::edit::{Gesture, HandleHit, Mods, Preview, adjust_handles, box_handles, hit_adjust, hit_handle};
use kition_office_core::ink::{EraserGesture, InkGesture, InkTool};
use kition_office_core::model::Geometry;
use kition_office_core::ops::{DrawingOp, FrameChange};
use kition_office_core::{Point, Xfrm};
use serde_json::json;

use crate::{DocDocument, PageArea};

/// Handle hit radius in screen pixels.
const REACH_PX: f64 = 4.0;
/// Frames and handles take the kit's brand.
const ACCENT: [u8; 4] = [crate::BRAND[0], crate::BRAND[1], crate::BRAND[2], 255];
const HANDLE: [u8; 4] = [255, 255, 255, 255];
/// Adjust handles are yellow, as in Office.
const ADJUST: [u8; 4] = [255, 214, 0, 255];

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
        // A shape turns; a chart, picture or other object does not.
        let turns = uid > 0 && matches!(o, InlineObject::Shape { .. });
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
        let tol = REACH_PX / event.zoom.max(0.05);
        let mods = Mods { shift: event.mods.shift, alt: event.mods.alt };
        match event.action {
            PointerAction::Down => {
                // A shape chosen in Insert › Shapes: this drag draws it.
                let armed = self.session.borrow().view.draw.shape.clone();
                if let Some(preset) = armed {
                    let drag = kition_office_core::connect::CreateDrag::start(&preset, p, &self.objects_on(page), tol * 1.5);
                    let frame = drag.frame();
                    let gesture = Gesture::moving(Vec::new(), p);
                    self.object_drag =
                        Some(ObjectDrag { gesture, preview: Preview { frames: Vec::new() }, page, uid: 0, end: None, create: Some((drag, frame)) });
                    return Ok(true);
                }
                // A selected connector's end: drag it onto another shape's site, or off.
                if let Some((_, uid, _)) = self.selected_object().filter(|(h, u, _)| h.page == page && *u > 0)
                    && let Some((connector, objects)) = self.page_objects(uid, page)
                    && let Some(end) = kition_office_core::connect::hit_end(&connector, p, tol)
                {
                    let drag = kition_office_core::connect::EndDrag::start(&connector, end, &objects);
                    let frame = drag.frame();
                    let gesture = Gesture::moving(Vec::new(), p);
                    self.object_drag =
                        Some(ObjectDrag { gesture, preview: Preview { frames: Vec::new() }, page, uid, end: Some((drag, frame)), create: None });
                    return Ok(true);
                }
                if let Some((hit, uid, _)) = self.selected_object().filter(|(h, _, _)| h.page == page)
                    && let Some(geometry) = self.selected_geometry()
                    && let Some(index) = hit_adjust(&frame_of(&hit), &geometry, p, tol)
                {
                    let gesture = Gesture::adjusting(uid.to_string(), frame_of(&hit), geometry, index);
                    self.object_drag = Some(ObjectDrag { gesture, preview: Preview { frames: Vec::new() }, page, uid, end: None, create: None });
                    return Ok(true);
                }
                if let Some((hit, uid, turns)) = self.selected_object().filter(|(h, _, _)| h.page == page)
                    && let Some(h) = hit_handle(&frame_of(&hit), p, tol, turns)
                {
                    let id = uid.to_string();
                    let frame = frame_of(&hit);
                    let gesture = match h {
                        HandleHit::Resize(i) => Gesture::resizing(id, i, frame, p),
                        HandleHit::Rotate => Gesture::rotating(vec![(id, frame)], &frame, p),
                    };
                    self.object_drag = Some(ObjectDrag { gesture, preview: Preview { frames: Vec::new() }, page, uid, end: None, create: None });
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
                self.object_drag = Some(ObjectDrag {
                    gesture: Gesture::moving(vec![(uid.to_string(), frame_of(&hit))], p),
                    preview: Preview { frames: Vec::new() },
                    page,
                    uid,
                    end: None,
                    create: None,
                });
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
                drag.preview = drag.gesture.drag(p, mods, tol, None);
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
                } else if moved {
                    self.session().run("arrange.bounds", &json!({ "x": f.x, "y": f.y, "width": f.w, "height": f.h, "page": drag.page }))
                } else {
                    self.session().run("arrange.bounds", &json!({ "width": f.w, "height": f.h }))
                };
                result.map_err(|e| e.to_string())?;
                Ok(true)
            }
            _ => Ok(false),
        }
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
            return;
        }
        let (pts, rot) = box_handles(&frame);
        for q in pts {
            let [x, y] = to(q);
            out.push(ToolOverlay::Anchor { x, y, color: HANDLE, filled: true, size: 8.0 });
        }
        if turns {
            let [x, y] = to(rot);
            out.push(ToolOverlay::Anchor { x, y, color: ACCENT, filled: true, size: 8.0 });
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
        doc.execute("document.setText", json!({ "text": "A line of text to draw over.\nAnother line." })).unwrap();
        let inks = |doc: &mut DocDocument| -> Vec<DrawingObject> {
            let d = doc.execute("drawing.describe", json!({})).unwrap();
            serde_json::from_value::<Vec<DrawingObject>>(d["objects"].clone())
                .unwrap()
                .into_iter()
                .filter(|o| o.kind == kition_office_core::model::ObjectKind::Ink)
                .collect()
        };
        doc.execute("draw.pen", json!({ "color": "C00000", "width": 3 })).unwrap();
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
        doc.execute("draw.eraser", json!({})).unwrap();
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 170.0, 120.0))).unwrap();
        doc.pointer(&ev(PointerAction::Drag, on_page(&doc, 170.0, 160.0))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 170.0, 160.0))).unwrap();
        assert!(inks(&mut doc).is_empty());
        doc.execute("edit.undo", json!({})).unwrap();
        assert_eq!(inks(&mut doc).len(), 1, "undo brings it back");
        // Back to selecting: the pointer selects again instead of drawing.
        doc.execute("draw.select", json!({})).unwrap();
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
        doc.execute("draw.shape", json!({ "preset": "ellipse" })).unwrap();
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
        doc.execute("draw.shape", json!({ "preset": "line" })).unwrap();
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
        doc.execute("draw.shape", json!({ "preset": "rect" })).unwrap();
        doc.key("Escape", ToolMods::default(), 1.0).unwrap();
        assert_eq!(doc.session.borrow().view.draw.shape, None);
        assert!(doc.execute("draw.shape", json!({ "preset": "notAShape" })).is_err());
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

    #[test]
    fn a_press_beside_objects_places_the_caret_as_before() {
        let mut doc = DocDocument::create("Doc.docx", 0, 0).unwrap();
        doc.execute("text.insert", json!({ "text": "Plain text" })).unwrap();
        doc.pointer(&ev(PointerAction::Down, on_page(&doc, 80.0, 80.0))).unwrap();
        doc.pointer(&ev(PointerAction::Up, on_page(&doc, 80.0, 80.0))).unwrap();
        assert!(doc.selected_object().is_none());
    }
}
