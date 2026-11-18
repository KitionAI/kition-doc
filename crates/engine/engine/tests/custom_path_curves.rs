use std::io::{Read, Write};

use kition_doc_engine::doc::graphic::PathSeg;
use kition_doc_engine::{Session, doc::InlineObject, io};
use kition_doc_layout::display::{self, Draw};

fn fixture(command: &str) -> Session {
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><w:body><w:p><w:r><w:drawing><wp:anchor><wp:positionH relativeFrom="page"><wp:posOffset>254000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>254000</wp:posOffset></wp:positionV><wp:extent cx="508000" cy="508000"/><wp:wrapNone/><a:graphic><a:graphicData><wps:wsp><wps:spPr><a:custGeom><a:pathLst><a:path w="40" h="40" fill="none"><a:moveTo><a:pt x="0" y="40"/></a:moveTo>{command}</a:path></a:pathLst></a:custGeom><a:noFill/><a:ln w="12700"><a:solidFill><a:srgbClr val="0000FF"/></a:solidFill></a:ln></wps:spPr></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p><w:sectPr><w:pgSz w:w="2000" w:h="2000"/><w:pgMar w:top="200" w:bottom="200" w:left="200" w:right="200"/></w:sectPr></w:body></w:document>"#
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    Session::new(io::open_bytes("curves.docx", &zip.finish().unwrap().into_inner()).unwrap())
}

const CUBIC: &str = r#"<a:cubicBezTo><a:pt x="0" y="0"/><a:pt x="40" y="0"/><a:pt x="40" y="40"/></a:cubicBezTo>"#;
const QUADRATIC: &str = r#"<a:quadBezTo><a:pt x="20" y="0"/><a:pt x="40" y="40"/></a:quadBezTo>"#;

fn check_curve(command: &str, tag: &str, controls: [f32; 6]) {
    let mut session = fixture(command);
    let mut pixels = None;
    for _ in 0..3 {
        let layout = session.export_layout();
        let draws = display::page_display(&session.doc, &layout.pages[0], &Default::default());
        let segments = draws
            .iter()
            .find_map(|draw| match draw {
                Draw::Path { segs, .. } => Some(segs),
                _ => None,
            })
            .unwrap();
        let actual = segments
            .iter()
            .find_map(|segment| match *segment {
                PathSeg::Cubic(a, b, c, d, e, f) => Some([a, b, c, d, e, f]),
                _ => None,
            })
            .expect("a true Bezier curve, not fixed straight pieces");
        for (actual, expected) in actual.into_iter().zip(controls) {
            assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
        }
        let image = kition_doc_render::render_page(&session.doc, &layout.pages[0], 1.0, &Default::default()).to_png();
        if let Some(previous) = &pixels {
            assert_eq!(previous, &image);
        } else {
            pixels = Some(image);
        }
        let bytes = kition_doc_docx::write(&session.doc).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut xml = String::new();
        zip.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
        assert!(xml.contains(&format!("<a:{tag}>")), "saving must retain the source curve command");
        session = Session::new(io::open_bytes("saved.docx", &bytes).unwrap());
    }
}

#[test]
fn custom_curves_keep_cubic_controls_and_pixels_through_repeated_saves() {
    check_curve(CUBIC, "cubicBezTo", [20.0, 20.0, 60.0, 20.0, 60.0, 60.0]);
}

#[test]
fn custom_curves_keep_quadratic_geometry_and_source_commands() {
    check_curve(QUADRATIC, "quadBezTo", [100.0 / 3.0, 100.0 / 3.0, 140.0 / 3.0, 100.0 / 3.0, 60.0, 60.0]);
}

#[test]
fn custom_curves_hit_testing_measures_the_curve_instead_of_its_chords() {
    let session = fixture(CUBIC);
    let InlineObject::Shape { freeform: Some(geometry), .. } = &session.doc.body[0].as_para().unwrap().objects[0] else { panic!("freeform missing") };
    // The cubic at t=0.3: x=40*(3*t*t-2*t*t*t), y=40*((1-t)^3+t^3).
    let distance = geometry.distance(0.0, 0.0, 40.0, 40.0, 8.64, 14.8).unwrap();
    assert!(distance < 0.0001, "curve point is {distance} away");
    assert!(geometry.distance(10.0, 20.0, 80.0, 20.0, 27.28, 27.4).unwrap() < 0.0001);
}

#[test]
fn custom_curves_sanitize_invalid_anchors_and_handle_indices_without_losing_valid_curves() {
    use kition_doc_engine::doc::freeform::{Bezier, BezierControls, FreePath, Freeform};
    let handles = BezierControls::Cubic([[0.0, 0.0], [40.0, 0.0]]);
    let geometry = Freeform {
        w: 40.0,
        h: 40.0,
        paths: vec![FreePath {
            pts: vec![[f32::NAN, 0.0], [0.0, 40.0], [40.0, 40.0]],
            curves: vec![
                Bezier { end: usize::MAX, controls: handles.clone() },
                Bezier { end: 2, controls: BezierControls::Quadratic([f32::INFINITY, 0.0]) },
                Bezier { end: 2, controls: handles },
            ],
            ..Default::default()
        }],
        ..Default::default()
    }
    .sanitized();
    assert!(geometry.distance(0.0, 0.0, 40.0, 40.0, 8.64, 14.8).unwrap() < 0.0001);
    assert_eq!(geometry.distance(0.0, 0.0, 40.0, 40.0, f32::NAN, 14.8), None);
}
