use super::*;
use kition_doc_doc::props::{LineSpacing, NumRef, ParaProps};

fn document(text: &str) -> Document {
    let mut doc = Document::from_text(text);
    doc.styles.default_chr.font = Some("Source Sans 3".into());
    doc.styles.default_chr.size = Some(12.0);
    doc.last_section.margin_top = 10.0;
    for index in 0..doc.body.len() {
        doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props = ParaProps {
            line_spacing: Some(LineSpacing::Exactly(20.0)),
            space_before: Some(99.0),
            space_after: Some(87.0),
            space_before_lines: Some(900),
            space_after_lines: Some(800),
            space_before_auto: Some(true),
            space_after_auto: Some(true),
            widow_control: Some(false),
            ..Default::default()
        };
    }
    doc
}

fn paragraphs(layout: &DocLayout) -> Vec<(usize, f32, Arc<ParaLayout>)> {
    layout
        .pages
        .iter()
        .enumerate()
        .flat_map(|(page, p)| {
            p.items.iter().filter_map(move |item| match item {
                Placed::Lines { y, para, .. } => Some((page, *y, para.clone())),
                _ => None,
            })
        })
        .collect()
}

fn measure(doc: &Document) -> DocLayout {
    layout(doc, &mut LayoutCache::new(), &Default::default())
}

#[test]
fn automatic_paragraph_spacing_replaces_points_and_line_units_and_collapses() {
    let doc = document("First\nSecond\nThird");
    let lines = paragraphs(&measure(&doc));
    assert_eq!(lines.len(), 3);
    assert!((lines[0].1 - 10.0).abs() < 0.01, "automatic space before drops at the document start");
    for pair in lines.windows(2) {
        assert!((pair[1].1 - pair[0].1 - pair[0].2.height - 14.0).abs() < 0.01, "HTML automatic margins overlap at fourteen points");
    }
    assert_eq!(doc.para(StoryRef::Body, &Path::top(0)).unwrap().props.space_before, Some(99.0), "layout preserves the source constraint");
}

#[test]
fn automatic_paragraph_spacing_drops_before_a_new_page() {
    let mut doc = document("First\nSecond");
    doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.page_break_before = Some(true);
    let lines = paragraphs(&measure(&doc));
    assert_eq!(lines[1].0, 1);
    assert!((lines[1].1 - 10.0).abs() < 0.01);
}

#[test]
fn automatic_paragraph_spacing_keeps_list_interiors_together() {
    let mut doc = document("Before\nItem one\nNested item\nItem two\nAfter");
    let num = doc.numbering.add_list(kition_doc_doc::ListKind::Numbered);
    doc.numbering
        .nums
        .iter_mut()
        .find(|list| list.id == num)
        .unwrap()
        .level_overrides
        .push((1, kition_doc_doc::numbering::Level { text: "%2.".into(), ..Default::default() }));
    for index in 1..=3 {
        doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props.numbering = Some(NumRef { num, level: u8::from(index == 2) });
    }
    let lines = paragraphs(&measure(&doc));
    assert_eq!(lines.len(), 5);
    for (index, pair) in lines.windows(2).enumerate() {
        let expected = if index == 0 || index == 3 { 14.0 } else { 0.0 };
        assert!((pair[1].1 - pair[0].1 - pair[0].2.height - expected).abs() < 0.01, "list boundary {index}");
    }
    let labels: Vec<_> = lines.iter().filter_map(|(_, _, p)| p.label.as_ref().map(|l| l.text.as_str())).collect();
    assert_eq!(labels, vec!["1.", "1.", "2."]);
}

