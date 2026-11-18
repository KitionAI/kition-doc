use std::io::Write;
use std::sync::Arc;

use kition_doc_engine::{
    doc::{Document, Path, StoryRef},
    io,
    layout::{self, Placed, para::ParaLayout},
};
use serde_json::json;

fn document(runs: &str, paragraph: &str) -> Document {
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr>{paragraph}</w:pPr>{runs}</w:p><w:sectPr><w:pgSz w:w="3400" w:h="6000"/><w:pgMar w:top="200" w:bottom="200" w:left="200" w:right="200"/></w:sectPr></w:body></w:document>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    io::open_bytes("fit.docx", &zip.finish().unwrap().into_inner()).unwrap()
}

fn run(text: &str, fit: &str, extra: &str) -> String {
    format!(r#"<w:r><w:rPr><w:rFonts w:ascii="Source Sans 3"/><w:sz w:val="24"/>{fit}{extra}</w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

fn laid(doc: &Document) -> Arc<ParaLayout> {
    layout::layout(doc, &mut layout::LayoutCache::new(), &Default::default()).pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Placed::Lines { para, .. } => Some(para.clone()),
            _ => None,
        })
        .unwrap()
}

fn width(para: &ParaLayout, start: usize, end: usize) -> f32 {
    para.clusters.iter().filter(|c| c.start >= start && c.end <= end).map(|c| c.adv).sum()
}

#[test]
fn fit_text_preserves_width_and_signed_id_over_three_save_cycles() {
    let mut doc = document(&run("Manual width", r#"<w:fitText w:val="1800" w:id="-42"/>"#, ""), "");
    for _ in 0..3 {
        let para = doc.para(StoryRef::Body, &Path::top(0)).unwrap();
        let props = serde_json::to_value(&para.runs[0].props).unwrap();
        assert_eq!(props["fitText"], json!({"width":90.0,"id":-42}));
        assert!((width(&laid(&doc), 0, para.text.len()) - 90.0).abs() < 0.01);
        doc = io::open_bytes("saved.docx", &io::save_bytes("saved.docx", &doc).unwrap()).unwrap();
    }
}

#[test]
fn fit_text_contiguous_id_shares_one_width_across_formatting_and_keeps_carets() {
    let fit = r#"<w:fitText w:val="2000" w:id="17"/>"#;
    let doc = document(&(run("Wide", fit, "") + &run(" bold", fit, "<w:b/>") + &run(" tail", fit, "<w:i/>")), "");
    let para = laid(&doc);
    assert_eq!(para.lines.len(), 1);
    assert!((width(&para, 0, 14) - 100.0).abs() < 0.01);
    assert!(para.clusters.len() >= 12, "editable characters must retain separate source ranges");
    assert_eq!(para.clusters.first().unwrap().start, 0);
    assert_eq!(para.clusters.last().unwrap().end, 14);
    assert!(para.styles.iter().any(|s| s.rc.bold));
    assert!(para.styles.iter().any(|s| s.rc.italic));
}

#[test]
fn fit_text_missing_and_noncontiguous_ids_are_independent() {
    let anonymous = r#"<w:fitText w:val="600"/>"#;
    let same_id = r#"<w:fitText w:val="600" w:id="17"/>"#;
    for (runs, ranges) in [
        (run("AA", anonymous, "") + &run("AA", anonymous, ""), vec![(0, 2), (2, 4)]),
        (run("AA", same_id, "") + &run("-", "", "") + &run("AA", same_id, ""), vec![(0, 2), (3, 5)]),
    ] {
        let mut doc = document(&runs, "");
        for cycle in 0..3 {
            let para = laid(&doc);
            for &(start, end) in &ranges {
                assert!(
                    (width(&para, start, end) - 30.0).abs() < 0.01,
                    "cycle {cycle} range {start}..{end}: {} source runs {:?}",
                    width(&para, start, end),
                    doc.para(StoryRef::Body, &Path::top(0)).unwrap().runs
                );
            }
            doc = io::open_bytes("saved.docx", &io::save_bytes("saved.docx", &doc).unwrap()).unwrap();
        }
    }
}

#[test]
fn fit_text_overflows_as_one_group_and_moves_whole_after_prefix() {
    let fit = r#"<w:fitText w:val="3600" w:id="17"/>"#;
    let text = "Manual width remains on one line even beyond the right indent";
    let doc = document(&run(text, fit, r#"<w:w w:val="50"/>"#), "");
    let para = laid(&doc);
    assert_eq!(para.lines.len(), 1);
    assert!((para.lines[0].end_x() - para.lines[0].left - 180.0).abs() < 0.01);
    assert!(para.lines[0].end_x() > para.lines[0].right);
    let prefixed = document(&(run("Prefix ", "", "") + &run(text, fit, "")), "");
    let para = laid(&prefixed);
    assert_eq!(para.lines.len(), 2);
    assert_eq!(para.lines[1].start, 7);
    assert_eq!(para.lines[1].stop, 7 + text.len());
}

#[test]
fn fit_text_keeps_internal_spaces_at_fixed_width_under_distributed_alignment() {
    let doc = document(&run("A B C", r#"<w:fitText w:val="1200" w:id="17"/>"#, ""), r#"<w:jc w:val="distribute"/>"#);
    let para = laid(&doc);
    assert_eq!(para.lines.len(), 1);
    assert!((para.lines[0].end_x() - para.lines[0].xs[0] - 60.0).abs() < 0.01);
}

#[test]
fn fit_text_multiple_text_nodes_in_one_anonymous_run_share_its_width() {
    let doc = document(r#"<w:r><w:rPr><w:fitText w:val="1200"/></w:rPr><w:t>One</w:t><w:t> run</w:t></w:r>"#, "");
    let para = laid(&doc);
    assert!((width(&para, 0, 7) - 60.0).abs() < 0.01);
}

#[test]
fn fit_text_glyph_scale_matches_fitted_positions_without_changing_height() {
    let natural = document(&run("HH", "", ""), "");
    let original = laid(&natural);
    let natural_width = width(&original, 0, 2);
    for (twips, target) in [(600, 30.0), (2400, 120.0)] {
        let doc = document(&run("HH", &format!(r#"<w:fitText w:val="{twips}" w:id="7"/>"#), r#"<w:w w:val="50"/>"#), "");
        let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
        let draws = layout::display::page_display(&doc, &result.pages[0], &Default::default());
        let scales: Vec<f32> = draws
            .iter()
            .filter_map(|draw| match draw {
                layout::display::Draw::Glyphs { horizontal_scale, .. } => Some(*horizontal_scale),
                _ => None,
            })
            .collect();
        assert!(!scales.is_empty());
        for scale in scales {
            assert!((scale - target / natural_width).abs() < 0.001);
            assert_eq!(layout::display::glyph_width_scale(scale), scale);
        }
        let fitted = laid(&doc);
        assert!((width(&fitted, 0, 2) - target).abs() < 0.01);
        assert_eq!(fitted.lines[0].height, original.lines[0].height);
    }
}

#[test]
fn fit_text_invalid_widths_leave_natural_geometry_finite() {
    let natural = laid(&document(&run("Safe", "", ""), ""));
    for value in ["NaN", "inf", "-20", "0", "invalid"] {
        let para = laid(&document(&run("Safe", &format!(r#"<w:fitText w:val="{value}"/>"#), ""), ""));
        assert_eq!(width(&para, 0, 4), width(&natural, 0, 4));
        assert!(para.lines.iter().flat_map(|line| &line.xs).all(|x| x.is_finite()));
    }
}
