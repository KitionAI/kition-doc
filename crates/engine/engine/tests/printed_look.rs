//! What a page draws as printed: lines between columns in every column
//! region, and automatic text colour turning white on dark shading.

use kition_doc_engine::{
    Session,
    doc::{Document, Rgb},
    layout::{Placed, display},
};
use serde_json::{Value, json};

fn run(s: &mut Session, command: &str, params: Value) -> Value {
    s.run(command, &params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

/// Vertical rules on the first page: (x, top, bottom).
fn separators(s: &mut Session) -> Vec<(f32, f32, f32)> {
    let layout = s.layout();
    layout.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Placed::Rule { x0, y0, x1, y1, .. } if (x0 - x1).abs() < 0.01 && (y1 - y0).abs() > 1.0 => Some((*x0, *y0, *y1)),
            _ => None,
        })
        .collect()
}

fn words(n: usize) -> String {
    (0..n).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ")
}

#[test]
fn a_continuous_section_draws_the_line_between_its_columns() {
    let mut s = Session::new(Document::new());
    run(&mut s, "text.insert", json!({ "text": "Masthead" }));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "page.break", json!({ "kind": "continuous" }));
    run(&mut s, "page.columns", json!({ "count": 2, "space": 24, "separator": true }));
    run(&mut s, "text.insert", json!({ "text": words(200) }));
    let lines = separators(&mut s);
    assert_eq!(lines.len(), 1, "one line between the two columns: {lines:?}");
    let (x, top, bottom) = lines[0];
    assert!((x - 306.0).abs() < 0.5, "midway between the columns: {x}");
    assert!(top > 80.0, "it starts where the columns start, below the masthead: {top}");
    assert!((bottom - 720.0).abs() < 0.5, "the columns run to the next page: down to the bottom margin ({bottom})");

    // Ended by another continuous break, the columns are balanced and the line ends with them.
    let mut s = Session::new(Document::new());
    run(&mut s, "text.insert", json!({ "text": "Masthead" }));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "page.break", json!({ "kind": "continuous" }));
    run(&mut s, "page.columns", json!({ "count": 2, "space": 24, "separator": true }));
    run(&mut s, "text.insert", json!({ "text": words(60) }));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "page.break", json!({ "kind": "continuous" }));
    run(&mut s, "page.columns", json!({ "count": 1 }));
    run(&mut s, "text.insert", json!({ "text": "Footer story" }));
    let lines = separators(&mut s);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let (_, top, bottom) = lines[0];
    // The balanced columns end level, the line with their text (the empty paragraph that only
    // carries the section break takes no room, as WPS for Mac lays it out; maintainer-approved
    // change), and the next section starts below.
    let layout = s.layout();
    let mut column_bottoms = Vec::new();
    let mut next_section = None;
    for item in &layout.pages[0].items {
        if let Placed::Lines { path, para, l0, l1, y, .. } = item
            && *y > top - 1.0
        {
            let first = para.lines[*l0].top;
            let end = para.lines.get(l1 - 1).map_or(*y, |line| y + line.top - first + line.height);
            let p = s.doc.para(kition_doc_engine::doc::StoryRef::Body, path);
            if p.is_some_and(|p| p.text.starts_with("Footer")) {
                next_section = Some(*y);
            } else if !p.is_some_and(|p| p.section.is_some() && p.text.is_empty()) {
                column_bottoms.push(end);
            }
        }
    }
    let tallest = column_bottoms.iter().copied().fold(0.0, f32::max);
    assert!(top > 80.0 && (bottom - tallest).abs() < 0.5, "from {top} to the tallest column's bottom {tallest}: {bottom}");
    assert!(next_section.is_some_and(|y| y >= bottom), "the next section starts below the line: {next_section:?}");
    assert!(bottom < 400.0, "well above the bottom margin: {bottom}");
}

#[test]
fn no_line_is_drawn_beside_an_empty_column() {
    // WPS for Mac draws a line between columns only beside a column that holds something.
    let mut s = Session::new(Document::new());
    run(&mut s, "page.columns", json!({ "count": 3, "space": 24, "separator": true }));
    run(&mut s, "text.insert", json!({ "text": "Only the first column has text." }));
    assert_eq!(separators(&mut s), vec![]);
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "page.break", json!({ "kind": "column" }));
    run(&mut s, "text.insert", json!({ "text": "The second column." }));
    let lines = separators(&mut s);
    assert_eq!(lines.len(), 1, "a line before the second column only: {lines:?}");
    assert_eq!((lines[0].1, lines[0].2), (72.0, 720.0), "a page of columns: the line runs margin to margin");
}

/// The colour of the glyphs reading `text` on the first page.
fn glyph_color(s: &mut Session, text: &str) -> Rgb {
    let layout = s.layout();
    display::page_display(&s.doc, &layout.pages[0], &Default::default())
        .into_iter()
        .find_map(|draw| match draw {
            display::Draw::Glyphs { color, text: drawn, .. } if drawn.contains(text) => Some(color),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no glyphs reading {text}"))
}

#[test]
fn automatic_text_is_white_on_dark_cell_and_paragraph_shading() {
    let mut s = Session::new(Document::new());
    run(&mut s, "table.insert", json!({ "rows": 1, "cols": 2 }));
    run(&mut s, "table.shading", json!({ "color": "0D5C63" }));
    run(&mut s, "text.insert", json!({ "text": "Dark" }));
    run(&mut s, "text.tab", json!({}));
    run(&mut s, "table.shading", json!({ "color": "F3E9E2" }));
    run(&mut s, "text.insert", json!({ "text": "Light" }));
    assert_eq!(glyph_color(&mut s, "Dark"), Rgb::WHITE, "automatic text on a dark cell");
    assert_eq!(glyph_color(&mut s, "Light"), Rgb::BLACK, "automatic text on a light cell");
    // A colour of its own stays as it is.
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "paragraph.shading", json!({ "color": "1F1F1F" }));
    run(&mut s, "text.insert", json!({ "text": "Shaded paragraph" }));
    assert_eq!(glyph_color(&mut s, "Shaded"), Rgb::WHITE, "automatic text on a dark paragraph shading");
}
