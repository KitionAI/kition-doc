use std::io::{Read, Write};
use std::sync::Arc;

use kition_doc_doc::freeform::{FreePath, Freeform};
use kition_doc_doc::para::{Float, GroupChild, ShapeKind};
use kition_doc_doc::props::{CharProps, Rgb};
use kition_doc_doc::{Document, InlineObject, Paragraph, para_block};

fn shape(closed: bool) -> InlineObject {
    InlineObject::Shape {
        vml: None,
        kind: ShapeKind::Freeform,
        w: 40.0,
        h: 30.0,
        fill: closed.then_some(Rgb(50, 90, 130)),
        stroke: Some(Rgb(10, 20, 30)),
        stroke_width: 1.5,
        float: Float::default(),
        story: None,
        freeform: Some(Arc::new(Freeform {
            w: 40.0,
            h: 30.0,
            paths: vec![FreePath { pts: vec![[0.0, 30.0], [20.0, 0.0], [40.0, 30.0]], closed, ..Default::default() }],
            alpha: 0.5,
            ink: None,
        })),
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    }
}

fn grouped(obj: InlineObject) -> InlineObject {
    InlineObject::Group {
        w: 240.0,
        h: 120.0,
        ch_w: 200.0,
        ch_h: 100.0,
        float: Float::default(),
        uid: 0,
        children: vec![GroupChild { x: 15.0, y: 25.0, obj }],
    }
}

fn document(obj: InlineObject) -> Document {
    let mut paragraph = Paragraph::new();
    paragraph.insert_object(0, obj, &CharProps::default()).unwrap();
    let mut doc = Document::new();
    doc.body = vec![para_block(paragraph)];
    doc
}

fn round_trip(doc: &Document) -> Document {
    kition_doc_docx::read(&kition_doc_docx::write(doc).unwrap()).unwrap()
}

fn nested_fixture() -> Vec<u8> {
    // The model flattens imported groups. Build the nested source markup directly,
    // so this test covers import without asking the writer to emit a nested model.
    let bytes = kition_doc_docx::write(&document(grouped(shape(true)))).unwrap();
    let mut source = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for index in 0..source.len() {
        let mut file = source.by_index(index).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        if file.name() == "word/document.xml" {
            let xml = String::from_utf8(data).unwrap();
            let start = r#"<wpg:grpSp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="190500" y="317500"/><a:ext cx="3048000" cy="1524000"/><a:chOff x="0" y="0"/><a:chExt cx="2540000" cy="1270000"/></a:xfrm></wpg:grpSpPr><wps:wsp>"#;
            assert_eq!(xml.matches("<wps:wsp>").count(), 1);
            data = xml.replace("<wps:wsp>", start).replace("</wps:wsp>", "</wps:wsp></wpg:grpSp>").into_bytes();
        }
        output.start_file(file.name(), zip::write::SimpleFileOptions::default()).unwrap();
        output.write_all(&data).unwrap();
    }
    output.finish().unwrap().into_inner()
}

fn assert_geometry(obj: &InlineObject, closed: bool, expected_w: f32, expected_h: f32) {
    let InlineObject::Shape { kind, w, h, freeform: Some(geometry), .. } = obj else {
        panic!("custom geometry missing: {obj:?}");
    };
    assert_eq!(*kind, ShapeKind::Freeform);
    for value in [*w, geometry.w] {
        assert!((value - expected_w).abs() < 0.0001);
    }
    for value in [*h, geometry.h] {
        assert!((value - expected_h).abs() < 0.0001);
    }
    assert_eq!(geometry.paths.len(), 1);
    assert_eq!(geometry.paths[0].closed, closed);
    assert_eq!(geometry.paths[0].pts.len(), 3);
    for (actual, expected) in geometry.paths[0].pts.iter().zip([[0.0, expected_h], [expected_w / 2.0, 0.0], [expected_w, expected_h]]) {
        assert!((actual[0] - expected[0]).abs() < 0.0001);
        assert!((actual[1] - expected[1]).abs() < 0.0001);
    }
    assert_eq!(geometry.alpha, 0.5);
}

#[test]
fn group_freeform_keeps_closed_paths_and_open_strokes_across_repeated_saves() {
    for closed in [true, false] {
        let mut doc = document(grouped(shape(closed)));
        for _ in 0..2 {
            doc = round_trip(&doc);
            let InlineObject::Group { children, w, h, ch_w, ch_h, .. } = &doc.body[0].as_para().unwrap().objects[0] else {
                panic!("group missing");
            };
            assert_eq!((*w, *h, *ch_w, *ch_h), (240.0, 120.0, 200.0, 100.0));
            assert_eq!((children[0].x, children[0].y), (15.0, 25.0));
            assert_geometry(&children[0].obj, closed, 40.0, 30.0);
        }
    }
}

#[test]
fn group_freeform_keeps_geometry_in_nested_groups() {
    let doc = kition_doc_docx::read(&nested_fixture()).unwrap();
    let InlineObject::Group { children, .. } = &doc.body[0].as_para().unwrap().objects[0] else { panic!("group missing") };
    // The reader flattens nested groups into the outer coordinate space.
    assert_eq!(children.len(), 1);
    assert!((children[0].x - 33.0).abs() < 0.0001);
    assert!((children[0].y - 55.0).abs() < 0.0001);
    assert_geometry(&children[0].obj, true, 48.0, 36.0);
}
