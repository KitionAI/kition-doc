use super::*;
use kition_doc_doc::props::{HeightRule, LineSpacing, NumRef, ParaProps, TableFloat};

fn paragraph(text: &str, height: f32) -> Paragraph {
    let mut paragraph = Paragraph::with_text(text, Default::default());
    paragraph.props = ParaProps {
        line_spacing: Some(LineSpacing::Exactly(height)),
        space_before: Some(0.0),
        space_after: Some(0.0),
        widow_control: Some(false),
        ..Default::default()
    };
    paragraph
}

fn fixture(filler: f32, text: &str) -> (Document, kition_doc_doc::Table) {
    let mut doc = Document::new();
    doc.last_section.page_h = 140.0;
    doc.last_section.margin_top = 10.0;
    doc.last_section.margin_bottom = 10.0;
    let mut heading = paragraph("Heading", 20.0);
    heading.props.keep_next = Some(true);
    doc.body = vec![kition_doc_doc::para_block(paragraph("Filler", filler)), kition_doc_doc::para_block(heading)];
    let mut table = kition_doc_doc::Table::new(1, 1, 400.0);
    table.props.cell_margins = Some([0.0; 4]);
    table.rows[0].cells[0].blocks = vec![kition_doc_doc::para_block(paragraph(text, 20.0))];
    (doc, table)
}

fn first_page(result: &DocLayout, block: u32) -> usize {
    result
        .pages
        .iter()
        .position(|page| {
            page.items.iter().any(|item| match item {
                Placed::Lines { path, .. } => path.0.first() == Some(&block),
                _ => false,
            })
        })
        .unwrap()
}

fn append(doc: &mut Document, table: kition_doc_doc::Table) -> DocLayout {
    doc.body.push(Arc::new(Block::Table(table)));
    layout(doc, &mut LayoutCache::new(), &LayoutOptions::default())
}

#[test]
fn keep_table_reserves_the_real_unsplittable_first_row() {
    let (mut doc, mut table) = fixture(40.0, "Cell");
    table.rows[0].props.height = Some(80.0);
    table.rows[0].props.height_rule = HeightRule::AtLeast;
    table.rows[0].props.cant_split = true;
    let result = append(&mut doc, table);
    assert_eq!(first_page(&result, 0), 0);
    assert_eq!(first_page(&result, 1), 1);
    assert_eq!(first_page(&result, 2), 1);
}

#[test]
fn keep_table_reserves_header_rows_and_unsplittable_blank_row_space() {
    for header in [false, true] {
        let (mut doc, mut table) = fixture(40.0, "Cell");
        table.rows[0].props.height = Some(80.0);
        table.rows[0].props.header = header;
        let result = append(&mut doc, table);
        assert_eq!(first_page(&result, 1), 1, "header={header}");
        assert_eq!(first_page(&result, 2), 1);
    }
}

#[test]
fn keep_table_reserves_only_the_first_piece_of_a_splittable_row() {
    let (mut doc, table) = fixture(70.0, "First\nSecond\nThird");
    let result = append(&mut doc, table);
    assert_eq!(first_page(&result, 1), 0);
    assert_eq!(first_page(&result, 2), 0);
    assert!(result.pages.len() > 1);
}

#[test]
fn keep_table_does_not_reserve_the_whole_table_when_its_first_row_fits() {
    let (mut doc, mut table) = fixture(70.0, "First");
    table.rows.push(table.rows[0].clone());
    table.rows[1].props.height = Some(80.0);
    table.rows[1].props.cant_split = true;
    let result = append(&mut doc, table);
    assert_eq!(first_page(&result, 1), 0);
    assert_eq!(first_page(&result, 2), 0);
}

#[test]
fn keep_table_does_not_reserve_flow_height_for_a_fitting_float() {
    let (mut doc, mut table) = fixture(90.0, "Cell");
    table.props.float = Some(TableFloat::default());
    let result = append(&mut doc, table);
    assert_eq!(first_page(&result, 1), 0);
}

#[test]
fn keep_table_preview_does_not_advance_visible_list_counters() {
    let (mut doc, mut table) = fixture(40.0, "Cell");
    let list = doc.numbering.add_list(kition_doc_doc::ListKind::Numbered);
    let numbered = Some(NumRef { num: list, level: 0 });
    doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.numbering = numbered;
    let cell = Arc::make_mut(&mut table.rows[0].cells[0].blocks[0]);
    let Block::Para(cell) = cell else { panic!() };
    cell.props.numbering = numbered;
    table.rows[0].props.height = Some(80.0);
    table.rows[0].props.cant_split = true;
    doc.body.push(Arc::new(Block::Table(table)));
    let mut after = paragraph("After", 20.0);
    after.props.numbering = numbered;
    doc.body.push(kition_doc_doc::para_block(after));
    let result = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    let labels: Vec<_> = result
        .pages
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Placed::Lines { para, l0: 0, .. } => para.label.as_ref().map(|label| label.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(labels, vec!["1.", "2.", "3."]);
}
