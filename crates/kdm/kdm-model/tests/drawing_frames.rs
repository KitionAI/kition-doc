//! A shape's frame as its own register (schema version 4, `KitDrawing 1.0` DR14): the replica keeps
//! a shape's position and size apart from its other members.

mod common;

use common::{SharedBlobs, append_object, first, para};
use kition_doc_doc::para::{Float, InlineObject, ShapeKind, Wrap};
use kition_doc_doc::props::Rgb;
use kition_doc_doc::{Document, StoryRef};

fn shape(uid: u32) -> InlineObject {
    InlineObject::Shape {
        kind: ShapeKind::Ellipse,
        w: 100.0,
        h: 50.0,
        fill: Some(Rgb(200, 0, 0)),
        stroke: None,
        stroke_width: 0.0,
        float: Float { wrap: Wrap::Square, x: 10.0, y: 20.0, ..Float::default() },
        story: None,
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid,
        vml: None,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    }
}

fn doc() -> Document {
    let mut d = Document::new();
    d.body = vec![para("Shapes: ")];
    append_object(&mut d, StoryRef::Body, 0, shape(1));
    append_object(&mut d, StoryRef::Body, 0, shape(2));
    d
}

#[test]
fn a_shapes_frame_is_its_own_register() {
    let blobs = SharedBlobs::default();
    let a = first(&doc(), &blobs);
    let replica = a.replica();
    let inline = replica.objects_of_type("Inline");
    assert_eq!(inline.len(), 2);
    for object in inline {
        let value = replica.get(&object, "value").unwrap();
        let frame = replica.get(&object, "frame").unwrap();
        assert!(value.get("w").is_none() && value.get("float").is_none(), "{value}");
        assert_eq!(frame["w"], 100.0);
    }
    assert_eq!(replica.min_schema_version(), kdm_model::SCHEMA_VERSION);
}
