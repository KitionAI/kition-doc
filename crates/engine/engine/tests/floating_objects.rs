//! Floating objects placed with the drawing commands.

use kition_doc_engine::doc::Document;
use kition_doc_engine::layout::{self, Placed};
use kition_doc_engine::{Selection, Session, io, render};
use serde_json::{Value, json};

const WORDS: &str = "The ferry is eleven metres of painted larch with a diesel engine older than most of her passengers, and she carries twelve people, four bicycles and the post.";

/// A letter page with 60 pt side margins in two columns 24 pt apart (60–294 and 318–552), filled
/// with `n` paragraphs.
fn columns(n: usize) -> Session {
    let mut s = Session::new(Document::new());
    s.run("layout.margins", &json!({ "top": 54, "bottom": 54, "left": 60, "right": 60 })).unwrap();
    s.run("layout.columns", &json!({ "count": 2, "space": 24 })).unwrap();
    for i in 0..n {
        if i > 0 {
            s.run("text.newParagraph", &json!({})).unwrap();
        }
        s.run("text.insert", &json!({ "text": format!("P{i}. {WORDS}") })).unwrap();
    }
    s
}

/// (left, top, right) of every body line on page `page` with its first characters.
fn body_lines(s: &mut Session, page: usize) -> Vec<(f32, f32, f32, String)> {
    let doc = s.doc.clone();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let mut out = Vec::new();
    for item in &result.pages[page].items {
        if let Placed::Lines { story, path, para, l0, l1, x, y, .. } = item {
            if *story != kition_doc_engine::doc::StoryRef::Body {
                continue;
            }
            let p = doc.para(*story, path).unwrap();
            for li in *l0..*l1 {
                let l = &para.lines[li];
                let text: String = p.text.get(l.start..l.stop).unwrap_or("").chars().take(12).collect();
                let left = x + l.xs.first().copied().unwrap_or(0.0);
                let right = x + l.xs.last().copied().unwrap_or(0.0);
                out.push((left, y + l.top - para.lines[*l0].top, right, text));
            }
        }
    }
    out
}

fn object_rect(s: &mut Session, page: usize) -> Vec<[f32; 4]> {
    let doc = s.doc.clone();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    result.pages[page]
        .items
        .iter()
        .filter_map(|it| match it {
            Placed::Object { rect, .. } => Some([rect.x, rect.y, rect.w, rect.h]),
            _ => None,
        })
        .collect()
}

fn describe_all(s: &mut Session) -> Value {
    s.sel = Selection::caret(s.doc.start_of(kition_doc_engine::doc::StoryRef::Body));
    s.run("drawing.describe", &json!({})).unwrap()
}

#[test]
fn drawing_insert_takes_the_wrap_it_is_given() {
    let mut s = columns(6);
    let id = s
        .run(
            "drawing.insert",
            &json!({ "kind": "textBox", "frame": { "x": 318, "y": 200, "w": 200, "h": 80 }, "placement": { "page": 0, "wrap": "topAndBottom" }, "text": "A pull quote." }),
        )
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let d = s.run("drawing.describe", &json!({ "ids": [id] })).unwrap();
    assert_eq!(d["objects"][0]["placement"]["wrap"], "topAndBottom", "{d}");
    assert_eq!(d["objects"][0]["frame"]["x"], 318.0, "{d}");
    // Shapes and charts too, behind the text or in front of it.
    let id = s
        .run("drawing.insert", &json!({ "geometry": { "preset": "ellipse" }, "frame": { "x": 400, "y": 500, "w": 80, "h": 80 }, "placement": { "page": 0, "wrap": "behindText" } }))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let d = s.run("drawing.describe", &json!({ "ids": [id] })).unwrap();
    assert_eq!(d["objects"][0]["placement"]["wrap"], "behindText", "{d}");
    let id = s.run("drawing.insert", &json!({ "kind": "chart", "placement": { "wrap": "square" } })).unwrap()["id"].as_str().unwrap().to_string();
    let d = s.run("drawing.describe", &json!({ "ids": [id] })).unwrap();
    assert_eq!(d["objects"][0]["placement"]["kind"], "float", "{d}");
    assert_eq!(d["objects"][0]["placement"]["wrap"], "square", "{d}");
    // A wrap Word does not have changes nothing.
    let before = s.doc.shapes().len();
    assert!(s.run("drawing.insert", &json!({ "kind": "shape", "placement": { "wrap": "around" } })).is_err());
    assert_eq!(s.doc.shapes().len(), before);
}

