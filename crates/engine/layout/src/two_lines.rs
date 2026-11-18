//! Two sub-rows in one logical line, with independent source-character geometry.

use std::collections::HashMap;
use std::ops::Range;

use kition_doc_doc::props::{CombineBrackets, EastAsianLayout, LineSpacing};
use kition_doc_doc::resolve::ResolvedPara;
use kition_doc_geom::Rect;

use crate::inline_groups::Groups;
use crate::para::{ClKind, Cluster, Glyph, Line, ParaLayout, StyleRun, VisCl};

#[derive(Clone, Debug)]
pub struct CombinedGroup {
    pub clusters: Range<usize>,
    pub split: usize,
    /// Horizontal position and visible advance of every source cluster, relative to the group.
    pub positions: Vec<(f32, f32)>,
}

fn group_at(paragraph: &ParaLayout, k: usize) -> Option<&CombinedGroup> {
    let index = paragraph.combined.partition_point(|group| group.clusters.end <= k);
    paragraph.combined.get(index).filter(|group| group.clusters.contains(&k))
}

fn groups_on_line<'a>(paragraph: &'a ParaLayout, line: &Line) -> &'a [CombinedGroup] {
    let first = paragraph.combined.partition_point(|group| group.clusters.end <= line.c0);
    let end = paragraph.combined.partition_point(|group| group.clusters.start < line.c1);
    paragraph.combined.get(first..end).unwrap_or(&[])
}

pub(crate) fn has_group(paragraph: &ParaLayout, line: &Line) -> bool {
    !groups_on_line(paragraph, line).is_empty()
}

pub(crate) fn apply(
    runs: &[(Range<usize>, EastAsianLayout)],
    paragraph: &ResolvedPara,
    styles: &mut Vec<StyleRun>,
    glyphs: &mut [Glyph],
    clusters: &mut [Cluster],
    groups: &mut Groups,
    text: &str,
) -> Vec<CombinedGroup> {
    let mut ranges: Vec<(Range<usize>, EastAsianLayout)> = Vec::new();
    for (range, settings) in runs {
        if settings.combine != Some(true)
            || settings.vertical == Some(true)
            || !matches!(settings.combine_brackets, None | Some(CombineBrackets::None))
        {
            continue;
        }
        match ranges.last_mut() {
            Some((last, previous)) if last.end == range.start && settings.id.is_some() && previous.id == settings.id => last.end = range.end,
            _ => ranges.push((range.clone(), *settings)),
        }
    }
    let count = clusters.len();
    let mut out = Vec::new();
    for (range, _) in ranges {
        if text
            .get(range.clone())
            .is_none_or(|text| text.chars().any(|c| matches!(c, '\t' | '\n' | '\u{000c}' | '\u{000e}' | kition_doc_doc::para::OBJ)))
        {
            continue;
        }
        let first = clusters.partition_point(|c| c.end <= range.start);
        let end = clusters.partition_point(|c| c.start < range.end);
        let Some(members) = clusters.get_mut(first..end).filter(|members| members.len() >= 2) else { continue };
        if members.iter().any(|c| !matches!(c.kind, ClKind::Text | ClKind::Space) || !c.adv.is_finite() || c.adv < 0.0)
            || (first..end).any(|k| groups.contains(k))
        {
            continue;
        }
        // Keep ligatures on one sub-row: an empty shaper cluster still belongs to its glyph.
        let midpoint = members.len().div_ceil(2);
        let Some(split) = (1..members.len()).filter(|k| members.get(*k).is_some_and(|c| c.g1 > c.g0)).min_by_key(|k| k.abs_diff(midpoint)) else {
            continue;
        };
        let upper: f32 = members.iter().take(split).map(|c| c.adv * 0.5).sum();
        let lower: f32 = members.iter().skip(split).map(|c| c.adv * 0.5).sum();
        let width = upper.max(lower);
        if !width.is_finite() || width <= 0.0 {
            continue;
        }
        let (ascent, descent) = members
            .iter()
            .filter_map(|c| styles.get(c.style as usize))
            .fold((0.0f32, 0.0f32), |(a, d), style| (a.max(style.ascent), d.max(style.descent)));
        let natural = (ascent + descent) * 0.5;
        let pitch = match paragraph.line_spacing {
            LineSpacing::Multiple(multiple) => natural * multiple,
            LineSpacing::Exactly(height) => height * 0.5,
            LineSpacing::AtLeast(height) => natural.max(height * 0.5),
        };
        let center = (ascent - descent) * 0.25;
        let mut mapped = HashMap::new();
        for (offset, c) in members.iter().enumerate() {
            let row = usize::from(offset >= split);
            if mapped.contains_key(&(c.style, row)) {
                continue;
            }
            let Some(mut style) = styles.get(c.style as usize).cloned() else { break };
            let Ok(index) = u16::try_from(styles.len()) else { break };
            style.size *= 0.5;
            style.row_shift = center + if row == 0 { pitch * 0.5 } else { -pitch * 0.5 };
            styles.push(style);
            mapped.insert((c.style, row), index);
        }
        if members.iter().enumerate().any(|(offset, c)| !mapped.contains_key(&(c.style, usize::from(offset >= split))))
            || !groups.register(first..end, width, count)
        {
            continue;
        }
        let mut positions = Vec::with_capacity(members.len());
        let mut x = (width - upper) * 0.5;
        for (offset, c) in members.iter_mut().enumerate() {
            let row = usize::from(offset >= split);
            if offset == split {
                x = (width - lower) * 0.5;
            }
            let advance = c.adv * 0.5;
            positions.push((x, advance));
            x += advance;
            c.adv = if row == 0 { advance } else { 0.0 };
            if offset + 1 == split {
                c.adv += width - upper;
            }
            c.style = mapped.get(&(c.style, row)).copied().unwrap_or(c.style);
            c.break_after = first + offset + 1 == end;
            for g in glyphs.get_mut(c.g0 as usize..c.g1 as usize).into_iter().flatten() {
                g.dx *= 0.5;
                g.dy *= 0.5;
            }
        }
        out.push(CombinedGroup { clusters: first..end, split: first + split, positions });
    }
    out
}

