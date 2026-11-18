//! What a saved file keeps that Kition Doc does not edit (written for these tests from the
//! spec): row property exceptions, explicit cell alignment, and more, each kept from the file and
//! written back as it was.

use std::io::{Read, Write};

use kition_doc_doc::props::VAlign;
use kition_doc_doc::{Block, Document};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zw.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zw.write_all(bytes).unwrap();
    }
    zw.finish().unwrap().into_inner()
}

/// A package with `body` as the document body, the document's relationships `rels`
/// (`Relationship` elements) and extra parts.
fn package(body: &str, rels: &str, extra: &[(&str, &str)]) -> Vec<u8> {
    let doc = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{body}</w:body></w:document>"#);
    let rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#
    );
    let mut entries: Vec<(&str, &[u8])> =
        vec![("_rels/.rels", ROOT_RELS.as_bytes()), ("word/document.xml", doc.as_bytes()), ("word/_rels/document.xml.rels", rels.as_bytes())];
    for (name, text) in extra {
        entries.push((name, text.as_bytes()));
    }
    zip(&entries)
}

fn part(package: &[u8], name: &str) -> Option<String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(package)).unwrap();
    let mut s = String::new();
    z.by_name(name).ok()?.read_to_string(&mut s).unwrap();
    Some(s)
}

fn round_trip(bytes: &[u8]) -> (Document, Vec<u8>) {
    let doc = kition_doc_docx::read(bytes).unwrap();
    let written = kition_doc_docx::write(&doc).unwrap();
    (doc, written)
}

