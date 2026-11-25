//! `drawing.*`: the Kit Harness Protocol's `KitDrawing 1.0` commands over the document's shapes and
//! text boxes, through the drawing model Kition's kits share (`kition-office-core`). A shape is named
//! by its id in the document (`uid`, kept in DOCX as its drawing id); its frame is where layout puts
//! it, in points on its page (`placement.page`), and a changed frame goes through `arrange.bounds`,
//! so a moved object floats and anchors to the paragraph under it as a drag does. Each command is
//! one undo step (the commands it runs push none of their own).
//!
//! Charts (`KitDrawing 1.1`) are described from their part (`c:chartSpace`) with the shared chart
//! codec; `drawing.chart` patches the part, so what the description leaves out stays, keeps it on
//! the chart (`Graphic::chart`, saved in place of the part read), and draws it again.

use kition_doc_doc::graphic::{Graphic, GraphicKind, TextAnchor, TextFrame};
use kition_doc_doc::para::{Float, InlineObject, ShapeKind, Wrap};
use kition_doc_doc::{Block, CharProps, Paragraph, PartKind, Pos, Rgb, StoryRef, para_block};
use kition_office_core::Xfrm;
use kition_office_core::chart::{ChartSpec, codec};
use kition_office_core::model::{self as dm, DrawingObject};
use kition_office_core::ops::{DrawingError, DrawingOp};
use serde_json::{Value, json};

