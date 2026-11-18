//! Hand-written minimal WordprocessingML fixtures (written for these tests from the spec).

use std::io::Write;

use kition_doc_doc::para::{Anchor, FloatAlign, NoteKind, ShapeKind, Wrap};
use kition_doc_doc::props::{Align, Border, BorderStyle, ControlMarks, Rgb, TextColor, VMerge};
use kition_doc_doc::{Block, Document, InlineObject, Paragraph};

const W_NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office""#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

pub fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zw.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zw.write_all(bytes).unwrap();
    }
    zw.finish().unwrap().into_inner()
}

fn rels(list: &[(&str, &str, &str)]) -> String {
    let mut s = String::from(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target) in list {
        let ext = if target.starts_with("http") { r#" TargetMode="External""# } else { "" };
        s.push_str(&format!(
            r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{ty}" Target="{target}"{ext}/>"#
        ));
    }
    s.push_str("</Relationships>");
    s
}

/// A package with `body` as the document body and extra (path, content) parts.
fn docx(body: &str, doc_rels: &[(&str, &str, &str)], extra: &[(&str, &str)]) -> Vec<u8> {
    let doc = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W_NS}><w:body>{body}</w:body></w:document>"#);
    let r = rels(doc_rels);
    let mut entries: Vec<(&str, &[u8])> =
        vec![("_rels/.rels", ROOT_RELS.as_bytes()), ("word/document.xml", doc.as_bytes()), ("word/_rels/document.xml.rels", r.as_bytes())];
    for (p, c) in extra {
        entries.push((p, c.as_bytes()));
    }
    zip(&entries)
}

fn read_body(body: &str) -> Document {
    kition_doc_docx::read(&docx(body, &[], &[])).unwrap()
}

fn paras(d: &Document) -> Vec<&Paragraph> {
    d.body.iter().filter_map(|b| b.as_para()).collect()
}

#[test]
fn toc_complex_field_spanning_paragraphs() {
    let body = r#"
<w:p><w:pPr><w:pStyle w:val="TOC1"/></w:pPr>
 <w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \o "1-3" \h \z \u </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>
 <w:hyperlink w:anchor="_Toc1"><w:r><w:t>Intro</w:t></w:r><w:r><w:tab/></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGEREF _Toc1 \h </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:hyperlink>
</w:p>
<w:p><w:hyperlink w:anchor="_Toc2"><w:r><w:t>Body</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t>2</w:t></w:r></w:hyperlink></w:p>
<w:p><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
<w:p><w:bookmarkStart w:id="0" w:name="_Toc1"/><w:r><w:t>Intro</w:t></w:r><w:bookmarkEnd w:id="0"/></w:p>
<w:p><w:r><w:t xml:space="preserve">Page </w:t></w:r><w:fldSimple w:instr=" PAGE "><w:r><w:t>7</w:t></w:r></w:fldSimple></w:p>"#;
    let d = read_body(body);
    let p = paras(&d);
    assert_eq!(p.len(), 5);
    assert_eq!(p[0].text, "\u{FFFC}Intro\t\u{FFFC}");
    assert_eq!(
        p[0].objects[0],
        InlineObject::Field {
            instr: r#"TOC \o "1-3" \h \z \u"#.into(),
            result: String::new(),
            locked: false,
            code_runs: Vec::new(),
            code_fields: Vec::new()
        }
    );
    assert_eq!(
        p[0].objects[1],
        InlineObject::Field {
            instr: r#"PAGEREF _Toc1 \h"#.into(),
            result: "1".into(),
            locked: false,
            code_runs: Vec::new(),
            code_fields: Vec::new()
        }
    );
    assert_eq!(p[0].props_of_char(3).link.as_deref(), Some("#_Toc1"));
    assert_eq!(p[0].props.style.as_deref(), Some("TOC1"));
    assert_eq!(p[1].text, "Body\t2");
    assert_eq!(p[1].props_of_char(0).link.as_deref(), Some("#_Toc2"));
    assert_eq!(p[2].text, "");
    assert_eq!(p[3].objects, vec![InlineObject::BookmarkStart { name: "_Toc1".into() }, InlineObject::BookmarkEnd { name: "_Toc1".into() }]);
    assert_eq!(p[4].plain_text(), "Page 7");
    assert_eq!(
        p[4].objects[0],
        InlineObject::Field { instr: "PAGE".into(), result: "7".into(), locked: false, code_runs: Vec::new(), code_fields: Vec::new() }
    );
    // And it writes back out and reads the same.
    let again = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(again.body, d.body);
}

