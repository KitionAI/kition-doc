use super::*;
use kition_doc_doc::props::{LineSpacing, ParaProps};

fn fixture(text: &str) -> Document {
    let mut doc = Document::from_text(text);
    doc.last_section.page_h = 140.0;
    doc.last_section.margin_top = 10.0;
    doc.last_section.margin_bottom = 10.0;
    for index in 0..doc.body.len() {
        doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props = ParaProps {
            line_spacing: Some(LineSpacing::Exactly(150.0)),
            space_before: Some(20.0),
            space_after: Some(0.0),
            widow_control: Some(true),
            ..Default::default()
        };
    }
    doc
}

fn page_for(result: &DocLayout, block: usize) -> usize {
    result
        .pages
        .iter()
        .position(|page| page.items.iter().any(|item| matches!(item, Placed::Lines { path, .. } if *path == Path::top(block))))
        .unwrap()
}

#[test]
fn column_start_spacing_does_not_create_a_blank_page_for_an_oversized_line() {
    let doc = fixture("Oversized first line");
    let result = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    assert_eq!(result.pages.len(), 1);
    assert_eq!(page_for(&result, 0), 0);
    let y = result.pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Placed::Lines { y, .. } => Some(*y),
            _ => None,
        })
        .unwrap();
    assert_eq!(y, 30.0, "the explicitly requested leading space remains present");
}

#[test]
fn column_start_spacing_does_not_move_an_orphan_from_an_empty_page() {
    let mut doc = fixture("First");
    let paragraph = doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    let properties = paragraph.props.clone();
    *paragraph = Paragraph::with_text("First\nSecond", Default::default());
    paragraph.props = properties;
    paragraph.props.line_spacing = Some(LineSpacing::Exactly(70.0));
    let result = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    assert_eq!(result.pages.len(), 2);
    assert_eq!(page_for(&result, 0), 0);
    assert!(result.pages.iter().all(|page| page.items.iter().any(|item| matches!(item, Placed::Lines { .. }))));
}

#[test]
fn column_start_spacing_still_moves_an_oversized_line_after_existing_content() {
    let mut doc = fixture("Filler\nOversized");
    let filler = doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    filler.props.line_spacing = Some(LineSpacing::Exactly(20.0));
    filler.props.space_before = Some(0.0);
    let result = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    assert_eq!(result.pages.len(), 2);
    assert_eq!(page_for(&result, 0), 0);
    assert_eq!(page_for(&result, 1), 1);
}
