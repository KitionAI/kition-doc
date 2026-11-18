//! Paragraph layout: resolve runs, shape them into clusters, break lines (first-fit, as word
//! processors do), place tabs, list labels and alignment.

use std::collections::VecDeque;
use std::sync::Arc;

use kition_doc_doc::numbering::{Level, LevelSuffix};
use kition_doc_doc::para::{COLUMN_BREAK, InlineObject, LINE_BREAK, NoteKind, OBJ, PAGE_BREAK, SOFT_HYPHEN};
use kition_doc_doc::props::{Align, CharProps, LineSpacing, ParaProps, TabAlign, TabLeader, TabStop};
use kition_doc_doc::resolve::{ResolvedChar, ResolvedPara};
use kition_doc_doc::{Document, Paragraph, Run};
use kition_doc_fonts::FaceRef;
use kition_office_core::math::MathJc;

use crate::fields::{FieldCtx, field_text_with};
pub use crate::two_lines::CombinedGroup;

/// A face at a size with the formatting needed to draw it.
#[derive(Clone, Debug)]
pub struct StyleRun {
    pub face: FaceRef,
    pub size: f32,
    /// Glyph width multiplier after character scale and manual group fitting.
    pub width_scale: f32,
    pub synth_bold: bool,
    pub synth_italic: bool,
    pub rc: Arc<ResolvedChar>,
    /// Line ascent/descent at this size, points.
    pub ascent: f32,
    pub descent: f32,
    /// Baseline shift (superscript etc.), points, positive up.
    pub shift: f32,
    /// Sub-row baseline displacement for combined text, excluded from logical line metrics.
    pub row_shift: f32,
}

/// A positioned glyph inside its cluster.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub gid: u32,
    /// Offset from the cluster's x, points.
    pub dx: f32,
    pub dy: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClKind {
    Text,
    Space,
    Tab,
    LineBreak,
    PageBreak,
    ColumnBreak,
    /// Inline object with its index in `Paragraph::objects`.
    Object(usize),
    /// Zero-width marker (bookmark, comment anchor) or hidden text.
    Marker,
}

/// The smallest unit of layout: a grapheme cluster (or object, tab, break).
#[derive(Clone, Debug)]
pub struct Cluster {
    pub start: usize,
    pub end: usize,
    pub adv: f32,
    pub kind: ClKind,
    /// Index into `ParaLayout::styles`.
    pub style: u16,
    /// Glyph range in `ParaLayout::glyphs`.
    pub g0: u32,
    pub g1: u32,
    /// A line may break after this cluster.
    pub break_after: bool,
    /// Height above the baseline for objects (images), points.
    pub obj_h: f32,
    /// Depth below the baseline for objects (equations), points.
    pub obj_d: f32,
    /// The cluster is a decimal separator (decimal tabs align on it).
    pub dot: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnd {
    /// Wrapped (or last line).
    Wrap,
    LineBreak,
    PageBreak,
    ColumnBreak,
    /// End of paragraph.
    Para,
}

/// A laid-out line. Coordinates are relative to the paragraph's content box: x from the
/// column's left edge, y from the top of the first line.
#[derive(Clone, Debug)]
pub struct Line {
    pub top: f32,
    pub height: f32,
    pub baseline: f32,
    /// How far the line's text reaches below its baseline (its fonts' descent), points: the
    /// line's text ends at `baseline + descent`, which may be above its bottom.
    pub descent: f32,
    /// Cluster index range.
    pub c0: usize,
    pub c1: usize,
    /// x of each cluster in the line, plus the end x (len = c1 - c0 + 1).
    pub xs: Vec<f32>,
    /// For tab clusters: the leader to draw across them.
    pub leaders: Vec<(usize, TabLeader)>,
    pub end: LineEnd,
    /// Byte range of the paragraph covered by the line.
    pub start: usize,
    pub stop: usize,
    /// Left and right edges available to the line.
    pub left: f32,
    pub right: f32,
    /// The line ends at a hyphenation point: (style, glyph, advance) of the hyphen drawn there.
    pub hyphen: Option<(u16, u32, f32)>,
    /// The text continues the previous line's row on the far side of a floating object (same top
    /// and height): one row, two lines. Row counts (line numbers, keeping rows on one page) skip it.
    pub beside: bool,
    /// Bidirectional lines (right-to-left text, or any line of a right-to-left paragraph): each
    /// cluster's visual box, in cluster order (len = c1 - c0), after the Unicode Bidirectional
    /// Algorithm reordered the line. Empty for plain left-to-right lines, whose clusters sit at
    /// `xs`. Read cluster geometry through [`Line::cl_left`] and friends, which handle both.
    pub vis: Vec<VisCl>,
    /// The paragraph reads right to left (its start edge is on the right).
    pub rtl: bool,
}

/// Where a cluster of a bidirectional line is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisCl {
    /// Left edge and width, points (paragraph coordinates, like `Line::xs`).
    pub x: f32,
    pub w: f32,
    /// Embedding level after rule L1 of UAX #9 (odd = right to left).
    pub level: u8,
}

impl VisCl {
    pub fn is_rtl(&self) -> bool {
        self.level % 2 == 1
    }
    /// The edge a caret before this cluster sits at (right edge of right-to-left text).
    pub fn leading(&self) -> f32 {
        if self.is_rtl() { self.x + self.w } else { self.x }
    }
    /// The edge a caret after this cluster sits at.
    pub fn trailing(&self) -> f32 {
        if self.is_rtl() { self.x } else { self.x + self.w }
    }
}

impl Line {
    /// Left edge of cluster `k` (a paragraph cluster index on this line).
    pub fn cl_left(&self, k: usize) -> Option<f32> {
        let i = k.checked_sub(self.c0)?;
        if self.vis.is_empty() { self.xs.get(i).copied() } else { self.vis.get(i).map(|v| v.x) }
    }
    /// Right edge of cluster `k`.
    pub fn cl_right(&self, k: usize) -> Option<f32> {
        let i = k.checked_sub(self.c0)?;
        if self.vis.is_empty() { self.xs.get(i + 1).copied() } else { self.vis.get(i).map(|v| v.x + v.w) }
    }
    /// Is cluster `k` right-to-left text?
    pub fn cl_rtl(&self, k: usize) -> bool {
        k.checked_sub(self.c0).and_then(|i| self.vis.get(i)).is_some_and(VisCl::is_rtl)
    }
    /// Caret x after the line's last cluster (its logical end), hyphen included.
    pub fn end_x(&self) -> f32 {
        let h = self.hyphen.map_or(0.0, |h| h.2);
        if !self.rtl && !self.vis.is_empty() && self.vis.iter().all(|v| v.level == 0) {
            return self.xs.last().copied().unwrap_or(self.left);
        }
        match self.vis.last() {
            Some(v) if v.is_rtl() => v.x - h,
            Some(v) => v.x + v.w + h,
            None => self.xs.last().copied().unwrap_or(self.left),
        }
    }
    /// x of the line's visual end for the paragraph's direction, where its paragraph mark goes:
    /// the right edge of the rightmost cluster in a left-to-right paragraph, the left edge of the
    /// leftmost in a right-to-left one. Differs from [`Line::end_x`] when the logically last text
    /// runs the other way (Hebrew ending a left-to-right paragraph, Latin ending a right-to-left one).
    pub fn visual_end_x(&self) -> f32 {
        let end = self.end_x();
        if self.rtl { self.vis.iter().map(|v| v.x).fold(end, f32::min) } else { self.vis.iter().map(|v| v.x + v.w).fold(end, f32::max) }
    }
    /// x where the hyphen of a hyphenated line (advance `adv`) is drawn.
    pub fn hyphen_x(&self, adv: f32) -> f32 {
        match self.vis.last() {
            Some(v) if v.is_rtl() => v.x - adv,
            Some(v) => v.x + v.w,
            None => self.xs.last().copied().unwrap_or(0.0) - adv,
        }
    }
    /// Visual extents, left to right, of clusters `k0..k1`: one span on a left-to-right line,
    /// possibly several where bidirectional text splits a logical range.
    pub fn spans(&self, k0: usize, k1: usize) -> Vec<(f32, f32)> {
        let (k0, k1) = (k0.max(self.c0), k1.min(self.c1));
        if k0 >= k1 {
            return Vec::new();
        }
        if self.vis.is_empty() {
            return match (self.cl_left(k0), self.cl_left(k1).or_else(|| self.xs.last().copied())) {
                (Some(a), Some(b)) => vec![(a, b)],
                _ => Vec::new(),
            };
        }
        let mut boxes: Vec<(f32, f32)> =
            self.vis.get(k0 - self.c0..k1 - self.c0).unwrap_or(&[]).iter().filter(|v| v.w > 0.0).map(|v| (v.x, v.x + v.w)).collect();
        boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out: Vec<(f32, f32)> = Vec::new();
        for (a, b) in boxes {
            match out.last_mut() {
                Some(last) if a <= last.1 + 0.01 => last.1 = last.1.max(b),
                _ => out.push((a, b)),
            }
        }
        out
    }
}

/// The list label drawn on the first line.
#[derive(Clone, Debug)]
pub struct Label {
    pub text: String,
    pub style: u16,
    pub glyphs: Vec<Glyph>,
    pub x: f32,
    pub width: f32,
}

/// A laid-out paragraph.
#[derive(Clone, Debug)]
pub struct ParaLayout {
    pub rp: ResolvedPara,
    pub styles: Vec<StyleRun>,
    pub glyphs: Vec<Glyph>,
    pub clusters: Vec<Cluster>,
    pub lines: Vec<Line>,
    pub label: Option<Label>,
    /// Sum of line heights (without space before/after).
    pub height: f32,
    pub text_len: usize,
    /// Paragraph contains fields whose text depends on the page.
    pub has_page_fields: bool,
    /// Note references (part id) in this paragraph, by cluster index.
    pub notes: Vec<(usize, u32)>,
    /// Proofing issues: (byte start, byte end, grammar?).
    pub issues: Vec<(usize, usize, bool)>,
    /// Drop cap: (clusters at the start that form it, lines it drops, width with its gap).
    pub drop_cap: Option<(usize, u8, f32)>,
    /// Clusters after which the line may break with a hyphen (soft hyphens, auto hyphenation), sorted.
    pub hyph_after: Vec<u32>,
    /// Spaces made half an em by balanced single-byte and double-byte widths, by cluster index,
    /// sorted: they never shrink to fit a justified line (WPS for Mac).
    pub firm_spaces: Vec<u32>,
    /// Bidi embedding level of each byte of the paragraph text (UAX #9, before the per-line
    /// rule L1). Empty when the paragraph is plain left-to-right text.
    pub bidi_levels: Vec<u8>,
    /// Width of the drop cap letter itself (without the gap to the text).
    pub drop_cap_w: f32,
    /// Laid-out equations by object index.
    pub maths: Vec<(usize, Arc<crate::math::MathLayout>)>,
    /// Uncached `EQ` fields drawn from their field codes, by object index.
    pub eq_fields: Vec<(usize, Arc<crate::math::MathLayout>)>,
    /// Display equations (on lines of their own): cluster index and justification.
    pub displays: Vec<(usize, kition_office_core::math::MathJc)>,
    /// Byte ranges of the text laid out as nothing ([`left_out`]): hidden text that is not shown,
    /// tracked deletions in the final text. Resolved as laid out, table formatting included.
    pub left_out: Vec<std::ops::Range<usize>>,
    /// The text drawn by clusters that stand for an object (field results, note numbers,
    /// equations) rather than for the paragraph's own text, by cluster index, sorted, with each of
    /// the cluster's glyphs' byte range in that text (a ligature's glyph spans its letters).
    pub shown: Vec<(usize, String, Vec<std::ops::Range<usize>>)>,
    /// Two editable sub-rows per logical group, sorted and disjoint by cluster range.
    /// Runs set across the line in vertical text (`w:eastAsianLayout w:vert`), by cluster
    /// range: the first cluster takes one em along the line, the rest none (see
    /// `display::lines`).
    pub across: Vec<std::ops::Range<usize>>,
    pub combined: Vec<CombinedGroup>,
    /// Content-control marks: the byte offset of the character beside each, and the ascent and
    /// descent it gives its line ([`control_marks`]).
    pub control_marks: Vec<(usize, f32, f32)>,
}

/// What a table style gives the text of a cell: its paragraph and run formatting, with the
/// cell's conditional formatting (header row, first column…) applied. It sits between document
/// defaults and the paragraph's style (`StyleSheet::resolve_para_in`, `resolve_char_in`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellText {
    pub para: ParaProps,
    pub chr: CharProps,
}

/// Inputs that change a paragraph's layout beyond its own content.
pub struct ParaEnv<'a> {
    pub doc: &'a Document,
    /// Column width, points.
    pub width: f32,
    pub label: Option<(String, Level)>,
    pub fields: &'a FieldCtx,
    pub show_hidden: bool,
    /// Leave tracked deletions out, like hidden text (the final, "No Markup" text).
    pub hide_deleted: bool,
    /// The table style's formatting for text in this paragraph's cell (`None` outside tables).
    pub table: Option<&'a CellText>,
    pub proofing: bool,
    /// Areas text must flow around (floating objects), relative to the paragraph: x from the
    /// column's left edge, y from the top of the first line.
    pub exclusions: &'a [Exclusion],
    /// Automatic equation numbers used before this paragraph.
    pub eq_number: u32,
    /// Section line grid in points (absent in web view and in ordinary table cells).
    pub grid_pitch: Option<f32>,
    /// The section's character grid pitch, points: East Asian characters take whole cells.
    pub char_pitch: Option<f32>,
    /// The text runs top to bottom (a turned section): runs set across the line take one em.
    pub vertical: bool,
    /// The lines from this one on run this wide: a paragraph continuing in a column of another
    /// width.
    pub widen: Option<(usize, f32)>,
}

/// An area text wraps around.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exclusion {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    pub clear: Clear,
}