#[test]
fn dashed_outlines_are_drawn_and_saved() {
    let mut s = Session::new(Document::new());
    s.run("text.insert", &json!({ "text": "A dashed box: " })).unwrap();
    let id = s
        .run(
            "drawing.insert",
            &json!({ "geometry": { "preset": "rect" }, "frame": { "x": 100, "y": 200, "w": 200, "h": 100 }, "placement": { "page": 0 }, "fill": { "none": true }, "stroke": { "color": { "rgb": "#000000" }, "width": 2, "dash": "dash" } }),
        )
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let d = s.run("drawing.describe", &json!({ "ids": [id.clone()] })).unwrap();
    assert_eq!(d["objects"][0]["stroke"]["dash"], "dash", "{d}");
    // Saved as the outline's preset dash, and read back.
    let bytes = io::save_bytes("dash.docx", &s.doc).unwrap();
    let xml = docx_part(&bytes, "word/document.xml");
    assert!(xml.contains(r#"<a:prstDash val="dash"/>"#), "{xml}");
    let mut reopened = Session::new(io::open_bytes("dash.docx", &bytes).unwrap());
    let d = describe_all(&mut reopened);
    assert_eq!(d["objects"][0]["stroke"]["dash"], "dash", "{d}");
    // Drawn with gaps: along the top edge (y = 200) some pixels are ink, some paper.
    let doc = reopened.doc.clone();
    let l = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let r = render::render_page(&doc, &l.pages[0], 1.0, &render::RenderOptions::default());
    let dark: Vec<bool> = (110..290).map(|x| r.pixels[((200 * r.width + x) * 4) as usize] < 128).collect();
    let (ink, paper) = (dark.iter().filter(|d| **d).count(), dark.iter().filter(|d| !**d).count());
    assert!(ink > 40 && paper > 40, "a dashed edge has dashes and gaps: {ink} ink, {paper} paper");
    // A solid outline again.
    s.run("drawing.format", &json!({ "ids": [id.clone()], "stroke": { "dash": "solid" } })).unwrap();
    let d = s.run("drawing.describe", &json!({ "ids": [id] })).unwrap();
    assert!(d["objects"][0]["stroke"].get("dash").is_none(), "{d}");
}

#[test]
fn describe_without_ids_lists_every_object_unless_some_are_selected() {
    let mut s = columns(4);
    for x in [100, 300] {
        s.run(
            "drawing.insert",
            &json!({ "geometry": { "preset": "rect" }, "frame": { "x": x, "y": 120, "w": 60, "h": 40 }, "placement": { "page": 0 } }),
        )
        .unwrap();
    }
    // The second one is selected: it alone is described.
    let d = s.run("drawing.describe", &json!({})).unwrap();
    assert_eq!(d["objects"].as_array().unwrap().len(), 1, "{d}");
    // With the caret in the text (even right beside an object), every object.
    s.run("select.collapse", &json!({ "end": true })).unwrap();
    let d = s.run("drawing.describe", &json!({})).unwrap();
    assert_eq!(d["objects"].as_array().unwrap().len(), 2, "{d}");
}

fn docx_part(bytes: &[u8], name: &str) -> String {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut out).unwrap();
    out
}

#[test]
fn an_object_anchored_in_a_later_column_is_placed_in_that_column() {
    // The last paragraph starts at the foot of the first column and goes on in the second; the
    // box is dropped beside its continuation, so it anchors to it there.
    let mut s = columns(8);
    s.run("text.newParagraph", &json!({})).unwrap();
    s.run("text.insert", &json!({ "text": format!("Long. {WORDS} {WORDS} {WORDS} {WORDS}") })).unwrap();
    s.run(
        "drawing.insert",
        &json!({ "kind": "textBox", "frame": { "x": 330, "y": 660, "w": 150, "h": 50 }, "placement": { "page": 0 }, "text": "Box" }),
    )
    .unwrap();
    assert_eq!(object_rect(&mut s, 0), [[330.0, 660.0, 150.0, 50.0]]);
    let d = describe_all(&mut s);
    assert_eq!((d["objects"][0]["frame"]["x"].as_f64(), d["objects"][0]["frame"]["y"].as_f64()), (Some(330.0), Some(660.0)), "{d}");
}

#[test]
fn text_wraps_around_a_floating_object_in_every_column_it_reaches() {
    let mut s = columns(26);
    // Across the gap between the columns, anchored in the second.
    s.run(
        "drawing.insert",
        &json!({ "kind": "textBox", "frame": { "x": 200, "y": 300, "w": 212, "h": 96 }, "placement": { "page": 0, "wrap": "topAndBottom" }, "text": "Nobody has ever missed the boat." }),
    )
    .unwrap();
    s.run("select.collapse", &json!({ "end": true })).unwrap();
    assert_eq!(object_rect(&mut s, 0), [[200.0, 300.0, 212.0, 96.0]]);
    let lines = body_lines(&mut s, 0);
    let overlapping: Vec<_> = lines.iter().filter(|(_, top, _, _)| *top > 300.0 - 16.0 && *top < 396.0 - 0.5).collect();
    assert!(overlapping.is_empty(), "no line in either column runs under the box: {overlapping:?}");
    assert!(lines.iter().any(|(left, top, _, _)| *left < 100.0 && (*top - 396.0).abs() < 0.5), "the first column resumes below the box: {lines:?}");
    assert!(lines.iter().any(|(left, top, _, _)| *left > 300.0 && (*top - 396.0).abs() < 0.5), "so does the second: {lines:?}");
    // It moves from where it is shown.
    let id = describe_all(&mut s)["objects"][0]["id"].as_str().unwrap().to_string();
    s.run("drawing.transform", &json!({ "ids": [id], "dx": 10, "dy": 20 })).unwrap();
    assert_eq!(object_rect(&mut s, 0), [[210.0, 320.0, 212.0, 96.0]]);
}