pub(crate) fn position(paragraph: &ParaLayout, line: &mut Line) {
    for group in groups_on_line(paragraph, line).iter().filter(|g| g.clusters.start >= line.c0 && g.clusters.end <= line.c1) {
        let Some(origin) = line.xs.get(group.clusters.start - line.c0).copied() else { continue };
        if line.vis.is_empty() {
            line.vis = (line.c0..line.c1)
                .filter_map(|k| {
                    let x = line.xs.get(k - line.c0).copied()?;
                    let right = line.xs.get(k + 1 - line.c0).copied()?;
                    Some(VisCl { x, w: right - x, level: 0 })
                })
                .collect();
        }
        for (k, (x, width)) in group.clusters.clone().zip(&group.positions) {
            if let Some(boxed) = line.vis.get_mut(k - line.c0) {
                *boxed = VisCl { x: origin + x, w: *width, level: 0 };
            }
        }
    }
}

pub(crate) fn cluster_box(paragraph: &ParaLayout, line: &Line, k: usize) -> Option<Rect> {
    group_at(paragraph, k)?;
    let c = paragraph.clusters.get(k)?;
    let style = paragraph.styles.get(c.style as usize)?;
    let top = line.baseline - style.shift - style.row_shift - style.ascent * 0.5;
    Some(Rect::new(line.cl_left(k)?, top, (line.cl_right(k)? - line.cl_left(k)?).max(0.0), (style.ascent + style.descent) * 0.5))
}

