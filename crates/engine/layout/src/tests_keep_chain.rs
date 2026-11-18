use super::*;
use kition_doc_doc::props::{LineSpacing, ParaProps};

fn fixture(text: &str, filler: f32) -> Document {
    let mut doc = Document::from_text(text);
    doc.last_section.page_h = 140.0;
    doc.last_section.margin_top = 10.0;
    doc.last_section.margin_bottom = 10.0;
    for index in 0..doc.body.len() {
        let paragraph = doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap();
        paragraph.text = paragraph.text.replace('\u{000b}', "\n");
        paragraph.props = ParaProps {
            line_spacing: Some(LineSpacing::Exactly(if index == 0 { filler } else { 20.0 })),
            space_before: Some(0.0),
            space_after: Some(0.0),
            widow_control: Some(false),
            ..Default::default()
        };
    }
    doc
}

fn pages(doc: &Document) -> Vec<usize> {
    let layout = layout(doc, &mut LayoutCache::new(), &LayoutOptions::default());
    (0..doc.body.len())
        .map(|block| {
            layout
                .pages
                .iter()
                .position(|page| page.items.iter().any(|item| matches!(item, Placed::Lines { path, .. } if *path == Path::top(block))))
                .unwrap()
        })
        .collect()
}

#[test]
fn keep_chain_moves_all_linked_headings_with_the_following_line() {
    let mut doc = fixture("Filler\nHeading\nSubheading\nBody", 80.0);
    for index in 1..=2 {
        doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props.keep_next = Some(true);
    }
    assert_eq!(pages(&doc), vec![0, 1, 1, 1]);
}

#[test]
fn keep_chain_respects_keep_lines_on_the_final_paragraph() {
    let mut doc = fixture("Filler\nHeading\nFirst\u{000b}Second", 80.0);
    doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.keep_next = Some(true);
    doc.para_mut(StoryRef::Body, &Path::top(2)).unwrap().props.keep_lines = Some(true);
    assert_eq!(pages(&doc), vec![0, 1, 1]);
}

#[test]
fn keep_chain_stops_before_a_forced_page_break() {
    let mut doc = fixture("Filler\nHeading\nBody", 100.0);
    doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.keep_next = Some(true);
    doc.para_mut(StoryRef::Body, &Path::top(2)).unwrap().props.page_break_before = Some(true);
    assert_eq!(pages(&doc), vec![0, 0, 1]);
}

#[test]
fn keep_chain_stops_at_a_section_boundary() {
    let mut doc = fixture("Filler\nHeading\nBody", 100.0);
    let section = doc.last_section.clone();
    let heading = doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap();
    heading.props.keep_next = Some(true);
    heading.section = Some(Box::new(section));
    assert_eq!(pages(&doc), vec![0, 0, 1]);
}

#[test]
fn keep_chain_taller_than_a_page_starts_on_a_fresh_page() {
    let mut doc = fixture("Filler\nHeading\nSubheading\nBody", 40.0);
    for index in 1..=2 {
        let paragraph = doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap();
        paragraph.props.keep_next = Some(true);
        paragraph.props.line_spacing = Some(LineSpacing::Exactly(60.0));
    }
    let actual = pages(&doc);
    assert_eq!(actual[0], 0);
    assert_eq!(actual[1], 1, "an oversized keep chain must start on an empty page before flowing across pages");
    assert!(actual.windows(2).all(|pages| pages[0] <= pages[1]));
}

#[test]
fn keep_chain_does_not_make_a_blank_page_for_leading_spacing() {
    let mut doc = fixture("Heading\nSubheading\nBody", 60.0);
    for index in 0..3 {
        let paragraph = doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap();
        paragraph.props.keep_next = Some(index < 2);
        paragraph.props.line_spacing = Some(LineSpacing::Exactly(60.0));
    }
    doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props.space_before = Some(20.0);
    assert_eq!(pages(&doc)[0], 0);
}