/// How text keeps clear of a wrap area.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Clear {
    /// Beside it on either side (square wrapping): every line it meets.
    #[default]
    Beside,
    /// Only above and below it (top and bottom wrapping).
    Below,
    /// Beside it where it meets a line's text, not just the line (a slice of a tight or through
    /// wrap's polygon), as WPS for Mac wraps text around an outline.
    Outline,
}

struct Builder<'a> {
    env: &'a ParaEnv<'a>,
    styles: Vec<StyleRun>,
    /// Hash of a style's format → (index in `styles`, small caps), see `style`.
    style_index: std::collections::HashMap<u64, Vec<(u16, bool)>>,
    glyphs: Vec<Glyph>,
    clusters: Vec<Cluster>,
    /// Bidi level per byte of the paragraph text (empty: all left to right).
    levels: Vec<u8>,
    /// Text drawn by clusters that stand for an object, by cluster index (see `ParaLayout::shown`).
    shown: Vec<(usize, String, Vec<std::ops::Range<usize>>)>,
    /// Paragraph offsets of the spaces beside Chinese, Japanese or Korean characters, sorted, when
    /// the document balances single-byte and double-byte widths: WPS for Mac makes them half an
    /// em wide (their font's ascent and descent still count).
    ea_spaces: Vec<usize>,
}

/// What a piece of text is shaped as: one face, case, script formatting and direction.
#[derive(Clone, Copy)]
struct SegKey {
    face: Option<FaceRef>,
    small: bool,
    /// Formatted with the complex-script properties (Persian, Arabic, Hebrew…).
    complex: bool,
    east_asian: bool,
    /// Odd bidi level: shaped right to left.
    rtl: bool,
}

impl SegKey {
    fn same(&self, o: &SegKey) -> bool {
        self.face.map(|f| f.id()) == o.face.map(|f| f.id())
            && self.small == o.small
            && self.complex == o.complex
            && self.east_asian == o.east_asian
            && self.rtl == o.rtl
    }
}

/// Combining marks and joiners (bidi classes NSM, BN: harakat, ZWNJ, ZWJ) belong to the
/// character before them: same font, same shaping run, so Arabic joining isn't broken.
fn is_mark(c: char) -> bool {
    use unicode_bidi::BidiClass::{BN, NSM};
    matches!(unicode_bidi::bidi_class(c), NSM | BN) && !c.is_control()
}

/// Bidi embedding levels per byte of `text` (UAX #9 rules up to I2), or empty when the
/// paragraph is left to right and has no right-to-left characters.
pub fn bidi_levels(text: &str, rtl_para: bool) -> Vec<u8> {
    if !rtl_para && !text.chars().any(kition_doc_fonts::is_rtl) {
        return Vec::new();
    }
    let level = if rtl_para { unicode_bidi::Level::rtl() } else { unicode_bidi::Level::ltr() };
    let info = unicode_bidi::ParagraphBidiInfo::new(text, Some(level));
    info.levels.iter().map(|l| l.number()).collect()
}

/// Rule L2 of UAX #9: the visual order (left to right) of items with these levels: from the
/// highest level down to the lowest odd one, reverse every run at that level or above.
pub fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let Some(min_odd) = levels.iter().copied().filter(|l| l % 2 == 1).min() else { return order };
    let max = levels.iter().copied().max().unwrap_or(0);
    let level_at = |order: &[usize], i: usize| order.get(i).and_then(|&o| levels.get(o)).copied().unwrap_or(0);
    let mut lvl = max;
    while lvl >= min_odd {
        let mut i = 0;
        while i < order.len() {
            if level_at(&order, i) >= lvl {
                let mut j = i;
                while j < order.len() && level_at(&order, j) >= lvl {
                    j += 1;
                }
                if let Some(run) = order.get_mut(i..j) {
                    run.reverse();
                }
                i = j;
            } else {
                i += 1;
            }
        }
        if lvl == 0 {
            break;
        }
        lvl -= 1;
    }
    order
}

impl<'a> Builder<'a> {
    fn requested_font(&self, family: &str, bold: bool, italic: bool) -> kition_doc_fonts::word::Resolved {
        kition_doc_fonts::word::resolve_with_charset(family, self.env.doc.settings.font_charset(family), bold, italic)
    }

    fn east_asian_font(&self, rc: &ResolvedChar, sample: char, bold: bool, italic: bool) -> kition_doc_fonts::word::Resolved {
        let language = self.env.doc.settings.font_charset(&rc.font_ea).and_then(kition_doc_fonts::word::charset_language).or(rc.lang_ea.as_deref());
        kition_doc_fonts::word::resolve_east_asian(&rc.font_ea, language, sample, bold, italic)
    }

    /// Style index for a resolved char in `face` (caps-scaled `size` override for small caps).
    fn style(&mut self, rc: &Arc<ResolvedChar>, face_override: Option<FaceRef>, small: bool) -> u16 {
        let r = self.requested_font(&rc.font, rc.bold, rc.italic);
        let face = face_override.unwrap_or(r.face);
        // The display list reads every run's format (link target, strike kind, underline colour,
        // baseline shift…) from its StyleRun, so runs share one only when their formats are equal.
        // Keyed by a hash of the commonly varying fields (no allocation: this runs for every shaped
        // piece), then confirmed by comparing whole formats within the bucket.
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (face.id(), small, &rc.font, rc.size.to_bits(), rc.bold, rc.italic, rc.color, rc.highlight, rc.shading).hash(&mut h);
            (rc.underline_color, rc.strike, rc.double_strike, &rc.link, rc.ins, rc.del, rc.hidden).hash(&mut h);
            h.finish()
        };
        let bucket = self.style_index.entry(key).or_default();
        // Formats that never compare equal (a NaN from a hostile file) get their own styles; a capped
        // scan keeps such a bucket from turning quadratic.
        for &(i, s) in bucket.iter().rev().take(64) {
            if s == small
                && let Some(st) = self.styles.get(i as usize)
                && st.face.id() == face.id()
                && (Arc::ptr_eq(&st.rc, rc) || *st.rc == **rc)
            {
                return i;
            }
        }
        let size = rc.draw_size() * if small { 0.8 } else { 1.0 };
        // A missing family WPS draws in a face of its own takes the line height measured there,
        // whatever face stands in for it here.
        let stands_for = if face_override.is_some_and(|f| f.id() != r.face.id()) { rc.font_ea.as_str() } else { rc.font.as_str() };
        let measured = kition_doc_fonts::word::measured_line_metrics(stands_for)
            .filter(|_| kition_doc_fonts::word::resolve(stands_for, rc.bold, rc.italic).substituted)
            .map(|(a, d)| (a * face.upem, d * face.upem));
        let (a, d) = measured.unwrap_or_else(|| kition_doc_fonts::word::line_metrics(&face));
        let k = size as f64 / face.upem.max(1.0);
        // A fallback face (letters the requested font lacks) is emboldened or slanted when it has
        // no bold or italic of its own.
        let (synth_bold, synth_italic) = match face_override {
            None => (r.synth_bold, r.synth_italic),
            Some(f) => (rc.bold && f.get().weight < 600.0, rc.italic && !f.get().italic),
        };
        let st = StyleRun {
            face,
            size,
            width_scale: rc.scale / 100.0,
            synth_bold,
            synth_italic,
            rc: rc.clone(),
            ascent: (a * k) as f32,
            descent: (d * k) as f32,
            shift: rc.baseline_shift(),
            row_shift: 0.0,
        };
        let i = self.styles.len().min(u16::MAX as usize) as u16;
        self.styles.push(st);
        self.style_index.entry(key).or_default().push((i, small));
        i
    }

    /// Shape `text` (a piece of the paragraph starting at byte `base`) in one style and append clusters.
    fn shape(&mut self, text: &str, base: usize, rc: &Arc<ResolvedChar>, kind_override: Option<ClKind>) {
        self.shape_with(text, base, rc, kind_override, true);
    }

    /// `para_text`: `text` is paragraph text at `base`, so the paragraph's bidi levels apply
    /// (false for field results and list labels, which are shaped on their own).
    fn shape_with(&mut self, text: &str, base: usize, rc: &Arc<ResolvedChar>, kind_override: Option<ClKind>, para_text: bool) {
        if text.is_empty() {
            return;
        }
        // Complex-script characters (Persian, Arabic…) use the run's complex-script font, size,
        // bold and italic.
        let rc_cs: Option<Arc<ResolvedChar>> = text.chars().any(|c| rc.uses_complex(c)).then(|| Arc::new(rc.complex()));
        let ea_sample = text.chars().find(|c| rc.uses_east_asian(*c));
        let rc_ea = ea_sample.map(|_| Arc::new(rc.east_asian()));
        let primary_ea = ea_sample.map(|c| {
            let requested = kition_doc_fonts::word::resolve(&rc.font_ea, rc.bold, rc.italic);
            if requested.substituted { self.east_asian_font(rc, c, rc.bold, rc.italic).face } else { requested.face }
        });
        let primary = self.requested_font(&rc.font, rc.bold, rc.italic).face;
        let primary_cs = rc_cs.as_ref().map(|r| self.requested_font(&r.font, r.bold, r.italic).face);
        let use_levels = para_text && !self.levels.is_empty();
        // Split by font coverage (fallback faces), small caps case, script formatting and direction.
        let mut seg_start = 0;
        let mut cur: Option<SegKey> = None;
        let mut segs: Vec<(usize, usize, SegKey)> = Vec::new();
        for (i, c) in text.char_indices() {
            let mark = is_mark(c);
            let complex = match cur {
                Some(k) if mark => k.complex,
                _ => rc_cs.is_some() && rc.uses_complex(c),
            };
            let east_asian = !complex && if mark { cur.map_or_else(|| rc.uses_east_asian(c), |k| k.east_asian) } else { rc.uses_east_asian(c) };
            let prim = if complex {
                primary_cs.unwrap_or(primary)
            } else if east_asian {
                primary_ea.unwrap_or(primary)
            } else {
                primary
            };
            let face = match cur {
                Some(k) if mark => k.face,
                _ if prim.covers(c) || c.is_whitespace() || c.is_control() || c == SOFT_HYPHEN => east_asian.then_some(prim),
                // Stay in the fallback face the word started in while it covers the letters.
                Some(SegKey { face: Some(f), .. }) if f.covers(c) => Some(f),
                _ => {
                    let (bold, italic) = if complex { (rc.bold_cs, rc.italic_cs) } else { (rc.bold, rc.italic) };
                    let regional = east_asian.then(|| self.east_asian_font(rc, c, bold, italic).face).filter(|face| face.covers(c));
                    let symbol = || {
                        (!complex && !kition_doc_fonts::is_rtl(c))
                            .then(|| kition_doc_fonts::word::symbol_fallback(c, bold, italic))
                            .flatten()
                            .map(|r| r.face)
                    };
                    regional
                        .or_else(symbol)
                        .or_else(|| kition_doc_fonts::FontDb::global().fallback_styled(c, prim.id(), bold, italic).map(|f| FaceRef::of(&f)))
                }
            };
            let small = rc.small_caps && !rc.caps && c.is_lowercase();
            let rtl = use_levels && self.levels.get(base + i).is_some_and(|l| l % 2 == 1);
            let k = SegKey { face, small, complex, east_asian, rtl };
            match cur {
                Some(p) if p.same(&k) => {}
                Some(p) => {
                    segs.push((seg_start, i, p));
                    seg_start = i;
                    cur = Some(k);
                }
                None => cur = Some(k),
            }
        }
        if let Some(p) = cur {
            segs.push((seg_start, text.len(), p));
        }
        for (a, b, key) in segs {
            let Some(sub) = text.get(a..b) else { continue };
            let rc = if key.complex {
                rc_cs.as_ref().unwrap_or(rc).clone()
            } else if key.east_asian {
                rc_ea.as_ref().unwrap_or(rc).clone()
            } else {
                rc.clone()
            };
            let rc = &rc;
            let small = key.small;
            let si = self.style(rc, key.face, small);
            let Some(st) = self.styles.get(si as usize).cloned() else { continue };
            let upper = rc.caps || small;
            let map = |c: char| if upper { c.to_uppercase().next().unwrap_or(c) } else { c };
            let shaped = if use_levels {
                kition_doc_fonts::shape_run(&st.face, sub, &[], map, key.rtl)
            } else {
                kition_doc_fonts::shape(&st.face, sub, &[], map)
            };
            let k = st.size / st.face.upem.max(1.0) as f32;
            let hscale = rc.scale / 100.0;
            // Group glyphs by cluster byte offset; graphemes may span several shaper clusters.
            let bounds: Vec<usize> = unicode_segmentation::UnicodeSegmentation::grapheme_indices(sub, true).map(|(i, _)| i).collect();
            let mut gi = 0usize;
            for (bi, &gs) in bounds.iter().enumerate() {
                let ge = bounds.get(bi + 1).copied().unwrap_or(sub.len());
                let g0 = self.glyphs.len() as u32;
                let mut adv = 0.0f32;
                while let Some(g) = shaped.get(gi) {
                    if g.cluster >= ge {
                        break;
                    }
                    self.glyphs.push(Glyph { gid: g.gid, dx: adv + g.x_offset as f32 * k * hscale, dy: g.y_offset as f32 * k });
                    adv += g.x_advance as f32 * k * hscale;
                    gi += 1;
                }
                let g1 = self.glyphs.len() as u32;
                let s = sub.get(gs..ge).unwrap_or("");
                let ch = s.chars().next().unwrap_or(' ');
                let kind = kind_override.unwrap_or(if ch == ' ' || ch == '\u{3000}' {
                    ClKind::Space
                } else if ch == SOFT_HYPHEN {
                    ClKind::Marker
                } else {
                    ClKind::Text
                });
                let adv = if kind == ClKind::Marker { 0.0 } else { adv + rc.spacing };
                self.clusters.push(Cluster {
                    start: base + a + gs,
                    end: base + a + ge,
                    adv,
                    kind,
                    style: si,
                    g0,
                    g1: if kind == ClKind::Marker { g0 } else { g1 },
                    break_after: false,
                    obj_h: 0.0,
                    obj_d: 0.0,
                    dot: s == "." || s == ",",
                });
            }
        }
    }

    /// Field tabs use the paragraph's stops, including a tab before another field.
    /// Every displayed piece still belongs to the same document object range.
    fn shape_field(&mut self, text: &str, start: usize, end: usize, rc: &Arc<ResolvedChar>) {
        if !text.contains('\t') {
            self.shape_atomic(text, start, end, rc);
            return;
        }
        let mut from = 0;
        for (index, ch) in text.char_indices().filter(|(_, ch)| *ch == '\t') {
            if let Some(piece) = text.get(from..index).filter(|piece| !piece.is_empty()) {
                self.shape_atomic(piece, start, end, rc);
            }
            let style = self.style(rc, None, false);
            let g = self.glyphs.len() as u32;
            self.clusters.push(Cluster {
                start,
                end,
                adv: 0.0,
                kind: ClKind::Tab,
                style,
                g0: g,
                g1: g,
                break_after: true,
                obj_h: 0.0,
                obj_d: 0.0,
                dot: false,
            });
            from = index + ch.len_utf8();
        }
        if let Some(piece) = text.get(from..).filter(|piece| !piece.is_empty()) {
            self.shape_atomic(piece, start, end, rc);
        }
    }

    /// One editing range for an object, with a separate cluster per shaped font run.
    fn shape_atomic(&mut self, text: &str, start: usize, end: usize, rc: &Arc<ResolvedChar>) {
        let before = self.clusters.len();
        self.shape_with(text, start, rc, None, false);
        let added: Vec<Cluster> = self.clusters.drain(before..).collect();
        if added.is_empty() {
            let si = self.style(rc, None, false);
            let g = self.glyphs.len() as u32;
            self.clusters.push(Cluster {
                start,
                end,
                adv: 0.0,
                kind: ClKind::Marker,
                style: si,
                g0: g,
                g1: g,
                break_after: false,
                obj_h: 0.0,
                obj_d: 0.0,
                dot: false,
            });
            return;
        }
        // Never draw a fallback face's glyph ids through the first face of a field.
        // Within each font run, preserve the glyph-to-text ranges of ligatures.
        let mut first = 0;
        while let Some(head) = added.get(first) {
            let count = if head.dot { 1 } else { added.iter().skip(first).take_while(|c| c.style == head.style && !c.dot).count() };
            let group = added.get(first..first + count).unwrap_or(&[]);
            let text_start = head.start.saturating_sub(start);
            let text_end = group.last().map_or(text_start, |c| c.end.saturating_sub(start));
            let mut x = 0.0;
            let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
            for c in group {
                for g in self.glyphs.get_mut(c.g0 as usize..c.g1 as usize).into_iter().flatten() {
                    g.dx += x;
                }
                x += c.adv;
                let origin = start.saturating_add(text_start);
                let (a, b) = (c.start.saturating_sub(origin), c.end.saturating_sub(origin));
                if c.g1 <= c.g0 {
                    let from = ranges.last().map(|r| r.start);
                    for r in ranges.iter_mut().rev().take_while(|r| Some(r.start) == from) {
                        r.end = b;
                    }
                }
                ranges.extend((c.g0..c.g1).map(|_| a..b));
            }
            let shown = text.get(text_start..text_end).unwrap_or("").to_string();
            self.shown.push((self.clusters.len(), shown, ranges));
            self.clusters.push(Cluster {
                start,
                end,
                adv: x,
                kind: ClKind::Text,
                style: head.style,
                g0: head.g0,
                g1: group.last().map_or(head.g0, |c| c.g1),
                break_after: false,
                obj_h: 0.0,
                obj_d: 0.0,
                dot: head.dot,
            });
            first += count;
        }
    }
}

