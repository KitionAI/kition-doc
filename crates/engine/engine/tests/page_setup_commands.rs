//! Page setup through the commands an agent uses: a partial
//! `page.pageSetup` changes only what it names, `page.size` keeps a landscape size,
//! vertical alignment is drawn, and keep with next holds at the foot of a page and a column.

use kition_doc_engine::{
    Session,
    doc::{Document, StoryRef},
    layout::Placed,
};
use serde_json::{Value, json};

fn run(s: &mut Session, command: &str, params: Value) -> Value {
    s.run(command, &params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn session() -> Session {
    Session::new(Document::new())
}

fn type_paragraphs(s: &mut Session, texts: &[&str]) {
    for (index, text) in texts.iter().enumerate() {
        if index > 0 {
            run(s, "text.newParagraph", json!({}));
        }
        run(s, "text.insert", json!({ "text": text }));
    }
}

/// The page and top of the first line of the body paragraph that starts with `text`.
fn where_is(s: &mut Session, text: &str) -> (usize, f32) {
    let layout = s.layout();
    let doc = &s.doc;
    for (index, page) in layout.pages.iter().enumerate() {
        for item in &page.items {
            if let Placed::Lines { story: StoryRef::Body, path, l0: 0, y, .. } = item
                && doc.para(StoryRef::Body, path).is_some_and(|p| p.text.starts_with(text))
            {
                return (index, *y);
            }
        }
    }
    panic!("no paragraph starts with {text}")
}

#[test]
fn a_partial_page_setup_changes_only_the_fields_it_names() {
    let mut s = session();
    run(&mut s, "page.size", json!({ "name": "A4" }));
    let set = run(&mut s, "page.pageSetup", json!({ "section": { "marginLeft": 90, "valign": "center" } }));
    assert_eq!(set["marginLeft"], 90.0);
    assert_eq!(set["valign"], "center");
    // Still A4 (595.3 x 841.9 pt), with the other margins as they were.
    assert!((set["pageW"].as_f64().unwrap() - 595.3).abs() < 0.5, "{set}");
    assert!((set["pageH"].as_f64().unwrap() - 841.9).abs() < 0.5, "{set}");
    assert_eq!(set["marginRight"], 72.0);
    // A field it does not know is refused, not dropped.
    assert!(s.run("page.pageSetup", &json!({ "section": { "pageWidth": 400 } })).is_err());
}

#[test]
fn a_size_wider_than_tall_is_landscape() {
    let mut s = session();
    let set = run(&mut s, "page.size", json!({ "width": 504, "height": 360 }));
    assert_eq!((set["pageW"].as_f64(), set["pageH"].as_f64(), set["landscape"].as_bool()), (Some(504.0), Some(360.0), Some(true)));
    let set = run(&mut s, "page.size", json!({ "width": 360, "height": 504 }));
    assert_eq!((set["pageW"].as_f64(), set["pageH"].as_f64(), set["landscape"].as_bool()), (Some(360.0), Some(504.0), Some(false)));
    // A named paper keeps the orientation the section has.
    run(&mut s, "page.orientation", json!({ "value": "landscape" }));
    let set = run(&mut s, "page.size", json!({ "name": "A5" }));
    assert!(set["landscape"].as_bool().unwrap() && set["pageW"].as_f64() > set["pageH"].as_f64(), "{set}");
}

#[test]
fn vertical_alignment_centres_the_page_even_when_another_section_follows() {
    let mut s = session();
    type_paragraphs(&mut s, &["Title page"]);
    let top = where_is(&mut s, "Title page").1;
    run(&mut s, "page.verticalAlign", json!({ "value": "center" }));
    let centred = where_is(&mut s, "Title page").1;
    assert!(centred > top + 250.0, "centred on the page: {top} -> {centred}");
    // A next-page section after it keeps the title page centred.
    run(&mut s, "page.break", json!({ "kind": "nextPage" }));
    run(&mut s, "page.verticalAlign", json!({ "value": "top" }));
    run(&mut s, "text.insert", json!({ "text": "Contents" }));
    let (page, y) = where_is(&mut s, "Title page");
    assert_eq!(page, 0);
    assert!((y - centred).abs() < 1.0, "still centred: {y} vs {centred}");
    let (page, y) = where_is(&mut s, "Contents");
    assert_eq!(page, 1);
    assert!((y - top).abs() < 1.0, "the next section is top-aligned: {y} vs {top}");
    assert!(s.run("page.verticalAlign", &json!({ "value": "middle" })).is_err(), "an unknown alignment is refused");
}

/// A heading kept with a body paragraph under widow control: at the foot of a page (or column)
/// where the heading and only one line of the body fit, the heading moves on with the body.
fn kept_heading(columns: bool) -> (Session, usize) {
    let mut s = session();
    run(&mut s, "page.size", json!({ "width": 300, "height": 300 }));
    run(&mut s, "page.margins", json!({ "top": 20, "bottom": 20, "left": 20, "right": 20 }));
    if columns {
        run(&mut s, "page.columns", json!({ "count": 2, "space": 12 }));
    }
    run(
        &mut s,
        "style.set",
        json!({ "style": "Normal", "para": { "spaceBefore": 0, "spaceAfter": 0, "lineSpacing": { "rule": "exactly", "value": 20 } } }),
    );
    // 11 filler lines of 20 pt (220 pt of the 260 pt column), then the heading (20 pt): one
    // body line would fit below it, two would not.
    let filler: Vec<String> = (1..=11).map(|i| format!("Filler {i}")).collect();
    let filler: Vec<&str> = filler.iter().map(String::as_str).collect();
    type_paragraphs(&mut s, &filler);
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "paragraph.set", json!({ "props": { "keepNext": true } }));
    run(&mut s, "text.insert", json!({ "text": "Heading" }));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "paragraph.set", json!({ "props": { "keepNext": false, "widowControl": true } }));
    run(&mut s, "text.insert", json!({ "text": "Body one" }));
    run(&mut s, "text.lineBreak", json!({}));
    run(&mut s, "text.insert", json!({ "text": "Body two" }));
    run(&mut s, "text.lineBreak", json!({}));
    run(&mut s, "text.insert", json!({ "text": "Body three" }));
    let pages = s.layout().pages.len();
    (s, pages)
}

#[test]
fn keep_with_next_moves_a_heading_whose_next_paragraph_would_leave_an_orphan() {
    let (mut s, _) = kept_heading(false);
    let heading = where_is(&mut s, "Heading");
    let body = where_is(&mut s, "Body one");
    assert_eq!((heading.0, body.0), (1, 1), "the heading goes with its body to page 2");
    let (mut s, pages) = kept_heading(true);
    assert_eq!(pages, 1);
    let heading = where_is(&mut s, "Heading");
    let body = where_is(&mut s, "Body one");
    assert!((heading.1 - 20.0).abs() < 0.5, "the heading starts the second column: {heading:?}");
    assert!(body.1 > heading.1, "{body:?}");
}
