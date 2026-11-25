//! C8-doc-3 item 3: what the independent reading (`bindings/wasm/tests/declarations_independent.rs`)
//! found the declarations miss or hold too much of, each class with a writer that really rewrites
//! the file between the agent's observation and its write: one change the write rests on, which
//! must refuse, and one it does not, which must apply. The body is [`TEXTS`].
use super::outside::{by_index, type_at};
use super::*;

/// A rectangle drawn at the caret in block 4, by its number, and the block its anchor is in.
fn with_shape() -> (Vec<u8>, u64, usize) {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 4, "off": 0 }, "focus": { "block": 4, "off": 0 } })).unwrap();
    let shape = doc.execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "w": 50, "h": 20 } })).unwrap();
    let uid = shape["id"].as_str().and_then(|id| id.parse().ok()).or_else(|| shape["id"].as_u64()).unwrap();
    let selection = doc.execute("file.selection", json!({})).unwrap();
    let block = selection["anchor"]["path"][0].as_u64().unwrap() as usize;
    (doc.save(None).unwrap(), uid, block)
}

/// The rectangle's anchor selected (the object the `arrange.*` commands then act on).
fn select_shape(block: usize) -> Value {
    by_index(block, 0, 3)
}

fn rotation(f: &Fixture, uid: u64) -> f64 {
    let mut doc = f.disk();
    let answer = doc.execute("drawing.describe", json!({ "ids": [uid] })).unwrap();
    answer.to_string().split("\"rot\":").nth(1).and_then(|rest| rest.split([',', '}']).next()).and_then(|n| n.parse().ok()).unwrap_or(0.0)
}

/// C8-doc-6: resizing reads the object's size, while moving also reads the page layout.
#[tokio::test]
async fn resizing_an_object_rests_on_its_size_without_the_page_layout() {
    let (bytes, uid, block) = with_shape();
    let write = json!([select_shape(block), { "command": "arrange.bounds", "params": { "width": 100 } }]);
    for outside in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        let steps = [("page.pageSetup", json!({ "section": { "marginTop": 90 } }))];
        if outside {
            f.colleague(&steps);
        } else {
            same_server(&f, &steps).await;
        }
        f.run(Some(basis), write.clone()).await.unwrap();
        f.close().await;

        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        let steps = [("drawing.transform", json!({ "ids": [uid], "rotateBy": 45 }))];
        if outside {
            f.colleague(&steps);
        } else {
            same_server(&f, &steps).await;
        }
        let error = f.run(Some(basis), write.clone()).await.unwrap_err();
        conflict(&error, "changed");
        f.close().await;
    }
}

/// Wrong write (a read that is not held): a section command acts on the section the selection is
/// in, which ends at the first paragraph after it holding a section break. Another writer's break
/// between the selection and the end of the document makes the write land on a section the agent
/// did not see; a change of another paragraph's text does not.
#[tokio::test]
async fn a_section_command_rests_on_the_section_breaks_after_the_selection() {
    // Two sections: blocks 0-3 and the rest (a break at the end of block 3).
    let mut doc = document();
    let end = TEXTS[3].len();
    doc.execute("select.range", json!({ "anchor": { "block": 3, "off": end }, "focus": { "block": 3, "off": end } })).unwrap();
    doc.execute("page.break", json!({ "kind": "continuous" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let f = Fixture::with(Guard::Premise, &bytes);
    let basis = f.basis().await;
    let before = std::fs::read(f.root.join(FILE)).unwrap();
    // The colleague breaks the first section at the end of block 2: block 1 is in a section of
    // its own (blocks 0-2), the agent's orientation would not reach block 3.
    let end = TEXTS[2].len();
    f.colleague(&[
        ("select.range", json!({ "anchor": { "block": 2, "off": end }, "focus": { "block": 2, "off": end } })),
        ("page.break", json!({ "kind": "continuous" })),
    ]);
    let after_colleague = std::fs::read(f.root.join(FILE)).unwrap();
    assert_ne!(before, after_colleague);
    let error =
        f.run(Some(basis), json!([by_index(1, 0, 0), { "command": "page.orientation", "params": { "value": "landscape" } }])).await.unwrap_err();
    conflict(&error, "changed");
    assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), after_colleague, "a refused write leaves the file as it was");
    f.close().await;

    let f = Fixture::with(Guard::Premise, &bytes);
    let basis = f.basis().await;
    f.colleague(&type_at(4, 0, "New "));
    f.run(Some(basis), json!([by_index(1, 0, 0), { "command": "page.orientation", "params": { "value": "landscape" } }])).await.unwrap();
    assert!(f.texts().iter().any(|t| t == "New "), "{:?}", f.texts());
    f.close().await;
}

/// Wrong write (a read that is not held): a command on the selected drawing object reads the
/// object (`arrange.rotate` turns it 90 degrees from where it is). Another agent's rotation of it
/// must refuse; another program's change of another paragraph must not. (Another program's
/// rotation is not told by a reread of the file: the guard is blind to it, see the card's report.)
#[tokio::test]
async fn a_command_on_the_selected_object_rests_on_the_object() {
    let (bytes, uid, block) = with_shape();
    let f = Fixture::with(Guard::Premise, &bytes);
    let basis = f.basis().await;
    let steps = json!([{ "command": "drawing.transform", "params": { "ids": [uid], "rotateBy": 45 } }]);
    let answer = tools::call_as(&f.kits, "doc_run", &json!({ "path": FILE, "steps": steps }), f.options, "c8-colleague").await;
    assert!(matches!(answer, Some(Ok(_))), "{answer:?}");
    let error = f
        .run(Some(basis.clone()), json!([select_shape(block), { "command": "arrange.rotate", "params": { "direction": "right" } }]))
        .await
        .unwrap_err();
    conflict(&error, "changed");
    f.close().await;

    let f = Fixture::with(Guard::Premise, &bytes);
    let basis = f.basis().await;
    f.colleague(&type_at(2, 0, "New "));
    f.run(Some(basis), json!([select_shape(block), { "command": "arrange.rotate", "params": { "direction": "right" } }])).await.unwrap();
    assert!((rotation(&f, uid) - 90.0).abs() < 0.5);
    f.close().await;
}

/// False refusal (a family hold the handler does not read): the page colour is a setting written
/// as given; a paragraph another writer added does not touch it, another page colour does.
#[tokio::test]
async fn the_page_colour_rests_on_the_settings_only() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    let end = TEXTS[3].len();
    f.colleague(&[
        ("select.range", json!({ "anchor": { "block": 3, "off": end }, "focus": { "block": 3, "off": end } })),
        ("text.newParagraph", json!({})),
        ("text.insert", json!({ "text": "Added" })),
    ]);
    f.run(Some(basis), json!([{ "command": "page.color", "params": { "color": "FFEECC" } }])).await.unwrap();
    assert_eq!(f.texts()[4], "Added");
    f.close().await;

    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("page.color", json!({ "color": "CCEEFF" }))]);
    let error = f.run(Some(basis), json!([{ "command": "page.color", "params": { "color": "FFEECC" } }])).await.unwrap_err();
    conflict(&error, "changed");
    f.close().await;
}

/// Another agent on the same server.
async fn same_server(f: &Fixture, steps: &[(&str, Value)]) {
    let steps: Vec<Value> = steps.iter().map(|(command, params)| json!({ "command": command, "params": params })).collect();
    let answer = tools::call_as(&f.kits, "doc_run", &json!({ "path": FILE, "steps": steps }), f.options, "c8-colleague").await;
    assert!(matches!(answer, Some(Ok(_))), "{answer:?}");
}

/// C8-doc-7: document design metadata consumes no caret; themes consume the
/// document's themed formatting, not its core properties or custom properties.
#[tokio::test]
async fn design_commands_hold_their_design_inputs() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, agent, peer, metadata) in [
            ("theme.effects", json!({}), json!({ "name": "Bold" }), true),
            ("theme.paragraphSpacing", json!({ "value": "double" }), json!({ "value": "compact" }), true),
            ("theme.set", json!({ "name": "Studio" }), json!({ "name": "Gallery" }), false),
            ("theme.fonts", json!({ "heading": "Georgia", "body": "Liberation Serif" }), json!({ "heading": "Liberation Mono" }), false),
            ("theme.colors", json!({ "name": "Gallery" }), json!({ "name": "Studio" }), false),
            ("theme.styleSet", json!({ "name": "lines" }), json!({ "name": "basic" }), false),
        ] {
            for necessary in [false, true] {
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = if necessary {
                    vec![(command, peer.clone())]
                } else if metadata {
                    vec![("paragraph.insert", json!({ "before": 0, "text": "Peer paragraph" }))]
                } else {
                    vec![("file.properties", json!({ "title": "Peer title", "custom": { "Peer": "kept" } }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "command": command, "params": agent }])).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: necessary write applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {error}"));
                } else {
                    let saved = f.disk();
                    let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                    if metadata {
                        assert_eq!(f.texts(), [vec!["Peer paragraph".to_owned()], TEXTS.iter().map(|s| s.to_string()).collect()].concat());
                    } else {
                        assert_eq!(f.texts(), TEXTS);
                        assert_eq!(data["core"]["title"], "Peer title");
                        assert!(data["customProps"].as_array().unwrap().iter().any(|p| p["name"] == "Peer" && p["value"] == "kept"));
                    }
                    match command {
                        "theme.effects" => assert_eq!(data["settings"]["themeName"], "Classic (Subtle)"),
                        "theme.paragraphSpacing" => assert_eq!(data["styles"]["defaultPara"]["spaceAfter"], 8.0),
                        "theme.set" => assert_eq!(data["settings"]["themeName"], "Studio"),
                        "theme.fonts" => assert_eq!(data["settings"]["majorFont"], "Georgia"),
                        "theme.colors" => assert_eq!(data["settings"]["themeColors"][4], json!([34, 34, 34])),
                        _ => {
                            let heading = data["styles"]["styles"].as_array().unwrap().iter().find(|s| s["id"] == "Heading1").unwrap();
                            assert!(heading["para"]["borders"]["bottom"].is_object(), "{heading}");
                        }
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: paragraph-property operations locate paragraphs by their places,
/// without consuming character offsets, marks or unrelated style definitions.
#[tokio::test]
async fn paragraph_property_commands_hold_paragraph_inputs() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, params, peer, path, value, direct) in [
            ("paragraph.set", json!({ "props": { "align": "center" } }), json!({ "align": "right" }), "/props/align", json!("center"), true),
            ("paragraph.indents", json!({ "left": 36 }), json!({ "indentLeft": 72 }), "/props/indentLeft", json!(36.0), true),
            ("paragraph.hangingIndent", json!({}), json!({ "indentLeft": 72 }), "/props/indentFirst", json!(-36.0), true),
            ("paragraph.removeHanging", json!({}), json!({ "indentFirst": -72 }), "/props/indentFirst", json!(0.0), true),
            ("paragraph.rtl", json!({}), json!({ "bidi": false }), "/props/bidi", json!(true), true),
            ("paragraph.ltr", json!({}), json!({ "bidi": true }), "/props/bidi", json!(false), true),
            (
                "paragraph.lineSpacing",
                json!({ "value": 1.25 }),
                json!({ "lineSpacing": { "rule": "multiple", "value": 3 } }),
                "/props/lineSpacing/value",
                json!(1.25),
                true,
            ),
            (
                "paragraph.single",
                json!({}),
                json!({ "lineSpacing": { "rule": "multiple", "value": 3 } }),
                "/props/lineSpacing/value",
                json!(1.0),
                true,
            ),
            (
                "paragraph.double",
                json!({}),
                json!({ "lineSpacing": { "rule": "multiple", "value": 3 } }),
                "/props/lineSpacing/value",
                json!(2.0),
                true,
            ),
            (
                "paragraph.oneAndHalf",
                json!({}),
                json!({ "lineSpacing": { "rule": "multiple", "value": 3 } }),
                "/props/lineSpacing/value",
                json!(1.5),
                true,
            ),
            ("paragraph.keepNext", json!({ "value": true }), json!({ "keepNext": false }), "/props/keepNext", json!(true), true),
            ("paragraph.keepLines", json!({ "value": true }), json!({ "keepLines": false }), "/props/keepLines", json!(true), true),
            ("paragraph.pageBreakBefore", json!({ "value": true }), json!({ "pageBreakBefore": false }), "/props/pageBreakBefore", json!(true), true),
            ("paragraph.widowControl", json!({ "value": false }), json!({ "widowControl": true }), "/props/widowControl", json!(false), true),
            ("paragraph.outlineLevel", json!({ "level": 1 }), json!({ "outlineLevel": 2 }), "/props/outlineLevel", json!(1), true),
            ("paragraph.shading", json!({ "color": null }), json!({ "shading": [12, 34, 56] }), "/props/shading", Value::Null, true),
            (
                "paragraph.borders",
                json!({ "kind": "outside" }),
                json!({ "borders": { "top": { "style": "dashed", "width": 2 } } }),
                "/props/borders/top/style",
                json!("single"),
                true,
            ),
            ("paragraph.indent", json!({}), json!({ "indentLeft": 72 }), "/props/indentLeft", json!(36.0), false),
            ("paragraph.outdent", json!({}), json!({ "indentLeft": 72 }), "/props/indentLeft", json!(0.0), false),
            ("paragraph.tabs", json!({ "set": [{ "pos": 72 }] }), json!({ "tabs": [{ "pos": 36 }] }), "/props/tabs/0/pos", json!(72.0), false),
        ] {
            for change in 0..3 {
                if change == 1 && !direct {
                    continue;
                }
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = match change {
                    0 => vec![
                        ("paragraph.text", json!({ "index": 0, "text": "Alpine one two" })),
                        ("select.range", by_index(0, 0, 5)["params"].clone()),
                        ("format.bold", json!({ "value": true })),
                    ],
                    1 => vec![("style.set", json!({ "style": "Normal", "chr": { "italic": true } }))],
                    _ => vec![("paragraph.set", json!({ "props": peer }))],
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "command": command, "params": params }])).await;
                if change == 2 {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: assigned field applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, change {change}, outside {outside}: {}", error["conflicts"]));
                } else {
                    let mut saved = f.disk();
                    let paragraph = saved.execute("file.paragraph", json!({ "path": [0] })).unwrap()["paragraph"].clone();
                    assert_eq!(paragraph.pointer(path).unwrap_or(&Value::Null), &value, "{command}: {paragraph}");
                    if change == 0 {
                        let mut texts: Vec<String> = TEXTS.iter().map(|t| t.to_string()).collect();
                        texts[0] = "Alpine one two".into();
                        assert_eq!(f.texts(), texts);
                        assert_eq!(paragraph["runs"][0]["props"]["bold"], true);
                    } else {
                        assert_eq!(f.texts(), TEXTS);
                        let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                        let normal = data["styles"]["styles"].as_array().unwrap().iter().find(|s| s["id"] == "Normal").unwrap();
                        assert_eq!(normal["chr"]["italic"], true);
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7 M1: these writes assign document/style metadata, so the old selection's
/// place is unrelated. The assigned field still belongs to the observation.
#[tokio::test]
async fn metadata_writes_do_not_hold_the_unused_selection() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, agent, peer) in [
            ("style.set", json!({ "style": "Heading1", "name": "Agent heading" }), json!({ "style": "Heading1", "name": "Peer heading" })),
            ("style.addToGallery", json!({ "style": "Heading1", "value": true }), json!({ "style": "Heading1", "value": false })),
            ("page.color", json!({ "color": "FFEECC" }), json!({ "color": "CCEEFF" })),
        ] {
            for necessary in [false, true] {
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = if necessary {
                    vec![(command, peer.clone())]
                } else {
                    vec![("paragraph.insert", json!({ "before": 0, "text": "Merged paragraph" }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "command": command, "params": agent }])).await;
                if necessary {
                    conflict(&result.unwrap_err(), "changed");
                    assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {error}"));
                } else {
                    assert_eq!(f.texts(), [vec!["Merged paragraph".to_owned()], TEXTS.iter().map(|t| t.to_string()).collect()].concat());
                    let mut saved = f.disk();
                    match command {
                        "style.set" | "style.addToGallery" => {
                            let styles = saved.execute("style.list", json!({})).unwrap();
                            let style = styles.as_array().unwrap().iter().find(|style| style["id"] == "Heading1").unwrap();
                            if command == "style.set" {
                                assert_eq!(style["name"], "Agent heading");
                            } else {
                                assert_eq!(style["quick"], true);
                            }
                        }
                        _ => {
                            use std::io::Read;
                            let mut archive = zip::ZipArchive::new(std::io::Cursor::new(std::fs::read(f.root.join(FILE)).unwrap())).unwrap();
                            let mut xml = String::new();
                            archive.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
                            assert!(xml.contains("FFEECC"), "{xml}");
                        }
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "false refusals: {}", failures.join("\n"));
}

/// C8-doc-7 M3: note number formats and hyphenation consume no page section.
/// Their own assigned settings remain held, even for an absolute assignment.
#[tokio::test]
async fn reference_metadata_does_not_hold_the_document_body() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, agent, peer) in [
            ("footnote.options", json!({ "footnoteFormat": "lowerRoman" }), json!({ "footnoteFormat": "upperRoman" })),
            ("page.hyphenation", json!({ "value": false }), json!({ "value": true })),
        ] {
            for necessary in [false, true] {
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = if necessary { vec![(command, peer.clone())] } else { vec![("page.orientation", json!({ "value": "landscape" }))] };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "command": command, "params": agent }])).await;
                if necessary {
                    if let Err(error) = result {
                        conflict(&error, "changed");
                        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                    } else {
                        failures.push(format!("{command}, outside {outside}: necessary write wrongly applied"));
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {}", error["conflicts"]));
                } else {
                    assert_eq!(f.texts(), TEXTS);
                    let mut saved = f.disk();
                    assert_eq!(saved.execute("page.section", json!({})).unwrap()["landscape"], true);
                    if command == "footnote.options" {
                        assert_eq!(saved.execute(command, json!({})).unwrap()["footnoteFormat"], "lowerRoman");
                    } else {
                        let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                        assert_eq!(data["settings"]["autoHyphenation"], false);
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "false refusals: {}", failures.join("\n"));
}

/// C8-doc-7 M2: paragraph formatting consumes paragraph properties and identity,
/// not the characters/marks at an implicit caret. Keep the paragraph's place too.
#[tokio::test]
async fn paragraph_formatting_does_not_hold_character_formatting() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, params, field, value, peer) in [
            ("paragraph.align", json!({ "value": "right" }), "align", json!("right"), json!({ "align": "center" })),
            ("format.alignCenter", json!({}), "align", json!("center"), json!({ "align": "right" })),
            ("format.alignLeft", json!({}), "align", json!("left"), json!({ "align": "right" })),
            ("format.alignRight", json!({}), "align", json!("right"), json!({ "align": "center" })),
            ("format.justify", json!({}), "align", json!("justify"), json!({ "align": "center" })),
            ("paragraph.distribute", json!({}), "align", json!("distribute"), json!({ "align": "center" })),
            ("paragraph.spacing", json!({ "before": 6, "after": 0 }), "spaceBefore", json!(6.0), json!({ "spaceBefore": 24 })),
            ("paragraph.addSpaceBefore", json!({}), "spaceBefore", json!(12.0), json!({ "spaceBefore": 24 })),
            ("paragraph.removeSpaceAfter", json!({}), "spaceAfter", json!(0.0), json!({ "spaceAfter": 24 })),
            ("paragraph.asianTypography", json!({ "kinsoku": true }), "kinsoku", json!(true), json!({ "kinsoku": false })),
        ] {
            for change in 0..3 {
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = match change {
                    0 => vec![
                        ("paragraph.text", json!({ "index": 0, "text": "Alpine one two" })),
                        ("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 5 } })),
                        ("format.bold", json!({ "value": true })),
                    ],
                    1 => vec![("paragraph.set", json!({ "props": peer }))],
                    _ => vec![("paragraph.insert", json!({ "before": 0, "text": "Above" }))],
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "command": command, "params": params }])).await;
                if change != 0 {
                    conflict(&result.unwrap_err(), if change == 1 { "changed" } else { "relocated" });
                    assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {}", error["conflicts"]));
                } else {
                    let mut texts: Vec<String> = TEXTS.iter().map(|text| text.to_string()).collect();
                    texts[0] = "Alpine one two".to_owned();
                    assert_eq!(f.texts(), texts);
                    let paragraph = f.disk().execute("file.paragraph", json!({ "path": [0] })).unwrap()["paragraph"].clone();
                    assert_eq!(paragraph["runs"][0]["props"]["bold"], true, "{command}: {paragraph}");
                    assert_eq!(paragraph["props"][field], value, "{command}: {paragraph}");
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "false refusals: {}", failures.join("\n"));
}

