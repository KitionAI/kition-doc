//! Anchor points and optional Bezier handles, using the shared drawing geometry.

use std::collections::BTreeMap;

use kition_office_core::kurbo::{BezPath, Point};
use serde::{Deserialize, Serialize};

use super::{FreePath, MAX_COORD, MAX_POINTS};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "points", rename_all = "camelCase")]
pub enum BezierControls {
    Quadratic([f32; 2]),
    Cubic([[f32; 2]; 2]),
}

impl BezierControls {
    fn finite(&self) -> bool {
        match self {
            Self::Quadratic(point) => point.iter().all(|value| value.is_finite()),
            Self::Cubic(points) => points.iter().flatten().all(|value| value.is_finite()),
        }
    }

    fn clamped(self) -> Self {
        let clamp = |point: [f32; 2]| point.map(|value| value.clamp(-MAX_COORD, MAX_COORD));
        match self {
            Self::Quadratic(point) => Self::Quadratic(clamp(point)),
            Self::Cubic(points) => Self::Cubic(points.map(clamp)),
        }
    }
}

/// Handles for the segment from anchor `end - 1` to anchor `end`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bezier {
    pub end: usize,
    pub controls: BezierControls,
}

impl FreePath {
    pub(super) fn sanitize_geometry(&mut self) {
        if self.curves.is_empty() {
            self.pts.retain(|point| point.iter().all(|value| value.is_finite()));
            self.pts.truncate(MAX_POINTS);
            for point in &mut self.pts {
                *point = point.map(|value| value.clamp(-MAX_COORD, MAX_COORD));
            }
            return;
        }
        let mut indices = BTreeMap::new();
        self.pts = std::mem::take(&mut self.pts)
            .into_iter()
            .enumerate()
            .filter(|(_, point)| point.iter().all(|value| value.is_finite()))
            .take(MAX_POINTS)
            .enumerate()
            .map(|(new, (old, point))| {
                indices.insert(old, new);
                point.map(|value| value.clamp(-MAX_COORD, MAX_COORD))
            })
            .collect();
        let mut curves = BTreeMap::new();
        for curve in self.curves.drain(..).take(MAX_POINTS) {
            let Some(previous) = curve.end.checked_sub(1) else { continue };
            let Some(&end) = indices.get(&curve.end) else { continue };
            if end == 0 || indices.get(&previous) != end.checked_sub(1).as_ref() || !curve.controls.finite() {
                continue;
            }
            curves.entry(end).or_insert(Bezier { end, controls: curve.controls.clamped() });
        }
        self.curves = curves.into_values().collect();
    }

    /// The exact contour. Curves add handles to anchors; an ink stroke has no handles.
    pub fn outline(&self) -> BezPath {
        let mut curves = BTreeMap::new();
        for curve in self.curves.iter().take(MAX_POINTS).filter(|curve| curve.end > 0 && curve.controls.finite()) {
            curves.entry(curve.end).or_insert(&curve.controls);
        }
        let point = |[x, y]: [f32; 2]| Point::new(f64::from(x.clamp(-MAX_COORD, MAX_COORD)), f64::from(y.clamp(-MAX_COORD, MAX_COORD)));
        let mut path = BezPath::new();
        for (index, &anchor) in self.pts.iter().take(MAX_POINTS).enumerate() {
            if !anchor.iter().all(|value| value.is_finite()) {
                continue;
            }
            let target = point(anchor);
            if path.elements().is_empty() {
                path.move_to(target);
                continue;
            }
            match curves.get(&index) {
                Some(BezierControls::Quadratic(control)) => path.quad_to(point(*control), target),
                Some(BezierControls::Cubic([first, second])) => path.curve_to(point(*first), point(*second), target),
                None => path.line_to(target),
            }
        }
        if self.closed && !path.elements().is_empty() {
            path.close_path();
        }
        path
    }
}
