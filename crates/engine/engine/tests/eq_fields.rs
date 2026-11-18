use std::io::{Read, Write};

use kition_doc_engine::{
    doc::Document,
    io,
    layout::{self, Placed, display::Draw},
};

const FONT: &str = r#"<w:rFonts w:ascii="Source Sans 3" w:hAnsi="Source Sans 3"/>"#;

/// A run with `half_points` size holding `inner`.
fn run(half_points: u32, inner: &str) -> String {
    format!(r#"<w:r><w:rPr>{FONT}<w:sz w:val="{half_points}"/></w:rPr>{inner}</w:r>"#)
}

/// An uncached field whose code is split into runs of their own sizes; the field itself is 12pt.
fn field(code: &[(&str, u32)]) -> String {
    let mut out = run(24, r#"<w:fldChar w:fldCharType="begin"/>"#);
    for (text, size) in code {
        out += &run(*size, &format!(r#"<w:instrText xml:space="preserve">{text}</w:instrText>"#));
    }
    out + &run(24, r#"<w:fldChar w:fldCharType="end"/>"#)
}

fn text(t: &str) -> String {
    run(24, &format!(r#"<w:t xml:space="preserve">{t}</w:t>"#))
}

fn document(runs: &str) -> Document {
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p>{runs}</w:p><w:sectPr><w:pgSz w:w="6000" w:h="6000"/><w:pgMar w:top="400" w:bottom="400" w:left="400" w:right="400"/></w:sectPr></w:body></w:document>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    io::open_bytes("eq.docx", &zip.finish().unwrap().into_inner()).unwrap()
}

/// The paired sample's combined characters: two 6pt pieces, raised 6pt and lowered 2pt, overstruck.
fn overstrike() -> String {
    field(&[(r" EQ \o(\s\up 6(", 24), ("HHH", 12), (r"),\s\do 2(", 24), ("HHH", 12), ("))", 24)])
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

/// Every text draw: its size, text and glyph positions (x, baseline y) on the page.
fn draws(doc: &Document) -> Vec<(f32, String, Vec<(f32, f32)>)> {
    let result = measure(doc);
    layout::display::page_display(doc, &result.pages[0], &Default::default())
        .into_iter()
        .filter_map(|draw| match draw {
            Draw::Glyphs { size, text, glyphs, .. } => Some((size, text, glyphs.into_iter().map(|(_, x, y)| (x, y)).collect())),
            _ => None,
        })
        .collect()
}

/// Glyphs of the given size, in drawing order.
fn glyphs_of(draws: &[(f32, String, Vec<(f32, f32)>)], size: f32) -> Vec<(f32, f32)> {
    draws.iter().filter(|d| (d.0 - size).abs() < 0.001).flat_map(|d| d.2.clone()).collect()
}

/// Each `w:instrText` of the saved main document: (size in half points, text).
fn saved_code_runs(doc: &Document) -> Vec<(u32, String)> {
    let bytes = io::save_bytes("saved.docx", doc).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
    let mut out = Vec::new();
    for chunk in xml.split("<w:r>").skip(1) {
        let run = chunk.split("</w:r>").next().unwrap();
        let Some(at) = run.find("<w:instrText") else { continue };
        let body = &run[at..];
        let text = &body[body.find('>').unwrap() + 1..body.find("</w:instrText>").unwrap()];
        let size = run.split(r#"<w:sz w:val=""#).nth(1).map_or(0, |s| s.split('"').next().unwrap().parse().unwrap());
        out.push((size, text.to_string()));
    }
    out
}

#[test]
fn eq_fields_keep_each_code_run_format_after_saves() {
    let mut doc = document(&overstrike());
    for _ in 0..3 {
        let runs = saved_code_runs(&doc);
        let code: String = runs.iter().map(|r| r.1.as_str()).collect();
        assert_eq!(code.trim(), r"EQ \o(\s\up 6(HHH),\s\do 2(HHH))");
        let small: Vec<&str> = runs.iter().filter(|r| r.0 == 12).map(|r| r.1.as_str()).collect();
        assert_eq!(small, ["HHH", "HHH"], "the 6pt pieces keep their own size: {runs:?}");
        assert!(runs.iter().all(|r| r.0 == 12 || r.0 == 24), "{runs:?}");
        doc = io::open_bytes("saved.docx", &io::save_bytes("saved.docx", &doc).unwrap()).unwrap();
    }
}

#[test]
fn eq_fields_overstrike_draws_raised_and_lowered_pieces_in_their_own_size() {
    let doc = document(&(overstrike() + &text("X")));
    let all = draws(&doc);
    let small = glyphs_of(&all, 6.0);
    assert_eq!(small.len(), 6, "both pieces are drawn at 6pt: {all:?}");
    let after = glyphs_of(&all, 12.0);
    assert_eq!(after.len(), 1);
    let (x_after, base) = after[0];
    let mut pieces = small.clone();
    pieces.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)));
    let (upper, lower) = pieces.split_at(3);
    assert!(upper.iter().all(|g| (g.1 - (base - 6.0)).abs() < 0.01), "raised 6pt: {upper:?} base {base}");
    assert!(lower.iter().all(|g| (g.1 - (base + 2.0)).abs() < 0.01), "lowered 2pt: {lower:?} base {base}");
    assert!((upper[0].0 - lower[0].0).abs() < 0.01, "equal pieces overlap at the same x");
    let w = (upper[2].0 - upper[0].0) / 2.0;
    assert!(w > 1.0);
    assert!((x_after - (upper[0].0 + 3.0 * w)).abs() < 0.05, "the text after takes the overstrike's width: {x_after} {upper:?}");
    let shown: String = all.iter().filter(|d| (d.0 - 6.0).abs() < 0.001).map(|d| d.1.as_str()).collect();
    assert_eq!(shown, "HHHHHH", "the pieces stay selectable text");
}

#[test]
fn eq_fields_stay_one_editing_unit_as_wide_as_their_drawing() {
    let doc = document(&(overstrike() + &text("X")));
    let result = measure(&doc);
    let para = paragraph(&result);
    let objects: Vec<_> = para.clusters.iter().filter(|c| matches!(c.kind, layout::para::ClKind::Object(_))).collect();
    assert_eq!(objects.len(), 1);
    assert!(objects[0].adv > 1.0, "the field occupies its overstrike's width");
}

#[test]
fn eq_fields_overstrike_aligns_center_left_and_right() {
    for (switch, cells) in [("", 1.0), (r"\ac", 1.0), (r"\al", 0.0), (r"\ar", 2.0)] {
        let doc = document(&field(&[(&format!(r" EQ \o{switch}(H,HHH) "), 24)]));
        let all = draws(&doc);
        let single = all.iter().find(|d| d.2.len() == 1).unwrap_or_else(|| panic!("{switch}: {all:?}"));
        let wide = all.iter().find(|d| d.2.len() == 3).unwrap();
        let w = (wide.2[2].0 - wide.2[0].0) / 2.0;
        assert!((single.2[0].0 - (wide.2[0].0 + cells * w)).abs() < 0.05, "{switch}: {all:?}");
        assert!((single.2[0].1 - wide.2[0].1).abs() < 0.001);
    }
}

#[test]
fn eq_fields_read_escaped_separators_and_default_raise() {
    let doc = document(&(field(&[(r" EQ \o(\,,H)\s\up(H) ", 24)]) + &text("X")));
    let mut all = draws(&doc);
    all.sort_by(|a, b| a.2[0].0.total_cmp(&b.2[0].0));
    let mut shown: Vec<&str> = all.iter().map(|d| d.1.as_str()).collect();
    shown.sort();
    assert_eq!(shown, [",", "H", "H", "X"], "{all:?}");
    let (comma, first) = (all.iter().find(|d| d.1 == ",").unwrap(), all.iter().find(|d| d.1 == "H").unwrap());
    assert!(comma.2[0].0 > first.2[0].0 && comma.2[0].0 < all[3].2[0].0, "the escaped comma is overstruck with the first H: {all:?}");
    let base = all[3].2[0].1;
    assert_eq!(all[3].1, "X");
    let raised = &all[2];
    assert_eq!(raised.1, "H");
    assert!((raised.2[0].1 - (base - 2.0)).abs() < 0.01, "\\up defaults to 2 points: {all:?}");
    assert!((first.2[0].1 - base).abs() < 0.01);
}

#[test]
fn eq_fields_fraction_bracket_and_box_draw_their_parts() {
    let doc = document(&(field(&[(r" EQ \f(H,HH)\b(H)\x(H) ", 24)]) + &text("X")));
    let result = measure(&doc);
    let list = layout::display::page_display(&doc, &result.pages[0], &Default::default());
    let texts: Vec<(String, f32)> = list
        .iter()
        .filter_map(|d| match d {
            Draw::Glyphs { text, glyphs, .. } => Some((text.clone(), glyphs[0].2)),
            _ => None,
        })
        .collect();
    let base = texts.last().unwrap().1;
    let num = texts.iter().find(|t| t.0 == "H").unwrap();
    let den = texts.iter().find(|t| t.0 == "HH").unwrap();
    assert!(num.1 < base - 1.0 && den.1 > base + 1.0, "numerator above, denominator below: {texts:?}");
    assert!(texts.iter().any(|t| t.0 == "(") && texts.iter().any(|t| t.0 == ")"), "{texts:?}");
    let rules = list.iter().filter(|d| matches!(d, Draw::Line { .. } | Draw::Fill { .. })).count();
    assert!(rules >= 5, "a fraction rule and four box sides: {rules}");
}

#[test]
fn eq_fields_keep_the_line_metrics_of_their_own_run() {
    // Pieces inside the extents of the field's own 12pt run never make the line shallower.
    let plain = measure(&document(&text("H")));
    let inside = field(&[(r" EQ \o(\s\up 4(", 24), ("HHH", 12), (r"),\s\do 1(", 24), ("HHH", 12), ("))", 24)]);
    let eq = measure(&document(&inside));
    let (a, b) = (&paragraph(&plain).lines[0], &paragraph(&eq).lines[0]);
    assert!((a.baseline - b.baseline).abs() < 0.001, "{} vs {}", a.baseline, b.baseline);
    assert!((a.height - b.height).abs() < 0.001, "{} vs {}", a.height, b.height);
}

#[test]
fn eq_fields_phonetic_guides_skip_format_switches_and_size_the_guide_text() {
    // Word's Phonetic Guide: `\* hpsN` sizes the guide text in half points, `\* "Font:…"` names
    // its font; neither switch is drawn.
    let code = r#" EQ \* jc0 \* "Font:Source Sans 3" \* hps12 \o\ad(\s\up 9(HHHH),H)"#;
    let doc = document(&(field(&[(code, 24)]) + &text("X")));
    let all = draws(&doc);
    let mut shown: Vec<&str> = all.iter().map(|d| d.1.as_str()).collect();
    shown.sort();
    assert_eq!(shown, ["H", "HHHH", "X"], "{all:?}");
    let guide = all.iter().find(|d| d.1 == "HHHH").unwrap();
    let base = all.iter().find(|d| d.1 == "H").unwrap();
    let after = all.iter().find(|d| d.1 == "X").unwrap();
    assert!((guide.0 - 6.0).abs() < 0.001, "hps12 is 6pt: {all:?}");
    assert!((base.0 - 12.0).abs() < 0.001);
    assert!((guide.2[0].1 - (base.2[0].1 - 9.0)).abs() < 0.01, "raised 9pt");
    let w = (guide.2[3].0 - guide.2[0].0) / 3.0;
    let guide_w = 4.0 * w;
    assert!((base.2[0].0 - (guide.2[0].0 + (guide_w - 2.0 * w) / 2.0)).abs() < 0.05, "the base is centred under the guide: {all:?}");
    assert!((after.2[0].0 - (guide.2[0].0 + guide_w)).abs() < 0.05, "the field is as wide as its guide: {all:?}");
}
