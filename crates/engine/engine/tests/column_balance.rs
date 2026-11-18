use std::io::Write;

use kition_doc_engine::{
    doc::Document,
    io,
    layout::{self, Placed},
};

/// One exact 12pt line holding `text`.
fn para(text: &str, sect: &str) -> String {
    let props = if sect.is_empty() { String::new() } else { format!("<w:pPr>{sect}</w:pPr>") };
    format!(
        r#"<w:p>{props}<w:r><w:rPr><w:rFonts w:ascii="Source Sans 3" w:hAnsi="Source Sans 3"/><w:sz w:val="20"/></w:rPr><w:t>{text}</w:t></w:r></w:p>"#
    )
}

fn sect(columns: u32, start: &str) -> String {
    let kind = if start.is_empty() { String::new() } else { format!(r#"<w:type w:val="{start}"/>"#) };
    format!(
        r#"<w:sectPr>{kind}<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/><w:cols w:num="{columns}" w:space="720"/></w:sectPr>"#
    )
}

/// A two-column section of `lines` one-line paragraphs (the last ends the section), then a
/// one-column section starting with "After" whose break is `next`.
fn document(lines: usize, next: &str) -> Document {
    let mut body = String::new();
    for i in 1..lines {
        body += &para(&format!("Line{i}"), "");
    }
    body += &para(&format!("Line{lines}"), &sect(2, ""));
    body += &para("After", "");
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}{}</w:body></w:document>"#,
        sect(1, next)
    );
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="exact"/></w:pPr></w:pPrDefault></w:docDefaults></w:styles>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in [("word/document.xml", xml.as_str()), ("word/styles.xml", styles), ("word/_rels/document.xml.rels", rels)] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    io::open_bytes("columns.docx", &zip.finish().unwrap().into_inner()).unwrap()
}

/// (text, x, top) of every line on the first page.
fn lines(doc: &Document) -> Vec<(String, f32, f32)> {
    let result = layout::layout(doc, &mut layout::LayoutCache::new(), &Default::default());
    assert_eq!(result.pages.len(), 1);
    let mut out = Vec::new();
    for item in &result.pages[0].items {
        if let Placed::Lines { path, para, l0, l1, x, y, .. } = item {
            let p = doc.body.get(path.0[0] as usize).and_then(|b| b.as_para()).unwrap();
            for li in *l0..*l1 {
                let l = &para.lines[li];
                let text = p.text.get(l.start..l.stop).unwrap_or("").trim().to_string();
                if !text.is_empty() {
                    out.push((text, *x, y + l.top - para.lines[*l0].top));
                }
            }
        }
    }
    out
}

fn column_of(all: &[(String, f32, f32)], text: &str) -> (f32, f32) {
    all.iter().find(|l| l.0 == text).map(|l| (l.1, l.2)).unwrap_or_else(|| panic!("{text}: {all:?}"))
}

#[test]
fn column_balance_splits_a_section_before_a_continuous_break_evenly() {
    for (count, first) in [(6, 3), (7, 4)] {
        let all = lines(&document(count, "continuous"));
        let left = column_of(&all, "Line1").0;
        for i in 1..=count {
            let (x, _) = column_of(&all, &format!("Line{i}"));
            assert_eq!(x == left, i <= first, "{count} lines: Line{i} in column {}: {all:?}", if x == left { 1 } else { 2 });
        }
        let (_, top_second) = column_of(&all, &format!("Line{}", first + 1));
        assert!((top_second - column_of(&all, "Line1").1).abs() < 0.01, "the second column starts level with the first");
        let (_, last_top) = column_of(&all, &format!("Line{first}"));
        let (after_x, after_top) = column_of(&all, "After");
        assert_eq!(after_x, left);
        assert!(
            after_top >= last_top + 11.9 && after_top <= last_top + 12.1,
            "the next section starts below the taller column: {after_top} {last_top}"
        );
    }
}

#[test]
fn column_balance_leaves_columns_before_a_page_break_unbalanced() {
    let doc = document(6, "nextPage");
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    assert_eq!(result.pages.len(), 2);
    let all: Vec<f32> = result.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Placed::Lines { x, .. } => Some(*x),
            _ => None,
        })
        .collect();
    assert!(all.iter().all(|x| (x - all[0]).abs() < 0.01), "one column holds every line: {all:?}");
}

#[test]
fn column_balance_keeps_a_kept_together_paragraph_in_one_column() {
    // A three-line paragraph kept together and one more line, in three columns: the paragraph
    // stays whole in the first column and the next line starts the second.
    let long = "Word ".repeat(30);
    let body = format!(
        r#"<w:p><w:pPr><w:keepLines/></w:pPr><w:r><w:rPr><w:sz w:val="20"/></w:rPr><w:t>{long}</w:t></w:r></w:p>{}{}"#,
        para("Next", &sect(3, "")),
        para("After", "")
    );
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}{}</w:body></w:document>"#,
        sect(1, "continuous")
    );
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="exact"/></w:pPr></w:pPrDefault></w:docDefaults></w:styles>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in [("word/document.xml", xml.as_str()), ("word/styles.xml", styles), ("word/_rels/document.xml.rels", rels)] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    let doc = io::open_bytes("keep.docx", &zip.finish().unwrap().into_inner()).unwrap();
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let pieces: Vec<(Vec<u32>, f32, usize)> = result.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Placed::Lines { path, x, l0, l1, .. } => Some((path.0.clone(), *x, l1 - l0)),
            _ => None,
        })
        .collect();
    let first: Vec<_> = pieces.iter().filter(|p| p.0 == [0]).collect();
    assert_eq!(first.len(), 1, "the kept paragraph is not split: {pieces:?}");
    assert!(first[0].2 >= 2, "{pieces:?}");
    let next = pieces.iter().find(|p| p.0 == [1]).unwrap();
    assert!(next.1 > first[0].1 + 1.0, "the next paragraph starts the second column: {pieces:?}");
}