pub(crate) fn caret_band(paragraph: &ParaLayout, li: usize, offset: usize) -> Option<(f32, f32)> {
    let line = paragraph.lines.get(li)?;
    let mut k = paragraph.clusters.partition_point(|c| c.end <= offset);
    if k == paragraph.clusters.len() && offset == paragraph.text_len {
        k = k.saturating_sub(1);
    }
    let rect = cluster_box(paragraph, line, k)?;
    Some((rect.y, rect.h))
}

pub(crate) fn end_caret(paragraph: &ParaLayout, line: &Line, offset: usize) -> Option<f32> {
    let k = line.c1.checked_sub(1)?;
    let cluster = paragraph.clusters.get(k)?;
    if cluster.end != offset || group_at(paragraph, k).is_none() {
        return None;
    }
    line.vis.get(k.checked_sub(line.c0)?).map(VisCl::trailing)
}

pub(crate) fn hit(paragraph: &ParaLayout, li: usize, x: f32, y: f32) -> Option<usize> {
    let line = paragraph.lines.get(li)?;
    for group in groups_on_line(paragraph, line).iter().filter(|g| g.clusters.start >= line.c0 && g.clusters.end <= line.c1) {
        let boxes: Vec<_> = group.clusters.clone().filter_map(|k| Some((k, cluster_box(paragraph, line, k)?))).collect();
        let left = boxes.iter().map(|(_, b)| b.x).fold(f32::INFINITY, f32::min);
        let right = boxes.iter().map(|(_, b)| b.right()).fold(f32::NEG_INFINITY, f32::max);
        if x < left - 1.0 || x > right + 1.0 {
            continue;
        }
        // Choose the sub-row before finding its nearest source caret.
        let row = boxes.iter().min_by(|(_, a), (_, b)| (y - a.y - a.h * 0.5).abs().total_cmp(&(y - b.y - b.h * 0.5).abs()))?.0 >= group.split;
        let range = if row { group.split..group.clusters.end } else { group.clusters.start..group.split };
        let mut best = (paragraph.text_len, f32::INFINITY);
        for k in range {
            let c = paragraph.clusters.get(k)?;
            let rect = cluster_box(paragraph, line, k)?;
            for (off, edge) in [(c.start, rect.x), (c.end, rect.right())] {
                let distance = (edge - x).abs();
                if distance < best.1 {
                    best = (off, distance);
                }
            }
        }
        return Some(best.0);
    }
    None
}

pub(crate) fn selection(paragraph: &ParaLayout, li: usize, from: usize, to: usize) -> Option<Vec<Rect>> {
    let line = paragraph.lines.get(li)?;
    if !has_group(paragraph, line) {
        return None;
    }
    let mut out: Vec<Rect> = Vec::new();
    for k in line.c0..line.c1 {
        let c = paragraph.clusters.get(k)?;
        if c.end <= from || c.start >= to {
            continue;
        }
        let rect = cluster_box(paragraph, line, k)
            .or_else(|| Some(Rect::new(line.cl_left(k)?, line.top, line.cl_right(k)? - line.cl_left(k)?, line.height)))?;
        match out.last_mut() {
            Some(last) if (last.y - rect.y).abs() < 0.01 && (last.h - rect.h).abs() < 0.01 && (last.right() - rect.x).abs() < 0.01 => {
                last.w += rect.w
            }
            _ => out.push(rect),
        }
    }
    Some(out)
}

pub(crate) fn vertical(paragraph: &ParaLayout, li: usize, offset: usize, x: f32, down: bool) -> Option<usize> {
    let line = paragraph.lines.get(li)?;
    let mut k = paragraph.clusters.partition_point(|c| c.end <= offset);
    if k == paragraph.clusters.len() && offset == paragraph.text_len {
        k = k.saturating_sub(1);
    }
    let group = group_at(paragraph, k)?;
    let lower = k >= group.split;
    if lower == down {
        return None;
    }
    let target = if down { group.split } else { group.clusters.start };
    let rect = cluster_box(paragraph, line, target)?;
    hit(paragraph, li, x, rect.y + rect.h * 0.5)
}
