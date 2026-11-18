use std::io::Write;

use kition_doc_engine::{
    doc::{Document, Path, StoryRef, props::CharProps},
    io,
    layout::{self, Placed},
};

const SLOTS: &str = r#"w:asciiTheme="minorHAnsi" w:eastAsiaTheme="minorEastAsia" w:cstheme="majorBidi""#;

fn document(language: &str, defaults: bool, slots: &str) -> Document {
    let body = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:rPr><w:rFonts {slots} w:hint="eastAsia"/><w:sz w:val="24"/><w:lang w:val="en-US" w:eastAsia="zh-CN"/></w:rPr><w:t>A&#x2014;B</w:t></w:r></w:p></w:body></w:document>"#
    );
    let theme = format!(
        r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Synthetic"><a:themeElements><a:fontScheme name="Synthetic">
        <a:majorFont><a:latin typeface="Source Serif 4"/><a:ea typeface="{}"/><a:cs typeface="{}"/><a:font script="Hans" typeface="Source Serif 4"/><a:font script="Hant" typeface="Source Sans 3"/><a:font script="Jpan" typeface="JetBrains Mono"/><a:font script="Arab" typeface="JetBrains Mono"/></a:majorFont>
        <a:minorFont><a:latin typeface="Source Sans 3"/><a:ea typeface="{}"/><a:cs typeface="{}"/><a:font script="Hans" typeface="Source Serif 4"/><a:font script="Hant" typeface="Source Sans 3"/><a:font script="Jpan" typeface="JetBrains Mono"/><a:font script="Arab" typeface="Source Serif 4"/></a:minorFont>
        </a:fontScheme></a:themeElements></a:theme>"#,
        if defaults { "Source Serif 4" } else { "" },
        if defaults { "Source Sans 3" } else { "" },
        if defaults { "Source Serif 4" } else { "" },
        if defaults { "Source Sans 3" } else { "" },
    );
    let settings = format!(r#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{language}</w:settings>"#);
    let relationships = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="theme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/><Relationship Id="settings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, content) in [
        ("word/document.xml", body.as_str()),
        ("word/theme/theme1.xml", theme.as_str()),
        ("word/settings.xml", settings.as_str()),
        ("word/_rels/document.xml.rels", relationships),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    io::open_bytes("theme.docx", &zip.finish().unwrap().into_inner()).unwrap()
}

fn properties(doc: &Document) -> &CharProps {
    &doc.para(StoryRef::Body, &Path::top(0)).unwrap().runs[0].props
}

#[test]
fn theme_font_languages_select_separate_east_asian_and_bidi_scripts() {
    let doc = document(r#"<w:themeFontLang w:val="en-US" w:eastAsia="zh-CN" w:bidi="fa-IR"/>"#, false, SLOTS);
    let props = properties(&doc);
    assert_eq!(props.font.as_deref(), Some("Source Sans 3"));
    assert_eq!(props.font_ea.as_deref(), Some("Source Serif 4"));
    assert_eq!(props.font_cs.as_deref(), Some("JetBrains Mono"));
    let refs = props.font_theme.as_ref().unwrap();
    assert_eq!(refs.east_asia.as_deref(), Some("minorEastAsia"));
    assert_eq!(refs.font_ea, props.font_ea);
    assert_eq!(refs.font_cs, props.font_cs);
}

#[test]
fn theme_font_languages_apply_the_script_mapping_to_latin_theme_slots() {
    let doc = document(r#"<w:themeFontLang w:val="ja-JP"/>"#, false, r#"w:asciiTheme="majorAscii" w:hAnsiTheme="majorHAnsi""#);
    assert_eq!(properties(&doc).font.as_deref(), Some("JetBrains Mono"));
}

#[test]
fn theme_font_languages_use_region_defaults_without_a_matching_language() {
    for language in ["", r#"<w:themeFontLang w:eastAsia="zz-ZZ" w:bidi="zz-ZZ"/>"#] {
        let doc = document(language, true, SLOTS);
        assert_eq!(properties(&doc).font_ea.as_deref(), Some("Source Serif 4"));
        assert_eq!(properties(&doc).font_cs.as_deref(), Some("Source Sans 3"));
    }
}

#[test]
fn theme_font_languages_distinguish_japanese_and_explicit_traditional_script() {
    for (language, family) in [("ja-JP", "JetBrains Mono"), ("zh-Hant-SG", "Source Sans 3"), ("zh-TW", "Source Sans 3")] {
        let settings = format!(r#"<w:themeFontLang w:eastAsia="{language}"/>"#);
        let doc = document(&settings, false, SLOTS);
        assert_eq!(properties(&doc).font_ea.as_deref(), Some(family));
    }
}

#[test]
fn theme_font_languages_preserve_resolved_references_through_repeated_saves() {
    let mut doc = document(r#"<w:themeFontLang w:val="en-US" w:eastAsia="zh-CN" w:bidi="fa-IR"/>"#, false, SLOTS);
    assert_eq!(properties(&doc).font_ea.as_deref(), Some("Source Serif 4"));
    let expected = properties(&doc).clone();
    for _ in 0..3 {
        doc = io::open_bytes("saved.docx", &io::save_bytes("saved.docx", &doc).unwrap()).unwrap();
        assert_eq!(properties(&doc), &expected);
        assert_eq!(doc.settings.theme_font_lang.as_ref().unwrap().east_asia.as_deref(), Some("zh-CN"));
    }
}

#[test]
fn theme_font_languages_reach_the_shaper_without_changing_the_latin_face() {
    let doc = document(r#"<w:themeFontLang w:eastAsia="zh-CN"/>"#, false, SLOTS);
    let layout = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
    let p = layout.pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Placed::Lines { para, .. } => Some(para),
            _ => None,
        })
        .unwrap();
    let families: Vec<_> = p.clusters.iter().map(|c| p.styles[c.style as usize].face.family.as_str()).collect();
    assert_eq!(families, ["Source Sans 3", "Source Serif 4", "Source Sans 3"]);
}
