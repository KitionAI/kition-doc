use super::*;
use kition_doc_doc::{
    InlineObject, Run,
    props::{CharProps, ControlMarks, LineSpacing},
    section::DocumentGrid,
};

fn document(text: &str, grid: bool) -> Document {
    let mut doc = Document::from_text(text);
    doc.styles.default_chr.font = Some("Source Sans 3".into());
    doc.styles.default_chr.size = Some(10.5);
    if grid {
        doc.last_section.grid = Some(DocumentGrid { kind: Some("lines".into()), line_pitch: Some(312), char_space: None });
    }
    for index in 0..doc.body.len() {
        let properties = &mut doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props;
        properties.line_spacing = Some(LineSpacing::Multiple(1.0));
        properties.space_before = Some(0.0);
        properties.space_after = Some(0.0);
    }
    doc
}

/// Each placed paragraph's first line: (page, height, baseline from the line's top, page y of its top).
fn lines(doc: &Document) -> Vec<(usize, f32, f32, f32)> {
    let result = layout(doc, &mut LayoutCache::new(), &LayoutOptions::default());
    let mut out = Vec::new();
    for (page, p) in result.pages.iter().enumerate() {
        for item in &p.items {
            if let Placed::Lines { para, y, l0, .. } = item {
                let line = &para.lines[*l0];
                out.push((page, line.height, line.baseline - line.top, *y));
            }
        }
    }
    out
}

fn mark(doc: &mut Document, index: usize, marks: ControlMarks) {
    doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props.control = Some(marks);
}

#[test]
fn a_content_controls_marks_make_room_on_its_first_and_last_lines() {
    // WPS for Mac: a 20 pt mark stands 22.5 pt above the baseline and 7.8 pt below it.
    let plain = lines(&document("One\nTwo\nThree", false));
    let mut doc = document("One\nTwo\nThree", false);
    mark(&mut doc, 0, ControlMarks { start: Some(20.0), end: None });
    mark(&mut doc, 2, ControlMarks { start: None, end: Some(20.0) });
    let marked = lines(&doc);
    for k in [0, 2] {
        assert!((marked[k].1 - 30.3).abs() < 0.01, "line {k}: {:?}", marked[k]);
        assert!((marked[k].2 - 22.5).abs() < 0.01, "line {k}: {:?}", marked[k]);
    }
    assert_eq!(marked[1].1, plain[1].1, "a line between the marks keeps its height");
}

#[test]
fn a_mark_without_a_size_takes_the_paragraph_styles_and_the_line_grid_rounds_it_up() {
    // At 10.5 pt the marks need 15.9 pt, more than one 15.6 pt grid row (WPS for Mac).
    let plain = lines(&document("One\nTwo\nThree", true));
    assert!(plain.iter().all(|l| (l.1 - 15.6).abs() < 0.01), "{plain:?}");
    let mut doc = document("One\nTwo\nThree", true);
    mark(&mut doc, 0, ControlMarks { start: Some(0.0), end: None });
    mark(&mut doc, 2, ControlMarks { start: None, end: Some(0.0) });
    let heights: Vec<f32> = lines(&doc).iter().map(|l| l.1).collect();
    assert!(heights.iter().zip([31.2, 15.6, 31.2]).all(|(h, e)| (h - e).abs() < 0.01), "{heights:?}");
}

#[test]
fn a_content_control_around_runs_makes_room_on_their_line() {
    let mut doc = document("ab cd", false);
    let control = CharProps { control: Some(ControlMarks { start: Some(20.0), end: Some(20.0) }), ..Default::default() };
    let p = doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    p.runs = vec![Run { len: 3, props: CharProps::default() }, Run { len: 2, props: control }];
    let line = lines(&doc)[0];
    assert!((line.1 - 30.3).abs() < 0.01 && (line.2 - 22.5).abs() < 0.01, "{line:?}");
}

#[test]
fn marks_after_a_page_break_stay_on_its_line_and_the_next_page_drops_the_space_before() {
    // WPS for Mac: the bookmark after the break starts no line on the next page, and the heading
    // after the break sits at the page's top without its space before.
    let mut doc = document("a\n\u{c}\nHeading", false);
    let p = doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap();
    p.insert_object(1, InlineObject::BookmarkStart { name: "_GoBack".into() }, &CharProps::default()).unwrap();
    p.insert_object(p.text.len(), InlineObject::BookmarkEnd { name: "_GoBack".into() }, &CharProps::default()).unwrap();
    doc.para_mut(StoryRef::Body, &Path::top(2)).unwrap().props.space_before = Some(12.0);
    let placed = lines(&doc);
    let second: Vec<_> = placed.iter().filter(|l| l.0 == 1).collect();
    assert_eq!(second.len(), 1, "only the heading is on page 2: {placed:?}");
    assert!((second[0].3 - doc.last_section.margin_top).abs() < 0.01, "{placed:?}");
}
