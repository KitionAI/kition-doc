//! DrawingML colours shared by charts, smart graphics and shapes, read by the code the Office kits share
//! ([`kition_office_core::drawingml`]) in Doc's colour type.

use kition_doc_doc::props::Rgb;
use kition_office_core::drawingml;

use crate::xml::El;

/// The theme in the shared colour type.
fn shared(theme: &[Rgb]) -> Vec<kition_office_core::model::Rgb> {
    theme.iter().map(|c| (*c).into()).collect()
}

/// Theme colour `i` (0 dk1, 1 lt1, 2 dk2, 3 lt2, 4..9 accent1..6, 10 hlink, 11 folHlink), falling
/// back to the default theme.
pub(crate) fn slot(theme: &[Rgb], i: usize) -> Rgb {
    drawingml::slot(&shared(theme), i).into()
}

/// The fill of a `…Pr` or `a:ln` element (see [`drawingml::fill_colour`]).
pub(crate) fn fill_colour(e: &El, theme: &[Rgb]) -> Option<Rgb> {
    drawingml::fill_colour(e, &shared(theme)).map(Into::into)
}

/// A gradient fill (`a:gradFill`) as Doc keeps it (see [`drawingml::gradient`]).
pub(crate) fn gradient(e: &El, theme: &[Rgb]) -> Option<kition_doc_doc::graphic::ShapeGradient> {
    let g = drawingml::gradient(e, &shared(theme))?;
    let stops = g.stops.into_iter().map(|(pos, color)| kition_doc_doc::graphic::GradientStop { pos, color: color.into() }).collect();
    Some(kition_doc_doc::graphic::ShapeGradient { stops, angle: g.angle, path: g.path })
}

/// A colour element (or the one inside `e`) with its modifiers (see [`drawingml::color`]).
pub(crate) fn color(e: &El, theme: &[Rgb]) -> Option<Rgb> {
    drawingml::color(e, &shared(theme)).map(Into::into)
}

/// `c` at `opacity` over a white page: how a translucent colour shows there.
pub(crate) fn over_white(c: Rgb, opacity: f64) -> Rgb {
    drawingml::over_white(c.into(), opacity).into()
}
