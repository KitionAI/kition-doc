//! Width constraints shared by inline text that must move as one unit.

use std::ops::Range;

#[derive(Default)]
pub(crate) struct Groups {
    ends: Vec<usize>,
    widths: Vec<f32>,
}

impl Groups {
    pub fn contains(&self, index: usize) -> bool {
        self.ends.get(index).is_some_and(|end| *end > index)
    }
    pub fn internal_after(&self, index: usize) -> bool {
        self.ends.get(index).is_some_and(|end| *end > index.saturating_add(1))
    }
    pub fn starting_width(&self, index: usize) -> Option<f32> {
        self.widths.get(index).copied().filter(|width| *width > 0.0)
    }
    pub fn register(&mut self, range: Range<usize>, width: f32, count: usize) -> bool {
        if range.start >= range.end || range.end > count || !width.is_finite() || width <= 0.0 || range.clone().any(|k| self.contains(k)) {
            return false;
        }
        self.ends.resize(count, 0);
        self.widths.resize(count, 0.0);
        if let Some(ends) = self.ends.get_mut(range.clone()) {
            ends.fill(range.end);
        }
        if let Some(value) = self.widths.get_mut(range.start) {
            *value = width;
        }
        true
    }
}
