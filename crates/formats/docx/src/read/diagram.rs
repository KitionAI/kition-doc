//! Smart graphics: the ready-made drawing (`dsp:drawing`) Word keeps beside the diagram data,
//! drawn by the reader the Office kits share ([`kition_office_core::smartart`]) and kept as Doc's
//! [`GraphicItem`]s. This side finds the diagram's parts in the package and maps the items.

use kition_doc_doc::graphic::{GraphicItem, PathSeg, TextAlign};
use kition_doc_doc::para::ShapeKind;
use kition_doc_doc::props::Rgb;
use kition_office_core::smartart::{self, Item, Seg};

use super::Reader;
use crate::package::{Rel, Rels, rel_is, rt};
use crate::xml::El;

/// The items of a smart graphic's drawing, scaled so its extent fills the `w` × `h` (points) frame.
/// `data` is the diagram's data model, for the prompts of empty placeholder nodes.
/// `image` maps a relationship id of the drawing part to a `Document::media` key.
pub(crate) fn diagram_items(
    drawing: &El,
    data: Option<&El>,
    theme: &[Rgb],
    w: f32,
    h: f32,
    image: &mut dyn FnMut(&str) -> Option<String>,
) -> Vec<GraphicItem> {
    let theme: Vec<kition_office_core::model::Rgb> = theme.iter().map(|c| (*c).into()).collect();
    smartart::items(drawing, data, &theme, w, h, image).into_iter().map(item).collect()
}

/// Doc's [`ShapeKind`] for one of [`smartart::SIMPLE_PRESETS`].
fn kind_of(prst: &str) -> ShapeKind {
    match prst {
        "roundRect" => ShapeKind::RoundedRectangle,
        "ellipse" => ShapeKind::Ellipse,
        "triangle" => ShapeKind::Triangle,
        "diamond" => ShapeKind::Diamond,
        "rightArrow" => ShapeKind::Arrow,
        "star5" => ShapeKind::Star,
        "heart" => ShapeKind::Heart,
        _ => ShapeKind::Rectangle,
    }
}

fn item(i: Item) -> GraphicItem {
    let rgb = |c: Option<kition_office_core::model::Rgb>| c.map(Rgb::from);
    match i {
        Item::Preset { rect, prst, fill, stroke, stroke_width } => {
            GraphicItem::Shape { rect, kind: kind_of(&prst), fill: rgb(fill), stroke: rgb(stroke), stroke_width }
        }
        Item::Path { segs, fill, stroke, stroke_width } => GraphicItem::Path {
            segs: segs
                .into_iter()
                .map(|s| match s {
                    Seg::Move(x, y) => PathSeg::Move(x, y),
                    Seg::Line(x, y) => PathSeg::Line(x, y),
                    Seg::Cubic(a, b, c, d, x, y) => PathSeg::Cubic(a, b, c, d, x, y),
                    Seg::Close => PathSeg::Close,
                })
                .collect(),
            fill: rgb(fill),
            stroke: rgb(stroke),
            stroke_width,
        },
        Item::Text { rect, text, size, color, bold, align, font, prompt } => GraphicItem::Text {
            rect,
            text,
            size,
            color: color.into(),
            bold,
            align: match align {
                smartart::TextAlign::Left => TextAlign::Left,
                smartart::TextAlign::Center => TextAlign::Center,
                smartart::TextAlign::Right => TextAlign::Right,
            },
            font,
            prompt,
        },
        Item::Image { rect, media } => GraphicItem::Image { rect, media },
    }
}

impl Reader<'_> {
    /// The drawing part of a smart graphic (`a:graphicData` with `dgm:relIds`): the one its
    /// data part names, else the drawing numbered like the data part. `None` when there's none.
    pub(super) fn diagram_part(&mut self, gd: &El, rels: &Rels) -> Option<String> {
        let data = self.diagram_data(gd, rels)?;
        let ext_id = self.graphic_part(&data).and_then(|d| d.find("dsp:dataModelExt").and_then(|e| e.attr("relId")).map(str::to_string));
        let is_drawing = |r: &&Rel| !r.external && rel_is(&r.kind, rt::DIAGRAM_DRAWING);
        let drawing = ext_id.as_deref().and_then(|id| rels.by_id(id)).filter(is_drawing).or_else(|| {
            // No usable relationship id: the drawing whose number matches the data part's.
            let num = |t: &str| t.rsplit('/').next().unwrap_or(t).chars().filter(char::is_ascii_digit).collect::<String>();
            let n = num(&data);
            rels.list.iter().filter(is_drawing).find(|r| !n.is_empty() && num(&r.target) == n)
        });
        drawing.map(|r| r.target.clone())
    }

    /// The id of the relationship to a smart graphic's drawing part, as its data part names it
    /// (`dsp:dataModelExt relId`): the id the parts use, not the markup, so it's kept with them.
    pub(super) fn diagram_drawing_rel(&mut self, gd: &El, rels: &Rels) -> Option<String> {
        let data = self.diagram_data(gd, rels)?;
        let id = self.graphic_part(&data)?.find("dsp:dataModelExt")?.attr("relId")?.to_string();
        rels.by_id(&id).is_some_and(|r| !r.external && rel_is(&r.kind, rt::DIAGRAM_DRAWING)).then_some(id)
    }

    /// The data model part of a smart graphic (`dgm:relIds r:dm`).
    pub(super) fn diagram_data(&mut self, gd: &El, rels: &Rels) -> Option<String> {
        super::part_of(rels, gd.child("dgm:relIds")?.attr("r:dm")?, rt::DIAGRAM_DATA)
    }

    /// The items of the smart graphic's drawing part at `path`, fitted to `w` × `h` points, with the
    /// prompts of the data model part at `data`.
    pub(super) fn diagram_drawing(&mut self, path: &str, data: Option<&str>, w: f32, h: f32) -> Vec<GraphicItem> {
        let Some(root) = self.graphic_part(path) else { return Vec::new() };
        let model = data.and_then(|d| self.graphic_part(d));
        let drels = self.pkg.rels(path);
        let theme = self.doc.settings.theme_colors.clone();
        let mut image = |id: &str| self.media_for(&drels, id);
        diagram_items(&root, model.as_deref(), &theme, w, h, &mut image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_doc_doc::THEME_COLORS;

    #[test]
    fn shared_items_become_docs_graphic_items() {
        // The shared reader's items in Doc's types: a rounded rectangle in the theme's accent with
        // its text, the theme passed through in Doc's colours.
        let x = r#"<dsp:drawing xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><dsp:spTree><dsp:sp><dsp:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="roundRect"><a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val="accent1"/></a:solidFill><a:ln><a:noFill/></a:ln></dsp:spPr><dsp:txBody><a:bodyPr/><a:p><a:r><a:rPr sz="2000"/><a:t>Foo</a:t></a:r></a:p></dsp:txBody></dsp:sp></dsp:spTree></dsp:drawing>"#;
        let root = crate::xml::parse(x.as_bytes()).unwrap();
        let out = diagram_items(&root, None, &THEME_COLORS, 100.0, 50.0, &mut |_| None);
        assert!(matches!(&out[0], GraphicItem::Shape { kind: ShapeKind::RoundedRectangle, fill: Some(Rgb(0x15, 0x60, 0x82)), .. }), "{out:?}");
        assert!(matches!(&out[1], GraphicItem::Text { text, size, .. } if text == "Foo" && *size == 20.0), "{out:?}");
    }
}