/// Shapes an `EQ` field's text through the paragraph's builder, each character in its code
/// run's formatting (the field's own where the code has no runs).
struct EqShaper<'s, 'a> {
    b: &'s mut Builder<'a>,
    runs: Option<&'s [Run]>,
    field: Arc<ResolvedChar>,
    /// Each code run's resolved formatting, once needed.
    resolved: Vec<Option<Arc<ResolvedChar>>>,
    resolve: &'s dyn Fn(&CharProps) -> Arc<ResolvedChar>,
    /// A Phonetic Guide's formatting for its guide text, and whether that text is being laid out.
    guide: &'s crate::eq_field::Guide,
    in_guide: bool,
}

impl EqShaper<'_, '_> {
    /// The code run holding byte `at` of the code.
    fn run_of(&self, at: usize) -> Option<usize> {
        let mut end = 0usize;
        self.runs?.iter().position(|r| {
            end = end.saturating_add(r.len);
            at < end
        })
    }

    fn rc_of(&mut self, run: Option<usize>) -> Arc<ResolvedChar> {
        let Some(i) = run else { return self.field.clone() };
        if let Some(Some(rc)) = self.resolved.get(i) {
            return rc.clone();
        }
        let Some(r) = self.runs.and_then(|runs| runs.get(i)) else { return self.field.clone() };
        let rc = (self.resolve)(&r.props);
        if let Some(slot) = self.resolved.get_mut(i) {
            *slot = Some(rc.clone());
        }
        rc
    }

    /// `text` shaped in `rc`: one glyph run per style, as a field box.
    fn shape(&mut self, text: &str, rc: &Arc<ResolvedChar>) -> crate::eq_field::EqBox {
        use crate::math::MItem;
        let b = &mut *self.b;
        let (c0, g0) = (b.clusters.len(), b.glyphs.len());
        b.shape_with(text, 0, rc, None, false);
        let clusters: Vec<Cluster> = b.clusters.drain(c0..).collect();
        let glyphs = b.glyphs.split_off(g0);
        let mut out = crate::eq_field::EqBox::default();
        let mut x = 0.0f32;
        let mut i = 0;
        while let Some(head) = clusters.get(i) {
            let group: Vec<&Cluster> = clusters.iter().skip(i).take_while(|c| c.style == head.style).collect();
            i += group.len();
            let Some(st) = b.styles.get(head.style as usize) else { continue };
            let mut placed = Vec::new();
            for c in &group {
                let (a, z) = ((c.g0 as usize).saturating_sub(g0), (c.g1 as usize).saturating_sub(g0));
                for g in glyphs.get(a..z).into_iter().flatten() {
                    placed.push((g.gid, x + g.dx, st.shift + g.dy));
                }
                x += c.adv;
            }
            let end = group.last().map_or(head.start, |c| c.end);
            let color = match st.rc.color {
                kition_doc_doc::props::TextColor::Rgb(c) => c,
                kition_doc_doc::props::TextColor::Auto => kition_doc_doc::props::Rgb::BLACK,
            };
            out.a = out.a.max(st.ascent + st.shift.max(0.0));
            out.d = out.d.max(st.descent - st.shift.min(0.0));
            if !placed.is_empty() {
                out.items.push(MItem::Glyphs {
                    face: st.face,
                    size: st.size,
                    color: color.into(),
                    synth_bold: st.synth_bold,
                    synth_italic: st.synth_italic,
                    glyphs: placed,
                    text: text.get(head.start..end).unwrap_or("").to_string(),
                });
            }
        }
        out.w = x;
        out
    }
}

impl crate::eq_field::EqText for EqShaper<'_, '_> {
    fn text(&mut self, chars: &[(usize, char)]) -> crate::eq_field::EqBox {
        let mut parts = Vec::new();
        let mut i = 0;
        while let Some(&(at, _)) = chars.get(i) {
            let run = self.run_of(at);
            let piece: String = chars.iter().skip(i).take_while(|(o, _)| self.run_of(*o) == run).map(|(_, c)| *c).collect();
            i += piece.chars().count();
            let mut rc = self.rc_of(run);
            if self.in_guide && (self.guide.size.is_some() || self.guide.font.is_some()) {
                let mut g = (*rc).clone();
                if let Some(size) = self.guide.size {
                    g.size = size;
                }
                if let Some(font) = &self.guide.font {
                    g.font.clone_from(font);
                    g.font_ea.clone_from(font);
                    g.font_cs.clone_from(font);
                }
                rc = Arc::new(g);
            }
            parts.push(self.shape(&piece, &rc));
        }
        let mut out = crate::eq_field::EqBox::default();
        for p in parts {
            out.join(p);
        }
        out
    }

    fn glyph(&mut self, ch: char, at: usize, size: Option<f32>) -> crate::eq_field::EqBox {
        let run = self.run_of(at);
        let mut rc = self.rc_of(run);
        if let Some(s) = size.filter(|s| s.is_finite() && *s > 0.0) {
            let mut sized = (*rc).clone();
            sized.size = s.min(1584.0);
            rc = Arc::new(sized);
        }
        self.shape(&ch.to_string(), &rc)
    }

    fn guide(&mut self, on: bool) -> bool {
        std::mem::replace(&mut self.in_guide, on)
    }

    fn metrics(&mut self, at: usize) -> (f32, kition_doc_doc::props::Rgb) {
        let run = self.run_of(at);
        let rc = self.rc_of(run);
        let color = match rc.color {
            kition_doc_doc::props::TextColor::Rgb(c) => c,
            kition_doc_doc::props::TextColor::Auto => kition_doc_doc::props::Rgb::BLACK,
        };
        (rc.draw_size(), color)
    }
}

/// Whether text with this formatting is left out of the layout: hidden text unless it is shown, and
/// tracked deletions in the final text. It takes no room, draws nothing and is no place to break.
pub(crate) fn left_out(rc: &ResolvedChar, show_hidden: bool, hide_deleted: bool) -> bool {
    (rc.hidden && !show_hidden) || (rc.del.is_some() && hide_deleted)
}