#[test]
fn automatic_paragraph_spacing_rechecks_neighbors_of_cached_paragraphs() {
    let mut doc = document("First list\nAnother list");
    let first_num = doc.numbering.add_list(kition_doc_doc::ListKind::Numbered);
    let other_num = doc.numbering.add_list(kition_doc_doc::ListKind::Numbered);
    doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props.numbering = Some(NumRef { num: first_num, level: 0 });
    doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.numbering = Some(NumRef { num: other_num, level: 0 });
    let mut cache = LayoutCache::new();
    let separate = paragraphs(&layout(&doc, &mut cache, &Default::default()));
    assert!((separate[1].1 - separate[0].1 - separate[0].2.height - 14.0).abs() < 0.01);
    doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.numbering = Some(NumRef { num: first_num, level: 0 });
    let joined = paragraphs(&layout(&doc, &mut cache, &Default::default()));
    assert!(Arc::ptr_eq(&separate[0].2, &joined[0].2), "an unchanged first paragraph keeps its shaped cache entry");
    assert!((joined[1].1 - joined[0].1 - joined[0].2.height).abs() < 0.01, "moving another item into the list changes its boundary margin");
}

#[test]
fn automatic_paragraph_spacing_preserves_cell_edges_and_internal_gaps() {
    let mut doc = document("First\nSecond");
    let mut table = kition_doc_doc::Table::new(1, 1, 300.0);
    table.rows[0].cells[0].blocks = doc.body.clone();
    doc.body = vec![Arc::new(Block::Table(table))];
    let result = measure(&doc);
    let lines = paragraphs(&result);
    assert_eq!(lines.len(), 2);
    assert!((lines[1].1 - lines[0].1 - lines[0].2.height - 14.0).abs() < 0.01);
    let cell_height = |layout: &DocLayout| {
        layout.pages[0]
            .items
            .iter()
            .find_map(|item| match item {
                Placed::Cell { rect, .. } => Some(rect.h),
                _ => None,
            })
            .unwrap()
    };
    let first_y = lines[0].1;
    let mut plain = doc.clone();
    if let Block::Table(table) = Arc::make_mut(&mut plain.body[0]) {
        for block in &mut table.rows[0].cells[0].blocks {
            if let Block::Para(p) = Arc::make_mut(block) {
                p.props.space_before_auto = Some(false);
                p.props.space_after_auto = Some(false);
                p.props.space_before_lines = Some(0);
                p.props.space_after_lines = Some(0);
                p.props.space_before = Some(0.0);
                p.props.space_after = Some(0.0);
            }
        }
    }
    let baseline = measure(&plain);
    assert!((first_y - paragraphs(&baseline)[0].1).abs() < 0.01, "no automatic margin before the first cell paragraph");
    assert!((cell_height(&result) - cell_height(&baseline) - 14.0).abs() < 0.01, "no automatic margin after the last cell paragraph");
}

#[test]
fn automatic_paragraph_spacing_respects_contextual_spacing() {
    let mut doc = document("First\nSecond");
    for index in 0..2 {
        doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props.contextual_spacing = Some(true);
    }
    let lines = paragraphs(&measure(&doc));
    assert!((lines[1].1 - lines[0].1 - lines[0].2.height).abs() < 0.01);
}

#[test]
fn automatic_paragraph_spacing_participates_in_keep_next_measurement() {
    let mut doc = document("Filler\nHeading\nFollowing");
    doc.last_section.page_h = 140.0;
    doc.last_section.margin_bottom = 10.0;
    let filler = &mut doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props;
    *filler = ParaProps { line_spacing: Some(LineSpacing::Exactly(70.0)), space_before: Some(0.0), space_after: Some(0.0), ..Default::default() };
    doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.keep_next = Some(true);
    let lines = paragraphs(&measure(&doc));
    assert_eq!(lines.iter().map(|l| l.0).collect::<Vec<_>>(), vec![0, 1, 1]);
}

#[test]
fn automatic_paragraph_spacing_fixed_policy_invalidates_cached_metrics() {
    let mut doc = document("First\nSecond");
    let mut cache = LayoutCache::new();
    let first = layout(&doc, &mut cache, &Default::default());
    let first = paragraphs(&first);
    assert!((first[0].2.rp.space_before - 14.0).abs() < 0.01);
    doc.settings.compat_flags.push("doNotUseHTMLParagraphAutoSpacing".into());
    let fixed = layout(&doc, &mut cache, &Default::default());
    let fixed = paragraphs(&fixed);
    assert_eq!(fixed[0].2.rp.space_before, 5.0);
    assert_eq!(fixed[0].2.rp.space_after, 10.0);
    assert!(!Arc::ptr_eq(&first[0].2, &fixed[0].2));
}
