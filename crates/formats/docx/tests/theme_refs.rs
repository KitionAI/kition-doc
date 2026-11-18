//! Theme colour and font references and paragraph ids (written for these tests from the spec):
//! kept from the file, written back while they hold, re-resolved by a theme change.

use std::collections::HashSet;
use std::io::{Read, Write};

use kition_doc_doc::props::{CharProps, Rgb, TextColor, ThemeColor, ThemeSlot};
use kition_doc_doc::{Block, Document, Paragraph, para_block};

const W_NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="w14""#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

const DOC_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;

const STYLES: &str = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi" w:eastAsiaTheme="minorEastAsia" w:cstheme="minorBidi"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/>
 <w:rPr><w:rFonts w:asciiTheme="majorHAnsi" w:eastAsiaTheme="majorEastAsia" w:hAnsiTheme="majorHAnsi" w:cstheme="majorBidi"/><w:color w:val="2F5496" w:themeColor="accent1" w:themeShade="BF"/></w:rPr></w:style>
<w:style w:type="table" w:styleId="GridAccent1"><w:name w:val="Grid Table Accent 1"/>
 <w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="8EAADB" w:themeColor="accent1" w:themeTint="99"/></w:tblBorders></w:tblPr>
 <w:tblStylePr w:type="firstRow"><w:rPr><w:b/><w:color w:val="FFFFFF" w:themeColor="background1"/></w:rPr><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="4472C4" w:themeFill="accent1"/></w:tcPr></w:tblStylePr>
 <w:tblStylePr w:type="band1Horz"><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="D9E2F3" w:themeFill="accent1" w:themeFillTint="33"/></w:tcPr></w:tblStylePr>
</w:style>"#;

const BODY: &str = r#"<w:p w14:paraId="1A2B3C4D" w14:textId="77777777"><w:pPr><w:pStyle w:val="Heading1"/><w:pBdr><w:bottom w:val="single" w:sz="4" w:space="1" w:color="ED7D31" w:themeColor="accent2"/></w:pBdr><w:shd w:val="clear" w:color="auto" w:fill="F2F2F2" w:themeFill="background1" w:themeFillShade="F2"/></w:pPr>
 <w:r><w:rPr><w:color w:val="C45911" w:themeColor="accent2" w:themeShade="BF"/></w:rPr><w:t>Title</w:t></w:r></w:p>
<w:p w14:paraId="2B3C4D5E" w14:textId="77777777"><w:r><w:rPr><w:rFonts w:asciiTheme="majorHAnsi" w:hAnsiTheme="majorHAnsi"/></w:rPr><w:t>Body</w:t></w:r></w:p>
<w:tbl><w:tblPr><w:tblStyle w:val="GridAccent1"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>
 <w:tr w14:paraId="3C4D5E6F" w14:textId="77777777"><w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="FFC000" w:themeFill="accent4"/></w:tcPr><w:p w14:paraId="4D5E6F70" w14:textId="77777777"><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p w14:paraId="5E6F7081" w14:textId="77777777"/>"#;

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zw.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zw.write_all(bytes).unwrap();
    }
    zw.finish().unwrap().into_inner()
}

fn package() -> Vec<u8> {
    let doc = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W_NS}><w:body>{BODY}</w:body></w:document>"#);
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {W_NS}>{STYLES}</w:styles>"#);
    zip(&[
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", DOC_RELS.as_bytes()),
        ("word/styles.xml", styles.as_bytes()),
    ])
}

