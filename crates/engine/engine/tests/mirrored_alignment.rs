use std::io::Write;

use kition_doc_engine::{
    doc::Document,
    io,
    layout::{self, Placed},
};

const NS: &str = r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;

/// A two-page document whose header holds a 1-inch square aligned `align` in the margin box.
fn document(align: &str) -> Document {
    with_position(&format!(
        r#"<wp:positionH relativeFrom="margin"><wp:align>{align}</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>"#
    ))
}

/// The same document with the square placed by `position` (its positionH and positionV).
fn with_position(position: &str) -> Document {
    let shape = format!(
        r#"<w:p><w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/>{position}<wp:extent cx="914400" cy="914400"/><wp:wrapNone/><wp:docPr id="1" name="Square"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#
    );
    let header = format!(r#"<w:hdr {NS}>{shape}</w:hdr>"#);
    let xml = format!(
        r#"<w:document {NS}><w:body><w:p><w:r><w:t>One</w:t></w:r></w:p><w:p><w:r><w:br w:type="page"/><w:t>Two</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in [("word/document.xml", xml.as_str()), ("word/header1.xml", header.as_str()), ("word/_rels/document.xml.rels", rels)] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    io::open_bytes("mirrored.docx", &zip.finish().unwrap().into_inner()).unwrap()
}

/// The square's left edge on each page.
fn squares(doc: &Document) -> Vec<f32> {
    let result = layout::layout(doc, &mut layout::LayoutCache::new(), &Default::default());
    result
        .pages
        .iter()
        .map(|page| {
            page.header
                .iter()
                .find_map(|item| match item {
                    Placed::Shape { rect, .. } => Some(rect.x),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no square on a page: {} header items", page.header.len()))
        })
        .collect()
}

#[test]
fn mirrored_alignment_puts_outside_right_on_odd_pages_and_left_on_even_pages() {
    // ECMA-376 inside and outside alignment: on odd pages inside is left and outside right; on
    // even pages they swap (WPS puts an outside header page number at the left of page 10).
    let (left, right) = (72.0, 612.0 - 72.0 - 72.0);
    let outside = squares(&document("outside"));
    assert_eq!(outside.len(), 2);
    assert!((outside[0] - right).abs() < 0.01 && (outside[1] - left).abs() < 0.01, "{outside:?}");
    let inside = squares(&document("inside"));
    assert!((inside[0] - left).abs() < 0.01 && (inside[1] - right).abs() < 0.01, "{inside:?}");
}

/// The square's top edge on the first page.
fn square_top(doc: &Document) -> f32 {
    let result = layout::layout(doc, &mut layout::LayoutCache::new(), &Default::default());
    result.pages[0]
        .header
        .iter()
        .find_map(|item| match item {
            Placed::Shape { rect, .. } => Some(rect.y),
            _ => None,
        })
        .unwrap()
}

#[test]
fn mirrored_alignment_reads_positions_given_as_alternate_content() {
    // Word 2010 writes percentage positions with an absolute fallback; the fallback (2688590
    // EMU = 211.7 pt from the page top) is where WPS draws the shape.
    let position = r#"<wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH><mc:AlternateContent><mc:Choice Requires="wp14"><wp:positionV relativeFrom="paragraph"><wp14:pctPosVOffset>20000</wp14:pctPosVOffset></wp:positionV></mc:Choice><mc:Fallback><wp:positionV relativeFrom="page"><wp:posOffset>2688590</wp:posOffset></wp:positionV></mc:Fallback></mc:AlternateContent>"#;
    let top = square_top(&with_position(position));
    assert!((top - 2_688_590.0 / 12_700.0).abs() < 0.01, "{top}");
}
