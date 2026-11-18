use std::io::{Read, Write};
use std::sync::Arc;

use kition_doc_engine::{
    Session,
    doc::Settings,
    io,
    layout::{self, Placed},
};
use serde_json::json;

const FAMILY: &str = "Missing Declared CJK Font";
const FONTS: &str = r#"<w:fonts xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:font w:name="Missing Declared CJK Font"><w:charset w:val="86"/><w:family w:val="roman"/><w:pitch w:val="variable"/></w:font><w:font w:name="Source Sans 3"><w:charset w:val="00"/></w:font></w:fonts>"#;

fn fixture() -> Session {
    let doc = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:rPr><w:rFonts w:ascii="{FAMILY}"/><w:sz w:val="24"/></w:rPr></w:pPr></w:p></w:body></w:document>"#
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="fonts" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml"/></Relationships>"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in
        [("word/document.xml", doc.as_bytes()), ("word/fontTable.xml", FONTS.as_bytes()), ("word/_rels/document.xml.rels", rels.as_bytes())]
    {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    Session::new(io::open_bytes("font-metadata.docx", &zip.finish().unwrap().into_inner()).unwrap())
}

#[test]
fn font_charset_metadata_is_parsed_and_preserved_verbatim_when_unchanged() {
    let session = fixture();
    let data = serde_json::to_value(&session.doc.settings).unwrap();
    assert_eq!(data["fontCharsets"][FAMILY], 134);
    assert_eq!(data["fontCharsets"]["Source Sans 3"], 0);
    let saved = kition_doc_docx::write(&session.doc).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&saved)).unwrap();
    let mut actual = String::new();
    zip.by_name("word/fontTable.xml").unwrap().read_to_string(&mut actual).unwrap();
    assert_eq!(actual, FONTS);
    let reopened = io::open_bytes("saved.docx", &saved).unwrap();
    assert_eq!(serde_json::to_value(&reopened.settings).unwrap()["fontCharsets"], data["fontCharsets"]);
}

#[test]
fn font_charset_changes_invalidate_layout_and_survive_save() {
    let mut session = fixture();
    let mut cache = layout::LayoutCache::new();
    let first = layout::layout(&session.doc, &mut cache, &Default::default());
    let get = |layout: &layout::DocLayout| {
        layout.pages[0]
            .items
            .iter()
            .find_map(|item| match item {
                Placed::Lines { para, .. } => Some(para.clone()),
                _ => None,
            })
            .unwrap()
    };
    let before = get(&first);
    let mut settings = serde_json::to_value(&session.doc.settings).unwrap();
    settings["fontCharsets"] = json!({FAMILY:128,"Source Sans 3":0});
    session.doc.settings = serde_json::from_value::<Settings>(settings).unwrap();
    let next = layout::layout(&session.doc, &mut cache, &Default::default());
    assert!(!Arc::ptr_eq(&before, &get(&next)), "font metadata changed, so cached font metrics must be recomputed");
    let saved = kition_doc_docx::write(&session.doc).unwrap();
    let reopened = io::open_bytes("saved.docx", &saved).unwrap();
    assert_eq!(serde_json::to_value(&reopened.settings).unwrap()["fontCharsets"][FAMILY], 128);
}

#[test]
fn font_charset_new_declarations_survive_without_an_original_table() {
    let mut session = Session::new(kition_doc_engine::doc::Document::from_text("Text"));
    session.doc.settings.font_charsets = Some([(FAMILY.to_string(), 0x88)].into_iter().collect());
    let saved = kition_doc_docx::write(&session.doc).unwrap();
    let reopened = io::open_bytes("saved.docx", &saved).unwrap();
    assert_eq!(serde_json::to_value(&reopened.settings).unwrap()["fontCharsets"][FAMILY], 136);
}

#[test]
fn font_charset_clearing_metadata_keeps_other_font_properties() {
    let mut session = fixture();
    session.doc.settings.font_charsets = Some(Default::default());
    let saved = kition_doc_docx::write(&session.doc).unwrap();
    let reopened = io::open_bytes("saved.docx", &saved).unwrap();
    assert_eq!(serde_json::to_value(&reopened.settings).unwrap()["fontCharsets"], json!({}));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&saved)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/fontTable.xml").unwrap().read_to_string(&mut xml).unwrap();
    assert!(xml.contains("<w:family w:val=\"roman\""));
    assert!(xml.contains("<w:pitch w:val=\"variable\""));
    assert!(!xml.contains("<w:charset"));
}
