use std::io::{Read, Write};

use kition_doc_engine::{
    doc::Document,
    io,
    layout::{self, Placed},
};

/// A list whose three levels use `formats`, with `counts[i]` items at level `i` in turn.
fn document(formats: &[&str], counts: &[usize]) -> Document {
    let levels: String = formats
        .iter()
        .enumerate()
        .map(|(i, f)| format!(r#"<w:lvl w:ilvl="{i}"><w:start w:val="1"/><w:numFmt w:val="{f}"/><w:lvlText w:val="%{}."/></w:lvl>"#, i + 1))
        .collect();
    let numbering = format!(
        r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="0">{levels}</w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    );
    let mut body = String::new();
    for (level, count) in counts.iter().enumerate() {
        for _ in 0..*count {
            body +=
                &format!(r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Item</w:t></w:r></w:p>"#);
        }
    }
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="30000"/></w:sectPr></w:body></w:document>"#
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in [("word/document.xml", xml.as_str()), ("word/numbering.xml", numbering.as_str()), ("word/_rels/document.xml.rels", rels)] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    io::open_bytes("list.docx", &zip.finish().unwrap().into_inner()).unwrap()
}

fn labels(doc: &Document) -> Vec<String> {
    let result = layout::layout(doc, &mut layout::LayoutCache::new(), &Default::default());
    result
        .pages
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|item| match item {
            Placed::Lines { para, l0: 0, .. } => para.label.as_ref().map(|l| l.text.clone()),
            _ => None,
        })
        .collect()
}

fn saved_formats(doc: &Document) -> String {
    let bytes = io::save_bytes("saved.docx", doc).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/numbering.xml").unwrap().read_to_string(&mut xml).unwrap();
    xml
}

const COUNTING: [&str; 12] = [
    "\u{4E00}",
    "\u{4E8C}",
    "\u{4E09}",
    "\u{56DB}",
    "\u{4E94}",
    "\u{516D}",
    "\u{4E03}",
    "\u{516B}",
    "\u{4E5D}",
    "\u{5341}",
    "\u{5341}\u{4E00}",
    "\u{5341}\u{4E8C}",
];
const LEGAL: [&str; 10] =
    ["\u{58F9}", "\u{8D30}", "\u{53C1}", "\u{8086}", "\u{4F0D}", "\u{9646}", "\u{67D2}", "\u{634C}", "\u{7396}", "\u{58F9}\u{62FE}"];

#[test]
fn chinese_numbering_formats_label_items_and_survive_saves() {
    let formats = ["chineseCounting", "chineseLegalSimplified", "decimalEnclosedCircleChinese"];
    let mut doc = document(&formats, &[12, 10, 21]);
    for _ in 0..2 {
        let got = labels(&doc);
        let mut want: Vec<String> = COUNTING.iter().map(|n| format!("{n}.")).collect();
        want.extend(LEGAL.iter().map(|n| format!("{n}.")));
        want.extend((0..20).map(|i| format!("{}.", char::from_u32(0x2460 + i).unwrap())));
        want.push("21.".into());
        assert_eq!(got, want);
        let xml = saved_formats(&doc);
        for f in formats {
            assert!(xml.contains(&format!(r#"w:numFmt w:val="{f}""#)), "{f} kept: {xml}");
        }
        doc = io::open_bytes("saved.docx", &io::save_bytes("saved.docx", &doc).unwrap()).unwrap();
    }
}
