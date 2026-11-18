use std::io::Write;

use kition_doc_engine::{Session, io};
use kition_doc_layout::Placed;

fn document(properties: &str, grid: &str) -> Session {
    let lines = (0..60).map(|_| "<w:t>Grid line</w:t><w:br/>").collect::<String>();
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr>{properties}</w:pPr><w:r><w:rPr><w:sz w:val="20"/></w:rPr>{lines}</w:r></w:p><w:sectPr><w:pgSz w:w="6000" w:h="6000"/><w:pgMar w:top="200" w:bottom="200" w:left="200" w:right="200"/>{grid}</w:sectPr></w:body></w:document>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    Session::new(io::open_bytes("grid.docx", &bytes).unwrap())
}

const GRID: &str = r#"<w:docGrid w:type="lines" w:linePitch="480" w:charSpace="-12"/>"#;

fn heights(session: &mut Session) -> Vec<f32> {
    session
        .layout()
        .pages
        .iter()
        .flat_map(|page| &page.items)
        .flat_map(|item| match item {
            Placed::Lines { para, l0, l1, .. } => para.lines[*l0..*l1].iter().map(|line| line.height).collect(),
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn document_grid_pitch_controls_line_height_and_pagination() {
    let mut grid = document("", GRID);
    let mut free = document("", "");
    assert!(heights(&mut grid).iter().all(|height| (*height - 24.0).abs() < 0.01));
    assert!(grid.layout().pages.len() > free.layout().pages.len());
}

#[test]
fn exact_spacing_and_paragraph_opt_out_override_the_grid() {
    let mut off = document(r#"<w:snapToGrid w:val="0"/>"#, GRID);
    let mut free = document("", "");
    assert_eq!(heights(&mut off), heights(&mut free));
    let mut exact = document(r#"<w:spacing w:line="200" w:lineRule="exact"/>"#, GRID);
    assert!(heights(&mut exact).iter().all(|height| (*height - 10.0).abs() < 0.01));
}

#[test]
fn section_grid_survives_docx_save_and_reopen() {
    let grid = document("", GRID);
    let before = serde_json::to_value(&grid.doc.last_section).unwrap();
    assert_eq!(before["grid"]["linePitch"], 480);
    assert_eq!(before["grid"]["charSpace"], -12);
    let saved = kition_doc_docx::write(&grid.doc).unwrap();
    let reopened = io::open_bytes("saved.docx", &saved).unwrap();
    assert_eq!(serde_json::to_value(&reopened.last_section).unwrap()["grid"], before["grid"]);
}

#[test]
fn grid_opt_out_is_inherited_from_document_defaults() {
    let mut grid = document("", GRID);
    grid.doc.styles.default_para.snap_to_grid = Some(false);
    let mut free = document("", "");
    assert_eq!(heights(&mut grid), heights(&mut free));
}

#[test]
fn table_cells_use_the_grid_only_with_the_compatibility_flag() {
    let mut grid = document("", GRID);
    let mut table = kition_doc_doc::Table::new(1, 1, 280.0);
    table.rows[0].cells[0].blocks = grid.doc.body.clone();
    grid.doc.body = vec![std::sync::Arc::new(kition_doc_doc::Block::Table(table))];
    assert!(heights(&mut grid).iter().all(|height| *height < 24.0));
    grid.doc.settings.compat_flags.push("adjustLineHeightInTable".into());
    grid.relayout();
    assert!(heights(&mut grid).iter().all(|height| (*height - 24.0).abs() < 0.01));
}

#[test]
fn disabled_and_invalid_grids_do_not_change_line_heights() {
    let mut free = document("", "");
    let expected = heights(&mut free);
    for grid in [
        r#"<w:docGrid w:type="default" w:linePitch="480"/>"#,
        r#"<w:docGrid w:type="snapToChars" w:linePitch="480"/>"#,
        r#"<w:docGrid w:type="lines" w:linePitch="-1"/>"#,
    ] {
        assert_eq!(heights(&mut document("", grid)), expected);
    }
}
