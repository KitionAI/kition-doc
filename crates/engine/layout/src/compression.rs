//! Whitespace capacity of full-width punctuation and of the automatic gaps between East Asian
//! and Latin text when character compression is enabled, and the marks that may hang past the
//! right edge at a line end (`overflowPunct`).

use std::ops::Range;

use unicode_script::{Script, UnicodeScript};

use crate::para::{ClKind, Cluster, StyleRun};

/// The share of a full-width mark's size it gives up to fit a line: half, and a line gives up no
/// more than its largest mark's half in all ([`apply`]), as WPS for Mac compresses punctuation
/// (synthetic calibration: one 10.5 pt comma absorbs 4.7 pt, four absorb 4.7 pt but not 5.7 pt,
/// two 20 pt marks absorb 10 pt but not 11 pt).
const SQUEEZE: f32 = 0.5;

pub(super) fn capacities(clusters: &[Cluster], styles: &[StyleRun], text: &str, mode: Option<&str>) -> Vec<f32> {
    if !matches!(mode, Some("compressPunctuation" | "compressPunctuationAndJapaneseKana")) {
        return Vec::new();
    }
    clusters
        .iter()
        .map(|cluster| {
            let character = text.get(cluster.start..cluster.end).and_then(|text| text.chars().next());
            if cluster.kind != ClKind::Text
                || !character.is_some_and(|c| matches!(c as u32, 0x3001 | 0x3002 | 0xff01 | 0xff0c | 0xff0e | 0xff1a | 0xff1b | 0xff1f))
            {
                return 0.0;
            }
            let size = styles.get(cluster.style as usize).map_or(0.0, |style| style.size);
            if cluster.adv.is_finite() && size.is_finite() { cluster.adv.min(size).max(0.0) * SQUEEZE } else { 0.0 }
        })
        .collect()
}

/// The share of an automatic East Asian–Latin gap it gives up to fit a line, with character
/// compression on (WPS for Mac, synthetic calibration at 10.5 pt: lines with one to three
/// quarter-em gaps take back 1.25 pt per gap; without compression they take back none).
const GAP_SQUEEZE: f32 = 0.48;

/// How far past the right edge an ideograph may reach for the gaps before it to bring it back,
/// as a share of its advance; another character comes back only if it starts inside the line
/// (same calibration: a final 10.5 pt ideograph takes back 3.2 to 4.7 pt, a final digit as much
/// as its advance).
const IDEOGRAPH_REACH: f32 = 0.35;

/// What each cluster's automatic gap before it gives up, and how much overflow the gaps may take
/// back when the cluster ends a line ([`GAP_SQUEEZE`], [`IDEOGRAPH_REACH`]). Empty without
/// character compression.
pub(super) fn gaps(clusters: &[Cluster], text: &str, spacing: &[f32], mode: Option<&str>) -> (Vec<f32>, Vec<f32>) {
    if !matches!(mode, Some("compressPunctuation" | "compressPunctuationAndJapaneseKana")) || spacing.iter().all(|gap| *gap <= 0.0) {
        return (Vec::new(), Vec::new());
    }
    let squeeze = clusters
        .iter()
        .enumerate()
        .map(|(index, _)| spacing.get(index).copied().filter(|gap| gap.is_finite()).unwrap_or(0.0).max(0.0) * GAP_SQUEEZE)
        .collect();
    let reach = clusters
        .iter()
        .map(|cluster| {
            let ideograph = text
                .get(cluster.start..cluster.end)
                .and_then(|text| text.chars().next())
                .is_some_and(|c| matches!(c.script(), Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul | Script::Bopomofo));
            let advance = if cluster.adv.is_finite() { cluster.adv.max(0.0) } else { 0.0 };
            if ideograph { advance * IDEOGRAPH_REACH } else { advance }
        })
        .collect();
    (squeeze, reach)
}

/// Take the overflow back from the line's automatic gaps (`(cluster, capacity)`, in order),
/// in proportion to their capacities; the gap before a cluster moves it and everything after.
pub(super) fn apply_gaps(gaps: &[(usize, f32)], first: usize, positions: &mut [f32], overflow: f32) -> f32 {
    if !overflow.is_finite() || overflow <= 0.0 {
        return 0.0;
    }
    let total: f32 = gaps.iter().map(|(_, capacity)| capacity).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let ratio = (overflow / total).min(1.0);
    let mut removed = 0.0;
    let mut next = gaps.iter().peekable();
    for (offset, position) in positions.iter_mut().enumerate() {
        while let Some((_, capacity)) = next.next_if(|(cluster, _)| *cluster <= first + offset) {
            removed += capacity * ratio;
        }
        *position -= removed;
    }
    removed
}

/// Remove only the overflow, distributed proportionally across punctuation whitespace, up to the
/// largest mark's capacity in all. Glyph advances stay intact; line positions carry the reduced
/// character boxes.
pub(super) fn apply(capacities: &[f32], range: Range<usize>, positions: &mut [f32], overflow: f32) -> f32 {
    if !overflow.is_finite() || overflow <= 0.0 {
        return 0.0;
    }
    let capacities = capacities.get(range).unwrap_or(&[]);
    let total: f32 = capacities.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let cap = capacities.iter().copied().fold(0.0f32, f32::max);
    let ratio = (overflow.min(cap) / total).min(1.0);
    let mut removed = 0.0;
    for (index, capacity) in capacities.iter().enumerate() {
        if let Some(position) = positions.get_mut(index) {
            *position -= removed;
        }
        removed += capacity * ratio;
    }
    if let Some(position) = positions.last_mut() {
        *position -= removed;
    }
    removed
}

/// What full-width punctuation does at a line's end: how much each cluster can give up
/// ([`capacities`]) and whether it may hang past the right edge.
pub(super) struct Marks {
    pub squeeze: Vec<f32>,
    pub hang: Vec<bool>,
    /// What the automatic gap before each cluster gives up, and how far past the edge the
    /// cluster may reach for the gaps to bring it back ([`gaps`]).
    pub gaps: Vec<f32>,
    pub reach: Vec<f32>,
}

/// Whether each cluster is a closing mark that may hang past the right edge at a line's end
/// (ideographic comma and full stop, full-width comma and full stop), when the paragraph lets
/// punctuation overflow and the mark's East Asian language is Chinese, Japanese or Korean (WPS
/// hangs nothing in text of another East Asian language, or none). Empty when the paragraph
/// doesn't let punctuation overflow.
pub(super) fn hangers(clusters: &[Cluster], styles: &[StyleRun], text: &str, overflow: bool) -> Vec<bool> {
    if !overflow {
        return Vec::new();
    }
    clusters
        .iter()
        .map(|cluster| {
            let character = text.get(cluster.start..cluster.end).and_then(|text| text.chars().next());
            let east_asian = styles.get(cluster.style as usize).and_then(|style| style.rc.lang_ea.as_deref()).is_some_and(|lang| {
                let lang = lang.get(..2).unwrap_or("");
                lang.eq_ignore_ascii_case("zh") || lang.eq_ignore_ascii_case("ja") || lang.eq_ignore_ascii_case("ko")
            });
            east_asian && cluster.kind == ClKind::Text && character.is_some_and(|c| matches!(c as u32, 0x3001 | 0x3002 | 0xff0c | 0xff0e))
        })
        .collect()
}
