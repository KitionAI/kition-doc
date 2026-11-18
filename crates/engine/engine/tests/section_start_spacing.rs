use std::io::Write;

use kition_doc_engine::{
    doc::Document,
    io,
    layout::{self, Placed},
};

fn document(next: &str) -> Document {
    let sect = |kind: &str| {
        let kind = if kind.is_empty() { String::new() } else { format!(r#"<w:type w:val="{kind}"/>"#) };
        format!(
            r#"<w:sectPr>{kind}<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#
        )
    };
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:spacing w:after="200"/>{}</w:pPr><w:r><w:t>End</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="480" w:after="0"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p>{}</w:body></w:document>"#,
        sect(""),
        sect(next)
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    io::open_bytes("sections.docx", &zip.finish().unwrap().into_inner()).unwrap()
}

/// The top of the second section's first line, and the page it is on.
fn heading(doc: &Document) -> (usize, f32) {
    let result = layout::layout(doc, &mut layout::LayoutCache::new(), &Default::default());
    result
        .pages
        .iter()
        .enumerate()
        .find_map(|(i, page)| {
            page.items.iter().find_map(|item| match item {
                Placed::Lines { path, para, y, .. } if path.0 == [1] => Some((i, y + para.lines[0].top)),
                _ => None,
            })
        })
        .unwrap()
}

#[test]
fn section_start_spacing_collapses_with_the_previous_paragraph_after() {
    // WPS references: a section's first paragraph at the top of its page keeps only the part
    // of its space before that exceeds the previous paragraph's space after (24 - 10 = 14 pt).
    let (page, top) = heading(&document("nextPage"));
    assert_eq!(page, 1);
    assert!((top - (72.0 + 14.0)).abs() < 0.01, "{top}");
}

#[test]
fn section_start_spacing_collapses_with_an_empty_marks_space_after() {
    // The empty paragraph that only ends a section before a new page is the paragraph the next
    // section's first space before collapses with (24 - 10 = 14 pt), though it takes no room.
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>End</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:after="200"/><w:sectPr>{page}</w:sectPr></w:pPr></w:p><w:p><w:pPr><w:spacing w:before="480" w:after="0"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p><w:sectPr><w:type w:val="nextPage"/>{page}</w:sectPr></w:body></w:document>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    let doc = io::open_bytes("mark-after.docx", &zip.finish().unwrap().into_inner()).unwrap();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let (page, top) = result
        .pages
        .iter()
        .enumerate()
        .find_map(|(i, page)| {
            page.items.iter().find_map(|item| match item {
                Placed::Lines { path, para, y, .. } if path.0 == [2] => Some((i, y + para.lines[0].top)),
                _ => None,
            })
        })
        .unwrap();
    assert_eq!(page, 1);
    assert!((top - (72.0 + 14.0)).abs() < 0.01, "{top}");
}

/// Two one-line paragraphs separated by an empty paragraph that only ends a section, the next
/// section starting `next`; the second paragraph's top on its page, and its page.
fn second_top(next: &str) -> (usize, f32) {
    let line = |t: &str| {
        format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#)
    };
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{}<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/><w:sectPr>{page}</w:sectPr></w:pPr></w:p>{}<w:sectPr><w:type w:val="{next}"/>{page}</w:sectPr></w:body></w:document>"#,
        line("First"),
        line("Second")
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    let doc = io::open_bytes("mark.docx", &zip.finish().unwrap().into_inner()).unwrap();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    result
        .pages
        .iter()
        .enumerate()
        .find_map(|(i, page)| {
            page.items.iter().find_map(|item| match item {
                Placed::Lines { path, para, y, .. } if path.0 == [2] => Some((i, y + para.lines[0].top)),
                _ => None,
            })
        })
        .unwrap()
}

#[test]
fn section_start_spacing_gives_an_empty_mark_before_a_continuous_section_no_line() {
    // WPS references: the empty paragraph that only ends a section takes no line when the next
    // section continues on the same page, so the next text follows the previous line directly.
    let (page, top) = second_top("continuous");
    assert_eq!(page, 0);
    assert!((top - (72.0 + 12.0)).abs() < 0.01, "{top}");
}

#[test]
fn section_start_spacing_never_moves_an_empty_mark_to_a_new_page() {
    // A page filled to its last line: the empty mark that ends the section stays on it, taking
    // no room, and the next section's text starts the next page at its top.
    let line = |t: &str| {
        format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#)
    };
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    // 648 pt of text area holds 54 lines of 12 pt.
    let lines: String = (0..54).map(|i| line(&format!("Line{i}"))).collect();
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{lines}<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/><w:sectPr>{page}</w:sectPr></w:pPr></w:p>{}<w:sectPr><w:type w:val="continuous"/>{page}</w:sectPr></w:body></w:document>"#,
        line("Next")
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    let doc = io::open_bytes("full.docx", &zip.finish().unwrap().into_inner()).unwrap();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let next = result
        .pages
        .iter()
        .enumerate()
        .find_map(|(i, page)| {
            page.items.iter().find_map(|item| match item {
                Placed::Lines { path, para, y, .. } if path.0 == [55] => Some((i, y + para.lines[0].top)),
                _ => None,
            })
        })
        .unwrap();
    assert_eq!(next.0, 1, "{next:?}");
    assert!((next.1 - 72.0).abs() < 0.01, "{next:?}");
}

#[test]
fn section_start_spacing_keeps_an_overflowing_empty_mark_before_a_new_page_section_on_its_page() {
    // WPS references: a page filled to its last line, then the empty paragraph that only ends a
    // section whose successor starts a new page. The mark stays at the foot of the full page, so
    // no blank page comes between: the next section's text is on the second page, at its top.
    let line = |t: &str| {
        format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#)
    };
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    // 648 pt of text area holds 54 lines of 12 pt.
    let lines: String = (0..54).map(|i| line(&format!("Line{i}"))).collect();
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{lines}<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/><w:sectPr>{page}</w:sectPr></w:pPr></w:p>{}<w:sectPr><w:type w:val="nextPage"/>{page}</w:sectPr></w:body></w:document>"#,
        line("Next")
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    let doc = io::open_bytes("full.docx", &zip.finish().unwrap().into_inner()).unwrap();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    assert_eq!(result.pages.len(), 2, "no blank page between the sections");
    let next = result
        .pages
        .iter()
        .enumerate()
        .find_map(|(i, page)| {
            page.items.iter().find_map(|item| match item {
                Placed::Lines { path, para, y, .. } if path.0 == [55] => Some((i, y + para.lines[0].top)),
                _ => None,
            })
        })
        .unwrap();
    assert_eq!(next.0, 1, "{next:?}");
    assert!((next.1 - 72.0).abs() < 0.01, "{next:?}");
}

#[test]
fn section_start_spacing_moves_a_kept_paragraph_of_a_mid_page_section_to_the_next_page() {
    // A continuous section starting one line above the bottom margin: its first paragraph, two
    // lines kept together, starts the next page rather than overflowing the margin.
    let line = |t: &str| {
        format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#)
    };
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    let lines: String = (0..53).map(|i| line(&format!("Line{i}"))).collect();
    let kept = r#"<w:p><w:pPr><w:keepLines/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:t>Kept</w:t><w:br/><w:t>Together</w:t></w:r></w:p>"#;
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{lines}<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/><w:sectPr>{page}</w:sectPr></w:pPr></w:p>{kept}<w:sectPr><w:type w:val="continuous"/>{page}</w:sectPr></w:body></w:document>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    let doc = io::open_bytes("kept.docx", &zip.finish().unwrap().into_inner()).unwrap();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let kept: Vec<(usize, f32)> = result
        .pages
        .iter()
        .enumerate()
        .flat_map(|(i, page)| {
            page.items.iter().filter_map(move |item| match item {
                Placed::Lines { path, para, y, l0, .. } if path.0 == [54] => Some((i, y + para.lines[*l0].top - para.lines[*l0].top)),
                _ => None,
            })
        })
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].0, 1, "{kept:?}");
    assert!((kept[0].1 - 72.0).abs() < 0.01, "{kept:?}");
}