/// Lay out one paragraph.
pub fn layout_para(p: &Paragraph, env: &ParaEnv) -> ParaLayout {
    let doc = env.doc;
    let mut rp = doc.styles.resolve_para_in(&p.props, env.table.map(|t| &t.para));
    // List level indents apply unless the paragraph sets its own.
    if let Some((_, lvl)) = &env.label {
        if p.props.indent_left.is_none() {
            rp.indent_left = lvl.indent;
        }
        if p.props.indent_first.is_none() {
            rp.indent_first = -lvl.hanging;
        }
    }
    let para_style = p.props.style.as_deref();
    let table_chr = env.table.map(|t| &t.chr);
    let resolve = |c: &CharProps| -> Arc<ResolvedChar> { Arc::new(doc.styles.resolve_char_in(para_style, table_chr, c)) };
    let levels = bidi_levels(&p.text, rp.bidi);
    let mut b = Builder {
        env,
        styles: Vec::new(),
        style_index: Default::default(),
        glyphs: Vec::new(),
        clusters: Vec::new(),
        levels,
        shown: Vec::new(),
        ea_spaces: if env.doc.settings.compat_flags.iter().any(|f| f == "balanceSingleByteDoubleByteWidth") {
            ea_spaces(&p.text)
        } else {
            Vec::new()
        },
    };
    let mark_rc = resolve(&p.mark);
    let mark_style = b.style(&mark_rc, None, false);
    let mut has_page_fields = false;
    let mut notes = Vec::new();
    let mut obj_index = 0usize;
    // Drop cap: the first character, sized so its capital spans `lines` lines.
    let drop_first = (rp.drop_cap > 0)
        .then(|| p.text.chars().next())
        .flatten()
        .filter(|c| !matches!(*c, '\t' | LINE_BREAK | PAGE_BREAK | COLUMN_BREAK | OBJ | '\r' | ' '))
        .filter(|c| p.text.len() > c.len_utf8());
    let mut drop_cap = None;
    let mut drop_cap_w = 0.0;
    let mut maths = Vec::new();
    let mut eq_fields = Vec::new();
    let mut displays = Vec::new();
    let mut eq_counter = env.eq_number;
    let math_props = doc.settings.math.clone().unwrap_or_default();
    // The byte ranges of runs left out of the layout, in order, adjacent runs merged.
    let mut left: Vec<std::ops::Range<usize>> = Vec::new();
    let mut fit_runs = Vec::new();
    let mut combined_runs = Vec::new();
    for (range, props) in p.run_ranges() {
        let rc = resolve(props);
        if let Some(fit) = rc.fit_text {
            fit_runs.push((range.clone(), fit));
        }
        if let Some(settings) = rc.east_asian_layout {
            combined_runs.push((range.clone(), settings));
        }
        let Some(text) = p.text.get(range.clone()) else { continue };
        if left_out(&rc, b.env.show_hidden, b.env.hide_deleted) {
            match left.last_mut() {
                Some(last) if last.end == range.start => last.end = range.end,
                _ => left.push(range.clone()),
            }
            let si = b.style(&rc, None, false);
            let g = b.glyphs.len() as u32;
            // Word still prints the note of a hidden reference mark (and numbers it); a deleted
            // reference is gone from the final text, note and all.
            let keeps_notes = !(rc.del.is_some() && b.env.hide_deleted);
            for (i, c) in text.char_indices() {
                let mut note = None;
                if c == OBJ {
                    if let Some(InlineObject::NoteRef { id, .. }) = p.objects.get(obj_index)
                        && keeps_notes
                    {
                        note = Some(*id);
                    }
                    obj_index += 1;
                }
                if let Some(id) = note {
                    notes.push((b.clusters.len(), id));
                }
                b.clusters.push(Cluster {
                    start: range.start + i,
                    end: range.start + i + c.len_utf8(),
                    adv: 0.0,
                    kind: ClKind::Marker,
                    style: si,
                    g0: g,
                    g1: g,
                    break_after: false,
                    obj_h: 0.0,
                    obj_d: 0.0,
                    dot: false,
                });
            }
            continue;
        }
        let mut seg = 0;
        if range.start == 0
            && let Some(c) = drop_first
            && let Some(first) = text.get(..c.len_utf8())
        {
            // The lines it spans are those of the text after it, spaced as the paragraph spaces
            // them (multiple, exact or at least, on the line grid), each a single line at least.
            let text_size = resolve(p.props_of_char(first.len())).size;
            let single = (text_size * 1.2).max(1.0);
            let spaced = line_spacing_heights(single, &rp, env.grid_pitch).0;
            let line_h = grid_line_height(spaced, &rp, env.grid_pitch).max(grid_multiple_floor(&rp, env.grid_pitch)).max(1.0);
            let lines = rp.drop_cap as f32;
            let mut big = (*rc).clone();
            // A document's drop cap carries the size Word gave it; otherwise the capital, about
            // 0.7 em tall, spans from line 1's caps to line N's baseline. (`clamp` would panic here
            // when the text itself is larger than 1000 pt.)
            big.size = if rc.size > text_size { rc.size } else { (((lines - 1.0) * line_h + text_size * 0.7) / 0.7).max(rc.size) }.min(1000.0);
            let before = b.clusters.len();
            b.shape(first, 0, &Arc::new(big), None);
            let w: f32 = b.clusters.get(before..).map(|c| c.iter().map(|c| c.adv).sum()).unwrap_or(0.0);
            let gap = rp.drop_cap_space.unwrap_or(text_size * 0.3);
            drop_cap = Some((b.clusters.len() - before, rp.drop_cap, w + gap));
            drop_cap_w = w;
            // The letter hangs beside the text; lines flow at the indent `break_lines` adds.
            for c in b.clusters.iter_mut().skip(before) {
                c.adv = 0.0;
            }
            seg = c.len_utf8();
        }
        for (i, c) in text.char_indices() {
            if i < seg {
                continue;
            }
            let special = matches!(c, '\t' | LINE_BREAK | PAGE_BREAK | COLUMN_BREAK | OBJ | '\r');
            if !special {
                continue;
            }
            if let Some(s) = text.get(seg..i) {
                b.shape(s, range.start + seg, &rc, None);
            }
            seg = i + c.len_utf8();
            let (start, end) = (range.start + i, range.start + i + c.len_utf8());
            let si = b.style(&rc, None, false);
            let g = b.glyphs.len() as u32;
            let push = |b: &mut Builder, kind: ClKind, adv: f32, h: f32| {
                b.clusters.push(Cluster { start, end, adv, kind, style: si, g0: g, g1: g, break_after: false, obj_h: h, obj_d: 0.0, dot: false })
            };
            match c {
                '\t' => push(&mut b, ClKind::Tab, 0.0, 0.0),
                LINE_BREAK | '\r' => push(&mut b, ClKind::LineBreak, 0.0, 0.0),
                PAGE_BREAK => push(&mut b, ClKind::PageBreak, 0.0, 0.0),
                COLUMN_BREAK => push(&mut b, ClKind::ColumnBreak, 0.0, 0.0),
                _ => {
                    let k = obj_index;
                    obj_index += 1;
                    match p.objects.get(k) {
                        Some(o) if o.is_drawing() => {
                            if let Some((w, h, float)) = o.frame()
                                && float.wrap == kition_doc_doc::para::Wrap::Inline
                            {
                                let maxw = (env.width - rp.indent_left.max(0.0) - rp.indent_right.max(0.0)).max(18.0);
                                let (w, h) = (w.clamp(1.0, 4000.0), h.clamp(1.0, 4000.0));
                                let s = if w > maxw { maxw / w } else { 1.0 };
                                // The line makes room for effects (shadows) around the picture too, and
                                // for a rotated one's bounds, as Word does.
                                let [el, et, er, eb] = float.effect_extent();
                                let (px, py) = float.spin_pad(w * s, h * s);
                                push(&mut b, ClKind::Object(k), w * s + el + er + 2.0 * px, h * s + et + eb + 2.0 * py);
                            } else {
                                push(&mut b, ClKind::Object(k), 0.0, 0.0);
                            }
                        }
                        Some(InlineObject::Field { instr, result, code_runs, .. }) if result.is_empty() && crate::eq_field::is_eq(instr) => {
                            let eq = crate::eq_field::parse(instr).unwrap_or_default();
                            let ml = {
                                let runs = kition_doc_doc::field_code_runs(instr, code_runs);
                                let mut shaper = EqShaper {
                                    b: &mut b,
                                    runs,
                                    field: rc.clone(),
                                    resolved: vec![None; runs.map_or(0, <[_]>::len)],
                                    resolve: &resolve,
                                    guide: &eq.guide,
                                    in_guide: false,
                                };
                                crate::eq_field::layout(&eq, &mut shaper)
                            };
                            // Like an equation: one editing unit as wide as its drawing.
                            b.clusters.push(Cluster {
                                start,
                                end,
                                adv: ml.width,
                                kind: ClKind::Object(k),
                                style: si,
                                g0: g,
                                g1: g,
                                break_after: false,
                                obj_h: ml.ascent.max(0.01),
                                obj_d: ml.descent,
                                dot: false,
                            });
                            eq_fields.push((k, Arc::new(ml)));
                        }
                        Some(InlineObject::Field { instr, result, code_fields, .. }) => {
                            let (t, page_dep) = field_text_with(instr, code_fields, result, env.fields);
                            has_page_fields |= page_dep;
                            b.shape_field(&t, start, end, &rc);
                        }
                        Some(InlineObject::NoteRef { kind, id, custom }) => {
                            let num = if custom.is_empty() {
                                let n = env.fields.note_number(*id);
                                match kind {
                                    NoteKind::Footnote => doc.settings.footnote_format.format(n),
                                    NoteKind::Endnote => doc.settings.endnote_format.format(n),
                                }
                            } else {
                                custom.clone()
                            };
                            let mut sup = (*rc).clone();
                            sup.vert_align = kition_doc_doc::props::VertAlign::Superscript;
                            b.shape_atomic(&num, start, end, &Arc::new(sup));
                            notes.push((b.clusters.len().saturating_sub(1), *id));
                        }
                        Some(InlineObject::Equation { linear, display, math, .. }) => {
                            let avail = (env.width - rp.indent_left.max(0.0) - rp.indent_right.max(0.0)).max(12.0);
                            let ml = crate::math::layout_equation(math, linear, &rc, *display, avail, &math_props, &mut eq_counter);
                            let st = b.styles.get(si as usize);
                            // The line is at least as tall as the text around it.
                            let (a, d) = st.map(|s| (s.ascent, s.descent)).unwrap_or((0.0, 0.0));
                            b.clusters.push(Cluster {
                                start,
                                end,
                                adv: ml.width,
                                kind: ClKind::Object(k),
                                style: si,
                                g0: g,
                                g1: g,
                                break_after: false,
                                obj_h: ml.ascent.max(a).max(0.01),
                                obj_d: ml.descent.max(d),
                                dot: false,
                            });
                            if *display {
                                displays.push((b.clusters.len() - 1, math.jc));
                            }
                            maths.push((k, Arc::new(ml)));
                        }
                        Some(InlineObject::Opaque { text, .. }) => b.shape_atomic(text, start, end, &rc),
                        _ => push(&mut b, ClKind::Marker, 0.0, 0.0),
                    }
                }
            }
        }
        if let Some(s) = text.get(seg..) {
            b.shape(s, range.start + seg, &rc, None);
        }
    }
    // Break opportunities and hyphenation points come from the text as laid out: hidden text that is
    // not shown and, without markup, tracked deletions are left out, so a space or soft hyphen there
    // is no place to break, and words are hyphenated as they read.
    // The text and, for each char of it, its end there and in `p.text`.
    let (text, ends): (std::borrow::Cow<str>, Vec<(usize, usize)>) = if left.is_empty() {
        (p.text.as_str().into(), Vec::new())
    } else {
        let (mut text, mut ends) = (String::new(), Vec::new());
        let mut next = left.iter().peekable();
        for (i, c) in p.text.char_indices() {
            // `left` is in order: skip the ranges that end here, then is `i` in the next one?
            while next.next_if(|r| r.end <= i).is_some() {}
            if next.peek().is_some_and(|r| r.start <= i) {
                continue;
            }
            text.push(c);
            ends.push((text.len(), i + c.len_utf8()));
        }
        (text.into(), ends)
    };
    // The end of a char in `text` → the same place in `p.text`.
    let to_para = |i: usize| -> Option<usize> {
        if left.is_empty() { Some(i) } else { ends.binary_search_by_key(&i, |e| e.0).ok().and_then(|k| ends.get(k)).map(|e| e.1) }
    };
    let mut found = Vec::new();
    for (i, o) in unicode_linebreak::linebreaks(&text) {
        // Word keeps "and/or" and web addresses whole: no break right after a slash; WPS for Mac
        // keeps "a?b", "a!b" and "a-b" whole too, hyphen included (a word too long for the line
        // still breaks anywhere).
        let after_slash = text.get(..i).is_some_and(|t| t.ends_with(['/', '?', '!', '-']))
            && text.get(i..).and_then(|t| t.chars().next()).is_some_and(char::is_alphanumeric);
        if (o == unicode_linebreak::BreakOpportunity::Allowed || i < text.len()) && !after_slash {
            found.push(i);
        }
    }
    // Asian typography: kinsoku and breaking Latin words anywhere.
    crate::kinsoku::apply(&text, &mut found, rp.kinsoku, rp.word_wrap);
    let opps: std::collections::HashSet<usize> = found.into_iter().filter_map(to_para).collect();
    for c in &mut b.clusters {
        c.break_after = opps.contains(&c.end) || matches!(c.kind, ClKind::Object(_) | ClKind::Tab);
    }

    // List label.
    let label = env.label.as_ref().map(|(text, lvl)| {
        let lc = p.mark.clone().overlaid(&lvl.chr);
        let rc = resolve(&lc);
        // A Symbol/Wingdings bullet (U+F0B7…) without that font installed, or in a face that has
        // no glyph at the private-use code: draw the standard character instead, since a
        // substitute has another glyph, or none, there.
        let r = kition_doc_fonts::word::resolve(&rc.font, rc.bold, rc.italic);
        let text: String = text
            .chars()
            .map(|c| match kition_doc_fonts::word::symbol_font_char(&rc.font, c) {
                Some(u) if r.substituted || !r.face.covers(c) => u,
                _ => c,
            })
            .collect();
        let before_c = b.clusters.len();
        let before_g = b.glyphs.len();
        b.shape_with(&text, 0, &rc, None, false);
        let mut cl: Vec<Cluster> = b.clusters.drain(before_c..).collect();
        // A right-to-left paragraph's label reads right to left too ("1." shows as ".1").
        if rp.bidi {
            let lv = bidi_levels(&text, true);
            let cl_lv: Vec<u8> = cl.iter().map(|c| lv.get(c.start).copied().unwrap_or(1)).collect();
            let order = visual_order(&cl_lv);
            cl = order.iter().filter_map(|&i| cl.get(i).cloned()).collect();
        }
        let mut glyphs = Vec::new();
        let mut x = 0.0;
        let mut style = mark_style;
        for c in &cl {
            style = c.style;
            for g in b.glyphs.get(c.g0 as usize..c.g1 as usize).into_iter().flatten() {
                glyphs.push(Glyph { dx: g.dx + x, ..*g });
            }
            x += c.adv;
        }
        b.glyphs.truncate(before_g);
        Label { text, style, glyphs, x: 0.0, width: x }
    });

    // A space beside Chinese text or another space, with single-byte and double-byte widths
    // balanced: half an em.
    let mut firm_spaces = Vec::new();
    for (k, c) in b.clusters.iter_mut().enumerate() {
        if c.kind == ClKind::Space
            && b.ea_spaces.binary_search(&c.start).is_ok()
            && let Some(st) = b.styles.get(c.style as usize)
        {
            c.adv = st.size * st.width_scale * 0.5;
            firm_spaces.push(k as u32);
        }
    }
    // On a character grid East Asian characters take whole grid cells, set at the cell's start
    // (WPS for Mac: luan's vertical 出师表).
    if let Some(pitch) = env.char_pitch.filter(|_| rp.snap_to_grid) {
        for c in &mut b.clusters {
            let east_asian = c.kind == ClKind::Text
                && p.text
                    .get(c.start..c.end)
                    .and_then(|t| t.chars().next())
                    .zip(b.styles.get(c.style as usize))
                    .is_some_and(|(ch, st)| st.rc.uses_east_asian(ch));
            if east_asian && c.adv > 0.0 {
                c.adv = pitch * (c.adv / pitch - 0.01).ceil().max(1.0);
            }
        }
    }
    // In vertical text a run set across the line (`w:eastAsianLayout w:vert`) takes one em
    // along it, whatever its letters (WPS for Mac: luan's red years in a vertical section).
    let mut across: Vec<std::ops::Range<usize>> = Vec::new();
    if env.vertical {
        let set_across = |c: &Cluster| {
            c.kind == ClKind::Text
                && b.styles.get(c.style as usize).is_some_and(|st| st.rc.east_asian_layout.is_some_and(|l| l.vertical == Some(true)))
        };
        let mut k = 0;
        while k < b.clusters.len() {
            if !b.clusters.get(k).is_some_and(set_across) {
                k += 1;
                continue;
            }
            let style = b.clusters.get(k).map(|c| c.style);
            let mut end = k;
            while b.clusters.get(end).is_some_and(|c| set_across(c) && Some(c.style) == style) {
                end += 1;
            }
            let size = style.and_then(|s| b.styles.get(s as usize)).map_or(10.5, |st| st.size);
            for (m, c) in b.clusters.iter_mut().enumerate().take(end).skip(k) {
                c.adv = if m == k { size } else { 0.0 };
                c.break_after = m + 1 == end && c.break_after;
            }
            across.push(k..end);
            k = end;
        }
    }
    let mut fit = crate::fit_text::apply(&fit_runs, &mut b.styles, &mut b.glyphs, &mut b.clusters);
    let combined = if !rp.bidi && b.levels.is_empty() && drop_cap.is_none() {
        crate::two_lines::apply(&combined_runs, &rp, &mut b.styles, &mut b.glyphs, &mut b.clusters, &mut fit, &p.text)
    } else {
        Vec::new()
    };
    let mut pl = ParaLayout {
        rp,
        styles: b.styles,
        glyphs: b.glyphs,
        clusters: b.clusters,
        shown: b.shown,
        combined,
        across,
        lines: Vec::new(),
        label,
        height: 0.0,
        text_len: p.text.len(),
        has_page_fields,
        notes,
        issues: if env.proofing { proof_issues(p) } else { Vec::new() },
        drop_cap,
        hyph_after: Vec::new(),
        firm_spaces,
        bidi_levels: b.levels,
        drop_cap_w,
        maths,
        eq_fields,
        displays,
        left_out: Vec::new(),
        control_marks: control_marks(p, resolve(&CharProps::default()).size),
    };
    pl.hyph_after = hyphenation_points(&text, to_para, &pl);
    pl.hyph_after.retain(|index| !fit.contains(*index as usize));
    pl.left_out = left;
    for k in pl.hyph_after.clone() {
        if let Some(c) = pl.clusters.get_mut(k as usize) {
            c.break_after = true;
        }
    }
    let mut auto_spacing = crate::auto_spacing::before(&pl.clusters, &pl.styles, &p.text, &pl.rp);
    let (gaps, reach) = crate::compression::gaps(&pl.clusters, &p.text, &auto_spacing, env.doc.settings.char_spacing.as_deref());
    let mut marks = crate::compression::Marks {
        squeeze: crate::compression::capacities(&pl.clusters, &pl.styles, &p.text, env.doc.settings.char_spacing.as_deref()),
        hang: crate::compression::hangers(&pl.clusters, &pl.styles, &p.text, pl.rp.overflow_punct),
        gaps,
        reach,
    };
    for index in 0..pl.clusters.len() {
        if fit.contains(index) {
            if let Some(value) = marks.squeeze.get_mut(index) {
                *value = 0.0;
            }
            if let Some(value) = marks.hang.get_mut(index) {
                *value = false;
            }
            if let Some(value) = marks.gaps.get_mut(index) {
                *value = 0.0;
            }
            if index > 0
                && fit.internal_after(index - 1)
                && let Some(value) = auto_spacing.get_mut(index)
            {
                *value = 0.0;
            }
        }
    }
    break_lines(&mut pl, env, mark_style, env.label.as_ref().map(|(_, l)| l), &auto_spacing, &marks, &fit);
    crate::spacing_units::apply(&mut pl, env);
    pl
}

