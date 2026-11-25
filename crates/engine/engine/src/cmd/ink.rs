//! `drawing.ink` and `drawing.erase`: the Kit Harness Protocol's `KitDrawing 1.2` ink over Word's
//! ink strokes (a floating freeform per stroke, `draw.rs`), through the ink contract Kition's kits
//! share (`kition_office_core::ink`). Points are in points on a page (`placement.page`, else the
//! caret's); each command is one undo step.

use kition_doc_doc::Pos;
use kition_doc_doc::freeform::InkTool as DocTool;
use kition_doc_doc::para::{InlineObject, OBJ};
use kition_office_core::ink::{self, InkStroke, InkTool};
use kition_office_core::model as dm;
use serde_json::{Value, json};

use crate::{CmdError, CmdResult, CommandSpec, Session};

/// How far around the eraser's path it takes ink, points.
pub const ERASER_RADIUS: f64 = 4.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("drawing.ink", "Draw Ink Strokes", "", add).params(
            r#"{"strokes": [{"tool"?: "pen"|"pencil"|"highlighter", "color"?, "width"?: pt, "points": [[x, y, pressure?], …]}], "placement"?: {"page"}} (page points; members left out take the current pen)"#,
        ),
        CommandSpec::new("drawing.erase", "Erase Ink", "", erase)
            .params(r#"{"path": [[x, y], …] | "x", "y", "radius"?: pt, "placement"?: {"page"}} (deletes the strokes the path touches)"#),
    ]
}

fn fail(cmd: &str, e: kition_office_core::ops::DrawingError) -> CmdError {
    let property = e.property.as_deref().map(|p| format!(" ({p})")).unwrap_or_default();
    let text = format!("{cmd}: {}{property}: {}", e.code, e.message);
    if e.code == "invalid" { CmdError::Params(text) } else { CmdError::Failed(text) }
}

fn doc_tool(t: InkTool) -> DocTool {
    match t {
        InkTool::Pen => DocTool::Pen,
        InkTool::Pencil => DocTool::Pencil,
        InkTool::Highlighter => DocTool::Highlighter,
    }
}

fn shared_tool(t: DocTool) -> InkTool {
    match t {
        DocTool::Pen => InkTool::Pen,
        DocTool::Pencil => InkTool::Pencil,
        DocTool::Highlighter => InkTool::Highlighter,
    }
}

/// Whether the command runs for anyone but a person (an agent, a script).
fn not_a_person(s: &Session) -> bool {
    s.origin.as_ref().is_some_and(|origin| origin.kind != kdm_model::kition_kit::kdm::OriginKind::Person)
}

/// The page a command names (`placement.page` or `page`), else the caret's. Anyone but a person
/// names it (C8-doc-2): the caret's page is the editor's, not the document's, and no premise
/// can hold it.
fn page_of(s: &mut Session, v: &Value, cmd: &str) -> Result<usize, CmdError> {
    let named = v.get("placement").and_then(|p| p.get("page")).or_else(|| v.get("page")).and_then(Value::as_u64);
    if named.is_none() && not_a_person(s) {
        return Err(CmdError::Params(format!("{cmd}: give `placement.page` (the page the points are on, counted from 0)")));
    }
    Ok(match named {
        Some(p) => usize::try_from(p).unwrap_or(usize::MAX),
        None => {
            let layout = s.layout();
            layout.caret_on(&s.sel.focus, s.page_hint).map_or(s.page_hint, |c| c.page)
        }
    })
}

/// The ids of every ink stroke in the document.
fn ink_ids(s: &Session) -> Vec<u32> {
    s.doc.shapes().into_iter().filter(|(_, o)| o.ink().is_some()).filter_map(|(_, o)| o.object_uid()).collect()
}