/// 32 lines each followed by 8 pt (640 of the 648 pt area), then a 12 pt heading with 18 pt
/// before kept with the next line; `section` puts the heading first in a continuous section.
fn heading_after_full_page(section: bool) -> (usize, f32) {
    let line = |t: &str| {
        format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="160" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#)
    };
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    let mut body: String = (0..32).map(|i| line(&format!("Line{i}"))).collect();
    if section {
        body += &format!(
            r#"<w:p><w:pPr><w:spacing w:before="0" w:after="160" w:line="240" w:lineRule="exact"/><w:sectPr>{page}</w:sectPr></w:pPr></w:p>"#
        );
    }
    let heading = r#"<w:p><w:pPr><w:keepNext/><w:spacing w:before="360" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p>"#;
    let kind = if section { r#"<w:type w:val="continuous"/>"# } else { "" };
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}{heading}{}<w:sectPr>{kind}{page}</w:sectPr></w:body></w:document>"#,
        line("Body")
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    let doc = io::open_bytes("keep.docx", &zip.finish().unwrap().into_inner()).unwrap();
    let index = if section { 33 } else { 32 };
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    result
        .pages
        .iter()
        .enumerate()
        .find_map(|(i, page)| {
            page.items.iter().find_map(|item| match item {
                Placed::Lines { path, para, y, .. } if path.0 == [index] => Some((i, y + para.lines[0].top)),
                _ => None,
            })
        })
        .unwrap()
}