/// Where a paragraph's content-control marks sit and how tall they are. WPS makes room on the line
/// for a control's start and end marks as if they were glyphs of their size (the paragraph
/// style's when the control sets none): 1.125 em above the baseline and 0.39 em below (WPS for Mac
/// references, 10.5 to 40 pt). A control around paragraphs marks its first and last paragraph's
/// ends; one around runs, its first and last character.
fn control_marks(p: &Paragraph, style_size: f32) -> Vec<(usize, f32, f32)> {
    let mut out = Vec::new();
    let mut push = |at: usize, size: Option<f32>| {
        if let Some(size) = size {
            let em = if size > 0.0 { size } else { style_size }.clamp(0.0, 1638.0);
            out.push((at, em * 1.125, em * 0.39));
        }
    };
    let last = p.text.len().saturating_sub(1);
    if let Some(m) = p.props.control {
        push(0, m.start);
        push(last, m.end);
    }
    let mut open: Option<(usize, usize, kition_doc_doc::props::ControlMarks)> = None;
    for (range, props) in p.run_ranges().map(|(r, props)| (r, props.control)).chain(std::iter::once((p.text.len()..p.text.len(), None))) {
        if let Some((_, end, m)) = &mut open
            && props == Some(*m)
            && *end == range.start
        {
            *end = range.end;
            continue;
        }
        if let Some((start, end, m)) = open.take() {
            push(start, m.start);
            push(end.saturating_sub(1).max(start), m.end);
        }
        open = props.map(|m| (range.start, range.end, m));
    }
    out
}

/// Next tab stop after `x` (column coordinates): explicit stops, the hanging-indent implicit stop,
/// then default stops.
fn next_tab(x: f32, tabs: &[TabStop], default_tab: f32, hanging_at: Option<f32>) -> TabStop {
    let mut best: Option<TabStop> = tabs.iter().copied().filter(|t| t.pos > x + 0.01).min_by(|a, b| a.pos.total_cmp(&b.pos));
    if let Some(h) = hanging_at
        && h > x + 0.01
        && best.is_none_or(|t| h < t.pos)
    {
        best = Some(TabStop { pos: h, align: TabAlign::Left, leader: TabLeader::None });
    }
    best.unwrap_or_else(|| {
        let d = if default_tab > 1.0 { default_tab } else { 36.0 };
        let n = ((x + 0.01) / d).floor() + 1.0;
        TabStop { pos: (n * d).min(1e6), align: TabAlign::Left, leader: TabLeader::None }
    })
}

fn grid_line_height(height: f32, paragraph: &ResolvedPara, pitch: Option<f32>) -> f32 {
    if !paragraph.snap_to_grid || matches!(paragraph.line_spacing, LineSpacing::Exactly(_)) {
        return height;
    }
    let Some(pitch) = pitch.filter(|pitch| pitch.is_finite() && *pitch > 0.0) else { return height };
    ((height / pitch - 0.00001).ceil() * pitch).max(height)
}

/// The least height of a gridded line, not rounded to whole rows: at a multiple, the multiple of the
/// pitch (WPS lays a 2.4-line heading out 2.4 rows tall, and 1.3-line body text 1.3 rows); at
/// least a height, that height (WPS spaces "at least 18 pt" lines 18 pt apart on a 15.6 pt grid);
/// 0 otherwise.
fn grid_multiple_floor(paragraph: &ResolvedPara, pitch: Option<f32>) -> f32 {
    match (paragraph.line_spacing, pitch) {
        (LineSpacing::Multiple(multiple), Some(pitch)) if paragraph.snap_to_grid && pitch.is_finite() && pitch > 0.0 && multiple.is_finite() => {
            multiple * pitch
        }
        (LineSpacing::AtLeast(minimum), Some(pitch)) if paragraph.snap_to_grid && pitch.is_finite() && pitch > 0.0 && minimum.is_finite() => {
            minimum.max(0.0)
        }
        _ => 0.0,
    }
}

/// Height before grid rounding and the text box used to center glyphs in that height.
fn line_spacing_heights(natural: f32, paragraph: &ResolvedPara, pitch: Option<f32>) -> (f32, f32) {
    match paragraph.line_spacing {
        LineSpacing::Multiple(multiple) => {
            if pitch.is_some_and(|pitch| paragraph.snap_to_grid && pitch.is_finite() && pitch > 0.0) {
                // On a document grid the multiple scales the pitch, not the text: a line takes the
                // rows its text needs or the pitch times the multiple, whichever is taller (see
                // `grid_multiple_floor`), as WPS spaces it. Enlarging the line box does not scale
                // the glyph metrics used to position its baseline.
                (natural, natural)
            } else {
                let height = natural * multiple;
                (height, height)
            }
        }
        // On a line grid the text's rows are rounded and the minimum is a floor of its own
        // (`grid_multiple_floor`).
        LineSpacing::AtLeast(_) if pitch.is_some_and(|pitch| paragraph.snap_to_grid && pitch.is_finite() && pitch > 0.0) => (natural, natural),
        LineSpacing::AtLeast(minimum) => {
            let height = natural.max(minimum);
            (height, height)
        }
        LineSpacing::Exactly(height) => (height, height),
    }
}

