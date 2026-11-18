//! What a saved DOCX keeps beyond the drawn model (phase 7 fidelity) replicates: a VML picture
//! kept as its own markup (naming its picture as the object's media, not as a carried part), a
//! VML drawing Kition Doc does not model (kept as a run), and the styles' latent styles and the
//! theme part (verbatim parts). None of it needs a schema change: the picture's markup travels in
//! the `ole` extra a picture already has, the drawing is an `opaque` object, the rest are blobs.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, append_object, assert_same, new_replica, para};
use kdm_model::{decode, encode, read_document};
use kition_doc_doc::graphic::{Embedded, EmbeddedRef};
use kition_doc_doc::{Document, InlineObject, StoryRef};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nstand-in";
const VML: &str = r#"<w:pict xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office"><v:shape id="P1" style="width:40pt;height:30pt"><v:imagedata r:id="rId1" o:title=""/></v:shape></w:pict>"#;
const LINE: &str = r#"<w:r xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:v="urn:schemas-microsoft-com:vml"><w:pict><v:line id="L1" from="0,0" to="10pt,0"/></w:pict></w:r>"#;
const LATENT: &str = r#"<w:latentStyles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:count="1"><w:lsdException w:name="Normal" w:qFormat="1"/></w:latentStyles>"#;

fn document() -> Document {
    let mut d = Document::new();
    d.body = vec![para("Pictures"), para("Lines")];
    let media = d.add_media(PNG.to_vec(), "png");
    let ole = Embedded {
        xml: VML.into(),
        refs: vec![EmbeddedRef {
            id: "rId1".into(),
            kind: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image".into(),
            target: "kition-doc:media".into(),
            external: false,
        }],
    };
    let image = InlineObject::Image {
        edge: Default::default(),
        geom: None,
        media,
        w: 40.0,
        h: 30.0,
        alt: String::new(),
        float: Default::default(),
        crop: [0.0; 4],
        ole: Some(Arc::new(ole)),
        adjust: Default::default(),
        uid: 0,
    };
    append_object(&mut d, StoryRef::Body, 0, image);
    append_object(&mut d, StoryRef::Body, 1, InlineObject::Opaque { format: "docx".into(), xml: LINE.into(), text: String::new() });
    d.passthrough.insert("kition-doc:styles.latentStyles".into(), Arc::new(LATENT.as_bytes().to_vec()));
    d
}

fn kept_picture(d: &Document) -> Option<Arc<Embedded>> {
    d.body.iter().filter_map(|b| b.as_para()).flat_map(|p| &p.objects).find_map(|o| match o {
        InlineObject::Image { ole, .. } => ole.clone(),
        _ => None,
    })
}

#[test]
fn kept_vml_and_verbatim_parts_read_back_from_a_replica() {
    let doc = document();
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&doc, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_same("kept", &doc, &read);
    assert_eq!(kept_picture(&read), kept_picture(&doc));
    assert_eq!(read.passthrough.get("kition-doc:styles.latentStyles"), doc.passthrough.get("kition-doc:styles.latentStyles"));

    // And saved from the replica's view: the picture stays VML, the line stays, the latent styles too.
    let bytes = encode("Kept.docx", &read).unwrap_or_else(|error| panic!("encode: {error}"));
    let again = decode("Kept.docx", &bytes).unwrap_or_else(|error| panic!("decode: {error}"));
    let picture = kept_picture(&again).unwrap_or_else(|| panic!("the picture is still VML"));
    assert!(picture.xml.contains("<v:imagedata") && picture.refs.iter().all(|r| r.target == "kition-doc:media"), "{picture:?}");
    let objects: Vec<&InlineObject> = again.body.iter().filter_map(|b| b.as_para()).flat_map(|p| &p.objects).collect();
    assert!(objects.iter().any(|o| matches!(o, InlineObject::Opaque { xml, .. } if xml.contains(r#"<v:line id="L1""#))), "{objects:?}");
    assert!(again.passthrough.get("kition-doc:styles.latentStyles").is_some_and(|b| String::from_utf8_lossy(b).contains(r#"w:name="Normal""#)));
}

/// A VML text box keeps its drawing (without its text) as the shape's `vml` extra, which 0.4.x
/// builds ignore (they write the box as DrawingML), and saving from a replica writes it back.
#[test]
fn a_kept_vml_text_box_drawing_replicates() {
    const BOX: &str = r##"<w:pict xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:v="urn:schemas-microsoft-com:vml"><v:shape id="T1" style="width:100pt;height:40pt" fillcolor="#FFFF00" strokecolor="#0000FF" strokeweight="2pt"><v:textbox><w:txbxContent/></v:textbox></v:shape></w:pict>"##;
    let mut doc = Document::new();
    doc.body = vec![para("Box")];
    let story = doc.add_part(kition_doc_doc::PartKind::TextBox, vec![para("In the box")]);
    let shape = InlineObject::Shape {
        kind: kition_doc_doc::para::ShapeKind::TextBox,
        w: 100.0,
        h: 40.0,
        fill: Some(kition_doc_doc::props::Rgb(255, 255, 0)),
        stroke: Some(kition_doc_doc::props::Rgb(0, 0, 255)),
        stroke_width: 2.0,
        float: Default::default(),
        story: Some(story),
        freeform: None,
        effects: Default::default(),
        vml: Some(Arc::new(Embedded { xml: BOX.into(), refs: Vec::new() })),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    append_object(&mut doc, StoryRef::Body, 0, shape);
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&doc, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_same("kept", &doc, &read);
    let kept = |d: &Document| {
        d.body.iter().filter_map(|b| b.as_para()).flat_map(|p| &p.objects).find_map(|o| match o {
            InlineObject::Shape { vml, .. } => vml.clone(),
            _ => None,
        })
    };
    assert_eq!(kept(&read), kept(&doc));

    let bytes = encode("Box.docx", &read).unwrap_or_else(|error| panic!("encode: {error}"));
    let again = decode("Box.docx", &bytes).unwrap_or_else(|error| panic!("decode: {error}"));
    let shell = kept(&again).unwrap_or_else(|| panic!("the box is still VML"));
    assert!(shell.xml.contains("<v:textbox><w:txbxContent/></v:textbox>"), "{shell:?}");
    let texts: Vec<String> = again.parts.values().flat_map(|p| common::texts(&p.blocks)).collect();
    assert!(texts.iter().any(|t| t == "In the box"), "{texts:?}");
}
