//! Theme colour and font references and paragraph ids (Phase 7 fidelity): they replicate as
//! three more run marks and optional fields of existing JSON registers, so the schema version
//! does not change and a 0.4.x build (whose types lack them) reads the document; two replicas
//! editing other fields one after the other agree on them.

mod common;

use common::{SharedBlobs, assert_same, edit, first, new_replica, para, peer, sync};
use kdm_model::read_document;
use kition_doc_doc::props::{Rgb, TextColor, ThemeColor, ThemeFonts, ThemeSlot};
use kition_doc_doc::{Block, Document};
use serde::Deserialize;
use serde_json::Value;

const ACCENT: ThemeColor = ThemeColor { slot: ThemeSlot::Accent1, tint: None, shade: Some(0xBF), rgb: Rgb(0x2F, 0x54, 0x96) };

fn document() -> Document {
    let mut d = Document::new();
    d.body = vec![para("Themed title"), para("Body text")];
    if let Block::Para(p) = std::sync::Arc::make_mut(&mut d.body[0]) {
        p.props.para_id = Some(0x1A2B_3C4D);
        p.props.shading_theme = Some(ThemeColor { slot: ThemeSlot::Background1, tint: None, shade: Some(0xF2), rgb: Rgb(0xF2, 0xF2, 0xF2) });
        p.props.shading = Some(Rgb(0xF2, 0xF2, 0xF2));
        p.format(0, 6, &|c| {
            c.color = Some(TextColor::Rgb(ACCENT.rgb));
            c.color_theme = Some(ACCENT);
            c.font_theme = Some(Box::new(ThemeFonts { ascii: Some("majorHAnsi".into()), ..Default::default() }));
        })
        .unwrap_or_else(|e| panic!("{e}"));
    }
    let h1 = d.styles.styles.iter_mut().find(|s| s.id == "Heading1").unwrap_or_else(|| panic!("Heading1"));
    h1.chr.color = Some(TextColor::Rgb(ACCENT.rgb));
    h1.chr.color_theme = Some(ACCENT);
    d
}

/// Paragraph properties as 0.4.x builds have them (a few fields; no ids or theme colours).
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
#[allow(dead_code)]
struct ParaPropsV04 {
    style: Option<String>,
    shading: Option<[u8; 3]>,
}

/// A run's marks as a 0.4.x build reads them into its character properties.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
#[allow(dead_code)]
struct CharPropsV04 {
    color: Option<Value>,
    bold: Option<bool>,
}

#[test]
fn theme_references_and_paragraph_ids_replicate_and_older_builds_read_them() {
    let doc = document();
    let blobs = SharedBlobs::default();
    let (replica, ids) = new_replica(&doc, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_same("kept", &doc, &read);

    // The previous schema's types read the paragraph and its runs' marks.
    let meta: Value = replica.get(&ids.body[0].object, "para").unwrap_or_else(|error| panic!("para: {error}"));
    let props = meta.get("props").cloned().unwrap_or_default();
    assert_eq!(props.get("paraId").and_then(Value::as_u64), Some(0x1A2B_3C4D));
    let old: ParaPropsV04 = serde_json::from_value(props).unwrap_or_else(|error| panic!("0.4.x paragraph props: {error}"));
    assert_eq!(old.shading, Some([0xF2, 0xF2, 0xF2]));
    let runs = replica.text_delta(&ids.body[0].object, "text").unwrap_or_else(|error| panic!("text: {error}"));
    let marks = Value::Object(runs[0].attributes.clone().into_iter().collect());
    assert!(marks.get("colorTheme").is_some() && marks.get("fontTheme").is_some(), "{marks}");
    let old: CharPropsV04 = serde_json::from_value(marks).unwrap_or_else(|error| panic!("0.4.x run marks: {error}"));
    assert!(old.color.is_some());
}

#[test]
fn two_replicas_agree_on_theme_references() {
    let blobs = SharedBlobs::default();
    let mut a = first(&document(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    // One edits the themed run's colour (its reference no longer holds); the other gives the
    // second paragraph an id and a themed shading.
    edit(&mut a, &blobs, |d| {
        if let Block::Para(p) = std::sync::Arc::make_mut(&mut d.body[0]) {
            p.format(0, 3, &|c| c.color = Some(TextColor::Rgb(Rgb(1, 2, 3)))).unwrap_or_else(|e| panic!("{e}"));
        }
    });
    sync(&mut a, &mut b);
    edit(&mut b, &blobs, |d| {
        if let Block::Para(p) = std::sync::Arc::make_mut(&mut d.body[1]) {
            p.props.para_id = Some(0x2B3C_4D5E);
            p.props.shading = Some(Rgb(0x44, 0x72, 0xC4));
            p.props.shading_theme = Some(ThemeColor { slot: ThemeSlot::Accent1, tint: None, shade: None, rgb: Rgb(0x44, 0x72, 0xC4) });
        }
    });
    sync(&mut a, &mut b);
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_same("views", &a.model().document, &b.model().document);
    let d = &a.model().document;
    let Block::Para(p0) = &*d.body[0] else { panic!("para") };
    assert_eq!(p0.props.para_id, Some(0x1A2B_3C4D));
    assert_eq!(p0.runs[0].props.color, Some(TextColor::Rgb(Rgb(1, 2, 3))));
    assert!(!p0.runs[0].props.color_theme.is_some_and(|t| t.holds(Some(Rgb(1, 2, 3)))), "an edited colour is not themed");
    assert_eq!(p0.runs[1].props.color_theme, Some(ACCENT), "the rest of the run keeps its reference");
    let Block::Para(p1) = &*d.body[1] else { panic!("para") };
    assert_eq!(p1.props.para_id, Some(0x2B3C_4D5E));
    assert_eq!(p1.props.shading_theme.map(|t| t.slot), Some(ThemeSlot::Accent1));
    assert_eq!(d.styles.get("Heading1").and_then(|s| s.chr.color_theme), Some(ACCENT));
}