fn break_lines(
    pl: &mut ParaLayout,
    env: &ParaEnv,
    mark_style: u16,
    level: Option<&Level>,
    auto_spacing: &[f32],
    marks: &crate::compression::Marks,
    fit: &crate::inline_groups::Groups,
) {
    let suffix = level.map(|l| l.suffix);
    let rp = pl.rp.clone();
    let mut width = env.width.max(12.0);
    let mut base_right = (width - rp.indent_right).max(1.0);
    let default_tab = env.doc.settings.default_tab;
    let first_left = rp.indent_left + rp.indent_first;
    // Word 2013+ (compatibility mode 15) fits more text on a justified line by shrinking its
    // spaces, up to MAX_SPACE_SHRINK of their width; never on lines with tabs.
    let shrink_spaces = rp.align == Align::Justify && env.doc.settings.compat_mode >= kition_doc_doc::COMPAT_MODE_CURRENT;
    let hanging_at = if rp.indent_first < 0.0 { Some(rp.indent_left) } else { None };
    let n = pl.clusters.len();
    // A right-to-left paragraph is laid out from its start edge, as if mirrored: indents, tabs,
    // the list label and alignment are measured from the right. Lines are mirrored back (and
    // bidirectional text reordered) once broken; floating objects are mirrored in.
    let rtl_para = rp.bidi;
    let bidi = rtl_para || !pl.bidi_levels.is_empty();
    let mirrored: Vec<Exclusion>;
    let exclusions: &[Exclusion] = if rtl_para {
        mirrored = env.exclusions.iter().map(|e| Exclusion { left: width - e.right, right: width - e.left, ..*e }).collect();
        &mirrored
    } else {
        env.exclusions
    };
    let mut first_x0: Option<f32> = None;
    let mut hcache: Vec<(u16, u32, f32)> = Vec::new();
    let mut lines: Vec<Line> = Vec::new();
    let mut i = 0usize;
    let mut top = 0.0f32;
    let mut first = true;
    let natural = pl.styles.get(mark_style as usize).map_or(14.0, |style| style.ascent + style.descent).max(1.0);
    let estimate = if matches!(rp.line_spacing, LineSpacing::Multiple(_)) { line_spacing_heights(natural, &rp, env.grid_pitch).0 } else { natural };
    let est_h = grid_line_height(estimate, &rp, env.grid_pitch).max(grid_multiple_floor(&rp, env.grid_pitch));
    // How far the text sits inside the line's height (centred in it), for outlines.
    let text_pad = ((est_h - natural) / 2.0).max(0.0);
    // The rest of the current row: spans on the far side of floating objects, left to right.
    let mut row_rest: VecDeque<(f32, f32)> = VecDeque::new();
    let mut row_first = 0usize;
    // Rows passed over because a word didn't fit beside a floating object.
    let mut skipped_rows = 0usize;
    loop {
        if let Some((from, w)) = env.widen
            && lines.len() >= from
            && w.is_finite()
        {
            width = w.max(12.0);
            base_right = (width - rp.indent_right).max(1.0);
        }
        let beside = !row_rest.is_empty();
        let (left, right_edge) = if let Some(span) = row_rest.pop_front() {
            span
        } else {
            // Flow around floating objects: the spans beside them, or below them.
            let mut guard = 0;
            loop {
                let mut lo = if first { first_left } else { rp.indent_left };
                if let Some((_, dl, dw)) = pl.drop_cap
                    && lines.len() < dl as usize
                {
                    lo = first_left.max(rp.indent_left) + dw;
                }
                match row_spans(exclusions, top, est_h, text_pad, lo, base_right).map(|mut spans| (spans.pop_front(), spans)) {
                    Ok((Some((a, b)), rest)) => {
                        row_rest = rest;
                        row_first = lines.len();
                        break (a, b.max(a + 12.0));
                    }
                    Err(below) if below > top && guard < 50 => {
                        top = below;
                        guard += 1;
                    }
                    Ok((None, _)) | Err(_) => {
                        row_first = lines.len();
                        break (lo, base_right.max(lo + 12.0));
                    }
                }
            }
        };
        // A word that doesn't fit the room beside a floating object isn't broken there: the line
        // moves down a row and tries again (WPS for Mac).
        let in_the_way = exclusions.iter().any(|e| e.bottom > top && e.top < top + est_h);
        if !beside && in_the_way && skipped_rows < 200 {
            let mut normal_left = if first { first_left } else { rp.indent_left };
            if let Some((_, dl, dw)) = pl.drop_cap
                && lines.len() < dl as usize
            {
                normal_left = first_left.max(rp.indent_left) + dw;
            }
            let narrowed = right_edge < base_right - 0.5 || left > normal_left.max(0.0) + 0.5;
            if narrowed {
                // Its punctuation gives up what it may at a line's end (as much as its largest
                // mark's), and a comma or full stop ending it hangs past the room.
                let (mut word, mut squeeze, mut hung) = (0.0f32, 0.0f32, 0.0f32);
                for (k, c) in pl.clusters.iter().enumerate().skip(i) {
                    if c.kind == ClKind::Space {
                        break;
                    }
                    word += c.adv;
                    squeeze = squeeze.max(marks.squeeze.get(k).copied().unwrap_or(0.0));
                    hung = if marks.hang.get(k).copied().unwrap_or(false) { c.adv } else { 0.0 };
                    if c.break_after {
                        break;
                    }
                }
                let word = word - squeeze - hung;
                if word > right_edge - left + 0.01 {
                    top += est_h;
                    skipped_rows += 1;
                    continue;
                }
            }
        }
        let mut x = left;
        // Label on the first line.
        if first && let Some(lab) = pl.label.as_mut() {
            // The number is aligned at the first-line indent: its left edge, centre or right edge.
            lab.x = match level.map(|l| l.align) {
                Some(Align::Center) => left - lab.width / 2.0,
                Some(Align::Right) => left - lab.width,
                _ => left,
            };
            let end = lab.x + lab.width;
            x = match suffix.unwrap_or(LevelSuffix::Tab) {
                LevelSuffix::Tab => {
                    let t = next_tab(end, &rp.tabs, default_tab, hanging_at).pos;
                    // The level's own tab stop after the number, when it comes first.
                    match level.and_then(|l| l.tab).filter(|p| p.is_finite() && *p > end + 0.01) {
                        Some(p) if p < t => p,
                        _ => t,
                    }
                }
                LevelSuffix::Space => end + pl.styles.get(lab.style as usize).map(|s| s.size * 0.25).unwrap_or(3.0),
                LevelSuffix::Nothing => end,
            };
        }
        let line_start_x = x;
        let c0 = i;
        let mut xs: Vec<f32> = Vec::new();
        let mut leaders = Vec::new();
        let mut last_break: Option<usize> = None; // cluster index after which we may break
        let mut last_plain: Option<(usize, f32)> = None; // last non-hyphen break and the x after it
        let mut end = LineEnd::Para;
        let mut j = i;
        let mut pending_tab: Option<(usize, TabStop, f32)> = None; // tab cluster, stop, x where tab started
        let mut space_w = 0.0f32; // width of the spaces so far on this line
        let mut punctuation_capacity = 0.0f32;
        // A line gives up at most its largest mark's capacity, however many marks share it.
        let mut punctuation_cap = 0.0f32;
        // The automatic East Asian–Latin gaps on this line and what each gives up.
        let mut line_gaps: Vec<(usize, f32)> = Vec::new();
        let mut gap_capacity = 0.0f32;
        let mut has_tab = false;
        let mut has_visible = false;
        while j < n {
            let Some(c) = pl.clusters.get(j).cloned() else { break };
            match c.kind {
                ClKind::LineBreak | ClKind::PageBreak | ClKind::ColumnBreak => {
                    resolve_tab(pl, &mut pending_tab, &mut xs, c0, x, &mut x);
                    xs.push(x);
                    end = match c.kind {
                        ClKind::LineBreak => LineEnd::LineBreak,
                        ClKind::PageBreak => LineEnd::PageBreak,
                        _ => LineEnd::ColumnBreak,
                    };
                    j += 1;
                    // Only marks after a page or column break (bookmarks, a content control's end):
                    // they stay on the break's line rather than starting one after it, as WPS
                    // keeps them.
                    if end != LineEnd::LineBreak && pl.clusters.get(j..).is_some_and(|rest| rest.iter().all(|c| c.kind == ClKind::Marker)) {
                        j = n;
                    }
                    break;
                }
                ClKind::Tab => {
                    has_tab = true;
                    resolve_tab(pl, &mut pending_tab, &mut xs, c0, x, &mut x);
                    let stop = next_tab(x, &rp.tabs, default_tab, hanging_at);
                    xs.push(x);
                    if stop.pos > right_edge + 0.01 && j > c0 && stop.align == TabAlign::Left {
                        // Tab past the right indent wraps (Word moves it to the next line).
                        if let Some(c) = pl.clusters.get_mut(j) {
                            c.adv = 0.0;
                        }
                        x = right_edge.max(x);
                        last_break = Some(j);
                        j += 1;
                        continue;
                    }
                    match stop.align {
                        TabAlign::Left | TabAlign::Bar | TabAlign::Clear => {
                            let w = (stop.pos - x).max(0.0);
                            if let Some(c) = pl.clusters.get_mut(j) {
                                c.adv = w;
                            }
                            if stop.leader != TabLeader::None {
                                leaders.push((j, stop.leader));
                            }
                            x += w;
                        }
                        _ => {
                            pending_tab = Some((j, stop, x));
                            if stop.leader != TabLeader::None {
                                leaders.push((j, stop.leader));
                            }
                        }
                    }
                    last_break = Some(j);
                    j += 1;
                    continue;
                }
                _ => {}
            }
            // A display equation sits on a line of its own.
            let is_display = pl.displays.iter().any(|(ci, _)| *ci == j);
            if is_display && (c0..j).any(|k| pl.clusters.get(k).is_some_and(|c| c.kind != ClKind::Marker)) {
                end = LineEnd::Wrap;
                break;
            }
            // Text after a right/centre tab grows leftwards into the tab's space first.
            let gap = if has_visible { auto_spacing.get(j).copied().unwrap_or(0.0) } else { 0.0 };
            let advance = c.adv + gap;
            let measured = fit.starting_width(j).map_or(advance, |width| width + gap);
            let absorbs = pending_tab.is_some_and(|(tj, stop, _)| {
                let room = pl.clusters.get(tj).map(|t| t.adv).unwrap_or(0.0);
                match stop.align {
                    TabAlign::Right | TabAlign::Decimal => room >= measured,
                    TabAlign::Center => room >= measured / 2.0 && x + measured / 2.0 <= right_edge + 0.01,
                    _ => false,
                }
            });
            let punctuation = marks.squeeze.get(j).copied().unwrap_or(0.0);
            // A closing mark may hang past the right edge when it starts inside it.
            let hangs = marks.hang.get(j).copied().unwrap_or(false) && !has_tab;
            let squeezed_gap = if gap > 0.0 { marks.gaps.get(j).copied().unwrap_or(0.0) } else { 0.0 };
            // The automatic gaps give up their share only on a line nothing else compresses on.
            let others = punctuation_capacity + punctuation > 0.0 || (shrink_spaces && space_w > 0.0);
            let gaps = if others { 0.0 } else { (gap_capacity + squeezed_gap).min(marks.reach.get(j).copied().unwrap_or(0.0)) };
            let shrink = if !has_tab {
                (punctuation_capacity + punctuation).min(punctuation_cap.max(punctuation))
                    + if shrink_spaces { space_w * MAX_SPACE_SHRINK } else { 0.0 }
                    + gaps
            } else {
                0.0
            };
            let fitted = fit.contains(j);
            let inside_group = j > 0 && fit.internal_after(j - 1);
            let fits = inside_group
                || (fitted && !has_visible)
                || absorbs
                || x + measured <= right_edge + shrink + 0.01
                || (hangs && x + gap <= right_edge + shrink + 0.01)
                || (!fitted && matches!(c.kind, ClKind::Space | ClKind::Marker));
            if !fits && j > c0 {
                // Wrap: back up to the last break opportunity on this line.
                end = LineEnd::Wrap;
                // Hyphenate only when the plain break leaves more than the hyphenation zone empty.
                if let (Some(bk), Some((pj, px))) = (last_break, last_plain)
                    && pj >= c0
                    && pj < bk
                    && pl.hyph_after.binary_search(&(bk as u32)).is_ok()
                    && right_edge - px <= HYPHENATION_ZONE
                {
                    last_break = Some(pj);
                }
                if let Some(bk) = last_break.filter(|bk| *bk >= c0) {
                    xs.truncate(bk + 1 - c0);
                    j = bk + 1;
                    // Recompute x for the kept clusters.
                    x = xs.last().copied().unwrap_or(line_start_x) + pl.clusters.get(bk).map(|c| c.adv).unwrap_or(0.0);
                } else {
                    // No opportunity: break here (a long word).
                }
                break;
            }
            x += gap;
            xs.push(x);
            x += c.adv;
            punctuation_capacity += punctuation;
            punctuation_cap = punctuation_cap.max(punctuation);
            if squeezed_gap > 0.0 {
                gap_capacity += squeezed_gap;
                line_gaps.push((j, squeezed_gap));
            }
            has_visible |= c.kind != ClKind::Marker;
            if c.kind == ClKind::Space && !fitted && pl.firm_spaces.binary_search(&(j as u32)).is_err() {
                space_w += c.adv;
            }
            // Decimal/center/right tab: shift pending text as it grows.
            if let Some((tj, stop, tx)) = pending_tab {
                let seg_w = x - tx;
                let shift = match stop.align {
                    TabAlign::Right => stop.pos - tx - seg_w,
                    TabAlign::Center => stop.pos - tx - seg_w / 2.0,
                    TabAlign::Decimal => {
                        // Width up to the first '.' in the segment.
                        let mut w = 0.0;
                        let mut found = false;
                        for k in tj + 1..=j {
                            let Some(cc) = pl.clusters.get(k) else { break };
                            let s = pl_text_at(cc);
                            if found {
                                break;
                            }
                            if s {
                                found = true;
                                break;
                            }
                            w += cc.adv + auto_spacing.get(k).copied().unwrap_or(0.0);
                        }
                        stop.pos - tx - if found { w } else { seg_w }
                    }
                    _ => 0.0,
                };
                let shift = shift.max(0.0);
                let old = pl.clusters.get(tj).map(|c| c.adv).unwrap_or(0.0);
                if (shift - old).abs() > 0.001 {
                    let delta = shift - old;
                    if let Some(c) = pl.clusters.get_mut(tj) {
                        c.adv = shift;
                    }
                    let from = tj + 1 - c0;
                    for v in xs.iter_mut().skip(from) {
                        *v += delta;
                    }
                    x += delta;
                }
            }
            if is_display {
                // Markers right after it stay on its line; anything else starts a new one.
                j += 1;
                while j < n && pl.clusters.get(j).is_some_and(|c| c.kind == ClKind::Marker) {
                    xs.push(x);
                    j += 1;
                }
                end = if j >= n { LineEnd::Para } else { LineEnd::Wrap };
                break;
            }
            if c.break_after {
                let hyph = pl.hyph_after.binary_search(&(j as u32)).is_ok();
                if !hyph {
                    last_break = Some(j);
                    last_plain = Some((j, x));
                } else if x + hyphen_glyph(pl, c.style, &mut hcache).2 <= right_edge + shrink + 0.01 {
                    last_break = Some(j);
                }
            }
            j += 1;
        }
        if end == LineEnd::Para || end == LineEnd::Wrap {
            let xc = x;
            resolve_tab(pl, &mut pending_tab, &mut xs, c0, xc, &mut x);
        }
        if j >= n && end != LineEnd::Wrap && !matches!(end, LineEnd::LineBreak | LineEnd::PageBreak | LineEnd::ColumnBreak) {
            end = LineEnd::Para;
        }
        let c1 = j.max(c0);
        // Ensure progress.
        let (c1, j) = if c1 == c0 && c0 < n { (c0 + 1, c0 + 1) } else { (c1, j) };
        while xs.len() < c1 - c0 {
            let last = xs.last().copied().unwrap_or(line_start_x);
            let adv = pl.clusters.get(c0 + xs.len().saturating_sub(1)).map(|c| c.adv).unwrap_or(0.0);
            xs.push(if xs.is_empty() { line_start_x } else { last + adv });
        }
        xs.truncate(c1 - c0);
        // A line wrapped at a hyphenation point shows a hyphen after its last cluster.
        let hyphen = (end == LineEnd::Wrap && c1 > c0 && pl.hyph_after.binary_search(&((c1 - 1) as u32)).is_ok())
            .then(|| pl.clusters.get(c1 - 1).map(|c| hyphen_glyph(pl, c.style, &mut hcache)))
            .flatten();
        let end_x = match (xs.last(), c1.checked_sub(1).and_then(|k| pl.clusters.get(k))) {
            (Some(lx), Some(c)) => lx + c.adv + hyphen.map_or(0.0, |h| h.2),
            _ => line_start_x,
        };
        xs.push(end_x);
        pending_tab = None;
        let _ = pending_tab;

        // Vertical metrics.
        let (mut asc, mut desc) = (0.0f32, 0.0f32);
        // The tallest picture on the line (it sits on the baseline).
        let mut obj_asc = 0.0f32;
        // Whether the line holds text (letters, not only pictures, spaces and marks).
        let mut texted = false;
        let mut any = false;
        let dropped = pl.drop_cap.map_or(0, |d| d.0);
        // A content control's marks on the line count like text.
        let (mut mark_asc, mut mark_desc) = (0.0f32, 0.0f32);
        for &(at, a, d) in &pl.control_marks {
            let here = pl.text_len == 0 || pl.clusters.get(c0..c1).into_iter().flatten().any(|c| c.start <= at && at < c.end);
            if here {
                mark_asc = mark_asc.max(a);
                mark_desc = mark_desc.max(d);
                texted = true;
            }
        }
        for (k, c) in pl.clusters.get(c0..c1).into_iter().flatten().enumerate() {
            // A floating object's anchor takes no room on its line: the font of the run holding it
            // doesn't make the line taller (WPS references).
            let floating_anchor = matches!(c.kind, ClKind::Object(_)) && c.obj_h == 0.0 && c.adv == 0.0;
            if c.kind == ClKind::Marker || floating_anchor || c0 + k < dropped {
                continue;
            }
            if let Some(st) = pl.styles.get(c.style as usize) {
                // A picture sits on the baseline of its run: the line keeps that font's ascent
                // and descent, and grows above the baseline to fit a taller picture.
                let (a, d) = if matches!(c.kind, ClKind::Object(_)) && c.obj_h > 0.0 {
                    if pl.eq_fields.iter().any(|(k, _)| c.kind == ClKind::Object(*k)) {
                        // An `EQ` field's drawing, and the field's own run around it.
                        (c.obj_h.max(st.ascent), c.obj_d.max(st.descent))
                    } else if pl.maths.iter().any(|(k, _)| c.kind == ClKind::Object(*k)) {
                        // An equation is text: its ascent and descent count like the text's.
                        (c.obj_h, c.obj_d)
                    } else {
                        obj_asc = obj_asc.max(c.obj_h);
                        (st.ascent, st.descent)
                    }
                } else {
                    texted |= c.kind == ClKind::Text;
                    // A character border's line stands above and below the text (WPS for Mac).
                    let border = st.rc.border.filter(|b| b.is_visible()).map_or(0.0, |b| b.width.clamp(0.0, 100.0));
                    (st.ascent + st.shift.max(0.0) + border, st.descent + (-st.shift).max(0.0) + border)
                };
                asc = asc.max(a);
                desc = desc.max(d);
                any = true;
            }
        }
        if (!any || (asc == 0.0 && desc == 0.0))
            && let Some(st) = pl.styles.get(mark_style as usize)
        {
            asc = st.ascent;
            desc = st.descent;
        }
        asc = asc.max(mark_asc);
        desc = desc.max(mark_desc);
        if first
            && let Some(lab) = &pl.label
            && let Some(st) = pl.styles.get(lab.style as usize)
        {
            asc = asc.max(st.ascent);
            desc = desc.max(st.descent);
        }
        // Line spacing scales the text's height, never a picture's: a line is at least as tall
        // as its tallest picture plus the descent below the baseline.
        // shortcut: Word leaves less than a full descent below a line of only pictures (0.5pt to
        // 1.9pt in docxide-pdf's header fixtures case121/case123, where the descent is 3.3pt), and
        // no rule tried so far (none, only the text's descent) matched both; it needs Word probes.
        let natural = asc + desc;
        let pictures = if obj_asc > asc { obj_asc + desc } else { 0.0 };
        let (text_height, baseline_height) = line_spacing_heights(natural, &rp, env.grid_pitch);
        // A line grid spaces a line holding an inline picture, chart or diagram like a text line:
        // it takes whole grid rows. A line of only pictures takes as many as the tallest needs,
        // the descent below it adding none, and the picture keeps to the line's top; with text
        // beside the pictures, the text's descent counts (measured on WPS references).
        let exact = matches!(rp.line_spacing, LineSpacing::Exactly(_));
        let gridded = rp.snap_to_grid && env.grid_pitch.is_some_and(|pitch| pitch.is_finite() && pitch > 0.0);
        let flush = if gridded && !texted && obj_asc > asc { obj_asc } else { 0.0 };
        let rows = if flush > 0.0 { flush } else { pictures };
        let unsnapped = if exact { baseline_height } else { baseline_height.max(pictures) };
        let snapped = grid_line_height(if exact { text_height } else { text_height.max(rows) }, &rp, env.grid_pitch)
            .max(grid_multiple_floor(&rp, env.grid_pitch));
        let height = if exact || gridded { snapped } else { snapped.max(pictures) };
        // Off a line grid, the room multiple line spacing adds goes below the text: the baseline
        // sits one ascent below the line's top (measured on WPS references).
        let below = matches!(rp.line_spacing, LineSpacing::Multiple(_)) && !gridded;
        // A gridded line held open by its "at least" height keeps its text at the bottom, as off
        // the grid (WPS references).
        let at_least_floor =
            matches!(rp.line_spacing, LineSpacing::AtLeast(m) if gridded && m > grid_line_height(text_height.max(rows), &rp, env.grid_pitch) + 0.01);
        let baseline = if below {
            top + unsnapped.min(natural.max(pictures)) - desc
        } else if at_least_floor {
            top + height - desc
        } else {
            (top + unsnapped - desc + (height - unsnapped) / 2.0).max(top + flush)
        };

        // Alignment / justification (trailing spaces hang).
        let mut content_end = xs.last().copied().unwrap_or(line_start_x);
        for k in (c0..c1).rev() {
            if fit.contains(k) {
                break;
            }
            match pl.clusters.get(k).map(|c| c.kind) {
                Some(ClKind::Space) | Some(ClKind::Marker) | Some(ClKind::LineBreak) | Some(ClKind::PageBreak) | Some(ClKind::ColumnBreak) => {
                    content_end = xs.get(k - c0).copied().unwrap_or(content_end);
                }
                _ => break,
            }
        }
        // A closing mark hanging past the right edge leaves the text before it to reach the edge.
        let hung = (c0..c1).rev().find(|k| !matches!(pl.clusters.get(*k).map(|c| c.kind), Some(ClKind::Space | ClKind::Marker)));
        if let Some(k) = hung.filter(|k| marks.hang.get(*k).copied().unwrap_or(false) && !has_tab)
            && let Some(start) = xs.get(k - c0).copied()
            && content_end > right_edge + 0.01
        {
            content_end = start.min(content_end);
        }
        if !has_tab {
            content_end -= crate::compression::apply(&marks.squeeze, c0..c1, &mut xs, content_end - right_edge);
            line_gaps.retain(|(k, _)| *k < c1);
            content_end -= crate::compression::apply_gaps(&line_gaps, c0, &mut xs, content_end - right_edge);
        }
        let slack = right_edge - content_end;
        let last_tab = (c0..c1).rev().find(|k| pl.clusters.get(*k).is_some_and(|c| c.kind == ClKind::Tab));
        let align_from = last_tab.map(|k| k + 1).unwrap_or(c0);
        let display = pl.displays.iter().find(|(ci, _)| *ci >= c0 && *ci < c1).map(|d| d.1);
        let shift = match (display, rp.align) {
            (Some(MathJc::Left), _) => 0.0,
            (Some(MathJc::Right), _) => slack,
            (Some(_), _) => slack / 2.0,
            (None, Align::Center) => slack / 2.0,
            (None, Align::Right) => slack,
            _ => 0.0,
        };
        if shift > 0.0 && slack > 0.0 {
            for v in xs.iter_mut().skip(align_from - c0) {
                *v += shift;
            }
        }
        let justify = display.is_none() && ((rp.align == Align::Justify && end == LineEnd::Wrap) || rp.align == Align::Distribute);
        // A line that only fits with shrunk spaces is shrunk, the last line of the paragraph too.
        let shrink = display.is_none() && shrink_spaces && slack < 0.0 && last_tab.is_none();
        if shrink {
            shrink_line_spaces(pl, &mut xs, c0, c1, content_end, slack, fit);
        } else if justify && slack > 0.0 {
            // Spaces inside the content (after the last tab).
            let spaces: Vec<usize> = (align_from..c1)
                .filter(|k| {
                    !fit.contains(*k)
                        && pl.clusters.get(*k).is_some_and(|c| c.kind == ClKind::Space)
                        && xs.get(k - c0).copied().unwrap_or(f32::MAX) < content_end - 0.01
                })
                .collect();
            if rp.align == Align::Distribute || spaces.is_empty() {
                if rp.align == Align::Distribute {
                    if (align_from..c1).any(|k| fit.contains(k)) {
                        let boundaries: Vec<usize> = (align_from..c1.saturating_sub(1)).filter(|k| !fit.internal_after(*k)).collect();
                        if !boundaries.is_empty() {
                            let per = slack / boundaries.len() as f32;
                            let mut count = 0;
                            for (q, value) in xs.iter_mut().enumerate().skip(align_from - c0) {
                                let k = q + c0;
                                while boundaries.get(count).is_some_and(|boundary| *boundary < k) {
                                    count += 1;
                                }
                                *value += per * count as f32;
                            }
                        }
                    } else {
                        let cnt = (c1 - align_from).saturating_sub(1).max(1) as f32;
                        let per = slack / cnt;
                        for (q, v) in xs.iter_mut().enumerate().skip(align_from - c0) {
                            let k = (q + c0 - align_from) as f32;
                            *v += per * k.min(cnt);
                        }
                    }
                }
            } else {
                let per = slack / spaces.len() as f32;
                let mut add = 0.0;
                let mut si = 0;
                for k in align_from..c1 {
                    if let Some(v) = xs.get_mut(k - c0) {
                        *v += add;
                    }
                    if spaces.get(si) == Some(&k) {
                        add += per;
                        si += 1;
                    }
                }
                if let Some(v) = xs.last_mut() {
                    *v += add;
                }
            }
        }
        let start = pl.clusters.get(c0).map(|c| c.start).unwrap_or(pl.text_len);
        let stop = if c1 > c0 { pl.clusters.get(c1 - 1).map(|c| c.end).unwrap_or(pl.text_len) } else { start };
        if first_x0.is_none() {
            first_x0 = xs.first().copied();
        }
        let mut line = Line {
            top,
            height,
            baseline,
            descent: desc,
            c0,
            c1,
            xs,
            leaders,
            end,
            start,
            stop,
            left,
            right: right_edge,
            hyphen,
            beside,
            vis: Vec::new(),
            rtl: rtl_para,
        };
        if bidi {
            reorder_line(pl, &mut line, width);
        }
        crate::two_lines::position(pl, &mut line);
        lines.push(line);
        first = false;
        i = j;
        // Text that wrapped continues on the row's far side; anything else ends the row.
        if end != LineEnd::Wrap || i >= n {
            row_rest.clear();
        }
        if row_rest.is_empty() {
            // The row is as tall as its tallest line.
            let row = lines.get_mut(row_first..).unwrap_or_default();
            let h = row.iter().map(|l| l.height).fold(0.0f32, f32::max);
            for l in row {
                l.baseline += h - l.height;
                l.height = h;
            }
            top += h;
        }
        if i >= n {
            // A paragraph ending with a line break gets an empty last line; one ending with a
            // page break keeps its mark on the break's line, as WPS (and Word 2013+) lay it out.
            if matches!(end, LineEnd::LineBreak | LineEnd::ColumnBreak) {
                let (asc, desc) = pl.styles.get(mark_style as usize).map(|s| (s.ascent, s.descent)).unwrap_or((10.0, 3.0));
                let natural = asc + desc;
                let (spaced, unsnapped) = line_spacing_heights(natural, &rp, env.grid_pitch);
                let h = grid_line_height(spaced, &rp, env.grid_pitch).max(grid_multiple_floor(&rp, env.grid_pitch));
                let x0 = rp.indent_left;
                let (x0, left, right) = if rtl_para { (width - x0, width - base_right, width - x0) } else { (x0, x0, base_right) };
                lines.push(Line {
                    top,
                    height: h,
                    baseline: top + unsnapped - desc + (h - unsnapped) / 2.0,
                    descent: desc,
                    c0: n,
                    c1: n,
                    xs: vec![x0],
                    leaders: Vec::new(),
                    end: LineEnd::Para,
                    start: pl.text_len,
                    stop: pl.text_len,
                    left,
                    right,
                    hyphen: None,
                    beside: false,
                    vis: Vec::new(),
                    rtl: rtl_para,
                });
                top += h;
            }
            break;
        }
        if lines.len() > 200_000 {
            break;
        }
    }
    // Hang the drop cap from the first line's caps down to line N's baseline, left of the text.
    if let Some((nc, dl, dw)) = pl.drop_cap
        && let (Some(l0), Some(ln)) = (lines.first(), lines.get((dl as usize).min(lines.len()).saturating_sub(1)))
    {
        let short = lines.len() < dl as usize;
        let dy = if short { (dl as f32 - 1.0) * l0.height } else { ln.baseline - l0.baseline };
        let dx = dw.min(first_x0.unwrap_or(0.0) - first_left.max(rp.indent_left));
        // Toward the start edge: left, or right (and from the letter's right edge) when mirrored.
        let dx = if rtl_para { pl.drop_cap_w - dx } else { dx };
        if short {
            top = top.max(l0.top + l0.height * dl as f32);
        }
        for c in pl.clusters.get(..nc).unwrap_or(&[]).to_vec() {
            for g in pl.glyphs.get_mut(c.g0 as usize..c.g1 as usize).into_iter().flatten() {
                g.dy -= dy;
                g.dx -= dx;
            }
        }
    }
    if rtl_para && let Some(lab) = pl.label.as_mut() {
        lab.x = width - lab.x - lab.width;
    }
    pl.height = top;
    pl.lines = lines;
}

