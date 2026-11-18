use std::io::Write;

use kition_doc_engine::{
    doc::{Document, StoryRef, TextColor},
    io,
    layout::{self, Placed, display::Draw},
};

fn document() -> Document {
    let para = |t: &str| format!(r#"<w:p><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{}{}{}<w:p><w:pPr><w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/><w:lnNumType w:countBy="1" w:restart="continuous"/></w:sectPr></w:pPr></w:p><w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#,
        para("One"),
        para("Two"),
        para("Three")
    );
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="21"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in [("word/document.xml", xml.as_str()), ("word/styles.xml", styles), ("word/_rels/document.xml.rels", rels)] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    io::open_bytes("numbers.docx", &zip.finish().unwrap().into_inner()).unwrap()
}

#[test]
fn line_numbers_use_text_formatting_and_skip_the_section_break_mark() {
    // WPS references draw line numbers in the text's own size and colour, and leave the empty
    // paragraph that only carries a section break unnumbered.
    let doc = document();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let numbers: Vec<_> = result.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Placed::Lines { story: StoryRef::Part(u32::MAX), para, .. } => Some(para.clone()),
            _ => None,
        })
        .collect();
    let texts: Vec<String> = numbers.iter().map(|p| p.text_len.to_string()).collect();
    assert_eq!(numbers.len(), 3, "{texts:?}");
    for para in &numbers {
        let style = &para.styles[para.clusters[0].style as usize];
        assert!((style.size - 10.5).abs() < 0.001, "{}", style.size);
        assert_eq!(style.rc.color, TextColor::Auto);
    }
}

#[test]
fn line_numbers_are_drawn_as_selectable_text() {
    // WPS's PDF carries line numbers as text; Kition's drawn glyphs name their text too.
    let doc = document();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let text: String = layout::display::page_display(&doc, &result.pages[0], &Default::default())
        .into_iter()
        .filter_map(|draw| match draw {
            Draw::Glyphs { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    for n in ["1", "2", "3"] {
        assert!(text.contains(n), "{n} in {text:?}");
    }
}