/// A document of `body` (paragraph XML) whose section ends with `cols`, then "After" in a
/// one-column continuous section.
fn sections(body: &str, cols: &str) -> Document {
    let first = format!(
        r#"<w:sectPr><w:type w:val="continuous"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{cols}</w:sectPr>"#
    );
    let body = body.replace("{sect}", &first) + &para("After", "");
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}{}</w:body></w:document>"#,
        sect(1, "continuous")
    );
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="exact"/></w:pPr></w:pPrDefault></w:docDefaults></w:styles>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in [("word/document.xml", xml.as_str()), ("word/styles.xml", styles), ("word/_rels/document.xml.rels", rels)] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    io::open_bytes("columns.docx", &zip.finish().unwrap().into_inner()).unwrap()
}

#[test]
fn a_paragraph_continuing_in_a_wider_column_runs_its_width() {
    // WPS for Mac (luan's "two columns, narrow left"): the text a paragraph carries on into the
    // wider second column is set as wide as that column.
    let text = "Words that run on from the narrow column into the wide one. ".repeat(6);
    let body = para(&text, "") + "<w:p><w:pPr>{sect}</w:pPr></w:p>";
    let doc = sections(&body, r#"<w:cols w:equalWidth="0" w:num="2"><w:col w:w="2000" w:space="720"/><w:col w:w="6640"/></w:cols>"#);
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let mut widths: Vec<(f32, f32)> = Vec::new();
    for item in &result.pages[0].items {
        if let Placed::Lines { path, para, l0, l1, x, .. } = item
            && path.0 == [0]
        {
            for line in &para.lines[*l0..*l1] {
                widths.push((*x, line.right));
            }
        }
    }
    let left = widths.iter().map(|w| w.0).fold(f32::MAX, f32::min);
    let narrow: Vec<f32> = widths.iter().filter(|w| w.0 == left).map(|w| w.1).collect();
    let wide: Vec<f32> = widths.iter().filter(|w| w.0 > left).map(|w| w.1).collect();
    assert!(!narrow.is_empty() && !wide.is_empty(), "{widths:?}");
    assert!(narrow.iter().all(|r| *r <= 100.0 + 0.01), "{narrow:?}");
    assert!(wide.iter().any(|r| *r > 200.0), "the wide column's lines run its width: {wide:?}");
}

#[test]
fn the_mark_ending_balanced_columns_takes_no_room_below_them() {
    // The empty paragraph that only ends a two-column section, before a continuous section:
    // the next section starts right below the taller column, as without it.
    let body: String = (1..=6).map(|i| para(&format!("Line{i}"), "")).collect::<String>() + "<w:p><w:pPr>{sect}</w:pPr></w:p>";
    let all = lines(&sections(&body, r#"<w:cols w:num="2" w:space="720"/>"#));
    let (_, last) = column_of(&all, "Line3");
    let (_, after) = column_of(&all, "After");
    assert!(after >= last + 11.9 && after <= last + 12.1, "After at {after}, the taller column's last line at {last}: {all:?}");
}

#[test]
fn the_last_page_of_a_section_running_on_is_balanced() {
    // WPS for Mac (luan's four-column section running on to the next page): the section's last
    // page ends with level columns before the next continuous section.
    let lines_on_first = 2 * 54; // two columns of 12 pt lines on a Letter page with 1" margins
    let body: String = (1..=lines_on_first + 6).map(|i| para(&format!("Line{i}"), "")).collect::<String>() + "<w:p><w:pPr>{sect}</w:pPr></w:p>";
    let doc = sections(&body, r#"<w:cols w:num="2" w:space="720"/>"#);
    let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    assert_eq!(result.pages.len(), 2);
    let mut columns: Vec<(f32, usize)> = Vec::new();
    for item in &result.pages[1].items {
        if let Placed::Lines { path, l0, l1, x, .. } = item
            && doc.body.get(path.0[0] as usize).and_then(|b| b.as_para()).is_some_and(|p| p.text.starts_with("Line"))
        {
            match columns.iter_mut().find(|c| (c.0 - *x).abs() < 0.5) {
                Some(c) => c.1 += l1 - l0,
                None => columns.push((*x, l1 - l0)),
            }
        }
    }
    let counts: Vec<usize> = columns.iter().map(|c| c.1).collect();
    assert_eq!(counts.iter().sum::<usize>(), 6, "{counts:?}");
    assert_eq!(counts, [3, 3], "the six lines on the last page stand three to a column: {columns:?}");
}
