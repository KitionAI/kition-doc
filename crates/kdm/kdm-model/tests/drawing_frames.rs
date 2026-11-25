//! A shape's frame as its own register (schema version 4, `KitDrawing 1.0` DR14): the replica keeps
//! a shape's position and size apart from its other members, so an agent's draft formatting a
//! shape or editing a chart merges with the person moving it meanwhile (data model D3, D4). Each
//! edit is written, reads back exactly, undoes and redoes exactly, and an unchanged view then
//! writes nothing.

mod common;

use common::{Session, SharedBlobs, append_object, assert_same, assert_unchanged_writes_nothing, body_para, first, merge_draft, para};
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

fn shapes(s: &Session) -> Vec<InlineObject> {
    s.model().document.shapes().into_iter().map(|(_, o)| o.clone()).collect()
}

#[test]
fn a_move_and_a_format_of_one_shape_both_stay() {
    let blobs = SharedBlobs::default();
    let mut a = first(&doc(), &blobs);
    merge_draft(
        &mut a,
        &blobs,
        |d| {
            if let Some(InlineObject::Shape { fill, kind, .. }) = body_para(d, 0).objects.get_mut(0) {
                *fill = Some(Rgb(0, 200, 0));
                *kind = ShapeKind::Star;
            }
            if let Some(InlineObject::Shape { float, .. }) = body_para(d, 0).objects.get_mut(1) {
                float.y = 99.0;
            }
        },
        |d| {
            if let Some(InlineObject::Shape { float, w, .. }) = body_para(d, 0).objects.get_mut(0) {
                float.x = 150.0;
                *w = 140.0;
            }
        },
    );
    let s = shapes(&a);
    let InlineObject::Shape { float, w, fill, kind, .. } = &s[0] else { panic!() };
    assert_eq!((float.x, *w), (150.0, 140.0), "the person's move and resize stay");
    assert_eq!((*fill, *kind), (Some(Rgb(0, 200, 0)), ShapeKind::Star), "the draft's format stays");
    let InlineObject::Shape { float, .. } = &s[1] else { panic!() };
    assert_eq!(float.y, 99.0, "the draft's move of the other shape stays");
    assert_unchanged_writes_nothing(&mut a, &blobs);
}

#[test]
fn an_art_text_warp_a_move_and_a_fill_of_one_shape_all_stay() {
    let blobs = SharedBlobs::default();
    let mut a = first(&doc(), &blobs);
    let arch = kition_doc_doc::graphic::PresetGeom { name: "textArchUp".into(), adj: vec![("adj".into(), 10_800_000.0)] };
    let warp_to = arch.clone();
    merge_draft(
        &mut a,
        &blobs,
        move |d| {
            if let Some(InlineObject::Shape { warp, .. }) = body_para(d, 0).objects.get_mut(0) {
                *warp = Some(warp_to);
            }
        },
        |d| {
            if let Some(InlineObject::Shape { float, fill, .. }) = body_para(d, 0).objects.get_mut(0) {
                float.x = 222.0;
                *fill = Some(Rgb(1, 2, 3));
            }
        },
    );
    let InlineObject::Shape { warp, float, fill, .. } = &shapes(&a)[0] else { panic!() };
    // Each member of a shape is its own entry: the draft's warp and the person's fill both stay.
    assert_eq!((warp.clone(), float.x, *fill), (Some(arch), 222.0, Some(Rgb(1, 2, 3))), "the warp, the move and the fill all stay");
    assert_unchanged_writes_nothing(&mut a, &blobs);
    // Taking the warp off deletes its entry (a map holds no `null`), and writes nothing more.
    let before = a.model().document.clone();
    common::edit(&mut a, &blobs, |d| {
        if let Some(InlineObject::Shape { warp, .. }) = body_para(d, 0).objects.get_mut(0) {
            *warp = None;
        }
    });
    common::assert_fresh(&a);
    let InlineObject::Shape { warp, .. } = &shapes(&a)[0] else { panic!() };
    assert_eq!(*warp, None);
    assert_unchanged_writes_nothing(&mut a, &blobs);
    assert!(a.undo().unwrap().0);
    assert_same("the warp back", &a.model().document, &before);
}

/// An edited chart's description is its own object (DR14): an agent's draft retitling a chart
/// merges with the person changing its data and moving it meanwhile, and the document draws the
/// merged chart from its part and description.
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
    fn change(d: &mut Document, f: &dyn Fn(&mut ChartSpec, &mut Float)) {
        if let Some(InlineObject::Graphic { graphic, float, .. }) = body_para(d, 0).objects.get_mut(0) {
            let g = Arc::make_mut(graphic);
            let mut spec = g.spec.as_deref().cloned().unwrap_or_default();
            f(&mut spec, float);
            g.spec = Some(Box::new(spec));
        }
    }
    merge_draft(
        &mut a,
        &blobs,
        |d| change(d, &|spec, _| spec.title = Some("From the draft".into())),
        |d| {
            change(d, &|spec, float| {
                spec.categories = vec!["North".into(), "South".into(), "East".into()];
                float.x = 150.0;
            })
        },
    );
    let objects = shapes(&a);
    let [InlineObject::Graphic { float, graphic, .. }] = objects.as_slice() else { panic!("one chart") };
    let spec = graphic.spec.as_deref().unwrap_or_else(|| panic!("the description"));
    assert_eq!(spec.title.as_deref(), Some("From the draft"));
    assert_eq!(spec.categories, ["North", "South", "East"]);
    assert_eq!((graphic.uid, float.x), (7, 150.0), "the id, and the move beside the edits");
    // Drawn from the merged description: the title's text is among its items.
    assert!(
        graphic.items.iter().any(|i| matches!(i, kition_doc_doc::graphic::GraphicItem::Text { text, .. } if text == "From the draft")),
        "the merged chart is drawn"
    );
    assert_unchanged_writes_nothing(&mut a, &blobs);
}