#[test]
fn hyperlink_runs_and_wrappers() {
    let body = r#"
<w:p>
 <w:hyperlink r:id="rId5" w:history="1"><w:r><w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr><w:t>site</w:t></w:r></w:hyperlink>
 <w:hyperlink r:id="rId5" w:anchor="frag"><w:r><w:t>X</w:t></w:r></w:hyperlink>
 <w:smartTag><w:r><w:t>A</w:t></w:r></w:smartTag><w:customXml><w:r><w:t>B</w:t></w:r></w:customXml>
 <w:sdt><w:sdtPr/><w:sdtContent><w:r><w:t>C</w:t></w:r></w:sdtContent></w:sdt>
 <w:r><w:sym w:font="Wingdings" w:char="F04A"/><w:noBreakHyphen/><w:softHyphen/><w:cr/><w:br w:type="page"/><w:br w:type="column"/><w:t xml:space="preserve"> e&amp;</w:t></w:r>
 <w:proofErr w:type="spellStart"/><w:r><w:rPr><w:b w:val="false"/><w:i w:val="0"/><w:caps w:val="off"/><w:strike w:val="true"/><w:color w:val="auto"/></w:rPr><w:t>T</w:t></w:r>
</w:p>
<w:sdt><w:sdtContent><w:p><w:r><w:t>in block sdt</w:t></w:r></w:p></w:sdtContent></w:sdt>"#;
    let bytes = docx(body, &[("rId5", "hyperlink", "https://example.org/")], &[]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    let p = paras(&d);
    assert_eq!(p[0].text, "siteXABC\u{F04A}\u{2011}\u{AD}\n\u{C}\u{E} e&T");
    assert_eq!(p[0].props_of_char(0).link.as_deref(), Some("https://example.org/"));
    assert_eq!(p[0].props_of_char(0).style.as_deref(), Some("Hyperlink"));
    assert_eq!(p[0].props_of_char(4).link.as_deref(), Some("https://example.org/#frag"));
    let sym_off = p[0].text.find('\u{F04A}').unwrap();
    assert_eq!(p[0].props_of_char(sym_off).font.as_deref(), Some("Wingdings"));
    let t = p[0].props_of_char(p[0].text.len() - 1);
    assert_eq!((t.bold, t.italic, t.caps, t.strike, t.color), (Some(false), Some(false), Some(false), Some(true), Some(TextColor::Auto)));
    assert_eq!(p[1].text, "in block sdt");
}

#[test]
fn a_drop_caps_distance_from_the_text_round_trips() {
    let body = r#"
<w:p><w:pPr><w:framePr w:dropCap="drop" w:lines="5" w:hSpace="5669" w:wrap="around" w:vAnchor="text" w:hAnchor="text"/></w:pPr><w:r><w:rPr><w:sz w:val="170"/></w:rPr><w:t>下</w:t></w:r></w:p>
<w:p><w:r><w:t>沉示例：本段第一个字符将下沉到多行</w:t></w:r></w:p>"#;
    let d = read_body(body);
    let p = paras(&d);
    assert_eq!((p[0].props.drop_cap, p[0].props.drop_cap_space), (Some(5), Some(283.45)));
    assert_eq!(p[0].text, "下沉示例：本段第一个字符将下沉到多行");
    let again = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    let q = paras(&again);
    assert_eq!((q[0].props.drop_cap, q[0].props.drop_cap_space), (Some(5), Some(283.45)));
}

#[test]
fn table_cell_spacing_round_trips() {
    let body = r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblCellSpacing w:w="312" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>"#;
    let d = read_body(body);
    let Block::Table(t) = &*d.body[0] else { panic!() };
    assert_eq!(t.props.cell_spacing, Some(15.6));
    let again = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    let Block::Table(t) = &*again.body[0] else { panic!() };
    assert_eq!(t.props.cell_spacing, Some(15.6));
}

#[test]
fn a_section_without_page_margins_takes_wps_margins() {
    // WPS for Mac lays out a section whose w:sectPr has no w:pgMar with 1.25-inch left and right
    // margins and 1-inch top and bottom ones (corpus reference).
    let body = r#"
<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>first section</w:t></w:r></w:p>
<w:p><w:r><w:t>last section</w:t></w:r></w:p>
<w:sectPr><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#;
    let d = read_body(body);
    let first = paras(&d)[0].section.as_deref().expect("a section break");
    assert_eq!((first.margin_left, first.margin_right, first.margin_top, first.margin_bottom), (90.0, 90.0, 72.0, 72.0));
    assert_eq!((d.last_section.margin_left, d.last_section.margin_right), (72.0, 72.0));
}

#[test]
fn content_controls_mark_their_first_and_last_paragraphs_and_their_runs() {
    let body = r#"
<w:sdt><w:sdtPr><w:rPr><w:sz w:val="72"/></w:rPr></w:sdtPr><w:sdtEndPr><w:rPr><w:sz w:val="80"/></w:rPr></w:sdtEndPr><w:sdtContent>
 <w:p><w:r><w:t>first</w:t></w:r></w:p>
 <w:p><w:r><w:t>middle</w:t></w:r></w:p>
 <w:p><w:r><w:t>last</w:t></w:r></w:p>
</w:sdtContent></w:sdt>
<w:p><w:r><w:t>A</w:t></w:r><w:sdt><w:sdtPr/><w:sdtContent><w:r><w:t>B</w:t></w:r></w:sdtContent></w:sdt></w:p>"#;
    let d = read_body(body);
    let p = paras(&d);
    assert_eq!(p[0].props.control, Some(ControlMarks { start: Some(36.0), end: None }));
    assert_eq!(p[1].props.control, None);
    assert_eq!(p[2].props.control, Some(ControlMarks { start: None, end: Some(40.0) }));
    // Around runs: the text stays as it is; the runs carry the marks (sizes unset: the style's).
    assert_eq!(p[3].text, "AB");
    assert_eq!(p[3].props_of_char(0).control, None);
    assert_eq!(p[3].props_of_char(1).control, Some(ControlMarks { start: Some(0.0), end: Some(0.0) }));
}

#[test]
fn alternate_content_choice_and_fallback() {
    let body = r#"
<w:p><w:r><mc:AlternateContent>
  <mc:Choice Requires="wps"><w:drawing><wp:anchor behindDoc="0" distL="12700"><wp:positionH relativeFrom="page"><wp:posOffset>127000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="457200"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="1" name="Text Box 1"/>
    <a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:prstGeom prst="rect"/></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>boxed</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic>
  </wp:anchor></w:drawing></mc:Choice>
  <mc:Fallback><w:pict><v:shape style="width:72pt;height:36pt"><v:textbox><w:txbxContent><w:p><w:r><w:t>old</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></mc:Fallback>
</mc:AlternateContent></w:r></w:p>
<w:p><mc:AlternateContent><mc:Choice Requires="w99"><w:r><w:t>future</w:t></w:r></mc:Choice><mc:Fallback><w:r><w:t>fallback text</w:t></w:r></mc:Fallback></mc:AlternateContent></w:p>"#;
    let d = read_body(body);
    let p = paras(&d);
    match &p[0].objects[0] {
        InlineObject::Shape { kind, w, h, float, story, .. } => {
            assert_eq!((*kind, *w, *h), (ShapeKind::TextBox, 72.0, 36.0));
            assert_eq!(float.wrap, Wrap::Square);
            assert_eq!(float.x, 10.0);
            assert_eq!(float.dist, 1.0);
            let s = d.parts.get(&story.unwrap()).unwrap();
            assert_eq!(s.blocks[0].as_para().unwrap().text, "boxed");
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(p[1].text, "fallback text");
}

#[test]
fn table_look_bitmask_and_legacy_merges() {
    let body = r#"
<w:tbl>
 <w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="5000" w:type="pct"/><w:jc w:val="center"/><w:tblLook w:val="04A0"/></w:tblPr>
 <w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid>
 <w:tr><w:tc><w:tcPr><w:hMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>wide</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:hMerge/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:vMerge w:val="restart"/><w:shd w:val="clear" w:fill="FF0000"/></w:tcPr><w:p/></w:tc></w:tr>
 <w:tr><w:trPr><w:trHeight w:val="400"/></w:trPr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p/></w:tc><w:sdt><w:sdtContent><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc></w:sdtContent></w:sdt></w:tr>
</w:tbl>
<w:tbl><w:tblPr><w:tblLook w:firstRow="0" w:lastRow="1" w:firstColumn="1" w:lastColumn="0" w:noHBand="1" w:noVBand="0"/></w:tblPr><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>
<w:p/>"#;
    let d = read_body(body);
    let Block::Table(t) = &*d.body[0] else { panic!() };
    assert_eq!(t.props.width_pct, Some(100.0));
    assert_eq!(t.props.align, Some(Align::Center));
    let l = t.props.look;
    assert!(l.header_row && l.first_column && l.banded_rows && !l.banded_columns && !l.total_row && !l.last_column);
    assert_eq!(t.rows[0].cells.len(), 2);
    assert_eq!(t.rows[0].cells[0].span(), 2);
    assert_eq!(t.rows[0].cells[1].props.vmerge, VMerge::Restart);
    assert_eq!(t.rows[1].cells[1].props.vmerge, VMerge::Continue);
    assert_eq!(t.rows[1].props.height, Some(20.0));
    assert_eq!(t.grid, vec![100.0, 100.0, 100.0]);
    let Block::Table(t2) = &*d.body[1] else { panic!() };
    let l = t2.props.look;
    assert!(!l.header_row && l.total_row && l.first_column && !l.last_column && !l.banded_rows && l.banded_columns);
}

#[test]
fn styles_numbering_settings_notes_comments_from_parts() {
    let styles = format!(
        r#"<w:styles {W_NS}>
 <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi"/><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>
 <w:style w:type="paragraph" w:default="1" w:styleId="a"><w:name w:val="Normal"/><w:qFormat/></w:style>
 <w:style w:type="paragraph" w:styleId="1"><w:name w:val="heading 1"/><w:basedOn w:val="a"/><w:next w:val="a"/><w:uiPriority w:val="9"/><w:qFormat/><w:pPr><w:keepNext/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:rFonts w:asciiTheme="majorHAnsi"/><w:b/><w:sz w:val="32"/></w:rPr></w:style>
 <w:style w:type="paragraph" w:customStyle="1" w:styleId="Fancy"><w:name w:val="Fancy"/><w:basedOn w:val="Fancy"/></w:style>
 <w:style w:type="table" w:styleId="Grid"><w:name w:val="Grid"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4"/></w:tblBorders></w:tblPr><w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr><w:tcPr><w:shd w:val="clear" w:fill="4472C4"/></w:tcPr></w:tblStylePr></w:style>
</w:styles>"#
    );
    let theme = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="T"><a:themeElements><a:clrScheme name="T"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:accent1><a:srgbClr val="4472C4"/></a:accent1></a:clrScheme><a:fontScheme name="T"><a:majorFont><a:latin typeface="Major Face"/></a:majorFont><a:minorFont><a:latin typeface="Minor Face"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#;
    let numbering = format!(
        r#"<w:numbering {W_NS}><w:abstractNum w:abstractNumId="7"><w:name w:val="L"/><w:lvl w:ilvl="0"><w:start w:val="3"/><w:numFmt w:val="upperLetter"/><w:lvlText w:val="%1)"/><w:lvlJc w:val="right"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="bullet"/><w:suff w:val="space"/><w:lvlText w:val="o"/><w:rPr><w:rFonts w:ascii="Courier New"/></w:rPr></w:lvl></w:abstractNum><w:num w:numId="2"><w:abstractNumId w:val="7"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="5"/></w:lvlOverride></w:num></w:numbering>"#
    );
    let settings = format!(
        r#"<w:settings {W_NS}><w:trackRevisions/><w:defaultTabStop w:val="708"/><w:evenAndOddHeaders w:val="1"/><w:footnotePr><w:numFmt w:val="lowerRoman"/></w:footnotePr></w:settings>"#
    );
    let footnotes = format!(
        r#"<w:footnotes {W_NS}><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> Note text</w:t></w:r></w:p></w:footnote></w:footnotes>"#
    );
    let comments = format!(
        r#"<w:comments {W_NS}><w:comment w:id="5" w:author="Zed" w:initials="Z" w:date="2026-01-01T00:00:00Z"><w:p><w:r><w:annotationRef/></w:r><w:r><w:t>Look</w:t></w:r></w:p></w:comment></w:comments>"#
    );
    let header = format!(r#"<w:hdr {W_NS}><w:p><w:r><w:t>H</w:t></w:r></w:p></w:hdr>"#);
    let core = r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Doc Title</dc:title><cp:revision>3</cp:revision></cp:coreProperties>"#;
    let body = r#"
<w:p><w:pPr><w:pStyle w:val="1"/><w:numPr><w:ilvl w:val="1"/><w:numId w:val="2"/></w:numPr></w:pPr><w:commentRangeStart w:id="5"/><w:r><w:rPr><w:rFonts w:asciiTheme="majorHAnsi"/></w:rPr><w:t>Heading</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r><w:r><w:commentReference w:id="5"/></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="a"/></w:pPr><w:ins w:id="1" w:author="I" w:date="2026-01-01T00:00:00Z"><w:r><w:t>new</w:t></w:r></w:ins><w:del w:id="2" w:author="I"><w:r><w:delText>old</w:delText></w:r></w:del></w:p>
<w:sectPr><w:headerReference w:type="default" r:id="rId9"/><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/><w:cols w:space="708"/><w:titlePg w:val="0"/></w:sectPr>"#;
    let doc_rels = [
        ("rId1", "styles", "styles.xml"),
        ("rId2", "theme", "theme/theme1.xml"),
        ("rId3", "numbering", "numbering.xml"),
        ("rId4", "settings", "/word/settings.xml"),
        ("rId5", "footnotes", "footnotes.xml"),
        ("rId6", "comments", "comments.xml"),
        ("rId9", "header", "header1.xml"),
    ];
    let root_rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#;
    let doc = format!(r#"<w:document {W_NS}><w:body>{body}</w:body></w:document>"#);
    let r = rels(&doc_rels);
    let bytes = zip(&[
        ("_rels/.rels", root_rels.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", r.as_bytes()),
        ("word/styles.xml", styles.as_bytes()),
        ("word/theme/theme1.xml", theme.as_bytes()),
        ("word/numbering.xml", numbering.as_bytes()),
        ("word/settings.xml", settings.as_bytes()),
        ("word/footnotes.xml", footnotes.as_bytes()),
        ("word/comments.xml", comments.as_bytes()),
        ("word/header1.xml", header.as_bytes()),
        ("docProps/core.xml", core.as_bytes()),
    ]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    // Styles: localized default id renamed to Normal; display names mapped.
    let h = d.styles.get("1").unwrap();
    assert_eq!(h.name, "Heading 1");
    assert_eq!(h.based_on.as_deref(), Some("Normal"));
    assert!(h.builtin && h.quick);
    assert_eq!(h.chr.font.as_deref(), Some("Major Face"));
    assert!(d.styles.get("Normal").is_some());
    assert_eq!(d.styles.get("Fancy").unwrap().based_on, None, "self-reference dropped");
    assert!(!d.styles.get("Fancy").unwrap().builtin);
    assert_eq!(d.styles.default_chr.font.as_deref(), Some("Minor Face"));
    assert_eq!(d.styles.default_chr.size, Some(11.0));
    let g = d.styles.get("Grid").unwrap().table.as_ref().unwrap();
    assert_eq!(g.header_fill, Some(kition_doc_doc::Rgb(0x44, 0x72, 0xC4)));
    assert_eq!(g.header_chr.bold, Some(true));
    assert_eq!(d.settings.theme_colors[4], kition_doc_doc::Rgb(0x44, 0x72, 0xC4));
    assert_eq!(d.settings.major_font, "Major Face");
    // Numbering.
    let lv = d.numbering.level(2, 0).unwrap();
    assert_eq!((lv.start, lv.text.as_str(), lv.indent, lv.hanging, lv.align), (3, "%1)", 36.0, 18.0, Align::Right));
    let lv1 = d.numbering.level(2, 1).unwrap();
    assert_eq!(lv1.chr.font.as_deref(), Some("Courier New"));
    assert_eq!(lv1.suffix, kition_doc_doc::numbering::LevelSuffix::Space);
    assert_eq!(d.numbering.num(2).unwrap().start_overrides, vec![(0, 5)]);
    // Settings.
    assert!(d.settings.track_changes && d.settings.even_odd_headers);
    assert_eq!(d.settings.default_tab, 35.4);
    assert_eq!(d.settings.footnote_format, kition_doc_doc::section::NumFormat::LowerRoman);
    // Body.
    let p = paras(&d);
    assert_eq!(p[0].props.style.as_deref(), Some("1"));
    assert_eq!(p[0].props.numbering, Some(kition_doc_doc::props::NumRef { num: 2, level: 1 }));
    assert_eq!(p[0].props_of_char(4).font.as_deref(), Some("Major Face"));
    match &p[0].objects[..] {
        [InlineObject::CommentStart { id: 5 }, InlineObject::NoteRef { kind: NoteKind::Footnote, id, .. }, InlineObject::CommentEnd { id: 5 }] => {
            let note = d.parts.get(id).unwrap().blocks[0].as_para().unwrap();
            assert_eq!(note.text, format!("{} Note text", kition_doc_doc::para::OBJ));
            assert_eq!(note.objects, vec![InlineObject::NoteRef { kind: NoteKind::Footnote, id: *id, custom: String::new() }]);
        }
        o => panic!("{o:?}"),
    }
    let c = d.comments.get(&5).unwrap();
    assert_eq!((c.author.as_str(), c.initials.as_str()), ("Zed", "Z"));
    assert_eq!(d.parts.get(&c.part).unwrap().blocks[0].as_para().unwrap().text, "Look");
    assert_eq!(p[1].props.style.as_deref(), Some("Normal"));
    assert_eq!(p[1].text, "newold");
    assert_eq!(d.revisions.len(), 2);
    assert_eq!(p[1].props_of_char(0).ins, Some(0));
    assert_eq!(p[1].props_of_char(3).del, Some(1));
    // Section + header.
    let s = &d.last_section;
    assert!(s.landscape);
    assert_eq!((s.page_w, s.page_h), (841.9, 595.3));
    assert!(!s.title_page);
    let hid = s.headers.default.unwrap();
    assert_eq!(d.parts.get(&hid).unwrap().blocks[0].as_para().unwrap().text, "H");
    assert_eq!(d.core.title, "Doc Title");
    assert_eq!(d.core.revision, 3);
}

/// A table style's own pPr/rPr, its `wholeTable` region (which covers every cell) and its table
/// shading and cell margins; a derived style keeps only what it sets itself.
#[test]
fn table_style_whole_table_shading_and_margins() {
    let styles = format!(
        r#"<w:styles {W_NS}>
 <w:style w:type="table" w:styleId="Base"><w:name w:val="Base"/><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:tblPr><w:shd w:val="clear" w:color="auto" w:fill="EEEEEE"/><w:tblCellMar><w:top w:w="20" w:type="dxa"/><w:left w:w="288" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="288" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>
 <w:style w:type="table" w:styleId="Whole"><w:name w:val="Whole"/><w:basedOn w:val="Base"/><w:rPr><w:b/></w:rPr><w:tblStylePr w:type="wholeTable"><w:pPr><w:jc w:val="center"/></w:pPr><w:rPr><w:color w:val="1F4E79"/></w:rPr><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="DDEBF7"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type="bogus"/><w:tblStylePr/></w:style>
</w:styles>"#
    );
    let bytes = docx("<w:p/>", &[("rId1", "styles", "styles.xml")], &[("word/styles.xml", &styles)]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    let base = d.styles.get("Base").unwrap();
    let parts = base.table.as_ref().unwrap();
    assert_eq!(parts.fill, Some(kition_doc_doc::Rgb(0xEE, 0xEE, 0xEE)));
    assert_eq!(parts.cell_margins, Some([1.0, 14.4, 0.0, 14.4]));
    let whole = d.styles.get("Whole").unwrap();
    assert_eq!(whole.para.align, Some(Align::Center));
    assert_eq!(whole.chr.bold, Some(true));
    assert_eq!(whole.chr.color, Some(TextColor::Rgb(kition_doc_doc::Rgb(0x1F, 0x4E, 0x79))));
    assert_eq!(whole.table.as_ref().unwrap().fill, Some(kition_doc_doc::Rgb(0xDD, 0xEB, 0xF7)));
    assert_eq!(whole.table.as_ref().unwrap().cell_margins, None);
    let merged = d.styles.table_style("Whole").unwrap();
    assert_eq!(merged.para.space_after, Some(0.0), "from Base");
    assert_eq!(merged.para.align, Some(Align::Center));
    assert_eq!(merged.parts.fill, Some(kition_doc_doc::Rgb(0xDD, 0xEB, 0xF7)));
    assert_eq!(merged.parts.cell_margins, Some([1.0, 14.4, 0.0, 14.4]), "from Base");
}

/// Word starts every note with a `w:footnoteRef` / `w:endnoteRef` mark: the note's own number.
/// It reads as a reference to the note itself, so the note text shows its number.
#[test]
fn note_marks_reference_their_own_note() {
    let note = |tag: &str, mark: &str, style: &str, id: u32, text: &str| {
        format!(
            r#"<w:{tag} w:id="{id}"><w:p><w:pPr><w:pStyle w:val="{style}Text"/></w:pPr><w:r><w:rPr><w:rStyle w:val="{style}Reference"/></w:rPr><w:{mark}/></w:r><w:r><w:t xml:space="preserve"> {text}</w:t></w:r></w:p></w:{tag}>"#
        )
    };
    let sep = |tag: &str| {
        format!(
            r#"<w:{tag} w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:{tag}><w:{tag} w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:{tag}>"#
        )
    };
    // The second footnote has a stray endnote mark, which is not its number.
    let footnotes = format!(
        r#"<w:footnotes {W_NS}>{}{}<w:footnote w:id="2"><w:p><w:r><w:endnoteRef/><w:t>Stray</w:t></w:r></w:p></w:footnote>{}</w:footnotes>"#,
        sep("footnote"),
        note("footnote", "footnoteRef", "Footnote", 1, "First."),
        note("footnote", "footnoteRef", "Footnote", 3, "Third."),
    );
    let endnotes = format!(r#"<w:endnotes {W_NS}>{}{}</w:endnotes>"#, sep("endnote"), note("endnote", "endnoteRef", "Endnote", 1, "End."));
    // A mark outside a note (here in the body) refers to nothing.
    let body = r#"<w:p><w:r><w:footnoteRef/><w:t>A</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r><w:r><w:footnoteReference w:id="2"/></w:r><w:r><w:footnoteReference w:id="3"/></w:r><w:r><w:endnoteReference w:id="1"/></w:r></w:p>"#;
    let bytes = docx(
        body,
        &[("rId1", "footnotes", "footnotes.xml"), ("rId2", "endnotes", "endnotes.xml")],
        &[("word/footnotes.xml", &footnotes), ("word/endnotes.xml", &endnotes)],
    );
    let d = kition_doc_docx::read(&bytes).unwrap();
    let p = paras(&d)[0];
    assert_eq!(p.text.chars().filter(|c| *c == kition_doc_doc::para::OBJ).count(), 4, "{:?}", p.objects);
    let refs: Vec<(NoteKind, u32)> = p
        .objects
        .iter()
        .map(|o| match o {
            InlineObject::NoteRef { kind, id, .. } => (*kind, *id),
            o => panic!("{o:?}"),
        })
        .collect();
    let first_para = |id: u32| d.parts.get(&id).unwrap().blocks[0].as_para().unwrap();
    for (kind, id) in [refs[0], refs[2], refs[3]] {
        let n = first_para(id);
        assert_eq!(n.objects, vec![InlineObject::NoteRef { kind, id, custom: String::new() }], "note {id}");
        assert!(n.text.starts_with(kition_doc_doc::para::OBJ), "{:?}", n.text);
        let style = if kind == NoteKind::Footnote { "FootnoteReference" } else { "EndnoteReference" };
        assert_eq!(n.props_of_char(0).style.as_deref(), Some(style));
    }
    assert_eq!(first_para(refs[0].1).plain_text(), " First.");
    assert_eq!(first_para(refs[3].1).plain_text(), " End.");
    let stray = first_para(refs[1].1);
    assert!(stray.objects.is_empty(), "{:?}", stray.objects);
    assert_eq!(stray.text, "Stray");
}

#[test]
fn empty_tbl_borders_elements_leave_sides_unset() {
    // Style TS: single borders on all six sides. Document: table styled TS with an empty
    // paired <w:tblBorders></w:tblBorders> and empty <w:tcBorders></w:tcBorders> per cell.
    // (Hand-written from ECMA-376 §17.4.39/§17.4.43; do not use files produced by Word.)
    let styles = format!(
        r#"<w:styles {W_NS}><w:style w:type="table" w:styleId="TS"><w:name w:val="TS"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="2" w:color="000000"/><w:left w:val="single" w:sz="2" w:color="000000"/><w:bottom w:val="single" w:sz="2" w:color="000000"/><w:right w:val="single" w:sz="2" w:color="000000"/><w:insideH w:val="single" w:sz="2" w:color="000000"/><w:insideV w:val="single" w:sz="2" w:color="000000"/></w:tblBorders></w:tblPr></w:style></w:styles>"#
    );
    let body = r#"<w:tbl><w:tblPr><w:tblStyle w:val="TS"/><w:tblBorders></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcBorders></w:tcBorders></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:tcBorders></w:tcBorders></w:tcPr><w:p/></w:tc></w:tr></w:tbl>"#;
    let d = kition_doc_docx::read(&docx(body, &[("rId1", "styles", "styles.xml")], &[("word/styles.xml", &styles)])).unwrap();
    let t = d.body.iter().find_map(|b| b.as_table()).unwrap();
    assert_eq!(t.props.borders, Some(kition_doc_doc::props::Borders::default()));
    assert_eq!(t.rows[0].cells[0].props.borders, Some(kition_doc_doc::props::Borders::default()));
    let parts = d.styles.get("TS").unwrap().table.clone().unwrap();
    let b = parts.borders.unwrap();
    assert!(b.top.is_some() && b.between.is_some() && b.inside_v.is_some());
}

#[test]
fn vml_image_and_inline_drawing() {
    let png: &[u8] = b"\x89PNG\r\n\x1a\nnot really a png";
    let body = r#"
<w:p><w:r><w:pict><v:shape style="width:1in;height:36pt" alt="vml pic"><v:imagedata r:id="rIdImg" o:title="t"/></v:shape></w:pict></w:r>
<w:r><w:drawing><wp:inline><wp:extent cx="1270000" cy="635000"/><wp:docPr id="3" name="P" descr="alt text"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rIdImg"/><a:srcRect l="10000"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>
<w:r><w:drawing><wp:inline><wp:extent cx="1" cy="1"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rIdMissing"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let doc = format!(r#"<w:document {W_NS}><w:body>{body}</w:body></w:document>"#);
    let r = rels(&[("rIdImg", "image", "media/pic.png"), ("rIdMissing", "image", "media/nope.png")]);
    let bytes = zip(&[
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", r.as_bytes()),
        ("word/media/pic.png", png),
    ]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    let p = paras(&d);
    // The third drawing's image part is missing: it keeps its frame, as Word shows it, under a media
    // name the document does not hold (so `doc_check` reports `doc.missing-media`).
    assert_eq!(p[0].objects.len(), 3);
    match &p[0].objects[2] {
        InlineObject::Image { media, .. } => {
            assert_eq!(media.as_str(), "nope.png");
            assert!(!d.media.contains_key(media.as_str()));
        }
        o => panic!("{o:?}"),
    }
    match (&p[0].objects[0], &p[0].objects[1]) {
        (InlineObject::Image { media: m1, w: w1, h: h1, alt: a1, .. }, InlineObject::Image { media: m2, w: w2, h: h2, alt: a2, crop, .. }) => {
            assert_eq!((m1.as_str(), *w1, *h1, a1.as_str()), ("pic.png", 72.0, 36.0, "vml pic"));
            assert_eq!((m2.as_str(), *w2, *h2, a2.as_str()), ("pic.png", 100.0, 50.0, "alt text"));
            assert_eq!(crop[0], 0.1);
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(d.media.get("pic.png").unwrap().as_slice(), png);
}

#[test]
fn chart_and_diagram_drawings_are_graphic_objects() {
    // A diagram's data holds a blip: the URI decides first, so no picture is read from it.
    let drawing = |uri: &str, data: &str| {
        format!(
            r#"<w:r><w:drawing><wp:inline><wp:extent cx="2743200" cy="1828800"/><wp:docPr id="5" name="G" descr="sales"/><a:graphic><a:graphicData uri="{uri}">{data}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
        )
    };
    let body = format!(
        "<w:p>{}{}</w:p>",
        drawing("http://schemas.openxmlformats.org/drawingml/2006/chart", ""),
        drawing("http://schemas.openxmlformats.org/drawingml/2006/diagram", r#"<a:blip r:embed="rIdImg"/>"#)
    );
    let d = read_body(&body);
    let p = paras(&d);
    assert_eq!(p[0].objects.len(), 2);
    match (&p[0].objects[0], &p[0].objects[1]) {
        (InlineObject::Graphic { w: w1, h: h1, alt, graphic: g1, .. }, InlineObject::Graphic { graphic: g2, .. }) => {
            assert_eq!((*w1, *h1, alt.as_str()), (216.0, 144.0, "sales"));
            assert_eq!(g1.kind, kition_doc_doc::graphic::GraphicKind::Chart);
            assert_eq!(g2.kind, kition_doc_doc::graphic::GraphicKind::Diagram);
            assert!(g1.items.is_empty() && g2.items.is_empty());
        }
        o => panic!("{o:?}"),
    }
}

#[test]
fn a_chart_part_is_built_once_and_only_through_a_chart_relationship() {
    let chart = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:barChart><c:ser><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#;
    let drawing = |id: &str| {
        format!(
            r#"<w:r><w:drawing><wp:inline><wp:extent cx="2743200" cy="1828800"/><wp:docPr id="5" name="G"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{id}"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
        )
    };
    let body = format!("<w:p>{}{}{}</w:p>", drawing("rIdChart"), drawing("rIdChart"), drawing("rIdNotChart"));
    let bytes = docx(
        &body,
        &[("rIdChart", "chart", "charts/chart1.xml"), ("rIdNotChart", "image", "charts/chart1.xml")],
        &[("word/charts/chart1.xml", chart)],
    );
    let d = kition_doc_docx::read(&bytes).unwrap();
    let graphics: Vec<_> = paras(&d)[0]
        .objects
        .iter()
        .filter_map(|o| if let InlineObject::Graphic { graphic, .. } = o { Some(graphic.clone()) } else { None })
        .collect();
    assert_eq!(graphics.len(), 3);
    assert!(!graphics[0].items.is_empty(), "the chart is drawn");
    assert!(std::sync::Arc::ptr_eq(&graphics[0], &graphics[1]), "one build for both references");
    assert!(graphics[2].items.is_empty(), "an image relationship is not a chart");
}

#[test]
fn strict_namespace_and_bad_numbers() {
    let doc = r#"<w:document xmlns:w="http://purl.oclc.org/ooxml/wordprocessingml/main"><w:body>
<w:p><w:pPr><w:jc w:val="end"/><w:ind w:start="1in" w:hanging="abc"/><w:spacing w:before="-50" w:line="99999999999999999999" w:lineRule="exact"/><w:outlineLvl w:val="300"/><w:numPr><w:numId w:val="-4"/></w:numPr></w:pPr>
<w:r><w:rPr><w:sz w:val="NaN"/><w:w w:val="999999"/><w:position w:val="-1e30"/><w:color w:val="GGGGGG"/><w:highlight w:val="mauve"/><w:u w:val="squiggle"/></w:rPr><w:t>strict</w:t></w:r></w:p>
<w:tbl><w:tblGrid><w:gridCol w:w="-5"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:gridSpan w:val="99999"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>
<w:sectPr><w:pgSz w:w="0" w:h="x"/><w:cols w:num="-3"/><w:pgNumType w:start="-1" w:fmt="klingon"/></w:sectPr>
</w:body></w:document>"#;
    let bytes = zip(&[("_rels/.rels", ROOT_RELS.as_bytes()), ("word/document.xml", doc.as_bytes())]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    let p = paras(&d);
    assert_eq!(p[0].text, "strict");
    assert_eq!(p[0].props.align, Some(Align::Right));
    assert_eq!(p[0].props.indent_left, Some(72.0));
    assert_eq!(p[0].props.space_before, Some(0.0));
    assert_eq!(p[0].props.outline_level, Some(9));
    let c = p[0].props_of_char(0);
    assert_eq!(c.size, None);
    assert_eq!(c.scale, Some(600.0));
    assert_eq!(c.color, None);
    let Block::Table(t) = &*d.body[1] else { panic!() };
    assert_eq!(t.rows[0].cells[0].span(), 63);
    assert_eq!(d.last_section.page_w, 612.0);
    assert_eq!(d.last_section.columns.count, 1);
    // The document can be written back.
    kition_doc_docx::write(&d).unwrap();
}

#[test]
fn unbalanced_fields_and_markers() {
    let body = r#"
<w:p><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>a</w:t></w:r><w:bookmarkEnd w:id="77"/><w:commentRangeEnd w:id="3"/></w:p>
<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>IF </w:instrText></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>MERGEFIELD x</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>X</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:instrText> = "y"</w:instrText></w:r></w:p>
<w:bookmarkStart w:id="1" w:name="between"/>
<w:p><w:r><w:t>after</w:t></w:r></w:p>
<w:bookmarkEnd w:id="1"/>"#;
    let d = read_body(body);
    let p = paras(&d);
    assert_eq!(p[0].text, "a");
    assert_eq!(
        p[1].objects,
        vec![InlineObject::Field {
            instr: "IF X = \"y\"".into(),
            result: String::new(),
            locked: false,
            code_runs: Vec::new(),
            code_fields: vec![kition_doc_doc::para::CodeField { start: 3, len: 1, instr: "MERGEFIELD x".into() }]
        }]
    );
    assert_eq!(p[2].objects, vec![InlineObject::BookmarkStart { name: "between".into() }, InlineObject::BookmarkEnd { name: "between".into() }]);
    assert_eq!(p[2].plain_text(), "after");
}

#[test]
fn header_self_reference_and_escaping_targets() {
    let hdr = format!(
        r#"<w:hdr {W_NS}><w:p><w:pPr><w:sectPr><w:headerReference w:type="default" r:id="rId1"/></w:sectPr></w:pPr><w:r><w:t>loop</w:t></w:r></w:p></w:hdr>"#
    );
    let hdr_rels = rels(&[("rId1", "header", "header1.xml")]);
    let body = r#"<w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rIdEvil"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:headerReference w:type="even" r:id="rIdH"/><w:footerReference w:type="first" r:id="rIdNone"/></w:sectPr>"#;
    let doc = format!(r#"<w:document {W_NS}><w:body>{body}</w:body></w:document>"#);
    let r = rels(&[("rIdH", "header", "header1.xml"), ("rIdEvil", "image", "../../../../etc/passwd"), ("rIdNone", "footer", "missing.xml")]);
    let bytes = zip(&[
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", r.as_bytes()),
        ("word/header1.xml", hdr.as_bytes()),
        ("word/_rels/header1.xml.rels", hdr_rels.as_bytes()),
    ]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    assert_eq!(d.last_section.headers.default, d.last_section.headers.even);
    assert!(d.media.is_empty());
    let out = kition_doc_docx::write(&d).unwrap();
    kition_doc_docx::read(&out).unwrap();
}

/// A document whose settings part is `settings` (None: no settings part at all).
fn with_settings(settings: Option<&str>) -> Document {
    let body = "<w:p><w:r><w:t>x</w:t></w:r></w:p>";
    match settings {
        None => read_body(body),
        Some(inner) => {
            let s = format!(r#"<w:settings {W_NS}>{inner}</w:settings>"#);
            kition_doc_docx::read(&docx(body, &[("rId1", "settings", "settings.xml")], &[("word/settings.xml", &s)])).unwrap()
        }
    }
}

#[test]
fn compatibility_mode_is_read_and_defaults_to_word_2007() {
    let compat = |v: &str| {
        format!(r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{v}"/></w:compat>"#)
    };
    assert_eq!(with_settings(Some(&compat("15"))).settings.compat_mode, 15);
    assert_eq!(with_settings(Some(&compat("14"))).settings.compat_mode, 14);
    // Unnamed, out of range or junk values.
    assert_eq!(with_settings(Some("<w:compat/>")).settings.compat_mode, kition_doc_doc::LEGACY_COMPAT_MODE);
    assert_eq!(with_settings(None).settings.compat_mode, kition_doc_doc::LEGACY_COMPAT_MODE);
    assert_eq!(with_settings(Some(&compat("-3"))).settings.compat_mode, 11);
    assert_eq!(with_settings(Some(&compat("99999"))).settings.compat_mode, 99);
    assert_eq!(with_settings(Some(&compat("abc"))).settings.compat_mode, kition_doc_doc::LEGACY_COMPAT_MODE);
    // Another vendor's setting of the same name is ignored.
    let other = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="urn:example" w:val="15"/></w:compat>"#;
    assert_eq!(with_settings(Some(other)).settings.compat_mode, kition_doc_doc::LEGACY_COMPAT_MODE);
}

#[test]
fn line_rule_is_read_case_insensitively() {
    // Word accepts `atleast` as well as the spec's `atLeast`.
    let d = read_body(
        r#"<w:p><w:pPr><w:spacing w:line="280" w:lineRule="atleast"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:line="300" w:lineRule="EXACT"/></w:pPr></w:p>"#,
    );
    let p = paras(&d);
    assert_eq!(p[0].props.line_spacing, Some(kition_doc_doc::props::LineSpacing::AtLeast(14.0)));
    assert_eq!(p[1].props.line_spacing, Some(kition_doc_doc::props::LineSpacing::Exactly(15.0)));
}

#[test]
fn char_border_reads_and_nil_resolves_off() {
    let d = read_body(
        r#"<w:p><w:r><w:rPr><w:bdr w:val="single" w:sz="8" w:space="1" w:color="FF0000"/></w:rPr><w:t>a</w:t></w:r><w:r><w:rPr><w:bdr w:val="nil"/></w:rPr><w:t>b</w:t></w:r></w:p>"#,
    );
    let p = paras(&d)[0];
    assert_eq!(
        p.props_of_char(0).border,
        Some(Border { style: BorderStyle::Single, width: 1.0, color: Some(Rgb(0xFF, 0, 0)), space: 1.0, theme: None })
    );
    let nil = p.props_of_char(1);
    assert_eq!(nil.border.map(|b| b.style), Some(BorderStyle::None));
    assert_eq!(d.styles.resolve_char(None, nil).border, None);
}

#[test]
fn anchor_alignment_reference_areas_and_distances() {
    let shape = |pos: &str, dist: &str| {
        format!(
            r#"<w:r><w:drawing><wp:anchor behindDoc="0" {dist}>{pos}<wp:extent cx="127000" cy="127000"/><wp:wrapTopAndBottom/><wp:docPr id="1" name="S"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:spPr><a:prstGeom prst="ellipse"/></wps:spPr></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
        )
    };
    let body = format!(
        "<w:p>{}{}</w:p>",
        shape(
            r#"<wp:positionH relativeFrom="margin"><wp:align>center</wp:align></wp:positionH><wp:positionV relativeFrom="topMargin"><wp:posOffset>-12700</wp:posOffset></wp:positionV>"#,
            r#"distT="25400" distB="50800" distL="0" distR="114300""#
        ),
        shape(
            r#"<wp:positionH relativeFrom="leftMargin"><wp:posOffset>63500</wp:posOffset></wp:positionH><wp:positionV relativeFrom="line"><wp:align>bottom</wp:align></wp:positionV>"#,
            ""
        ),
    );
    let d = read_body(&body);
    let p = paras(&d);
    let floats: Vec<_> = p[0].objects.iter().filter_map(|o| if let InlineObject::Shape { float, .. } = o { Some(*float) } else { None }).collect();
    let [a, b] = floats.as_slice() else { panic!("{floats:?}") };
    assert_eq!((a.h_rel, a.h_align, a.v_rel, a.v_align, a.y), (Anchor::Margin, Some(FloatAlign::Center), Anchor::TopMargin, None, -1.0));
    assert_eq!((a.dist, a.dist_top, a.dist_bottom), (9.0, 2.0, 4.0));
    assert_eq!((b.h_rel, b.h_align, b.x, b.v_rel, b.v_align), (Anchor::LeftMargin, None, 5.0, Anchor::Line, Some(FloatAlign::End)));
    assert_eq!((b.dist, b.dist_top, b.dist_bottom), (0.0, 0.0, 0.0));
}

#[test]
fn documents_without_a_compatibility_mode_are_laid_out_as_word_2007() {
    assert_eq!(read_body("<w:p/>").settings.compat_mode, kition_doc_doc::LEGACY_COMPAT_MODE);
}

#[test]
fn style_rfonts_without_ascii_inherits_doc_defaults_font() {
    // Normal names only East Asian / complex-script fonts, so Latin text keeps the
    // docDefaults font. A run hinted as East Asian still uses its East Asian font.
    let styles = format!(
        r#"<w:styles {W_NS}>
 <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:eastAsia="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults>
 <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/><w:rPr><w:rFonts w:eastAsia="Times New Roman" w:cs="Times New Roman"/></w:rPr></w:style>
</w:styles>"#
    );
    let body = r#"<w:p><w:r><w:t>Plain</w:t></w:r></w:p><w:p><w:r><w:rPr><w:rFonts w:hint="eastAsia" w:eastAsia="SimSun"/></w:rPr><w:t>漢字</w:t></w:r></w:p>"#;
    let d = kition_doc_docx::read(&docx(body, &[("rId1", "styles", "styles.xml")], &[("word/styles.xml", &styles)])).unwrap();
    assert_eq!(d.styles.get("Normal").unwrap().chr.font, None);
    let ps = paras(&d);
    let font = |p: &Paragraph| d.styles.resolve_char(p.props.style.as_deref(), &p.runs[0].props).font;
    assert_eq!(font(ps[0]), "Calibri");
    assert_eq!(font(ps[1]), "SimSun");
}

#[test]
fn bare_break_directly_in_paragraph() {
    let body = r#"<w:p><w:r><w:t>First</w:t></w:r><w:br/><w:r><w:t>Second</w:t></w:r><w:cr/><w:br w:type="page"/><w:r><w:t>Third</w:t></w:r></w:p>"#;
    let d = read_body(body);
    assert_eq!(paras(&d)[0].text, "First\nSecond\n\u{C}Third");
    let out = kition_doc_docx::write(&d).unwrap();
    let d2 = kition_doc_docx::read(&out).unwrap();
    assert_eq!(paras(&d2)[0].text, "First\nSecond\n\u{C}Third");
}

/// An absolutely positioned VML text box in a header floats where its style puts it
/// (relative to the page here, behind the text at a negative z-index, no wrapping) instead of
/// sitting inline in the header's flow, and it keeps that placement when saved and read back.
#[test]
fn absolute_vml_text_box_in_header_floats() {
    let w10 = r#"xmlns:w10="urn:schemas-microsoft-com:office:word""#;
    let boxes = r##"<w:p><w:r><w:pict><v:shape id="Box" type="#_x0000_t202" style="position:absolute;left:0;margin-left:0pt;margin-top:80pt;width:405pt;height:491.4pt;z-index:-1;mso-position-horizontal-relative:page;mso-position-vertical-relative:page"><v:textbox><w:txbxContent><w:p><w:r><w:t>WATERMARK</w:t></w:r></w:p></w:txbxContent></v:textbox><w10:wrap type="none"/></v:shape></w:pict></w:r></w:p>
<w:p><w:r><w:pict><v:rect style="position:absolute;margin-left:12pt;margin-top:-6pt;width:1in;height:36pt;z-index:3;mso-position-horizontal:center;mso-position-horizontal-relative:margin;mso-position-vertical-relative:top-margin-area;mso-wrap-distance-left:9pt;mso-wrap-distance-bottom:4pt"><v:textbox><w:txbxContent><w:p><w:r><w:t>square</w:t></w:r></w:p></w:txbxContent></v:textbox><w10:wrap type="square"/></v:rect></w:pict></w:r>
<w:r><w:pict><v:shape style="position:absolute;margin-left:1in;margin-top:2pt;width:72pt;height:20pt;z-index:5"><v:textbox><w:txbxContent><w:p><w:r><w:t>front</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r>
<w:r><w:pict><v:shape style="width:72pt;height:20pt"><v:textbox><w:txbxContent><w:p><w:r><w:t>inline</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>""##;
    let header = format!(r#"<w:hdr {W_NS} {w10}><w:p><w:r><w:t>Header label</w:t></w:r></w:p>{boxes}</w:hdr>"#);
    let body = r#"<w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/></w:sectPr>"#;
    let bytes = docx(body, &[("rIdH", "header", "header1.xml")], &[("word/header1.xml", &header)]);
    let check = |d: &Document| {
        let hid = d.last_section.headers.default.unwrap();
        let blocks = &d.parts.get(&hid).unwrap().blocks;
        let shapes: Vec<_> = blocks
            .iter()
            .filter_map(|b| b.as_para())
            .flat_map(|p| &p.objects)
            .filter_map(|o| if let InlineObject::Shape { kind, w, h, float, story, .. } = o { Some((*kind, *w, *h, *float, *story)) } else { None })
            .collect();
        let [(k0, w0, h0, f0, s0), (_, w1, h1, f1, _), (_, _, _, f2, _), (_, _, _, f3, _)] = shapes.as_slice() else { panic!("{shapes:?}") };
        assert_eq!(*k0, ShapeKind::TextBox);
        assert!((w0 - 405.0).abs() < 0.01 && (h0 - 491.4).abs() < 0.01, "{w0} x {h0}");
        assert_eq!((f0.wrap, f0.h_rel, f0.v_rel, f0.h_align, f0.v_align), (Wrap::BehindText, Anchor::Page, Anchor::Page, None, None));
        assert!(f0.x.abs() < 0.01 && (f0.y - 80.0).abs() < 0.01, "offset {} {}", f0.x, f0.y);
        let text = d.parts.get(&s0.unwrap()).unwrap().blocks[0].as_para().unwrap().text.clone();
        assert_eq!(text, "WATERMARK");
        assert_eq!((*w1, *h1), (72.0, 36.0));
        assert_eq!(
            (f1.wrap, f1.h_rel, f1.h_align, f1.v_rel, f1.v_align),
            (Wrap::Square, Anchor::Margin, Some(FloatAlign::Center), Anchor::TopMargin, None)
        );
        assert!((f1.y + 6.0).abs() < 0.01 && (f1.dist - 9.0).abs() < 0.01 && (f1.dist_bottom - 4.0).abs() < 0.01, "{f1:?}");
        // No wrap element: in front of the text, relative to the column and paragraph.
        assert_eq!((f2.wrap, f2.h_rel, f2.v_rel), (Wrap::InFrontOfText, Anchor::Column, Anchor::Paragraph));
        assert!((f2.x - 72.0).abs() < 0.01 && (f2.y - 2.0).abs() < 0.01, "{f2:?}");
        // Not absolutely positioned: still inline.
        assert_eq!(f3.wrap, Wrap::Inline);
    };
    let d = kition_doc_docx::read(&bytes).unwrap();
    check(&d);
    // Saved (as DrawingML anchors) and read back, the boxes keep their placement.
    check(&kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap());
}

#[test]
fn hostile_vml_style_values_stay_finite() {
    let body = r#"<w:p><w:r><w:pict><v:shape style="position:ABSOLUTE;margin-left:1e39pt;left:-1e39pt;margin-top:NaNpt;top:;width:-5pt;height:1e30in;z-index:99999999999999999999999;mso-position-horizontal:bogus;mso-wrap-distance-left:-3pt;mso-wrap-distance-top:1e9pt;;:;position"><v:textbox><w:txbxContent><w:p/></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>"#;
    let d = read_body(body);
    let p = paras(&d);
    let Some(InlineObject::Shape { w, h, float, .. }) = p[0].objects.first() else { panic!("{:?}", p[0].objects) };
    assert_eq!(float.wrap, Wrap::InFrontOfText);
    assert!([*w, *h, float.x, float.y, float.dist, float.dist_top].iter().all(|v| v.is_finite()), "{w} {h} {float:?}");
    assert_eq!((float.h_align, float.dist, float.dist_top), (None, 0.0, 1584.0));
}

// ---- charts and OLE objects written back ----

const CHART_XML: &str = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><c:chart><c:plotArea><c:barChart><c:ser><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart></c:plotArea></c:chart><c:externalData r:id="rId1"><c:autoUpdate val="0"/></c:externalData></c:chartSpace>"#;
/// Stands in for the chart's workbook: opaque bytes, never opened.
const WORKBOOK: &str = "PK\u{3}\u{4} not really a workbook";
const COLORS_XML: &str = r#"<cs:colorStyle xmlns:cs="http://schemas.microsoft.com/office/drawing/2012/chartStyle" meth="cycle" id="10"/>"#;
const CT_CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const CT_XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// A package holding one inline chart (3 × 1.5 in) with its workbook and colour style parts.
fn chart_docx() -> Vec<u8> {
    let body = r#"<w:p><w:r><w:t>Sales</w:t></w:r><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="2743200" cy="1371600"/><wp:docPr id="1" name="Chart 1"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="rId7"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let chart_rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" Target="../embeddings/Microsoft_Excel_Worksheet.xlsx"/><Relationship Id="rId2" Type="http://schemas.microsoft.com/office/2011/relationships/chartColorStyle" Target="colors1.xml"/></Relationships>"#;
    let types = format!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="xlsx" ContentType="{CT_XLSX}"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/charts/chart1.xml" ContentType="{CT_CHART}"/><Override PartName="/word/charts/colors1.xml" ContentType="application/vnd.ms-office.chartcolorstyle+xml"/></Types>"#
    );
    docx(
        body,
        &[("rId7", "chart", "charts/chart1.xml")],
        &[
            ("[Content_Types].xml", &types),
            ("word/charts/chart1.xml", CHART_XML),
            ("word/charts/_rels/chart1.xml.rels", chart_rels),
            ("word/charts/colors1.xml", COLORS_XML),
            ("word/embeddings/Microsoft_Excel_Worksheet.xlsx", WORKBOOK),
        ],
    )
}

/// The entries of a zip, by name.
fn unzip(bytes: &[u8]) -> std::collections::BTreeMap<String, Vec<u8>> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = std::collections::BTreeMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        out.insert(f.name().to_string(), b);
    }
    out
}

fn text(files: &std::collections::BTreeMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8(files.get(name).unwrap_or_else(|| panic!("no {name} in {:?}", files.keys())).clone()).unwrap()
}

/// Values of attribute `attr` on the elements named `el` (in written order).
fn attr_values(xml: &str, el: &str, attr: &str) -> Vec<String> {
    let start = |s: &str| format!(" {}", s.split('>').next().unwrap_or(""));
    xml.split(&format!("<{el} "))
        .skip(1)
        .filter_map(|s| start(s).split(&format!(" {attr}=\"")).nth(1)?.split('"').next().map(str::to_string))
        .collect()
}

/// (type, target) of relationship `id` in a relationships part.
fn rel(rels: &str, id: &str) -> (String, String) {
    let r = rels.split("<Relationship ").find(|r| r.contains(&format!("Id=\"{id}\""))).unwrap_or_else(|| panic!("no {id} in {rels}"));
    let get = |a: &str| r.split(&format!("{a}=\"")).nth(1).unwrap().split('"').next().unwrap().to_string();
    (get("Type"), get("Target"))
}

/// Package part a relationship target of `from_dir` names.
fn part_at(from_dir: &str, target: &str) -> String {
    let mut parts: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Checks a written chart: its part and the parts it relates to are all in the package, with the
/// original bytes and content types. Returns the chart's part name.
fn check_chart(files: &std::collections::BTreeMap<String, Vec<u8>>, rid: &str) -> String {
    let (kind, target) = rel(&text(files, "word/_rels/document.xml.rels"), rid);
    assert!(kind.ends_with("/chart"), "{kind}");
    let chart = part_at("word", &target);
    assert_eq!(text(files, &chart), CHART_XML);
    let (dir, file) = chart.rsplit_once('/').unwrap();
    let chart_rels = text(files, &format!("{dir}/_rels/{file}.rels"));
    let (kind, wb) = rel(&chart_rels, "rId1");
    assert!(kind.ends_with("/package"), "{kind}");
    let wb = part_at(dir, &wb);
    assert_eq!(files.get(&wb).map(Vec::as_slice), Some(WORKBOOK.as_bytes()), "{wb}");
    let (kind, colors) = rel(&chart_rels, "rId2");
    assert!(kind.ends_with("/chartColorStyle"), "{kind}");
    assert_eq!(text(files, &part_at(dir, &colors)), COLORS_XML);
    let types = text(files, "[Content_Types].xml");
    assert!(types.contains(&format!(r#"PartName="/{chart}" ContentType="{CT_CHART}""#)), "{types}");
    assert!(types.contains(&format!(r#"PartName="/{wb}" ContentType="{CT_XLSX}""#)), "{types}");
    chart
}

fn graphics(d: &Document) -> Vec<InlineObject> {
    paras(d).iter().flat_map(|p| p.objects.iter()).filter(|o| matches!(o, InlineObject::Graphic { .. })).cloned().collect()
}

#[test]
fn chart_round_trips_with_its_parts() {
    let d = kition_doc_docx::read(&chart_docx()).unwrap();
    let out = kition_doc_docx::write(&d).unwrap();
    let files = unzip(&out);
    let body = text(&files, "word/document.xml");
    let ids = attr_values(&body, "c:chart", "r:id");
    assert_eq!(ids.len(), 1, "{body}");
    assert_eq!(check_chart(&files, &ids[0]), "word/charts/chart1.xml");
    assert_eq!(attr_values(&body, "wp:extent", "cx"), ["2743200"]);

    // Read back: still drawn, and a second save still carries it.
    let back = kition_doc_docx::read(&out).unwrap();
    let g = graphics(&back);
    let [InlineObject::Graphic { w, graphic, .. }] = g.as_slice() else { panic!("{g:?}") };
    assert_eq!(*w, 216.0);
    assert!(!graphic.items.is_empty() && graphic.source.is_some());
    let again = unzip(&kition_doc_docx::write(&back).unwrap());
    let ids = attr_values(&text(&again, "word/document.xml"), "c:chart", "r:id");
    check_chart(&again, &ids[0]);
}

#[test]
fn a_flat_group_keeps_its_members_width_scale() {
    // Two lines in a group of no height (`a:ext cy="0"`, `a:chExt cy="0"`), as a header rule
    // drawn red then grey: the members' space is still 9995 wide, so the red line takes about a
    // quarter of the group's 500 pt and the grey one the rest.
    let line = |x: u32, w: u32, color: &str| {
        format!(
            r#"<wps:wsp><wps:cNvCnPr/><wps:spPr><a:xfrm><a:off x="{x}" y="1994"/><a:ext cx="{w}" cy="0"/></a:xfrm><a:prstGeom prst="line"><a:avLst/></a:prstGeom><a:ln w="12700"><a:solidFill><a:srgbClr val="{color}"/></a:solidFill></a:ln></wps:spPr><wps:bodyPr/></wps:wsp>"#
        )
    };
    let body = format!(
        r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="6350000" cy="0"/><wp:docPr id="1" name="Group 1"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="6350000" cy="0"/><a:chOff x="1254" y="1994"/><a:chExt cx="9995" cy="0"/></a:xfrm></wpg:grpSpPr>{}{}</wpg:wgp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#,
        line(3879, 7370, "5A5A5A"),
        line(1254, 2665, "C31F1F")
    );
    let d = read_body(&body);
    let group = paras(&d)[0].objects.iter().find(|o| matches!(o, InlineObject::Group { .. })).cloned().expect("a group");
    let rects: Vec<[f32; 4]> = group.group_rects(0.0, 0.0, 500.0, 0.0).into_iter().map(|(r, _)| r).collect();
    assert_eq!(rects.len(), 2, "{rects:?}");
    let near = |a: f32, b: f32| (a - b).abs() < 0.5;
    assert!(near(rects[0][0], 131.3) && near(rects[0][2], 368.7), "the grey line: {rects:?}");
    assert!(near(rects[1][0], 0.0) && near(rects[1][2], 133.3), "the red line: {rects:?}");
}

#[test]
fn chart_inside_a_group_round_trips_with_its_parts() {
    // A group of a rectangle and a chart (`wpg:graphicFrame`), as Word writes grouped charts.
    let body = r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="3657600" cy="1828800"/><wp:docPr id="1" name="Group 1"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="3657600" cy="1828800"/><a:chOff x="0" y="0"/><a:chExt cx="3657600" cy="1828800"/></a:xfrm></wpg:grpSpPr>
<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"/></wps:spPr><wps:bodyPr/></wps:wsp>
<wpg:graphicFrame><wpg:cNvPr id="2" name="Chart 2" descr="sales"/><wpg:cNvFrPr/><wpg:xfrm><a:off x="914400" y="0"/><a:ext cx="2743200" cy="1371600"/></wpg:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="rId7"/></a:graphicData></a:graphic></wpg:graphicFrame>
</wpg:wgp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let original = unzip(&chart_docx());
    let mut entries: Vec<(&str, &[u8])> =
        original.iter().filter(|(k, _)| *k != "word/document.xml").map(|(k, v)| (k.as_str(), v.as_slice())).collect();
    let doc = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W_NS}><w:body>{body}</w:body></w:document>"#);
    entries.push(("word/document.xml", doc.as_bytes()));
    let d = kition_doc_docx::read(&zip(&entries)).unwrap();
    let group = paras(&d).iter().flat_map(|p| p.objects.iter()).find(|o| matches!(o, InlineObject::Group { .. })).cloned();
    let Some(InlineObject::Group { children, .. }) = group else { panic!("no group") };
    assert_eq!(children.len(), 2);
    let InlineObject::Graphic { w, h, alt, graphic, .. } = &children[1].obj else { panic!("{:?}", children[1].obj) };
    assert_eq!((children[1].x, *w, *h, alt.as_str()), (72.0, 216.0, 108.0, "sales"));
    assert!(!graphic.items.is_empty() && graphic.source.is_some(), "drawn and kept");

    let files = unzip(&kition_doc_docx::write(&d).unwrap());
    let body = text(&files, "word/document.xml");
    assert!(body.contains("<wpg:graphicFrame>"), "{body}");
    let ids = attr_values(&body, "c:chart", "r:id");
    assert_eq!(ids.len(), 1, "{body}");
    check_chart(&files, &ids[0]);
    assert_eq!(attr_values(&body, "a:off", "x"), ["0", "0", "914400"]);
    // Read back: the same group, the chart drawn the same and still kept (its markup's
    // namespace declarations are the written part's).
    let back = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    let Some(InlineObject::Group { children: again, .. }) = paras(&back)[0].objects.first().cloned() else { panic!("no group") };
    // The member keeps the id it was written with (DR2).
    let mut first = again[0].clone();
    first.obj.set_object_uid(children[0].obj.object_uid().unwrap_or(0));
    assert_eq!(first, children[0]);
    let InlineObject::Graphic { w: w2, graphic: g2, .. } = &again[1].obj else { panic!("{:?}", again[1].obj) };
    assert_eq!((again[1].x, *w2), (72.0, 216.0));
    assert!(g2.items == graphic.items && g2.source.is_some());
}

#[test]
fn moved_and_copied_chart_keeps_its_parts() {
    let d = kition_doc_docx::read(&chart_docx()).unwrap();
    let g = graphics(&d);
    let [InlineObject::Graphic { alt, graphic, .. }] = g.as_slice() else { panic!("{g:?}") };
    // Resized and dragged to float on the page in Kition Doc, then copied and pasted.
    let float = kition_doc_doc::para::Float { wrap: Wrap::Square, h_rel: Anchor::Page, v_rel: Anchor::Page, x: 36.0, y: 18.0, ..Default::default() };
    let moved = InlineObject::Graphic { w: 288.0, h: 144.0, alt: alt.clone(), float, graphic: graphic.clone() };
    let mut p = Paragraph::new();
    p.insert_object(0, moved.clone(), &Default::default()).unwrap();
    p.insert_object(p.len(), moved, &Default::default()).unwrap();
    let mut doc = d.clone();
    doc.body = vec![kition_doc_doc::para_block(p)];

    let out = kition_doc_docx::write(&doc).unwrap();
    let files = unzip(&out);
    let body = text(&files, "word/document.xml");
    assert_eq!(attr_values(&body, "wp:extent", "cx"), ["3657600", "3657600"], "the new size");
    assert_eq!(body.matches("<wp:posOffset>457200</wp:posOffset>").count(), 2, "the new position");
    let ids = attr_values(&body, "c:chart", "r:id");
    assert_eq!(ids.len(), 2);
    let charts: Vec<String> = ids.iter().map(|id| check_chart(&files, id)).collect();
    assert_ne!(charts[0], charts[1], "each copy has its own chart part");

    let back = kition_doc_docx::read(&out).unwrap();
    let g = graphics(&back);
    assert_eq!(g.len(), 2);
    for o in &g {
        let InlineObject::Graphic { w, float, graphic, .. } = o else { panic!() };
        assert_eq!((*w, float.wrap, float.x), (288.0, Wrap::Square, 36.0));
        assert!(!graphic.items.is_empty() && graphic.source.is_some());
    }
}

#[test]
fn ole_object_round_trips() {
    let body = r##"<w:p><w:r><w:object w:dxaOrig="1440" w:dyaOrig="720"><v:shape id="_x0000_i1025" type="#_x0000_t75" style="width:72pt;height:36pt" o:ole=""><v:imagedata r:id="rId8" o:title=""/></v:shape><o:OLEObject Type="Embed" ProgID="Package" ShapeID="_x0000_i1025" DrawAspect="Content" ObjectID="_1000000001" r:id="rId9"/></w:object></w:r></w:p>"##;
    let picture = "\u{89}PNG stand-in";
    let ole = "\u{d0}\u{cf} opaque OLE storage";
    let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="png" ContentType="image/png"/><Default Extension="bin" ContentType="application/vnd.openxmlformats-officedocument.oleObject"/></Types>"#;
    let bytes = docx(
        body,
        &[("rId8", "image", "media/image1.png"), ("rId9", "oleObject", "embeddings/oleObject1.bin")],
        &[("[Content_Types].xml", types), ("word/media/image1.png", picture), ("word/embeddings/oleObject1.bin", ole)],
    );
    let d = kition_doc_docx::read(&bytes).unwrap();
    let Some(InlineObject::Image { w, ole: Some(_), .. }) = paras(&d)[0].objects.first() else { panic!("{:?}", paras(&d)[0].objects) };
    assert_eq!(*w, 72.0);

    // Resized in Kition Doc.
    let mut doc = d.clone();
    if let Some(kition_doc_doc::Block::Para(p)) = doc.body.first_mut().map(std::sync::Arc::make_mut)
        && let Some(InlineObject::Image { w, h, .. }) = p.objects.first_mut()
    {
        (*w, *h) = (144.0, 72.0);
    }
    let out = kition_doc_docx::write(&doc).unwrap();
    let files = unzip(&out);
    let body = text(&files, "word/document.xml");
    let rels = text(&files, "word/_rels/document.xml.rels");
    assert_eq!(attr_values(&body, "v:shape", "style"), ["width:144pt;height:72pt"]);
    let [ole_id] = attr_values(&body, "o:OLEObject", "r:id").try_into().unwrap();
    let (kind, target) = rel(&rels, &ole_id);
    assert!(kind.ends_with("/oleObject"), "{kind}");
    let ole_part = part_at("word", &target);
    assert_eq!(files.get(&ole_part).map(Vec::as_slice), Some(ole.as_bytes()));
    assert!(
        text(&files, "[Content_Types].xml")
            .contains(&format!(r#"PartName="/{ole_part}" ContentType="application/vnd.openxmlformats-officedocument.oleObject""#))
    );
    let [pic_id] = attr_values(&body, "v:imagedata", "r:id").try_into().unwrap();
    let (kind, target) = rel(&rels, &pic_id);
    assert!(kind.ends_with("/image"), "{kind}");
    assert_eq!(files.get(&part_at("word", &target)).map(Vec::as_slice), Some(picture.as_bytes()));
    assert!(!body.contains("<w:drawing>"), "the object, not a plain picture");

    let back = kition_doc_docx::read(&out).unwrap();
    assert!(matches!(paras(&back)[0].objects.first(), Some(InlineObject::Image { w: 144.0, ole: Some(_), .. })));
}

#[test]
fn hostile_embedded_relationships_stay_bounded() {
    // The chart relates to itself, the main document, a relationships part and a missing part;
    // another drawing uses an id with no relationship. Reading and saving neither loop nor carry
    // those along, and the chart is still written back.
    let chart_rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="x/package" Target="chart1.xml"/><Relationship Id="rId2" Type="x/a" Target="../document.xml"/><Relationship Id="rId3" Type="x/b" Target="../_rels/document.xml.rels"/><Relationship Id="rId4" Type="x/c" Target="gone.bin"/><Relationship Id="rId4" Type="x/d" Target="chart1.xml"/></Relationships>"#;
    let drawing = |id: &str| {
        format!(
            r#"<w:r><w:drawing><wp:inline><wp:extent cx="2743200" cy="1371600"/><wp:docPr id="1" name="C"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{id}"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
        )
    };
    let body = format!("<w:p>{}{}</w:p>", drawing("rId7"), drawing("rIdNowhere"));
    let bytes = docx(
        &body,
        &[("rId7", "chart", "charts/chart1.xml")],
        &[("word/charts/chart1.xml", CHART_XML), ("word/charts/_rels/chart1.xml.rels", chart_rels)],
    );
    let d = kition_doc_docx::read(&bytes).unwrap();
    let g = graphics(&d);
    assert_eq!(g.len(), 2);
    let sources: Vec<bool> = g.iter().map(|o| matches!(o, InlineObject::Graphic { graphic, .. } if graphic.source.is_some())).collect();
    assert_eq!(sources, [true, false], "a dangling id can't be written back");
    let files = unzip(&kition_doc_docx::write(&d).unwrap());
    let body = text(&files, "word/document.xml");
    let [id] = attr_values(&body, "c:chart", "r:id").try_into().unwrap();
    let chart = part_at("word", &rel(&text(&files, "word/_rels/document.xml.rels"), &id).1);
    let rels = text(&files, "word/charts/_rels/chart1.xml.rels");
    assert_eq!(rel(&rels, "rId1").1, "chart1.xml", "its relationship to itself, to the same written part");
    assert!(!rels.contains("document.xml") && !rels.contains("gone.bin") && rels.matches("Id=\"rId4\"").count() <= 1, "{rels}");
    assert_eq!(text(&files, &chart), CHART_XML);
    assert_eq!(files.keys().filter(|k| k.starts_with("word/charts/") && !k.contains("_rels")).count(), 1);
}

/// A group as Word saves one (in `mc:AlternateContent`, needing `wpg`): its members' space
/// starts at `a:chOff`, and a nested group is flattened into it through its own transform.
#[test]
fn word_group_with_nested_group() {
    let body = r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="wpg"><w:drawing><wp:anchor behindDoc="0" distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="1" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="2540000" cy="1270000"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="1" name="Group 1"/><wp:cNvGraphicFramePr/>
<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2540000" cy="1270000"/><a:chOff x="127000" y="127000"/><a:chExt cx="2540000" cy="1270000"/></a:xfrm></wpg:grpSpPr>
<wps:wsp><wps:cNvPr id="2" name="Rectangle 2"/><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="127000" y="127000"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:bodyPr/></wps:wsp>
<wpg:grpSp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="1397000" y="762000"/><a:ext cx="1270000" cy="635000"/><a:chOff x="0" y="0"/><a:chExt cx="2540000" cy="1270000"/></a:xfrm></wpg:grpSpPr>
<wps:wsp><wps:cNvPr id="3" name="Oval 3"/><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="254000" y="254000"/><a:ext cx="508000" cy="254000"/></a:xfrm><a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom></wps:spPr><wps:bodyPr/></wps:wsp>
</wpg:grpSp></wpg:wgp></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice><mc:Fallback><w:pict/></mc:Fallback></mc:AlternateContent></w:r></w:p>"#;
    let d = read_body(body);
    let p = paras(&d);
    let InlineObject::Group { w, h, float, ch_w, ch_h, children, .. } = &p[0].objects[0] else { panic!("{:?}", p[0].objects) };
    assert_eq!((*w, *h, *ch_w, *ch_h), (200.0, 100.0, 200.0, 100.0));
    assert_eq!(float.wrap, Wrap::Square);
    let got: Vec<(f32, f32, ShapeKind, f32, f32)> = children
        .iter()
        .map(|c| match &c.obj {
            InlineObject::Shape { kind, w, h, .. } => (c.x, c.y, *kind, *w, *h),
            o => panic!("{o:?}"),
        })
        .collect();
    assert_eq!(got, [(0.0, 0.0, ShapeKind::Rectangle, 100.0, 50.0), (110.0, 60.0, ShapeKind::Ellipse, 20.0, 10.0)]);
}

/// The first object of the first paragraph, made mutable.
fn first_object(doc: &mut Document) -> &mut InlineObject {
    let Some(kition_doc_doc::Block::Para(p)) = doc.body.first_mut().map(std::sync::Arc::make_mut) else { panic!("a paragraph") };
    p.objects.first_mut().expect("an object")
}

/// An OLE object turned or flipped in Kition Doc is written back turned, in its VML
/// shape (`rotation`, `flip`) and in a DrawingML picture (`a:xfrm` rot/flipH/flipV, with the
/// effect extent covering the rotated bounds like other drawings).
#[test]
fn rotated_ole_objects_round_trip() {
    let picture = "\u{89}PNG stand-in";
    let ole = "\u{d0}\u{cf} opaque OLE storage";
    let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="png" ContentType="image/png"/><Default Extension="bin" ContentType="application/vnd.openxmlformats-officedocument.oleObject"/></Types>"#;
    let parts = [("[Content_Types].xml", types), ("word/media/image1.png", picture), ("word/embeddings/oleObject1.bin", ole)];
    let doc_rels = [("rId8", "image", "media/image1.png"), ("rId9", "oleObject", "embeddings/oleObject1.bin")];
    let spin = |o: &InlineObject| o.frame().map(|f| f.2.spin()).unwrap();

    // VML: read turned, written with the new turn.
    let vml = r##"<w:p><w:r><w:object w:dxaOrig="1440" w:dyaOrig="720"><v:shape id="_x0000_i1025" type="#_x0000_t75" style="width:72pt;height:36pt;rotation:30;flip:x" o:ole=""><v:imagedata r:id="rId8" o:title=""/></v:shape><o:OLEObject Type="Embed" ProgID="Package" ShapeID="_x0000_i1025" DrawAspect="Content" ObjectID="_1000000001" r:id="rId9"/></w:object></w:r></w:p>"##;
    let mut d = kition_doc_docx::read(&docx(vml, &doc_rels, &parts)).unwrap();
    assert_eq!(spin(&paras(&d)[0].objects[0]), kition_doc_geom::Spin::new(30.0, true, false));
    if let Some(f) = first_object(&mut d).float_mut() {
        f.set_spin(kition_doc_geom::Spin::new(45.0, false, true));
    }
    let out = kition_doc_docx::write(&d).unwrap();
    let body = text(&unzip(&out), "word/document.xml");
    assert_eq!(attr_values(&body, "v:shape", "style"), ["width:72pt;height:36pt;rotation:45;flip:y"]);
    let back = kition_doc_docx::read(&out).unwrap();
    assert!(matches!(&paras(&back)[0].objects[0], InlineObject::Image { ole: Some(_), .. }));
    assert_eq!(spin(&paras(&back)[0].objects[0]), kition_doc_geom::Spin::new(45.0, false, true));

    // DrawingML: the picture's `a:xfrm` and the drawing's effect extent follow the turn.
    let dml = r##"<w:p><w:r><w:object w:dxaOrig="1440" w:dyaOrig="720"><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="914400" cy="457200"/><wp:docPr id="7" name="Object 7"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name=""/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId8"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm flipH="1"><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing><o:OLEObject Type="Embed" ProgID="Package" DrawAspect="Content" ObjectID="_1000000002" r:id="rId9"/></w:object></w:r></w:p>"##;
    let mut d = kition_doc_docx::read(&docx(dml, &doc_rels, &parts)).unwrap();
    assert_eq!(spin(&paras(&d)[0].objects[0]), kition_doc_geom::Spin::new(0.0, true, false));
    if let Some(f) = first_object(&mut d).float_mut() {
        f.set_spin(kition_doc_geom::Spin::new(90.0, true, false));
    }
    let out = kition_doc_docx::write(&d).unwrap();
    let body = text(&unzip(&out), "word/document.xml");
    assert_eq!(attr_values(&body, "a:xfrm", "rot"), ["5400000"]);
    assert_eq!(attr_values(&body, "a:xfrm", "flipH"), ["1"]);
    // 72 × 36 pt turned 90°: 18 pt narrower each side, 18 pt taller each side.
    assert_eq!(attr_values(&body, "wp:effectExtent", "l"), ["-228600"]);
    assert_eq!(attr_values(&body, "wp:effectExtent", "t"), ["228600"]);
    let back = kition_doc_docx::read(&out).unwrap();
    let o = &paras(&back)[0].objects[0];
    assert!(matches!(o, InlineObject::Image { ole: Some(_), .. }), "{o:?}");
    assert_eq!(spin(o), kition_doc_geom::Spin::new(90.0, true, false));
    assert_eq!(o.frame().unwrap().2.effect, [0.0; 4], "the rotated overhang is not kept as effects room");
}

/// Tracked formatting changes (ECMA-376 §17.13.5) survive open and save — run,
/// paragraph mark, paragraph, numbering, table, row, cell and section — each written last in
/// its parent, holding the properties before the change.
#[test]
fn formatting_revisions_round_trip() {
    let body = r#"
<w:p><w:r><w:t xml:space="preserve">Plain, </w:t></w:r><w:r><w:rPr><w:b/><w:rPrChange w:id="5" w:author="Alice" w:date="2026-10-01T00:00:00Z"><w:rPr/></w:rPrChange></w:rPr><w:t>bold by Alice</w:t></w:r><w:r><w:rPr><w:sz w:val="28"/><w:rPrChange w:id="99999999999" w:author="Bob"><w:rPr><w:i/><w:sz w:val="24"/><w:rPrChange w:id="1" w:author="Nested"><w:rPr/></w:rPrChange></w:rPr></w:rPrChange></w:rPr><w:t xml:space="preserve"> end.</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/><w:numberingChange w:id="6" w:author="Alice" w:date="2026-10-01T00:00:00Z" w:original="1."/></w:numPr><w:jc w:val="center"/><w:rPr><w:i/><w:rPrChange w:id="7" w:author="Alice" w:date="2026-10-01T00:00:00Z"><w:rPr/></w:rPrChange></w:rPr><w:pPrChange w:id="8" w:author="Alice" w:date="2026-10-01T00:00:00Z"><w:pPr><w:ind w:left="720"/></w:pPr></w:pPrChange></w:pPr><w:r><w:t>Centred</w:t></w:r></w:p>
<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:jc w:val="center"/><w:tblPrChange w:id="9" w:author="Bob"><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr></w:tblPrChange></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>
<w:tr><w:trPr><w:cantSplit/><w:trPrChange w:id="10" w:author="Bob"><w:trPr/></w:trPrChange></w:trPr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/><w:shd w:val="clear" w:color="auto" w:fill="FFFF00"/><w:tcPrChange w:id="11" w:author="Bob"><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr></w:tcPrChange></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p/>
<w:sectPr><w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/><w:sectPrChange w:id="12" w:author="Carol" w:date="2026-10-02T00:00:00Z"><w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr></w:sectPrChange></w:sectPr>"#;
    let d = read_body(body);
    let rev = |i: u32| d.revisions.get(i as usize).map(|r| (r.kind, r.author.as_str(), r.date.as_str()));
    let p = paras(&d);
    let runs: Vec<_> = p[0].run_ranges().map(|(r, c)| (r, c.clone())).collect();
    assert_eq!(runs.len(), 3);
    let alice = runs[1].1.fmt_change.as_deref().expect("Alice's rPrChange");
    assert_eq!(rev(alice.rev), Some((kition_doc_doc::RevisionKind::Format, "Alice", "2026-10-01T00:00:00Z")));
    assert_eq!((runs[1].1.bold, alice.old.bold), (Some(true), None));
    let bob = runs[2].1.fmt_change.as_deref().expect("Bob's rPrChange");
    assert_eq!((bob.old.italic, bob.old.size, bob.old.fmt_change.is_none()), (Some(true), Some(12.0), true), "a nested change is dropped");
    assert!(runs[0].1.fmt_change.is_none());
    let pc = p[1].props.fmt_change.as_deref().expect("pPrChange");
    assert_eq!((p[1].props.align, pc.old.align, pc.old.indent_left), (Some(Align::Center), None, Some(36.0)));
    assert_eq!(p[1].props.num_change.as_deref().map(|n| n.original.as_str()), Some("1."));
    assert!(p[1].mark.fmt_change.is_some());
    let Some(Block::Table(t)) = d.body.get(2).map(|b| &**b) else { panic!("table") };
    assert_eq!(t.props.fmt_change.as_deref().map(|c| c.old.align), Some(None));
    assert_eq!(t.rows[0].props.fmt_change.as_deref().map(|c| c.old.cant_split), Some(false));
    assert_eq!(t.rows[0].cells[0].props.fmt_change.as_deref().map(|c| c.old.shading), Some(None));
    let sc = d.last_section.fmt_change.as_deref().expect("sectPrChange");
    assert_eq!((d.last_section.landscape, sc.old.landscape), (true, false));
    assert_eq!(rev(sc.rev).map(|r| r.1), Some("Carol"));

    // Written back: each change is the last child of its parent, and reads back the same.
    let bytes = kition_doc_docx::write(&d).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("word/document.xml").unwrap(), &mut xml).unwrap();
    for (tag, parent) in [
        ("w:rPrChange", "</w:rPr></w:rPrChange></w:rPr>"),
        ("w:pPrChange", "</w:pPr></w:pPrChange></w:pPr>"),
        ("w:tblPrChange", "</w:tblPr></w:tblPrChange></w:tblPr>"),
        ("w:trPrChange", "</w:trPrChange></w:trPr>"),
        ("w:tcPrChange", "</w:tcPr></w:tcPrChange></w:tcPr>"),
        ("w:sectPrChange", "</w:sectPr></w:sectPrChange></w:sectPr>"),
    ] {
        assert!(xml.contains(&format!("<{tag} w:id=")), "{tag}: {xml}");
        assert!(xml.contains(parent), "{tag} last: {xml}");
    }
    assert!(xml.contains(r#"<w:numberingChange w:id="#) && xml.contains(r#"w:original="1."/></w:numPr>"#), "{xml}");
    assert!(xml.contains(r#"<w:b/><w:bCs/><w:rPrChange w:id="#), "{xml}");
    let again = kition_doc_docx::read(&bytes).unwrap();
    assert_eq!(again.body, d.body);
    assert_eq!(again.last_section, d.last_section);
    assert_eq!(again.revisions, d.revisions);
}

#[test]
fn east_asian_fonts_and_languages_round_trip() {
    let body = r#"
<w:p><w:r><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="SimSun" w:cs="Arial"/><w:lang w:val="en-US" w:eastAsia="zh-CN" w:bidi="ar-SA"/></w:rPr><w:t>Hello</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:rFonts w:eastAsia="MS Mincho" w:hint="eastAsia"/><w:lang w:eastAsia="ja-JP"/></w:rPr><w:t>x</w:t></w:r></w:p>"#;
    let d = read_body(body);
    let p = paras(&d);
    let first = p[0].props_of_char(0);
    assert_eq!(first.font.as_deref(), Some("Calibri"));
    assert_eq!(first.font_ea.as_deref(), Some("SimSun"));
    assert_eq!(first.font_cs.as_deref(), Some("Arial"));
    assert_eq!((first.lang.as_deref(), first.lang_ea.as_deref(), first.lang_bidi.as_deref()), (Some("en-US"), Some("zh-CN"), Some("ar-SA")));
    let second = p[1].props_of_char(0);
    // A run hinted East Asian takes that font as its own too (see the reader).
    assert_eq!(
        (second.font.as_deref(), second.font_ea.as_deref(), second.font_hint.as_deref()),
        (Some("MS Mincho"), Some("MS Mincho"), Some("eastAsia"))
    );
    assert_eq!(first.font_hint, None);
    assert_eq!((second.lang.as_deref(), second.lang_ea.as_deref()), (None, Some("ja-JP")));
    let again = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(again.body, d.body);
}

/// The text of a package part, if the package has it.
fn package_part(bytes: &[u8], name: &str) -> Option<String> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut entry = zip.by_name(name).ok()?;
    let mut text = String::new();
    entry.read_to_string(&mut text).unwrap();
    Some(text)
}

#[test]
fn the_font_table_is_kept_unless_it_embeds_fonts() {
    let table = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:fonts xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:font w:name="SimSun"><w:altName w:val="Song"/><w:panose1 w:val="02010600030101010101"/><w:charset w:val="86"/><w:family w:val="auto"/><w:pitch w:val="variable"/></w:font></w:fonts>"#;
    let bytes = docx("<w:p><w:r><w:t>x</w:t></w:r></w:p>", &[("rId9", "fontTable", "fontTable.xml")], &[("word/fontTable.xml", table)]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    let written = kition_doc_docx::write(&d).unwrap();
    assert_eq!(package_part(&written, "word/fontTable.xml").as_deref(), Some(table));
    let rels = package_part(&written, "word/_rels/document.xml.rels").unwrap();
    assert!(rels.contains("relationships/fontTable") && rels.contains(r#"Target="fontTable.xml""#), "{rels}");
    assert!(package_part(&written, "[Content_Types].xml").unwrap().contains("/word/fontTable.xml"));
    // And again from what was written.
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(package_part(&kition_doc_docx::write(&again).unwrap(), "word/fontTable.xml").as_deref(), Some(table));

    // A table pointing at embedded font parts (not carried) is left out.
    let embedding = table.replace("<w:family", r#"<w:embedRegular xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId1" w:fontKey="{00000000-0000-0000-0000-000000000000}"/><w:family"#);
    let bytes = docx("<w:p><w:r><w:t>x</w:t></w:r></w:p>", &[("rId9", "fontTable", "fontTable.xml")], &[("word/fontTable.xml", &embedding)]);
    let written = kition_doc_docx::write(&kition_doc_docx::read(&bytes).unwrap()).unwrap();
    assert!(package_part(&written, "word/fontTable.xml").is_none());
    assert!(!package_part(&written, "word/_rels/document.xml.rels").unwrap().contains("fontTable"));
}

#[test]
fn east_asian_paragraph_grid_properties_round_trip() {
    let body = r#"
<w:p><w:pPr><w:adjustRightInd w:val="0"/><w:snapToGrid w:val="0"/><w:jc w:val="center"/><w:textAlignment w:val="baseline"/></w:pPr><w:r><w:t>Grid</w:t></w:r></w:p>
<w:p><w:pPr><w:textAlignment w:val="sideways"/></w:pPr><w:r><w:t>Bad value</w:t></w:r></w:p>"#;
    let d = read_body(body);
    let p = paras(&d);
    assert_eq!(
        (p[0].props.adjust_right_ind, p[0].props.snap_to_grid, p[0].props.text_alignment.as_deref()),
        (Some(false), Some(false), Some("baseline"))
    );
    assert_eq!(p[1].props.text_alignment, None);
    let written = kition_doc_docx::write(&d).unwrap();
    let xml = package_part(&written, "word/document.xml").unwrap();
    // In schema order: after `w:bidi`'s place and before `w:spacing`; `w:textAlignment` after `w:jc`.
    assert!(
        xml.contains(r#"<w:adjustRightInd w:val="0"/><w:snapToGrid w:val="0"/><w:jc w:val="center"/><w:textAlignment w:val="baseline"/>"#),
        "{xml}"
    );
    assert_eq!(kition_doc_docx::read(&written).unwrap().body, d.body);
}

#[test]
fn legacy_compat_options_and_theme_font_languages_round_trip() {
    let settings = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:defaultTabStop w:val="420"/><w:characterSpacingControl w:val="compressPunctuation"/><w:compat><w:spaceForUL/><w:balanceSingleByteDoubleByteWidth/><w:ulTrailSpace/><w:doNotExpandShiftReturn w:val="0"/><w:useFELayout/><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat><w:themeFontLang w:val="en-US" w:eastAsia="zh-CN"/></w:settings>"#;
    let bytes = docx("<w:p><w:r><w:t>x</w:t></w:r></w:p>", &[("rId9", "settings", "settings.xml")], &[("word/settings.xml", settings)]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    let s = &d.settings;
    assert_eq!(s.compat_flags, ["spaceForUL", "balanceSingleByteDoubleByteWidth", "ulTrailSpace", "useFELayout"]);
    assert_eq!(s.char_spacing.as_deref(), Some("compressPunctuation"));
    let lang = s.theme_font_lang.as_ref().unwrap();
    assert_eq!((lang.latin.as_deref(), lang.east_asia.as_deref(), lang.bidi.as_deref()), (Some("en-US"), Some("zh-CN"), None));
    let written = kition_doc_docx::write(&d).unwrap();
    let xml = package_part(&written, "word/settings.xml").unwrap();
    assert!(
        xml.contains(r#"<w:compat><w:spaceForUL/><w:balanceSingleByteDoubleByteWidth/><w:ulTrailSpace/><w:useFELayout/><w:compatSetting"#),
        "{xml}"
    );
    assert!(xml.contains(r#"<w:characterSpacingControl w:val="compressPunctuation"/>"#), "{xml}");
    assert!(xml.contains(r#"<w:themeFontLang w:val="en-US" w:eastAsia="zh-CN"/>"#), "{xml}");
    assert_eq!(kition_doc_docx::read(&written).unwrap().settings, d.settings);
}

#[test]
fn latent_styles_are_kept() {
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="21"/></w:rPr></w:rPrDefault></w:docDefaults><w:latentStyles w:defLockedState="0" w:defUIPriority="99" w:defSemiHidden="0" w:defUnhideWhenUsed="0" w:defQFormat="0" w:count="376"><w:lsdException w:name="Normal" w:uiPriority="0" w:qFormat="1"/><w:lsdException w:name="heading 1" w:uiPriority="9" w:qFormat="1"/><w:lsdException w:name="heading 2" w:semiHidden="1" w:uiPriority="9" w:unhideWhenUsed="1" w:qFormat="1"/></w:latentStyles><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"#;
    let latent = &styles[styles.find("<w:latentStyles").unwrap()..styles.find("<w:style ").unwrap()];
    let bytes = docx("<w:p><w:r><w:t>x</w:t></w:r></w:p>", &[("rId9", "styles", "styles.xml")], &[("word/styles.xml", styles)]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    let written = kition_doc_docx::write(&d).unwrap();
    let out = package_part(&written, "word/styles.xml").unwrap();
    // After the document defaults, before the first style, as the schema orders them.
    let at = out.find(latent).unwrap_or_else(|| panic!("{out}"));
    assert!(out.find("</w:docDefaults>").unwrap() < at && at < out.find("<w:style ").unwrap(), "{out}");
    let again = kition_doc_docx::write(&kition_doc_docx::read(&written).unwrap()).unwrap();
    assert!(package_part(&again, "word/styles.xml").unwrap().contains(latent));
    // A document made in Kition Doc has none.
    assert!(!package_part(&kition_doc_docx::write(&Document::new()).unwrap(), "word/styles.xml").unwrap().contains("latentStyles"));
}

const THEME_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Test Theme"><a:themeElements><a:clrScheme name="Test"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="Test"><a:majorFont><a:latin typeface="Test Heading"/><a:ea typeface=""/><a:cs typeface=""/><a:font script="Hans" typeface="Test Hei"/></a:majorFont><a:minorFont><a:latin typeface="Test Body"/><a:ea typeface=""/><a:cs typeface=""/><a:font script="Hans" typeface="Test Song"/></a:minorFont></a:fontScheme><a:fmtScheme name="Test"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="50000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"/></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst><a:outerShdw blurRad="57150" dist="19050" dir="5400000" algn="ctr" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="63000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#;

#[test]
fn the_theme_part_is_kept_while_the_theme_is_unchanged() {
    let bytes = docx("<w:p><w:r><w:t>x</w:t></w:r></w:p>", &[("rId9", "theme", "theme/theme1.xml")], &[("word/theme/theme1.xml", THEME_XML)]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    assert_eq!(
        (d.settings.theme_name.as_str(), d.settings.major_font.as_str(), d.settings.minor_font.as_str()),
        ("Test Theme", "Test Heading", "Test Body")
    );
    let written = kition_doc_docx::write(&d).unwrap();
    assert_eq!(package_part(&written, "word/theme/theme1.xml").as_deref(), Some(THEME_XML));
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(again.settings, d.settings);
    assert_eq!(package_part(&kition_doc_docx::write(&again).unwrap(), "word/theme/theme1.xml").as_deref(), Some(THEME_XML));

    // A theme changed in Kition Doc is written from the document's settings.
    let mut changed = d.clone();
    changed.settings.minor_font = "Other Body".into();
    let theme = package_part(&kition_doc_docx::write(&changed).unwrap(), "word/theme/theme1.xml").unwrap();
    assert!(theme.contains(r#"<a:latin typeface="Other Body"/>"#) && !theme.contains("Test Song"), "{theme}");
    let mut recolored = d.clone();
    recolored.settings.theme_colors[4] = Rgb(1, 2, 3);
    let theme = package_part(&kition_doc_docx::write(&recolored).unwrap(), "word/theme/theme1.xml").unwrap();
    assert!(theme.contains(r#"<a:accent1><a:srgbClr val="010203"/></a:accent1>"#), "{theme}");

    // A theme part relating to other parts is not carried.
    let theme_rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/></Relationships>"#;
    let bytes = docx(
        "<w:p><w:r><w:t>x</w:t></w:r></w:p>",
        &[("rId9", "theme", "theme/theme1.xml")],
        &[("word/theme/theme1.xml", THEME_XML), ("word/theme/_rels/theme1.xml.rels", theme_rels)],
    );
    let theme = package_part(&kition_doc_docx::write(&kition_doc_docx::read(&bytes).unwrap()).unwrap(), "word/theme/theme1.xml").unwrap();
    assert!(!theme.contains("Test Song") && theme.contains("Test Body"), "{theme}");
}

#[test]
fn a_written_theme_reads_back_without_a_kept_copy() {
    // No theme part: the one written from the settings is not kept as a copy when read back.
    let d = read_body("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    assert!(!d.passthrough.contains_key("word/theme/theme1.xml"));
    let again = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(again.passthrough, d.passthrough);
}

// ---- legacy VML drawings kept as VML ----

#[test]
fn a_vml_picture_is_written_back_as_vml() {
    let png: &[u8] = b"\x89PNG\r\n\x1a\nstand-in";
    let pict = r##"<w:pict><v:shapetype id="_x0000_t75" coordsize="21600,21600" o:spt="75"/><v:shape id="Pic1" type="#_x0000_t75" style="width:72pt;height:36pt;mso-wrap-style:square" o:preferrelative="t"><v:imagedata r:id="rIdImg" o:title="logo"/><o:lock v:ext="edit" aspectratio="t"/></v:shape></w:pict>"##;
    let body = format!(r#"<w:p><w:r><w:rPr><w:b/></w:rPr>{pict}</w:r></w:p>"#);
    let types =
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="png" ContentType="image/png"/></Types>"#;
    let doc = format!(r#"<w:document {W_NS}><w:body>{body}</w:body></w:document>"#);
    let r = rels(&[("rIdImg", "image", "media/pic.png")]);
    let bytes = zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", r.as_bytes()),
        ("word/media/pic.png", png),
    ]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    let Some(InlineObject::Image { media, w, ole: Some(_), .. }) = paras(&d)[0].objects.first() else { panic!("{:?}", paras(&d)[0].objects) };
    assert_eq!((media.as_str(), *w), ("pic.png", 72.0));

    let out = kition_doc_docx::write(&d).unwrap();
    let files = unzip(&out);
    let body = text(&files, "word/document.xml");
    assert!(!body.contains("<w:drawing>"), "{body}");
    assert_eq!(attr_values(&body, "v:shape", "style"), ["width:72pt;height:36pt;mso-wrap-style:square"], "an unchanged style stays as it was");
    assert!(body.contains(r#"o:preferrelative="t""#) && body.contains("<o:lock ") && body.contains("<v:shapetype "), "{body}");
    let [id] = attr_values(&body, "v:imagedata", "r:id").try_into().unwrap();
    let (kind, target) = rel(&text(&files, "word/_rels/document.xml.rels"), &id);
    assert!(kind.ends_with("/image"), "{kind}");
    assert_eq!(files.get(&part_at("word", &target)).map(Vec::as_slice), Some(png));
    // The picture is written once, as the media file.
    assert_eq!(files.values().filter(|b| b.as_slice() == png).count(), 1, "{:?}", files.keys());

    // Read back: the same object, still kept; resized: its style follows.
    let back = kition_doc_docx::read(&out).unwrap();
    let plain = |d: &Document| -> Vec<InlineObject> {
        paras(d)[0]
            .objects
            .iter()
            .map(|o| match o.clone() {
                InlineObject::Image { ole, media, w, h, alt, float, crop, adjust, geom, edge, .. } => {
                    assert!(ole.is_some(), "still kept");
                    InlineObject::Image { ole: None, media, w, h, alt, float, crop, adjust, geom, edge, uid: 0 }
                }
                o => o,
            })
            .collect()
    };
    assert_eq!(plain(&back), plain(&d));
    let mut resized = back.clone();
    if let Some(kition_doc_doc::Block::Para(p)) = resized.body.first_mut().map(std::sync::Arc::make_mut)
        && let Some(InlineObject::Image { w, h, .. }) = p.objects.first_mut()
    {
        (*w, *h) = (144.0, 72.0);
    }
    let body = text(&unzip(&kition_doc_docx::write(&resized).unwrap()), "word/document.xml");
    assert_eq!(attr_values(&body, "v:shape", "style"), ["mso-wrap-style:square;width:144pt;height:72pt"]);
}

#[test]
fn vml_drawings_kition_doc_does_not_model_are_kept() {
    let line = r##"<w:pict><v:line id="L1" style="position:absolute;z-index:1" from="0,0" to="100pt,0" strokecolor="#0101FF" strokeweight="0.48pt"><v:stroke dashstyle="dash"/></v:line></w:pict>"##;
    let group = r#"<w:pict><v:group id="G1" style="width:441pt;height:20pt" coordsize="8821,417"><v:shape id="S1" style="position:absolute;left:0;top:288;width:720;height:10" path="m0,4l720,4e"/></v:group></w:pict>"#;
    // A fill picture would need its part, which is not carried: left out as before.
    let filled = r#"<w:pict><v:rect id="R1" style="width:10pt;height:10pt"><v:fill r:id="rIdFill" type="frame"/></v:rect></w:pict>"#;
    let body = format!(
        r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:rPr><w:i/></w:rPr>{line}</w:r><w:r><w:t>b</w:t></w:r><w:r>{group}</w:r><w:r>{filled}</w:r></w:p>"#
    );
    let d = read_body(&body);
    let p = paras(&d)[0];
    assert_eq!(p.objects.len(), 2, "{:?}", p.objects);
    assert!(p.objects.iter().all(|o| matches!(o, InlineObject::Opaque { format, text, .. } if format == "docx" && text.is_empty())));
    let out = kition_doc_docx::write(&d).unwrap();
    let xml = package_part(&out, "word/document.xml").unwrap();
    assert!(xml.contains(r#"<w:r><w:rPr><w:i/><w:iCs/></w:rPr><w:pict><v:line id="L1""#), "{xml}");
    assert!(xml.contains(r#"strokeweight="0.48pt"><v:stroke dashstyle="dash"/></v:line></w:pict></w:r>"#), "{xml}");
    assert!(xml.contains(r#"<v:group id="G1""#) && !xml.contains("R1"), "{xml}");
    let again = kition_doc_docx::read(&out).unwrap();
    assert_eq!(again.body, d.body);
}

#[test]
fn a_vml_text_box_with_an_empty_picture_is_read() {
    // Writers give text boxes an empty `v:imagedata`; it names no picture, so the box is a box.
    let body = r##"<w:p><w:r><w:pict><v:shape id="T1" type="#_x0000_t202" style="position:absolute;margin-left:1pt;margin-top:2pt;width:100pt;height:40pt;z-index:2" filled="false" fillcolor="#FF0000" stroked="f" strokecolor="#00FF00"><v:imagedata o:title=""/><v:textbox><w:txbxContent><w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:txbxContent></v:textbox></v:shape></w:pict></w:r>
<w:r><w:pict><v:shape id="T2" style="width:50pt;height:20pt" fillcolor="#FFFF00" strokecolor="#0000FF" strokeweight="2pt"><v:textbox><w:txbxContent><w:p><w:r><w:t>inside</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>"##;
    let d = read_body(body);
    let shapes: Vec<_> = paras(&d)[0]
        .objects
        .iter()
        .filter_map(|o| {
            if let InlineObject::Shape { kind, fill, stroke, stroke_width, story, .. } = o {
                Some((*kind, *fill, *stroke, *stroke_width, *story))
            } else {
                None
            }
        })
        .collect();
    let [(k1, f1, s1, w1, Some(t1)), (_, f2, s2, w2, _)] = shapes.as_slice() else { panic!("{:?}", paras(&d)[0].objects) };
    assert_eq!(*k1, ShapeKind::TextBox);
    // Turned off: no fill or pen whatever the colours say.
    assert_eq!((*f1, *s1, *w1), (None, None, 0.0));
    assert_eq!((*f2, *s2, *w2), (Some(Rgb(255, 255, 0)), Some(Rgb(0, 0, 255)), 2.0));
    // A box ending with a table ends with a paragraph, as Word (and saving) gives it one.
    let story = &d.parts.get(t1).unwrap().blocks;
    assert!(matches!((story.first().map(|b| &**b), story.last().map(|b| &**b)), (Some(Block::Table(_)), Some(Block::Para(_)))), "{story:?}");
    let again = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(again.body, d.body);
    assert_eq!(again.parts.get(t1).map(|p| &p.blocks), d.parts.get(t1).map(|p| &p.blocks));
}

#[test]
fn a_text_box_holding_a_picture_stays_a_text_box() {
    let png: &[u8] = b"\x89PNG\r\n\x1a\nstand-in";
    let inner = r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="127000" cy="127000"/><wp:docPr id="2" name="P"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rIdImg"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let body = format!(
        r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="1270000" cy="635000"/><wp:docPr id="1" name="Box"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:spPr><a:prstGeom prst="rect"/></wps:spPr><wps:txbx><w:txbxContent>{inner}</w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
    );
    let doc = format!(r#"<w:document {W_NS}><w:body>{body}</w:body></w:document>"#);
    let r = rels(&[("rIdImg", "image", "media/pic.png")]);
    let bytes = zip(&[
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", r.as_bytes()),
        ("word/media/pic.png", png),
    ]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    let Some(InlineObject::Shape { kind: ShapeKind::TextBox, story: Some(id), .. }) = paras(&d)[0].objects.first() else {
        panic!("{:?}", paras(&d)[0].objects)
    };
    let inner = d.parts.get(id).unwrap().blocks[0].as_para().unwrap();
    assert!(matches!(inner.objects.first(), Some(InlineObject::Image { .. })), "{:?}", inner.objects);
}

/// A package with comments and the parts Word extends them with: replies and done state
/// (`commentsExtended`), durable ids (`commentsIds`), UTC dates (`commentsExtensible`) and the
/// authors' identities (`people`).
fn commented_docx() -> Vec<u8> {
    let w15 = r#"xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml""#;
    let w14 = r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#;
    let comments = format!(
        r#"<w:comments {W_NS} {w14}><w:comment w:id="0" w:author="Ann" w:initials="" w:date="2026-03-01T09:00:00Z"><w:p w14:paraId="1A2B3C4D"><w:r><w:annotationRef/></w:r><w:r><w:t>Check this</w:t></w:r></w:p></w:comment><w:comment w:id="1" w:author="Ben" w:initials="B" w:date="2026-03-01T10:00:00Z"><w:p w14:paraId="0000AAA1"><w:r><w:t>First</w:t></w:r></w:p><w:p w14:paraId="0000BBB2"><w:r><w:t>Done</w:t></w:r></w:p></w:comment></w:comments>"#
    );
    let ext = format!(
        r#"<w15:commentsEx {w15}><w15:commentEx w15:paraId="1A2B3C4D" w15:done="1"/><w15:commentEx w15:paraId="0000BBB2" w15:paraIdParent="1A2B3C4D" w15:done="0"/></w15:commentsEx>"#
    );
    let ids = r#"<w16cid:commentsIds xmlns:w16cid="http://schemas.microsoft.com/office/word/2016/wordml/cid"><w16cid:commentId w16cid:paraId="1A2B3C4D" w16cid:durableId="5E6F7081"/><w16cid:commentId w16cid:paraId="0000BBB2" w16cid:durableId="12345678"/></w16cid:commentsIds>"#;
    let extensible = r#"<w16cex:commentsExtensible xmlns:w16cex="http://schemas.microsoft.com/office/word/2018/wordml/cex"><w16cex:commentExtensible w16cex:durableId="5E6F7081" w16cex:dateUtc="2026-03-01T01:00:00Z"/><w16cex:commentExtensible w16cex:durableId="12345678" w16cex:dateUtc="2026-03-01T02:00:00Z"/></w16cex:commentsExtensible>"#;
    let people = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w15:people {w15}><w15:person w15:author="Ann"><w15:presenceInfo w15:providerId="None" w15:userId="Ann"/></w15:person></w15:people>"#
    );
    let body = r#"<w:p><w:commentRangeStart w:id="0"/><w:r><w:t>text</w:t></w:r><w:commentRangeEnd w:id="0"/><w:r><w:commentReference w:id="0"/></w:r><w:r><w:commentReference w:id="1"/></w:r></w:p>"#;
    let doc = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W_NS}><w:body>{body}</w:body></w:document>"#);
    let r = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/><Relationship Id="rId2" Type="http://schemas.microsoft.com/office/2011/relationships/commentsExtended" Target="commentsExtended.xml"/><Relationship Id="rId3" Type="http://schemas.microsoft.com/office/2016/09/relationships/commentsIds" Target="commentsIds.xml"/><Relationship Id="rId4" Type="http://schemas.microsoft.com/office/2018/08/relationships/commentsExtensible" Target="commentsExtensible.xml"/><Relationship Id="rId5" Type="http://schemas.microsoft.com/office/2011/relationships/people" Target="people.xml"/></Relationships>"#;
    zip(&[
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", r.as_bytes()),
        ("word/comments.xml", comments.as_bytes()),
        ("word/commentsExtended.xml", ext.as_bytes()),
        ("word/commentsIds.xml", ids.as_bytes()),
        ("word/commentsExtensible.xml", extensible.as_bytes()),
        ("word/people.xml", people.as_bytes()),
    ])
}

/// `(paraId, durableId)` of each `w16cid:commentId`, and `(durableId, dateUtc)` of each
/// `w16cex:commentExtensible`, from a written package.
fn comment_ids(bytes: &[u8]) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let attr = |s: &str, name: &str| s.split(&format!("{name}=\"")).nth(1).and_then(|r| r.split('"').next()).unwrap_or("").to_string();
    let ids = package_part(bytes, "word/commentsIds.xml").unwrap();
    let ext = package_part(bytes, "word/commentsExtensible.xml").unwrap();
    let a = ids.split("<w16cid:commentId ").skip(1).map(|e| (attr(e, "w16cid:paraId"), attr(e, "w16cid:durableId"))).collect();
    let b = ext.split("<w16cex:commentExtensible ").skip(1).map(|e| (attr(e, "w16cex:durableId"), attr(e, "w16cex:dateUtc"))).collect();
    (a, b)
}

#[test]
fn comment_threads_ids_dates_and_people_are_kept() {
    let bytes = commented_docx();
    let d = kition_doc_docx::read(&bytes).unwrap();
    let (a, b) = (&d.comments[&0], &d.comments[&1]);
    assert!(a.resolved && !b.resolved && b.parent == Some(0));
    assert_eq!((a.para_id.as_str(), a.durable_id.as_str(), a.date_utc.as_str()), ("1A2B3C4D", "5E6F7081", "2026-03-01T01:00:00Z"));
    assert_eq!((b.para_id.as_str(), b.durable_id.as_str(), b.date_utc.as_str()), ("0000BBB2", "12345678", "2026-03-01T02:00:00Z"));

    let written = kition_doc_docx::write(&d).unwrap();
    let comments = package_part(&written, "word/comments.xml").unwrap();
    assert!(comments.contains(r#"w14:paraId="1A2B3C4D""#) && comments.contains(r#"w14:paraId="0000BBB2""#), "{comments}");
    assert_eq!(comments.matches("<w:annotationRef/>").count(), 2, "each comment starts with its mark");
    assert!(comments.contains(r#"w:author="Ann" w:date="2026-03-01T09:00:00Z" w:initials="""#), "{comments}");
    let ext = package_part(&written, "word/commentsExtended.xml").unwrap();
    assert!(ext.contains(r#"w15:paraId="0000BBB2" w15:paraIdParent="1A2B3C4D" w15:done="0""#), "{ext}");
    let (ids, dates) = comment_ids(&written);
    assert_eq!(ids, [("1A2B3C4D".into(), "5E6F7081".into()), ("0000BBB2".into(), "12345678".into())]);
    assert_eq!(dates, [("5E6F7081".into(), "2026-03-01T01:00:00Z".into()), ("12345678".into(), "2026-03-01T02:00:00Z".into())]);
    let people = String::from_utf8(zip_part(&bytes, "word/people.xml")).unwrap();
    assert_eq!(package_part(&written, "word/people.xml").as_deref(), Some(people.as_str()));
    let rels = package_part(&written, "word/_rels/document.xml.rels").unwrap();
    for t in ["2011/relationships/people", "2016/09/relationships/commentsIds", "2018/08/relationships/commentsExtensible"] {
        assert!(rels.contains(t), "{rels}");
    }
    let types = package_part(&written, "[Content_Types].xml").unwrap();
    for t in ["people+xml", "commentsIds+xml", "commentsExtensible+xml"] {
        assert!(types.contains(t), "{types}");
    }
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(again.comments, d.comments);
}

#[test]
fn comment_ids_stay_valid_after_edits() {
    let mut d = kition_doc_docx::read(&commented_docx()).unwrap();
    // A reply made in Kition (no ids), and a copy that claims the first comment's ids.
    let part = d.add_part(kition_doc_doc::PartKind::Comment, vec![kition_doc_doc::para_block(Paragraph::with_text("Reply", Default::default()))]);
    d.comments.insert(2, kition_doc_doc::Comment { author: "Cy".into(), parent: Some(0), part, ..Default::default() });
    let mut dup = d.comments[&0].clone();
    dup.part = d.add_part(kition_doc_doc::PartKind::Comment, vec![kition_doc_doc::para_block(Paragraph::with_text("Copy", Default::default()))]);
    d.comments.insert(3, dup);
    // The second comment is deleted.
    d.comments.remove(&1);

    let written = kition_doc_docx::write(&d).unwrap();
    let comments = package_part(&written, "word/comments.xml").unwrap();
    let para_ids: Vec<&str> = comments.split(r#"w14:paraId=""#).skip(1).filter_map(|s| s.split('"').next()).collect();
    assert_eq!(para_ids.len(), 3);
    let (ids, dates) = comment_ids(&written);
    assert_eq!(ids.len(), 3, "every comment has a durable id");
    let mut seen_para: Vec<&str> = ids.iter().map(|(p, _)| p.as_str()).collect();
    let mut seen_durable: Vec<&str> = ids.iter().map(|(_, d)| d.as_str()).collect();
    seen_para.sort();
    seen_para.dedup();
    seen_durable.sort();
    seen_durable.dedup();
    assert_eq!((seen_para.len(), seen_durable.len()), (3, 3), "unique: {ids:?}");
    for (p, dur) in &ids {
        assert!(para_ids.contains(&p.as_str()), "{p} names a comment paragraph");
        assert!(u32::from_str_radix(dur, 16).unwrap() < 0x8000_0000);
        assert!(dates.iter().any(|(x, _)| x == dur));
    }
    assert!(ids.contains(&("1A2B3C4D".into(), "5E6F7081".into())), "the original keeps its ids");
    assert!(!ids.iter().any(|(_, d)| d == "12345678"), "the deleted comment's id is gone");
    let ext = package_part(&written, "word/commentsExtended.xml").unwrap();
    assert!(ext.contains(r#"w15:paraIdParent="1A2B3C4D""#), "{ext}");
    assert!(!ext.contains("0000BBB2"), "{ext}");

    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(again.comments.len(), 3);
    assert_eq!(again.comments.values().filter(|c| c.parent.is_some()).count(), 1);
    assert!(again.comments.values().all(|c| !c.durable_id.is_empty() && !c.para_id.is_empty()));
}

/// A part's bytes from a package.
fn zip_part(bytes: &[u8], name: &str) -> Vec<u8> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = Vec::new();
    zip.by_name(name).unwrap().read_to_end(&mut out).unwrap();
    out
}

/// The kept VML drawing of a document's first text box.
fn first_text_box(d: &Document) -> (u32, Option<std::sync::Arc<kition_doc_doc::graphic::Embedded>>) {
    paras(d)
        .iter()
        .flat_map(|p| &p.objects)
        .find_map(|o| match o {
            InlineObject::Shape { kind: ShapeKind::TextBox, story: Some(id), vml, .. } => Some((*id, vml.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text box"))
}

#[test]
fn a_vml_word_art_watermark_is_read_as_fitted_text_and_written_back() {
    // An art text watermark (VML text path, shape type 136): its string in its font, stretched to
    // fill the shape, half see-through grey behind the text.
    let body = r##"<w:p><w:r><w:pict><v:shapetype id="_x0000_t136" coordsize="21600,21600" o:spt="136" adj="10800" path="m@7,l@8,m@5,21600l@6,21600e"><v:path textpathok="t" o:connecttype="custom"/><v:textpath on="t" fitshape="t"/></v:shapetype><v:shape id="W1" o:spid="_x0000_s4097" o:spt="136" type="#_x0000_t136" style="position:absolute;left:0pt;height:200pt;width:400pt;mso-position-horizontal:center;mso-position-horizontal-relative:margin;mso-position-vertical:center;mso-position-vertical-relative:margin;rotation:20643840f;z-index:-251649024" fillcolor="#C0C0C0" filled="t" stroked="f" coordsize="21600,21600"><v:fill on="t" opacity="32768f" focussize="0,0"/><v:stroke on="f"/><v:imagedata o:title=""/><o:lock v:ext="edit" aspectratio="t"/><v:textpath on="t" fitshape="t" fitpath="t" trim="t" xscale="f" string="SAMPLE" style="font-family:&quot;SimSun&quot;;font-size:1pt;font-weight:bold"/></v:shape></w:pict></w:r></w:p>"##;
    let d = read_body(body);
    let (story, vml) = first_text_box(&d);
    assert!(vml.is_some(), "the drawing is kept");
    let shape = paras(&d)[0].objects.iter().find(|o| matches!(o, InlineObject::Shape { .. })).unwrap();
    let InlineObject::Shape { w, h, warp, float, fill, stroke, .. } = shape else { unreachable!() };
    assert_eq!((*w, *h, *fill, *stroke), (400.0, 200.0, None, None));
    assert_eq!(warp.as_ref().map(|g| g.name.as_str()), Some("textPlain"));
    assert_eq!(float.wrap, Wrap::BehindText);
    assert!((float.rot - 315.0).abs() < 0.01, "{}", float.rot);
    let blocks = &d.parts.get(&story).unwrap().blocks;
    let text = blocks[0].as_para().unwrap();
    assert_eq!(text.text, "SAMPLE");
    let props = &text.runs[0].props;
    // #C0C0C0 at half opacity over the white page.
    assert_eq!(props.color, Some(TextColor::Rgb(Rgb(224, 224, 224))));
    assert_eq!((props.font.as_deref(), props.bold), (Some("SimSun"), Some(true)));

    // Saved: the same VML, its string the story's text; no DrawingML.
    let written = kition_doc_docx::write(&d).unwrap();
    let doc = package_part(&written, "word/document.xml").unwrap();
    assert!(!doc.contains("<wps:wsp") && !doc.contains("<w:drawing"), "{doc}");
    assert!(doc.contains(r#"string="SAMPLE""#) && doc.contains(r#"opacity="32768f""#) && doc.contains("rotation:20643840f"), "{doc}");
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(again.body, d.body, "reads back the same, drawing included");

    // Its text edited: still the art text, with the new string.
    let mut edited = d.clone();
    edited.parts.get_mut(&story).unwrap().blocks = vec![kition_doc_doc::para_block(Paragraph::with_text("DRAFT & COPY", props.clone()))];
    let doc = package_part(&kition_doc_docx::write(&edited).unwrap(), "word/document.xml").unwrap();
    assert!(doc.contains(r#"string="DRAFT &amp; COPY""#) && !doc.contains("SAMPLE"), "{doc}");
}

#[test]
fn a_vml_text_box_takes_its_line_from_its_stroke_element() {
    // `stroked="t"` with the colour and weight on `v:stroke` (as WPS writes them): a 1 pt
    // dark-blue line; stroked with no colour anywhere: VML's default black 0.75 pt line;
    // `stroked="f"`: none.
    let shape = |attrs: &str, stroke: &str| {
        format!(
            r##"<w:p><w:r><w:pict><v:rect style="width:150pt;height:52pt" fillcolor="#D9E2F3" filled="t" {attrs}>{stroke}<v:textbox><w:txbxContent><w:p><w:r><w:t>box</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:r></w:p>"##
        )
    };
    let line = |body: String| match &paras(&read_body(&body))[0].objects[0] {
        InlineObject::Shape { stroke, stroke_width, .. } => (*stroke, *stroke_width),
        other => panic!("{other:?}"),
    };
    assert_eq!(line(shape(r#"stroked="t""#, r##"<v:stroke weight="1pt" color="#1F4E79" joinstyle="miter"/>"##)), (Some(Rgb(0x1F, 0x4E, 0x79)), 1.0));
    assert_eq!(line(shape(r#"stroked="t""#, "")), (Some(Rgb::BLACK), 0.75));
    assert_eq!(line(shape(r#"stroked="f""#, r##"<v:stroke weight="1pt" color="#1F4E79"/>"##)), (None, 0.0));
}

#[test]
fn a_vml_text_box_is_written_back_as_vml_around_its_text() {
    let body = r##"<w:p><w:r><w:pict><v:shapetype id="_x0000_t202" coordsize="21600,21600" o:spt="202" path="m,l,21600r21600,l21600,xe"><v:stroke joinstyle="miter"/><v:path gradientshapeok="t" o:connecttype="rect"/></v:shapetype><v:shape id="T1" o:spid="_x0000_s1026" type="#_x0000_t202" style="position:absolute;margin-left:1pt;margin-top:2pt;width:100pt;height:40pt;z-index:2" o:gfxdata="UEsDBA==" fillcolor="#FFFF00" filled="t" strokecolor="#0000FF" strokeweight="2pt"><v:fill on="t" focussize="0,0"/><v:stroke weight="2pt" color="#0000FF"/><o:lock v:ext="edit"/><v:textbox inset="2pt,1pt,2pt,1pt"><w:txbxContent><w:p><w:r><w:t>inside</w:t></w:r></w:p></w:txbxContent></v:textbox><w10:wrap type="square"/></v:shape></w:pict></w:r></w:p>"##;
    let body = body.replace("<w:pict>", r#"<w:pict xmlns:w10="urn:schemas-microsoft-com:office:word">"#);
    let d = read_body(&body);
    let (story, vml) = first_text_box(&d);
    let shell = vml.unwrap_or_else(|| panic!("the drawing is kept"));
    assert!(!shell.xml.contains("inside"), "the text is the story's, not the drawing's: {}", shell.xml);

    // Saved unchanged: the same VML, the text inside it, no DrawingML.
    let written = kition_doc_docx::write(&d).unwrap();
    let doc = package_part(&written, "word/document.xml").unwrap();
    assert!(!doc.contains("<wps:wsp") && !doc.contains("<w:drawing"), "{doc}");
    for kept in [
        r#"<v:shapetype id="_x0000_t202""#,
        r#"o:gfxdata="UEsDBA==""#,
        r#"<v:fill on="t" focussize="0,0"/>"#,
        r#"<o:lock v:ext="edit"/>"#,
        r#"<v:textbox inset="2pt,1pt,2pt,1pt"><w:txbxContent><w:p"#,
        r#"<w10:wrap type="square"/>"#,
        "margin-left:1pt;margin-top:2pt;width:100pt;height:40pt;z-index:2",
    ] {
        assert!(doc.contains(kept), "{kept} in {doc}");
    }
    assert!(doc.contains(">inside<"), "{doc}");
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(again.body, d.body, "reads back the same, drawing included");
    assert_eq!(again.parts.get(&first_text_box(&again).0).map(|p| &p.blocks), d.parts.get(&story).map(|p| &p.blocks));

    // Its text edited: still VML, with the new text.
    let mut edited = d.clone();
    edited.parts.get_mut(&story).unwrap().blocks = vec![kition_doc_doc::para_block(Paragraph::with_text("changed", Default::default()))];
    let doc = package_part(&kition_doc_docx::write(&edited).unwrap(), "word/document.xml").unwrap();
    assert!(doc.contains("<v:textbox") && doc.contains(">changed<") && !doc.contains("inside"), "{doc}");

    // Moved: the style takes the new place, and Office's stale DrawingML copy goes.
    let mut moved = d.clone();
    for b in moved.body.iter_mut() {
        if let Block::Para(p) = std::sync::Arc::make_mut(b) {
            for o in p.objects.iter_mut() {
                if let InlineObject::Shape { float, w, .. } = o {
                    float.x = 30.0;
                    *w = 120.0;
                }
            }
        }
    }
    let doc = package_part(&kition_doc_docx::write(&moved).unwrap(), "word/document.xml").unwrap();
    assert!(doc.contains("<v:textbox") && doc.contains("margin-left:30pt") && doc.contains("width:120pt"), "{doc}");
    assert!(!doc.contains("o:gfxdata"), "{doc}");

    // Filled differently in Kition: written as DrawingML, which can say so.
    let mut refilled = d.clone();
    for b in refilled.body.iter_mut() {
        if let Block::Para(p) = std::sync::Arc::make_mut(b) {
            for o in p.objects.iter_mut() {
                if let InlineObject::Shape { fill, .. } = o {
                    *fill = Some(Rgb(1, 2, 3));
                }
            }
        }
    }
    let doc = package_part(&kition_doc_docx::write(&refilled).unwrap(), "word/document.xml").unwrap();
    assert!(doc.contains("<wps:wsp") && doc.contains(r#"<a:srgbClr val="010203"/>"#) && !doc.contains("<v:textbox"), "{doc}");
    assert!(doc.contains(">inside<"), "{doc}");
}

#[test]
fn text_effects_of_runs_and_styles_are_kept_and_only_w14() {
    let styles = format!(
        r#"<w:styles {W_NS} xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"><w:style w:type="character" w:styleId="Glowing"><w:name w:val="Glowing"/><w:rPr><w:b/><w14:glow w14:rad="63500"><w14:schemeClr w14:val="accent1"><w14:alpha w14:val="60000"/></w14:schemeClr></w14:glow></w:rPr></w:style></w:styles>"#
    );
    let body = r#"<w:p><w:r><w:rPr><w:sz w:val="28"/><w14:textFill><w14:solidFill><w14:srgbClr w14:val="FF0000"/></w14:solidFill></w14:textFill></w:rPr><w:t>red</w:t></w:r></w:p>"#;
    let doc = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W_NS} xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"><w:body>{body}</w:body></w:document>"#
    );
    let r = rels(&[("rId1", "styles", "styles.xml")]);
    let bytes = zip(&[
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", r.as_bytes()),
        ("word/styles.xml", styles.as_bytes()),
    ]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    let run = &paras(&d)[0].runs[0].props;
    assert_eq!(run.text_effects.as_deref(), Some(r#"<w14:textFill><w14:solidFill><w14:srgbClr w14:val="FF0000"/></w14:solidFill></w14:textFill>"#));
    assert_eq!(run.size, Some(14.0));
    let written = kition_doc_docx::write(&d).unwrap();
    let doc = package_part(&written, "word/document.xml").unwrap();
    assert!(doc.contains(r#"<w:szCs w:val="28"/><w14:textFill><w14:solidFill><w14:srgbClr w14:val="FF0000"/>"#), "{doc}");
    let styles = package_part(&written, "word/styles.xml").unwrap();
    assert!(styles.contains(r#"<w14:glow w14:rad="63500"><w14:schemeClr w14:val="accent1"><w14:alpha w14:val="60000"/>"#), "{styles}");
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(paras(&again)[0].runs[0].props, *run);
    assert_eq!(again.styles, d.styles);

    // Whatever a replica says, only well-formed w14 elements are written.
    let mut odd = d.clone();
    if let Block::Para(p) = std::sync::Arc::make_mut(&mut odd.body[0]) {
        p.runs[0].props.text_effects = Some(r#"<w:b/><w14:ligatures w14:val="standard"/></w:rPr><evil>"#.into());
    }
    let written = kition_doc_docx::write(&odd).unwrap();
    let doc = package_part(&written, "word/document.xml").unwrap();
    assert!(!doc.contains("evil") && !doc.contains("<w:b/>"), "only w14 elements are written: {doc}");
    assert_eq!(paras(&kition_doc_docx::read(&written).unwrap())[0].text, "red", "the run is intact");
    if let Block::Para(p) = std::sync::Arc::make_mut(&mut odd.body[0]) {
        p.runs[0].props.text_effects = Some(r#"<w:b/><w14:ligatures w14:val="standard"/>"#.into());
    }
    let doc = package_part(&kition_doc_docx::write(&odd).unwrap(), "word/document.xml").unwrap();
    assert!(doc.contains(r#"<w14:ligatures w14:val="standard"/>"#) && !doc.contains("<w:b/>"), "{doc}");
}

#[test]
fn settings_kition_doc_does_not_model_are_kept_in_schema_order() {
    // Out of schema order, as some writers save it.
    let settings = format!(
        r#"<w:settings {W_NS} xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"><w:zoom w:percent="120"/><w:docVars><w:docVar w:name="Client" w:val="Synthetic"/></w:docVars><w:writeProtection w:recommended="1"/><w:defaultTabStop w:val="420"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/><w:compatSetting w:name="doNotFlipMirrorIndents" w:uri="http://schemas.microsoft.com/office/word" w:val="1"/></w:compat><w14:defaultImageDpi w14:val="300"/><w:clrSchemeMapping w:bg1="light1" w:t1="dark1"/></w:settings>"#
    );
    let bytes = docx("<w:p><w:r><w:t>x</w:t></w:r></w:p>", &[("rId1", "settings", "settings.xml")], &[("word/settings.xml", &settings)]);
    let d = kition_doc_docx::read(&bytes).unwrap();
    let written = kition_doc_docx::write(&d).unwrap();
    let out = package_part(&written, "word/settings.xml").unwrap();
    let order = ["<w:writeProtection", "<w:zoom", "<w:defaultTabStop", "<w:compat>", "<w:docVars>", "<w:clrSchemeMapping", "<w14:defaultImageDpi"];
    let at: Vec<usize> = order.iter().map(|t| out.find(t).unwrap_or_else(|| panic!("{t} in {out}"))).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "schema order: {out}");
    assert!(out.contains(r#"w:name="doNotFlipMirrorIndents""#) && out.contains(r#"<w:docVar w:name="Client" w:val="Synthetic"/>"#), "{out}");
    assert!(out.contains(r#"<w:defaultTabStop w:val="420"/>"#), "the model's own: {out}");
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(again.passthrough, d.passthrough, "saving again keeps the same");
}

/// A shape as Word inserts it gives its fill, outline and text colour only through its style's
/// theme references (`wps:style`); a shape's own fill and outline, and a run's colour, win.
#[test]
fn a_shape_takes_what_it_does_not_say_from_its_style() {
    let body = r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="1270000" cy="635000"/><wp:docPr id="1" name="Rectangle 1"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:style><a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="15000"/></a:schemeClr></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></wps:style><wps:txbx><w:txbxContent><w:p><w:r><w:t>Hi</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let d = read_body(body);
    let p = paras(&d);
    let InlineObject::Shape { fill, stroke, stroke_width, story, .. } = &p[0].objects[0] else { panic!("{:?}", p[0].objects) };
    let accent1 = kition_doc_doc::THEME_COLORS[4];
    assert_eq!(*fill, Some(accent1));
    assert!(stroke.is_some_and(|c| c != accent1), "the outline is accent1 shaded: {stroke:?}");
    assert_eq!(*stroke_width, 1.0);
    let part = &d.parts[&story.unwrap()];
    let text = part.blocks[0].as_para().unwrap();
    assert_eq!(text.runs[0].props.color, Some(kition_doc_doc::props::TextColor::Rgb(kition_doc_doc::THEME_COLORS[1])));

    let own = body
        .replace(
            "<a:avLst/></a:prstGeom></wps:spPr>",
            r#"<a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val="accent2"/></a:solidFill><a:ln><a:noFill/></a:ln></wps:spPr>"#,
        )
        .replace("<w:r><w:t>Hi</w:t>", r#"<w:r><w:rPr><w:color w:val="FF0000"/></w:rPr><w:t>Hi</w:t>"#);
    let d = read_body(&own);
    let p = paras(&d);
    let InlineObject::Shape { fill, stroke, story, .. } = &p[0].objects[0] else { panic!() };
    assert_eq!((*fill, *stroke), (Some(kition_doc_doc::THEME_COLORS[5]), None));
    let text = d.parts[&story.unwrap()].blocks[0].as_para().unwrap().runs[0].props.color;
    assert_eq!(text, Some(kition_doc_doc::props::TextColor::Rgb(kition_doc_doc::props::Rgb::parse("FF0000").unwrap())));
}

/// A shape's text frame (`wps:bodyPr` anchor and margins) is read, drawn and written back.
#[test]
fn a_shape_keeps_its_text_frame() {
    use kition_doc_doc::graphic::{TextAnchor, TextFrame};
    let shape = |body_pr: &str| {
        format!(
            r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="1270000" cy="635000"/><wp:docPr id="1" name="S"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr/><wps:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Hi</w:t></w:r></w:p></w:txbxContent></wps:txbx>{body_pr}</wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
        )
    };
    let frame_of = |d: &Document| match &paras(d)[0].objects[0] {
        InlineObject::Shape { body, .. } => *body,
        other => panic!("{other:?}"),
    };
    assert_eq!(frame_of(&read_body(&shape("<wps:bodyPr/>"))), None);
    let d = read_body(&shape(r#"<wps:bodyPr rot="0" lIns="0" tIns="45720" rIns="91440" bIns="45720" anchor="ctr"/>"#));
    let want = TextFrame { anchor: TextAnchor::Middle, insets: [0.0, 3.6, 7.2, 3.6], fit: false, no_wrap: false, fit_height: false };
    assert_eq!(frame_of(&d), Some(want));
    let back = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(frame_of(&back), Some(want));
}

#[test]
fn a_text_box_that_fits_its_text_keeps_saying_so() {
    use kition_doc_doc::graphic::{TextAnchor, TextFrame};
    // `wrap="none"` with `a:spAutoFit`: the box takes its text's size; kept through saving.
    let body = r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="1828800" cy="1828800"/><wp:docPr id="1" name="S"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>11</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr wrap="none" lIns="0" tIns="0" rIns="0" bIns="0" anchor="t"><a:spAutoFit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let frame_of = |d: &Document| match &paras(d)[0].objects[0] {
        InlineObject::Shape { body, .. } => *body,
        other => panic!("{other:?}"),
    };
    let d = read_body(body);
    let want = TextFrame { anchor: TextAnchor::Top, insets: [0.0; 4], fit: true, no_wrap: false, fit_height: false };
    assert_eq!(frame_of(&d), Some(want));
    let back = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(frame_of(&back), Some(want));
}

#[test]
fn a_text_box_that_grows_to_its_wrapped_text_keeps_saying_so() {
    use kition_doc_doc::graphic::{TextAnchor, TextFrame};
    // `a:spAutoFit` with the text wrapped: the box keeps its width and takes its text's height;
    // kept through saving.
    let body = r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="1270000" cy="508000"/><wp:docPr id="1" name="S"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Rich text</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr wrap="square" lIns="0" tIns="0" rIns="0" bIns="0" anchor="t"><a:spAutoFit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let frame_of = |d: &Document| match &paras(d)[0].objects[0] {
        InlineObject::Shape { body, .. } => *body,
        other => panic!("{other:?}"),
    };
    let d = read_body(body);
    let want = TextFrame { anchor: TextAnchor::Top, insets: [0.0; 4], fit: false, no_wrap: false, fit_height: true };
    assert_eq!(frame_of(&d), Some(want));
    let back = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(frame_of(&back), Some(want));
}

#[test]
fn a_text_box_that_does_not_wrap_keeps_saying_so() {
    use kition_doc_doc::graphic::{TextAnchor, TextFrame};
    // `wrap="none"` without `a:spAutoFit`: the text is not wrapped and the box keeps its size;
    // kept through saving.
    let body = r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="381000" cy="381000"/><wp:docPr id="1" name="S"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>LEOPARD</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr wrap="none" lIns="0" tIns="0" rIns="0" bIns="0" anchor="ctr"><a:noAutofit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let frame_of = |d: &Document| match &paras(d)[0].objects[0] {
        InlineObject::Shape { body, .. } => *body,
        other => panic!("{other:?}"),
    };
    let d = read_body(body);
    let want = TextFrame { anchor: TextAnchor::Middle, insets: [0.0; 4], fit: false, no_wrap: true, fit_height: false };
    assert_eq!(frame_of(&d), Some(want));
    let back = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(frame_of(&back), Some(want));
}

#[test]
fn a_visible_text_outline_is_read_for_drawing() {
    use kition_doc_doc::props::{TextOutline, TextPaint, ThemeSlot};
    // `w14:textOutline` with a visible colour and its `w14:textFill`: drawn outlined (as WPS
    // prints it); a fully transparent outline is no outline.
    let run = |outline: &str, fill: &str| {
        format!(
            r#"<w:p><w:r xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"><w:rPr><w14:textOutline w14:w="12700" w14:cap="flat" w14:cmpd="sng" w14:algn="ctr">{outline}<w14:prstDash w14:val="solid"/><w14:round/></w14:textOutline>{fill}</w:rPr><w:t>Outlined</w:t></w:r></w:p>"#
        )
    };
    let outline_of = |d: &Document| paras(d)[0].runs[0].props.text_outline;
    let red = r#"<w14:solidFill><w14:srgbClr w14:val="FF0000"/></w14:solidFill>"#;
    let green = r#"<w14:textFill><w14:solidFill><w14:srgbClr w14:val="00FF00"/></w14:solidFill></w14:textFill>"#;
    let d = read_body(&run(red, green));
    assert_eq!(
        outline_of(&d),
        Some(TextOutline { stroke: TextPaint::Rgb(Rgb(255, 0, 0)), width: 1.0, fill: Some(Some(TextPaint::Rgb(Rgb(0, 255, 0)))) })
    );
    let accent = r#"<w14:solidFill><w14:schemeClr w14:val="accent1"/></w14:solidFill>"#;
    let hollow = r#"<w14:textFill><w14:noFill/></w14:textFill>"#;
    assert_eq!(
        outline_of(&read_body(&run(accent, hollow))),
        Some(TextOutline { stroke: TextPaint::Theme(ThemeSlot::Accent1), width: 1.0, fill: Some(None) })
    );
    assert_eq!(outline_of(&read_body(&run(accent, ""))).map(|o| o.fill), Some(None));
    let clear = r#"<w14:solidFill><w14:schemeClr w14:val="accent1"><w14:alpha w14:val="0"/></w14:schemeClr></w14:solidFill>"#;
    assert_eq!(outline_of(&read_body(&run(clear, green))), None);
    assert_eq!(outline_of(&read_body(&run("<w14:noFill/>", green))), None);
    // Saved as written, and read back the same.
    let back = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(outline_of(&back), outline_of(&d));
}

#[test]
fn a_blurred_text_effect_is_read_for_drawing() {
    // A blurred `w14:shadow`, a `w14:glow` with a size, or a `w14:reflection`: WPS prints the
    // letters as a picture. A sharp shadow or a glow of no size is no blur.
    let run = |effect: &str| {
        format!(
            r#"<w:p><w:r xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"><w:rPr>{effect}</w:rPr><w:t>Shadowed</w:t></w:r></w:p>"#
        )
    };
    let blur_of = |d: &Document| paras(d)[0].runs[0].props.text_blur;
    let shadow = |blur: u32| {
        format!(
            r#"<w14:shadow w14:blurRad="{blur}" w14:dist="19050" w14:dir="2700000" w14:sx="100000" w14:sy="100000" w14:kx="0" w14:ky="0" w14:algn="tl"><w14:srgbClr w14:val="000000"><w14:alpha w14:val="60000"/></w14:srgbClr></w14:shadow>"#
        )
    };
    let d = read_body(&run(&shadow(38100)));
    assert_eq!(blur_of(&d), Some(true));
    assert_eq!(blur_of(&read_body(&run(&shadow(0)))), None);
    let glow = |rad: u32| format!(r#"<w14:glow w14:rad="{rad}"><w14:srgbClr w14:val="FF0000"/></w14:glow>"#);
    assert_eq!(blur_of(&read_body(&run(&glow(63500)))), Some(true));
    assert_eq!(blur_of(&read_body(&run(&glow(0)))), None);
    let reflection = r#"<w14:reflection w14:blurRad="6350" w14:stA="52000" w14:stPos="0" w14:endA="300" w14:endPos="35000" w14:dist="0" w14:dir="5400000" w14:fadeDir="5400000" w14:sx="100000" w14:sy="-100000" w14:kx="0" w14:ky="0" w14:algn="bl" w14:rotWithShape="0"/>"#;
    assert_eq!(blur_of(&read_body(&run(reflection))), Some(true));
    assert_eq!(blur_of(&read_body(&run(""))), None);
    // Saved as written, and read back the same.
    let back = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(blur_of(&back), Some(true));
}

#[test]
fn a_picture_cut_to_a_preset_outline_keeps_it() {
    use kition_doc_doc::graphic::PresetGeom;
    // `pic:spPr/a:prstGeom`: an ellipse, and a rounded rectangle with its corner size; a plain
    // rectangle is no outline. Written back on save.
    let png: &[u8] = b"\x89PNG\r\n\x1a\nnot really a png";
    let pic = |geom: &str| {
        format!(
            r#"<w:r><w:drawing><wp:inline><wp:extent cx="1270000" cy="635000"/><wp:docPr id="3" name="P"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rIdImg"/></pic:blipFill><pic:spPr>{geom}</pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
        )
    };
    let body = format!(
        "<w:p>{}{}{}</w:p>",
        pic(r#"<a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom>"#),
        pic(r#"<a:prstGeom prst="roundRect"><a:avLst><a:gd name="adj" fmla="val 30000"/></a:avLst></a:prstGeom>"#),
        pic(r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#)
    );
    let doc = format!(r#"<w:document {W_NS}><w:body>{body}</w:body></w:document>"#);
    let r = rels(&[("rIdImg", "image", "media/pic.png")]);
    let bytes = zip(&[
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", r.as_bytes()),
        ("word/media/pic.png", png),
    ]);
    let geoms = |d: &Document| -> Vec<Option<PresetGeom>> {
        paras(d)[0].objects.iter().filter_map(|o| if let InlineObject::Image { geom, .. } = o { Some(geom.clone()) } else { None }).collect()
    };
    let d = kition_doc_docx::read(&bytes).unwrap();
    let want = vec![
        Some(PresetGeom { name: "ellipse".into(), adj: Vec::new() }),
        Some(PresetGeom { name: "roundRect".into(), adj: vec![("adj".into(), 30000.0)] }),
        None,
    ];
    assert_eq!(geoms(&d), want);
    let back = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(geoms(&back), want);
}

#[test]
fn a_picture_keeps_its_line_and_effects() {
    use kition_doc_doc::effects::{Glow, Shadow, ShapeEffects};
    use kition_doc_doc::graphic::PictureEdge;
    // `pic:spPr` `a:ln` (a 2 pt red line) and `a:effectLst` (a blurred shadow and a glow): read,
    // and written back on save. A line with no fill is no line.
    let png: &[u8] = b"\x89PNG\r\n\x1a\nnot really a png";
    let pic = |sppr: &str| {
        format!(
            r#"<w:r><w:drawing><wp:inline><wp:extent cx="1270000" cy="635000"/><wp:docPr id="3" name="P"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rIdImg"/></pic:blipFill><pic:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{sppr}</pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
        )
    };
    let line = r#"<a:ln w="25400"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:ln>"#;
    let effects = r#"<a:effectLst><a:glow rad="63500"><a:srgbClr val="00FF00"><a:alpha val="40000"/></a:srgbClr></a:glow><a:outerShdw blurRad="50800" dist="38100" dir="2700000" algn="tl" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw></a:effectLst>"#;
    let body = format!("<w:p>{}{}</w:p>", pic(&format!("{line}{effects}")), pic(r#"<a:ln w="25400"><a:noFill/></a:ln>"#));
    let doc = format!(r#"<w:document {W_NS}><w:body>{body}</w:body></w:document>"#);
    let r = rels(&[("rIdImg", "image", "media/pic.png")]);
    let bytes = zip(&[
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", r.as_bytes()),
        ("word/media/pic.png", png),
    ]);
    let edges = |d: &Document| -> Vec<PictureEdge> {
        paras(d)[0].objects.iter().filter_map(|o| if let InlineObject::Image { edge, .. } = o { Some(*edge) } else { None }).collect()
    };
    let d = kition_doc_docx::read(&bytes).unwrap();
    let got = edges(&d);
    assert_eq!(got.len(), 2);
    assert_eq!((got[0].line, got[0].line_width), (Some(Rgb(255, 0, 0)), 2.0));
    let fx: ShapeEffects = got[0].effects;
    let shadow: Shadow = fx.shadow.expect("a shadow");
    assert!((shadow.blur - 4.0).abs() < 0.01 && (shadow.distance - 3.0).abs() < 0.01 && (shadow.angle - 45.0).abs() < 0.01, "{shadow:?}");
    let glow: Glow = fx.glow.expect("a glow");
    assert!(glow.color == Rgb(0, 255, 0) && (glow.size - 5.0).abs() < 0.01, "{glow:?}");
    assert!(got[1].is_empty(), "{:?}", got[1]);
    let back = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    assert_eq!(edges(&back), got);
}

#[test]
fn picture_colour_adjustments_are_read_and_saved() {
    use kition_doc_doc::graphic::ImageAdjust;
    // `a:blip` effects: washout (brightness +70 %, contrast -70 %), grey scale, black and white,
    // and opacity; drawn by the renderers, written back on save.
    let png: &[u8] = b"\x89PNG\r\n\x1a\nnot really a png";
    let pic = |effects: &str| {
        format!(
            r#"<w:r><w:drawing><wp:inline><wp:extent cx="1270000" cy="635000"/><wp:docPr id="3" name="P"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rIdImg">{effects}</a:blip></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
        )
    };
    let body = format!(
        "<w:p>{}{}{}{}</w:p>",
        pic(r#"<a:lum bright="70000" contrast="-70000"/>"#),
        pic("<a:grayscl/>"),
        pic(r#"<a:biLevel thresh="50000"/>"#),
        pic(r#"<a:alphaModFix amt="40000"/>"#)
    );
    let doc = format!(r#"<w:document {W_NS}><w:body>{body}</w:body></w:document>"#);
    let r = rels(&[("rIdImg", "image", "media/pic.png")]);
    let bytes = zip(&[
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", r.as_bytes()),
        ("word/media/pic.png", png),
    ]);
    let adjusts = |d: &Document| -> Vec<ImageAdjust> {
        paras(d)[0].objects.iter().filter_map(|o| if let InlineObject::Image { adjust, .. } = o { Some(*adjust) } else { None }).collect()
    };
    let d = kition_doc_docx::read(&bytes).unwrap();
    let want = vec![
        ImageAdjust { bright: 0.7, contrast: -0.7, ..Default::default() },
        ImageAdjust { gray: true, ..Default::default() },
        ImageAdjust { bilevel: Some(0.5), ..Default::default() },
        ImageAdjust { opacity: 0.4, ..Default::default() },
    ];
    assert_eq!(adjusts(&d), want);
    let saved = kition_doc_docx::write(&d).unwrap();
    let xml = package_part(&saved, "word/document.xml").unwrap();
    assert!(
        xml.contains(r#"<a:lum bright="70000" contrast="-70000"/>"#)
            && xml.contains("<a:grayscl/>")
            && xml.contains(r#"<a:biLevel thresh="50000"/>"#)
            && xml.contains(r#"<a:alphaModFix amt="40000"/>"#),
        "{xml}"
    );
    assert_eq!(adjusts(&kition_doc_docx::read(&saved).unwrap()), want);
}

#[test]
fn fields_nested_in_a_field_code_keep_their_code() {
    use kition_doc_doc::para::CodeField;
    // WPS's even-page header `第{ = { page } }页` writes the inner result as field code; Word
    // writes it as text. Either way the inner field is kept with its place in the code.
    let field = |inner_result: &str| {
        format!(
            r#"<w:p><w:r><w:t>第</w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> =</w:instrText></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> page </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{inner_result}<w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:instrText xml:space="preserve"> + 1 </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>7</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t>页</w:t></w:r></w:p>"#
        )
    };
    let cases = [
        (r#"<w:r><w:instrText xml:space="preserve">6</w:instrText></w:r>"#, "= + 1", CodeField { start: 1, len: 0, instr: "page".into() }),
        (r#"<w:r><w:t>6</w:t></w:r>"#, "=6 + 1", CodeField { start: 1, len: 1, instr: "page".into() }),
    ];
    for (inner, code, nested) in cases {
        let d = read_body(&field(inner));
        let Some(InlineObject::Field { instr, result, code_fields, .. }) = paras(&d)[0].objects.first().cloned() else {
            panic!("a field: {:?}", paras(&d)[0].objects)
        };
        assert_eq!((instr.as_str(), result.as_str(), code_fields.clone()), (code, "7", vec![nested.clone()]));
        let again = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
        assert_eq!(paras(&again)[0].objects, paras(&d)[0].objects, "saved and read again");
    }
}

#[test]
fn a_package_without_styles_takes_the_formats_paragraph_defaults() {
    use kition_doc_doc::props::LineSpacing;
    // No styles part: single line spacing and no space after, as WPS lays it out, not the
    // template spacing a new document gets.
    let d = read_body("<w:p><w:r><w:t>Plain</w:t></w:r></w:p>");
    assert_eq!(d.styles.default_para.line_spacing, Some(LineSpacing::Multiple(1.0)));
    assert_eq!(d.styles.default_para.space_after, Some(0.0));
}
