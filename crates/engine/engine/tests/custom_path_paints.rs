use std::io::Write;

use kition_doc_engine::{Session, io};

fn fixture(paths: &str) -> Session {
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><w:body><w:p><w:r><w:drawing><wp:anchor>
<wp:positionH relativeFrom="page"><wp:posOffset>254000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>254000</wp:posOffset></wp:positionV><wp:extent cx="508000" cy="508000"/><wp:wrapNone/>
<a:graphic><a:graphicData><wps:wsp><wps:spPr><a:custGeom><a:pathLst>{paths}</a:pathLst></a:custGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:ln w="25400"><a:solidFill><a:srgbClr val="0000FF"/></a:solidFill></a:ln></wps:spPr></wps:wsp></a:graphicData></a:graphic>
</wp:anchor></w:drawing></w:r></w:p><w:sectPr><w:pgSz w:w="2000" w:h="2000"/><w:pgMar w:top="200" w:bottom="200" w:left="200" w:right="200"/></w:sectPr></w:body></w:document>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    Session::new(io::open_bytes("paths.docx", &zip.finish().unwrap().into_inner()).unwrap())
}

fn check_repeated_saves(paths: &str, pixels: &[(u32, u32, [u8; 4])]) {
    let mut session = fixture(paths);
    for _ in 0..3 {
        let layout = session.export_layout();
        assert_eq!(layout.pages.len(), 1);
        let image = kition_doc_render::render_page(&session.doc, &layout.pages[0], 1.0, &Default::default());
        for &(x, y, expected) in pixels {
            assert_eq!(image.pixel(x, y), expected, "pixel ({x}, {y})");
        }
        let bytes = kition_doc_docx::write(&session.doc).unwrap();
        session = Session::new(io::open_bytes("saved.docx", &bytes).unwrap());
    }
}

const TRIANGLE: &str = r#"<a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="40" y="0"/></a:lnTo><a:lnTo><a:pt x="0" y="40"/></a:lnTo>"#;
const OUTER: &str = r#"<a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="40" y="0"/></a:lnTo><a:lnTo><a:pt x="40" y="40"/></a:lnTo><a:lnTo><a:pt x="0" y="40"/></a:lnTo><a:close/>"#;
const INNER: &str = r#"<a:moveTo><a:pt x="10" y="10"/></a:moveTo><a:lnTo><a:pt x="10" y="30"/></a:lnTo><a:lnTo><a:pt x="30" y="30"/></a:lnTo><a:lnTo><a:pt x="30" y="10"/></a:lnTo><a:close/>"#;

#[test]
fn custom_path_paints_fill_an_open_contour_without_closing_its_stroke() {
    check_repeated_saves(&format!(r#"<a:path w="40" h="40">{TRIANGLE}</a:path>"#), &[(30, 30, [255, 0, 0, 255]), (19, 40, [255, 255, 255, 255])]);
}

#[test]
fn custom_path_paints_keep_closed_unfilled_outlines_closed() {
    check_repeated_saves(
        &format!(r#"<a:path w="40" h="40" fill="none">{TRIANGLE}<a:close/></a:path>"#),
        &[(30, 30, [255, 255, 255, 255]), (20, 40, [0, 0, 255, 255])],
    );
}

#[test]
fn custom_path_paints_honor_disabled_strokes() {
    check_repeated_saves(
        &format!(r#"<a:path w="40" h="40" fill="none" stroke="0">{TRIANGLE}<a:close/></a:path>"#),
        &[(40, 20, [255, 255, 255, 255])],
    );
}

#[test]
fn custom_path_paints_preserve_opposite_winding_holes_in_one_path() {
    check_repeated_saves(&format!(r#"<a:path w="40" h="40">{OUTER}{INNER}</a:path>"#), &[(25, 25, [255, 0, 0, 255]), (40, 40, [255, 255, 255, 255])]);
}

#[test]
fn custom_path_paints_do_not_merge_independent_path_fills() {
    check_repeated_saves(&format!(r#"<a:path w="40" h="40">{OUTER}</a:path><a:path w="40" h="40">{INNER}</a:path>"#), &[(40, 40, [255, 0, 0, 255])]);
}

#[test]
fn custom_path_paints_keep_transparent_strokes_after_saving() {
    use kition_doc_engine::doc::{InlineObject, Path, StoryRef};
    use std::sync::Arc;

    let mut session = fixture(&format!(r#"<a:path w="40" h="40" fill="none">{TRIANGLE}<a:close/></a:path>"#));
    let paragraph = session.doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    let InlineObject::Shape { freeform: Some(geometry), .. } = &mut paragraph.objects[0] else { panic!("freeform missing") };
    Arc::make_mut(geometry).alpha = 0.5;
    for _ in 0..3 {
        let layout = session.export_layout();
        let image = kition_doc_render::render_page(&session.doc, &layout.pages[0], 1.0, &Default::default());
        assert_eq!(image.pixel(20, 40), [128, 128, 255, 255]);
        let bytes = kition_doc_docx::write(&session.doc).unwrap();
        session = Session::new(io::open_bytes("saved.docx", &bytes).unwrap());
    }
}
