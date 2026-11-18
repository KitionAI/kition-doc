//! A shape's outline from the preset catalog Kition's kits share (`kition-office-core`), so a preset
//! is drawn the same in Word, Sheet and Slide, and every preset a file names is drawn as itself.

use kition_office_core::kurbo::{Affine, BezPath, Vec2};
use kition_office_core::preset::Geometry;

use crate::graphic::PresetGeom;
use crate::para::ShapeKind;

/// The outline of a shape of `kind` (or of preset `geom`) filling the box at (`x`, `y`) sized
/// `w` × `h`, page points. `None` for a line or a freeform without a preset of its own, which are
/// drawn from their own geometry, and for a box that is not finite.
pub fn shape_outline(kind: ShapeKind, geom: Option<&PresetGeom>, x: f64, y: f64, w: f64, h: f64) -> Option<BezPath> {
    if !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite()) {
        return None;
    }
    let (name, adj) = match geom.filter(|g| !g.name.is_empty()) {
        Some(g) => (g.name.as_str(), g.adjust_values()),
        None if matches!(kind, ShapeKind::Line | ShapeKind::Freeform) => return None,
        None => (kind.preset_name(), Vec::new()),
    };
    let g = preset_geometry(name, &adj, w, h)?;
    Some(Affine::translate(Vec2::new(x, y)) * g.outline())
}

/// The geometry of preset `name` in a `w` × `h` box at the origin: its sub-paths, each with how
/// it is filled and whether it is stroked, and its text box. Adjust values not given (or not
/// finite) take the preset's defaults; an unknown preset is a rectangle.
pub fn preset_geometry(name: &str, adj: &[f64], w: f64, h: f64) -> Option<Geometry> {
    let defaults = kition_office_core::preset::info(name).map(|i| i.defaults).unwrap_or(&[]);
    let adj: Vec<f64> = adj.iter().enumerate().map(|(i, v)| if v.is_finite() { *v } else { defaults.get(i).copied().unwrap_or(0.0) }).collect();
    kition_office_core::preset::build(name, w.max(0.0), h.max(0.0), &adj)
        .or_else(|| kition_office_core::preset::build("rect", w.max(0.0), h.max(0.0), &[]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_office_core::kurbo::Shape as _;

    #[test]
    fn kinds_and_presets_come_from_the_catalog() {
        let star = shape_outline(ShapeKind::Star, None, 10.0, 20.0, 100.0, 50.0).unwrap();
        let bb = star.bounding_box();
        assert!((bb.x0 - 10.0).abs() < 0.5 && (bb.x1 - 110.0).abs() < 0.5, "{bb:?}");
        let heart = PresetGeom { name: "heart".into(), adj: vec![] };
        assert!(shape_outline(ShapeKind::Rectangle, Some(&heart), 0.0, 0.0, 10.0, 10.0).is_some());
        // A preset with a guide given and one not: the other takes its default.
        let arrow = PresetGeom { name: "rightArrow".into(), adj: vec![("adj2".into(), 20000.0)] };
        assert!(shape_outline(ShapeKind::Rectangle, Some(&arrow), 0.0, 0.0, 100.0, 40.0).is_some());
        assert!(shape_outline(ShapeKind::Line, None, 0.0, 0.0, 10.0, 10.0).is_none());
        assert!(shape_outline(ShapeKind::Ellipse, None, f64::NAN, 0.0, 10.0, 10.0).is_none());
        // An unknown preset falls back to the frame.
        let odd = PresetGeom { name: "noSuchShape".into(), adj: vec![] };
        assert!(shape_outline(ShapeKind::Rectangle, Some(&odd), 0.0, 0.0, 10.0, 10.0).is_some());
    }
}