/// C8-doc-7 M5: assigning a direct character property never resolves a paragraph
/// style. A changed assigned character property is still a conflict.
#[tokio::test]
async fn direct_character_assignments_do_not_hold_the_paragraph_style() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, params, peer, field, value) in [
            ("format.fontName", json!({ "name": "Liberation Serif" }), json!({ "name": "Liberation Mono" }), "font", json!("Liberation Serif")),
            ("format.fontSize", json!({ "size": 18 }), json!({ "size": 24 }), "size", json!(18.0)),
            ("format.highlight", json!({ "color": "yellow" }), json!({ "color": "blue" }), "highlight", json!("yellow")),
            ("format.spacing", json!({ "points": 1 }), json!({ "points": 2 }), "spacing", json!(1.0)),
            ("format.scale", json!({ "percent": 150 }), json!({ "percent": 125 }), "scale", json!(150.0)),
            ("format.position", json!({ "points": 3 }), json!({ "points": 4 }), "position", json!(3.0)),
            ("format.set", json!({ "props": { "bold": true } }), json!({ "props": { "bold": false } }), "bold", json!(true)),
        ] {
            for necessary in [false, true] {
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = if necessary {
                    vec![("select.range", by_index(1, 0, 4)["params"].clone()), (command, peer.clone())]
                } else {
                    vec![("caret.set", json!({ "pos": { "block": 1, "off": 0 } })), ("paragraph.set", json!({ "props": { "style": "Heading1" } }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([by_index(1, 0, 4), { "command": command, "params": params }])).await;
                if necessary {
                    conflict(&result.unwrap_err(), "changed");
                    assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {}", error["conflicts"]));
                } else {
                    assert_eq!(f.texts(), TEXTS);
                    let paragraph = f.disk().execute("file.paragraph", json!({ "path": [1] })).unwrap()["paragraph"].clone();
                    assert_eq!(paragraph["props"]["style"], "Heading1", "{command}: {paragraph}");
                    assert_eq!(paragraph["runs"][0]["props"][field], value, "{command}: {paragraph}");
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "false refusals: {}", failures.join("\n"));
}

#[tokio::test]
async fn a_plain_clipboard_paste_rests_on_the_properties_its_lines_copy() {
    for outside in [false, true] {
        for (block, refuse) in [(3, true), (4, false)] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            let steps = [
                ("select.range", json!({ "anchor": { "block": block, "off": 0 }, "focus": { "block": block, "off": 0 } })),
                ("format.alignCenter", json!({})),
            ];
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f
                .run(
                    Some(basis),
                    json!([
                        { "command": "select.range", "params": { "anchor": { "block": 0, "off": 0 }, "focus": { "block": 1, "off": 4 } } },
                        { "command": "edit.copy", "params": {} },
                        by_index(3, 5, 5),
                        { "command": "edit.pasteMerge", "params": {} }
                    ]),
                )
                .await;
            if refuse {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
                assert_eq!(f.texts().len(), TEXTS.len() + 1);
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn replacing_comment_text_rests_on_the_formatting_it_discards() {
    let (bytes, _) = with_comment();
    let mut reread = DocDocument::open(FILE, &bytes).unwrap();
    let thread = reread.execute("comment.threads", json!({})).unwrap();
    let comment: u32 = thread[0]["comments"][0]["id"].as_str().unwrap().trim_start_matches('c').parse().unwrap();
    let part = (0..32u64)
        .find(|part| {
            reread
                .execute("file.paragraph", json!({ "story": { "part": part }, "path": [0] }))
                .is_ok_and(|answer| answer["paragraph"]["text"] == "Look")
        })
        .unwrap();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let at = if relevant { json!({ "story": { "part": part }, "path": [0], "off": 0 }) } else { json!({ "block": 4, "off": 0 }) };
            let steps = [("caret.set", json!({ "pos": at })), ("select.word", json!({})), ("format.bold", json!({ "value": true }))];
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let before = std::fs::read(f.root.join(FILE)).unwrap();
            let result = f.run(Some(basis), json!([{ "command": "comment.edit", "params": { "id": comment, "text": "Replaced" } }])).await;
            if relevant {
                conflict(&result.unwrap_err(), "changed");
                assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
            } else {
                result.unwrap();
                assert!(f.disk().execute("comment.threads", json!({})).unwrap().to_string().contains("Replaced"));
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn removing_headers_rests_on_nested_body_section_properties() {
    let (bytes, _) = with_header();
    let seed = Fixture::with(Guard::Premise, &bytes);
    seed.call("run", json!({ "steps": [by_index(4, 0, 0), { "command": "table.insert", "params": { "rows": 1, "cols": 1 } }] })).await.unwrap();
    let bytes = std::fs::read(seed.root.join(FILE)).unwrap();
    seed.close().await;
    for outside in [false, true] {
        for relevant in [true, false] {
            let imported = section_in_paragraph(&bytes, false, !outside);
            let f = Fixture::with(Guard::Premise, &imported);
            let path = if outside { json!([0]) } else { json!([4, 0, 0, 0]) };
            let p = f.disk().execute("file.paragraph", json!({ "path": path })).unwrap();
            let id = format!("p{:08X}", p["paragraph"]["props"]["paraId"].as_u64().unwrap());
            if !outside {
                // Served open normalizes cell sections away. Seed the supported live-model
                // path before observing it; the outside route uses a persisted body section.
                let mut meta = f.call("query", json!({ "objects": [id], "fields": ["para"] })).await.unwrap()["objects"][0]["fields"]["para"].clone();
                meta["section"] = f.call("query", json!({ "objects": ["document"], "fields": ["lastSection"] })).await.unwrap()["objects"][0]["fields"]["lastSection"].clone();
                f.call("batch", json!({ "ops": [{ "op": "set", "target": id, "field": "para", "value": meta }] })).await.unwrap();
            }
            let basis = f.basis().await;
            let meta = f.call("query", json!({ "objects": [id], "fields": ["para"] })).await.unwrap()["objects"][0]["fields"]["para"].clone();
            assert!(meta["section"]["headers"]["default"].is_string(), "outside {outside}: {meta}");
            let mut changed = meta.clone();
            changed["section"]["headers"]["first"] = changed["section"]["headers"]["default"].clone();
            changed["section"]["headers"]["default"] = Value::Null;
            if relevant {
                let input = json!({ "path": FILE, "ops": [{ "op": "set", "target": id, "field": "para", "value": changed }] });
                if outside {
                    f.rewrite(&section_in_paragraph(&bytes, true, false));
                } else {
                    let answer = tools::call_as(&f.kits, "doc_batch", &input, f.options, "c8-colleague").await;
                    assert!(matches!(answer, Some(Ok(_))), "{answer:?}");
                }
            } else if outside {
                f.colleague(&type_at(5, 0, "Other "));
            } else {
                same_server(&f, &type_at(5, 0, "Other ")).await;
            }
            let result = f.run(Some(basis), json!([{ "command": "header.remove", "params": {} }])).await;
            if relevant {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
}

// Insert our own section XML in a cell or top-level paragraph for the import probes.
fn section_in_paragraph(bytes: &[u8], first: bool, nested: bool) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut input = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut entry = input.by_index(i).unwrap();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        if entry.name() == "word/document.xml" {
            let mut xml = String::from_utf8(content).unwrap();
            let start = xml.rfind("<w:sectPr>").unwrap();
            let end = start + xml[start..].find("</w:sectPr>").unwrap() + "</w:sectPr>".len();
            let mut section = xml[start..end].to_owned();
            if first {
                section = section.replace("w:type=\"default\"", "w:type=\"first\"");
            }
            let cell = if nested { xml.find("<w:tc>").unwrap() } else { 0 };
            let para = cell + xml[cell..].find("<w:p ").unwrap();
            let insert = para + xml[para..].find('>').unwrap() + 1;
            if xml[insert..].starts_with("<w:pPr>") {
                let after = insert + xml[insert..].find("</w:pPr>").unwrap();
                let before = insert + "<w:pPr>".len();
                if let Some(at) = xml[before..after].find("<w:sectPr>") {
                    xml.replace_range(before + at..after, &section);
                } else {
                    xml.insert_str(after, &section);
                }
            } else {
                xml.insert_str(insert, &format!("<w:pPr>{section}</w:pPr>"));
            }
            content = xml.into_bytes();
        }
        output.start_file(entry.name(), zip::write::SimpleFileOptions::default()).unwrap();
        output.write_all(&content).unwrap();
    }
    output.finish().unwrap().into_inner()
}

#[tokio::test]
async fn formatting_and_replacing_drawing_text_rest_on_the_frame_they_validate() {
    let (bytes, uid, _) = with_shape();
    for command in ["drawing.format", "drawing.text"] {
        let params =
            if command == "drawing.format" { json!({ "ids": [uid], "fill": { "color": "#FF0000" } }) } else { json!({ "id": uid, "text": "Hi" }) };
        let mut probe = DocDocument::open(FILE, &bytes).unwrap();
        probe.execute("page.pageSetup", json!({ "section": { "marginLeft": 200000 } })).unwrap();
        assert!(probe.execute(command, params.clone()).is_err(), "hostile section must make the observed frame invalid");
        for outside in [false, true] {
            for change in ["validity", "title", "paragraph order"] {
                let f = Fixture::with(Guard::Premise, &bytes);
                if change == "validity" {
                    // The live model accepts hostile section geometry; the served file clips
                    // its lengths. Observe before a colleague makes the frame valid again.
                    let mut section = f.call("query", json!({ "objects": ["document"], "fields": ["lastSection"] })).await.unwrap()["objects"][0]["fields"]["lastSection"].clone();
                    section["marginLeft"] = json!(200000);
                    f.call("batch", json!({ "ops": [{ "op": "set", "target": "document", "field": "lastSection", "value": section }] }))
                        .await
                        .unwrap();
                }
                let basis = f.basis().await;
                let steps = match change {
                    "validity" => vec![("page.pageSetup", json!({ "section": { "marginLeft": 90 } }))],
                    "title" => vec![("file.properties", json!({ "title": "Unrelated" }))],
                    _ => vec![
                        ("select.range", json!({ "anchor": { "block": 2, "off": 0 }, "focus": { "block": 3, "off": 0 } })),
                        ("edit.cut", json!({})),
                        ("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 0 } })),
                        ("edit.paste", json!({})),
                    ],
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let result = f.run(Some(basis), json!([{ "command": command, "params": params }])).await;
                if change == "validity" {
                    conflict(&result.unwrap_err(), "changed");
                } else {
                    result.unwrap();
                }
                f.close().await;
            }
        }
    }
}

#[tokio::test]
async fn converting_all_equations_rests_on_each_equations_value() {
    let bytes = with_equation();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = if relevant {
                vec![
                    ("select.range", json!({ "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 3 } })),
                    ("equation.set", json!({ "linear": "y^3" })),
                ]
            } else {
                vec![("file.properties", json!({ "title": "Unrelated" }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([{ "command": "equation.convert", "params": { "to": "professional", "all": true } }])).await;
            if relevant {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn converting_all_equations_rests_on_value_only_edits() {
    let bytes = with_equation();
    let alternate = Fixture::with(Guard::Premise, &bytes);
    alternate.run(None, json!([by_index(1, 0, 3), { "command": "equation.set", "params": { "linear": "z^4" } }])).await.unwrap();
    let value = alternate.call("query", json!({ "objects": [1000001], "fields": ["value"] })).await.unwrap()["objects"][0]["fields"]["value"].clone();
    assert!(value.is_object());
    alternate.close().await;
    for outside in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        let input = json!({ "path": FILE, "ops": [{ "op": "set", "target": 1000001, "field": "value", "value": value }] });
        if outside {
            let other = Fixture::with(Guard::Premise, &bytes);
            other.call("batch", input).await.unwrap();
            f.rewrite(&std::fs::read(other.root.join(FILE)).unwrap());
            other.close().await;
        } else {
            let answer = tools::call_as(&f.kits, "doc_batch", &input, f.options, "c8-colleague").await;
            assert!(matches!(answer, Some(Ok(_))), "{answer:?}");
        }
        let result = f.run(Some(basis), json!([{ "command": "equation.convert", "params": { "to": "professional", "all": true } }])).await;
        conflict(&result.unwrap_err(), "changed");
        f.close().await;
    }
}

#[tokio::test]
async fn comparing_rests_on_the_table_contents_it_discards() {
    let bytes = super::holds::table_document();
    for outside in [false, true] {
        for command in ["file.compare", "file.combine"] {
            for relevant in [true, false] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let steps = if relevant {
                    vec![
                        ("caret.set", json!({ "pos": { "story": "body", "path": [4, 0, 0, 0], "off": 0 } })),
                        ("text.insert", json!({ "text": "New " })),
                    ]
                } else {
                    vec![("file.properties", json!({ "title": "Unrelated" }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let error = f.run(Some(basis), json!([{ "command": command, "params": { "text": "Revised" } }])).await.unwrap_err();
                if relevant {
                    conflict(&error, "changed");
                } else {
                    assert_eq!(error["error"]["code"], "command-failed", "{error}");
                }
                f.close().await;
            }
        }
    }
}

#[tokio::test]
async fn a_connector_rests_on_its_positioned_destination() {
    let (bytes, ids) = floating_endpoints();
    let mut doc = DocDocument::open(FILE, &bytes).unwrap();
    for (block, x) in [(1, 100), (2, 300)] {
        doc.execute("select.range", json!({ "anchor": { "block": block, "off": 0 }, "focus": { "block": block, "off": 3 } })).unwrap();
        doc.execute("arrange.position", json!({ "x": x, "y": 300 })).unwrap();
    }
    let bytes = doc.save(None).unwrap();
    let mut projected = DocDocument::open(FILE, &bytes).unwrap();
    projected.execute("drawing.connect", json!({ "from": ids[0], "to": ids[1] })).unwrap();
    let destination = projected.execute("file.selection", json!({})).unwrap()["anchor"]["path"][0].as_u64().unwrap() as usize;
    assert!(![0, 1, 2].contains(&destination), "destination {destination}");
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let block = if relevant { destination } else { 3 };
            let steps = [
                ("select.range", json!({ "anchor": { "block": block, "off": 0 }, "focus": { "block": block, "off": 1 } })),
                ("format.italic", json!({ "value": true })),
            ];
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([{ "command": "drawing.connect", "params": { "from": ids[0], "to": ids[1] } }])).await;
            if relevant {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn rejection_navigation_rests_on_table_format_changes() {
    let mut doc = DocDocument::open(FILE, &super::holds::table_document()).unwrap();
    doc.execute("change.track", json!({ "value": true })).unwrap();
    doc.execute("caret.set", json!({ "pos": { "story": "body", "path": [4, 0, 0, 0], "off": 0 } })).unwrap();
    doc.execute("table.properties", json!({ "align": "center" })).unwrap();
    doc.execute("change.track", json!({ "value": false })).unwrap();
    let bytes = doc.save(None).unwrap();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = if relevant {
                vec![
                    ("caret.set", json!({ "pos": { "story": "body", "path": [4, 0, 0, 0], "off": 0 } })),
                    ("table.properties", json!({ "align": "right" })),
                ]
            } else {
                vec![("file.properties", json!({ "title": "Unrelated" }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([by_index(6, 0, 0), { "command": "change.reject", "params": {} }])).await;
            if relevant {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn moving_a_drawing_rests_on_the_layout_at_its_destination() {
    let (bytes, uid, _) = with_shape();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = if relevant {
                vec![("caret.set", json!({ "pos": { "block": 5, "off": 0 } })), ("paragraph.set", json!({ "props": { "spaceBefore": 80 } }))]
            } else {
                vec![("file.properties", json!({ "title": "Unrelated" }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([{ "command": "drawing.transform", "params": { "ids": [uid], "dx": 20, "dy": 200 } }])).await;
            if relevant {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn text_to_table_rests_on_the_existing_table_it_discards() {
    let bytes = super::holds::table_document();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = if relevant {
                vec![("caret.set", json!({ "pos": { "story": "body", "path": [4, 0, 0, 0], "off": 0 } })), ("text.insert", json!({ "text": "New " }))]
            } else {
                vec![("file.properties", json!({ "title": "Unrelated" }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f
                .run(
                    Some(basis),
                    json!([
                        { "command": "select.range", "params": { "anchor": { "block": 3, "off": 0 }, "focus": { "block": 5, "off": 0 } } },
                        { "command": "table.fromText", "params": {} }
                    ]),
                )
                .await;
            if relevant {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn text_to_table_rests_on_the_existing_table_properties_it_discards() {
    let bytes = super::holds::table_document();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = if relevant {
                vec![
                    ("caret.set", json!({ "pos": { "story": "body", "path": [4, 0, 0, 0], "off": 0 } })),
                    ("table.properties", json!({ "align": "center" })),
                ]
            } else {
                vec![("file.properties", json!({ "title": "Unrelated" }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f
                .run(
                    Some(basis),
                    json!([
                        { "command": "select.range", "params": { "anchor": { "block": 3, "off": 0 }, "focus": { "block": 5, "off": 0 } } },
                        { "command": "table.fromText", "params": {} }
                    ]),
                )
                .await;
            if relevant {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn inspector_removals_rest_on_unanchored_comments_they_count() {
    let (bytes, _) = with_comment();
    let mut doc = DocDocument::open(FILE, &bytes).unwrap();
    doc.execute("paragraph.text", json!({ "index": 2, "text": "" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let threads = DocDocument::open(FILE, &bytes).unwrap().execute("comment.threads", json!({})).unwrap();
    assert_eq!(threads.as_array().unwrap().len(), 1, "{threads}");
    for command in ["file.removeHiddenContent", "comment.deleteCurrent"] {
        for outside in [false, true] {
            for relevant in [true, false] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let steps = if relevant {
                    vec![("comment.replyCurrent", json!({ "id": 0, "text": "Unanchored reply" }))]
                } else {
                    vec![("file.properties", json!({ "title": "Unrelated" }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let params = if command == "file.removeHiddenContent" { json!({ "remove": ["hidden"] }) } else { json!({ "all": true }) };
                let result = f.run(Some(basis), json!([{ "command": command, "params": params }])).await;
                if relevant {
                    conflict(&result.unwrap_err(), "changed");
                } else {
                    result.unwrap();
                }
                f.close().await;
            }
        }
    }
}

#[tokio::test]
async fn inspector_removals_rest_on_the_comment_count_they_report() {
    let (bytes, _) = with_comment();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = if relevant {
                vec![("comment.replyCurrent", json!({ "id": 0, "text": "Another reply" }))]
            } else {
                vec![("file.properties", json!({ "title": "Unrelated" }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([{ "command": "file.removeHiddenContent", "params": { "remove": ["hidden"] } }])).await;
            if relevant {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn deleting_a_reply_rests_on_its_discarded_initials() {
    let (bytes, _) = with_comment();
    let mut doc = DocDocument::open(FILE, &bytes).unwrap();
    doc.execute("comment.replyCurrent", json!({ "id": 0, "text": "Reply" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let threads = DocDocument::open(FILE, &bytes).unwrap().execute("comment.threads", json!({})).unwrap();
    let reply = threads[0]["comments"][1]["toolId"].clone();
    assert!(reply.as_str().is_some_and(|id| id.starts_with("comment:")), "{threads}");
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            if relevant {
                let input = json!({ "path": FILE, "ops": [{ "op": "set", "target": reply, "field": "initials", "value": "ZZ" }] });
                if outside {
                    let other = Fixture::with(Guard::Premise, &bytes);
                    other.call("batch", input).await.unwrap();
                    f.rewrite(&std::fs::read(other.root.join(FILE)).unwrap());
                    other.close().await;
                } else {
                    let answer = tools::call_as(&f.kits, "doc_batch", &input, f.options, "c8-colleague").await;
                    assert!(matches!(answer, Some(Ok(_))), "{answer:?}");
                }
            } else if outside {
                f.colleague(&[("file.properties", json!({ "title": "Unrelated" }))]);
            } else {
                same_server(&f, &[("file.properties", json!({ "title": "Unrelated" }))]).await;
            }
            let result = f.run(Some(basis), json!([{ "command": "comment.deleteCurrent", "params": { "id": 1 } }])).await;
            if relevant {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn deleting_comments_rests_on_the_formatting_they_discard() {
    let (bytes, _) = with_comment();
    let mut doc = DocDocument::open(FILE, &bytes).unwrap();
    doc.execute("comment.replyCurrent", json!({ "id": 0, "text": "Reply" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut doc = DocDocument::open(FILE, &bytes).unwrap();
    let part = (0..32u64)
        .find(|part| {
            doc.execute("file.paragraph", json!({ "story": { "part": part }, "path": [0] })).is_ok_and(|v| v["paragraph"]["text"] == "Reply")
        })
        .unwrap();
    for command in ["comment.deleteCurrent", "file.removeHiddenContent"] {
        for outside in [false, true] {
            for relevant in [true, false] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let steps = if relevant {
                    vec![
                        ("caret.set", json!({ "pos": { "story": { "part": part }, "path": [0], "off": 0 } })),
                        ("select.word", json!({})),
                        ("format.bold", json!({ "value": true })),
                    ]
                } else {
                    vec![("file.properties", json!({ "title": "Unrelated" }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let params = if command == "file.removeHiddenContent" { json!({ "remove": ["comments"] }) } else { json!({ "id": 1 }) };
                let result = f.run(Some(basis), json!([{ "command": command, "params": params }])).await;
                if relevant {
                    conflict(&result.unwrap_err(), "changed");
                } else {
                    result.unwrap();
                }
                f.close().await;
            }
        }
    }
}

#[tokio::test]
async fn paragraph_splitting_rests_on_the_properties_it_copies() {
    let write = json!([by_index(1, 5, 5), { "command": "text.newParagraph", "params": {} }]);
    for outside in [false, true] {
        for (block, refuse) in [(1, true), (4, false)] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            let steps = [
                ("select.range", json!({ "anchor": { "block": block, "off": 0 }, "focus": { "block": block, "off": 0 } })),
                ("format.alignCenter", json!({})),
            ];
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), write.clone()).await;
            if refuse {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
                assert_eq!(f.texts().len(), TEXTS.len() + 1);
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn paragraph_joining_rests_on_the_previous_paragraph() {
    let write = json!([by_index(2, 0, 0), { "command": "text.backspace", "params": {} }]);
    for outside in [false, true] {
        for (block, refuse) in [(1, true), (4, false)] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            let steps = type_at(block, 0, "New ");
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), write.clone()).await;
            if refuse {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
                assert_eq!(f.texts().len(), TEXTS.len() - 1);
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn pasted_paragraph_delimiters_rest_on_the_properties_they_copy() {
    for outside in [false, true] {
        for (text, block, refuse) in [("a\rb", 1, true), ("a\nb", 1, true), ("a\u{2029}b", 1, true), ("a\rb", 4, false)] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            let steps = [
                ("select.range", json!({ "anchor": { "block": block, "off": 0 }, "focus": { "block": block, "off": 0 } })),
                ("format.alignCenter", json!({})),
            ];
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([by_index(1, 5, 5), { "command": "edit.pasteText", "params": { "text": text } }])).await;
            if refuse {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
                assert_eq!(f.texts().len(), TEXTS.len() + 1);
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn paragraph_sorting_rests_on_the_whole_last_paragraph() {
    let write = json!([
        { "command": "select.range", "params": { "anchor": { "block": 1, "off": 0 }, "focus": { "block": 3, "off": 0 } } },
        { "command": "paragraph.sort", "params": {} }
    ]);
    for outside in [false, true] {
        for (block, refuse) in [(3, true), (4, false)] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            let steps = type_at(block, 3, "New ");
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), write.clone()).await;
            if refuse {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
                assert!(f.texts()[4].contains("New "));
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn paragraph_splitting_rests_on_the_next_style_it_resolves() {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 0 } })).unwrap();
    doc.execute("paragraph.style", json!({ "style": "Heading1" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let write = json!([by_index(1, TEXTS[1].len(), TEXTS[1].len()), { "command": "text.newParagraph", "params": {} }]);
    for outside in [false, true] {
        for (style, refuse) in [("Heading1", true), ("Heading2", false)] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = [("style.set", json!({ "style": style, "next": "Title" }))];
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), write.clone()).await;
            if refuse {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn a_table_stroke_rests_on_the_cells_at_its_point() {
    let mut doc = DocDocument::create(FILE, 612, 792).unwrap();
    doc.execute("table.insert", json!({ "rows": 2, "cols": 2 })).unwrap();
    let mut carets = Vec::new();
    for col in [0, 1] {
        let pos = json!({ "story": "body", "path": [0, 0, col, 0], "off": 0 });
        doc.execute("select.range", json!({ "anchor": pos, "focus": pos })).unwrap();
        doc.execute("text.insert", json!({ "text": "Cell" })).unwrap();
        carets.push(doc.execute("file.layout", json!({})).unwrap()["caret"].clone());
    }
    let x = (carets[0]["x"].as_f64().unwrap() + carets[1]["x"].as_f64().unwrap()) / 2.0;
    let y = carets[0]["y"].as_f64().unwrap();
    let pos = json!({ "block": 1, "off": 0 });
    doc.execute("select.range", json!({ "anchor": pos, "focus": pos })).unwrap();
    doc.execute("text.insert", json!({ "text": "Outside" })).unwrap();
    doc.execute("text.newParagraph", json!({})).unwrap();
    doc.execute("text.insert", json!({ "text": "Distant" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let write = json!([by_index(2, 0, 0), { "command": "table.draw", "params": { "page": 0, "x0": x, "x1": x, "y0": y, "y1": y + 12.0 } }]);
    for outside in [false, true] {
        for (path, refuse) in [(vec![0, 0, 0, 0], true), (vec![2], false)] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let pos = json!({ "story": "body", "path": path, "off": 0 });
            let steps = [("select.range", json!({ "anchor": pos, "focus": pos })), ("text.insert", json!({ "text": "New " }))];
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), write.clone()).await;
            if refuse {
                conflict(&result.unwrap_err(), "changed");
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn whole_story_commands_rest_on_the_paragraphs_they_scan() {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 0 } })).unwrap();
    doc.execute("style.add", json!({ "name": "Mine", "fromSelection": false })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for command in ["style.delete", "file.removeHiddenContent", "bookmark.insert"] {
            let params = match command {
                "style.delete" => json!({ "style": "Mine" }),
                "file.removeHiddenContent" => json!({ "remove": ["hidden"] }),
                _ => json!({ "name": "Shared" }),
            };
            for relevant in [true, false] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let edit = match (command, relevant) {
                    ("bookmark.insert", false) => ("text.insert", json!({ "text": "Unrelated " })),
                    (_, false) => ("file.properties", json!({ "title": "Another title" })),
                    ("style.delete", true) => ("paragraph.style", json!({ "style": "Mine" })),
                    ("file.removeHiddenContent", true) => ("format.hidden", json!({ "value": true })),
                    _ => ("bookmark.insert", json!({ "name": "Shared" })),
                };
                let pos = json!({ "block": 3, "off": 0 });
                let end = json!({ "block": 3, "off": 3 });
                let steps = [("select.range", json!({ "anchor": pos, "focus": end })), edit];
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let write = json!([by_index(1, 0, 0), { "command": command, "params": params }]);
                let result = f.run(Some(basis), write).await;
                if relevant {
                    match result {
                        Ok(_) => failures.push(format!("{command}: applied after a relevant edit; outside={outside}")),
                        Err(error) => conflict(&error, "changed"),
                    }
                } else {
                    if let Err(error) = result {
                        failures.push(format!("{command}: refused the unrelated edit; outside={outside}: {error:?}"));
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn rejection_rests_on_paragraph_changes_beyond_its_text_range() {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 3, "off": 0 }, "focus": { "block": 3, "off": 0 } })).unwrap();
    doc.execute("change.track", json!({ "value": true })).unwrap();
    doc.execute("text.insert", json!({ "text": "Tracked " })).unwrap();
    doc.execute("change.track", json!({ "value": false })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for fallback in [false, true] {
            for relevant in [false, true] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let block = if relevant { if fallback { 3 } else { 1 } } else { 4 };
                let mut steps = vec![("select.range", json!({ "anchor": { "block": block, "off": 0 }, "focus": { "block": block, "off": 0 } }))];
                if fallback && relevant {
                    steps.push(("text.insert", json!({ "text": "Shift " })));
                } else if fallback {
                    steps.push(("file.properties", json!({ "title": "Unrelated" })));
                } else {
                    steps.push(("format.alignCenter", json!({})));
                }
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                // At a caret with no revision Reject navigates to the next body change.
                let write = json!([by_index(1, 0, if fallback { 0 } else { 3 }), { "command": "change.reject", "params": {} }]);
                let result = f.run(Some(basis), write).await;
                if relevant {
                    match result {
                        Ok(_) => failures.push(format!("Reject ignored paragraph changes; outside={outside}, fallback={fallback}")),
                        Err(error) => conflict(&error, "changed"),
                    }
                } else {
                    result.unwrap();
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn equation_enter_rests_on_the_paragraph_it_splits() {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 0 } })).unwrap();
    doc.execute("equation.insert", json!({ "linear": "x", "edit": false })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let block = if relevant { 1 } else { 4 };
            let steps = [
                ("select.range", json!({ "anchor": { "block": block, "off": 0 }, "focus": { "block": block, "off": 0 } })),
                ("format.alignCenter", json!({})),
            ];
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f
                .run(
                    Some(basis),
                    json!([
                        by_index(1, 0, 0),
                        { "command": "equation.edit", "params": {} },
                        { "command": "equation.enter", "params": {} }
                    ]),
                )
                .await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("Equation Enter ignored copied properties; outside={outside}")),
                    Err(error) => conflict(&error, "changed"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn typing_autoformats_rest_on_numbering_and_the_previous_border() {
    let mut doc = document();
    doc.execute("paragraph.text", json!({ "index": 1, "text": "1." })).unwrap();
    doc.execute("paragraph.text", json!({ "index": 3, "text": "--" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for border in [false, true] {
            for relevant in [true, false] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let block = if relevant { if border { 2 } else { 1 } } else { 4 };
                let steps = [
                    ("select.range", json!({ "anchor": { "block": block, "off": 0 }, "focus": { "block": block, "off": 0 } })),
                    (
                        if !relevant {
                            "format.alignCenter"
                        } else if border {
                            "paragraph.borders"
                        } else {
                            "paragraph.bullets"
                        },
                        if border && relevant { json!({ "kind": "top" }) } else { json!({}) },
                    ),
                ];
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let write = if border {
                    json!([by_index(3, 2, 2), { "command": "text.insert", "params": { "text": "-" } }, { "command": "text.newParagraph", "params": {} }])
                } else {
                    json!([by_index(1, 2, 2), { "command": "text.insert", "params": { "text": " " } }])
                };
                let result = f.run(Some(basis), write).await;
                if relevant {
                    match result {
                        Ok(_) => failures.push(format!("AutoFormat applied after relevant properties changed; border={border}, outside={outside}")),
                        Err(error) => conflict(&error, "changed"),
                    }
                } else {
                    result.unwrap();
                    assert!(f.texts()[if border { 3 } else { 1 }].is_empty(), "AutoFormat did not consume its marker");
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn comparing_rests_on_body_paragraphs_and_not_document_properties() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for command in ["file.compare", "file.combine"] {
            for relevant in [true, false] {
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = if relevant {
                    vec![
                        ("select.range", json!({ "anchor": { "block": 3, "off": 0 }, "focus": { "block": 3, "off": 0 } })),
                        ("format.alignCenter", json!({})),
                    ]
                } else {
                    vec![("file.properties", json!({ "title": "Colleague title" }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let result = f.run(Some(basis), json!([{ "command": command, "params": { "text": TEXTS.join("\n") } }])).await;
                if relevant {
                    match result {
                        Ok(_) => failures.push(format!("{command} applied after body formatting changed; outside={outside}")),
                        Err(error) => conflict(&error, "changed"),
                    }
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(error["error"]["code"], "command-failed", "{error}");
                    assert!(error["error"]["message"].as_str().unwrap().contains("command disabled"), "{error}");
                    assert_eq!(f.texts(), TEXTS);
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn fragment_pasting_rests_on_ids_without_unrelated_formatting() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for html in [false, true] {
            for relevant in [true, false] {
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = if relevant {
                    vec![("paragraph.insert", json!({ "before": 4, "text": "New id" }))]
                } else {
                    vec![
                        ("select.range", json!({ "anchor": { "block": 4, "off": 0 }, "focus": { "block": 4, "off": 0 } })),
                        ("format.alignCenter", json!({})),
                    ]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let write = if html {
                    json!([by_index(1, 0, 0), { "command": "edit.pasteSpecial", "params": { "as": "html", "html": "<p>x</p><p>y</p>" } }])
                } else {
                    json!([by_index(2, 0, 3), { "command": "edit.copy", "params": {} }, by_index(1, 0, 0), { "command": "edit.paste", "params": {} }])
                };
                let result = f.run(Some(basis), write).await;
                if relevant {
                    match result {
                        Ok(_) => failures.push(format!("fragment paste applied after the id set changed; html={html}, outside={outside}")),
                        Err(error) => conflict(&error, "changed"),
                    }
                } else {
                    result.unwrap();
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn an_equation_structure_name_does_not_replace_its_caret_premise() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            let block = if relevant { 0 } else { 4 };
            let steps = [
                ("select.range", json!({ "anchor": { "block": block, "off": 0 }, "focus": { "block": block, "off": 1 } })),
                ("format.italic", json!({})),
            ];
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([{ "command": "equation.insertStructure", "params": { "id": "frac.stacked" } }])).await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("built-in structure inserted after caret formatting changed; outside={outside}")),
                    Err(error) => conflict(&error, "changed"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn inserting_a_drawing_rests_on_the_id_scan() {
    let mut probe = document();
    let before = probe.derived(&[json!("document")], &["drawingIds".into()]).unwrap();
    probe.execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "w": 20, "h": 20 } })).unwrap();
    let after = probe.derived(&[json!("document")], &["drawingIds".into()]).unwrap();
    assert_ne!(serde_json::to_value(before).unwrap(), serde_json::to_value(after).unwrap());
    let all = probe.derived(&[], &[]).unwrap();
    assert_eq!(all.iter().filter(|v| v.id == "document").count(), 1);
    let under = probe.under("drawing.insert", &json!({ "geometry": { "preset": "ellipse" } })).unwrap().unwrap();
    assert!(
        under.premises.iter().any(|p| p.id == "document" && p.fields.as_ref().is_some_and(|f| f.iter().any(|f| f == "~drawingIds"))),
        "{under:?}"
    );
    let mut failures = Vec::new();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            let steps = if relevant {
                vec![
                    ("caret.set", json!({ "pos": { "block": 4, "off": 0 } })),
                    ("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "w": 20, "h": 20 } })),
                ]
            } else {
                vec![("file.properties", json!({ "title": "Unrelated" }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f
                .run(Some(basis), json!([by_index(0, 0, 0), { "command": "drawing.insert", "params": { "geometry": { "preset": "ellipse" } } }]))
                .await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("drawing inserted after the id set changed; outside={outside}")),
                    Err(error) => conflict(&error, "changed"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn floating_endpoints() -> (Vec<u8>, Vec<Value>) {
    let mut doc = document();
    let mut ids = Vec::new();
    for block in [1, 2] {
        doc.execute("caret.set", json!({ "pos": { "block": block, "off": 0 } })).unwrap();
        let shape = doc.execute("drawing.insert", json!({ "geometry": { "preset": "rect" } })).unwrap();
        ids.push(json!(shape["id"].as_str().unwrap().parse::<u32>().unwrap()));
        doc.execute("arrange.wrap", json!({ "wrap": "square" })).unwrap();
    }
    (doc.save(None).unwrap(), ids)
}

#[tokio::test]
async fn grouping_rests_on_every_selected_anchor() {
    let (bytes, ids) = floating_endpoints();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = type_at(if relevant { 2 } else { 4 }, 0, "Changed ");
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([{ "on": ids, "command": "arrange.group", "params": {} }])).await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("grouped after its secondary anchor changed; outside={outside}")),
                    Err(error) => conflict(&error, "changed"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn a_drawing_insert_rests_on_its_destination_text() {
    let mut doc = document();
    let pos = json!({ "block": 0, "off": 0 });
    doc.execute("caret.set", json!({ "pos": pos })).unwrap();
    let params = json!({ "geometry": { "preset": "rect" }, "frame": { "x": 100, "y": 300, "w": 60, "h": 30 }, "placement": { "page": 0 } });
    doc.execute("drawing.insert", params.clone()).unwrap();
    let destination = doc.execute("file.selection", json!({})).unwrap()["anchor"]["path"][0].as_u64().unwrap();
    assert_ne!(destination, 0);
    let bytes = document().save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let at = json!({ "block": destination, "off": 0 });
            let end = json!({ "block": destination, "off": 1 });
            let steps = if relevant {
                vec![("select.range", json!({ "anchor": at, "focus": end })), ("format.underline", json!({}))]
            } else {
                vec![("file.properties", json!({ "title": "Unrelated" }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([by_index(0, 0, 0), { "command": "drawing.insert", "params": params }])).await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("inserted over changed destination text; outside={outside}")),
                    Err(error) => conflict(&error, "changed"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn an_eraser_rests_on_the_ink_geometry_even_when_a_hit_disappears() {
    let mut doc = document();
    let result = doc
        .execute(
            "drawing.ink",
            json!({ "placement": { "page": 0 }, "strokes": [{
        "tool": "pen", "color": "C00000", "width": 60, "points": [[100, 100], [200, 100]]
    }] }),
        )
        .unwrap();
    assert_eq!(result["ids"].as_array().unwrap().len(), 1);
    let bytes = doc.save(None).unwrap();
    let erase = json!({ "placement": { "page": 0 }, "path": [[150, 120]] });
    assert_eq!(doc.execute("drawing.erase", erase.clone()).unwrap()["erased"], 1);
    let mut doc = DocDocument::open(FILE, &bytes).unwrap();
    doc.execute("select.addObject", json!({ "index": 0 })).unwrap();
    doc.execute("shape.outline", json!({ "width": 1 })).unwrap();
    assert_eq!(doc.execute("drawing.erase", erase.clone()).unwrap()["erased"], 0);
    let mut failures = Vec::new();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = if relevant {
                vec![("select.addObject", json!({ "index": 0 })), ("shape.outline", json!({ "width": 1 }))]
            } else {
                vec![("file.properties", json!({ "title": "Unrelated" }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([{ "command": "drawing.erase", "params": erase }])).await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("erased after the consumed stroke width changed; outside={outside}")),
                    Err(error) => conflict(&error, "changed"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn a_drawing_move_rests_on_its_laid_out_frame() {
    let mut doc = document();
    doc.execute("caret.set", json!({ "pos": { "block": 0, "off": 3 } })).unwrap();
    let shape = doc.execute("drawing.insert", json!({ "geometry": { "preset": "rect" } })).unwrap();
    let uid = shape["id"].as_str().unwrap().parse::<u32>().unwrap();
    let bytes = doc.save(None).unwrap();
    let before = doc.execute("drawing.describe", json!({ "ids": [uid] })).unwrap();
    doc.execute("style.set", json!({ "style": "Normal", "chr": { "size": 30 } })).unwrap();
    let after = doc.execute("drawing.describe", json!({ "ids": [uid] })).unwrap();
    assert_ne!(before, after, "the style change must move the inline frame");
    let mut failures = Vec::new();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let style = if relevant { "Normal" } else { "Title" };
            let steps = [("style.set", json!({ "style": style, "chr": { "size": 30 } }))];
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([{ "command": "drawing.transform", "params": { "ids": [uid], "dx": 10 } }])).await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("inline frame moved after a consumed style changed; outside={outside}")),
                    Err(error) => conflict(&error, "changed"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn paragraph_insertion_rests_on_the_kind_of_its_indexed_block() {
    let mut doc = DocDocument::create(FILE, 612, 792).unwrap();
    doc.execute("table.insert", json!({ "rows": 1, "cols": 1 })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        let pos = json!({ "story": "body", "path": [0, 0, 0, 0], "off": 0 });
        let steps = [("select.range", json!({ "anchor": pos, "focus": pos })), ("table.toText", json!({}))];
        if outside {
            f.colleague(&steps);
        } else {
            same_server(&f, &steps).await;
        }
        match f.run(Some(basis), json!([by_index(1, 0, 0), { "command": "paragraph.insert", "params": { "after": 0, "text": "New" } }])).await {
            Ok(_) => failures.push(format!("paragraph inserted after the indexed table became text; outside={outside}")),
            Err(error) => assert_eq!(error["error"]["code"], "conflict", "{error}"),
        }
        f.close().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn named_drawings_rest_on_their_anchor_text() {
    let (bytes, ids) = floating_endpoints();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = type_at(if relevant { 1 } else { 4 }, 0, "New ");
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result =
                f.run(Some(basis), json!([{ "command": "drawing.format", "params": { "ids": [ids[0]], "fill": { "color": "#FF0000" } } }])).await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("drawing formatted after anchor offsets changed; outside={outside}")),
                    Err(error) => conflict(&error, "changed"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn a_connector_with_endpoint_ids_still_rests_on_the_caret() {
    let (bytes, ids) = floating_endpoints();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let block = if relevant { 0 } else { 4 };
            let steps = [
                ("select.range", json!({ "anchor": { "block": block, "off": 0 }, "focus": { "block": block, "off": 1 } })),
                ("format.italic", json!({})),
            ];
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([{ "command": "drawing.connect", "params": { "from": ids[0], "to": ids[1] } }])).await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("connector inserted with changed caret formatting; outside={outside}")),
                    Err(error) => conflict(&error, "changed"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn list_neighbours_rest_on_the_lists_containing_them() {
    let mut doc = DocDocument::create(FILE, 612, 792).unwrap();
    doc.execute("table.insert", json!({ "rows": 2, "cols": 2 })).unwrap();
    for (row, text) in [(0, "B"), (1, "A")] {
        let pos = json!({ "story": "body", "path": [0, row, 0, 0], "off": 0 });
        doc.execute("select.range", json!({ "anchor": pos, "focus": pos })).unwrap();
        doc.execute("text.insert", json!({ "text": text })).unwrap();
    }
    doc.execute("caret.set", json!({ "pos": { "block": 1, "off": 0 } })).unwrap();
    doc.execute("text.insert", json!({ "text": "After" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = if relevant {
                vec![("caret.set", json!({ "pos": { "story": "body", "path": [0, 0, 0, 0], "off": 0 } })), ("table.sort", json!({ "header": false }))]
            } else {
                vec![("file.properties", json!({ "title": "Unrelated" }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([by_index(1, 0, 0), { "command": "paragraph.bullets", "params": {} }])).await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("list continuation applied after neighbour rows changed; outside={outside}")),
                    Err(error) => assert_eq!(error["error"]["code"], "conflict", "{error}"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn deleting_a_comment_rests_on_its_body_anchors() {
    let mut doc = document();
    doc.execute("caret.set", json!({ "pos": { "block": 1, "off": 3 } })).unwrap();
    let added = doc.execute("comment.add", json!({ "text": "Check" })).unwrap();
    let tool = added["threadId"].clone();
    assert!(tool.is_string(), "{added}");
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for relevant in [true, false] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = type_at(if relevant { 1 } else { 4 }, 0, "New ");
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([{ "command": "comment.delete", "params": { "thread": tool } }])).await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("comment deleted after its anchors moved; outside={outside}")),
                    Err(error) => conflict(&error, "changed"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn restarting_a_list_rests_on_the_rows_its_forward_scan_visits() {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 1, "off": TEXTS[1].len() }, "focus": { "block": 1, "off": TEXTS[1].len() } })).unwrap();
    doc.execute("text.newParagraph", json!({})).unwrap();
    doc.execute("table.insert", json!({ "rows": 2, "cols": 1 })).unwrap();
    for (row, text) in [(0, "Z"), (1, "A")] {
        doc.execute("select.range", json!({ "anchor": { "story": "body", "path": [2, row, 0, 0], "off": 0 }, "focus": { "story": "body", "path": [2, row, 0, 0], "off": 0 } })).unwrap();
        doc.execute("text.insert", json!({ "text": text })).unwrap();
    }
    doc.execute("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "story": "body", "path": [2, 0, 0, 0], "off": 1 } })).unwrap();
    doc.execute("paragraph.numbering", json!({})).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        // An outside save's unrelated list-id churn is already pinned by numbering_rests_on_the_paragraph_before.
        for relevant in if outside { vec![true] } else { vec![true, false] } {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = if relevant {
                vec![
                    (
                        "select.range",
                        json!({ "anchor": { "story": "body", "path": [2, 0, 0, 0], "off": 0 }, "focus": { "story": "body", "path": [2, 0, 0, 0], "off": 0 } }),
                    ),
                    ("table.sort", json!({ "header": false })),
                ]
            } else {
                vec![("file.properties", json!({ "title": "Unrelated" }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let result = f.run(Some(basis), json!([by_index(0, 0, 0), { "command": "paragraph.restartNumbering", "params": {} }])).await;
            if relevant {
                match result {
                    Ok(_) => failures.push(format!("restart applied after the table rows moved; outside={outside}")),
                    Err(error) => conflict(&error, "changed"),
                }
            } else {
                result.unwrap();
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn header_controls_rest_on_the_body_section_their_handlers_use() {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 1, "off": TEXTS[1].len() }, "focus": { "block": 1, "off": TEXTS[1].len() } })).unwrap();
    doc.execute("page.break", json!({ "kind": "continuous" })).unwrap();
    doc.execute("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 0 } })).unwrap();
    doc.execute("header.insert", json!({ "text": "Head" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut reopened = DocDocument::open(FILE, &bytes).unwrap();
    let part = (0..64)
        .find(|part| {
            reopened
                .execute("file.paragraph", json!({ "story": { "part": part }, "path": [0] }))
                .ok()
                .is_some_and(|p| p["paragraph"]["text"] == "Head")
        })
        .unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for command in ["headerFooter.position", "page.linkToPrevious"] {
            for relevant in [true, false] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let steps = if relevant {
                    vec![
                        ("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 0 } })),
                        ("page.margins", json!({ "left": 100 })),
                    ]
                } else {
                    type_at(4, 0, "New ").to_vec()
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let result = f
                    .run(
                        Some(basis),
                        json!([
                            { "command": "caret.set", "params": { "pos": { "story": { "part": part }, "path": [0], "off": 0 } } },
                            { "command": command, "params": if command == "headerFooter.position" { json!({ "header": 36 }) } else { json!({}) } }
                        ]),
                    )
                    .await;
                if relevant {
                    match result {
                        Ok(_) => failures.push(format!("{command} applied after its body section changed; outside={outside}")),
                        Err(error) => conflict(&error, "changed"),
                    }
                } else {
                    result.unwrap();
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Wrong write (item 6): the object model keeps a paragraph another agent joined into the one
/// before as two stored paragraphs shown as one, so a write by the index of the paragraph that
/// followed it landed on the paragraph now at that index. A join before the index refuses; one
/// after it applies.
#[tokio::test]
async fn a_write_by_index_after_a_join_before_it_is_refused() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    same_server(
        &f,
        &[("select.range", json!({ "anchor": { "block": 2, "off": 0 }, "focus": { "block": 2, "off": 0 } })), ("text.backspace", json!({}))],
    )
    .await;
    let joined = f.texts();
    let error = f.run(Some(basis), json!([by_index(2, 0, 0), { "command": "format.alignCenter", "params": {} }])).await.unwrap_err();
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert_eq!(f.texts(), joined);
    f.close().await;

    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    same_server(
        &f,
        &[("select.range", json!({ "anchor": { "block": 4, "off": 0 }, "focus": { "block": 4, "off": 0 } })), ("text.backspace", json!({}))],
    )
    .await;
    f.run(Some(basis), json!([by_index(2, 0, 0), { "command": "format.alignCenter", "params": {} }])).await.unwrap();
    f.close().await;
}

/// False refusal (a family hold the handler does not read): a reply reads its thread, not the
/// tracked changes; another agent accepting every tracked change does not touch it. (What must
/// refuse it, another writer's change of its thread, cannot be held: a comment has no tool id.)
#[tokio::test]
async fn a_reply_rests_on_the_comments_not_on_the_tracked_changes() {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 2, "off": 7 }, "focus": { "block": 2, "off": 7 } })).unwrap();
    doc.execute("change.track", json!({ "value": true })).unwrap();
    doc.execute("text.insert", json!({ "text": "Z" })).unwrap();
    doc.execute("change.track", json!({ "value": false })).unwrap();
    doc.execute("select.range", json!({ "anchor": { "block": 3, "off": 0 }, "focus": { "block": 3, "off": 5 } })).unwrap();
    let thread = doc.execute("comment.add", json!({ "text": "Look" })).unwrap()["thread"].clone();
    let f = Fixture::with(Guard::Premise, &doc.save(None).unwrap());
    let basis = f.basis().await;
    same_server(&f, &[("change.acceptAll", json!({}))]).await;
    f.run(Some(basis), json!([{ "command": "comment.reply", "params": { "thread": thread, "text": "Reply" } }])).await.unwrap();
    f.close().await;
}

/// A document of [`TEXTS`] with Track Changes on and one tracked insertion, `Z` in block 2.
fn tracked() -> Vec<u8> {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 2, "off": 5 }, "focus": { "block": 2, "off": 5 } })).unwrap();
    doc.execute("change.track", json!({ "value": true })).unwrap();
    doc.execute("text.insert", json!({ "text": "Z" })).unwrap();
    doc.save(None).unwrap()
}

/// Wrong write (C8-doc-4, document-wide reads: `doc.revisions`): typing while changes are tracked
/// makes the typed text a revision, numbered in the root's `revisions` and merged with the
/// author's revision next to it (`edit::insert` under `settings.trackChanges`). Another writer's
/// rejection of a tracked change elsewhere renumbers them: the typing must be refused. A page
/// colour (a setting the typing does not read) must not refuse it.
#[tokio::test]
async fn typing_under_tracking_rests_on_the_revisions() {
    let bytes = tracked();
    for outside in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        if outside {
            f.colleague(&[("change.rejectAll", json!({}))]);
        } else {
            same_server(&f, &[("change.rejectAll", json!({}))]).await;
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let error = f.run(Some(basis), json!([by_index(4, 0, 0), { "command": "text.insert", "params": { "text": "New " } }])).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "outside {outside}: {error}");
        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before, "a refused write leaves the file as it was");
        f.close().await;

        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        if outside {
            f.colleague(&[("page.color", json!({ "color": "CCEEFF" }))]);
        } else {
            same_server(&f, &[("page.color", json!({ "color": "CCEEFF" }))]).await;
        }
        f.run(Some(basis), json!([by_index(4, 0, 0), { "command": "text.insert", "params": { "text": "New " } }])).await.unwrap();
        assert!(f.texts()[4].starts_with("New "), "outside {outside}: {:?}", f.texts());
        f.close().await;
    }
}

/// Wrong write (C8-doc-4, editor state read but not declared: the editing mode): typing in an
/// equation is a mode (`Session::math`), which `text.*`, `edit.paste*` and `equation.*` read: a
/// step in that mode types at the equation (`Session::run_command` leaves it with the caret after
/// it) whatever its `on` names. Another agent who left the editor editing an equation must not
/// make the agent's typing land there; the mode is editor state, put back after each call. The
/// same agent's own equation editing in the same run keeps the mode for its later steps.
#[tokio::test]
async fn another_agents_equation_editing_is_not_the_agents_mode() {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 0 } })).unwrap();
    doc.execute("equation.insert", json!({ "linear": "x^2", "edit": false })).unwrap();
    let f = Fixture::with(Guard::Premise, &doc.save(None).unwrap());
    let basis = f.basis().await;
    let at = json!({ "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 0 } });
    same_server(&f, &[("select.range", at.clone()), ("equation.edit", json!({}))]).await;
    let target = f.blocks().await[3].0.clone();
    f.run(Some(basis), json!([{ "on": [target], "command": "text.insert", "params": { "text": "Typed" } }])).await.unwrap();
    let texts = f.texts();
    assert_eq!(texts[3], "Typed", "{texts:?}");
    assert!(!texts[1].contains("Typed"), "{texts:?}");
    // Its own equation editing earlier in the run is the mode its later steps type in.
    let answer =
        f.run(None, json!([{ "command": "select.range", "params": at }, { "command": "equation.edit", "params": {} }, { "command": "equation.type", "params": { "text": "+1" } }])).await;
    assert!(answer.is_ok(), "{answer:?}");
    f.close().await;
}

/// Wrong write (C8-doc-4, document-wide reads: the root's fields a command reads, per command from
/// the reading): a table is as wide as the page's text column, read from the section
/// (`lastSection`'s page size and margins). Another writer's narrower margins must refuse the
/// agent's table; a page colour (a setting it does not read) must not.
#[tokio::test]
async fn a_table_rests_on_the_page_it_is_as_wide_as() {
    for outside in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        let margins = ("page.margins", json!({ "left": 144, "right": 144 }));
        if outside {
            f.colleague(&[margins]);
        } else {
            same_server(&f, &[margins]).await;
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let error =
            f.run(Some(basis), json!([by_index(4, 0, 0), { "command": "table.insert", "params": { "rows": 2, "cols": 2 } }])).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "outside {outside}: {error}");
        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before, "a refused write leaves the file as it was");
        f.close().await;

        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        let colour = ("page.color", json!({ "color": "CCEEFF" }));
        if outside {
            f.colleague(&[colour]);
        } else {
            same_server(&f, &[colour]).await;
        }
        f.run(Some(basis), json!([by_index(4, 0, 0), { "command": "table.insert", "params": { "rows": 2, "cols": 2 } }])).await.unwrap();
        f.close().await;
    }
}

/// Wrong write (C8-doc-4 item 3, the blind spot C8-doc-3 found): another program's rotation of a
/// drawing object was told by a reread of the file only as its paragraph's text: the object model
/// minted the `Inline`'s object id again on every read and nothing owning holds it (its paragraph's
/// text names it in a mark), so the reread paired it by equal content alone, which a rotation
/// changes, and the created and deleted `1` cancelled out. Its object id now follows its number
/// (`kdm_model::write`), so the reread pairs it and tells its `frame`: `arrange.rotate` on it must
/// refuse; another program's change of another paragraph must not (above).
#[tokio::test]
async fn another_programs_rotation_of_the_selected_object_is_told() {
    let (bytes, uid, block) = with_shape();
    let f = Fixture::with(Guard::Premise, &bytes);
    let basis = f.basis().await;
    f.colleague(&[("drawing.transform", json!({ "ids": [uid], "rotateBy": 45 }))]);
    let before = std::fs::read(f.root.join(FILE)).unwrap();
    let error =
        f.run(Some(basis), json!([select_shape(block), { "command": "arrange.rotate", "params": { "direction": "right" } }])).await.unwrap_err();
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before, "a refused write leaves the file as it was");
    assert!((rotation(&f, uid) - 45.0).abs() < 0.5);
    f.close().await;
}

/// False refusal (C8-doc-4, held-more: the call's selection held by commands that do not read
/// it): Track Changes, a reply by comment number, Restrict Editing read no selection (the
/// independent reading; `tests/acts.rs` runs them with two selections). A paragraph another agent
/// put above the one the call's selection is in (it moves that selection) must not refuse them; another agent's Track Changes
/// still refuses Track Changes (the setting it rests on).
#[tokio::test]
async fn a_command_reading_no_selection_does_not_rest_on_the_calls_selection() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    // The session's selection is the caret at the start of the document (as opened).
    same_server(&f, &[("paragraph.insert", json!({ "before": 0, "text": "New " }))]).await;
    f.run(Some(basis), json!([{ "command": "change.track", "params": { "value": true } }])).await.unwrap();
    assert!(f.disk().inspect().unwrap().to_string().contains("New "));
    f.close().await;

    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    same_server(&f, &[("change.track", json!({ "value": true }))]).await;
    let error = f.run(Some(basis), json!([{ "command": "change.track", "params": { "value": false } }])).await.unwrap_err();
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    f.close().await;
}

/// Wrong write (C8-doc-5, document-wide reads: the document's properties): a Title field takes
/// its text from `core.title` (`tools.rs` `field.insertDocProperty`). Another writer's new title must
/// refuse it; another program's save (which stamps `core.modified` and `core.revision`) of an edit
/// elsewhere must not.
#[tokio::test]
async fn a_title_field_rests_on_the_title_only() {
    let write = json!([by_index(4, 0, 0), { "command": "field.insertDocProperty", "params": { "name": "Title" } }]);
    for outside in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        let title = ("file.properties", json!({ "title": "Another title" }));
        if outside {
            f.colleague(&[title]);
        } else {
            same_server(&f, &[title]).await;
        }
        let error = f.run(Some(basis), write.clone()).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "outside {outside}: {error}");
        f.close().await;

        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        if outside {
            f.colleague(&type_at(2, 0, "New "));
        } else {
            same_server(&f, &type_at(2, 0, "New ")).await;
        }
        f.run(Some(basis), write.clone()).await.unwrap();
        f.close().await;
    }
}

/// Wrong write (C8-doc-5, read but no stored field holds it: the clipboard): a paste takes what
/// the session's clipboard holds, which no stored field holds and an agent cannot read: another
/// agent's copy made the agent's `edit.paste` put that agent's text in. An agent's paste of the
/// session's clipboard is refused; a copy earlier in the same run is its own, and pastes.
#[tokio::test]
async fn an_agent_pastes_only_what_its_own_call_copied() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    same_server(&f, &[("select.range", json!({ "anchor": { "block": 2, "off": 0 }, "focus": { "block": 2, "off": 5 } })), ("edit.copy", json!({}))])
        .await;
    let before = std::fs::read(f.root.join(FILE)).unwrap();
    let error = f.run(Some(basis), json!([by_index(4, 0, 0), { "command": "edit.paste", "params": {} }])).await.unwrap_err();
    assert!(error.to_string().contains("clipboard"), "{error}");
    assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before, "a refused write leaves the file as it was");
    // Its own copy in the same run.
    f.run(None, json!([by_index(3, 0, 5), { "command": "edit.copy", "params": {} }, by_index(4, 0, 0), { "command": "edit.paste", "params": {} }]))
        .await
        .unwrap();
    assert!(f.texts()[4].starts_with("Delta"), "{:?}", f.texts());
    f.close().await;
}

/// Wrong write (C8-doc-5, other objects read and not held: the table a selection takes): a cut
/// across a table copies the table whole (its grid, its properties, its rows) and deletes it;
/// another writer's centring of the table must refuse it; another paragraph's text must not.
#[tokio::test]
async fn a_cut_across_a_table_rests_on_the_table() {
    let bytes = super::holds::table_document();
    let cut = json!([
        { "command": "select.range", "params": { "anchor": { "story": "body", "path": [4, 0, 0, 0], "off": 0 }, "focus": { "story": "body", "path": [5], "off": 0 } } },
        { "command": "edit.cut", "params": {} }
    ]);
    let at = json!({ "story": "body", "path": [4, 0, 1, 0], "off": 0 });
    for outside in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        let width = [("select.range", json!({ "anchor": at, "focus": at })), ("table.properties", json!({ "align": "center" }))];
        if outside {
            f.colleague(&width);
        } else {
            same_server(&f, &width).await;
        }
        let error = f.run(Some(basis), cut.clone()).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "outside {outside}: {error}");
        f.close().await;

        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        if outside {
            f.colleague(&type_at(0, 0, "New "));
        } else {
            same_server(&f, &type_at(0, 0, "New ")).await;
        }
        f.run(Some(basis), cut.clone()).await.unwrap();
        f.close().await;
    }
}

/// Standard base64 of `bytes`.
pub(super) fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            out.push(if i <= chunk.len() { ABC[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// A 16 x 16 PNG of one colour.
pub(super) fn png(rgb: [u8; 3]) -> Vec<u8> {
    let picture = image::RgbaImage::from_pixel(16, 16, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// The document of [`TEXTS`] with a red picture at the start of block 4, and the picture's number
/// (the file's `wp:docPr/@id`), as another program reading the file finds it.
pub(super) fn with_picture() -> (Vec<u8>, Value) {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 4, "off": 0 }, "focus": { "block": 4, "off": 0 } })).unwrap();
    doc.execute("image.insert", json!({ "data": base64(&png([200, 30, 30])), "width": 40 })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut reread = DocDocument::open(FILE, &bytes).unwrap();
    let paragraph = reread.execute("file.paragraph", json!({ "path": [4] })).unwrap();
    let picture = paragraph
        .to_string()
        .split("\"type\":\"image\"")
        .nth(1)
        .and_then(|rest| rest.split("\"uid\":").nth(1))
        .and_then(|rest| rest.split([',', '}']).next().and_then(|n| n.trim().parse::<u64>().ok()));
    (bytes, json!(picture.unwrap_or_else(|| panic!("a picture has its number: {paragraph}"))))
}

/// `docx` with every picture file (`word/media/*`) holding `bytes` instead: another program's
/// replacement of a picture in place, under the same name.
fn pictures_replaced(docx: &[u8], bytes: &[u8]) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(docx)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        out.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(if name.starts_with("word/media/") { bytes } else { &data }).unwrap();
    }
    out.finish().unwrap().into_inner()
}

/// Wrong write (C8-doc-6, pictures without a number): a picture had no number (`wp:docPr/@id`
/// was read for drawing objects only), so an agent could not name it, and a picture command held
/// it only through its paragraph's text. Now every picture has its number, kept in the file: a
/// step names it by `on`, and the command (`image.color` re-encodes its bitmap, `objects.rs`
/// `adjust`) rests on it and on its bitmap. Another agent's border on the picture, and another
/// program's replacement of the picture's file in place (same name, other pixels), must refuse
/// it; another paragraph's text must not.
#[tokio::test]
async fn a_picture_command_rests_on_the_picture_by_its_number() {
    for outside in [false, true] {
        let (bytes, picture) = with_picture();
        let write = json!([{ "on": [picture], "command": "image.color", "params": { "mode": "grayscale" } }]);
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        if outside {
            f.rewrite(&pictures_replaced(&std::fs::read(f.root.join(FILE)).unwrap(), &png([30, 30, 200])));
        } else {
            same_server(
                &f,
                &[
                    ("select.range", json!({ "anchor": { "block": 4, "off": 0 }, "focus": { "block": 4, "off": 3 } })),
                    ("image.border", json!({ "color": "00FF00", "width": 2 })),
                ],
            )
            .await;
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let error = f.run(Some(basis), write.clone()).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "outside {outside}: {error}");
        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before, "a refused write leaves the file as it was");
        f.close().await;

        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        if outside {
            f.colleague(&type_at(0, 0, "New "));
        } else {
            same_server(&f, &type_at(0, 0, "New ")).await;
        }
        f.run(Some(basis), write).await.unwrap();
        assert!(f.texts()[0].starts_with("New "), "outside {outside}: {:?}", f.texts());
        // The number names the same picture after a save and a reread.
        let reread = Fixture::with(Guard::Premise, &std::fs::read(f.root.join(FILE)).unwrap());
        let answer = reread.call("query", json!({ "objects": [picture], "fields": ["uid"] })).await.unwrap();
        assert_eq!(answer["objects"][0]["type"], "Inline", "{answer}");
        reread.close().await;
        f.close().await;
    }
}

/// The document of [`TEXTS`] with an equation (`x^2`) at the start of block 1.
fn with_equation() -> Vec<u8> {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 0 } })).unwrap();
    doc.execute("equation.insert", json!({ "linear": "x^2", "edit": false })).unwrap();
    doc.save(None).unwrap()
}

/// Wrong write (C8-doc-6, equations without a number): an `equation.*` command edits the equation
/// at the caret or the one being edited (`equation.rs` `equation_at_caret`) and reads it
/// (`equation.number` toggles its number mark). It had no number and nothing held it: another
/// agent's change of the equation (`equation.set`) let the agent's toggle land on an equation it
/// did not see. Now every equation has its number and the command rests on it: another agent's
/// and another program's change of it must refuse; another paragraph's text must not.
#[tokio::test]
async fn an_equation_command_rests_on_the_equation() {
    let bytes = with_equation();
    let write = json!([by_index(1, 0, 3), { "command": "equation.number", "params": {} }]);
    let at = json!({ "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 3 } });
    for outside in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        let change = [("select.range", at.clone()), ("equation.set", json!({ "linear": "y^3" }))];
        if outside {
            f.colleague(&change);
        } else {
            same_server(&f, &change).await;
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let error = f.run(Some(basis), write.clone()).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "outside {outside}: {error}");
        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before, "a refused write leaves the file as it was");
        f.close().await;

        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        if outside {
            f.colleague(&type_at(3, 0, "New "));
        } else {
            same_server(&f, &type_at(3, 0, "New ")).await;
        }
        f.run(Some(basis), write.clone()).await.unwrap();
        assert!(f.texts()[3].starts_with("New "), "outside {outside}: {:?}", f.texts());
        f.close().await;
    }
}

/// The document of [`TEXTS`] with a comment on `Gamma` (block 2), and its thread's tool id as the
/// file reads (`comment.threads`).
fn with_comment() -> (Vec<u8>, Value) {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 2, "off": 0 }, "focus": { "block": 2, "off": 5 } })).unwrap();
    doc.execute("comment.add", json!({ "text": "Look", "name": "Ann" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let threads = DocDocument::open(FILE, &bytes).unwrap().execute("comment.threads", json!({})).unwrap();
    let id = threads[0]["toolId"].clone();
    assert!(id.as_str().is_some_and(|id| id.starts_with("thread:")), "a thread has its tool id: {threads}");
    (bytes, id)
}

/// Wrong write (C8-doc-6, comments without a tool id): deleting a thread deletes its comments
/// (`review.rs` `delete_from_thread`), which it reads. A comment had no tool id: nothing held the
/// thread, and its session number (`c0`) named another thread after another program's save. Now
/// a thread is named by its first comment's durable id (`w16cid:durableId`, kept in
/// `commentsIds.xml`) and the delete rests on it: another agent's or program's reply to it must
/// refuse; another comment elsewhere must not.
#[tokio::test]
async fn a_thread_command_rests_on_the_thread_by_its_tool_id() {
    let (bytes, thread) = with_comment();
    let write = json!([{ "command": "comment.delete", "params": { "thread": thread } }]);
    for outside in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        if outside {
            f.colleague(&[("comment.reply", json!({ "thread": "c0", "text": "Not yet", "name": "Bo" }))]);
        } else {
            same_server(&f, &[("comment.reply", json!({ "thread": thread, "text": "Not yet", "name": "Bo" }))]).await;
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let error = f.run(Some(basis), write.clone()).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "outside {outside}: {error}");
        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before, "a refused write leaves the file as it was");
        f.close().await;

        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        let elsewhere = [
            ("select.range", json!({ "anchor": { "block": 4, "off": 0 }, "focus": { "block": 4, "off": 7 } })),
            ("comment.add", json!({ "text": "Other", "name": "Cy" })),
        ];
        if outside {
            f.colleague(&elsewhere);
        } else {
            same_server(&f, &elsewhere).await;
        }
        f.run(Some(basis), write.clone()).await.unwrap();
        let threads = f.disk().execute("comment.threads", json!({})).unwrap();
        assert_eq!(threads.as_array().map(Vec::len), Some(1), "outside {outside}: {threads}");
        assert_ne!(threads[0]["toolId"], thread, "outside {outside}: {threads}");
        f.close().await;
    }
}

/// The document of [`TEXTS`] with a header (`Header note.`), and the header's part number as the
/// file reads.
fn with_header() -> (Vec<u8>, u64) {
    let mut doc = document();
    doc.execute("header.insert", json!({ "text": "Header note." })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut reread = DocDocument::open(FILE, &bytes).unwrap();
    let part = (0..32u64)
        .find(|n| {
            reread
                .execute("file.paragraph", json!({ "path": [0], "story": { "part": n } }))
                .is_ok_and(|answer| answer.to_string().contains("Header note."))
        })
        .unwrap();
    (bytes, part)
}

/// False refusal (C8-doc-6, parts without a tool id): Replace All with the caret in a header
/// searches that story only (`edit.rs` `replace_all`, `search(s, story)`), and a part had no tool
/// id, so the write held every paragraph and the whole root. Now a part is named by what a save
/// keeps (a header by its section and slot: `part:header-1-default`): the write rests on the
/// header's paragraphs and paragraph list. A paragraph another agent or program adds to the
/// header must refuse it; another paragraph's text in the body must not.
#[tokio::test]
async fn replace_all_in_a_header_rests_on_the_header() {
    let (bytes, part) = with_header();
    let at = json!({ "story": { "part": part }, "path": [0], "off": 0 });
    // An agent names a part's position by its number with `caret.set` (its `story` takes one).
    let write = json!([
        { "command": "caret.set", "params": { "pos": at } },
        { "command": "edit.replaceAll", "params": { "text": "note", "with": "N" } }
    ]);
    let end = json!({ "story": { "part": part }, "path": [0], "off": 12 });
    for outside in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        let added = [("caret.set", json!({ "pos": end })), ("text.newParagraph", json!({})), ("text.insert", json!({ "text": "Another note." }))];
        if outside {
            f.colleague(&added);
        } else {
            same_server(&f, &added).await;
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let error = f.run(Some(basis), write.clone()).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "outside {outside}: {error}");
        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before, "a refused write leaves the file as it was");
        f.close().await;

        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        if outside {
            f.colleague(&type_at(3, 0, "New "));
        } else {
            same_server(&f, &type_at(3, 0, "New ")).await;
        }
        f.run(Some(basis), write.clone()).await.unwrap();
        assert!(f.texts()[3].starts_with("New "), "outside {outside}: {:?}", f.texts());
        f.close().await;
    }
}

/// Wrong write (C8-doc-6, other paragraphs read: section breaks): columns applied to the whole
/// document change every section (`page.rs` `columns`, `apply: "document"`: every section
/// `Document::sections` finds by the body's section breaks). The write held only the breaks from
/// the selection on: another writer's break before the selection made a section the agent did
/// not see, and the write gave it columns too. A break anywhere must refuse; another paragraph's
/// text must not.
#[tokio::test]
async fn columns_for_the_whole_document_rest_on_every_section_break() {
    let write = json!([by_index(4, 0, 0), { "command": "page.columns", "params": { "count": 2, "apply": "document" } }]);
    let end = TEXTS[1].len();
    let brk = [
        ("select.range", json!({ "anchor": { "block": 1, "off": end }, "focus": { "block": 1, "off": end } })),
        ("page.break", json!({ "kind": "continuous" })),
    ];
    for outside in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        if outside {
            f.colleague(&brk);
        } else {
            same_server(&f, &brk).await;
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let error = f.run(Some(basis), write.clone()).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "outside {outside}: {error}");
        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before, "a refused write leaves the file as it was");
        f.close().await;

        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        if outside {
            f.colleague(&type_at(2, 0, "New "));
        } else {
            same_server(&f, &type_at(2, 0, "New ")).await;
        }
        f.run(Some(basis), write.clone()).await.unwrap();
        assert!(f.texts()[2].starts_with("New "), "outside {outside}: {:?}", f.texts());
        f.close().await;
    }
}

/// Wrong write (C8-doc-6, a read no stored field holds: the layout): a stroke drawn at a point of
/// a page is anchored in the paragraph laid out under it (`draw.rs`, the layout's hit test).
/// Another writer's long text above moved the paragraphs down, and the agent's stroke went into a
/// paragraph it had not seen there. The write rests on where the page's paragraphs are laid out
/// (`~layout`): text that moves them must refuse; text that moves nothing (a word at the start of
/// the last paragraph) must not.
#[tokio::test]
async fn a_stroke_at_a_point_rests_on_the_layout_of_its_page() {
    let write = json!([{ "command": "ink.stroke", "params": {
        "page": 0, "points": [[100, 150], [200, 150]], "tool": "pen", "color": "C00000", "width": 2
    } }]);
    let long = "Long words ".repeat(60);
    for outside in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        if outside {
            f.colleague(&type_at(0, 0, &long));
        } else {
            same_server(&f, &type_at(0, 0, &long)).await;
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let error = f.run(Some(basis), write.clone()).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "outside {outside}: {error}");
        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before, "a refused write leaves the file as it was");
        f.close().await;

        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        if outside {
            f.colleague(&type_at(4, 0, "New "));
        } else {
            same_server(&f, &type_at(4, 0, "New ")).await;
        }
        f.run(Some(basis), write.clone()).await.unwrap();
        assert!(f.texts()[4].starts_with("New "), "outside {outside}: {:?}", f.texts());
        f.close().await;
    }
}

/// The document of [`TEXTS`] with block 1 numbered.
fn with_numbered_block() -> Vec<u8> {
    let mut doc = document();
    doc.execute("select.range", json!({ "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 0 } })).unwrap();
    doc.execute("paragraph.numbering", json!({})).unwrap();
    doc.save(None).unwrap()
}

/// Wrong write (C8-doc-6, other paragraphs read: list neighbours): numbering a paragraph
/// continues the list of the paragraph before it (`lists.rs`, the previous paragraph's numbering
/// read). The write held the list definitions and the body's order, not the paragraph before:
/// another writer's removal of its numbering made the agent's paragraph start a list it had not
/// seen. That paragraph's properties must refuse it; another paragraph's text must not.
#[tokio::test]
async fn numbering_rests_on_the_paragraph_before() {
    let bytes = with_numbered_block();
    let write = json!([by_index(2, 0, 0), { "command": "paragraph.numbering", "params": {} }]);
    let off = [
        ("select.range", json!({ "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 0 } })),
        ("paragraph.numbering", json!({ "off": true })),
    ];
    for outside in [false, true] {
        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        if outside {
            f.colleague(&off);
        } else {
            same_server(&f, &off).await;
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        let error = f.run(Some(basis), write.clone()).await.unwrap_err();
        assert_eq!(error["error"]["code"], "conflict", "outside {outside}: {error}");
        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before, "a refused write leaves the file as it was");
        f.close().await;

        let f = Fixture::with(Guard::Premise, &bytes);
        let basis = f.basis().await;
        if outside {
            f.colleague(&type_at(4, 0, "New "));
        } else {
            same_server(&f, &type_at(4, 0, "New ")).await;
        }
        let answer = f.run(Some(basis), write.clone()).await;
        answer.unwrap();
        assert!(f.texts()[4].starts_with("New "), "{:?}", f.texts());
        let mut saved = f.disk();
        let previous = saved.execute("file.paragraph", json!({ "path": [1] })).unwrap();
        let numbered = saved.execute("file.paragraph", json!({ "path": [2] })).unwrap();
        assert_eq!(numbered["paragraph"]["props"]["numbering"], previous["paragraph"]["props"]["numbering"]);
        f.close().await;
    }
}

/// C8-doc-7: these reference operations consume the selected paragraph or
/// fragment, not the rest of the document's text, styles and metadata.
#[tokio::test]
async fn short_reference_commands_do_not_scan_the_document() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for command in ["toc.addText", "footnote.insert", "endnote.insert", "index.markEntry", "tableOfAuthorities.markCitation", "quickPart.insert"]
        {
            for necessary in [false, true] {
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = if !necessary {
                    vec![
                        ("file.properties", json!({ "custom": { "Peer": "kept" } })),
                        ("paragraph.text", json!({ "index": 4, "text": "Peer ending" })),
                    ]
                } else if command == "toc.addText" {
                    vec![("caret.set", json!({ "pos": { "block": 1, "off": 0 } })), ("paragraph.set", json!({ "props": { "outlineLevel": 2 } }))]
                } else {
                    vec![(
                        "paragraph.text",
                        json!({ "index": if command == "quickPart.insert" { 0 } else { 1 }, "text": "Peer changed the selected text" }),
                    )]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let write = match command {
                    "toc.addText" => json!([by_index(1, 0, 4), { "command": command, "params": { "level": 1 } }]),
                    "footnote.insert" | "endnote.insert" => {
                        json!([by_index(1, 0, 4), { "command": command, "params": { "text": "Agent note" } }])
                    }
                    "quickPart.insert" => {
                        json!([by_index(0, 0, 5), { "command": command, "params": { "save": "Agent fragment" } }, by_index(1, 0, 4), { "command": command, "params": { "insert": "Agent fragment" } }])
                    }
                    _ => json!([by_index(1, 0, 4), { "command": command, "params": {} }]),
                };
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), write).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: necessary write applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {}", error["conflicts"]));
                } else {
                    let mut texts: Vec<String> = TEXTS.iter().map(|s| s.to_string()).collect();
                    texts[4] = "Peer ending".into();
                    texts[1] = match command {
                        "footnote.insert" | "endnote.insert" => " three four".into(),
                        "index.markEntry" | "tableOfAuthorities.markCitation" => "Beta three four".into(),
                        "quickPart.insert" => "Alpha three four".into(),
                        _ => TEXTS[1].into(),
                    };
                    assert_eq!(f.texts(), texts, "{command}");
                    let mut saved = f.disk();
                    let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                    assert!(data["customProps"].as_array().unwrap().iter().any(|p| p["name"] == "Peer" && p["value"] == "kept"));
                    match command {
                        "toc.addText" => {
                            assert_eq!(saved.execute("file.paragraph", json!({ "path": [1] })).unwrap()["paragraph"]["props"]["outlineLevel"], 0)
                        }
                        "footnote.insert" | "endnote.insert" => assert!(
                            saved
                                .execute("footnote.show", json!({}))
                                .unwrap()
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|n| n["text"].as_str().is_some_and(|s| s.contains("Agent note")))
                        ),
                        "index.markEntry" | "tableOfAuthorities.markCitation" => {
                            assert!(data.to_string().contains(if command == "index.markEntry" { "XE" } else { "TA" }))
                        }
                        _ => {}
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: page properties address a section by paragraph position, not by
/// character offsets. Odd/even headers are a document setting, with no section.
#[tokio::test]
async fn section_properties_do_not_hold_caret_characters() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, params, path, value) in [
            ("page.orientation", json!({ "value": "landscape" }), "/lastSection/landscape", json!(true)),
            ("page.margins", json!({ "top": 50 }), "/lastSection/marginTop", json!(50.0)),
            ("page.size", json!({ "width": 600, "height": 800 }), "/lastSection/pageW", json!(600.0)),
            ("page.columns", json!({ "count": 2 }), "/lastSection/columns/count", json!(2)),
            ("page.lineNumbers", json!({ "value": "restartPage" }), "/lastSection/lineNumbers/restart", json!("page")),
            ("page.pageNumberFormat", json!({ "format": "lowerRoman", "start": 2 }), "/lastSection/pageNumStart", json!(2)),
            ("page.pageSetup", json!({ "section": { "marginTop": 50 } }), "/lastSection/marginTop", json!(50.0)),
            ("page.verticalAlign", json!({ "value": "center" }), "/lastSection/valign", json!("center")),
            ("page.differentFirstPage", json!({ "value": true }), "/lastSection/titlePage", json!(true)),
            ("page.differentOddEven", json!({ "value": true }), "/settings/evenOddHeaders", json!(true)),
            ("page.borders", json!({ "kind": "box" }), "/lastSection/pageBorders/top/style", json!("single")),
        ] {
            for necessary in [false, true] {
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = if necessary {
                    vec![(command, params.clone())]
                } else {
                    vec![
                        ("paragraph.text", json!({ "index": 0, "text": "Alpine one two" })),
                        ("select.range", by_index(0, 0, 5)["params"].clone()),
                        ("format.bold", json!({ "value": true })),
                    ]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "command": command, "params": params }])).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: assigned field applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {}", error["conflicts"]));
                } else {
                    let mut texts: Vec<String> = TEXTS.iter().map(|t| t.to_string()).collect();
                    texts[0] = "Alpine one two".into();
                    assert_eq!(f.texts(), texts);
                    let mut saved = f.disk();
                    let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                    assert_eq!(data.pointer(path).unwrap_or(&Value::Null), &value, "{command}: {}", data["lastSection"]);
                    assert_eq!(saved.execute("file.paragraph", json!({ "path": [0] })).unwrap()["paragraph"]["runs"][0]["props"]["bold"], true);
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: absolute character operations do not resolve the old paragraph
/// style. Their assigned character fields (and Clear's paragraph fields) stay held.
#[tokio::test]
async fn absolute_character_operations_hold_assigned_fields() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, params, peer, path, value) in [
            ("format.bold", json!({ "value": true }), json!({ "bold": false }), "/runs/0/props/bold", json!(true)),
            ("format.italic", json!({ "value": true }), json!({ "italic": false }), "/runs/0/props/italic", json!(true)),
            ("format.strikethrough", json!({ "value": true }), json!({ "strike": false }), "/runs/0/props/strike", json!(true)),
            ("format.doubleStrikethrough", json!({ "value": true }), json!({ "doubleStrike": false }), "/runs/0/props/doubleStrike", json!(true)),
            ("format.allCaps", json!({ "value": true }), json!({ "caps": false }), "/runs/0/props/caps", json!(true)),
            ("format.smallCaps", json!({ "value": true }), json!({ "smallCaps": false }), "/runs/0/props/smallCaps", json!(true)),
            ("format.hidden", json!({ "value": true }), json!({ "hidden": false }), "/runs/0/props/hidden", json!(true)),
            ("format.outline", json!({ "value": true }), json!({ "outline": false }), "/runs/0/props/outline", json!(true)),
            ("format.shadow", json!({ "value": true }), json!({ "shadow": false }), "/runs/0/props/shadow", json!(true)),
            ("format.emboss", json!({ "value": true }), json!({ "emboss": false }), "/runs/0/props/emboss", json!(true)),
            ("format.engrave", json!({ "value": true }), json!({ "engrave": false }), "/runs/0/props/engrave", json!(true)),
            ("format.subscript", json!({ "value": true }), json!({ "vertAlign": "superscript" }), "/runs/0/props/vertAlign", json!("subscript")),
            ("format.superscript", json!({ "value": true }), json!({ "vertAlign": "subscript" }), "/runs/0/props/vertAlign", json!("superscript")),
            (
                "format.border",
                json!({ "value": true }),
                json!({ "border": { "style": "double", "width": 2 } }),
                "/runs/0/props/border/style",
                json!("single"),
            ),
            ("format.underline", json!({ "style": "wave" }), json!({ "underline": "double" }), "/runs/0/props/underline", json!("wave")),
            ("format.doubleUnderline", json!({}), json!({ "underline": "single" }), "/runs/0/props/underline", json!("double")),
            ("format.wordUnderline", json!({}), json!({ "underline": "single" }), "/runs/0/props/underline", json!("words")),
            ("format.color", json!({ "color": "auto" }), json!({ "color": "123456" }), "/runs/0/props/color", json!("Auto")),
            ("format.shading", json!({ "color": null }), json!({ "shading": [12, 34, 56] }), "/runs/0/props/shading", Value::Null),
            ("format.resetChar", json!({}), json!({ "bold": true }), "/runs/0/props/bold", Value::Null),
            ("format.clear", json!({}), json!({}), "/props/indentLeft", Value::Null),
            ("format.charStyle", json!({ "style": "Strong" }), json!({ "style": "Emphasis" }), "/runs/0/props/style", json!("Strong")),
            ("format.changeCase", json!({ "mode": "upper" }), json!({}), "/text", json!("BETA three four")),
        ] {
            for necessary in [false, true] {
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = if !necessary {
                    vec![("style.set", json!({ "style": "Normal", "chr": { "italic": true } }))]
                } else if command == "format.clear" {
                    vec![("caret.set", json!({ "pos": { "block": 1, "off": 0 } })), ("paragraph.set", json!({ "props": { "indentLeft": 72 } }))]
                } else if command == "format.changeCase" {
                    vec![("paragraph.text", json!({ "index": 1, "text": "Beto three four" }))]
                } else {
                    vec![("select.range", by_index(1, 0, 4)["params"].clone()), ("format.set", json!({ "props": peer }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([by_index(1, 0, 4), { "command": command, "params": params }])).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: assigned field applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {}", error["conflicts"]));
                } else {
                    let mut saved = f.disk();
                    let paragraph = saved.execute("file.paragraph", json!({ "path": [1] })).unwrap()["paragraph"].clone();
                    assert_eq!(paragraph.pointer(path).unwrap_or(&Value::Null), &value, "{command}: {paragraph}");
                    let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                    let normal = data["styles"]["styles"].as_array().unwrap().iter().find(|s| s["id"] == "Normal").unwrap();
                    assert_eq!(normal["chr"]["italic"], true);
                    let mut texts: Vec<String> = TEXTS.iter().map(|s| s.to_string()).collect();
                    if command == "format.changeCase" {
                        texts[1] = "BETA three four".into();
                    }
                    assert_eq!(f.texts(), texts);
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: table-wide property operations locate the table, without reading
/// cell text, row formatting, or character offsets inside the focus cell.
#[tokio::test]
async fn table_properties_hold_the_table_fields_they_use() {
    let mut doc = document();
    doc.execute("caret.set", json!({ "pos": { "block": 0, "off": 0 } })).unwrap();
    let table = doc.execute("table.insert", json!({ "rows": 2, "cols": 2 })).unwrap()["table"][0].as_u64().unwrap();
    for (row, col, text) in [(0, 0, "A1"), (0, 1, "B1"), (1, 0, "A2"), (1, 1, "B2")] {
        doc.execute("caret.set", json!({ "pos": { "story": "body", "path": [table, row, col, 0], "off": 0 } })).unwrap();
        doc.execute("text.insert", json!({ "text": text })).unwrap();
    }
    let bytes = doc.save(None).unwrap();
    let at = json!({ "story": "body", "path": [table, 0, 0, 0], "off": 0 });
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, params, path, value) in [
            ("table.style", json!({ "style": "Table Grid" }), "/props/style", json!("TableGrid")),
            ("table.look", json!({ "headerRow": false }), "/props/look/headerRow", json!(false)),
            ("table.cellMargins", json!({ "top": 3 }), "/props/cellMargins/0", json!(3.0)),
            ("table.properties", json!({ "align": "center" }), "/props/align", json!("center")),
        ] {
            for change in 0..5 {
                let f = Fixture::with(Guard::Premise, &bytes);
                f.run(None, json!([{ "command": "caret.set", "params": { "pos": at } }])).await.unwrap();
                let basis = f.basis().await;
                let steps = match change {
                    0 => vec![("caret.set", json!({ "pos": at })), ("text.insert", json!({ "text": "Peer " }))],
                    1 => vec![
                        ("caret.set", json!({ "pos": { "story": "body", "path": [table, 0, 1, 0], "off": 0 } })),
                        ("text.insert", json!({ "text": "Peer " })),
                        ("table.properties", json!({ "rowHeight": 40 })),
                    ],
                    3 => vec![("caret.set", json!({ "pos": at })), ("table.deleteTable", json!({}))],
                    4 => vec![("page.insertCover", json!({ "title": "Peer before table" }))],
                    _ => vec![
                        ("caret.set", json!({ "pos": at })),
                        (command, if command == "table.style" { json!({ "style": "Normal Table" }) } else { params.clone() }),
                    ],
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "command": command, "params": params }])).await;
                if change >= 2 {
                    match result {
                        Err(error) => {
                            // The retained caret follows the cell by identity. Inserting
                            // before its table changes the indexed table place, not its fields.
                            conflict(&error, if change == 4 { "relocated" } else { "changed" });
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: assigned field applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, change {change}, outside {outside}: {error}"));
                } else {
                    let mut saved = f.disk();
                    let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                    let tab = &data["body"][table as usize];
                    assert_eq!(tab.pointer(path).unwrap_or(&Value::Null), &value, "{command}: {tab}");
                    for (col, text) in [(0, if change == 0 { "Peer A1" } else { "A1" }), (1, if change == 1 { "Peer B1" } else { "B1" })] {
                        assert_eq!(saved.execute("file.paragraph", json!({ "path": [table, 0, col, 0] })).unwrap()["paragraph"]["text"], text);
                    }
                    if change == 1 {
                        assert_eq!(tab["rows"][0]["props"]["height"], 40.0);
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: rotations and wrapping consume shape frames, without appearance.
/// Outside frame changes must remain protected despite unowned Inline objects.
#[tokio::test]
async fn shape_frames_do_not_hold_appearance() {
    let (bytes, uid, block) = with_shape();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, params, path, value) in [
            ("arrange.rotate", json!({ "direction": "right" }), "/float/rot", json!(90.0)),
            ("arrange.rotation", json!({ "degrees": 30 }), "/float/rot", json!(30.0)),
            ("arrange.wrap", json!({ "wrap": "behindText" }), "/float/wrap", json!("behindText")),
        ] {
            for necessary in [false, true] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let steps = if !necessary {
                    vec![("drawing.format", json!({ "ids": [uid], "fill": { "color": "#123456" } }))]
                } else if command == "arrange.wrap" {
                    vec![("select.range", select_shape(block)["params"].clone()), (command, json!({ "wrap": "inFrontOfText" }))]
                } else {
                    vec![("drawing.transform", json!({ "ids": [uid], "rotateBy": 45 }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([select_shape(block), { "command": command, "params": params }])).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: assigned frame applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {error}"));
                } else {
                    let p = f.disk().execute("file.paragraph", json!({ "path": [block] })).unwrap()["paragraph"].clone();
                    let object = p["objects"].as_array().unwrap().iter().find(|o| o["uid"] == uid).unwrap();
                    assert_eq!(object.pointer(path).unwrap_or(&Value::Null), &value, "{command}: {object}");
                    assert_eq!(object["fill"], json!([18, 52, 86]));
                    assert_eq!(f.texts(), TEXTS);
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: scanning or rebuilding body content does not consume unrelated
/// custom properties. A core-property assignment only consumes its own field.
#[tokio::test]
async fn document_operations_do_not_hold_unrelated_metadata() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, params, marker) in [
            ("file.properties", json!({ "title": "Agent title" }), "Agent title"),
            ("edit.replace", json!({ "text": "Beta", "with": "Agent" }), "Agent three four"),
            ("edit.replaceAll", json!({ "text": "three", "with": "Agent" }), "Beta Agent four"),
            ("caption.insert", json!({ "text": "Agent caption" }), "Agent caption"),
            ("toc.insert", json!({ "title": "Agent contents" }), "Agent contents"),
            ("mailMerge.insertField", json!({ "field": "City" }), "MERGEFIELD"),
            ("mailMerge.addressBlock", json!({}), "ADDRESSBLOCK"),
            ("mailMerge.greetingLine", json!({}), "GREETINGLINE"),
            ("mailMerge.rules", json!({ "rule": "MERGEREC" }), "MERGEREC"),
            ("change.acceptAll", json!({}), ""),
            ("change.rejectAll", json!({}), ""),
        ] {
            for necessary in [false, true] {
                let bytes = if command.starts_with("change.") { tracked() } else { document().save(None).unwrap() };
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let steps = if !necessary {
                    vec![("file.properties", json!({ "custom": { "Peer": "kept" } }))]
                } else if command == "file.properties" {
                    vec![(command, json!({ "title": "Peer title" }))]
                } else {
                    vec![("paragraph.text", json!({ "index": 1, "text": "Peer three four" }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([by_index(1, 0, 4), { "command": command, "params": params }])).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: necessary write applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {error}"));
                } else {
                    let saved = f.disk();
                    let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                    assert!(data["customProps"].as_array().unwrap().iter().any(|p| p["name"] == "Peer" && p["value"] == "kept"));
                    if command == "file.properties" {
                        assert_eq!(data["core"]["title"], "Agent title");
                        assert_eq!(f.texts(), TEXTS);
                    } else if command.starts_with("change.") {
                        assert!(data["revisions"].as_array().unwrap().is_empty());
                        let mut texts: Vec<String> = TEXTS.iter().map(|s| s.to_string()).collect();
                        if command == "change.acceptAll" {
                            texts[2] = "GammaZ five six".into();
                        }
                        assert_eq!(f.texts(), texts);
                    } else if command.starts_with("edit.") {
                        let mut texts: Vec<String> = TEXTS.iter().map(|s| s.to_string()).collect();
                        texts[1] = marker.into();
                        assert_eq!(f.texts(), texts);
                    } else {
                        assert!(data.to_string().contains(marker), "{command} did not save {marker}");
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: replacement text containing a paragraph break resolves the split
/// paragraph's next style; the plain single-paragraph path does not.
#[tokio::test]
async fn replacement_paragraph_breaks_hold_the_split_style() {
    let mut doc = document();
    doc.execute("caret.set", json!({ "pos": { "block": 1, "off": 0 } })).unwrap();
    doc.execute("paragraph.set", json!({ "props": { "style": "Heading1" } })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for necessary in [false, true] {
            let f = Fixture::with(Guard::Premise, &bytes);
            let basis = f.basis().await;
            let steps = if necessary {
                vec![("style.set", json!({ "style": "Heading1", "next": "Heading2" }))]
            } else {
                vec![("file.properties", json!({ "custom": { "Peer": "kept" } }))]
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let before = std::fs::read(f.root.join(FILE)).unwrap();
            let result = f
                .run(
                    Some(basis),
                    json!([by_index(1, 0, TEXTS[1].len()), { "command": "edit.replace", "params": { "text": TEXTS[1], "with": "Agent\rNext" } }]),
                )
                .await;
            if necessary {
                match result {
                    Err(error) => {
                        conflict(&error, "changed");
                        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                    }
                    Ok(_) => failures.push(format!("outside {outside}: changed split style applied")),
                }
            } else if let Err(error) = result {
                failures.push(format!("outside {outside}: {error}"));
            } else {
                assert_eq!(f.texts(), ["Alpha one two", "Agent", "Next", "Gamma five six", "Delta seven eight", "Epsilon nine ten"]);
                let mut saved = f.disk();
                assert_eq!(saved.execute("file.paragraph", json!({ "path": [2] })).unwrap()["paragraph"]["props"]["style"], "Normal");
                let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                assert!(data["customProps"].as_array().unwrap().iter().any(|p| p["name"] == "Peer" && p["value"] == "kept"));
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: named comment commands consume their comment/thread, without
/// using the body selection to choose it. Selection fallback remains guarded.
#[tokio::test]
async fn named_comment_operations_do_not_hold_the_body_caret() {
    let (bytes, thread) = with_comment();
    let mut doc = DocDocument::open(FILE, &bytes).unwrap();
    doc.execute("comment.reply", json!({ "thread": thread, "text": "Reply", "name": "Bo" })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut reread = DocDocument::open(FILE, &bytes).unwrap();
    let threads = reread.execute("comment.threads", json!({})).unwrap();
    let root: u64 = threads[0]["comments"][0]["id"].as_str().unwrap().trim_start_matches('c').parse().unwrap();
    let reply: u64 = threads[0]["comments"][1]["id"].as_str().unwrap().trim_start_matches('c').parse().unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for command in ["comment.delete", "comment.deleteCurrent", "comment.resolveCurrent"] {
            for necessary in [false, true] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let steps = if !necessary {
                    vec![("paragraph.insert", json!({ "before": 0, "text": "Peer paragraph" }))]
                } else if command == "comment.resolveCurrent" {
                    vec![(command, json!({ "id": root, "value": true }))]
                } else {
                    vec![("comment.edit", json!({ "id": reply, "text": "Peer reply" }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let params = match command {
                    "comment.delete" => json!({ "thread": thread }),
                    "comment.deleteCurrent" => json!({ "id": reply }),
                    _ => json!({ "id": root, "value": true }),
                };
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "command": command, "params": params }])).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: necessary write applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {error}"));
                } else {
                    assert_eq!(f.texts(), [vec!["Peer paragraph".to_owned()], TEXTS.iter().map(|s| s.to_string()).collect()].concat());
                    let threads = f.disk().execute("comment.threads", json!({})).unwrap();
                    if command == "comment.delete" {
                        assert!(threads.as_array().unwrap().is_empty());
                    } else if command == "comment.deleteCurrent" {
                        assert_eq!(threads[0]["comments"].as_array().unwrap().len(), 1);
                        assert_eq!(threads[0]["comments"][0]["body"], "Look");
                    } else {
                        assert_eq!(threads[0]["resolved"], true);
                        assert_eq!(threads[0]["comments"].as_array().unwrap().len(), 2);
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: list formatting reads paragraph numbering, not characters at
/// the implicit caret. The assigned paragraph properties remain protected.
#[tokio::test]
async fn list_properties_do_not_hold_caret_characters() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, params) in [
            ("paragraph.bullets", json!({})),
            ("paragraph.bullets", json!({ "off": true })),
            ("paragraph.numbering", json!({})),
            ("paragraph.numbering", json!({ "off": true })),
            ("paragraph.multilevel", json!({})),
            ("paragraph.defineBullet", json!({ "char": "★" })),
            ("paragraph.defineNumber", json!({ "format": "upperRoman", "start": 3 })),
            ("list.define", json!({ "levels": [{ "format": "upperRoman" }] })),
        ] {
            let bytes = document().save(None).unwrap();
            for necessary in [false, true] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let steps = if necessary {
                    vec![("paragraph.numbering", json!({}))]
                } else {
                    vec![
                        ("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 5 } })),
                        ("format.bold", json!({ "value": true })),
                    ]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "command": command, "params": params }])).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command} {params}, outside {outside}: changed numbering applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command} {params}, outside {outside}: {error}"));
                } else {
                    assert_eq!(f.texts(), TEXTS);
                    let mut saved = f.disk();
                    let para = saved.execute("file.paragraph", json!({ "path": [0] })).unwrap()["paragraph"].clone();
                    assert_eq!(para["runs"][0]["props"]["bold"], true);
                    let numbering = para["props"]["numbering"].clone();
                    if params.get("off") == Some(&json!(true)) {
                        assert_eq!(numbering["num"], 0);
                    } else {
                        let levels = saved.execute("list.get", json!({ "num": numbering["num"] })).unwrap();
                        assert_eq!(numbering["level"], 0);
                        if command == "paragraph.bullets" || command == "paragraph.defineBullet" {
                            assert_eq!(levels["levels"][0]["format"], "bullet");
                        }
                        if command == "paragraph.defineNumber" || command == "list.define" {
                            assert_eq!(levels["levels"][0]["format"], "upperRoman");
                        }
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: defining a new unlinked list without applying it uses the
/// numbering definitions, without the body caret or existing styles.
#[tokio::test]
async fn an_unapplied_unlinked_list_uses_definitions_only() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for peer in ["body", "styles", "definitions"] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            let steps = match peer {
                "body" => vec![("paragraph.insert", json!({ "before": 0, "text": "Peer paragraph" }))],
                "styles" => vec![("style.set", json!({ "style": "Normal", "chr": { "italic": true } }))],
                _ => vec![("list.define", json!({ "apply": false, "levels": [{ "format": "lowerRoman" }] }))],
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let before = std::fs::read(f.root.join(FILE)).unwrap();
            let result =
                f.run(Some(basis), json!([{ "command": "list.define", "params": { "apply": false, "levels": [{ "format": "upperRoman" }] } }])).await;
            if peer == "definitions" {
                match result {
                    Err(error) => {
                        conflict(&error, "changed");
                        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                    }
                    Ok(_) => failures.push(format!("outside {outside}: changed definitions applied")),
                }
            } else if let Err(error) = result {
                failures.push(format!("peer {peer}, outside {outside}: {error}"));
            } else {
                let mut saved = f.disk();
                let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                assert!(data["numbering"]["abstracts"].as_array().unwrap().iter().any(|n| n["levels"][0]["format"] == "upperRoman"));
                for index in 0..f.texts().len() {
                    assert!(saved.execute("file.paragraph", json!({ "path": [index] })).unwrap()["paragraph"]["props"]["numbering"].is_null());
                }
                if peer == "body" {
                    assert_eq!(f.texts(), [vec!["Peer paragraph".to_owned()], TEXTS.iter().map(|s| s.to_string()).collect()].concat());
                } else {
                    assert!(data["styles"]["styles"].as_array().unwrap().iter().any(|s| s["id"] == "Normal" && s["chr"]["italic"] == true));
                    assert_eq!(f.texts(), TEXTS);
                }
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: explicit off short-circuits before reading list definitions,
/// styles, or neighbours. It still assigns the paragraph's numbering field.
#[tokio::test]
async fn explicitly_disabling_a_list_does_not_resolve_definitions() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for command in ["paragraph.bullets", "paragraph.numbering"] {
            for peer in ["definitions", "styles", "assigned"] {
                let f = Fixture::new(Guard::Premise);
                let basis = f.basis().await;
                let steps = match peer {
                    "definitions" => vec![("list.define", json!({ "apply": false, "levels": [{ "format": "lowerRoman" }] }))],
                    "styles" => vec![("style.set", json!({ "style": "Normal", "chr": { "italic": true } }))],
                    _ => vec![("paragraph.numbering", json!({}))],
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "command": command, "params": { "off": true } }])).await;
                if peer == "assigned" {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: assigned numbering applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, peer {peer}, outside {outside}: {error}"));
                } else {
                    let mut saved = f.disk();
                    let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                    assert_eq!(saved.execute("file.paragraph", json!({ "path": [0] })).unwrap()["paragraph"]["props"]["numbering"]["num"], 0);
                    assert_eq!(f.texts(), TEXTS);
                    if peer == "definitions" {
                        assert!(data["numbering"]["abstracts"].as_array().unwrap().iter().any(|n| n["levels"][0]["format"] == "lowerRoman"));
                    } else {
                        assert!(data["styles"]["styles"].as_array().unwrap().iter().any(|s| s["id"] == "Normal" && s["chr"]["italic"] == true));
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: direct reset and sorting do not
/// resolve style definitions. Their own text and assigned formatting still hold.
#[tokio::test]
async fn direct_paragraph_operations_do_not_resolve_styles() {
    let mut doc = document();
    doc.execute("caret.set", json!({ "pos": { "block": 0, "off": 0 } })).unwrap();
    doc.execute("paragraph.set", json!({ "props": { "style": "Heading1", "indentLeft": 36 } })).unwrap();
    doc.execute("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 5 } })).unwrap();
    doc.execute("format.charStyle", json!({ "style": "Strong" })).unwrap();
    doc.execute("format.bold", json!({ "value": true })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for level in ["sort", "paragraphStyle", "paragraphFormatting", "characterStyle", "characterFormatting"] {
            for necessary in [false, true] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let steps = if !necessary {
                    vec![("style.set", json!({ "style": "Normal", "chr": { "italic": true } }))]
                } else {
                    match level {
                        "sort" => vec![("paragraph.text", json!({ "index": 1, "text": "Peer three four" }))],
                        "paragraphStyle" => vec![("paragraph.set", json!({ "props": { "style": "Heading2" } }))],
                        "paragraphFormatting" => vec![("paragraph.set", json!({ "props": { "indentLeft": 72 } }))],
                        _ => vec![
                            ("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 5 } })),
                            if level == "characterStyle" {
                                ("format.charStyle", json!({ "style": "Emphasis" }))
                            } else {
                                ("format.bold", json!({ "value": false }))
                            },
                        ],
                    }
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let write = match level {
                    "sort" => {
                        json!([{ "command": "select.range", "params": { "anchor": { "block": 0, "off": 0 }, "focus": { "block": 3, "off": TEXTS[3].len() } } }, { "command": "paragraph.sort", "params": { "descending": true } }])
                    }
                    _ => json!([by_index(0, 0, 5), { "command": "style.inspectorClear", "params": { "level": level } }]),
                };
                let result = f.run(Some(basis), write).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{level}, outside {outside}: necessary input applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{level}, outside {outside}: {error}"));
                } else {
                    let mut saved = f.disk();
                    let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                    assert!(data["styles"]["styles"].as_array().unwrap().iter().any(|s| s["id"] == "Normal" && s["chr"]["italic"] == true));
                    let para = saved.execute("file.paragraph", json!({ "path": [0] })).unwrap()["paragraph"].clone();
                    match level {
                        "sort" => assert_eq!(f.texts(), [TEXTS[2], TEXTS[3], TEXTS[1], TEXTS[0], TEXTS[4]]),
                        _ => {
                            assert_eq!(f.texts(), TEXTS);
                            match level {
                                "paragraphStyle" => assert!(para["props"]["style"].is_null()),
                                "paragraphFormatting" => {
                                    assert!(para["props"]["indentLeft"].is_null());
                                    assert_eq!(para["props"]["style"], "Heading1");
                                }
                                "characterStyle" => assert!(para["runs"][0]["props"]["style"].is_null()),
                                _ => {
                                    assert!(para["runs"][0]["props"]["bold"].is_null());
                                    assert_eq!(para["runs"][0]["props"]["style"], "Strong");
                                }
                            }
                        }
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: the paragraph levels of the inspector reset paragraph fields,
/// without reading the characters at the implicit caret.
#[tokio::test]
async fn paragraph_inspector_does_not_hold_caret_characters() {
    let mut doc = document();
    doc.execute("caret.set", json!({ "pos": { "block": 0, "off": 0 } })).unwrap();
    doc.execute("paragraph.set", json!({ "props": { "style": "Heading1", "indentLeft": 36 } })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for level in ["paragraphStyle", "paragraphFormatting"] {
            for necessary in [false, true] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let steps = if necessary {
                    vec![(
                        "paragraph.set",
                        if level == "paragraphStyle" {
                            json!({ "props": { "style": "Heading2" } })
                        } else {
                            json!({ "props": { "indentLeft": 72 } })
                        },
                    )]
                } else {
                    vec![
                        ("select.range", json!({ "anchor": { "block": 0, "off": 0 }, "focus": { "block": 0, "off": 5 } })),
                        ("format.bold", json!({ "value": true })),
                    ]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "command": "style.inspectorClear", "params": { "level": level } }])).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{level}, outside {outside}: assigned paragraph fields applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{level}, outside {outside}: {error}"));
                } else {
                    let p = f.disk().execute("file.paragraph", json!({ "path": [0] })).unwrap()["paragraph"].clone();
                    assert_eq!(p["runs"][0]["props"]["bold"], true);
                    assert_eq!(f.texts(), TEXTS);
                    if level == "paragraphStyle" {
                        assert!(p["props"]["style"].is_null());
                        assert_eq!(p["props"]["indentLeft"], 36.0);
                    } else {
                        assert_eq!(p["props"]["style"], "Heading1");
                        assert!(p["props"]["indentLeft"].is_null());
                    }
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: geometry, crop, description and reset do not decode bitmap
/// bytes. An edit to another picture's bitmap must merge with these writes.
#[tokio::test]
async fn picture_properties_do_not_hold_unrelated_bitmaps() {
    let (bytes, picture) = with_picture();
    let mut doc = DocDocument::open(FILE, &bytes).unwrap();
    doc.execute("select.range", json!({ "anchor": { "block": 3, "off": 0 }, "focus": { "block": 3, "off": 0 } })).unwrap();
    doc.execute("image.insert", json!({ "data": base64(&png([30, 30, 200])), "width": 40 })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for (command, params, peer) in [
            ("image.altText", json!({ "text": "Agent description" }), json!({ "text": "Peer description" })),
            ("image.crop", json!({ "left": 0.1 }), json!({ "left": 0.2 })),
            ("image.size", json!({ "width": 100 }), json!({ "width": 80 })),
            ("image.reset", json!({}), json!({ "left": 0.2 })),
        ] {
            for necessary in [false, true] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let (block, peer_command, peer_params) = if necessary {
                    (4, if command == "image.reset" { "image.crop" } else { command }, peer.clone())
                } else {
                    (3, "image.color", json!({ "mode": "grayscale" }))
                };
                let steps = [
                    ("select.range", json!({ "anchor": { "block": block, "off": 0 }, "focus": { "block": block, "off": 3 } })),
                    (peer_command, peer_params),
                ];
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let result = f.run(Some(basis), json!([{ "on": [picture], "command": command, "params": params }])).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: assigned picture field applied")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {error}"));
                } else {
                    let mut saved = f.disk();
                    let p = saved.execute("file.paragraph", json!({ "path": [4] })).unwrap()["paragraph"].clone();
                    let target = p["objects"].as_array().unwrap().iter().find(|o| o["uid"] == picture).unwrap();
                    match command {
                        "image.altText" => assert_eq!(target["alt"], "Agent description"),
                        "image.crop" => assert_eq!(target["crop"][0], json!(0.1_f32)),
                        "image.size" => assert_eq!(target["w"].as_f64(), Some(100.0)),
                        _ => assert_eq!(target["crop"], json!([0.0, 0.0, 0.0, 0.0])),
                    }
                    let p = saved.execute("file.paragraph", json!({ "path": [3] })).unwrap()["paragraph"].clone();
                    let blue = p["objects"].as_array().unwrap().iter().find(|o| o["type"] == "image").unwrap();
                    // JSON omits embedded media; verify the bitmap in the saved package.
                    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(std::fs::read(f.root.join(FILE)).unwrap())).unwrap();
                    let mut item = archive.by_name(&format!("word/media/{}", blue["media"].as_str().unwrap())).unwrap();
                    let mut bytes = Vec::new();
                    std::io::Read::read_to_end(&mut item, &mut bytes).unwrap();
                    let pixels = image::load_from_memory(&bytes).unwrap().to_rgb8();
                    let pixel = pixels.get_pixel(0, 0).0;
                    assert_eq!(pixel[0], pixel[1]);
                    assert_eq!(pixel[1], pixel[2]);
                    assert_eq!(saved.execute("file.paragraph", json!({ "path": [1] })).unwrap()["paragraph"]["text"], TEXTS[1]);
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: style definitions without Apply use style data, without
/// consuming the table under the caret or its cell contents.
#[tokio::test]
async fn table_style_definitions_do_not_hold_the_caret_table() {
    let mut doc = document();
    doc.execute("caret.set", json!({ "pos": { "block": 0, "off": 0 } })).unwrap();
    let table = doc.execute("table.insert", json!({ "rows": 2, "cols": 2 })).unwrap()["table"][0].as_u64().unwrap();
    let at = json!({ "story": "body", "path": [table, 0, 0, 0], "off": 0 });
    doc.execute("caret.set", json!({ "pos": at })).unwrap();
    doc.execute("text.insert", json!({ "text": "A1" })).unwrap();
    doc.execute("table.newStyle", json!({ "name": "MyGrid", "apply": false })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut failures = Vec::new();
    for outside in [false, true] {
        for command in ["table.newStyle", "table.setStyle"] {
            for necessary in [false, true] {
                let f = Fixture::with(Guard::Premise, &bytes);
                let basis = f.basis().await;
                let name = if command == "table.newStyle" { "AgentGrid" } else { "MyGrid" };
                let steps = if necessary {
                    vec![(
                        command,
                        if command == "table.newStyle" {
                            json!({ "name": name, "apply": false })
                        } else {
                            json!({ "style": name, "wholeTable": { "fill": "112233" } })
                        },
                    )]
                } else {
                    vec![("caret.set", json!({ "pos": at })), ("text.insert", json!({ "text": "Peer " }))]
                };
                if outside {
                    f.colleague(&steps);
                } else {
                    same_server(&f, &steps).await;
                }
                let before = std::fs::read(f.root.join(FILE)).unwrap();
                let params = if command == "table.newStyle" {
                    json!({ "name": name, "apply": false, "wholeTable": { "fill": "123456" } })
                } else {
                    json!({ "style": name, "wholeTable": { "fill": "123456" } })
                };
                let result = f.run(Some(basis), json!([{ "command": command, "params": params }])).await;
                if necessary {
                    match result {
                        Err(error) => {
                            conflict(&error, "changed");
                            assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                        }
                        Ok(_) => failures.push(format!("{command}, outside {outside}: assigned style changed")),
                    }
                } else if let Err(error) = result {
                    failures.push(format!("{command}, outside {outside}: {error}"));
                } else {
                    let mut saved = f.disk();
                    let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                    let style = data["styles"]["styles"].as_array().unwrap().iter().find(|s| s["name"] == name).unwrap();
                    assert_eq!(style["table"]["fill"], json!([18, 52, 86]));
                    let t = &data["body"][table as usize];
                    assert_eq!(t["props"]["style"], "TableGrid");
                    let p = saved.execute("file.paragraph", json!({ "path": [table, 0, 0, 0] })).unwrap();
                    assert_eq!(p["paragraph"]["text"], "Peer A1");
                }
                f.close().await;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// C8-doc-7: a longer replacement with no paragraph break does not resolve
/// styles. Keep the existing paragraph premises and the split/AutoFormat paths.
#[tokio::test]
async fn plain_paragraph_replacement_does_not_resolve_styles() {
    let mut failures = Vec::new();
    for outside in [false, true] {
        for peer in ["styles", "paragraph", "assigned"] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            let steps = match peer {
                "styles" => vec![("style.set", json!({ "style": "Normal", "chr": { "italic": true } }))],
                "paragraph" => vec![("caret.set", json!({ "pos": { "block": 1, "off": 0 } })), ("paragraph.align", json!({ "value": "center" }))],
                _ => vec![("paragraph.text", json!({ "index": 1, "text": "Peer three four" }))],
            };
            if outside {
                f.colleague(&steps);
            } else {
                same_server(&f, &steps).await;
            }
            let before = std::fs::read(f.root.join(FILE)).unwrap();
            let result = f.run(Some(basis), json!([{ "command": "paragraph.text", "params": { "index": 1, "text": "Agent paragraph" } }])).await;
            if peer == "assigned" {
                match result {
                    Err(error) => {
                        conflict(&error, "changed");
                        assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
                    }
                    Ok(_) => failures.push(format!("outside {outside}: assigned text changed")),
                }
            } else if let Err(error) = result {
                failures.push(format!("peer {peer}, outside {outside}: {error}"));
            } else {
                let mut saved = f.disk();
                let mut texts: Vec<String> = TEXTS.iter().map(|s| s.to_string()).collect();
                texts[1] = "Agent paragraph".into();
                assert_eq!(f.texts(), texts);
                if peer == "paragraph" {
                    assert_eq!(saved.execute("file.paragraph", json!({ "path": [1] })).unwrap()["paragraph"]["props"]["align"], "center");
                } else {
                    let data: Value = serde_json::from_slice(&saved.save(Some("check.json")).unwrap()).unwrap();
                    assert!(data["styles"]["styles"].as_array().unwrap().iter().any(|s| s["id"] == "Normal" && s["chr"]["italic"] == true));
                }
            }
            f.close().await;
        }
        let mut doc = document();
        doc.execute("caret.set", json!({ "pos": { "block": 1, "off": 0 } })).unwrap();
        doc.execute("paragraph.set", json!({ "props": { "style": "Heading1" } })).unwrap();
        let f = Fixture::with(Guard::Premise, &doc.save(None).unwrap());
        let basis = f.basis().await;
        let steps = [("style.set", json!({ "style": "Heading1", "next": "Heading2" }))];
        if outside {
            f.colleague(&steps);
        } else {
            same_server(&f, &steps).await;
        }
        let before = std::fs::read(f.root.join(FILE)).unwrap();
        match f.run(Some(basis), json!([{ "command": "paragraph.text", "params": { "index": 1, "text": "Agent\rNext" } }])).await {
            Err(error) => {
                conflict(&error, "changed");
                assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
            }
            Ok(_) => failures.push(format!("outside {outside}: changed split style applied")),
        }
        f.close().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
