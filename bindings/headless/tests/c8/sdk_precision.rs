//! C8-doc-pin: complete inline inventories allow object-scoped holds.
use super::independent::{base64, png, with_picture};
use super::*;

fn shapes() -> (Vec<u8>, Vec<u64>) {
    let mut doc = document();
    doc.execute("caret.set", json!({ "pos": { "block": 2, "off": 0 } })).unwrap();
    let mut ids = Vec::new();
    for _ in 0..2 {
        let shape = doc.execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "w": 50, "h": 20 } })).unwrap();
        ids.push(shape["id"].as_str().unwrap().parse().unwrap());
    }
    (doc.save(None).unwrap(), ids)
}

#[tokio::test]
async fn a_selected_shape_holds_its_frame_without_other_shapes_in_the_paragraph() {
    let (bytes, ids) = shapes();
    for same in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        f.colleague(&[("drawing.transform", json!({ "ids": [ids[usize::from(!same)]], "rotateBy": 45 }))]);
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let answer = f.run(Some(basis), json!([{ "on": [ids[0]], "command": "arrange.rotation", "params": { "degrees": 90 } }])).await;
        if same {
            conflict(&answer.unwrap_err(), "changed");
            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
        } else {
            answer.unwrap();
            let result = f.disk().execute("drawing.describe", json!({ "ids": ids })).unwrap();
            let text = result.to_string();
            assert!(text.contains("\"rot\":90.0") || text.contains("\"rot\":90,"), "{result}");
            assert!(text.contains("\"rot\":45.0") || text.contains("\"rot\":45,"), "{result}");
        }
        f.close().await;
    }
}

#[tokio::test]
async fn a_shape_named_by_id_does_not_hold_its_paragraph_index() {
    let (bytes, ids) = shapes();
    for same in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        if same {
            f.colleague(&[("drawing.transform", json!({ "ids": [ids[0]], "rotateBy": 45 }))]);
        } else {
            f.colleague(&[("paragraph.insert", json!({ "before": 0, "text": "Peer paragraph" }))]);
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let answer = f.run(Some(basis), json!([{ "on": [ids[0]], "command": "arrange.rotation", "params": { "degrees": 90 } }])).await;
        if same {
            conflict(&answer.unwrap_err(), "changed");
            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
        } else {
            answer.unwrap();
            assert_eq!(f.texts()[0], "Peer paragraph");
            let result = f.disk().execute("drawing.describe", json!({ "ids": [ids[0]] })).unwrap();
            let text = result.to_string();
            assert!(text.contains("\"rot\":90.0") || text.contains("\"rot\":90,"), "{result}");
        }
        f.close().await;
    }
}

#[tokio::test]
async fn bitmap_adjustments_hold_media_but_allow_other_picture_metadata() {
    let (bytes, picture) = with_picture();
    let mut doc = DocDocument::open(FILE, &bytes).unwrap();
    doc.execute("caret.set", json!({ "pos": { "block": 3, "off": 0 } })).unwrap();
    doc.execute("image.insert", json!({ "data": base64(&png([30, 50, 200])), "width": 40 })).unwrap();
    let bytes = doc.save(None).unwrap();
    for same in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        f.colleague(&[
            (
                "select.range",
                json!({ "anchor": { "block": if same { 4 } else { 3 }, "off": 0 }, "focus": { "block": if same { 4 } else { 3 }, "off": 3 } }),
            ),
            (
                if same { "image.color" } else { "image.altText" },
                if same { json!({ "mode": "grayscale" }) } else { json!({ "text": "Peer picture" }) },
            ),
        ]);
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let answer = f.run(Some(basis), json!([{ "on": [picture], "command": "image.color", "params": { "mode": "grayscale" } }])).await;
        if same {
            conflict(&answer.unwrap_err(), "changed");
            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
        } else {
            answer.unwrap();
            let mut archive = zip::ZipArchive::new(std::io::Cursor::new(std::fs::read(f.root.join(FILE)).unwrap())).unwrap();
            let mut colors = Vec::new();
            for index in 0..archive.len() {
                let mut entry = archive.by_index(index).unwrap();
                if entry.name().starts_with("word/media/") {
                    let mut stored = Vec::new();
                    std::io::Read::read_to_end(&mut entry, &mut stored).unwrap();
                    let pixels = image::load_from_memory(&stored).unwrap().to_rgb8();
                    let pixel = pixels.get_pixel(0, 0).0;
                    colors.push(pixel);
                }
            }
            assert_eq!(colors.len(), 2);
            assert_eq!(colors.iter().filter(|pixel| **pixel == [30, 50, 200]).count(), 1);
            assert_eq!(colors.iter().filter(|pixel| pixel[0] == pixel[1] && pixel[1] == pixel[2]).count(), 1);
            let paragraph = f.disk().execute("file.paragraph", json!({ "path": [3] })).unwrap();
            assert!(paragraph.to_string().contains("Peer picture"), "{paragraph}");
        }
        f.close().await;
    }
}

#[tokio::test]
async fn watermark_holds_its_settings_without_the_caret_paragraph() {
    for same in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        if same {
            f.colleague(&[("page.color", json!({ "color": "EECCAA" }))]);
        } else {
            f.colleague(&[
                ("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 5 } })),
                ("format.bold", json!({ "value": true })),
            ]);
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let answer = f.run(Some(basis), json!([{ "command": "page.watermark", "params": { "text": "Agent" } }])).await;
        if same {
            conflict(&answer.unwrap_err(), "changed");
            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
        } else {
            answer.unwrap();
            assert_eq!(f.texts(), TEXTS);
            let paragraph = f.disk().execute("file.paragraph", json!({ "path": [0] })).unwrap();
            assert_eq!(paragraph["paragraph"]["runs"][0]["props"]["bold"], true);
        }
        f.close().await;
    }
}
