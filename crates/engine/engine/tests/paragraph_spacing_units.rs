use std::io::Write;

use kition_doc_engine::{
    Session,
    doc::{Document, Path, StoryRef},
    io,
    layout::{self, Placed},
};
use serde_json::json;

fn document(spacing: &str, style_spacing: &str) -> Document {
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
      <w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Source Sans 3"/><w:sz w:val="24"/></w:rPr><w:t>First</w:t></w:r></w:p>
      <w:p><w:pPr><w:pStyle w:val="Measured"/><w:spacing {spacing}/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Source Sans 3"/><w:sz w:val="24"/></w:rPr><w:t>Second</w:t></w:r></w:p>
      <w:sectPr><w:pgMar w:top="200" w:bottom="200" w:left="200" w:right="200"/></w:sectPr>
      </w:body></w:document>"#
    );
    let styles = format!(
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:styleId="Measured"><w:name w:val="Measured"/><w:pPr><w:spacing {style_spacing} w:line="240" w:lineRule="auto"/></w:pPr></w:style></w:styles>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let relationships = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    for (name, text) in [("word/document.xml", xml), ("word/styles.xml", styles), ("word/_rels/document.xml.rels", relationships.into())] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    let doc = io::open_bytes("spacing.docx", &zip.finish().unwrap().into_inner()).unwrap();
    assert!(doc.styles.get("Measured").is_some(), "the fixture must load its paragraph style");
    doc
}

fn gap(doc: &Document) -> f32 {
    let layout = layout::layout(doc, &mut layout::LayoutCache::new(), &Default::default());
    let lines: Vec<_> = layout.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Placed::Lines { y, para, .. } => Some((*y, para.height)),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 2);
    lines[1].0 - lines[0].0 - lines[0].1
}

#[test]
fn paragraph_line_units_survive_docx_round_trip_without_discarding_point_values() {
    let doc = document(r#"w:before="240" w:after="160" w:beforeLines="150" w:afterLines="25" w:beforeAutospacing="0" w:afterAutospacing="1""#, "");
    let inspect = |doc: &Document| serde_json::to_value(&doc.para(StoryRef::Body, &Path::top(1)).unwrap().props).unwrap();
    let props = inspect(&doc);
    assert_eq!(props["spaceBefore"], 12.0);
    assert_eq!(props["spaceAfter"], 8.0);
    assert_eq!(props["spaceBeforeLines"], 150);
    assert_eq!(props["spaceAfterLines"], 25);
    assert_eq!(props["spaceBeforeAuto"], false);
    assert_eq!(props["spaceAfterAuto"], true);
    let reopened = io::open_bytes("saved.docx", &io::save_bytes("saved.docx", &doc).unwrap()).unwrap();
    assert_eq!(inspect(&reopened), props);
}

#[test]
fn paragraph_line_units_use_nonzero_lines_before_points_and_inherit_independently() {
    // Without a line grid a line unit is 12 pt, whatever the font, as in WPS for Mac.
    let doc = document(r#"w:before="400""#, r#"w:beforeLines="150""#);
    assert!((gap(&doc) - 18.0).abs() < 0.01);
    let zero = document(r#"w:before="400" w:beforeLines="0""#, r#"w:beforeLines="150""#);
    assert!((gap(&zero) - 20.0).abs() < 0.01, "zero line units leave the point constraint active");
}

#[test]
fn paragraph_line_units_affect_spacing_after_and_collapsed_gaps() {
    let mut doc = document(r#"w:beforeLines="100""#, "");
    let mut props = serde_json::to_value(&doc.para(StoryRef::Body, &Path::top(0)).unwrap().props).unwrap();
    props["spaceAfterLines"] = json!(200);
    doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props = serde_json::from_value(props).unwrap();
    assert!((gap(&doc) - 24.0).abs() < 0.01);
}

#[test]
fn paragraph_line_units_can_be_replaced_by_point_spacing_commands() {
    let mut session = Session::new(document(r#"w:beforeLines="200""#, ""));
    session.run("caret.set", &json!({"pos":{"block":1,"off":0}})).unwrap();
    session.run("para.spacing", &json!({"before":7})).unwrap();
    assert!((gap(&session.doc) - 7.0).abs() < 0.01);
}

#[test]
fn paragraph_line_units_use_the_active_grid_pitch() {
    let mut doc = document(r#"w:beforeLines="200""#, "");
    doc.last_section.grid =
        Some(kition_doc_engine::doc::section::DocumentGrid { kind: Some("lines".into()), line_pitch: Some(312), char_space: None });
    assert!((gap(&doc) - 31.2).abs() < 0.01);
}

#[test]
fn paragraph_line_units_toggles_remove_inherited_constraints() {
    let mut session = Session::new(document("", r#"w:beforeLines="200" w:afterLines="100""#));
    session.run("caret.set", &json!({"pos":{"block":1,"off":0}})).unwrap();
    session.run("para.addSpaceBefore", &json!({})).unwrap();
    session.run("para.removeSpaceAfter", &json!({})).unwrap();
    assert!(gap(&session.doc).abs() < 0.01);
    let properties = &session.doc.para(StoryRef::Body, &Path::top(1)).unwrap().props;
    assert_eq!(properties.space_after_lines, Some(0));
    assert_eq!(properties.space_after, Some(0.0));
}

#[test]
fn paragraph_line_units_float_anchor_follows_the_final_paragraph_top() {
    use kition_doc_engine::doc::para::{Anchor, Float, InlineObject, Wrap};

    for wrap in [Wrap::BehindText, Wrap::InFrontOfText] {
        let mut doc = document(r#"w:beforeLines="200""#, "");
        let image = InlineObject::Image {
            edge: Default::default(),
            geom: None,
            media: "anchor-fixture".into(),
            w: 20.0,
            h: 20.0,
            alt: "Synthetic anchor".into(),
            float: Float { wrap, v_rel: Anchor::Paragraph, y: 3.0, ..Default::default() },
            crop: [0.0; 4],
            ole: None,
            adjust: Default::default(),
            uid: 0,
        };
        doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap().insert_object(0, image, &Default::default()).unwrap();
        let result = layout::layout(&doc, &mut layout::LayoutCache::new(), &Default::default());
        let paragraph_y = result.pages[0]
            .items
            .iter()
            .filter_map(|item| match item {
                Placed::Lines { y, .. } => Some(*y),
                _ => None,
            })
            .nth(1)
            .unwrap();
        let image_y = result.pages[0]
            .items
            .iter()
            .find_map(|item| match item {
                Placed::Image { rect, .. } => Some(rect.y),
                _ => None,
            })
            .unwrap();
        // WPS for Mac measures the offset from the paragraph's top above its space before: two
        // 12 pt line units here.
        assert!((image_y - (paragraph_y - 24.0) - 3.0).abs() < 0.01, "{wrap:?}: image={image_y}, paragraph={paragraph_y}");
    }
}