fn table(doc: &mut Document) -> &mut kition_doc_doc::Table {
    doc.body
        .iter_mut()
        .find_map(|b| match std::sync::Arc::make_mut(b) {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .unwrap()
}

const EX: &str = r#"<w:tblPrEx><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="FF0000"/><w:insideV w:val="nil"/></w:tblBorders><w:tblCellMar><w:left w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPrEx>"#;

fn row_table() -> Vec<u8> {
    let cell = |valign: &str, text: &str| {
        format!(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/>{valign}</w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>"#)
    };
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{EX}<w:trPr><w:cantSplit/></w:trPr>{}{}{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl><w:p/>"#,
        cell(r#"<w:vAlign w:val="top"/>"#, "a"),
        cell(r#"<w:vAlign w:val="center"/>"#, "b"),
        cell("", "c"),
        cell("", "d"),
        cell("", "e"),
        cell("", "f"),
    );
    package(&body, "", &[])
}

#[test]
fn row_exceptions_are_written_back_before_the_row_properties() {
    let (mut doc, written) = round_trip(&row_table());
    let xml = part(&written, "word/document.xml").unwrap();
    assert!(xml.contains(&format!("<w:tr>{EX}<w:trPr><w:cantSplit/></w:trPr>")), "{xml}");
    assert_eq!(xml.matches("<w:tblPrEx>").count(), 1, "only the row that had them: {xml}");
    // Read again, the same.
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(again.body, doc.body);
    // Only WordprocessingML elements in one `w:tblPrEx` are ever written.
    for hostile in [r#"<w:tblPrEx><x:evil xmlns:x="urn:x"/></w:tblPrEx>"#, "</w:tr><w:p>", r#"<w:tblPrEx/><w:tblPrEx/>"#, r#"<w:jc w:val="center"/>"#]
    {
        table(&mut doc).rows[0].props.exceptions = Some(hostile.into());
        let xml = part(&kition_doc_docx::write(&doc).unwrap(), "word/document.xml").unwrap();
        assert!(!xml.contains("tblPrEx") && !xml.contains("evil"), "{hostile}: {xml}");
        assert!(kition_doc_docx::read(&kition_doc_docx::write(&doc).unwrap()).is_ok());
    }
}

#[test]
fn an_explicit_top_alignment_stays_explicit() {
    let (doc, written) = round_trip(&row_table());
    let xml = part(&written, "word/document.xml").unwrap();
    assert_eq!(xml.matches(r#"<w:vAlign w:val="top"/>"#).count(), 1, "{xml}");
    assert_eq!(xml.matches(r#"<w:vAlign w:val="center"/>"#).count(), 1, "{xml}");
    let mut doc = doc;
    let cells = &table(&mut doc).rows[0].cells;
    assert_eq!((cells[0].props.valign, cells[0].props.valign_set), (VAlign::Top, true));
    assert_eq!((cells[2].props.valign, cells[2].props.valign_set), (VAlign::Top, false));
}

const STYLES: &str = r#"<w:style w:type="paragraph" w:default="1" w:styleId="a"><w:name w:val="Normal"/><w:qFormat/></w:style>
<w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:basedOn w:val="a"/><w:next w:val="a"/><w:uiPriority w:val="29"/><w:unhideWhenUsed/><w:pPr><w:spacing w:before="200" w:beforeLines="50" w:beforeAutospacing="0"/></w:pPr><w:rPr><w:i/></w:rPr></w:style>
<w:style w:type="table" w:styleId="GridTable4"><w:name w:val="Grid Table 4"/><w:basedOn w:val="TableNormal"/><w:uiPriority w:val="49"/>
 <w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>
 <w:tblPr><w:tblStyleRowBandSize w:val="1"/><w:tblStyleColBandSize w:val="1"/><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="666666"/></w:tblBorders></w:tblPr>
 <w:tblStylePr w:type="firstRow"><w:rPr><w:b/><w:color w:val="FFFFFF"/></w:rPr><w:tblPr/><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="000000"/></w:tcPr></w:tblStylePr>
 <w:tblStylePr w:type="band2Horz"><w:tblPr/><w:tcPr><w:tcBorders><w:insideV w:val="nil"/></w:tcBorders></w:tcPr></w:tblStylePr>
 <w:tblStylePr w:type="nwCell"><w:tblPr/><w:tcPr><w:tcBorders><w:bottom w:val="nil"/></w:tcBorders></w:tcPr></w:tblStylePr>
</w:style>
<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:tblPr><w:tblInd w:w="0" w:type="dxa"/></w:tblPr></w:style>"#;

fn styled() -> Vec<u8> {
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {NS}>{STYLES}</w:styles>"#);
    package(
        r#"<w:p><w:pPr><w:pStyle w:val="Quote"/></w:pPr><w:r><w:t>q</w:t></w:r></w:p>"#,
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
        &[("word/styles.xml", &styles)],
    )
}

#[test]
fn styles_are_written_back_as_the_file_had_them_until_edited() {
    let (doc, written) = round_trip(&styled());
    let xml = part(&written, "word/styles.xml").unwrap();
    // What the model does not hold: table style regions, column bands, spacing in lines, flags.
    for kept in [
        r#"<w:tblStylePr w:type="band2Horz">"#,
        r#"<w:tblStylePr w:type="nwCell">"#,
        "<w:tblStyleColBandSize",
        r#"w:beforeLines="50""#,
        "<w:unhideWhenUsed/>",
    ] {
        assert!(xml.contains(kept), "{kept}: {xml}");
    }
    // The default paragraph style reads as `Normal`; the styles naming it are kept naming it so.
    assert!(xml.contains(r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/>"#), "{xml}");
    assert!(!xml.contains(r#"w:val="a""#) && !xml.contains(r#"w:styleId="a""#), "{xml}");
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(again.styles, doc.styles);
    assert_eq!(part(&kition_doc_docx::write(&again).unwrap(), "word/styles.xml").unwrap(), xml);

    // An edited style is written from the model; the others stay as they were.
    let mut edited = doc.clone();
    let grid = edited.styles.styles.iter_mut().find(|s| s.id == "GridTable4").unwrap();
    grid.table.as_mut().unwrap().header_fill = Some(kition_doc_doc::props::Rgb(0x12, 0x34, 0x56));
    let xml = part(&kition_doc_docx::write(&edited).unwrap(), "word/styles.xml").unwrap();
    assert!(xml.contains(r#"w:fill="123456""#) && !xml.contains("band2Horz"), "{xml}");
    assert!(xml.contains(r#"w:beforeLines="50""#), "{xml}");
}

/// Kept markup records (`{digest:016x} {len}\n{xml}` each) and back.
fn records(bytes: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while let Some(nl) = rest.iter().position(|b| *b == b'\n') {
        let head = std::str::from_utf8(&rest[..nl]).unwrap();
        let (digest, len) = head.split_once(' ').unwrap();
        let len: usize = len.parse().unwrap();
        out.push((digest.to_string(), String::from_utf8(rest[nl + 1..nl + 1 + len].to_vec()).unwrap()));
        rest = &rest[nl + 1 + len..];
    }
    out
}

fn to_bytes(records: &[(String, String)]) -> Vec<u8> {
    records.iter().flat_map(|(d, x)| format!("{d} {}\n{x}", x.len()).into_bytes()).collect()
}

#[test]
fn kept_styles_only_ever_write_the_style_they_stand_for() {
    const KEY: &str = "kition-doc:styles.kept";
    let (doc, _) = round_trip(&styled());
    let kept = records(doc.passthrough.get(KEY).unwrap());
    assert_eq!(kept.len(), doc.styles.styles.len());
    let count = |d: &Document| part(&kition_doc_docx::write(d).unwrap(), "word/styles.xml").unwrap();
    // A record standing for another style id, or not a style, is not written.
    for (from, to) in [
        (r#"w:styleId="Quote""#, r#"w:styleId="Other""#),
        (r#"<w:style w:type="paragraph" w:styleId="Quote""#, r#"<w:style w:type="table" w:styleId="Quote""#),
        ("<w:style ", "<w:p "),
        ("</w:style>", "</w:p>"),
    ] {
        let mut d = doc.clone();
        let changed: Vec<(String, String)> = kept.iter().map(|(dg, x)| (dg.clone(), x.replacen(from, to, 1))).collect();
        d.passthrough.insert(KEY.into(), std::sync::Arc::new(to_bytes(&changed)));
        let xml = count(&d);
        assert_eq!(xml.matches("<w:style ").count(), doc.styles.styles.len(), "{to}: {xml}");
        assert!(!xml.contains("Other") && !xml.contains("<w:p ") && xml.matches(r#"w:type="table""#).count() == 2, "{to}: {xml}");
    }
    // Malformed records are ignored.
    for junk in [&b"zz 5\n<w:st"[..], b"0000000000000001 999999\n<w:style/>", b"\n\n\n", &[0xff, 0xfe, b'\n']] {
        let mut d = doc.clone();
        d.passthrough.insert(KEY.into(), std::sync::Arc::new(junk.to_vec()));
        assert_eq!(count(&d).matches("<w:style ").count(), doc.styles.styles.len());
    }
}

const INK_NS: &str = r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office""#;

/// A run of ink as Word writes it: an ink drawing naming its ink part, with a VML picture of
/// the ink as the fallback.
fn ink_run(left: &str) -> String {
    format!(
        r#"<w:r><mc:AlternateContent {INK_NS}><mc:Choice Requires="wps"><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="2" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>12700</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>12700</wp:posOffset></wp:positionV><wp:extent cx="254000" cy="127000"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="7" name="Ink 7"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingInk"><mc:AlternateContent xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main"><mc:Choice Requires="a14"><w14:contentPart bwMode="clr" r:id="rId5"><w14:nvContentPartPr><w14:cNvPr id="7" name="Ink 7"/><w14:cNvContentPartPr/></w14:nvContentPartPr><w14:xfrm><a:off x="0" y="0"/><a:ext cx="254000" cy="127000"/></w14:xfrm></w14:contentPart></mc:Choice></mc:AlternateContent></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice><mc:Fallback><w:pict><v:shape id="s1" o:spt="75" style="position:absolute;margin-left:{left}pt;margin-top:1pt;width:20pt;height:10pt;z-index:2" coordsize="21600,21600"><v:imagedata r:id="rId6" o:title=""/></v:shape></w:pict></mc:Fallback></mc:AlternateContent></w:r>"#
    )
}

fn ink() -> Vec<u8> {
    let types = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="png" ContentType="image/png"/><Override PartName="/word/ink/ink1.xml" ContentType="application/inkml+xml"/></Types>"#;
    let ink = r#"<inkml:ink xmlns:inkml="http://www.w3.org/2003/InkML"><inkml:trace>0 0, 10 10</inkml:trace></inkml:ink>"#;
    package(
        &format!("<w:p>{}</w:p>", ink_run("1")),
        r#"<Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="ink/ink1.xml"/><Relationship Id="rId6" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>"#,
        &[("[Content_Types].xml", types), ("word/ink/ink1.xml", ink), ("word/media/image1.png", "\u{89}PNG stand-in")],
    )
}

#[test]
fn ink_is_kept_whole_and_shown_as_its_fallback_picture() {
    use kition_doc_doc::InlineObject;
    let (doc, written) = round_trip(&ink());
    // Shown as its picture.
    let para = doc.body.iter().find_map(|b| b.as_para()).unwrap();
    let image = para.objects.iter().find(|o| matches!(o, InlineObject::Image { .. })).unwrap();
    let InlineObject::Image { w, h, ole, .. } = image else { unreachable!() };
    assert_eq!((*w, *h), (20.0, 10.0));
    assert!(ole.is_some());
    // Written back whole: the ink drawing, its part and content type, and the picture once.
    let xml = part(&written, "word/document.xml").unwrap();
    assert!(xml.contains("wordprocessingInk") && xml.contains("<w14:contentPart") && xml.contains(r#"<mc:Choice Requires="a14">"#), "{xml}");
    assert!(xml.contains(r#"xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main""#), "the prefix it requires is declared: {xml}");
    assert_eq!(
        part(&written, "word/ink/ink1.xml").as_deref(),
        Some(r#"<inkml:ink xmlns:inkml="http://www.w3.org/2003/InkML"><inkml:trace>0 0, 10 10</inkml:trace></inkml:ink>"#)
    );
    assert!(part(&written, "[Content_Types].xml").unwrap().contains(r#"PartName="/word/ink/ink1.xml" ContentType="application/inkml+xml""#));
    let rels = part(&written, "word/_rels/document.xml.rels").unwrap();
    let rel_of = |target: &str| {
        let at = rels.find(&format!(r#"Target="{target}""#)).unwrap_or_else(|| panic!("{target}: {rels}"));
        let start = rels[..at].rfind("Id=\"").unwrap() + 4;
        rels[start..].split('"').next().unwrap().to_string()
    };
    assert!(xml.contains(&format!(r#"<w14:contentPart bwMode="clr" r:id="{}">"#, rel_of("ink/ink1.xml"))), "{xml}\n{rels}");
    let media: Vec<&str> = rels.match_indices("media/").map(|(i, _)| &rels[i..i + 16]).collect();
    assert_eq!(media.len(), 1, "{rels}");
    assert!(xml.contains(&format!(r#"<v:imagedata r:id="{}""#, rel_of(media[0]))), "{xml}\n{rels}");
    // Read again, the same.
    // Saved again, the same (relationship and drawing ids are the writer's own).
    let again = kition_doc_docx::write(&kition_doc_docx::read(&written).unwrap()).unwrap();
    assert_eq!(part(&again, "word/document.xml").unwrap(), xml);
}

#[test]
fn moved_ink_is_written_as_its_picture_where_it_now_is() {
    use kition_doc_doc::InlineObject;
    let (mut doc, _) = round_trip(&ink());
    let block = doc.body.iter_mut().find(|b| b.as_para().is_some()).unwrap();
    let Block::Para(p) = std::sync::Arc::make_mut(block) else { unreachable!() };
    for o in p.objects.iter_mut() {
        if let InlineObject::Image { float, .. } = o {
            float.x += 30.0;
        }
    }
    let written = kition_doc_docx::write(&doc).unwrap();
    let xml = part(&written, "word/document.xml").unwrap();
    assert!(!xml.contains("wordprocessingInk") && !xml.contains("mc:AlternateContent"), "{xml}");
    assert!(xml.contains("<w:pict><v:shape") && xml.contains("margin-left:31pt"), "{xml}");
}

fn notes(body: &str, notes: &str) -> Vec<u8> {
    let footnotes = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><w:footnotes {NS}><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>{notes}</w:footnotes>"#
    );
    package(
        body,
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>"#,
        &[("word/footnotes.xml", &footnotes)],
    )
}

fn note_texts(doc: &Document) -> Vec<String> {
    doc.parts
        .values()
        .filter(|p| p.kind == kition_doc_doc::PartKind::Footnote)
        .map(|p| p.blocks.iter().filter_map(|b| b.as_para()).map(|p| p.text.replace(kition_doc_doc::para::OBJ, "#")).collect::<String>())
        .collect()
}

#[test]
fn notes_nothing_refers_to_are_not_read() {
    let note =
        |id: u32, text: &str| format!(r#"<w:footnote w:id="{id}"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t>{text}</w:t></w:r></w:p></w:footnote>"#);
    let body = r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:footnoteReference w:id="3"/></w:r><w:r><w:footnoteReference w:id="1"/></w:r><w:r><w:footnoteReference w:id="3"/></w:r></w:p>"#;
    let (doc, written) = round_trip(&notes(body, &[note(1, "one"), note(2, "unseen"), note(3, "three")].concat()));
    assert_eq!(note_texts(&doc), ["#one", "#three"]);
    let xml = part(&written, "word/footnotes.xml").unwrap();
    assert!(!xml.contains("unseen") && xml.matches("<w:footnote w:id=").count() == 2, "{xml}");
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(note_texts(&again).len(), 2);
}

#[test]
fn a_custom_mark_in_the_run_after_its_reference_is_the_mark() {
    use kition_doc_doc::InlineObject;
    // As some writers save it: the mark in its own run after the reference, the note's text
    // starting with the mark rather than the note's number.
    let body = r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:footnoteReference w:id="1" w:customMarkFollows="true"/></w:r><w:r><w:t>†</w:t></w:r><w:r><w:t> b</w:t></w:r></w:p><w:p><w:r><w:footnoteReference w:id="1" w:customMarkFollows="1"/></w:r></w:p>"#;
    let note = r#"<w:footnote w:id="1"><w:p><w:r><w:t>† dagger</w:t></w:r></w:p></w:footnote>"#;
    let (doc, written) = round_trip(&notes(body, note));
    let refs: Vec<String> = doc
        .body
        .iter()
        .filter_map(|b| b.as_para())
        .flat_map(|p| p.objects.iter())
        .filter_map(|o| match o {
            InlineObject::NoteRef { custom, .. } => Some(custom.clone()),
            _ => None,
        })
        .collect();
    // The first has its mark; the second has none to read, so it is numbered.
    assert_eq!(refs, ["†", ""]);
    assert_eq!(
        doc.body.iter().filter_map(|b| b.as_para()).map(|p| p.text.replace(kition_doc_doc::para::OBJ, "#")).collect::<Vec<_>>(),
        ["a# b", "#"]
    );
    let xml = part(&written, "word/document.xml").unwrap();
    assert!(xml.contains(r#"<w:footnoteReference w:customMarkFollows="1" w:id="1"/><w:t xml:space="preserve">†</w:t>"#), "{xml}");
    // A note referenced with a custom mark is not given its number.
    let notes_xml = part(&written, "word/footnotes.xml").unwrap();
    assert!(!notes_xml.contains("<w:footnoteRef/>"), "{notes_xml}");
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(note_texts(&again), note_texts(&doc));
    assert_eq!(note_texts(&doc), ["† dagger"]);
}

#[test]
fn an_ole_object_writes_its_picture_once() {
    let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="png" ContentType="image/png"/><Default Extension="bin" ContentType="application/vnd.openxmlformats-officedocument.oleObject"/></Types>"#;
    let body = r##"<w:p><w:r><w:object w:dxaOrig="1440" w:dyaOrig="720"><v:shape xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" id="i1" type="#_x0000_t75" style="width:72pt;height:36pt" o:ole=""><v:imagedata r:id="rId8" o:title=""/></v:shape><o:OLEObject xmlns:o="urn:schemas-microsoft-com:office:office" Type="Embed" ProgID="Package" ShapeID="i1" DrawAspect="Content" ObjectID="_1" r:id="rId9"/></w:object></w:r></w:p>"##;
    let rels = r#"<Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject" Target="embeddings/oleObject1.bin"/>"#;
    let bytes = package(
        body,
        rels,
        &[("[Content_Types].xml", types), ("word/media/image1.png", "\u{89}PNG stand-in"), ("word/embeddings/oleObject1.bin", "OLE")],
    );
    let (doc, written) = round_trip(&bytes);
    let names: Vec<String> =
        zip::ZipArchive::new(std::io::Cursor::new(&written)).unwrap().file_names().filter(|n| n.contains("media/")).map(str::to_string).collect();
    assert_eq!(names.len(), 1, "{names:?}");
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(again.media.len(), doc.media.len());
    assert_eq!(again.passthrough, doc.passthrough, "the kept object's parts, unchanged");
}

#[test]
fn a_shape_without_a_line_keeps_the_line_width() {
    use kition_doc_doc::InlineObject;
    let body = r#"<w:p><w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><wp:extent cx="914400" cy="457200"/><wp:docPr id="1" name="Box 1"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln w="6350"><a:noFill/></a:ln></wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let width = |d: &Document| {
        d.body.iter().filter_map(|b| b.as_para()).flat_map(|p| p.objects.iter()).find_map(|o| match o {
            InlineObject::Shape { stroke, stroke_width, .. } => Some((*stroke, *stroke_width)),
            _ => None,
        })
    };
    let (doc, written) = round_trip(&package(body, "", &[]));
    assert_eq!(width(&doc), Some((None, 0.5)));
    assert!(part(&written, "word/document.xml").unwrap().contains(r#"<a:ln w="6350"><a:noFill/></a:ln>"#));
    assert_eq!(width(&kition_doc_docx::read(&written).unwrap()), Some((None, 0.5)));
}

#[test]
fn a_comment_reference_keeps_its_run_formatting() {
    use kition_doc_doc::InlineObject;
    let comments = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><w:comments {NS}><w:comment w:id="0" w:author="A" w:initials="A"><w:p><w:r><w:t>note</w:t></w:r></w:p></w:comment></w:comments>"#
    );
    let body = r#"<w:p><w:commentRangeStart w:id="0"/><w:r><w:t>text</w:t></w:r><w:commentRangeEnd w:id="0"/><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="24"/></w:rPr><w:commentReference w:id="0"/></w:r></w:p>"#;
    let bytes = package(
        body,
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>"#,
        &[("word/comments.xml", &comments)],
    );
    let end = |d: &Document| {
        let p = d.body.iter().find_map(|b| b.as_para()).unwrap();
        let at = p.text.char_indices().filter(|(_, c)| *c == kition_doc_doc::para::OBJ).map(|(i, _)| i).next_back().unwrap();
        assert!(matches!(p.object_at(at), Some(InlineObject::CommentEnd { .. })));
        p.props_of_char(at).clone()
    };
    let (doc, written) = round_trip(&bytes);
    assert_eq!((end(&doc).font.as_deref(), end(&doc).size), (Some("Arial"), Some(12.0)));
    let again = kition_doc_docx::read(&written).unwrap();
    assert_eq!(end(&again), end(&doc));
    // A comment made without formatting is written without any.
    let mut plain = doc.clone();
    let block = plain.body.iter_mut().find(|b| b.as_para().is_some()).unwrap();
    let Block::Para(p) = std::sync::Arc::make_mut(block) else { unreachable!() };
    for r in p.runs.iter_mut() {
        r.props = Default::default();
    }
    let xml = part(&kition_doc_docx::write(&plain).unwrap(), "word/document.xml").unwrap();
    assert!(xml.contains(r#"<w:r><w:commentReference w:id="0"/></w:r>"#), "{xml}");
}