/// The pen a stroke without its own members takes: the Draw tab's current one.
fn current_pen(s: &Session) -> InkStroke {
    let tool = s.view.draw.mode.pen().unwrap_or(DocTool::Pen);
    let set = s.view.draw.settings(tool);
    InkStroke {
        tool: shared_tool(tool),
        color: Some(dm::Color::rgb(dm::Rgb(set.color.0, set.color.1, set.color.2))),
        width: f64::from(set.width),
        points: Vec::new(),
    }
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    // The Draw tab's current pen is the editor's: anyone but a person gives every stroke's own
    // (C8-doc-2).
    if not_a_person(s)
        && v.get("strokes")
            .and_then(Value::as_array)
            .is_some_and(|strokes| strokes.iter().any(|st| ["tool", "color", "width"].iter().any(|m| st.get(*m).is_none())))
    {
        return Err(CmdError::Params("drawing.ink: give every stroke its `tool`, `color` and `width` (the current pen is the editor's)".into()));
    }
    let strokes = ink::parse_strokes(v, &current_pen(s)).map_err(|e| fail("drawing.ink", e))?;
    let page = page_of(s, v, "drawing.ink")?;
    let mut calls = Vec::with_capacity(strokes.len());
    for st in &strokes {
        let color = match &st.color {
            Some(c) => super::drawing::rgb_of(c, "color").map_err(|e| fail("drawing.ink", e))?,
            None => doc_tool(st.tool).default_color(),
        };
        let points: Vec<Value> = st.points.iter().map(|p| json!([p[0], p[1]])).collect();
        calls.push(json!({ "page": page, "tool": format!("{:?}", doc_tool(st.tool)).to_ascii_lowercase(), "color": format!("{:02X}{:02X}{:02X}", color.0, color.1, color.2), "width": st.width, "points": points }));
    }
    let before = ink_ids(s);
    for call in calls {
        s.run("ink.stroke", &call)?;
    }
    s.doc.assign_object_ids();
    let ids: Vec<String> = ink_ids(s).into_iter().filter(|id| !before.contains(id)).map(|id| id.to_string()).collect();
    Ok(json!({ "ids": ids }))
}

/// Word's ink strokes on `page`, described: (their id, the stroke in page points).
pub fn strokes_on(s: &mut Session, page: usize) -> Vec<(u32, InkStroke)> {
    let layout = s.layout();
    super::draw::ink_on(s, &layout, page)
        .into_iter()
        .filter_map(|(pos, rect, f, width)| {
            let o = s.doc.para_at(&pos)?.object_at(pos.off)?;
            let uid = o.object_uid()?;
            Some((uid, stroke_of(o, &f, rect.x, rect.y, rect.w, rect.h, width)))
        })
        .collect()
}

/// An ink freeform at (`x`, `y`) sized `w` × `h` on its page, as a shared stroke.
pub fn stroke_of(o: &InlineObject, f: &kition_doc_doc::freeform::Freeform, x: f32, y: f32, w: f32, h: f32, width: f32) -> InkStroke {
    let color = match o {
        InlineObject::Shape { stroke: Some(c), .. } => Some(dm::Color::rgb(dm::Rgb(c.0, c.1, c.2))),
        _ => None,
    };
    let points = f.placed(x, y, w, h).into_iter().next().map(|(pts, _)| pts).unwrap_or_default();
    InkStroke {
        tool: f.ink.map_or(InkTool::Pen, shared_tool),
        color,
        width: f64::from(width),
        points: points.into_iter().map(|[px, py]| [f64::from(px), f64::from(py), 0.5]).collect(),
    }
}

/// The ink strokes `drawing.erase` with `v` takes, by id, without taking them (what an eraser
/// path names now: the premise guard's place, C8-doc-2).
pub fn erase_hits(s: &mut Session, v: &Value) -> Result<Vec<u32>, CmdError> {
    let gesture = ink::parse_eraser(v, ERASER_RADIUS).map_err(|e| fail("drawing.erase", e))?;
    let page = page_of(s, v, "drawing.erase")?;
    if s.view.hide_ink {
        return Ok(Vec::new());
    }
    let on_page = strokes_on(s, page);
    let strokes: Vec<InkStroke> = on_page.iter().map(|(_, st)| st.clone()).collect();
    Ok(gesture.hits(&strokes).into_iter().filter_map(|i| on_page.get(i).map(|(id, _)| *id)).collect())
}

