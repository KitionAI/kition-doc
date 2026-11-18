//! Charts inserted with the Insert tab's command: they take their data,
//! survive the next paragraph, and take alternative text that is saved with them.

use kition_doc_engine::doc::para::InlineObject;
use kition_doc_engine::doc::{Document, graphic::GraphicKind};
use kition_doc_engine::{Session, io};
use serde_json::json;

fn charts(doc: &Document) -> Vec<InlineObject> {
    doc.shapes()
        .into_iter()
        .map(|(_, o)| o.clone())
        .filter(|o| matches!(o, InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Chart))
        .collect()
}

fn session() -> Session {
    let mut s = Session::new(Document::new());
    s.run("text.insert", &json!({ "text": "Where each pound went:" })).unwrap();
    s.run("text.newParagraph", &json!({})).unwrap();
    s
}

#[test]
fn insert_chart_takes_its_data() {
    let mut s = session();
    let id = s
        .run(
            "insert.chart",
            &json!({
                "type": "pie",
                "title": "Spending",
                "categories": ["Food", "Rent", "Van"],
                "series": [{ "name": "Share", "values": [60, 25, 15] }],
                "legend": "right",
                "width": 300,
                "height": 200
            }),
        )
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let d = s.run("drawing.describe", &json!({ "ids": [id] })).unwrap();
    let o = &d["objects"][0];
    assert_eq!(o["kind"], "chart", "{d}");
    let chart = &o["chart"];
    assert_eq!(chart["kind"], "pie", "{chart}");
    assert_eq!(chart["title"], "Spending", "{chart}");
    assert_eq!(chart["categories"], json!(["Food", "Rent", "Van"]), "{chart}");
    assert_eq!(chart["series"][0]["name"], "Share", "{chart}");
    assert_eq!(chart["series"][0]["values"], json!([60.0, 25.0, 15.0]), "{chart}");
    assert_eq!(chart["legend"], "right", "{chart}");
    assert_eq!((o["frame"]["w"].as_f64(), o["frame"]["h"].as_f64()), (Some(300.0), Some(200.0)), "{o}");
}

#[test]
fn the_paragraph_after_an_inserted_chart_keeps_the_chart() {
    for (command, params) in
        [("insert.chart", json!({ "type": "column" })), ("drawing.insert", json!({ "kind": "chart", "chart": { "kind": "bar" } }))]
    {
        let mut s = session();
        s.run(command, &params).unwrap();
        s.run("text.newParagraph", &json!({})).unwrap();
        s.run("text.insert", &json!({ "text": "After the chart." })).unwrap();
        assert_eq!(charts(&s.doc).len(), 1, "{command}: the next paragraph deleted the chart");
        let text = s.run("document.text", &json!({})).unwrap().to_string();
        assert!(text.contains("After the chart."), "{command}: {text}");
    }
}

#[test]
fn a_chart_takes_alt_text_that_is_saved() {
    let mut s = session();
    s.run("insert.chart", &json!({ "type": "doughnut" })).unwrap();
    s.run("picture.altText", &json!({ "text": "Doughnut chart of spending: food 46%, rent 18%" })).unwrap();
    let alt = |doc: &Document| match charts(doc).first() {
        Some(InlineObject::Graphic { alt, .. }) => alt.clone(),
        _ => String::new(),
    };
    assert_eq!(alt(&s.doc), "Doughnut chart of spending: food 46%, rent 18%");
    let bytes = io::save_bytes("chart.docx", &s.doc).unwrap();
    let reopened = io::open_bytes("chart.docx", &bytes).unwrap();
    assert_eq!(alt(&reopened), "Doughnut chart of spending: food 46%, rent 18%");
}
