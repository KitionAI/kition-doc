use std::io::Write;

use kition_doc_engine::{
    doc::{Document, Path, Pos, StoryRef},
    io,
    layout::{self, Placed, display::Draw},
};
use serde_json::json;

fn run(text: &str, attributes: &str, formatting: &str) -> String {
    let settings = if attributes.is_empty() { String::new() } else { format!("<w:eastAsianLayout {attributes}/>") };
    format!(
        r#"<w:r><w:rPr><w:rFonts w:ascii="Source Sans 3"/><w:sz w:val="24"/>{settings}{formatting}</w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#
    )
}

fn document(runs: &str, paragraph: &str) -> Document {
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:spacing w:line="312" w:lineRule="auto"/>{paragraph}</w:pPr>{runs}</w:p><w:sectPr><w:pgSz w:w="3400" w:h="6000"/><w:pgMar w:top="200" w:bottom="200" w:left="200" w:right="200"/></w:sectPr></w:body></w:document>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    io::open_bytes("two-lines.docx", &zip.finish().unwrap().into_inner()).unwrap()
}

fn measure(doc: &Document) -> layout::DocLayout {
    layout::layout(doc, &mut layout::LayoutCache::new(), &Default::default())
}

fn paragraph(result: &layout::DocLayout) -> &layout::ParaLayout {
    result.pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Placed::Lines { para, .. } => Some(para.as_ref()),
            _ => None,
        })
        .unwrap()
}

