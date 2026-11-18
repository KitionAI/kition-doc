//! Custom contours share a paint operation only within their original compound path.

use kition_doc_doc::freeform::Freeform;
use kition_doc_doc::graphic::PathSeg;
use kition_doc_doc::props::Rgb;
use kition_doc_geom::Rect;
use kition_office_core::kurbo::{Affine, PathEl, Point, QuadBez};

use crate::display::Draw;

pub(crate) fn draw(rect: Rect, f: &Freeform, fill: Option<Rgb>, stroke: Option<Rgb>, width: f32, out: &mut Vec<Draw>) {
    let sx = if f.w > 0.0 { rect.w / f.w } else { 1.0 };
    let sy = if f.h > 0.0 { rect.h / f.h } else { 1.0 };
    let (sx, sy) = (if sx.is_finite() { sx } else { 1.0 }, if sy.is_finite() { sy } else { 1.0 });
    let transform = Affine::translate((f64::from(rect.x), f64::from(rect.y))) * Affine::scale_non_uniform(f64::from(sx), f64::from(sy));
    let mut segments = Vec::new();
    let mut paint = None;
    let flush = |segments: &mut Vec<PathSeg>, paint: Option<(Option<Rgb>, Option<Rgb>)>, out: &mut Vec<Draw>| {
        if let Some((fill, stroke)) = paint
            && !segments.is_empty()
        {
            out.push(Draw::Path { segs: std::mem::take(segments), fill, stroke, stroke_width: width, stroke_alpha: f.alpha });
        }
    };
    for ((pts, closed), path) in f.placed(rect.x, rect.y, rect.w, rect.h).into_iter().zip(&f.paths) {
        let outline = stroke.filter(|_| path.stroked);
        if f.is_ink() {
            if let Some(color) = outline {
                let mut pts: Vec<_> = pts.into_iter().map(|[x, y]| (x, y)).collect();
                if closed && let Some(first) = pts.first().copied() {
                    pts.push(first);
                }
                out.push(Draw::Ink { pts, color, width: width.max(0.25), alpha: f.alpha });
            }
            continue;
        }
        let current = (fill.filter(|_| path.filled), outline);
        if !path.joins_previous || paint != Some(current) {
            flush(&mut segments, paint, out);
            paint = Some(current);
        }
        let mut current = Point::ORIGIN;
        for element in (transform * path.outline()).elements() {
            let segment = match *element {
                PathEl::MoveTo(point) => {
                    current = point;
                    PathSeg::Move(point.x as f32, point.y as f32)
                }
                PathEl::LineTo(point) => {
                    current = point;
                    PathSeg::Line(point.x as f32, point.y as f32)
                }
                PathEl::QuadTo(control, end) => {
                    let curve = QuadBez::new(current, control, end).raise();
                    current = end;
                    PathSeg::Cubic(curve.p1.x as f32, curve.p1.y as f32, curve.p2.x as f32, curve.p2.y as f32, end.x as f32, end.y as f32)
                }
                PathEl::CurveTo(first, second, end) => {
                    current = end;
                    PathSeg::Cubic(first.x as f32, first.y as f32, second.x as f32, second.y as f32, end.x as f32, end.y as f32)
                }
                PathEl::ClosePath => PathSeg::Close,
            };
            segments.push(segment);
        }
    }
    flush(&mut segments, paint, out);
}
