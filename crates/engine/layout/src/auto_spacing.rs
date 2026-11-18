//! OOXML automatic spacing between adjoining East Asian and Latin/numeric regions.
//! It belongs to layout, not document text, and disappears at line boundaries.

use kition_doc_doc::resolve::ResolvedPara;
use unicode_script::{Script, UnicodeScript};

use crate::para::{ClKind, Cluster, StyleRun};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Region {
    EastAsian,
    Latin,
    Number,
}

fn region(c: char, east_asian_slot: bool) -> Option<Region> {
    // Fullwidth Latin letters/numbers are not ideographs (ISO 29500 autoSpaceLikeWord95).
    if matches!(c as u32, 0xff10..=0xff19 | 0xff21..=0xff3a | 0xff41..=0xff5a) {
        return None;
    }
    // Numeric symbols such as enclosed digits and Roman numerals follow their resolved
    // East Asian font slot, rather than gaining a gap as an ordinary digit run.
    if c.is_numeric() && east_asian_slot {
        return Some(Region::EastAsian);
    }
    match c.script() {
        Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul | Script::Bopomofo if c.is_alphabetic() => Some(Region::EastAsian),
        Script::Latin if c.is_alphabetic() => Some(Region::Latin),
        _ if c.is_numeric() => Some(Region::Number),
        _ => None,
    }
}

pub(crate) fn before(clusters: &[Cluster], styles: &[StyleRun], text: &str, paragraph: &ResolvedPara) -> Vec<f32> {
    if !paragraph.auto_space_de && !paragraph.auto_space_dn {
        return Vec::new();
    }
    let mut spacing = vec![0.0; clusters.len()];
    let mut previous: Option<(Region, f32)> = None;
    for (index, cluster) in clusters.iter().enumerate() {
        if cluster.kind == ClKind::Marker {
            continue;
        }
        let style = styles.get(cluster.style as usize);
        let current = (cluster.kind == ClKind::Text)
            .then(|| {
                text.get(cluster.start..cluster.end)
                    .and_then(|text| text.chars().next())
                    .and_then(|c| region(c, style.is_some_and(|style| style.rc.uses_east_asian(c))))
            })
            .flatten();
        let Some(current) = current else {
            previous = None;
            continue;
        };
        let size = style.map_or(0.0, |style| style.size);
        if let Some((previous_region, previous_size)) = previous {
            let other = match (previous_region, current) {
                (Region::EastAsian, other) | (other, Region::EastAsian) => Some(other),
                _ => None,
            };
            let enabled = match other {
                Some(Region::Latin) => paragraph.auto_space_de,
                Some(Region::Number) => paragraph.auto_space_dn,
                _ => false,
            };
            if enabled && let Some(gap) = spacing.get_mut(index) {
                // Quarter-em spacing, also observable at the corpus heading/number boundaries.
                *gap = previous_size.min(size) * 0.25;
            }
        }
        previous = Some((current, size));
    }
    spacing
}