/// Finish a broken line of a bidirectional paragraph: resolve each cluster's level for the line
/// (UAX #9 rule L1), order the clusters visually (rule L2) and give each its box. `line.xs`
/// arrive in logical order measured from the paragraph's start edge (mirrored for a
/// right-to-left paragraph); afterwards `line.vis` holds the boxes and `xs` their left edges.
fn reorder_line(pl: &ParaLayout, line: &mut Line, width: f32) {
    let rtl_para = line.rtl;
    if rtl_para {
        let (l, r) = (line.left, line.right);
        line.left = width - r;
        line.right = width - l;
    }
    let (c0, c1) = (line.c0, line.c1);
    let n = c1.saturating_sub(c0);
    if n == 0 {
        if rtl_para {
            line.xs = line.xs.iter().map(|x| width - x).collect();
        }
        return;
    }
    let para_level = u8::from(rtl_para);
    let mut levels: Vec<u8> = (c0..c1).map(|k| pl.clusters.get(k).and_then(|c| pl.bidi_levels.get(c.start)).copied().unwrap_or(para_level)).collect();
    // L1: tabs and breaks, and the whitespace before them or at the end of the line, take the
    // paragraph level (so trailing spaces stay at the line's end edge).
    let mut trailing = true;
    for (i, k) in (c0..c1).enumerate().rev() {
        let kind = pl.clusters.get(k).map(|c| c.kind);
        let reset = match kind {
            Some(ClKind::Tab | ClKind::LineBreak) => {
                trailing = true;
                true
            }
            Some(ClKind::Space | ClKind::PageBreak | ClKind::ColumnBreak | ClKind::Marker) => trailing,
            _ => {
                trailing = false;
                false
            }
        };
        if reset && let Some(v) = levels.get_mut(i) {
            *v = para_level;
        }
    }
    if !rtl_para && levels.iter().all(|l| *l == 0) {
        return; // A left-to-right line of a paragraph with right-to-left text elsewhere.
    }
    let order = visual_order(&levels);
    let hyph = line.hyphen.map_or(0.0, |h| h.2);
    let mut w: Vec<f32> = (0..n).map(|i| line.xs.get(i + 1).zip(line.xs.get(i)).map_or(0.0, |(b, a)| b - a)).collect();
    if let Some(last) = w.last_mut() {
        *last -= hyph;
    }
    let total: f32 = w.iter().sum();
    let x0 = line.xs.first().copied().unwrap_or(0.0);
    let mut x = if rtl_para { width - x0 - total } else { x0 };
    let mut vis = vec![VisCl { x: 0.0, w: 0.0, level: para_level }; n];
    for &i in &order {
        if let (Some(v), Some(wi), Some(l)) = (vis.get_mut(i), w.get(i), levels.get(i)) {
            *v = VisCl { x, w: *wi, level: *l };
            x += *wi;
        }
    }
    line.vis = vis;
    let end = line.end_x();
    line.xs = line.vis.iter().map(|v| v.x).chain(std::iter::once(end)).collect();
}

/// Narrowest span text flows into beside a floating object, points. Word puts an empty
/// paragraph beside a floating table in a 21.3pt gap but not in an 18.6pt one.
const MIN_SPAN: f32 = 20.0;

