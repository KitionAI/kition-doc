use super::*;
use kition_doc_doc::props::{LineSpacing, NumRef, ParaProps};

#[test]
fn keep_list_labels_participate_in_following_paragraph_height_without_being_counted_twice() {
    let mut doc = Document::from_text("Filler\nHeading\nFollowing body");
    doc.styles.default_chr.font = Some("Source Sans 3".into());
    doc.styles.default_chr.size = Some(12.0);
    doc.last_section.page_w = 244.0;
    doc.last_section.page_h = 140.0;
    doc.last_section.margin_top = 10.0;
    doc.last_section.margin_bottom = 10.0;
    for index in 0..3 {
        doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props = ParaProps {
            line_spacing: Some(LineSpacing::Exactly(if index == 0 { 80.0 } else { 20.0 })),
            space_before: Some(0.0),
            space_after: Some(0.0),
            widow_control: Some(false),
            ..Default::default()
        };
    }
    doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.keep_next = Some(true);
    let number = doc.numbering.add_list(kition_doc_doc::ListKind::Numbered);
    let following = &mut doc.para_mut(StoryRef::Body, &Path::top(2)).unwrap().props;
    following.numbering = Some(NumRef { num: number, level: 0 });
    following.keep_lines = Some(true);
    let result = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    let page_of = |block| {
        result
            .pages
            .iter()
            .position(|page| page.items.iter().any(|item| matches!(item, Placed::Lines { path, .. } if *path == Path::top(block))))
            .unwrap()
    };
    assert_eq!(page_of(0), 0);
    assert_eq!(page_of(1), 1);
    assert_eq!(page_of(2), 1);
    let labels: Vec<_> = result
        .pages
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Placed::Lines { para, l0: 0, .. } => para.label.as_ref().map(|label| label.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(labels, vec!["1."]);
}