#[test]
fn section_start_spacing_keeps_the_collapsed_space_before_at_a_new_page_top() {
    // WPS references: a section's first paragraph that starts the next page keeps its space
    // before less the space after which ended the previous section (18 - 8 = 10 pt).
    let heading = heading_after_full_page(true);
    assert_eq!(heading.0, 1, "{heading:?}");
    assert!((heading.1 - (72.0 + 10.0)).abs() < 0.01, "{heading:?}");
}

#[test]
fn section_start_spacing_drops_space_before_where_text_flows_to_a_new_page() {
    // WPS references: a heading that the text's flow carries to the top of a new page within a
    // section starts there without its space before.
    let heading = heading_after_full_page(false);
    assert_eq!(heading.0, 1, "{heading:?}");
    assert!((heading.1 - 72.0).abs() < 0.01, "{heading:?}");
}

#[test]
fn section_start_spacing_keeps_a_fitting_section_heading_where_it_starts() {
    // WPS references: a continuous section's first paragraph that fits where the section starts
    // stays there although it is kept with the next paragraph, which continues on the next page.
    let line = |t: &str| {
        format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#)
    };
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    let lines: String = (0..52).map(|i| line(&format!("Line{i}"))).collect();
    let heading = r#"<w:p><w:pPr><w:keepNext/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p>"#;
    let body = r#"<w:p><w:pPr><w:keepLines/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/></w:pPr><w:r><w:t>Body</w:t><w:br/><w:t>Two</w:t></w:r></w:p>"#;
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{lines}<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/><w:sectPr>{page}</w:sectPr></w:pPr></w:p>{heading}{body}<w:sectPr><w:type w:val="continuous"/>{page}</w:sectPr></w:body></w:document>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    let doc = io::open_bytes("heading.docx", &zip.finish().unwrap().into_inner()).unwrap();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let page_of = |index: u32| {
        result.pages.iter().position(|page| page.items.iter().any(|item| matches!(item, Placed::Lines { path, .. } if path.0 == [index]))).unwrap()
    };
    assert_eq!(page_of(53), 0, "the heading stays on the first page");
    assert_eq!(page_of(54), 1);
}

#[test]
fn section_start_spacing_fits_a_last_line_whose_text_ends_above_the_margin() {
    // WPS references: the last line on a page fits when its text ends above the bottom margin
    // although the grid room below its text reaches past it. Here 12 pt text takes two 12 pt grid
    // rows; the 645 pt text area holds 26 full line boxes, and the 27th box ends 3 pt past the
    // margin with its text inside.
    let line = |t: &str| {
        format!(
            r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Source Sans 3" w:hAnsi="Source Sans 3"/><w:sz w:val="24"/></w:rPr><w:t>{t}</w:t></w:r></w:p>"#
        )
    };
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1500" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/><w:docGrid w:type="lines" w:linePitch="240"/>"#;
    let lines: String = (0..27).map(|i| line(&format!("Line{i}"))).collect();
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{lines}<w:sectPr>{page}</w:sectPr></w:body></w:document>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    let doc = io::open_bytes("grid.docx", &zip.finish().unwrap().into_inner()).unwrap();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let first = result.pages[0].items.iter().find_map(|item| match item {
        Placed::Lines { para, .. } => Some((para.lines[0].height, para.lines[0].baseline - para.lines[0].top, para.lines[0].descent)),
        _ => None,
    });
    let (height, baseline, descent) = first.unwrap();
    assert!((height - 24.0).abs() < 0.01, "{first:?}");
    assert!(height - baseline - descent > 3.0, "the text ends more than 3 pt above the line box bottom: {first:?}");
    let on_first = result.pages[0].items.iter().filter(|item| matches!(item, Placed::Lines { .. })).count();
    assert_eq!(on_first, 27, "{first:?}");
}

#[test]
fn section_start_spacing_merges_a_page_break_with_the_empty_mark_after_it() {
    // WPS for Mac (corpus references): a paragraph ending with a page break, then the empty
    // paragraph that only ends its section, before a section that starts a new page. The break and
    // the section break make one: the mark stays on the break's page, and the next section's text
    // is on the second page rather than a third.
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>First</w:t></w:r></w:p><w:p><w:r><w:br w:type="page"/></w:r></w:p><w:p><w:pPr><w:sectPr>{page}</w:sectPr></w:pPr></w:p><w:p><w:r><w:t>Next</w:t></w:r></w:p><w:sectPr><w:type w:val="nextPage"/>{page}</w:sectPr></w:body></w:document>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    let doc = io::open_bytes("merged.docx", &zip.finish().unwrap().into_inner()).unwrap();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    assert_eq!(result.pages.len(), 2, "no page holding only the section mark");
    let next =
        result.pages.iter().position(|page| page.items.iter().any(|item| matches!(item, Placed::Lines { path, .. } if path.0 == [3]))).unwrap();
    assert_eq!(next, 1);
}