/// The spans of a row at `top` (about `h` tall) between `lo` and `hi` that text can use around
/// the exclusions, left to right (Square wrapping uses both sides of an object). `Err(y)`: none
/// here, try again at `y` (below what's in the way).
pub(crate) fn row_spans(exclusions: &[Exclusion], top: f32, h: f32, pad: f32, lo: f32, hi: f32) -> Result<VecDeque<(f32, f32)>, f32> {
    // An outline meets the row's text: `pad` in from its top and bottom.
    let band = |e: &Exclusion| if e.clear == Clear::Outline { pad.clamp(0.0, h / 2.0) } else { 0.0 };
    let here: Vec<&Exclusion> =
        exclusions.iter().filter(|e| e.bottom > top + band(e) && e.top < top + h - band(e) && e.right > lo && e.left < hi).collect();
    if here.is_empty() {
        return Ok(VecDeque::from([(lo, hi)]));
    }
    if let Some(below) = here.iter().filter(|e| e.clear == Clear::Below).map(|e| e.bottom).reduce(f32::max) {
        return Err(below);
    }
    let mut spans = vec![(lo, hi)];
    for e in &here {
        spans = spans
            .into_iter()
            .flat_map(|(a, b)| if e.right <= a || e.left >= b { vec![(a, b)] } else { vec![(a, e.left.min(b)), (e.right.max(a), b)] })
            .collect();
    }
    spans.retain(|(a, b)| b - a >= MIN_SPAN);
    if spans.is_empty() {
        // Nothing wide enough: the line goes below the first object that ends (its text below an
        // outline's).
        return Err(here.iter().map(|e| e.bottom - band(e)).reduce(f32::min).unwrap_or(top));
    }
    Ok(spans.into())
}

/// Word's default hyphenation zone (0.25"), points.
const HYPHENATION_ZONE: f32 = 18.0;

/// How much of their width the spaces of a justified line may give up to fit more text: a tenth
/// (WPS for Mac, synthetic references in compatibility mode 15: lines of 7 and 14 spaces take
/// back 10 % of them but not 11 %).
const MAX_SPACE_SHRINK: f32 = 0.1;

/// Take `-slack` (an overflow) out of the line's inner spaces, in proportion to their widths and
/// never more than [`MAX_SPACE_SHRINK`] of them. Trailing spaces hang and are left alone.
fn shrink_line_spaces(pl: &ParaLayout, xs: &mut [f32], c0: usize, c1: usize, content_end: f32, slack: f32, fit: &crate::inline_groups::Groups) {
    let spaces: Vec<(usize, f32)> = (c0..c1)
        .filter_map(|k| {
            if fit.contains(k) {
                return None;
            }
            let c = pl.clusters.get(k).filter(|c| c.kind == ClKind::Space && pl.firm_spaces.binary_search(&(k as u32)).is_err())?;
            (xs.get(k - c0).copied().unwrap_or(f32::MAX) < content_end - 0.01).then_some((k, c.adv))
        })
        .collect();
    let total: f32 = spaces.iter().map(|s| s.1).sum();
    if total <= 0.0 || !slack.is_finite() {
        return;
    }
    let ratio = (-slack / total).clamp(0.0, MAX_SPACE_SHRINK);
    let mut take = 0.0;
    let mut next = spaces.iter().peekable();
    for k in c0..c1 {
        if let Some(v) = xs.get_mut(k - c0) {
            *v -= take;
        }
        if let Some((_, adv)) = next.next_if(|s| s.0 == k) {
            take += adv * ratio;
        }
    }
    if let Some(v) = xs.last_mut() {
        *v -= take;
    }
}

/// The hyphen glyph and advance in a style (cached per paragraph).
fn hyphen_glyph(pl: &ParaLayout, style: u16, cache: &mut Vec<(u16, u32, f32)>) -> (u16, u32, f32) {
    if let Some(h) = cache.iter().find(|h| h.0 == style) {
        return *h;
    }
    let h = pl
        .styles
        .get(style as usize)
        .and_then(|st| {
            let g = kition_doc_fonts::shape(&st.face, "-", &[], |c| c).into_iter().next()?;
            let k = st.size / st.face.upem.max(1.0) as f32 * st.rc.scale / 100.0;
            Some((style, g.gid, g.x_advance as f32 * k))
        })
        .unwrap_or((style, 0, 0.0));
    cache.push(h);
    h
}

/// Clusters after which a line may end with a hyphen: the soft hyphens, found in `text`, the
/// paragraph as laid out (without what the layout leaves out, [`left_out`]); `to_para` maps the
/// end of a char there to the paragraph's text.
fn hyphenation_points(text: &str, to_para: impl Fn(usize) -> Option<usize>, pl: &ParaLayout) -> Vec<u32> {
    // Only soft hyphens: WPS for Mac has no automatic hyphenation, so the document setting
    // (`Settings::auto_hyphenation`, kept and saved) doesn't break words.
    let bytes: Vec<usize> = text.char_indices().filter(|(_, c)| *c == SOFT_HYPHEN).map(|(i, c)| i + c.len_utf8()).collect();
    let mut bytes: Vec<usize> = bytes.into_iter().filter_map(to_para).collect();
    if bytes.is_empty() {
        return Vec::new();
    }
    bytes.sort_unstable();
    let mut out: Vec<u32> = pl
        .clusters
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c.kind, ClKind::Text | ClKind::Marker) && bytes.binary_search(&c.end).is_ok())
        .map(|(i, _)| i as u32)
        .collect();
    out.dedup();
    out
}

fn pl_text_at(c: &Cluster) -> bool {
    c.dot
}

fn resolve_tab(pl: &mut ParaLayout, pending: &mut Option<(usize, TabStop, f32)>, xs: &mut [f32], c0: usize, x: f32, xo: &mut f32) {
    // Finish a centre/right/decimal tab: its text (possibly empty) ends at `x`.
    let Some((tj, stop, tx)) = pending.take() else { return };
    let seg = x - tx - pl.clusters.get(tj).map(|c| c.adv).unwrap_or(0.0);
    let old = pl.clusters.get(tj).map(|c| c.adv).unwrap_or(0.0);
    let want = match stop.align {
        TabAlign::Right => stop.pos - tx - seg,
        TabAlign::Decimal => {
            let end = c0.saturating_add(xs.len()).min(pl.clusters.len());
            let point = (tj.saturating_add(1)..end).find(|&index| pl.clusters.get(index).is_some_and(|c| c.dot));
            point.and_then(|index| xs.get(index.saturating_sub(c0))).map_or(stop.pos - tx - seg, |point| old + stop.pos - point)
        }
        TabAlign::Center => stop.pos - tx - seg / 2.0,
        _ => return,
    }
    .max(0.0);
    let delta = want - old;
    if delta.abs() > 0.001 {
        if let Some(c) = pl.clusters.get_mut(tj) {
            c.adv = want;
        }
        for v in xs.iter_mut().skip(tj + 1 - c0) {
            *v += delta;
        }
        *xo += delta;
    }
}

impl ParaLayout {
    /// x of byte offset `off` within line `li` (clamped to the line).
    pub fn x_of(&self, li: usize, off: usize) -> Option<f32> {
        let l = self.lines.get(li)?;
        if !l.vis.is_empty() {
            return self.x_of_bidi(l, off);
        }
        for k in l.c0..l.c1 {
            let c = self.clusters.get(k)?;
            if off <= c.start {
                return l.xs.get(k - l.c0).copied();
            }
            if off < c.end {
                return l.xs.get(k - l.c0).copied();
            }
        }
        l.xs.last().copied()
    }
    /// The line holding offset `off` (a position at a wrap point belongs to the next line).
    pub fn line_of(&self, off: usize) -> usize {
        // Several displayed field pieces may share one object range across lines.
        // Its leading caret belongs to the first piece, its trailing caret to the
        // next source cluster (or the paragraph's last line at its end).
        let cluster = self.clusters.partition_point(|c| c.end <= off);
        self.lines.iter().position(|line| (line.c0..line.c1).contains(&cluster)).unwrap_or_else(|| self.lines.len().saturating_sub(1))
    }
    /// Caret x on a bidirectional line: the leading edge of the cluster after `off` (the right
    /// edge of right-to-left text), except where `off` leaves an embedded run (the cluster before
    /// it has the higher level), which keeps the caret at that run's trailing edge, so each
    /// visual boundary of the common mixed cases has its own offset.
    fn x_of_bidi(&self, l: &Line, off: usize) -> Option<f32> {
        let k = (l.c0..l.c1).find(|&k| self.clusters.get(k).is_some_and(|c| off < c.end || off <= c.start));
        let Some(k) = k else { return Some(crate::two_lines::end_caret(self, l, off).unwrap_or_else(|| l.end_x())) };
        let c = self.clusters.get(k)?;
        let here = l.vis.get(k - l.c0)?;
        if off > c.start || k == l.c0 {
            return Some(here.leading());
        }
        let prev = l.vis.get(k - 1 - l.c0)?;
        Some(if prev.level > here.level { prev.trailing() } else { here.leading() })
    }

    /// Visual extents, left to right, of the text between byte offsets `from..to` on line `li`
    /// (one span unless bidirectional text splits the range).
    pub fn x_spans(&self, li: usize, from: usize, to: usize) -> Vec<(f32, f32)> {
        let Some(l) = self.lines.get(li) else { return Vec::new() };
        if l.vis.is_empty() {
            return match (self.x_of(li, from), self.x_of(li, to)) {
                (Some(a), Some(b)) => vec![(a, b)],
                _ => Vec::new(),
            };
        }
        let ks: Vec<usize> = (l.c0..l.c1).filter(|&k| self.clusters.get(k).is_some_and(|c| c.start < to && c.end > from)).collect();
        match (ks.first(), ks.last()) {
            (Some(&a), Some(&b)) => l.spans(a, b + 1),
            _ => Vec::new(),
        }
    }

    /// Caret offsets a click or arrow key can reach on line `li`: each cluster start and the line
    /// end, except after a line-ending break or past a wrapped line's trailing space.
    pub fn line_offsets(&self, li: usize) -> Vec<usize> {
        let Some(l) = self.lines.get(li) else { return Vec::new() };
        let mut v: Vec<usize> = (l.c0..l.c1).filter_map(|k| self.clusters.get(k).map(|c| c.start)).collect();
        let last_break =
            l.c1 > l.c0 && self.clusters.get(l.c1 - 1).is_some_and(|c| matches!(c.kind, ClKind::LineBreak | ClKind::PageBreak | ClKind::ColumnBreak));
        let wrapped = l.c1 > l.c0 && l.end == LineEnd::Wrap && self.lines.get(li + 1).is_some();
        if !last_break && !wrapped || l.c1 == l.c0 {
            v.push(l.stop);
        }
        v.dedup();
        v
    }

    /// Byte offset closest to x on line `li`.
    pub fn off_at_x(&self, li: usize, x: f32) -> usize {
        let Some(l) = self.lines.get(li) else { return self.text_len };
        if !l.vis.is_empty() {
            let mut best = (l.start, f32::MAX);
            for off in self.line_offsets(li) {
                let d = self.x_of(li, off).map_or(f32::MAX, |cx| (cx - x).abs());
                if d < best.1 {
                    best = (off, d);
                }
            }
            return best.0;
        }
        let mut best = l.start;
        let mut bestd = f32::MAX;
        for k in l.c0..=l.c1 {
            let Some(cx) = l.xs.get(k - l.c0) else { break };
            let off = if k < l.c1 { self.clusters.get(k).map(|c| c.start).unwrap_or(l.stop) } else { l.stop };
            // Don't place the caret after a line-ending break or wrap-trailing space on a wrapped line.
            if k == l.c1 && k > l.c0 {
                let lastc = self.clusters.get(k - 1);
                if lastc.is_some_and(|c| matches!(c.kind, ClKind::LineBreak | ClKind::PageBreak | ClKind::ColumnBreak)) {
                    continue;
                }
                if l.end == LineEnd::Wrap && self.lines.get(li + 1).is_some() {
                    continue;
                }
            }
            let d = (cx - x).abs();
            if d < bestd {
                bestd = d;
                best = off;
            }
        }
        best
    }
    /// Line ascent/descent of the paragraph mark (for empty paragraphs and caret height).
    pub fn caret_metrics(&self, li: usize) -> (f32, f32) {
        let Some(l) = self.lines.get(li) else { return (10.0, 3.0) };
        (l.baseline - l.top, l.top + l.height - l.baseline)
    }
}

/// Spelling and grammar issues in a paragraph (skipping "do not check" and hidden runs).
fn proof_issues(p: &Paragraph) -> Vec<(usize, usize, bool)> {
    if p.text.trim().is_empty() || p.text.len() > 100_000 {
        return Vec::new();
    }
    let text = proof_text(p);
    let skip = |a: usize, b: usize| {
        p.run_ranges().any(|(r, c)| r.start < b && a < r.end && (c.no_proof == Some(true) || c.hidden == Some(true) || c.link.is_some()))
    };
    let mut v: Vec<(usize, usize, bool)> =
        kition_doc_proof::check_spelling(&text).into_iter().filter(|i| !skip(i.start, i.end)).map(|i| (i.start, i.end, false)).collect();
    v.extend(kition_doc_proof::check_grammar(&text).into_iter().filter(|i| !skip(i.start, i.end)).map(|i| (i.start, i.end, true)));
    v
}

/// Text for proofing with the same byte offsets: inline objects and tracked deletions become
/// U+0001 bytes (neither words nor spaces).
pub fn proof_text(p: &kition_doc_doc::Paragraph) -> String {
    let mut out = String::with_capacity(p.text.len());
    for (i, c) in p.text.char_indices() {
        let deleted = c == kition_doc_doc::para::OBJ || p.props_of_char(i).del.is_some();
        if deleted {
            for _ in 0..c.len_utf8() {
                out.push('\u{1}');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Byte offsets of the spaces beside a Han, kana or Hangul character or another space in `text`,
/// sorted: WPS for Mac makes every space of a run of two or more half an em, between Latin
/// letters too (synthetic references in Helvetica Neue, Times New Roman and Arial).
fn ea_spaces(text: &str) -> Vec<usize> {
    let cjk = |c: char| matches!(c as u32, 0x1100..=0x11ff | 0x2e80..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0xfe30..=0xfe4f | 0xff00..=0xffef | 0x20000..=0x3ffff);
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let balanced = |c: char| c == ' ' || cjk(c);
    chars
        .iter()
        .enumerate()
        .filter(|(k, (_, c))| {
            *c == ' '
                && (k.checked_sub(1).and_then(|j| chars.get(j)).is_some_and(|(_, p)| balanced(*p))
                    || chars.get(k + 1).is_some_and(|(_, n)| balanced(*n)))
        })
        .map(|(_, (i, _))| *i)
        .collect()
}