use crate::{CmdError, CmdResult, CommandSpec, Selection, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("drawing.describe", "Describe Drawing Objects", "", describe)
            .params(r#"{"ids"?: [id]} (default: the selected objects, else (a caret, not a selection) every object in the document)"#)
            .pure(),
        CommandSpec::new("drawing.insert", "Insert Drawing Object", "", |s, v| {
            let r = insert(s, v)?;
            on_top(s, &r)?;
            Ok(r)
        })
        .params(
            r#"{"kind"?: "shape"|"textBox"|"chart", "geometry"?: {"preset", "adjust"?}, "frame"?: {x,y,w,h,rot?,flipH?,flipV?} (points on page "placement.page"; without x/y the object goes in line at the caret), "placement"?: {"page"?, "wrap"?: "inline"|"square"|"tight"|"through"|"topAndBottom"|"behindText"|"inFrontOfText"}, "fill"?, "stroke"?, "text"?, "chart"?: {kind?, title?, categories?, series?: [{name, values, color?}], legend?, dataLabels?, gridlines?, xTitle?, yTitle?}}"#,
        ),
        CommandSpec::new("drawing.delete", "Delete Drawing Objects", "", |s, v| apply(s, "drawing.delete", v)).params(r#"{"ids": [id]}"#),
        CommandSpec::new("drawing.transform", "Transform Drawing Objects", "", |s, v| apply(s, "drawing.transform", v))
            .params(r#"{"ids": [id], "frame"? | "dx"?, "dy"?, "rotateBy"?, "flipH"?, "flipV"?}"#),
        CommandSpec::new("drawing.format", "Format Drawing Objects", "", |s, v| apply(s, "drawing.format", v)).params(r#"{"ids": [id], "fill"?, "stroke"?, "geometry"?}"#),
        CommandSpec::new("drawing.text", "Set Drawing Object Text", "", |s, v| apply(s, "drawing.text", v)).params(r#"{"id", "text"? | "paragraphs"?}"#),
        CommandSpec::new("drawing.reconnect", "Reconnect Connector", "", |s, v| apply(s, "drawing.reconnect", v))
            .params(r#"{"id", "end": "start" | "end", "to"?: id, "site"?: n, "point"?: [x, y]}"#),
        CommandSpec::new("drawing.connect", "Connect Drawing Objects", "", connect)
            .params(r#"{"from": id, "to": id, "fromSite"?, "toSite"?, "geometry"?: {"preset": "straightConnector1"|"bentConnector3"|"curvedConnector3"}} (floating shapes)"#),
        CommandSpec::new("drawing.group", "Group Drawing Objects", "", group)
            .params(r#"{"ids": [id]} (two or more floating shapes and pictures in the body, on one page)"#),
        CommandSpec::new("drawing.ungroup", "Ungroup Drawing Objects", "", ungroup).params(r#"{"ids": [id]} (groups)"#),
        CommandSpec::new("drawing.arrange", "Arrange Drawing Objects", "", arrange)
            .params(r#"{"ids": [id], "order": "front"|"back"|"forward"|"backward"} (floating objects; an inline one has no place in the stack)"#),
        // Insert › Chart: `drawing.insert` of a chart of a type, with its data when given.
        CommandSpec::new("chart.insert", "Chart", "Insert › Illustrations", |s, v| insert(s, &chart_insert(v)?))
            .params(r#"{"type"?: "column"|"stackedColumn"|"percentColumn"|"bar"|"stackedBar"|"percentBar"|"line"|"lineMarkers"|"stackedLine"|"area"|"stackedArea"|"pie"|"doughnut"|"scatter"|"radar", "title"?: text|null, "categories"?, "series"?: [{name, values, color?}], "legend"?, "dataLabels"?, "gridlines"?, "xTitle"?, "yTitle"?, "width"?: pt, "height"?: pt} (in line at the caret, which stays after it; without data, Word's sample data)"#),
        CommandSpec::new("drawing.chart", "Change Chart", "", |s, v| apply(s, "drawing.chart", v)).params(
            r#"{"id", "kind"?, "title"?: text|null, "legend"?: "top"|"bottom"|"left"|"right"|"topRight"|null, "dataLabels"?, "gridlines"?, "xTitle"?, "yTitle"?, "categories"?, "series"?: [{name, values, color?}]}"#,
        ),
    ]
}

/// `chart.insert`'s parameters as `drawing.insert`'s: `type` is the chart's kind, `width` and
/// `height` its frame, and the rest its description (`drawing.chart`'s members).
fn chart_insert(v: &Value) -> Result<Value, CmdError> {
    let mut chart = match v {
        Value::Object(m) => m.clone(),
        Value::Null => serde_json::Map::new(),
        _ => return Err(CmdError::Params("chart.insert: parameters are an object".into())),
    };
    let kind = chart.remove("type").or_else(|| chart.remove("kind")).unwrap_or_else(|| json!("column"));
    chart.insert("kind".into(), kind);
    let (w, h) = (chart.remove("width"), chart.remove("height"));
    let mut out = json!({ "kind": "chart", "chart": chart });
    if w.is_some() || h.is_some() {
        let num = |x: Option<Value>, name: &str, default: f64| match x {
            None => Ok(default),
            Some(x) => x.as_f64().ok_or_else(|| CmdError::Params(format!("chart.insert: `{name}` is a number of points"))),
        };
        out["frame"] = json!({ "w": num(w, "width", 432.0)?, "h": num(h, "height", 252.0)? });
    }
    Ok(out)
}

fn fail(cmd: &str, e: DrawingError) -> CmdError {
    let property = e.property.as_deref().map(|p| format!(" ({p})")).unwrap_or_default();
    let text = format!("{cmd}: {}{property}: {}", e.code, e.message);
    match e.code {
        "invalid" => CmdError::Params(text),
        _ => CmdError::Failed(text),
    }
}

/// Objects `KitDrawing` describes: shapes and text boxes, not freeforms (ink and drawn paths), and
/// charts (1.1).
fn is_drawing(o: &InlineObject) -> bool {
    match o {
        // A freeform drawn with a pen is ink (1.2); other freeforms are not described.
        InlineObject::Shape { kind, freeform, .. } => *kind != ShapeKind::Freeform || freeform.as_ref().is_some_and(|f| f.is_ink()),
        InlineObject::Graphic { graphic, .. } => graphic.kind == GraphicKind::Chart,
        InlineObject::Group { .. } => true,
        _ => false,
    }
}

/// The group holding drawing object `uid` as a member: the group's place, the group, and the
/// member's index.
fn member_of(s: &Session, uid: u32) -> Option<(Pos, InlineObject, usize)> {
    if uid == 0 {
        return None;
    }
    s.doc.shapes().into_iter().find_map(|(pos, o)| match o {
        InlineObject::Group { children, .. } => children.iter().position(|c| c.obj.object_uid() == Some(uid)).map(|i| (pos.clone(), o.clone(), i)),
        _ => None,
    })
}

/// Member `index` of group `g` (at `pos`), described where the group shows it.
fn member_drawing(s: &mut Session, pos: &Pos, g: &InlineObject, index: usize) -> Option<DrawingObject> {
    let InlineObject::Group { w, h, float, .. } = g else { return None };
    let (frame, placement) = frame_of(s, pos, *w, *h, float);
    let (r, child) = g.group_rects(frame.x as f32, frame.y as f32, *w, *h).into_iter().nth(index)?;
    let child = child.clone();
    let mut o = to_drawing(s, pos, &child);
    o.frame = Xfrm { x: f64::from(r[0]), y: f64::from(r[1]), w: f64::from(r[2]), h: f64::from(r[3]), ..o.frame };
    o.placement = placement;
    Some(o)
}

fn uid_of(o: &InlineObject) -> u32 {
    o.object_uid().unwrap_or(0)
}

/// A chart's description: as edited, else from its part.
fn chart_spec(s: &Session, g: &Graphic) -> Option<ChartSpec> {
    match &g.spec {
        Some(spec) => Some((**spec).clone()),
        None => codec::read(&kition_doc_docx::chart_part(&s.doc, g)?).ok(),
    }
}

/// Chart `g` described as `spec` (its part stays as made or read), drawn at `w` × `h`.
fn chart_changed(s: &Session, g: &Graphic, spec: &ChartSpec, w: f32, h: f32) -> Result<Graphic, DrawingError> {
    let part = kition_doc_docx::chart_part(&s.doc, g).ok_or_else(|| DrawingError::unsupported("chart", "this chart's part was not kept"))?;
    codec::patch(&part, spec).map_err(|e| DrawingError::invalid(format!("the chart part: {}", e.0)))?;
    Ok(chart_drawn(s, Graphic { spec: Some(Box::new(spec.clone())), ..g.clone() }, w, h))
}

/// Chart `g` drawn again from its part at `w` × `h`.
fn chart_drawn(s: &Session, mut g: Graphic, w: f32, h: f32) -> Graphic {
    if let Some(part) = kition_doc_docx::chart_part(&s.doc, &g) {
        g.items = kition_doc_docx::chart_part_items(&part, &s.doc.settings.theme_colors, w, h);
        (g.w, g.h) = (w, h);
    }
    g
}

fn object_ids(v: &Value) -> Result<Option<Vec<u32>>, DrawingError> {
    let list: Vec<&Value> = match (v.get("ids"), v.get("id")) {
        (Some(Value::Array(a)), _) => a.iter().collect(),
        (None, Some(x)) => vec![x],
        (None, None) => return Ok(None),
        _ => return Err(DrawingError::invalid("`ids` is a list of ids")),
    };
    list.into_iter()
        .map(|x| {
            let text = match x {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => return Err(DrawingError::invalid("an id is a string")),
            };
            text.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(|| DrawingError::not_found(&text))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn find(s: &Session, cmd: &str, uid: u32) -> Result<(Pos, InlineObject), CmdError> {
    if s.doc.find_shape(uid).is_none() && member_of(s, uid).is_some() {
        return Err(fail(cmd, DrawingError::unsupported("ids", format!("object {uid} is in a group: change the group, or ungroup it"))));
    }
    let pos = s.doc.find_shape(uid).ok_or_else(|| fail(cmd, DrawingError::not_found(&uid.to_string())))?;
    let o = s.doc.para_at(&pos).and_then(|p| p.object_at(pos.off)).cloned().ok_or_else(|| fail(cmd, DrawingError::not_found(&uid.to_string())))?;
    if !is_drawing(&o) {
        return Err(fail(cmd, DrawingError::unsupported("kind", format!("object {uid} is a freeform drawn as a path, not a shape or ink"))));
    }
    if cmd == "drawing.chart" && !matches!(o, InlineObject::Graphic { .. }) {
        return Err(fail(cmd, DrawingError::unsupported("kind", format!("object {uid} is not a chart"))));
    }
    Ok((pos, o))
}

fn describe(s: &mut Session, v: &Value) -> CmdResult {
    let ids = match object_ids(v).map_err(|e| fail("drawing.describe", e))? {
        Some(ids) => ids,
        None => {
            // The objects the person selected (DR9); a caret beside an object is not a selection
            // of it, so then every object is described.
            let selected: Vec<u32> = if s.sel.anchor == s.sel.focus {
                Vec::new()
            } else {
                super::objects::selected_all(s, 64).iter().filter(|(_, o)| is_drawing(o)).map(|(_, o)| uid_of(o)).collect()
            };
            if selected.is_empty() { stacked(s).into_iter().map(|(uid, _)| uid).collect() } else { selected }
        }
    };
    let mut objects = Vec::new();
    for uid in ids {
        if s.doc.find_shape(uid).is_none()
            && let Some((pos, g, i)) = member_of(s, uid)
            && let Some(o) = member_drawing(s, &pos, &g, i)
        {
            objects.push(o);
            continue;
        }
        let (pos, o) = find(s, "drawing.describe", uid)?;
        objects.push(to_drawing(s, &pos, &o));
    }
    let mut out = json!({ "objects": objects });
    if objects.len() > 1 {
        out["selection"] = dm::describe_many(&objects);
    }
    Ok(out)
}

/// Where an object is drawn: 0 behind the text, 1 in the text (inline), 2 in front of it.
fn layer(o: &InlineObject) -> u8 {
    match o.frame().map(|(_, _, f)| f.wrap) {
        Some(Wrap::BehindText) => 0,
        Some(Wrap::Inline) | None => 1,
        Some(_) => 2,
    }
}

/// Every drawing object, bottom to top as layout draws them: behind the text, inline, then in
/// front, each floating layer by its place in the stack (`Float::z`), else in document order;
/// with whether it floats.
fn stacked(s: &Session) -> Vec<(u32, bool)> {
    let mut all: Vec<(u8, i64, usize, u32)> = s
        .doc
        .shapes()
        .iter()
        .filter(|(_, o)| is_drawing(o))
        .enumerate()
        .map(|(i, (_, o))| (layer(o), o.frame().and_then(|(_, _, f)| f.z).unwrap_or(i64::MIN), i, uid_of(o)))
        .collect();
    all.sort();
    all.into_iter().map(|(l, _, _, uid)| (uid, l != 1)).collect()
}

/// Spacing between the places `drawing.arrange` gives floating objects, over Word's own base.
const Z_BASE: i64 = 251_658_240;
const Z_STEP: i64 = 1024;

/// A new floating object goes on top of the others, as Word puts it.
fn on_top(s: &mut Session, result: &Value) -> Result<(), CmdError> {
    let Some(uid) = result.get("id").and_then(Value::as_str).and_then(|id| id.parse::<u32>().ok()) else { return Ok(()) };
    let Some(pos) = s.doc.find_shape(uid) else { return Ok(()) };
    let floats: Vec<i64> = s
        .doc
        .shapes()
        .iter()
        .filter(|(_, o)| uid_of(o) != uid)
        .filter_map(|(_, o)| o.frame().filter(|(_, _, f)| f.wrap != Wrap::Inline).map(|(_, _, f)| f.z.unwrap_or(0)))
        .collect();
    let mine = s.doc.para_at(&pos).and_then(|p| p.object_at(pos.off)).and_then(|o| o.frame()).is_some_and(|(_, _, f)| f.wrap != Wrap::Inline);
    if !mine || floats.is_empty() {
        return Ok(());
    }
    let top = floats.iter().copied().max().unwrap_or(0).max(Z_BASE + Z_STEP * floats.len() as i64) + Z_STEP;
    edit(s, &pos, |o| {
        if let Some(f) = o.float_mut() {
            f.z = Some(top);
        }
    })
}

/// `drawing.arrange` (`KitDrawing 1.4`, DR21): floating objects move in the stack, one undo step.
/// Every floating object gets its place anew, in steps of [`Z_STEP`].
fn arrange(s: &mut Session, v: &Value) -> CmdResult {
    const CMD: &str = "drawing.arrange";
    let (named, order) = kition_office_core::arrange::Order::parse(v).map_err(|e| fail(CMD, e))?;
    let all = stacked(s);
    for id in &named {
        match id.parse::<u32>().ok().and_then(|uid| all.iter().find(|(u, _)| *u == uid)) {
            None => return Err(fail(CMD, DrawingError::not_found(id))),
            Some((_, false)) => {
                return Err(fail(
                    CMD,
                    DrawingError::unsupported("placement", format!("object {id} is in line with the text: it has no place in the stack")),
                ));
            }
            Some(_) => {}
        }
    }
    // The stack: the floating objects, bottom to top (those behind the text below the others).
    let stack: Vec<String> = all.iter().filter(|(_, floats)| *floats).map(|(uid, _)| uid.to_string()).collect();
    let next = kition_office_core::arrange::arrange(&stack, &named, order).map_err(|e| fail(CMD, e))?;
    if next == stack {
        return Ok(json!({}));
    }
    for (i, id) in next.iter().enumerate() {
        let uid: u32 = id.parse().unwrap_or(0);
        let Some(pos) = s.doc.find_shape(uid) else { continue };
        let z = Z_BASE + Z_STEP * (i as i64 + 1);
        edit(s, &pos, |o| {
            if let Some(f) = o.float_mut() {
                f.z = Some(z);
            }
        })?;
    }
    Ok(json!({}))
}

/// `drawing.group` (`KitDrawing 1.4`, DR22): Layout › Arrange › Group on the objects named.
fn group(s: &mut Session, v: &Value) -> CmdResult {
    const CMD: &str = "drawing.group";
    let named = kition_office_core::arrange::group_ids(v, 2).map_err(|e| fail(CMD, e))?;
    let mut picks = Vec::new();
    for id in &named {
        let uid: u32 = id.parse().map_err(|_| fail(CMD, DrawingError::not_found(id)))?;
        let (pos, o) = find(s, CMD, uid)?;
        if matches!(o, InlineObject::Graphic { .. }) {
            return Err(fail(CMD, DrawingError::unsupported("kind", format!("object {id} is a chart: charts are not grouped yet"))));
        }
        if !o.is_floating() {
            return Err(fail(CMD, DrawingError::unsupported("placement", format!("object {id} is in line with the text: give it a wrapping first"))));
        }
        picks.push(pos);
    }
    let first = picks.remove(0);
    let len = kition_doc_doc::para::OBJ.len_utf8();
    s.sel = Selection { anchor: first.clone(), focus: Pos { off: first.off + len, ..first } };
    s.also_selected = picks;
    s.run("arrange.group", &json!({})).map_err(|e| fail(CMD, DrawingError::unsupported("ids", e.to_string())))?;
    s.doc.assign_object_ids();
    let (a, _) = s.sel.ordered();
    let uid = s.doc.para_at(&a).and_then(|p| p.object_at(a.off)).and_then(InlineObject::object_uid).unwrap_or(0);
    Ok(json!({ "id": uid.to_string() }))
}

/// `drawing.ungroup` (DR22): Layout › Arrange › Ungroup on each group named; answers the members.
fn ungroup(s: &mut Session, v: &Value) -> CmdResult {
    const CMD: &str = "drawing.ungroup";
    let named = kition_office_core::arrange::group_ids(v, 1).map_err(|e| fail(CMD, e))?;
    let mut members = Vec::new();
    for id in &named {
        let uid: u32 = id.parse().map_err(|_| fail(CMD, DrawingError::not_found(id)))?;
        let (pos, o) = find(s, CMD, uid)?;
        let InlineObject::Group { children, .. } = &o else {
            return Err(fail(CMD, DrawingError::unsupported("kind", format!("object {id} is not a group"))));
        };
        members.extend(children.iter().filter_map(|c| c.obj.object_uid()).filter(|u| *u > 0));
        let len = kition_doc_doc::para::OBJ.len_utf8();
        s.sel = Selection { anchor: pos.clone(), focus: Pos { off: pos.off + len, ..pos } };
        s.also_selected.clear();
        s.run("arrange.ungroup", &json!({}))?;
    }
    s.doc.assign_object_ids();
    Ok(json!({ "ids": members.into_iter().map(|u| u.to_string()).collect::<Vec<_>>() }))
}

/// A Word shape, described: its frame where layout puts it on its page.
pub fn to_drawing(s: &mut Session, pos: &Pos, o: &InlineObject) -> DrawingObject {
    if let InlineObject::Group { w, h, float, children, uid, .. } = o {
        let (frame, placement) = frame_of(s, pos, *w, *h, float);
        return DrawingObject {
            id: uid.to_string(),
            kind: dm::ObjectKind::Group,
            frame,
            placement,
            members: Some(children.iter().filter_map(|c| c.obj.object_uid()).filter(|u| *u > 0).map(|u| u.to_string()).collect()),
            ..Default::default()
        };
    }
    if let InlineObject::Graphic { w, h, float, graphic, .. } = o {
        let (frame, placement) = frame_of(s, pos, *w, *h, float);
        return DrawingObject {
            id: graphic.uid.to_string(),
            kind: dm::ObjectKind::Chart,
            frame,
            placement,
            chart: chart_spec(s, graphic),
            ..Default::default()
        };
    }
    if let InlineObject::Shape { w, h, float, freeform: Some(f), stroke_width, uid, .. } = o
        && f.is_ink()
    {
        let (frame, placement) = frame_of(s, pos, *w, *h, float);
        let stroke = super::ink::stroke_of(o, f, frame.x as f32, frame.y as f32, *w, *h, *stroke_width);
        return DrawingObject { id: uid.to_string(), kind: dm::ObjectKind::Ink, frame, placement, ink: Some(vec![stroke]), ..Default::default() };
    }
    let InlineObject::Shape {
        kind, w, h, fill, stroke, stroke_width, float, story, geom, uid, gradient, effects, connection, warp, body, dash, ..
    } = o
    else {
        return DrawingObject::default();
    };
    let (frame, placement) = frame_of(s, pos, *w, *h, float);
    let rgb = |c: &Rgb| dm::Color::rgb(dm::Rgb(c.0, c.1, c.2));
    let geometry = match geom.as_ref().filter(|g| !g.name.is_empty()) {
        Some(g) => dm::Geometry::Preset { preset: g.name.clone(), adjust: g.adj.iter().cloned().collect() },
        None => dm::Geometry::preset(kind.preset_name()),
    };
    let frame_body = body.unwrap_or_default();
    let text = story.map(|id| dm::TextBody {
        warp: warp.as_ref().map(|w| dm::Warp { preset: w.name.clone(), adjust: w.adj.iter().cloned().collect() }),
        anchor: match frame_body.anchor {
            TextAnchor::Top => dm::TextAnchor::Top,
            TextAnchor::Middle => dm::TextAnchor::Middle,
            TextAnchor::Bottom => dm::TextAnchor::Bottom,
        },
        // In hundredths of a point (no f32 noise).
        insets: frame_body.insets.map(|v| (f64::from(v) * 100.0).round() / 100.0),
        ..story_text(s, id)
    });
    DrawingObject {
        id: uid.to_string(),
        kind: if *kind == ShapeKind::TextBox { dm::ObjectKind::TextBox } else { dm::ObjectKind::Shape },
        frame,
        placement,
        geometry,
        fill: Some(match gradient.as_deref().filter(|g| g.stops.len() >= 2) {
            Some(g) => dm::Fill::Gradient(dm::Gradient {
                stops: g.stops.iter().map(|st| dm::GradientStop { pos: f64::from(st.pos), color: rgb(&st.color) }).collect(),
                angle: f64::from(g.angle),
                path: g.path.clone(),
            }),
            None => fill.as_ref().map_or(dm::Fill::None, |c| dm::Fill::Solid(rgb(c))),
        }),
        stroke: Some(match stroke {
            None => dm::Stroke { none: true, ..Default::default() },
            Some(c) => {
                dm::Stroke { none: false, color: Some(rgb(c)), width: Some(f64::from(*stroke_width)), dash: dash.map(|d| d.name().to_string()) }
            }
        }),
        text,
        chart: None,
        ink: None,
        members: None,
        effects: effects_of(effects),
        connection: connection.map(|c| kition_office_core::connect::Connection {
            start: c.start.map(|(id, site)| kition_office_core::connect::End { id: id.to_string(), site: site as usize }),
            end: c.end.map(|(id, site)| kition_office_core::connect::End { id: id.to_string(), site: site as usize }),
        }),
    }
}

/// Connectors glued to objects that moved, put back on their ends' sites (`KitDrawing 1.4`,
/// DR25), in the command that moved them: `drawing.*` commands, and (from the session) every
/// other edit, such as a drag with the pointer or text that pushes a floating shape down.
pub(crate) fn reroute_connectors(s: &mut Session) -> Result<(), CmdError> {
    let connectors: Vec<u32> =
        s.doc.shapes().iter().filter(|(_, o)| matches!(o, InlineObject::Shape { connection: Some(_), .. })).map(|(_, o)| uid_of(o)).collect();
    for uid in connectors {
        let Ok((pos, o)) = find(s, "drawing.connect", uid) else { continue };
        let before = to_drawing(s, &pos, &o);
        let Some(conn) = before.connection.clone() else { continue };
        let mut ends = Vec::new();
        for e in [&conn.start, &conn.end].into_iter().flatten() {
            if let Ok(end_uid) = e.id.parse::<u32>()
                && let Some(p) = s.doc.find_shape(end_uid)
                && let Some(obj) = s.doc.para_at(&p).and_then(|q| q.object_at(p.off)).cloned()
            {
                ends.push(to_drawing(s, &p, &obj));
            }
        }
        let Some(f) = kition_office_core::connect::reroute(&before, &conn, &ends) else { continue };
        let same = (f.x - before.frame.x).abs() < 0.01
            && (f.y - before.frame.y).abs() < 0.01
            && (f.w - before.frame.w).abs() < 0.01
            && (f.h - before.frame.h).abs() < 0.01;
        if same && (f.flip_h, f.flip_v) == (before.frame.flip_h, before.frame.flip_v) {
            continue;
        }
        let after = DrawingObject { frame: f, ..before.clone() };
        write_back(s, "drawing.connect", uid, &before, &after, true)?;
    }
    Ok(())
}

/// `drawing.connect` (`KitDrawing 1.4`, DR25): a floating connector glued to two shapes' sites
/// (named, or the nearest pair), one undo step.
fn connect(s: &mut Session, v: &Value) -> CmdResult {
    const CMD: &str = "drawing.connect";
    let c = kition_office_core::connect::Connect::parse(v).map_err(|e| fail(CMD, e))?;
    let end = |s: &mut Session, text: &str| -> Result<(u32, DrawingObject), CmdError> {
        let uid: u32 = text.parse().map_err(|_| fail(CMD, DrawingError::not_found(text)))?;
        let (pos, o) = find(s, CMD, uid)?;
        if !matches!(o, InlineObject::Shape { .. }) || !o.is_floating() {
            return Err(fail(CMD, DrawingError::unsupported("from", format!("object {uid} is not a floating shape"))));
        }
        Ok((uid, to_drawing(s, &pos, &o)))
    };
    let (from, a) = end(s, &c.from)?;
    let (to, b) = end(s, &c.to)?;
    let (i, j, p0, p1) = c.sites(&a, &b).map_err(|e| fail(CMD, e))?;
    let page = a.placement.extra.get("page").cloned().unwrap_or(json!(0));
    let frame = kition_office_core::connect::line_frame(p0, p1);
    // A new object takes the selection's place: put it at the caret instead.
    s.sel = Selection::caret(s.sel.focus.clone());
    let r = insert(
        s,
        &json!({ "geometry": { "preset": c.preset }, "frame": frame, "placement": { "page": page }, "fill": { "none": true }, "stroke": { "color": "#4472C4", "width": 1.5 } }),
    )?;
    let uid: u32 = r
        .get("id")
        .and_then(Value::as_str)
        .and_then(|t| t.parse().ok())
        .ok_or_else(|| CmdError::Failed("drawing.connect: the connector was not made".into()))?;
    let pos = s.doc.find_shape(uid).ok_or_else(|| CmdError::Failed("drawing.connect: the connector vanished".into()))?;
    edit(s, &pos, |o| {
        if let InlineObject::Shape { connection, .. } = o {
            *connection = Some(kition_doc_doc::graphic::Connection { start: Some((from, i as u32)), end: Some((to, j as u32)) });
        }
    })?;
    on_top(s, &r)?;
    Ok(json!({ "id": uid.to_string() }))
}

/// A shape's shadow and glow (DR24): its colours with their opacity.
fn effects_of(e: &kition_doc_doc::effects::ShapeEffects) -> Option<dm::Effects> {
    // The opacity to a thousandth (the model keeps a percentage).
    let tinted = |c: &Rgb, transparency: f32| dm::Color {
        alpha: Some(((100.0 - f64::from(transparency.clamp(0.0, 100.0))) * 10.0).round() / 1000.0),
        ..dm::Color::rgb(dm::Rgb(c.0, c.1, c.2))
    };
    let out = dm::Effects {
        shadow: e.shadow.map(|s| dm::Shadow {
            color: tinted(&s.color, s.transparency),
            blur: f64::from(s.blur),
            distance: f64::from(s.distance),
            angle: f64::from(s.angle),
        }),
        glow: e.glow.map(|g| dm::Glow { color: tinted(&g.color, g.transparency), size: f64::from(g.size) }),
    };
    (!out.is_empty()).then_some(out)
}

/// A text box's story as the shared description's text (DR8): its paragraphs' alignment and
/// their runs' font, size, weight, slant, underline and colour.
fn story_text(s: &Session, id: u32) -> dm::TextBody {
    let blocks = s.doc.parts.get(&id).map(|p| p.blocks.clone()).unwrap_or_default();
    let mut paragraphs = Vec::new();
    for b in &blocks {
        // A table in the box is not text the description holds.
        let Some(p) = b.as_para() else { continue };
        let mut runs = Vec::new();
        let mut at = 0usize;
        for r in &p.runs {
            let end = (at + r.len).min(p.text.len());
            let text: String = p.text.get(at..end).unwrap_or("").chars().filter(|c| *c != kition_doc_doc::para::OBJ).collect();
            at = end;
            if text.is_empty() {
                continue;
            }
            let pr = &r.props;
            runs.push(dm::Run {
                text,
                font: pr.font.clone(),
                size: pr.size.map(f64::from),
                bold: pr.bold,
                italic: pr.italic,
                underline: pr.underline.map(|u| u != kition_doc_doc::props::Underline::None),
                color: match pr.color {
                    Some(kition_doc_doc::props::TextColor::Rgb(c)) => Some(dm::Color::rgb(dm::Rgb(c.0, c.1, c.2))),
                    _ => None,
                },
            });
        }
        let align = p.props.align.and_then(|a| match a {
            kition_doc_doc::props::Align::Left => Some(dm::Align::Left),
            kition_doc_doc::props::Align::Center => Some(dm::Align::Center),
            kition_doc_doc::props::Align::Right => Some(dm::Align::Right),
            kition_doc_doc::props::Align::Justify => Some(dm::Align::Justify),
            _ => None,
        });
        paragraphs.push(dm::Paragraph { align, runs });
    }
    dm::TextBody { paragraphs, anchor: dm::TextAnchor::Top, ..dm::TextBody::plain("") }
}

/// The document's theme colour `i` (0 dk1, 1 lt1, …), or the default theme's.
fn theme_slot(s: &Session, i: usize) -> Rgb {
    s.doc.settings.theme_colors.get(i).or_else(|| kition_doc_doc::THEME_COLORS.get(i)).copied().unwrap_or(Rgb::BLACK)
}

/// The shared description's text as a text box's story: a paragraph per paragraph, a run per run.
fn story_blocks(t: &dm::TextBody) -> Result<kition_doc_doc::Blocks, DrawingError> {
    let mut blocks = Vec::new();
    for p in &t.paragraphs {
        let mut para = Paragraph::new();
        for r in &p.runs {
            let text: String = r.text.chars().filter(|c| *c != kition_doc_doc::para::OBJ && *c != '\n').collect();
            if text.is_empty() {
                continue;
            }
            let props = CharProps {
                font: r.font.clone(),
                size: r.size.map(|v| v as f32),
                bold: r.bold,
                italic: r.italic,
                underline: r.underline.map(|u| if u { kition_doc_doc::props::Underline::Single } else { kition_doc_doc::props::Underline::None }),
                color: r.color.as_ref().map(|c| rgb_of(c, "text.color").map(kition_doc_doc::props::TextColor::Rgb)).transpose()?,
                ..CharProps::default()
            };
            if para.runs.is_empty() {
                para.mark = props.clone();
            }
            para.runs.push(kition_doc_doc::para::Run { len: text.len(), props });
            para.text.push_str(&text);
        }
        para.props.align = p.align.map(|a| match a {
            dm::Align::Left => kition_doc_doc::props::Align::Left,
            dm::Align::Center => kition_doc_doc::props::Align::Center,
            dm::Align::Right => kition_doc_doc::props::Align::Right,
            dm::Align::Justify => kition_doc_doc::props::Align::Justify,
        });
        para.normalize();
        blocks.push(para_block(para));
    }
    if blocks.is_empty() {
        blocks.push(para_block(Paragraph::new()));
    }
    Ok(blocks)
}

/// Where layout puts an object: its frame in points on its page, and its placement.
fn frame_of(s: &mut Session, pos: &Pos, w: f32, h: f32, float: &Float) -> (Xfrm, dm::Placement) {
    let hit = s.layout().object(pos, s.page_hint);
    let spin = float.spin();
    let (page, x, y) = hit.as_ref().map_or((0, 0.0, 0.0), |h| (h.page, f64::from(h.rect.x), f64::from(h.rect.y)));
    let frame = Xfrm { x, y, w: f64::from(w), h: f64::from(h), rot: f64::from(spin.deg), flip_h: spin.flip_h, flip_v: spin.flip_v };
    let mut placement =
        dm::Placement { kind: if float.wrap == Wrap::Inline { dm::PlacementKind::Inline } else { dm::PlacementKind::Float }, ..Default::default() };
    placement.extra.insert("page".into(), json!(page));
    placement.extra.insert("wrap".into(), serde_json::to_value(float.wrap).unwrap_or(Value::Null));
    (frame, placement)
}

pub(super) fn rgb_of(c: &dm::Color, property: &str) -> Result<Rgb, DrawingError> {
    match &c.base {
        dm::ColorBase::Rgb { rgb } if c.mods.is_empty() && c.alpha.is_none_or(|a| a >= 1.0) => Ok(Rgb(rgb.0, rgb.1, rgb.2)),
        dm::ColorBase::Rgb { .. } => Err(DrawingError::unsupported(property, "Word shapes take an opaque RGB colour without transforms")),
        dm::ColorBase::Theme { .. } => Err(DrawingError::unsupported(property, "Word shapes take an RGB colour, not a theme colour")),
    }
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let DrawingOp::Insert(o) = DrawingOp::parse("drawing.insert", v).map_err(|e| fail("drawing.insert", e))? else {
        return Err(CmdError::Params("drawing.insert: not an insert".into()));
    };
    // How text wraps around it (Word's Wrap Text, as `drawing.describe` gives it), checked first.
    let wrap = match v.get("placement").and_then(|p| p.get("wrap")).filter(|w| !w.is_null()) {
        None => None,
        Some(w) => Some(serde_json::from_value::<Wrap>(w.clone()).map_err(|_| {
            fail(
                "drawing.insert",
                DrawingError::invalid("placement.wrap is inline, square, tight, through, topAndBottom, behindText or inFrontOfText"),
            )
        })?),
    };
    // A selected object stays: the new one goes beside it, as in Word (not over the selection).
    if super::objects::selected(s).is_some() {
        s.sel = Selection::caret(s.sel.ordered().1);
    }
    let has_frame = v.get("frame").is_some();
    let (w, h) = if has_frame {
        (o.frame.w, o.frame.h)
    } else {
        match o.kind {
            dm::ObjectKind::TextBox => (144.0, 72.0),
            // As Word inserts one: 6 by 3.5 inches, narrowed to fit the column later by layout.
            dm::ObjectKind::Chart => (432.0, 252.0),
            dm::ObjectKind::Shape | dm::ObjectKind::Ink | dm::ObjectKind::Group => (108.0, 72.0),
        }
    };
    if o.kind == dm::ObjectKind::Ink {
        return Err(fail("drawing.insert", DrawingError::unsupported("kind", "ink is drawn with drawing.ink")));
    }
    let created = if let Some(spec) = o.chart.as_ref().filter(|_| o.kind == dm::ObjectKind::Chart) {
        if o.frame.rot != 0.0 || o.frame.flip_h || o.frame.flip_v {
            return Err(fail("drawing.insert", DrawingError::unsupported("frame.rot", "a chart does not turn")));
        }
        kition_office_core::ops::check_frame(&o.frame).map_err(|e| fail("drawing.insert", e))?;
        if !(4.0..=2000.0).contains(&w) || !(4.0..=2000.0).contains(&h) {
            return Err(fail("drawing.insert", DrawingError::invalid("a chart is 4 to 2000 pt wide and high")));
        }
        let graphic = Graphic { kind: GraphicKind::Chart, chart: Some(std::sync::Arc::new(codec::write(spec))), ..Default::default() };
        let graphic = chart_drawn(s, graphic, w as f32, h as f32);
        let props = s.typing_props();
        let at = super::delete_selection(s)?;
        let obj = InlineObject::Graphic {
            w: w as f32,
            h: h as f32,
            alt: String::new(),
            float: Float { wrap: Wrap::Inline, ..Default::default() },
            graphic: std::sync::Arc::new(graphic),
        };
        let end = s.doc.insert_object(&at, obj, &props)?;
        s.sel = Selection { anchor: at.clone(), focus: end };
        Some(at)
    } else if o.kind == dm::ObjectKind::TextBox {
        s.run("textBox.insert", &json!({ "width": w, "height": h }))?;
        // `textBox.insert` puts the caret in the box; the new object is its anchor's.
        let part = s.sel.focus.story;
        s.doc
            .shapes()
            .into_iter()
            .find(|(_, x)| matches!(x, InlineObject::Shape { story: Some(id), .. } if StoryRef::Part(*id) == part))
            .map(|(p, _)| p)
    } else {
        s.run("shape.insert", &json!({ "kind": "rectangle", "width": w, "height": h }))?;
        super::objects::selected(s).map(|(p, _)| p)
    };
    let pos = created.ok_or_else(|| CmdError::Failed("drawing.insert: the new shape was not found".into()))?;
    s.doc.assign_object_ids();
    let obj = s
        .doc
        .para_at(&pos)
        .and_then(|p| p.object_at(pos.off))
        .cloned()
        .ok_or_else(|| CmdError::Failed("drawing.insert: the new shape vanished".into()))?;
    let uid = uid_of(&obj);
    let before = to_drawing(s, &pos, &obj);
    let mut after = before.clone();
    if o.kind == dm::ObjectKind::Chart {
        if has_frame && v.get("frame").is_some_and(|f| f.get("x").is_some() || f.get("y").is_some()) {
            after.frame = o.frame;
            write_back(s, "drawing.insert", uid, &before, &after, true)?;
        }
        if let Some(wrap) = wrap {
            set_wrap(s, uid, wrap)?;
        }
        // The caret stays after the chart, so what is typed next (a new paragraph) follows it
        // rather than replacing it; the chart is still the object at the caret.
        if let Some(pos) = s.doc.find_shape(uid) {
            s.sel = Selection::caret(Pos { off: pos.off + kition_doc_doc::para::OBJ.len_utf8(), ..pos });
        }
        return Ok(json!({ "id": uid.to_string() }));
    }
    after.geometry = o.geometry.clone();
    if v.get("fill").is_some() {
        after.fill.clone_from(&o.fill);
    }
    if v.get("stroke").is_some() {
        after.stroke = o
            .stroke
            .clone()
            .map(|st| kition_office_core::ops::DrawingOp::Format {
                ids: vec![],
                format: kition_office_core::ops::Format { stroke: Some(st), ..Default::default() },
            })
            .and_then(|op| {
                let mut x = before.clone();
                op.apply(&mut x).ok()?;
                x.stroke
            });
    }
    if o.text.is_some() {
        after.text.clone_from(&o.text);
    }
    if has_frame {
        after.frame = o.frame;
        after.frame.rot = o.frame.rot;
    }
    // The page the frame is on, when named (a shape drawn with the pointer on a later page).
    if let Some(page) = v.get("placement").and_then(|p| p.get("page")).filter(|p| p.is_u64()) {
        after.placement.extra.insert("page".into(), page.clone());
    }
    write_back(s, "drawing.insert", uid, &before, &after, has_frame)?;
    if let Some(wrap) = wrap {
        set_wrap(s, uid, wrap)?;
    }
    Ok(json!({ "id": uid.to_string() }))
}

/// Object `uid` wrapped `wrap`: a floating one keeps its place, an inline one starts floating at
/// its anchor (or a floating one goes in line).
fn set_wrap(s: &mut Session, uid: u32, wrap: Wrap) -> Result<(), CmdError> {
    let pos = s.doc.find_shape(uid).ok_or_else(|| fail("drawing.insert", DrawingError::not_found(&uid.to_string())))?;
    edit(s, &pos, |o| {
        if let Some(f) = o.float_mut() {
            f.wrap = wrap;
        }
    })
}

fn apply(s: &mut Session, cmd: &str, v: &Value) -> CmdResult {
    let op = DrawingOp::parse(cmd, v).map_err(|e| fail(cmd, e))?;
    let ids: Vec<u32> = op
        .ids()
        .iter()
        .map(|id| id.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(|| fail(cmd, DrawingError::not_found(id))))
        .collect::<Result<_, _>>()?;
    // Every object first, so a bad id or value changes nothing.
    let mut changes = Vec::new();
    for uid in &ids {
        let (pos, o) = find(s, cmd, *uid)?;
        let before = to_drawing(s, &pos, &o);
        let mut after = before.clone();
        op.apply(&mut after).map_err(|e| fail(cmd, e))?;
        changes.push((*uid, before, after));
    }
    if let DrawingOp::Delete(_) = op {
        for uid in &ids {
            let (pos, _) = find(s, cmd, *uid)?;
            let end = Pos { off: pos.off + kition_doc_doc::para::OBJ.len_utf8(), ..pos.clone() };
            s.doc.delete_range(&pos, &end)?;
        }
        s.doc.prune_text_boxes();
        s.sel = Selection::caret(s.doc.clamp(&s.sel.focus));
        return Ok(json!({ "deleted": ids.len() }));
    }
    let mut frames = Vec::new();
    for (uid, before, after) in &changes {
        let moved = after.frame.x != before.frame.x || after.frame.y != before.frame.y;
        write_back(s, cmd, *uid, before, after, moved)?;
        let (pos, o) = find(s, cmd, *uid)?;
        frames.push(json!(to_drawing(s, &pos, &o).frame));
    }
    reroute_connectors(s)?;
    Ok(json!({ "frames": frames }))
}

/// Writes what differs between `before` and `after` into shape `uid`. `moved`: put it where the
/// frame says (it floats, as a drag makes it).
fn write_back(s: &mut Session, cmd: &str, uid: u32, before: &DrawingObject, after: &DrawingObject, moved: bool) -> Result<(), CmdError> {
    kition_office_core::ops::check_frame(&after.frame).map_err(|e| fail(cmd, e))?;
    if before.kind == dm::ObjectKind::Chart {
        return chart_back(s, cmd, uid, before, after, moved);
    }
    // A group moves, resizes and turns with its members; its members keep their own looks.
    if before.kind == dm::ObjectKind::Group
        && (DrawingObject { frame: before.frame, placement: before.placement.clone(), ..after.clone() }) != *before
    {
        return Err(fail(cmd, DrawingError::unsupported("kind", "a group's members are formatted one by one")));
    }
    // Ink moves and resizes like a shape; its look is its pen's (`drawing.ink`).
    if before.kind == dm::ObjectKind::Ink {
        for (property, changed) in [
            ("fill", after.fill != before.fill),
            ("stroke", after.stroke != before.stroke),
            ("geometry", after.geometry != before.geometry),
            ("text", after.text != before.text),
        ] {
            if changed {
                return Err(fail(cmd, DrawingError::unsupported(property, "ink keeps its pen's look")));
            }
        }
    }
    // Check every value before changing anything.
    // A gradient: its stops' colours, its first colour the solid stand-in.
    let mut gradient: Option<Option<std::sync::Arc<kition_doc_doc::graphic::ShapeGradient>>> = None;
    let fill = match (&after.fill, after.fill != before.fill) {
        (_, false) => None,
        (Some(dm::Fill::Solid(c)), true) => {
            gradient = Some(None);
            Some(Some(rgb_of(c, "fill").map_err(|e| fail(cmd, e))?))
        }
        (Some(dm::Fill::Gradient(g)), true) => {
            let stops = g
                .stops
                .iter()
                .map(|st| Ok(kition_doc_doc::graphic::GradientStop { pos: st.pos as f32, color: rgb_of(&st.color, "fill.gradient")? }))
                .collect::<Result<Vec<_>, DrawingError>>()
                .map_err(|e| fail(cmd, e))?;
            let first = stops.first().map(|s| s.color);
            gradient = Some(Some(std::sync::Arc::new(kition_doc_doc::graphic::ShapeGradient { stops, angle: g.angle as f32, path: g.path.clone() })));
            Some(first)
        }
        (Some(dm::Fill::None) | None, true) => {
            gradient = Some(None);
            Some(None)
        }
        (Some(dm::Fill::Other(kind)), true) => return Err(fail(cmd, DrawingError::unsupported("fill", format!("a {kind} fill")))),
    };
    let stroke = match (&after.stroke, after.stroke != before.stroke) {
        (Some(st), true) => {
            let dash = match st.dash.as_deref() {
                None | Some("solid") => None,
                Some(d) => Some(
                    kition_doc_doc::graphic::LineDash::parse(d)
                        .ok_or_else(|| fail(cmd, DrawingError::unsupported("stroke.dash", format!("dash `{d}` is not drawn"))))?,
                ),
            };
            let color = if st.none {
                None
            } else {
                st.color.as_ref().map(|c| rgb_of(c, "stroke.color")).transpose().map_err(|e| fail(cmd, e))?.or(Some(Rgb::BLACK))
            };
            Some((color, st.width.map(|w| w as f32), dash))
        }
        _ => None,
    };
    let geom = match (&after.geometry, after.geometry != before.geometry) {
        (dm::Geometry::Preset { preset, adjust }, true) => {
            if kition_office_core::preset::info(preset).is_none() {
                return Err(fail(cmd, DrawingError::unsupported("geometry.preset", format!("preset `{preset}` is not drawn"))));
            }
            Some((preset.clone(), adjust.iter().map(|(k, v)| (k.clone(), *v)).collect::<Vec<_>>()))
        }
        (dm::Geometry::Custom { .. }, true) => {
            return Err(fail(cmd, DrawingError::unsupported("geometry.paths", "custom geometry is drawn with the Draw tab")));
        }
        _ => None,
    };
    // The words, the warp (art text, DR26) and the frame (anchor, margins) change apart: a new warp
    // or frame keeps the text's formatting.
    let unbent = |t: &Option<dm::TextBody>| {
        t.clone().map(|t| dm::TextBody { warp: None, anchor: dm::TextAnchor::Top, insets: dm::TextBody::default().insets, ..t })
    };
    let text = match (&after.text, unbent(&after.text) != unbent(&before.text)) {
        (Some(t), true) => Some(story_blocks(t).map_err(|e| fail(cmd, e))?),
        _ => None,
    };
    let warp_of = |t: &Option<dm::TextBody>| t.as_ref().and_then(|t| t.warp.clone());
    let warp = (warp_of(&after.text) != warp_of(&before.text))
        .then(|| warp_of(&after.text).map(|w| kition_doc_doc::graphic::PresetGeom { name: w.preset, adj: w.adjust.into_iter().collect() }));
    let frame_of_text = |t: &Option<dm::TextBody>| t.as_ref().map(|t| (t.anchor, t.insets));
    let text_frame = (frame_of_text(&after.text) != frame_of_text(&before.text)).then(|| {
        after.text.as_ref().and_then(|t| {
            let frame = TextFrame {
                anchor: match t.anchor {
                    dm::TextAnchor::Top => TextAnchor::Top,
                    dm::TextAnchor::Middle => TextAnchor::Middle,
                    dm::TextAnchor::Bottom => TextAnchor::Bottom,
                },
                insets: t.insets.map(|v| v.clamp(0.0, 1584.0) as f32),
                fit: false,
                no_wrap: false,
                fit_height: false,
            };
            (frame != TextFrame::default()).then_some(frame)
        })
    });
    // Shadow and glow: their colours in RGB with an opacity.
    let effects = if after.effects != before.effects {
        let transparency = |c: &dm::Color| (100.0 - c.alpha.unwrap_or(1.0).clamp(0.0, 1.0) * 100.0) as f32;
        let e = after.effects.clone().unwrap_or_default();
        let shadow = e
            .shadow
            .map(|s| {
                Ok::<_, DrawingError>(kition_doc_doc::effects::Shadow {
                    color: rgb_of(&dm::Color { alpha: None, ..s.color.clone() }, "effects.shadow.color")?,
                    transparency: transparency(&s.color),
                    blur: s.blur as f32,
                    distance: s.distance as f32,
                    angle: s.angle as f32,
                    rot_with_shape: false,
                })
            })
            .transpose()
            .map_err(|e| fail(cmd, e))?;
        let glow = e
            .glow
            .map(|g| {
                Ok::<_, DrawingError>(kition_doc_doc::effects::Glow {
                    color: rgb_of(&dm::Color { alpha: None, ..g.color.clone() }, "effects.glow.color")?,
                    size: g.size as f32,
                    transparency: transparency(&g.color),
                })
            })
            .transpose()
            .map_err(|e| fail(cmd, e))?;
        Some((shadow, glow))
    } else {
        None
    };
    // A connector's glued ends (`drawing.reconnect`): to shapes that are there.
    let connection = if after.connection != before.connection {
        let end = |e: &Option<kition_office_core::connect::End>| -> Result<Option<(u32, u32)>, CmdError> {
            let Some(e) = e else { return Ok(None) };
            let uid =
                e.id.parse::<u32>()
                    .ok()
                    .filter(|u| *u > 0 && s.doc.find_shape(*u).is_some())
                    .ok_or_else(|| fail(cmd, DrawingError::not_found(&e.id)))?;
            Ok(Some((uid, u32::try_from(e.site).unwrap_or(0))))
        };
        let c = after.connection.clone().unwrap_or_default();
        let (start, end) = (end(&c.start)?, end(&c.end)?);
        Some((start.is_some() || end.is_some()).then_some(kition_doc_doc::graphic::Connection { start, end }))
    } else {
        None
    };
    let spun = (after.frame.rot, after.frame.flip_h, after.frame.flip_v) != (before.frame.rot, before.frame.flip_h, before.frame.flip_v);
    let sized = (after.frame.w, after.frame.h) != (before.frame.w, before.frame.h);
    let (pos, _) = find(s, cmd, uid)?;
    // Text into the shape's story (a story made for it when it has none).
    let mut first_text = false;
    if let Some(mut text) = text {
        let story = match s.doc.para_at(&pos).and_then(|p| p.object_at(pos.off)) {
            Some(InlineObject::Shape { story: Some(id), .. }) => *id,
            shape => {
                // A shape's first text looks as in a shape Word inserts: centred, in white (its
                // style's font colour, lt1); a text box's is plain.
                if matches!(shape, Some(InlineObject::Shape { kind, .. }) if *kind != ShapeKind::TextBox) {
                    first_text = true;
                    let white = kition_doc_doc::props::TextColor::Rgb(theme_slot(s, 1));
                    for block in &mut text {
                        let Block::Para(p) = std::sync::Arc::make_mut(block) else { continue };
                        p.props.align.get_or_insert(kition_doc_doc::props::Align::Center);
                        for props in p.runs.iter_mut().map(|r| &mut r.props).chain(std::iter::once(&mut p.mark)) {
                            props.color.get_or_insert(white);
                        }
                    }
                }
                let id = s.doc.add_part(PartKind::TextBox, vec![para_block(Paragraph::new())]);
                edit(s, &pos, |o| {
                    if let InlineObject::Shape { story, .. } = o {
                        *story = Some(id);
                    }
                })?;
                id
            }
        };
        if let Some(part) = s.doc.parts.get_mut(&story) {
            part.blocks = text;
        }
    }
    edit(s, &pos, |o| {
        if let InlineObject::Shape {
            kind,
            fill: f,
            stroke: st,
            stroke_width,
            geom: g,
            float,
            gradient: gr,
            effects: fx,
            warp: wp,
            connection: cn,
            body: bd,
            dash: dsh,
            ..
        } = o
        {
            if let Some(next) = text_frame {
                // A box that fits its text, or doesn't wrap it, keeps doing so with its new anchor
                // and margins.
                let (fit, no_wrap) = bd.map_or((false, false), |b| (b.fit, b.no_wrap));
                *bd = next
                    .map(|f| TextFrame { fit, no_wrap, ..f })
                    .or_else(|| (fit || no_wrap).then(|| TextFrame { fit, no_wrap, ..TextFrame::default() }));
            }
            // ...and sits in the middle, unless the text came with a frame of its own.
            if first_text && bd.is_none() {
                *bd = Some(TextFrame { anchor: TextAnchor::Middle, ..TextFrame::default() });
            }
            if let Some(c) = fill {
                *f = c;
            }
            if let Some(next) = warp.clone() {
                *wp = next;
            }
            if let Some(next) = connection {
                *cn = next;
            }
            if let Some((shadow, glow)) = effects {
                fx.shadow = shadow;
                fx.glow = glow;
            }
            if let Some(next) = gradient.clone() {
                *gr = next;
            }
            if let Some((c, w, d)) = stroke {
                *st = c;
                *dsh = d;
                if let Some(w) = w {
                    *stroke_width = w.clamp(0.0, 72.0);
                }
            }
            if let Some((name, adj)) = &geom {
                let canonical = [
                    ShapeKind::Rectangle,
                    ShapeKind::RoundedRectangle,
                    ShapeKind::Ellipse,
                    ShapeKind::Triangle,
                    ShapeKind::Diamond,
                    ShapeKind::Arrow,
                    ShapeKind::Star,
                    ShapeKind::Heart,
                ]
                .into_iter()
                .find(|k| k.preset_name() == name && adj.is_empty());
                match canonical {
                    // A text box keeps being one whatever its outline.
                    Some(k) if *kind != ShapeKind::TextBox => {
                        *kind = k;
                        *g = None;
                    }
                    _ => *g = Some(kition_doc_doc::graphic::PresetGeom { name: name.clone(), adj: adj.clone() }),
                }
            }
            if spun {
                float.set_spin(kition_doc_geom::Spin::new(after.frame.rot as f32, after.frame.flip_h, after.frame.flip_v));
            }
        }
        if let InlineObject::Group { float, .. } = o
            && spun
        {
            float.set_spin(kition_doc_geom::Spin::new(after.frame.rot as f32, after.frame.flip_h, after.frame.flip_v));
        }
    })?;
    // Size and place through the editor's own Move or Resize, on the object selected.
    if sized || moved {
        let end = Pos { off: pos.off + kition_doc_doc::para::OBJ.len_utf8(), ..pos.clone() };
        s.sel = Selection { anchor: pos.clone(), focus: end };
        let mut params = json!({ "width": after.frame.w, "height": after.frame.h });
        if moved {
            let page = after
                .placement
                .extra
                .get("page")
                .and_then(Value::as_u64)
                .or_else(|| before.placement.extra.get("page").and_then(Value::as_u64))
                .unwrap_or(0);
            params["x"] = json!(after.frame.x);
            params["y"] = json!(after.frame.y);
            params["page"] = json!(page);
        }
        s.run("arrange.bounds", &params)?;
    }
    Ok(())
}

/// Writes what differs between `before` and `after` into chart `uid`: its frame (a chart does not
/// turn, and has no fill, outline, outline shape or text of a shape's) and its part.
fn chart_back(s: &mut Session, cmd: &str, uid: u32, before: &DrawingObject, after: &DrawingObject, moved: bool) -> Result<(), CmdError> {
    if (after.frame.rot, after.frame.flip_h, after.frame.flip_v) != (before.frame.rot, before.frame.flip_h, before.frame.flip_v) {
        return Err(fail(cmd, DrawingError::unsupported("frame.rot", "a chart does not turn")));
    }
    for (property, changed) in [
        ("fill", after.fill != before.fill),
        ("stroke", after.stroke != before.stroke),
        ("geometry", after.geometry != before.geometry),
        ("text", after.text != before.text),
    ] {
        if changed {
            return Err(fail(cmd, DrawingError::unsupported(property, "a chart's look is changed with drawing.chart")));
        }
    }
    let (pos, o) = find(s, cmd, uid)?;
    let InlineObject::Graphic { graphic, .. } = &o else { return Err(fail(cmd, DrawingError::not_found(&uid.to_string()))) };
    if after.chart != before.chart {
        let spec = after.chart.as_ref().ok_or_else(|| fail(cmd, DrawingError::unsupported("chart", "this chart's part could not be read")))?;
        let new = chart_changed(s, graphic, spec, before.frame.w as f32, before.frame.h as f32).map_err(|e| fail(cmd, e))?;
        edit(s, &pos, |o| {
            if let InlineObject::Graphic { graphic, .. } = o {
                *graphic = std::sync::Arc::new(new);
            }
        })?;
    }
    let sized = (after.frame.w, after.frame.h) != (before.frame.w, before.frame.h);
    if sized || moved {
        let end = Pos { off: pos.off + kition_doc_doc::para::OBJ.len_utf8(), ..pos.clone() };
        s.sel = Selection { anchor: pos.clone(), focus: end };
        let mut params = json!({ "width": after.frame.w, "height": after.frame.h });
        if moved {
            let page = after
                .placement
                .extra
                .get("page")
                .and_then(Value::as_u64)
                .or_else(|| before.placement.extra.get("page").and_then(Value::as_u64))
                .unwrap_or(0);
            params["x"] = json!(after.frame.x);
            params["y"] = json!(after.frame.y);
            params["page"] = json!(page);
        }
        s.run("arrange.bounds", &params)?;
    }
    // A chart made or edited here is drawn at its size, not scaled (its text keeps its size).
    if sized
        && let Ok((pos, InlineObject::Graphic { w, h, graphic, .. })) = find(s, cmd, uid)
        && (graphic.chart.is_some() || graphic.spec.is_some())
    {
        let redrawn = chart_drawn(s, (*graphic).clone(), w, h);
        edit(s, &pos, |o| {
            if let InlineObject::Graphic { graphic, .. } = o {
                *graphic = std::sync::Arc::new(redrawn);
            }
        })?;
    }
    Ok(())
}

fn edit(s: &mut Session, pos: &Pos, f: impl FnOnce(&mut InlineObject)) -> Result<(), CmdError> {
    let para = s.doc.para_mut(pos.story, &pos.path)?;
    let o = para.object_at_mut(pos.off).ok_or_else(|| CmdError::Failed("the shape vanished".into()))?;
    f(o);
    para.touch();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_doc_doc::Document;

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        s.run("text.insert", &json!({ "text": "Some text before the shapes. " })).unwrap();
        s
    }

    #[test]
    fn floating_objects_move_in_the_stack_in_one_step_and_reopen() {
        let mut s = session();
        // Each insert selects the new object; the next goes in after it, not over it.
        let ins = |s: &mut Session, preset: &str, x: f64| -> String {
            let id = s.run("drawing.insert", &json!({ "geometry": { "preset": preset }, "frame": { "x": x, "y": 300, "w": 120, "h": 80 } })).unwrap()
                ["id"]
                .as_str()
                .unwrap()
                .to_string();
            s.run("select.collapse", &json!({ "end": true })).unwrap();
            id
        };
        let (a, b, c) = (ins(&mut s, "rect", 100.0), ins(&mut s, "ellipse", 140.0), ins(&mut s, "triangle", 180.0));
        let order = |s: &mut Session| -> Vec<String> {
            s.sel = Selection::caret(Pos::body(0, 0));
            let d = s.run("drawing.describe", &json!({})).unwrap();
            d["objects"].as_array().unwrap().iter().map(|o| o["id"].as_str().unwrap().to_string()).collect()
        };
        // Each new one goes on top.
        assert_eq!(order(&mut s), [a.clone(), b.clone(), c.clone()]);
        s.run("drawing.arrange", &json!({ "ids": [a], "order": "front" })).unwrap();
        assert_eq!(order(&mut s), [b.clone(), c.clone(), a.clone()]);
        s.run("drawing.arrange", &json!({ "ids": [a], "order": "backward" })).unwrap();
        assert_eq!(order(&mut s), [b.clone(), a.clone(), c.clone()]);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(order(&mut s), [b.clone(), c.clone(), a.clone()]);
        // Layout draws them in that order: the rectangle's outline last.
        let layout = kition_doc_layout::layout(&s.doc, &mut kition_doc_layout::LayoutCache::default(), &kition_doc_layout::LayoutOptions::default());
        let shapes: Vec<u32> = layout.pages[0]
            .items
            .iter()
            .filter_map(|it| {
                if let kition_doc_layout::Placed::Object { story, path, off, .. } = it {
                    s.doc.para(*story, path).and_then(|p| p.object_at(*off)).and_then(InlineObject::object_uid)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(shapes.iter().map(u32::to_string).collect::<Vec<_>>(), [b.clone(), c.clone(), a.clone()]);
        // An inline object has no place in the stack; an unknown id changes nothing.
        s.run("select.collapse", &json!({ "end": true })).unwrap();
        let inline = s.run("drawing.insert", &json!({ "geometry": { "preset": "rect" } })).unwrap()["id"].as_str().unwrap().to_string();
        let e = s.run("drawing.arrange", &json!({ "ids": [inline], "order": "front" })).unwrap_err();
        assert!(e.to_string().contains("unsupported"), "{e}");
        assert!(s.run("drawing.arrange", &json!({ "ids": ["999"], "order": "front" })).is_err());
        s.run("drawing.delete", &json!({ "ids": [inline] })).unwrap();
        // Saved and reopened, the stack stays.
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        assert_eq!(order(&mut back), [b, c, a]);
    }

    #[test]
    fn floating_objects_group_move_together_ungroup_and_reopen() {
        let mut s = session();
        let ins = |s: &mut Session, preset: &str, x: f64| -> String {
            let id = s.run("drawing.insert", &json!({ "geometry": { "preset": preset }, "frame": { "x": x, "y": 300, "w": 80, "h": 60 } })).unwrap()
                ["id"]
                .as_str()
                .unwrap()
                .to_string();
            s.run("select.collapse", &json!({ "end": true })).unwrap();
            id
        };
        let (a, b, c) = (ins(&mut s, "rect", 100.0), ins(&mut s, "ellipse", 300.0), ins(&mut s, "triangle", 450.0));
        let g = s.run("drawing.group", &json!({ "ids": [a, b] })).unwrap()["id"].as_str().unwrap().to_string();
        let group = obj(&mut s, &g);
        assert_eq!(group.kind, dm::ObjectKind::Group);
        assert_eq!(group.members.as_deref(), Some(&[a.clone(), b.clone()][..]), "members keep their ids");
        assert!((group.frame.x - 100.0).abs() < 0.5 && (group.frame.w - 280.0).abs() < 0.5, "{:?}", group.frame);
        // A member is still described, where the group shows it.
        let m = obj(&mut s, &b);
        assert!((m.frame.x - 300.0).abs() < 0.5 && m.geometry == dm::Geometry::preset("ellipse"), "{:?}", m.frame);
        // Moving the group carries its members; a member is changed through its group.
        s.run("drawing.transform", &json!({ "ids": [g], "dx": 40 })).unwrap();
        assert!((obj(&mut s, &b).frame.x - 340.0).abs() < 0.5);
        assert!(s.run("drawing.format", &json!({ "ids": [g], "fill": { "color": "#FF0000" } })).unwrap_err().to_string().contains("unsupported"));
        assert!(s.run("drawing.transform", &json!({ "ids": [a], "dx": 5 })).unwrap_err().to_string().contains("unsupported"));
        assert!(s.run("drawing.group", &json!({ "ids": [c] })).is_err());
        assert!(s.run("drawing.ungroup", &json!({ "ids": [c] })).unwrap_err().to_string().contains("unsupported"));
        // Saved and reopened: the group and its members keep their ids.
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        let reopened = obj(&mut back, &g);
        assert_eq!((reopened.kind, reopened.members.clone()), (dm::ObjectKind::Group, Some(vec![a.clone(), b.clone()])));
        // Ungrouping puts the members back where they appear, in one step.
        let r = s.run("drawing.ungroup", &json!({ "ids": [g] })).unwrap();
        assert_eq!(r["ids"], json!([a, b]));
        assert!((obj(&mut s, &b).frame.x - 340.0).abs() < 0.5);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(obj(&mut s, &g).kind, dm::ObjectKind::Group);
    }

    #[test]
    fn gradient_fills_format_describe_undo_and_reopen() {
        let mut s = session();
        let id = s.run("drawing.insert", &json!({ "geometry": { "preset": "rect" }, "frame": { "x": 100, "y": 300, "w": 200, "h": 100 } })).unwrap()
            ["id"]
            .as_str()
            .unwrap()
            .to_string();
        let gradient = json!({ "gradient": { "stops": [{ "pos": 0, "color": "#204080" }, { "pos": 1, "color": "#C7CFDF" }], "angle": 90 } });
        s.run("drawing.format", &json!({ "ids": [id], "fill": gradient })).unwrap();
        let expected: dm::Fill = serde_json::from_value(gradient).unwrap();
        assert_eq!(obj(&mut s, &id).fill.as_ref(), Some(&expected));
        let radial = json!({ "gradient": { "stops": [{ "pos": 0, "color": "#FFFFFF" }, { "pos": 1, "color": "#000000" }], "path": "circle" } });
        s.run("drawing.format", &json!({ "ids": [id], "fill": radial })).unwrap();
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(obj(&mut s, &id).fill.as_ref(), Some(&expected));
        // A theme colour Word shapes do not keep is refused; a solid fill replaces the gradient.
        assert!(s.run("drawing.format", &json!({ "ids": [id], "fill": { "gradient": { "stops": [{ "pos": 0, "color": { "theme": "accent1" } }, { "pos": 1, "color": "#000000" }] } } })).is_err());
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        assert_eq!(obj(&mut back, &id).fill, Some(expected));
        s.run("drawing.format", &json!({ "ids": [id], "fill": { "color": "#FF0000" } })).unwrap();
        assert_eq!(obj(&mut s, &id).fill, Some(dm::Fill::Solid(dm::Color::rgb(dm::Rgb(255, 0, 0)))));
    }

    #[test]
    fn shadows_and_glows_format_describe_undo_and_reopen() {
        let mut s = session();
        let id = s.run("drawing.insert", &json!({ "geometry": { "preset": "rect" }, "frame": { "x": 100, "y": 300, "w": 200, "h": 100 } })).unwrap()
            ["id"]
            .as_str()
            .unwrap()
            .to_string();
        s.run(
            "drawing.format",
            &json!({ "ids": [id], "effects": { "shadow": { "color": { "rgb": "#000000", "alpha": 0.4 }, "blur": 4, "distance": 3, "angle": 45 }, "glow": { "color": { "rgb": "#5645D4", "alpha": 0.6 }, "size": 6 } } }),
        )
        .unwrap();
        let e = obj(&mut s, &id).effects.unwrap();
        let shadow = e.shadow.clone().unwrap();
        assert_eq!((shadow.blur, shadow.distance, shadow.angle, shadow.color.alpha), (4.0, 3.0, 45.0, Some(0.4)));
        assert_eq!(e.glow.as_ref().map(|g| (g.size, g.color.alpha)), Some((6.0, Some(0.6))));
        s.run("drawing.format", &json!({ "ids": [id], "effects": { "shadow": null } })).unwrap();
        assert!(obj(&mut s, &id).effects.is_some_and(|e| e.shadow.is_none() && e.glow.is_some()));
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(obj(&mut s, &id).effects, Some(e.clone()));
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        assert_eq!(obj(&mut back, &id).effects, Some(e));
        assert!(s.run("drawing.format", &json!({ "ids": [id], "effects": { "glow": { "color": { "theme": "accent1" } } } })).is_err());
    }

    #[test]
    fn shape_text_takes_runs_and_alignment_and_reopens() {
        let mut s = session();
        let id = s
            .run("drawing.insert", &json!({ "geometry": { "preset": "rect" }, "frame": { "x": 100, "y": 300, "w": 240, "h": 120 }, "text": "x" }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let paragraphs = json!([
            { "align": "center", "runs": [{ "text": "Q3 " }, { "text": "results", "bold": true, "size": 18, "color": "#C00000" }] },
            { "runs": [{ "text": "up 12%", "italic": true, "underline": true, "font": "Arial" }] }
        ]);
        s.run("drawing.text", &json!({ "id": id, "paragraphs": paragraphs })).unwrap();
        let t = obj(&mut s, &id).text.unwrap();
        assert_eq!(t.paragraphs.len(), 2);
        assert_eq!(t.paragraphs[0].align, Some(dm::Align::Center));
        let r = &t.paragraphs[0].runs[1];
        assert_eq!(
            (r.text.as_str(), r.bold, r.size, r.color.clone()),
            ("results", Some(true), Some(18.0), Some(dm::Color::rgb(dm::Rgb(0xC0, 0, 0))))
        );
        let r = &t.paragraphs[1].runs[0];
        assert_eq!((r.italic, r.underline, r.font.as_deref()), (Some(true), Some(true), Some("Arial")));
        assert_eq!(t.text(), "Q3 results\nup 12%");
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(obj(&mut s, &id).text.unwrap().text(), "x");
        s.run("edit.redo", &json!({})).unwrap();
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        let again = obj(&mut back, &id).text.unwrap();
        assert_eq!(
            (again.paragraphs[0].align, again.paragraphs[0].runs[1].bold, again.paragraphs[1].runs[0].italic),
            (Some(dm::Align::Center), Some(true), Some(true))
        );
        // A theme colour is not a Word run's.
        assert!(s.run("drawing.text", &json!({ "id": id, "paragraphs": [{ "runs": [{ "text": "a", "color": { "theme": "accent1" } }] }] })).is_err());
    }

    #[test]
    fn a_connector_joins_two_shapes_follows_them_and_reopens() {
        let mut s = session();
        let ins = |s: &mut Session, x: f64, y: f64| -> String {
            let id =
                s.run("drawing.insert", &json!({ "geometry": { "preset": "rect" }, "frame": { "x": x, "y": y, "w": 100, "h": 50 } })).unwrap()["id"]
                    .as_str()
                    .unwrap()
                    .to_string();
            s.run("select.collapse", &json!({ "end": true })).unwrap();
            id
        };
        let (a, b) = (ins(&mut s, 100.0, 300.0), ins(&mut s, 400.0, 450.0));
        let c = s.run("drawing.connect", &json!({ "from": a, "to": b })).unwrap()["id"].as_str().unwrap().to_string();
        let o = obj(&mut s, &c);
        let conn = o.connection.clone().unwrap();
        assert_eq!((conn.start.as_ref().unwrap().id.as_str(), conn.end.as_ref().unwrap().id.as_str()), (a.as_str(), b.as_str()));
        let site = |s: &mut Session, id: &str, k: usize| kition_office_core::connect::sites(&obj(s, id))[k];
        let near = |p: kition_office_core::Point, q: kition_office_core::Point| (p - q).hypot() < 1.5;
        let (sa, sb) = (conn.start.as_ref().unwrap().site, conn.end.as_ref().unwrap().site);
        let (p0, p1) = kition_office_core::connect::line_ends(&o.frame);
        assert!(near(p0, site(&mut s, &a, sa)) && near(p1, site(&mut s, &b, sb)), "{p0:?} {p1:?} {:?}", o.frame);
        // b moves: the end follows in the same step; undo puts it back.
        s.run("drawing.transform", &json!({ "ids": [b], "dx": 60, "dy": 80 })).unwrap();
        let (q0, q1) = kition_office_core::connect::line_ends(&obj(&mut s, &c).frame);
        assert!(near(q0, p0) && near(q1, site(&mut s, &b, sb)), "{q0:?} {q1:?}");
        s.run("edit.undo", &json!({})).unwrap();
        let (r0, r1) = kition_office_core::connect::line_ends(&obj(&mut s, &c).frame);
        assert!(near(r0, p0) && near(r1, p1), "{r0:?} {r1:?}");
        // Saved and reopened, it is still glued to the same shapes.
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        let again = obj(&mut back, &c).connection.unwrap();
        assert_eq!((again.start.unwrap().id, again.end.unwrap().id), (a.clone(), b.clone()));
        assert!(s.run("drawing.connect", &json!({ "from": a, "to": a })).is_err());
    }

    #[test]
    fn a_shapes_first_text_is_centred_white_and_in_the_middle_as_in_word() {
        let mut s = session();
        let frame = |y: i32| json!({ "x": 100, "y": y, "w": 200, "h": 100 });
        let shape = s.run("drawing.insert", &json!({ "geometry": { "preset": "rect" }, "frame": frame(100), "text": "Shape" })).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        s.run("select.collapse", &json!({ "end": true })).unwrap();
        let boxed =
            s.run("drawing.insert", &json!({ "kind": "textBox", "frame": frame(300), "text": "Box" })).unwrap()["id"].as_str().unwrap().to_string();
        s.run("select.collapse", &json!({ "end": true })).unwrap();
        let red = s
            .run("drawing.insert", &json!({ "geometry": { "preset": "ellipse" }, "frame": frame(500), "paragraphs": [{ "align": "left", "runs": [{ "text": "Red", "color": "#C00000" }] }] }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let look = |s: &mut Session, id: &str| {
            let t = obj(s, id).text.unwrap();
            (t.anchor, t.paragraphs[0].align, t.paragraphs[0].runs[0].color.clone())
        };
        let white = Some(dm::Color::rgb(dm::Rgb(0xFF, 0xFF, 0xFF)));
        assert_eq!(look(&mut s, &shape), (dm::TextAnchor::Middle, Some(dm::Align::Center), white));
        // A text box's text is plain; what the text says itself wins.
        assert_eq!(look(&mut s, &boxed), (dm::TextAnchor::Top, None, None));
        assert_eq!(look(&mut s, &red), (dm::TextAnchor::Middle, Some(dm::Align::Left), Some(dm::Color::rgb(dm::Rgb(0xC0, 0, 0)))));
        // Saved and reopened, the shape keeps its look.
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        assert_eq!(look(&mut back, &shape).0, dm::TextAnchor::Middle);
    }

    #[test]
    fn a_shapes_text_frame_is_described_set_undone_and_saved() {
        let mut s = session();
        let frame = json!({ "x": 100, "y": 200, "w": 200, "h": 120 });
        let id = s.run("drawing.insert", &json!({ "geometry": { "preset": "rect" }, "frame": frame, "text": "Bottom" })).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let body = |s: &mut Session, id: &str| obj(s, id).text.map(|t| (t.anchor, t.insets, t.text())).unwrap();
        // A shape's first text sits in the middle, as in Word.
        assert_eq!(body(&mut s, &id), (dm::TextAnchor::Middle, [7.2, 3.6, 7.2, 3.6], "Bottom".to_string()));
        // The frame changes and the words stay.
        s.run("drawing.text", &json!({ "id": id, "text": "Bottom", "body": { "anchor": "bottom", "insets": [0, 0, 0, 0] } })).unwrap();
        assert_eq!(body(&mut s, &id), (dm::TextAnchor::Bottom, [0.0; 4], "Bottom".to_string()));
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(body(&mut s, &id).0, dm::TextAnchor::Middle);
        s.run("edit.redo", &json!({})).unwrap();
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        assert_eq!(body(&mut back, &id), (dm::TextAnchor::Bottom, [0.0; 4], "Bottom".to_string()));
    }

    #[test]
    fn art_text_bends_its_text_saves_and_comes_off() {
        use kition_doc_layout::display::{Draw, page_display};
        let mut s = session();
        let frame = json!({ "x": 100, "y": 200, "w": 300, "h": 120 });
        let id = s.run("drawing.insert", &json!({ "kind": "textBox", "frame": frame, "text": "Art text", "warp": "textArchUp" })).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let warp = |s: &mut Session, id: &str| obj(s, id).text.and_then(|t| t.warp).map(|w| w.preset);
        assert_eq!(warp(&mut s, &id).as_deref(), Some("textArchUp"));
        // Its text is drawn as bent outlines, not glyphs.
        let draws = |s: &Session| {
            let l = kition_doc_layout::layout(&s.doc, &mut kition_doc_layout::LayoutCache::new(), &Default::default());
            page_display(&s.doc, &l.pages[0], &Default::default())
        };
        let glyphs = |d: &[Draw]| d.iter().filter(|d| matches!(d, Draw::Glyphs { text, .. } if text.contains("Art"))).count();
        let d = draws(&s);
        assert!(glyphs(&d) == 0 && d.iter().any(|d| matches!(d, Draw::Path { segs, .. } if segs.len() > 50)));
        // A new warp keeps the words and their formatting.
        s.run("drawing.text", &json!({ "id": id, "paragraphs": [{ "runs": [{ "text": "Art ", "bold": true }, { "text": "text" }] }] })).unwrap();
        s.run("drawing.text", &json!({ "id": id, "warp": { "preset": "textWave1", "adjust": { "adj1": 15000 } } })).unwrap();
        let t = obj(&mut s, &id).text.unwrap();
        assert_eq!((t.text().as_str(), t.paragraphs[0].runs[0].bold, t.warp.as_ref().unwrap().adjust["adj1"]), ("Art text", Some(true), 15000.0));
        // The adjust value is drawn: a smaller wave bends the outlines less.
        let paths = |d: Vec<Draw>| {
            d.into_iter().filter_map(|d| if let Draw::Path { segs, .. } = d { Some(format!("{segs:?}")) } else { None }).collect::<Vec<_>>()
        };
        let deep = paths(draws(&s));
        s.run("drawing.text", &json!({ "id": id, "warp": { "preset": "textWave1", "adjust": { "adj1": 2000 } } })).unwrap();
        assert_ne!(paths(draws(&s)), deep);
        s.run("drawing.text", &json!({ "id": id, "warp": { "preset": "textWave1", "adjust": { "adj1": 15000 } } })).unwrap();
        // Saved and reopened, it keeps its warp and adjust values.
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        let again = obj(&mut back, &id).text.unwrap().warp.unwrap();
        assert_eq!((again.preset.as_str(), again.adjust["adj1"]), ("textWave1", 15000.0));
        // Removed, the text is flat glyphs again; undo bends it back.
        s.run("drawing.text", &json!({ "id": id, "warp": null })).unwrap();
        assert_eq!(warp(&mut s, &id), None);
        assert!(glyphs(&draws(&s)) > 0);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(warp(&mut s, &id).as_deref(), Some("textWave1"));
        assert!(s.run("drawing.text", &json!({ "id": id, "warp": "textSpiral" })).is_err());
    }

    #[test]
    fn connectors_follow_a_shape_moved_by_the_editors_own_move() {
        let mut s = session();
        let ins = |s: &mut Session, x: f64, y: f64| -> String {
            let id =
                s.run("drawing.insert", &json!({ "geometry": { "preset": "rect" }, "frame": { "x": x, "y": y, "w": 100, "h": 50 } })).unwrap()["id"]
                    .as_str()
                    .unwrap()
                    .to_string();
            s.run("select.collapse", &json!({ "end": true })).unwrap();
            id
        };
        let (a, b) = (ins(&mut s, 100.0, 300.0), ins(&mut s, 400.0, 450.0));
        let c = s.run("drawing.connect", &json!({ "from": a, "to": b })).unwrap()["id"].as_str().unwrap().to_string();
        let before = obj(&mut s, &c).frame;
        // Layout › Arrange's Move (what a drag with the pointer runs) on b alone.
        let (pos, _) = find(&s, "test", b.parse().unwrap()).unwrap();
        let end = Pos { off: pos.off + kition_doc_doc::para::OBJ.len_utf8(), ..pos.clone() };
        s.sel = Selection { anchor: pos, focus: end };
        s.run("arrange.bounds", &json!({ "x": 460, "y": 530, "page": 0, "width": 100, "height": 50 })).unwrap();
        let sb = obj(&mut s, &b);
        let conn = obj(&mut s, &c);
        let site = kition_office_core::connect::sites(&sb)[conn.connection.as_ref().unwrap().end.as_ref().unwrap().site];
        let (_, p1) = kition_office_core::connect::line_ends(&conn.frame);
        assert!((p1 - site).hypot() < 1.5, "{p1:?} {site:?}");
        // One step: undo puts the connector back with the shape.
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(obj(&mut s, &c).frame, before);
    }

    #[test]
    fn a_plain_line_glued_at_both_ends_is_a_connector_that_saves() {
        let mut s = session();
        let ins = |s: &mut Session, preset: &str, x: f64, y: f64| -> String {
            let id =
                s.run("drawing.insert", &json!({ "geometry": { "preset": preset }, "frame": { "x": x, "y": y, "w": 100, "h": 50 } })).unwrap()["id"]
                    .as_str()
                    .unwrap()
                    .to_string();
            s.run("select.collapse", &json!({ "end": true })).unwrap();
            id
        };
        let (a, b, line) = (ins(&mut s, "rect", 100.0, 300.0), ins(&mut s, "rect", 400.0, 450.0), ins(&mut s, "line", 250.0, 380.0));
        s.run("drawing.reconnect", &json!({ "id": line, "end": "start", "to": a, "site": 3 })).unwrap();
        s.run("drawing.reconnect", &json!({ "id": line, "end": "end", "to": b, "site": 1 })).unwrap();
        let o = obj(&mut s, &line);
        let conn = o.connection.clone().unwrap();
        assert_eq!((conn.start.unwrap().id, conn.end.unwrap().id), (a.clone(), b.clone()));
        let (p0, p1) = kition_office_core::connect::line_ends(&o.frame);
        assert!((p0 - kition_office_core::connect::sites(&obj(&mut s, &a))[3]).hypot() < 1.5, "{p0:?}");
        assert!((p1 - kition_office_core::connect::sites(&obj(&mut s, &b))[1]).hypot() < 1.5, "{p1:?}");
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        let again = obj(&mut back, &line).connection.unwrap();
        assert_eq!((again.start.unwrap().id, again.end.unwrap().id), (a, b));
    }

    #[test]
    fn insert_art_text_makes_a_drawing_object_with_its_warp() {
        let mut s = session();
        let id = s.run("artText.insert", &json!({ "text": "Kition", "warp": "textWave1" })).unwrap()["id"].as_str().unwrap().to_string();
        let o = obj(&mut s, &id);
        let t = o.text.clone().unwrap();
        assert_eq!(
            (o.kind, t.text().as_str(), t.paragraphs[0].runs[0].size, t.paragraphs[0].runs[0].bold),
            (dm::ObjectKind::TextBox, "Kition", Some(36.0), Some(true))
        );
        assert_eq!((t.warp.unwrap().preset.as_str(), o.fill, o.stroke.map(|st| st.none)), ("textWave1", Some(dm::Fill::None), Some(true)));
        // One step: undo takes it away.
        s.run("edit.undo", &json!({})).unwrap();
        assert!(
            s.run("drawing.describe", &json!({ "ids": [id] })).is_err()
                || s.run("drawing.describe", &json!({ "ids": [id] })).unwrap()["objects"].as_array().unwrap().is_empty()
        );
        // Without a warp it is flat art text.
        let id = s.run("artText.insert", &json!({})).unwrap()["id"].as_str().unwrap().to_string();
        assert_eq!(obj(&mut s, &id).text.unwrap().warp, None);
    }

    fn obj(s: &mut Session, id: &str) -> DrawingObject {
        let d = s.run("drawing.describe", &json!({ "ids": [id] })).unwrap();
        serde_json::from_value(d["objects"][0].clone()).unwrap()
    }

    #[test]
    fn insert_edit_undo_save_and_reopen() {
        let mut s = session();
        let id = s
            .run("drawing.insert", &json!({ "geometry": { "preset": "ellipse" }, "frame": { "x": 200, "y": 300, "w": 150, "h": 80 }, "fill": { "color": "#5645D4" }, "stroke": { "color": "#000000", "width": 2 } }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let o = obj(&mut s, &id);
        assert_eq!(o.geometry, dm::Geometry::preset("ellipse"));
        assert_eq!((o.frame.x, o.frame.y, o.frame.w, o.frame.h), (200.0, 300.0, 150.0, 80.0));
        assert_eq!(o.placement.kind, dm::PlacementKind::Float);
        assert_eq!(o.fill, Some(dm::Fill::Solid(dm::Color::rgb(dm::Rgb(0x56, 0x45, 0xD4)))));

        let undo = s.history.len();
        s.run("drawing.transform", &json!({ "ids": [id], "dx": 20, "dy": 10, "rotateBy": 30 })).unwrap();
        assert_eq!(s.history.len(), undo + 1, "one undo step");
        s.run("drawing.format", &json!({ "ids": [id], "geometry": { "preset": "hexagon" }, "stroke": { "none": true } })).unwrap();
        s.run("drawing.text", &json!({ "id": id, "text": "Plan\nAhead" })).unwrap();
        let o = obj(&mut s, &id);
        assert!((o.frame.x - 220.0).abs() < 0.01 && (o.frame.y - 310.0).abs() < 0.01 && (o.frame.rot - 30.0).abs() < 0.01, "{:?}", o.frame);
        assert_eq!(o.geometry, dm::Geometry::preset("hexagon"));
        assert!(o.stroke.unwrap().none);
        assert_eq!(o.text.unwrap().text(), "Plan\nAhead");
        s.run("edit.undo", &json!({})).unwrap();
        assert!(obj(&mut s, &id).text.is_none_or(|t| t.text().is_empty()));
        s.run("edit.redo", &json!({})).unwrap();

        // Save as DOCX, reopen, and edit again: the id, preset, place and text come back.
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        let o = obj(&mut back, &id);
        assert_eq!(o.geometry, dm::Geometry::preset("hexagon"));
        assert!((o.frame.x - 220.0).abs() < 0.5 && (o.frame.rot - 30.0).abs() < 0.01, "{:?}", o.frame);
        assert_eq!(o.text.unwrap().text(), "Plan\nAhead");
        back.run("drawing.transform", &json!({ "ids": [id], "frame": { "x": 100, "y": 120, "w": 60, "h": 60 } })).unwrap();
        let o = obj(&mut back, &id);
        assert_eq!((o.frame.x, o.frame.y, o.frame.w), (100.0, 120.0, 60.0));
    }

    #[test]
    fn charts_insert_change_undo_save_and_reopen() {
        let mut s = session();
        let id = s.run("drawing.insert", &json!({ "kind": "chart", "chart": { "kind": "line", "title": "Trend" } })).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let o = obj(&mut s, &id);
        assert_eq!((o.kind, o.placement.kind), (dm::ObjectKind::Chart, dm::PlacementKind::Inline));
        let c = o.chart.unwrap();
        assert_eq!((c.kind, c.title.as_deref(), c.series.len()), (kition_office_core::chart::ChartKind::Line, Some("Trend"), 2));
        let drawn = |s: &Session| {
            s.doc
                .shapes()
                .into_iter()
                .find_map(|(_, o)| if let InlineObject::Graphic { graphic, .. } = o { Some(graphic.items.len()) } else { None })
                .unwrap()
        };
        assert!(drawn(&s) > 0, "the new chart is drawn");

        let undo = s.history.len();
        s.run(
            "drawing.chart",
            &json!({ "id": id, "kind": "column", "categories": ["A", "B"], "series": [{ "name": "Sales", "values": [3, 4] }], "dataLabels": true }),
        )
        .unwrap();
        assert_eq!(s.history.len(), undo + 1, "one undo step");
        let c = obj(&mut s, &id).chart.unwrap();
        assert_eq!((c.categories.len(), c.series.len(), c.data_labels), (2, 1, true));
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(obj(&mut s, &id).chart.unwrap().kind, kition_office_core::chart::ChartKind::Line);
        s.run("edit.redo", &json!({})).unwrap();
        // A chart's frame resizes; it does not turn, and it has no shape's look.
        s.run("drawing.transform", &json!({ "ids": [id], "frame": { "x": 0, "y": 0, "w": 300, "h": 200 } })).unwrap();
        assert_eq!(obj(&mut s, &id).frame.w, 300.0);
        let e = s.run("drawing.transform", &json!({ "ids": [id], "rotateBy": 90 })).unwrap_err();
        assert!(e.to_string().contains("unsupported"), "{e}");
        let e = s.run("drawing.format", &json!({ "ids": [id], "fill": { "color": "#FF0000" } })).unwrap_err();
        assert!(e.to_string().contains("unsupported"), "{e}");
        let before = obj(&mut s, &id).chart.unwrap();

        // Save as DOCX, reopen, and edit again: the id and the chart come back.
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        let o = obj(&mut back, &id);
        assert_eq!(o.chart.as_ref(), Some(&before));
        assert_eq!(o.frame.w, 300.0);
        back.run("drawing.chart", &json!({ "id": id, "title": null, "legend": "top" })).unwrap();
        let c = obj(&mut back, &id).chart.unwrap();
        assert_eq!((c.title, c.legend), (None, Some(kition_office_core::chart::LegendPos::Top)));
        let again = Session::new(kition_doc_docx::read(&kition_doc_docx::write(&back.doc).unwrap()).unwrap());
        let mut again = again;
        assert_eq!(obj(&mut again, &id).chart.unwrap().legend, Some(kition_office_core::chart::LegendPos::Top));
    }

    /// The parts of a saved package whose names contain `like`, with their text.
    fn parts_like(bytes: &[u8], like: &str) -> Vec<(String, Vec<u8>)> {
        use std::io::Read;
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        let names: Vec<String> = z.file_names().filter(|n| n.contains(like)).map(str::to_string).collect();
        names
            .into_iter()
            .map(|n| {
                let mut b = Vec::new();
                z.by_name(&n).unwrap().read_to_end(&mut b).unwrap();
                (n, b)
            })
            .collect()
    }

    #[test]
    fn a_new_or_edited_chart_is_saved_with_a_workbook_of_its_data() {
        let mut s = session();
        let id = s.run("drawing.insert", &json!({ "kind": "chart", "chart": { "kind": "column", "categories": ["North", "South"], "series": [{ "name": "Sales", "values": [3, 4] }] } })).unwrap()["id"].as_str().unwrap().to_string();
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let chart = String::from_utf8(parts_like(&bytes, "word/charts/chart").into_iter().find(|(n, _)| n.ends_with(".xml")).unwrap().1).unwrap();
        assert!(chart.contains(r#"<c:externalData r:id="rIdKitionData">"#) && chart.contains("<c:f>Sheet1!$B$2:$B$3</c:f>"), "{chart}");
        let rels = String::from_utf8(parts_like(&bytes, "word/charts/_rels/").remove(0).1).unwrap();
        assert!(rels.contains("rIdKitionData") && rels.contains("relationships/package") && rels.contains("../embeddings/"), "{rels}");
        let books = parts_like(&bytes, "word/embeddings/");
        assert_eq!(books.len(), 1);
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(books[0].1.clone())).unwrap();
        let mut sheet = String::new();
        std::io::Read::read_to_string(&mut z.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
        assert!(sheet.contains("North") && sheet.contains("Sales") && sheet.contains("<v>4</v>"), "{sheet}");
        let types = String::from_utf8(parts_like(&bytes, "[Content_Types].xml").remove(0).1).unwrap();
        assert!(types.contains("spreadsheetml.sheet"), "the workbook part has its content type");

        // Reopened and edited again: the chart gets a new workbook of its new data, and the old one goes.
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        back.run("drawing.chart", &json!({ "id": id, "series": [{ "name": "Sales", "values": [30, 40] }] })).unwrap();
        let again = kition_doc_docx::write(&back.doc).unwrap();
        let books = parts_like(&again, "word/embeddings/");
        assert_eq!(books.len(), 1, "one workbook: {:?}", books.iter().map(|b| &b.0).collect::<Vec<_>>());
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(books[0].1.clone())).unwrap();
        let mut sheet = String::new();
        std::io::Read::read_to_string(&mut z.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
        assert!(sheet.contains("<v>40</v>") && !sheet.contains("<v>4</v>"), "{sheet}");
        let rels = String::from_utf8(parts_like(&again, "word/charts/_rels/").remove(0).1).unwrap();
        assert_eq!(rels.matches("relationships/package").count(), 1, "{rels}");
        // A format-only change keeps the workbook it has.
        let mut third = Session::new(kition_doc_docx::read(&again).unwrap());
        third.run("drawing.chart", &json!({ "id": id, "title": "Kept data" })).unwrap();
        let kept = kition_doc_docx::write(&third.doc).unwrap();
        let chart = String::from_utf8(parts_like(&kept, "word/charts/chart").into_iter().find(|(n, _)| n.ends_with(".xml")).unwrap().1).unwrap();
        assert!(chart.contains("Kept data") && chart.contains("c:externalData"), "{chart}");
        assert_eq!(parts_like(&kept, "word/embeddings/").len(), 1);
    }

    #[test]
    fn chart_axes_labels_and_series_looks_change_draw_and_reopen() {
        let mut s = session();
        let id =
            s.run("drawing.insert", &json!({ "kind": "chart", "chart": { "kind": "lineMarkers" } })).unwrap()["id"].as_str().unwrap().to_string();
        let items = |s: &Session| {
            s.doc
                .shapes()
                .into_iter()
                .find_map(|(_, o)| if let InlineObject::Graphic { graphic, .. } = o { Some(graphic.items.clone()) } else { None })
                .unwrap()
        };
        let before = items(&s);
        s.run(
            "drawing.chart",
            &json!({ "id": id, "yAxis": { "min": 0, "max": 8, "major": 4 }, "dataLabels": true, "dataLabelPosition": "t", "seriesFormat": [{ "index": 0, "marker": "square", "smooth": true }] }),
        )
        .unwrap();
        let c = obj(&mut s, &id).chart.unwrap();
        assert_eq!((c.y_axis.max, c.y_axis.major, c.data_label_position.as_deref()), (Some(8.0), Some(4.0), Some("t")));
        assert_eq!((c.series[0].marker.as_deref(), c.series[0].smooth), (Some("square"), true));
        // Drawn again from the change: its labels and its fixed ticks show.
        let after = items(&s);
        assert_ne!(after, before);
        let texts: Vec<String> = after
            .iter()
            .filter_map(|i| if let kition_doc_doc::graphic::GraphicItem::Text { text, .. } = i { Some(text.clone()) } else { None })
            .collect();
        assert!(texts.iter().any(|t| t == "8") && texts.iter().any(|t| t == "4.3"), "{texts:?}");
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        let r = obj(&mut back, &id).chart.unwrap();
        assert_eq!(
            (r.y_axis, r.data_label_position, r.series[0].marker.clone(), r.series[0].smooth),
            (c.y_axis, c.data_label_position, c.series[0].marker.clone(), true)
        );
    }

    #[test]
    fn a_series_becomes_a_line_on_the_secondary_axis_draws_and_reopens() {
        use kition_doc_doc::graphic::GraphicItem;
        let mut s = session();
        let chart = json!({ "kind": "column", "categories": ["A", "B", "C"], "series": [{ "name": "Sales", "values": [100, 300, 200] }, { "name": "Share", "values": [0.2, 0.5, 0.3] }] });
        let id = s.run("drawing.insert", &json!({ "kind": "chart", "chart": chart })).unwrap()["id"].as_str().unwrap().to_string();
        let items = |s: &Session| {
            s.doc
                .shapes()
                .into_iter()
                .find_map(|(_, o)| if let InlineObject::Graphic { graphic, .. } = o { Some(graphic.items.clone()) } else { None })
                .unwrap()
        };
        let texts = |items: &[GraphicItem]| {
            items.iter().filter_map(|i| if let GraphicItem::Text { text, .. } = i { Some(text.clone()) } else { None }).collect::<Vec<_>>()
        };
        let lines = |items: &[GraphicItem]| items.iter().filter(|i| matches!(i, GraphicItem::Path { fill: None, stroke: Some(_), .. })).count();
        let before = items(&s);
        s.run("drawing.chart", &json!({ "id": id, "seriesFormat": [{ "index": 1, "kind": "line", "secondary": true }] })).unwrap();
        let c = obj(&mut s, &id).chart.unwrap();
        assert_eq!(
            (c.series[1].kind.clone(), c.series[1].secondary, c.series[0].name.as_str()),
            (Some(kition_office_core::chart::ChartKind::Line), true, "Sales")
        );
        // Drawn: a line for the share, and its own axis's labels (fractions) on the right.
        let after = items(&s);
        assert!(lines(&after) > lines(&before));
        assert!(
            texts(&after).iter().any(|t| t == "0.5" || t == "0.6") && !texts(&before).iter().any(|t| t == "0.5" || t == "0.6"),
            "{:?}",
            texts(&after)
        );
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        let r = obj(&mut back, &id).chart.unwrap();
        assert_eq!((r.series[1].kind.clone(), r.series[1].secondary), (Some(kition_office_core::chart::ChartKind::Line), true));
        s.run("edit.undo", &json!({})).unwrap();
        assert!(!obj(&mut s, &id).chart.unwrap().series[1].secondary);
    }

    #[test]
    fn trendlines_and_error_bars_draw_save_and_reopen() {
        use kition_doc_doc::graphic::GraphicItem;
        let mut s = session();
        let chart = json!({ "kind": "column", "categories": ["A", "B", "C", "D"], "series": [{ "name": "S", "values": [1, 3, 2, 5] }] });
        let id = s.run("drawing.insert", &json!({ "kind": "chart", "chart": chart })).unwrap()["id"].as_str().unwrap().to_string();
        let strokes = |s: &Session| {
            s.doc
                .shapes()
                .into_iter()
                .find_map(|(_, o)| if let InlineObject::Graphic { graphic, .. } = o { Some(graphic.items.clone()) } else { None })
                .unwrap()
                .iter()
                .filter(|i| matches!(i, GraphicItem::Path { fill: None, stroke: Some(_), .. }))
                .count()
        };
        let before = strokes(&s);
        let change = json!({ "id": id, "seriesFormat": [{ "index": 0, "trendline": "linear", "errorBars": { "kind": "fixedVal", "value": 0.5 } }] });
        s.run("drawing.chart", &change).unwrap();
        let c = obj(&mut s, &id).chart.unwrap();
        assert_eq!((c.series[0].trendline.as_ref().unwrap().kind.as_str(), c.series[0].error_bars.as_ref().unwrap().value), ("linear", Some(0.5)));
        // One trendline and four error bars are drawn.
        assert_eq!(strokes(&s), before + 5);
        let bytes = kition_doc_docx::write(&s.doc).unwrap();
        let mut back = Session::new(kition_doc_docx::read(&bytes).unwrap());
        let r = obj(&mut back, &id).chart.unwrap();
        assert_eq!((r.series[0].trendline.clone(), r.series[0].error_bars.clone()), (c.series[0].trendline.clone(), c.series[0].error_bars.clone()));
        s.run("drawing.chart", &json!({ "id": id, "seriesFormat": [{ "index": 0, "trendline": null, "errorBars": null }] })).unwrap();
        assert_eq!(strokes(&s), before);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(strokes(&s), before + 5);
    }

    #[test]
    fn an_edited_chart_from_a_file_keeps_what_the_description_leaves_out() {
        let mut s = session();
        let id = s.run("drawing.insert", &json!({ "kind": "chart" })).unwrap()["id"].as_str().unwrap().to_string();
        let mut back = Session::new(kition_doc_docx::read(&kition_doc_docx::write(&s.doc).unwrap()).unwrap());
        // The reopened chart is read from its part; mark the part with something Kition does not model.
        let part = back
            .doc
            .shapes()
            .into_iter()
            .find_map(|(_, o)| {
                if let InlineObject::Graphic { graphic, .. } = o { graphic.source.as_ref()?.refs.first().map(|r| r.target.clone()) } else { None }
            })
            .expect("the chart is read with its part");
        let marked = String::from_utf8_lossy(&back.doc.passthrough[&part]).replacen(
            "</c:plotArea>",
            "<c:extLst><c:ext uri=\"{K}\"><kept/></c:ext></c:extLst></c:plotArea>",
            1,
        );
        back.doc.passthrough.insert(part, std::sync::Arc::new(marked.into_bytes()));
        back.run("drawing.chart", &json!({ "id": id, "title": "Revenue" })).unwrap();
        let reread = kition_doc_docx::read(&kition_doc_docx::write(&back.doc).unwrap()).unwrap();
        let graphic = reread
            .shapes()
            .into_iter()
            .find_map(|(_, o)| if let InlineObject::Graphic { graphic, .. } = o { Some(graphic.clone()) } else { None })
            .unwrap();
        let xml = kition_doc_docx::chart_part(&reread, &graphic).unwrap();
        assert!(xml.contains("Revenue") && xml.contains("<kept/>"), "the edited part keeps what it was not told about");
    }

    #[test]
    fn an_inline_shape_and_a_text_box_and_errors() {
        let mut s = session();
        let tb = s.run("drawing.insert", &json!({ "kind": "textBox", "text": "Inside" })).unwrap()["id"].as_str().unwrap().to_string();
        let o = obj(&mut s, &tb);
        assert_eq!((o.kind, o.placement.kind), (dm::ObjectKind::TextBox, dm::PlacementKind::Inline));
        assert_eq!(o.text.unwrap().text(), "Inside");
        let e = s.run("drawing.format", &json!({ "ids": [tb], "fill": { "color": "accent1" } })).unwrap_err();
        assert!(e.to_string().contains("unsupported (fill)"), "{e}");
        let before = s.doc.clone();
        let e = s.run("drawing.transform", &json!({ "ids": [tb, "999"], "dx": 1 })).unwrap_err();
        assert!(e.to_string().contains("not-found"), "{e}");
        assert_eq!(s.doc.plain_text(StoryRef::Body), before.plain_text(StoryRef::Body));
        // Sizing an inline shape keeps it in line.
        s.run("drawing.transform", &json!({ "ids": [tb], "frame": { "x": o.frame.x, "y": o.frame.y, "w": 200, "h": 90 } })).unwrap();
        let o = obj(&mut s, &tb);
        assert_eq!((o.frame.w, o.placement.kind), (200.0, dm::PlacementKind::Inline));
        s.run("drawing.delete", &json!({ "ids": [tb] })).unwrap();
        assert!(s.run("drawing.describe", &json!({})).unwrap()["objects"].as_array().unwrap().is_empty());
    }

    #[test]
    fn copies_get_new_ids() {
        let mut s = session();
        let a = s.run("drawing.insert", &json!({})).unwrap()["id"].as_str().unwrap().to_string();
        let (pos, o) = find(&s, "t", a.parse().unwrap()).unwrap();
        // A copy of the shape (as paste makes) gets an id of its own.
        s.doc.insert_object(&pos, o, &CharProps::default()).unwrap();
        s.doc.assign_object_ids();
        let ids: Vec<u32> = s.doc.shapes().iter().map(|(_, o)| uid_of(o)).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn a_floating_shape_moves_with_its_paragraph_when_text_reflows() {
        let mut s = session();
        let id = s.run("drawing.insert", &json!({ "frame": { "x": 200, "y": 150, "w": 80, "h": 40 } })).unwrap()["id"].as_str().unwrap().to_string();
        let before = obj(&mut s, &id).frame;
        // Paragraphs typed above its anchor paragraph push it down the page with its text.
        s.run("select.collapse", &json!({})).unwrap();
        s.run("caret.docStart", &json!({})).unwrap();
        for _ in 0..6 {
            s.run("text.insert", &json!({ "text": "A new paragraph above the shape's paragraph." })).unwrap();
            s.run("text.newParagraph", &json!({})).unwrap();
        }
        let after = obj(&mut s, &id).frame;
        assert!(after.y > before.y + 30.0, "the shape follows its paragraph: {before:?} → {after:?}");
        assert_eq!(after.x, before.x);
    }
}