fn erase(s: &mut Session, v: &Value) -> CmdResult {
    let hit = erase_hits(s, v)?;
    for uid in &hit {
        let Some(pos) = s.doc.find_shape(*uid) else { continue };
        let end = Pos { off: pos.off + OBJ.len_utf8(), ..pos.clone() };
        s.doc.delete_range(&pos, &end)?;
        let len = OBJ.len_utf8();
        for at in [&mut s.sel.anchor, &mut s.sel.focus] {
            if at.story == pos.story && at.path == pos.path && at.off > pos.off {
                at.off = at.off.saturating_sub(len).max(pos.off);
            }
        }
    }
    Ok(json!({ "erased": hit.len(), "ids": hit.iter().map(u32::to_string).collect::<Vec<_>>() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_office_core::model::DrawingObject;

    fn session() -> Session {
        let mut s = Session::new(kition_doc_doc::Document::new());
        s.run("file.setText", &json!({ "text": "First paragraph of text.\nSecond paragraph of text." })).unwrap();
        s
    }

    fn inks(s: &mut Session) -> Vec<DrawingObject> {
        let d = s.run("drawing.describe", &json!({})).unwrap();
        serde_json::from_value::<Vec<DrawingObject>>(d["objects"].clone()).unwrap().into_iter().filter(|o| o.kind == dm::ObjectKind::Ink).collect()
    }

    #[test]
    fn strokes_draw_describe_undo_erase_and_reopen() {
        let mut s = session();
        let undo = s.history.len();
        let r = s
            .run(
                "drawing.ink",
                &json!({ "placement": { "page": 0 }, "strokes": [
                    { "points": [[100, 100], [200, 100]], "color": "#C00000", "width": 2 },
                    { "tool": "highlighter", "points": [[100, 160], [200, 160]] },
                ] }),
            )
            .unwrap();
        assert_eq!(s.history.len(), undo + 1, "one undo step for both strokes");
        let ids: Vec<String> = serde_json::from_value(r["ids"].clone()).unwrap();
        assert_eq!(ids.len(), 2);
        let objects = inks(&mut s);
        assert_eq!(objects.len(), 2);
        let pen = objects.iter().find(|o| o.ink.as_ref().unwrap()[0].tool == InkTool::Pen).unwrap();
        let st = &pen.ink.as_ref().unwrap()[0];
        assert_eq!(st.width, 2.0);
        assert!(
            (st.points[0][0] - 100.0).abs() < 0.5 && (st.points[1][0] - 200.0).abs() < 0.5 && (st.points[0][1] - 100.0).abs() < 0.5,
            "{:?}",
            st.points
        );
        assert_eq!(st.color, Some(dm::Color::rgb(dm::Rgb(0xC0, 0, 0))));
        assert_eq!(objects.iter().filter(|o| o.ink.as_ref().unwrap()[0].tool == InkTool::Highlighter).count(), 1);
        // Ink keeps its pen's look.
        let e = s.run("drawing.format", &json!({ "ids": [pen.id], "fill": { "color": "#00FF00" } })).unwrap_err();
        assert!(e.to_string().contains("unsupported"), "{e}");

        // The eraser takes the stroke its path crosses, and only it, in one step.
        let undo = s.history.len();
        let r = s.run("drawing.erase", &json!({ "placement": { "page": 0 }, "path": [[150, 80], [150, 120]] })).unwrap();
        assert_eq!(r["erased"], 1);
        assert_eq!(s.history.len(), undo + 1);
        let left = inks(&mut s);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].ink.as_ref().unwrap()[0].tool, InkTool::Highlighter);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(inks(&mut s).len(), 2, "undo brings the erased stroke back");

        // Save as DOCX and reopen: the strokes, their pens and their ids come back.
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        let reopened = inks(&mut back);
        assert_eq!(reopened.len(), 2);
        let mut got: Vec<String> = reopened.iter().map(|o| o.id.clone()).collect();
        got.sort();
        let mut want = ids.clone();
        want.sort();
        assert_eq!(got, want);
        assert!(reopened.iter().any(|o| o.ink.as_ref().unwrap()[0].tool == InkTool::Highlighter));
    }

    #[test]
    fn bad_strokes_and_empty_erasers_change_nothing() {
        let mut s = session();
        assert!(s.run("drawing.ink", &json!({ "strokes": [{ "points": [] }] })).is_err());
        assert!(s.run("drawing.ink", &json!({ "strokes": [{ "tool": "brush", "points": [[1, 1]] }] })).is_err());
        assert!(inks(&mut s).is_empty());
        let r = s.run("drawing.erase", &json!({ "x": 10, "y": 10 })).unwrap();
        assert_eq!(r["erased"], 0);
    }
}