fn glyphs(doc: &Document, result: &layout::DocLayout) -> Vec<(f32, f32, f32)> {
    layout::display::page_display(doc, &result.pages[0], &Default::default())
        .into_iter()
        .flat_map(|draw| match draw {
            Draw::Glyphs { size, glyphs, .. } => glyphs.into_iter().map(|(_, x, y)| (size, x, y)).collect(),
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn two_lines_settings_preserve_signed_ids_explicit_false_and_brackets() {
    for bracket in ["none", "round", "square", "angle", "curly"] {
        let mut doc =
            document(&run("ABCD", &format!(r#"w:id="-42" w:combine="0" w:combineBrackets="{bracket}" w:vert="1" w:vertCompress="0""#), ""), "");
        for _ in 0..3 {
            let props = serde_json::to_value(&doc.para(StoryRef::Body, &Path::top(0)).unwrap().runs[0].props).unwrap();
            assert_eq!(
                props["eastAsianLayout"],
                json!({"id":-42,"combine":false,"combineBrackets":bracket,"vertical":true,"verticalCompress":false})
            );
            doc = io::open_bytes("saved.docx", &io::save_bytes("saved.docx", &doc).unwrap()).unwrap();
        }
    }
}

#[test]
fn two_lines_shares_one_logical_line_and_draws_two_half_size_rows() {
    let text = "H".repeat(40);
    let doc = document(&run(&text, r#"w:id="2" w:combine="1""#, ""), "");
    let result = measure(&doc);
    let para = paragraph(&result);
    assert_eq!(para.lines.len(), 1);
    assert_eq!(para.clusters.len(), 40, "source grapheme boundaries remain editable");
    let ink = glyphs(&doc, &result);
    assert_eq!(ink.len(), 40);
    assert!(ink.iter().all(|g| (g.0 - 6.0).abs() < 0.001));
    assert!(ink[..20].iter().all(|g| (g.2 - ink[0].2).abs() < 0.001));
    assert!(ink[20..].iter().all(|g| (g.2 - ink[20].2).abs() < 0.001));
    assert!(ink[20].2 > ink[0].2 + 5.0);
    assert!((ink[20].1 - ink[0].1).abs() < 0.001);
    let natural = document(&run("H", "", ""), "");
    let normal = measure(&natural);
    assert!((para.lines[0].height - paragraph(&normal).lines[0].height).abs() < 0.001);
    let expected = paragraph(&normal).clusters[0].adv * 10.0;
    assert!((para.lines[0].end_x() - para.lines[0].left - expected).abs() < 0.01);
}

#[test]
fn two_lines_contiguous_formatting_runs_share_rows_and_retain_font_styles() {
    let attrs = r#"w:id="2" w:combine="1""#;
    let doc = document(&(run("HHHHHHHH", attrs, "<w:b/>") + &run("HHHHHHHH", attrs, "<w:i/>")), "");
    let result = measure(&doc);
    let ink = glyphs(&doc, &result);
    assert_eq!(paragraph(&result).lines.len(), 1);
    assert_eq!(ink.len(), 16);
    assert!(ink[..8].iter().all(|g| (g.2 - ink[0].2).abs() < 0.001));
    assert!(ink[8..].iter().all(|g| (g.2 - ink[8].2).abs() < 0.001));
    assert!(ink[8].2 > ink[0].2 + 5.0);
    let used: Vec<_> = paragraph(&result).clusters.iter().map(|c| &paragraph(&result).styles[c.style as usize]).collect();
    assert!(used[..8].iter().all(|style| style.rc.bold));
    assert!(used[8..].iter().all(|style| style.rc.italic));
}

#[test]
fn two_lines_anonymous_and_noncontiguous_groups_remain_independent_after_saves() {
    for attrs in [r#"w:combine="1""#, r#"w:id="2" w:combine="1""#] {
        let separator = if attrs.contains("id") { run("-", "", "") } else { String::new() };
        let mut doc = document(&(run("HHHHHHHH", attrs, "") + &separator + &run("HHHHHHHH", attrs, "")), "");
        for _ in 0..3 {
            let result = measure(&doc);
            let ink = glyphs(&doc, &result);
            assert!(ink.iter().filter(|g| (g.0 - 6.0).abs() < 0.001).count() == 16);
            assert!((ink[4].1 - ink[0].1).abs() < 0.001);
            assert!(ink[4].2 > ink[0].2 + 5.0);
            doc = io::open_bytes("saved.docx", &io::save_bytes("saved.docx", &doc).unwrap()).unwrap();
        }
    }
}

#[test]
fn two_lines_mouse_and_caret_distinguish_the_sub_rows() {
    let doc = document(&run(&"H".repeat(40), r#"w:id="2" w:combine="1""#, ""), "");
    let result = measure(&doc);
    let upper = Pos::new(StoryRef::Body, Path::top(0), 1);
    let lower = Pos::new(StoryRef::Body, Path::top(0), 21);
    let a = result.caret(&upper).unwrap();
    let b = result.caret(&lower).unwrap();
    assert_eq!(a.page, b.page);
    assert!((a.x - b.x).abs() < 0.01);
    assert!(b.top > a.top + 5.0);
    assert_eq!(result.hit(a.page, a.x, a.top + a.height / 2.0, StoryRef::Body).unwrap(), upper);
    assert_eq!(result.hit(b.page, b.x, b.top + b.height / 2.0, StoryRef::Body).unwrap(), lower);
    assert_eq!(result.vertical(&upper, None, 1, 0).unwrap().0, lower);
    assert_eq!(result.vertical(&lower, None, -1, 0).unwrap().0, upper);
    assert!(matches!(result.visual_step(&Pos::new(StoryRef::Body, Path::top(0), 19), false, 0),
        Some(layout::VisualStep::Moved(position)) if position.off == 20));
    let upper_rects = result.selection_rects(&doc, &upper, &Pos::new(StoryRef::Body, Path::top(0), 2), 0);
    let lower_rects = result.selection_rects(&doc, &lower, &Pos::new(StoryRef::Body, Path::top(0), 22), 0);
    assert_eq!(upper_rects.len(), 1);
    assert_eq!(lower_rects.len(), 1);
    assert!((upper_rects[0].1.x - lower_rects[0].1.x).abs() < 0.01);
    assert!(lower_rects[0].1.y > upper_rects[0].1.y + 5.0);
    assert!(upper_rects[0].1.h < paragraph(&result).lines[0].height);
}

#[test]
fn two_lines_highlights_follow_sub_row_bands() {
    let doc = document(&run("HHHHHHHH", r#"w:id="2" w:combine="1""#, r#"<w:highlight w:val="yellow"/>"#), "");
    let result = measure(&doc);
    let rects: Vec<_> = layout::display::page_display(&doc, &result.pages[0], &Default::default())
        .into_iter()
        .filter_map(|draw| match draw {
            Draw::Fill { rect, color: kition_doc_engine::doc::Rgb(255, 255, 0), .. } => Some(rect),
            _ => None,
        })
        .collect();
    assert_eq!(rects.len(), 8);
    assert!(rects.iter().all(|rect| rect.h < paragraph(&result).lines[0].height));
    assert!(rects[4].y > rects[0].y + 5.0);
}

#[test]
fn two_lines_odd_group_trailing_caret_follows_the_last_visible_glyph() {
    let doc = document(&run("HHHHH", r#"w:id="2" w:combine="1""#, ""), "");
    let result = measure(&doc);
    let ink = glyphs(&doc, &result);
    let caret = result.caret(&Pos::new(StoryRef::Body, Path::top(0), 5)).unwrap();
    let advance = ink[4].1 - ink[3].1;
    assert!((caret.x - ink[4].1 - advance).abs() < 0.01);
    let hit = result.hit(caret.page, caret.x, caret.top + caret.height * 0.5, StoryRef::Body).unwrap();
    assert_eq!(hit.off, 5);
}

#[test]
fn two_lines_group_moves_intact_after_prefix_and_keeps_distribution_internal_width() {
    let attrs = r#"w:id="2" w:combine="1""#;
    let doc = document(&(run("Prefix ", "", "") + &run(&"H".repeat(80), attrs, "")), "");
    let result = measure(&doc);
    let para = paragraph(&result);
    assert_eq!(para.lines.len(), 2);
    assert_eq!(para.lines[1].start, 7);
    assert_eq!(para.lines[1].stop, 87);
    let plain = document(&run("HHHHHHHH", attrs, ""), "");
    let distributed = document(&run("HHHHHHHH", attrs, ""), r#"<w:jc w:val="distribute"/>"#);
    assert_eq!(glyphs(&plain, &measure(&plain)), glyphs(&distributed, &measure(&distributed)));
}
