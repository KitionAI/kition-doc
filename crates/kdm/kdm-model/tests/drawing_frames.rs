//! A shape's frame as its own register (schema version 4, `KitDrawing 1.0` DR14): one person
//! moving or resizing a shape while another formats it keeps both changes, and two shapes edited
//! at once both keep theirs.

mod common;

use common::{Session, SharedBlobs, append_object, assert_same, body_para, edit, first, para, peer, sync};
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

fn shapes(s: &Session) -> Vec<InlineObject> {
    s.model().document.shapes().into_iter().map(|(_, o)| o.clone()).collect()
}

#[test]
fn a_move_and_a_format_of_one_shape_both_stay() {
    let blobs = SharedBlobs::default();
    let mut a = first(&doc(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    edit(&mut a, &blobs, |d| {
        if let Some(InlineObject::Shape { float, w, .. }) = body_para(d, 0).objects.get_mut(0) {
            float.x = 150.0;
            *w = 140.0;
        }
    });
    edit(&mut b, &blobs, |d| {
        if let Some(InlineObject::Shape { fill, kind, .. }) = body_para(d, 0).objects.get_mut(0) {
            *fill = Some(Rgb(0, 200, 0));
            *kind = ShapeKind::Star;
        }
        if let Some(InlineObject::Shape { float, .. }) = body_para(d, 0).objects.get_mut(1) {
            float.y = 99.0;
        }
    });
    sync(&mut a, &mut b);
    assert_same("after a move and a format", &a.model().document, &b.model().document);
    let s = shapes(&a);
    let InlineObject::Shape { float, w, fill, kind, .. } = &s[0] else { panic!() };
    assert_eq!((float.x, *w), (150.0, 140.0), "A's move and resize stay");
    assert_eq!((*fill, *kind), (Some(Rgb(0, 200, 0)), ShapeKind::Star), "B's format stays");
    let InlineObject::Shape { float, .. } = &s[1] else { panic!() };
    assert_eq!(float.y, 99.0, "B's move of the other shape stays");
}

#[test]
fn an_art_text_warp_a_move_and_a_fill_of_one_shape_all_stay() {
    let blobs = SharedBlobs::default();
    let mut a = first(&doc(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    let arch = kition_doc_doc::graphic::PresetGeom { name: "textArchUp".into(), adj: vec![("adj".into(), 10_800_000.0)] };
    edit(&mut a, &blobs, |d| {
        if let Some(InlineObject::Shape { warp, .. }) = body_para(d, 0).objects.get_mut(0) {
            *warp = Some(arch.clone());
        }
    });
    edit(&mut b, &blobs, |d| {
        if let Some(InlineObject::Shape { float, fill, .. }) = body_para(d, 0).objects.get_mut(0) {
            float.x = 222.0;
            *fill = Some(Rgb(1, 2, 3));
        }
    });
    sync(&mut a, &mut b);
    assert_same("after a warp, a move and a fill", &a.model().document, &b.model().document);
    let InlineObject::Shape { warp, float, fill, .. } = &shapes(&b)[0] else { panic!() };
    // Each member of a shape is its own entry (schema 6): A's warp and B's fill both stay.
    assert_eq!((warp.clone(), float.x, *fill), (Some(arch), 222.0, Some(Rgb(1, 2, 3))), "A's warp, B's move and fill all stay");
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

/// A replica as schema 3 wrote it: shapes' frames inside their `value`, minimum version 3.
fn version_3_snapshot() -> Vec<u8> {
    use kition_kit::kdm::{ActorId, DocId, Origin, Replica, Schema};
    let mut v: serde_json::Value = serde_json::from_str(include_str!("../model/schema.json")).unwrap();
    v["schemaVersion"] = serde_json::json!(3);
    let schema = std::sync::Arc::new(Schema::parse(&v.to_string()).unwrap());
    let mut ids = kdm_model::Ids::default();
    let mut sink = SharedBlobs::default();
    let (mut replica, ()) = Replica::create_with(schema, DocId::new(), ActorId::random(), &Origin::system("create"), |tx| {
        kdm_model::write_document(tx, &doc(), &mut ids, &mut sink).map(|_| ())
    })
    .unwrap();
    // Put the frames back into the values, as schema 3 kept them.
    replica
        .transact(&Origin::system("legacy"), |tx| {
            for inline in tx.objects_of_type("Inline") {
                let mut value = tx.get(&inline, "value")?;
                let frame = tx.get(&inline, "frame")?;
                if let (Some(map), Some(frame)) = (value.as_object_mut(), frame.as_object()) {
                    for (k, x) in frame {
                        map.insert(k.clone(), x.clone());
                    }
                }
                tx.set(&inline, "value", value)?;
                tx.set(&inline, "frame", serde_json::Value::Null)?;
            }
            Ok(())
        })
        .unwrap();
    replica.snapshot().unwrap()
}

#[test]
fn a_version_3_replica_reads_and_splits_its_frames_on_the_first_write() {
    use kition_kit::kdm::Origin;
    use kition_kit::session::MemoryBlobs;
    let mut s = kdm_model::DocSession::open_snapshot(&version_3_snapshot(), MemoryBlobs::default()).unwrap();
    let read: Vec<f32> = s.document().shapes().iter().filter_map(|(_, o)| o.frame().map(|(w, _, _)| w)).collect();
    assert_eq!(read, [100.0, 100.0], "the frames read from the values");
    let mut after = s.document().clone();
    after.para_mut(StoryRef::Body, &kition_doc_doc::Path::top(0)).unwrap().insert_text(0, "My ", &Default::default()).unwrap();
    assert_eq!(s.record(&Origin::person("user:a"), &after).unwrap(), 1, "the edit is one undo step");
    assert_eq!(s.replica().min_schema_version(), kdm_model::SCHEMA_VERSION);
    for object in s.replica().objects_of_type("Inline") {
        assert!(s.replica().get(&object, "value").unwrap().get("w").is_none());
        assert_eq!(s.replica().get(&object, "frame").unwrap()["w"], 100.0);
    }
}

/// An edited chart's description is its own object (schema 5, DR14): one person retitling a chart,
/// another changing its data and a third moving it all keep their change, and every replica draws
/// the merged chart from its part and description.
#[test]
fn a_title_edit_a_data_edit_and_a_move_of_one_chart_all_stay() {
    use kition_doc_doc::graphic::{Graphic, GraphicKind};
    use kition_office_core::chart::{ChartKind, ChartSpec, codec};
    use std::sync::Arc;
    let spec = ChartSpec::sample(ChartKind::Column);
    let made = Graphic {
        kind: GraphicKind::Chart,
        uid: 7,
        chart: Some(Arc::new(codec::write(&spec))),
        spec: Some(Box::new(spec.clone())),
        ..Default::default()
    };
    let mut d = Document::new();
    d.body = vec![para("Chart: ")];
    let obj = InlineObject::Graphic {
        w: 300.0,
        h: 200.0,
        alt: String::new(),
        float: Float { wrap: Wrap::Square, x: 10.0, y: 20.0, ..Float::default() },
        graphic: Arc::new(made),
    };
    append_object(&mut d, StoryRef::Body, 0, obj);
    let blobs = SharedBlobs::default();
    let mut a = first(&d, &blobs);
    let mut b = peer(&a, 2, &blobs);
    let change = |s: &mut Session, f: &dyn Fn(&mut ChartSpec, &mut Float)| {
        edit(s, &blobs, |d| {
            if let Some(InlineObject::Graphic { graphic, float, .. }) = body_para(d, 0).objects.get_mut(0) {
                let g = Arc::make_mut(graphic);
                let mut spec = g.spec.as_deref().cloned().unwrap_or_default();
                f(&mut spec, float);
                g.spec = Some(Box::new(spec));
            }
        });
    };
    change(&mut a, &|spec, _| spec.title = Some("From A".into()));
    change(&mut b, &|spec, float| {
        spec.categories = vec!["North".into(), "South".into(), "East".into()];
        float.x = 150.0;
    });
    sync(&mut a, &mut b);
    assert_same("after a title edit, a data edit and a move", &a.model().document, &b.model().document);
    let objects = shapes(&a);
    let [InlineObject::Graphic { float, graphic, .. }] = objects.as_slice() else { panic!("one chart") };
    let spec = graphic.spec.as_deref().unwrap_or_else(|| panic!("the description"));
    assert_eq!(spec.title.as_deref(), Some("From A"));
    assert_eq!(spec.categories, ["North", "South", "East"]);
    assert_eq!((graphic.uid, float.x), (7, 150.0), "the id, and the move beside the edits");
    // Drawn from the merged description: the title's text is among its items.
    assert!(
        graphic.items.iter().any(|i| matches!(i, kition_doc_doc::graphic::GraphicItem::Text { text, .. } if text == "From A")),
        "the merged chart is drawn"
    );
}
