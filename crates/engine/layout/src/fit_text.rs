//! Manual run widths are measured over a contiguous group, after font fallback shaping.

use kition_doc_doc::props::FitText;
use std::collections::HashMap;

use crate::inline_groups::Groups;
use crate::para::{ClKind, Cluster, Glyph, StyleRun};

pub(crate) fn apply(
    runs: &[(std::ops::Range<usize>, FitText)],
    styles: &mut Vec<StyleRun>,
    glyphs: &mut [Glyph],
    clusters: &mut [Cluster],
) -> Groups {
    let mut ranges: Vec<(std::ops::Range<usize>, kition_doc_doc::props::FitText)> = Vec::new();
    for (range, fit) in runs {
        match ranges.last_mut() {
            Some((last, previous)) if last.end == range.start && fit.id.is_some() && previous.id == fit.id => last.end = range.end,
            _ => ranges.push((range.clone(), *fit)),
        }
    }
    if ranges.is_empty() {
        return Groups::default();
    }
    let mut groups = Groups::default();
    let cluster_count = clusters.len();
    for (range, fit) in ranges {
        let first = clusters.partition_point(|c| c.end <= range.start);
        let end = clusters.partition_point(|c| c.start < range.end);
        let Some(members) = clusters.get_mut(first..end).filter(|members| !members.is_empty()) else { continue };
        // Explicit controls and objects have independent geometry. Do not scale them
        // through an ordinary text-width constraint.
        if members.iter().any(|c| !matches!(c.kind, ClKind::Text | ClKind::Space | ClKind::Marker)) {
            continue;
        }
        let natural: f32 = members.iter().map(|c| c.adv).sum();
        let factor = fit.width / natural;
        if !natural.is_finite() || natural <= 0.0 || !factor.is_finite() || !(0.000001..=1_000_000.0).contains(&factor) {
            continue;
        }
        let mut mapped = HashMap::new();
        for c in members.iter() {
            if mapped.contains_key(&c.style) {
                continue;
            }
            let Some(mut style) = styles.get(c.style as usize).cloned() else { break };
            let Ok(index) = u16::try_from(styles.len()) else { break };
            style.width_scale *= factor;
            if !style.width_scale.is_finite() || !(0.000001..=1_000_000.0).contains(&style.width_scale) {
                break;
            }
            styles.push(style);
            mapped.insert(c.style, index);
        }
        if members.iter().any(|c| !mapped.contains_key(&c.style)) {
            continue;
        }
        for (offset, c) in members.iter_mut().enumerate() {
            c.adv *= factor;
            c.style = mapped.get(&c.style).copied().unwrap_or(c.style);
            c.break_after = first + offset + 1 == end;
            for g in glyphs.get_mut(c.g0 as usize..c.g1 as usize).into_iter().flatten() {
                g.dx *= factor;
            }
        }
        groups.register(first..end, fit.width, cluster_count);
    }
    groups
}