fn part(package: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(package)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

fn paras(d: &Document) -> Vec<&Paragraph> {
    d.body.iter().filter_map(|b| b.as_para()).collect()
}

fn para_ids(xml: &str, tag: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in xml.split(&format!("<{tag} ")).skip(1) {
        let head = chunk.split('>').next().unwrap_or("");
        if let Some(rest) = head.split("w14:paraId=\"").nth(1) {
            out.push(rest[..8].to_string());
        }
    }
    out
}

#[test]
fn theme_references_are_read_next_to_their_colours_and_fonts() {
    let d = kition_doc_docx::read(&package()).unwrap();
    let h1 = d.styles.get("Heading1").unwrap();
    assert_eq!(h1.chr.color, Some(TextColor::Rgb(Rgb(0x2F, 0x54, 0x96))));
    assert_eq!(h1.chr.color_theme, Some(ThemeColor { slot: ThemeSlot::Accent1, tint: None, shade: Some(0xBF), rgb: Rgb(0x2F, 0x54, 0x96) }));
    let fonts = h1.chr.font_theme.as_deref().unwrap();
    assert_eq!(
        (fonts.ascii.as_deref(), fonts.east_asia.as_deref(), fonts.cs.as_deref()),
        (Some("majorHAnsi"), Some("majorEastAsia"), Some("majorBidi"))
    );
    assert_eq!(fonts.font, h1.chr.font);
    let grid = d.styles.get("GridAccent1").unwrap().table.as_ref().unwrap();
    assert_eq!(grid.borders.unwrap().top.unwrap().theme.map(|t| (t.slot, t.tint)), Some((ThemeSlot::Accent1, Some(0x99))));
    assert_eq!(grid.fill_themes.get("headerFill").map(|t| t.slot), Some(ThemeSlot::Accent1));
    assert_eq!(grid.fill_themes.get("bandFill").map(|t| t.tint), Some(Some(0x33)));
    assert_eq!(grid.header_chr.color_theme.map(|t| t.slot), Some(ThemeSlot::Background1));
    let p = paras(&d);
    assert_eq!(p[0].props.para_id, Some(0x1A2B_3C4D));
    assert_eq!(p[0].props.shading_theme.map(|t| (t.slot, t.shade)), Some((ThemeSlot::Background1, Some(0xF2))));
    assert_eq!(p[0].props.borders.unwrap().bottom.unwrap().theme.map(|t| t.slot), Some(ThemeSlot::Accent2));
    assert_eq!(p[0].runs[0].props.color_theme.map(|t| t.slot), Some(ThemeSlot::Accent2));
    let Block::Table(t) = &*d.body[2] else { panic!("table") };
    assert_eq!(t.rows[0].props.para_id, Some(0x3C4D_5E6F));
    assert_eq!(t.rows[0].cells[0].props.shading_theme.map(|t| t.slot), Some(ThemeSlot::Accent4));
}

#[test]
fn theme_references_and_paragraph_ids_are_written_back() {
    let d = kition_doc_docx::read(&package()).unwrap();
    let out = kition_doc_docx::write(&d).unwrap();
    let styles = part(&out, "word/styles.xml");
    for attr in [
        r#"w:val="2F5496" w:themeColor="accent1" w:themeShade="BF""#,
        r#"w:asciiTheme="majorHAnsi""#,
        r#"w:hAnsiTheme="majorHAnsi""#,
        r#"w:eastAsiaTheme="majorEastAsia""#,
        r#"w:cstheme="majorBidi""#,
        r#"w:eastAsiaTheme="minorEastAsia""#,
        r#"w:color="8EAADB" w:themeColor="accent1" w:themeTint="99""#,
        r#"w:fill="4472C4" w:themeFill="accent1""#,
        r#"w:fill="D9E2F3" w:themeFill="accent1" w:themeFillTint="33""#,
        r#"w:val="FFFFFF" w:themeColor="background1""#,
    ] {
        assert!(styles.contains(attr), "styles.xml lost {attr}");
    }
    let body = part(&out, "word/document.xml");
    for attr in [
        r#"w:val="C45911" w:themeColor="accent2" w:themeShade="BF""#,
        r#"w:color="ED7D31" w:themeColor="accent2""#,
        r#"w:fill="F2F2F2" w:themeFill="background1" w:themeFillShade="F2""#,
        r#"w:fill="FFC000" w:themeFill="accent4""#,
    ] {
        assert!(body.contains(attr), "document.xml lost {attr}");
    }
    assert_eq!(para_ids(&body, "w:p"), ["1A2B3C4D", "2B3C4D5E", "4D5E6F70", "5E6F7081"]);
    assert_eq!(para_ids(&body, "w:tr"), ["3C4D5E6F"]);
    // Read back: the same model.
    let again = kition_doc_docx::read(&out).unwrap();
    assert_eq!(again.styles, d.styles);
    assert_eq!(again.body, d.body);
}

#[test]
fn an_edited_colour_or_font_drops_its_reference() {
    let mut d = kition_doc_docx::read(&package()).unwrap();
    let Block::Para(p) = std::sync::Arc::make_mut(&mut d.body[0]) else { panic!("para") };
    p.runs[0].props.color = Some(TextColor::Rgb(Rgb(0x11, 0x22, 0x33)));
    let Block::Para(p) = std::sync::Arc::make_mut(&mut d.body[1]) else { panic!("para") };
    p.runs[0].props.font = Some("Courier New".into());
    let body = part(&kition_doc_docx::write(&d).unwrap(), "word/document.xml");
    assert!(body.contains(r#"<w:color w:val="112233"/>"#), "{body}");
    assert!(!body.contains(r#"w:themeColor="accent2" w:themeShade="BF""#));
    assert!(body.contains(r#"w:ascii="Courier New""#) && !body.contains("majorHAnsi"));
}

#[test]
fn a_theme_change_re_resolves_the_references_that_hold() {
    let mut d = kition_doc_docx::read(&package()).unwrap();
    // A run whose colour was edited keeps it through the theme change.
    let Block::Para(p) = std::sync::Arc::make_mut(&mut d.body[0]) else { panic!("para") };
    p.runs[0].props.color = Some(TextColor::Rgb(Rgb(0x11, 0x22, 0x33)));
    d.settings.theme_colors[4] = Rgb(0x80, 0x00, 0x00); // accent1
    d.settings.theme_colors[7] = Rgb(0x00, 0x80, 0x00); // accent4
    d.settings.major_font = "Georgia".into();
    assert!(d.retheme());
    let h1 = d.styles.get("Heading1").unwrap();
    let dark_red =
        ThemeColor { slot: ThemeSlot::Accent1, tint: None, shade: Some(0xBF), rgb: Rgb(0, 0, 0) }.resolve(&d.settings.theme_colors).unwrap();
    assert_eq!(dark_red, Rgb(0x60, 0x00, 0x00));
    assert_eq!(h1.chr.color, Some(TextColor::Rgb(dark_red)));
    assert_eq!(h1.chr.color_theme.unwrap().rgb, dark_red);
    assert_eq!(h1.chr.font.as_deref(), Some("Georgia"));
    let grid = d.styles.get("GridAccent1").unwrap().table.as_ref().unwrap();
    assert_eq!(grid.header_fill, Some(Rgb(0x80, 0x00, 0x00)));
    // Accent 1 lightened by tint 99: luminance 0.25 → 0.25 × 0.6 + 0.4.
    assert_eq!(grid.borders.unwrap().top.unwrap().color, Some(Rgb(0xFF, 0x1A, 0x1A)));
    let p = paras(&d);
    assert_eq!(p[0].runs[0].props.color, Some(TextColor::Rgb(Rgb(0x11, 0x22, 0x33))), "an edited colour is not the theme's");
    assert_eq!(p[1].runs[0].props.font.as_deref(), Some("Georgia"));
    let Block::Table(t) = &*d.body[2] else { panic!("table") };
    assert_eq!(t.rows[0].cells[0].props.shading, Some(Rgb(0x00, 0x80, 0x00)));
    // Written with the references and their new colours.
    let out = kition_doc_docx::write(&d).unwrap();
    let styles = part(&out, "word/styles.xml");
    assert!(styles.contains(r#"w:val="600000" w:themeColor="accent1" w:themeShade="BF""#), "{styles}");
    assert!(styles.contains(r#"w:fill="800000" w:themeFill="accent1""#));
    assert!(part(&out, "word/document.xml").contains(r#"w:fill="008000" w:themeFill="accent4""#));
    // Nothing left to move.
    assert!(!d.retheme());
}

#[test]
fn paragraph_ids_stay_unique_and_valid_after_edits() {
    let mut d = kition_doc_docx::read(&package()).unwrap();
    // A copy of the first paragraph (as splitting or pasting makes) and a new paragraph.
    let copy = d.body[0].clone();
    d.body.insert(1, copy);
    d.body.insert(2, para_block(Paragraph::with_text("new", CharProps::default())));
    // An id out of range is not written.
    let Block::Para(p) = std::sync::Arc::make_mut(&mut d.body[3]) else { panic!("para") };
    p.props.para_id = Some(0x9000_0000);
    let body = part(&kition_doc_docx::write(&d).unwrap(), "word/document.xml");
    let ids = para_ids(&body, "w:p");
    assert_eq!(ids.len(), 6, "every paragraph has an id: {ids:?}");
    assert_eq!(ids[0], "1A2B3C4D", "the first of two copies keeps the id");
    assert!(!ids[1..].contains(&"1A2B3C4D".to_string()));
    let unique: HashSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len());
    for id in &ids {
        let v = u32::from_str_radix(id, 16).unwrap();
        assert!(v > 0 && v < 0x8000_0000, "{id}");
    }
    assert!(ids.contains(&"4D5E6F70".to_string()) && ids.contains(&"5E6F7081".to_string()));
    // Read back and written again: the same ids.
    let again = kition_doc_docx::read(&kition_doc_docx::write(&d).unwrap()).unwrap();
    let body2 = part(&kition_doc_docx::write(&again).unwrap(), "word/document.xml");
    assert_eq!(para_ids(&body2, "w:p"), ids);
}

#[test]
fn a_document_without_paragraph_ids_is_written_without_them() {
    let d = Document::from_text("one\ntwo");
    let body = part(&kition_doc_docx::write(&d).unwrap(), "word/document.xml");
    assert!(!body.contains("w14:paraId"), "{body}");
}
