//! Page → draw items, shared by the raster renderer, the PDF exporter and thumbnails.

use kition_doc_doc::graphic::{Graphic, GraphicItem, GraphicKind, PathSeg, TextAlign};
use kition_doc_doc::para::{InlineObject, ShapeKind};
use kition_doc_doc::props::{Border, BorderStyle, Rgb, TabLeader, TextColor, TextDirection, Underline};
use kition_doc_doc::section::SectionStart;
use kition_doc_doc::{Block, Document, Path, StoryRef};
use kition_doc_fonts::{BezPath, FaceRef};
use kition_doc_geom::{Rect, Spin};

use crate::math::MItem;
use crate::para::{ClKind, LineEnd, ParaLayout};
use crate::{Page, Placed};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stroke {
    Solid,
    Dotted,
    Dashed,
    Double,
    Wave,
}

#[derive(Clone, Debug)]
pub enum Draw {
    Glyphs {
        face: FaceRef,
        size: f32,
        /// Width of each glyph outline relative to its normal width; origins are already placed.
        horizontal_scale: f32,
        /// Glyph id, x, baseline y (page coordinates).
        glyphs: Vec<(u32, f32, f32)>,
        color: Rgb,
        alpha: f32,
        synth_bold: bool,
        synth_italic: bool,
        /// The text these glyphs show (for PDF text extraction), and the hyperlink.
        text: String,
        link: Option<String>,
        /// Byte range of `text` each glyph shows (one per glyph; a ligature's range covers all
        /// its characters). Empty when unknown: the PDF writer then guesses from the font.
        ranges: Vec<std::ops::Range<usize>>,
    },
    Fill {
        rect: Rect,
        color: Rgb,
        alpha: f32,
    },
    Line {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        width: f32,
        color: Rgb,
        stroke: Stroke,
        alpha: f32,
    },
    Image {
        rect: Rect,
        media: String,
        crop: [f32; 4],
        alpha: f32,
        /// Colour adjustments of its pixels (its opacity is in `alpha`).
        adjust: kition_doc_doc::graphic::ImageAdjust,
        /// The preset outline it is cut to (`None`: its rectangle).
        geom: Option<kition_doc_doc::graphic::PresetGeom>,
        /// Its line and effects, drawn around and behind it.
        edge: kition_doc_doc::graphic::PictureEdge,
    },
    Shape {
        rect: Rect,
        kind: ShapeKind,
        fill: Option<Rgb>,
        stroke: Option<Rgb>,
        stroke_width: f32,
        /// Shadow, glow and soft edges (drawn with [`kition_doc_doc::effects::bands`]).
        effects: kition_doc_doc::effects::ShapeEffects,
        /// The preset outline when the kind does not name it ([`kition_doc_doc::outline`]).
        geom: Option<kition_doc_doc::graphic::PresetGeom>,
        /// A gradient fill over the outline (`fill` its first colour, for what draws one).
        gradient: Option<std::sync::Arc<kition_doc_doc::graphic::ShapeGradient>>,
        /// The outline's dash (`None`: solid).
        dash: Option<kition_doc_doc::graphic::LineDash>,
    },
    /// An ink stroke: a line through `pts` (page points) with round ends and joins, `width`
    /// wide, in `color` at opacity `alpha`.
    Ink {
        pts: Vec<(f32, f32)>,
        color: Rgb,
        width: f32,
        alpha: f32,
    },
    /// A vector path (a chart's or diagram's polygons, slices, lines): filled, then stroked.
    Path {
        segs: Vec<PathSeg>,
        fill: Option<Rgb>,
        stroke: Option<Rgb>,
        stroke_width: f32,
        stroke_alpha: f32,
    },
    /// An inline chart or diagram's draws, as one figure with its alt text (for tagged PDF).
    Figure {
        alt: String,
        kind: GraphicKind,
        draws: Vec<Draw>,
    },
    /// A formatting mark (¶ · → ↵) in the UI's mark colour, or in `color` (a tracked
    /// paragraph mark in its reviser's colour).
    Mark {
        x: f32,
        baseline: f32,
        size: f32,
        ch: char,
        color: Option<Rgb>,
    },
    /// Turned text (a table cell's text direction): `items` are drawn in a frame turned `turn`
    /// whose origin is page point (`x`, `y`) (see [`crate::turn_point`]).
    Turned {
        x: f32,
        y: f32,
        turn: TextDirection,
        items: Vec<Draw>,
    },
    /// `items` drawn clipped to page rectangle `rect`: a text box's text, cut off at the box's
    /// edge where a line runs past it (as WPS draws it).
    Clipped {
        rect: Rect,
        items: Vec<Draw>,
    },
    /// A rotated or flipped picture, shape or chart: `items` drawn turned by `spin` about page
    /// point (`cx`, `cy`) (see [`Spin::matrix`]).
    Rotated {
        cx: f32,
        cy: f32,
        spin: Spin,
        items: Vec<Draw>,
    },
    /// A formatting-mark label (a section break's name) in the UI's mark colour, left edge at
    /// `x`, drawn with the same face as [`Draw::Mark`]. Screen only, like every mark.
    MarkText {
        x: f32,
        baseline: f32,
        size: f32,
        text: String,
    },
}

/// Bound a display-list glyph width independently of its already-positioned origins.
pub fn glyph_width_scale(scale: f32) -> f32 {
    if scale.is_finite() { scale.clamp(0.000001, 1_000_000.0) } else { 1.0 }
}

impl Draw {
    /// The affine map (a, b, c, d, e, f: x' = a·x + c·y + e, y' = b·x + d·y + f) from a frame
    /// turned `turn` with its origin at page point (`x`, `y`) to the page.
    pub fn turn_matrix(turn: TextDirection, x: f32, y: f32) -> [f32; 6] {
        match turn {
            TextDirection::Horizontal => [1.0, 0.0, 0.0, 1.0, x, y],
            TextDirection::Down => [0.0, 1.0, -1.0, 0.0, x, y],
            TextDirection::Up => [0.0, -1.0, 1.0, 0.0, x, y],
        }
    }
}

#[derive(Clone, Debug)]
pub struct DisplayOptions {
    /// Show formatting marks (¶).
    pub marks: bool,
    /// Dim headers/footers (editing the body) or the body (editing a header/footer).
    pub dim_header: bool,
    pub dim_body: bool,
    /// Show tracked changes as markup (coloured, underlined/struck).
    pub markup: bool,
    /// Show on-screen-only marks: equation placeholders and prompts (never in print or PDF).
    pub placeholders: bool,
    /// Review › Hide Ink: leave ink strokes out (they stay in the document).
    pub hide_ink: bool,
    /// Draw the spelling and grammar squiggles the layout found (the editor's screen only,
    /// never in print).
    pub proofing: bool,
    /// How tracked changes and comments are marked when `markup` is on (Track Changes Options).
    pub revisions: MarkupOptions,
}

impl Default for DisplayOptions {
    /// The page as printed: nothing dimmed, no on-screen marks.
    fn default() -> Self {
        DisplayOptions {
            marks: false,
            dim_header: false,
            dim_body: false,
            markup: true,
            placeholders: false,
            hide_ink: false,
            proofing: false,
            revisions: MarkupOptions::default(),
        }
    }
}

/// An option list with stable names for commands and saved preferences.
macro_rules! named {
    ($(#[$m:meta])* $name:ident { $($(#[$vm:meta])* $v:ident = $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub enum $name { $($(#[$vm])* $v),+ }
        impl $name {
            /// Every choice with its name, in menu order.
            pub const ALL: &'static [($name, &'static str)] = &[$(($name::$v, $s)),+];
            pub fn name(self) -> &'static str {
                Self::ALL.iter().find(|(v, _)| *v == self).map(|(_, s)| *s).unwrap_or("")
            }
            pub fn parse(s: &str) -> Option<Self> {
                Self::ALL.iter().find(|(_, n)| *n == s).map(|(v, _)| *v)
            }
        }
    };
}

named! {
    /// How inserted text is marked.
    InsertMark {
        #[default]
        Underline = "underline",
        DoubleUnderline = "doubleUnderline",
        Bold = "bold",
        Italic = "italic",
        Strikethrough = "strikethrough",
        ColorOnly = "colorOnly",
        None = "none",
    }
}

named! {
    /// How deleted text is marked.
    DeleteMark {
        #[default]
        Strikethrough = "strikethrough",
        DoubleStrikethrough = "doubleStrikethrough",
        Hidden = "hidden",
        Caret = "caret",
        Hash = "hash",
        Underline = "underline",
        ColorOnly = "colorOnly",
    }
}

named! {
    /// Where the bar beside changed lines goes.
    ChangeBar {
        #[default]
        Outside = "outside",
        Left = "left",
        Right = "right",
        None = "none",
    }
}

named! {
    /// What the markup area beside the page shows.
    BalloonMode {
        /// Deletions, formatting and comments in balloons (drawn like `CommentsAndFormatting`
        /// for now: revisions stay inline).
        Revisions = "revisions",
        /// Everything inline, no markup area.
        Inline = "inline",
        #[default]
        CommentsAndFormatting = "commentsAndFormatting",
    }
}

/// Track Changes Options: what markup shows and how revisions are drawn. A per-user preference,
/// as in Word (saved with the interface settings, not the document).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MarkupOptions {
    pub comments: bool,
    pub ink: bool,
    pub insertions_deletions: bool,
    pub formatting: bool,
    pub balloons: BalloonMode,
    pub insert_mark: InsertMark,
    /// `None` = by author.
    pub insert_color: Option<Rgb>,
    pub delete_mark: DeleteMark,
    /// `None` = by author.
    pub delete_color: Option<Rgb>,
    pub changed_lines: ChangeBar,
    /// `None` = automatic (dark grey).
    pub changed_lines_color: Option<Rgb>,
    /// Record formatting changes while tracking.
    pub track_formatting: bool,
}

impl Default for MarkupOptions {
    fn default() -> Self {
        MarkupOptions {
            comments: true,
            ink: true,
            insertions_deletions: true,
            formatting: true,
            balloons: BalloonMode::default(),
            insert_mark: InsertMark::default(),
            insert_color: None,
            delete_mark: DeleteMark::default(),
            delete_color: None,
            changed_lines: ChangeBar::default(),
            changed_lines_color: None,
            track_formatting: true,
        }
    }
}

impl MarkupOptions {
    /// Tracked deletions take no room: they are hidden, or insertions and deletions aren't shown.
    pub fn hides_deletions(&self) -> bool {
        !self.insertions_deletions || self.delete_mark == DeleteMark::Hidden
    }
}

/// Colour for revisions by author index.
pub fn revision_color(i: u32) -> Rgb {
    const C: [Rgb; 6] =
        [Rgb(0xB0, 0x1E, 0x8F), Rgb(0x1E, 0x6E, 0xB0), Rgb(0x2E, 0x8B, 0x3E), Rgb(0xC0, 0x5A, 0x10), Rgb(0x70, 0x3C, 0xB0), Rgb(0x0E, 0x7C, 0x86)];
    C.get(i as usize % C.len()).copied().unwrap_or(C[0])
}

fn stroke_of(u: Underline) -> Stroke {
    match u {
        Underline::Dotted => Stroke::Dotted,
        Underline::Dash | Underline::DotDash | Underline::DotDotDash => Stroke::Dashed,
        Underline::Double => Stroke::Double,
        Underline::Wave | Underline::DoubleWave => Stroke::Wave,
        _ => Stroke::Solid,
    }
}

/// Draw items for a page.
pub fn page_display(doc: &Document, page: &Page, opts: &DisplayOptions) -> Vec<Draw> {
    let mut out = Vec::new();
    let ha = if opts.dim_header { 0.45 } else { 1.0 };
    let ba = if opts.dim_body { 0.45 } else { 1.0 };
    placed(doc, &page.header, opts, ha, &mut out);
    placed(doc, &page.footer, opts, ha, &mut out);
    placed(doc, &page.items, opts, ba, &mut out);
    if opts.markup && (opts.revisions.insertions_deletions || opts.revisions.formatting) && opts.revisions.changed_lines != ChangeBar::None {
        change_bars(doc, page, &opts.revisions, ba, &mut out);
    }
    out
}

/// The automatic colour of the bars beside changed lines.
pub const CHANGE_BAR: Rgb = Rgb(0x50, 0x50, 0x50);

/// Bars in the margin beside body lines with tracked insertions or deletions, or (when shown)
/// tracked formatting changes of their text, paragraph or paragraph mark.
fn change_bars(doc: &Document, page: &Page, m: &MarkupOptions, alpha: f32, out: &mut Vec<Draw>) {
    // Outside: the left margin, or the outer one of a right-hand page with mirrored margins.
    let right = match m.changed_lines {
        ChangeBar::Right => true,
        ChangeBar::Outside => doc.settings.mirror_margins && page.number % 2 == 1,
        _ => false,
    };
    let bx = if right { page.body.right() + 9.0 } else { page.body.x - 9.0 };
    let color = m.changed_lines_color.unwrap_or(CHANGE_BAR);
    for it in &page.items {
        let Placed::Lines { story, path, para: pl, l0, l1, y, turn, .. } = it else { continue };
        if turn.is_turned() {
            continue;
        }
        let Some(first) = pl.lines.get(*l0) else { continue };
        let (text, fmt) = (m.insertions_deletions, m.formatting);
        let para = doc.para(*story, path);
        let mark_rev = para.is_some_and(|p| (text && (p.mark.ins.is_some() || p.mark.del.is_some())) || (fmt && p.mark.fmt_change.is_some()));
        // A paragraph whose own formatting changed is marked on every line.
        let para_fmt = fmt && para.is_some_and(|p| p.props.fmt_change.is_some());
        for li in *l0..*l1 {
            let Some(line) = pl.lines.get(li) else { continue };
            let changed = para_fmt
                || (line.c0..line.c1)
                    .filter_map(|k| pl.clusters.get(k).and_then(|c| pl.styles.get(c.style as usize)))
                    .any(|st| (text && (st.rc.ins.is_some() || st.rc.del.is_some())) || (fmt && st.rc.fmt.is_some()))
                || (mark_rev && line.end == LineEnd::Para);
            if changed {
                let top = y + (line.top - first.top);
                out.push(Draw::Line { x0: bx, y0: top, x1: bx, y1: top + line.height, width: 0.75, color, stroke: Stroke::Solid, alpha });
            }
        }
    }
}

fn border_stroke(s: BorderStyle) -> Stroke {
    match s {
        BorderStyle::Dotted => Stroke::Dotted,
        BorderStyle::Dashed | BorderStyle::DotDash => Stroke::Dashed,
        BorderStyle::Double | BorderStyle::Triple => Stroke::Double,
        BorderStyle::Wave | BorderStyle::DoubleWave => Stroke::Wave,
        _ => Stroke::Solid,
    }
}

/// A border line from `(x0, y0)` to `(x1, y1)` (paragraph rules and character borders).
fn rule(x0: f32, y0: f32, x1: f32, y1: f32, b: &Border, alpha: f32) -> Draw {
    Draw::Line {
        x0,
        y0,
        x1,
        y1,
        width: if b.style == BorderStyle::Thick { b.width.max(1.5) } else { b.width.max(0.25) },
        color: b.color.unwrap_or(Rgb::BLACK),
        stroke: border_stroke(b.style),
        alpha,
    }
}

/// Closed box around one line segment of a character-border group, widened by `space`.
// Note: border drawn outside text advance, reserve width in line breaking if overlap shows.
fn char_box(b: &Border, x0: f32, x1: f32, top: f32, bottom: f32, alpha: f32, out: &mut Vec<Draw>) {
    let (l, r) = (x0 - b.space, x1 + b.space);
    out.push(rule(l, top, r, top, b, alpha));
    out.push(rule(l, bottom, r, bottom, b, alpha));
    out.push(rule(l, top, l, bottom, b, alpha));
    out.push(rule(r, top, r, bottom, b, alpha));
}

/// The draws of placed items in order, art text bent by its warp.
fn placed(doc: &Document, items: &[Placed], opts: &DisplayOptions, alpha: f32, out: &mut Vec<Draw>) {
    // Shading drawn so far (cell and paragraph shading come before their text): what automatic
    // text colour is drawn on.
    let mut fills: Vec<(Rect, Rgb)> = Vec::new();
    let background = |fills: &[(Rect, Rgb)], it: &Placed| match it {
        Placed::Lines { para, l0, x, y, turn, .. } => {
            // Just inside the start of its first line (a turned paragraph's frame corner).
            let at = if turn.is_turned() {
                kition_doc_geom::Point { x: *x, y: *y }
            } else {
                kition_doc_geom::Point { x: *x + 0.5, y: *y + para.lines.get(*l0).map_or(0.0, |line| line.height / 2.0) }
            };
            fills.iter().rev().find(|(rect, _)| rect.contains(at)).map(|(_, color)| *color)
        }
        _ => None,
    };
    let mut i = 0;
    while let Some(it) = items.get(i) {
        i += 1;
        if let Placed::Fill { rect, color } = it {
            fills.push((*rect, *color));
        }
        if let Placed::Warp { rect, warp, items: n, spin } = it {
            let end = (i + n).min(items.len());
            let mut flat = Vec::new();
            for it in items.get(i..end).unwrap_or(&[]) {
                item(doc, it, opts, alpha, background(&fills, it), &mut flat);
            }
            spun(*spin, *rect, warp_draws(&flat, *rect, &warp.name, &warp.adj), out);
            i = end;
            continue;
        }
        if let Placed::Flat { items: n, .. } = it {
            let end = (i + n).min(items.len());
            let mut drawn = Vec::new();
            placed(doc, items.get(i..end).unwrap_or(&[]), opts, alpha, &mut drawn);
            out.extend(inked(drawn));
            i = end;
            continue;
        }
        if let Placed::Clip { rect, items: n } = it {
            let end = (i + n).min(items.len());
            let mut drawn = Vec::new();
            placed(doc, items.get(i..end).unwrap_or(&[]), opts, alpha, &mut drawn);
            out.push(Draw::Clipped { rect: *rect, items: drawn });
            i = end;
            continue;
        }
        item(doc, it, opts, alpha, background(&fills, it), out);
    }
}

/// Art text (KitDrawing 1.3 DR26): the glyphs of `flat` (a text box's text as laid out) as
/// outlines filling `rect`, bent by `preset` at its `adjust` values with the warp geometry Kition's kits share.
/// Underlines, highlights and other marks are left out.
pub fn warp_draws(flat: &[Draw], rect: Rect, preset: &str, adjust: &[(String, f64)]) -> Vec<Draw> {
    // Each run's letters with how they are painted: filled in the text's colour, or (outlined
    // letters, already drawn as paths) with their own fill and outline.
    let runs: Vec<(Option<Rgb>, Option<(Rgb, f32)>, Vec<BezPath>)> = flat
        .iter()
        .filter_map(|d| match d {
            Draw::Path { segs, fill, stroke, stroke_width, .. } => Some((*fill, stroke.map(|s| (s, *stroke_width)), vec![bez_path(segs)])),
            _ => glyph_outlines(d).map(|(color, paths)| (Some(color), None, paths)),
        })
        .collect();
    let ink = runs.iter().flat_map(|(_, _, ps)| ps).map(kurbo::Shape::bounding_box).reduce(|a, b| a.union(b));
    let Some(ink) = ink.filter(|b| b.width() > 0.0 && b.height() > 0.0) else { return Vec::new() };
    let (w, h) = (f64::from(rect.w).max(1.0), f64::from(rect.h).max(1.0));
    // The ink fills the box, then bends with it.
    let fit = kurbo::Affine::scale_non_uniform(w / ink.width(), h / ink.height()) * kurbo::Affine::translate(-ink.origin().to_vec2());
    let back = kurbo::Affine::translate((f64::from(rect.x), f64::from(rect.y)));
    runs.into_iter()
        .filter(|(_, _, ps)| !ps.is_empty())
        .map(|(fill, stroke, paths)| {
            let mut segs = Vec::new();
            for p in paths {
                // A bent path comes back flattened; the plain warp keeps the letters' curves.
                path_segs(&(back * kition_office_core::warp::warp_path_adj(preset, adjust, w, h, &(fit * &p))), &mut segs);
            }
            Draw::Path { segs, fill, stroke: stroke.map(|s| s.0), stroke_width: stroke.map_or(0.0, |s| s.1), stroke_alpha: 1.0 }
        })
        .collect()
}

/// [`PathSeg`]s as a path.
fn bez_path(segs: &[PathSeg]) -> BezPath {
    let mut p = BezPath::new();
    let pt = |x: f32, y: f32| kurbo::Point::new(f64::from(x), f64::from(y));
    for s in segs {
        match *s {
            PathSeg::Move(x, y) => p.move_to(pt(x, y)),
            PathSeg::Line(x, y) => p.line_to(pt(x, y)),
            PathSeg::Cubic(a, b, c, d, x, y) => p.curve_to(pt(a, b), pt(c, d), pt(x, y)),
            PathSeg::Close => p.close_path(),
        }
    }
    p
}

/// The outlines of a glyph run's glyphs where they are placed, in page points, with its colour.
/// `None` for a draw that isn't glyphs.
fn glyph_outlines(d: &Draw) -> Option<(Rgb, Vec<BezPath>)> {
    let Draw::Glyphs { face, size, horizontal_scale, glyphs, color, synth_italic, .. } = d else { return None };
    let db = kition_doc_fonts::FontDb::global();
    let k = f64::from(*size) / face.upem.max(1.0);
    let stretch = kurbo::Affine::scale_non_uniform(f64::from(glyph_width_scale(*horizontal_scale)), 1.0);
    let skew = if *synth_italic { kurbo::Affine::new([1.0, 0.0, -0.21, 1.0, 0.0, 0.0]) } else { kurbo::Affine::IDENTITY };
    let paths = glyphs
        .iter()
        .filter(|(_, x, y)| x.is_finite() && y.is_finite())
        .filter_map(|&(gid, x, y)| {
            let o = db.outline(face, gid);
            (!o.elements().is_empty())
                .then(|| kurbo::Affine::translate((f64::from(x), f64::from(y))) * stretch * skew * kurbo::Affine::scale(k) * &*o)
        })
        .collect();
    Some((*color, paths))
}

/// Glyph runs as filled outlines: ink that is not text (a placeholder's prompt).
fn outlined(draws: Vec<Draw>) -> Vec<Draw> {
    draws
        .iter()
        .filter_map(glyph_outlines)
        .filter(|(_, paths)| !paths.is_empty())
        .map(|(color, paths)| {
            let mut segs = Vec::new();
            for p in paths {
                path_segs(&p, &mut segs);
            }
            Draw::Path { segs, fill: Some(color), stroke: None, stroke_width: 0.0, stroke_alpha: 1.0 }
        })
        .collect()
}

/// `draws` with their glyph runs as filled outlines, at any depth: ink that is not text (a
/// group printed as a picture). Everything else stays as it is.
fn inked(draws: Vec<Draw>) -> Vec<Draw> {
    draws
        .into_iter()
        .filter_map(|d| match d {
            Draw::Glyphs { .. } => outlined(vec![d]).pop(),
            Draw::Turned { x, y, turn, items } => Some(Draw::Turned { x, y, turn, items: inked(items) }),
            Draw::Rotated { cx, cy, spin, items } => Some(Draw::Rotated { cx, cy, spin, items: inked(items) }),
            Draw::Clipped { rect, items } => Some(Draw::Clipped { rect, items: inked(items) }),
            Draw::Figure { alt, kind, draws } => Some(Draw::Figure { alt, kind, draws: inked(draws) }),
            d => Some(d),
        })
        .collect()
}

/// A path's elements as [`PathSeg`]s, quadratic curves raised to cubics.
fn path_segs(path: &BezPath, segs: &mut Vec<PathSeg>) {
    let mut last = kurbo::Point::ZERO;
    let f = |q: kurbo::Point| (q.x as f32, q.y as f32);
    for el in path.elements() {
        match *el {
            kurbo::PathEl::MoveTo(a) => {
                segs.push(PathSeg::Move(f(a).0, f(a).1));
                last = a;
            }
            kurbo::PathEl::LineTo(a) => {
                segs.push(PathSeg::Line(f(a).0, f(a).1));
                last = a;
            }
            kurbo::PathEl::QuadTo(c, a) => {
                let (c1, c2) = (f(last + (c - last) * (2.0 / 3.0)), f(a + (c - a) * (2.0 / 3.0)));
                segs.push(PathSeg::Cubic(c1.0, c1.1, c2.0, c2.1, f(a).0, f(a).1));
                last = a;
            }
            kurbo::PathEl::CurveTo(c1, c2, a) => {
                segs.push(PathSeg::Cubic(f(c1).0, f(c1).1, f(c2).0, f(c2).1, f(a).0, f(a).1));
                last = a;
            }
            kurbo::PathEl::ClosePath => segs.push(PathSeg::Close),
        }
    }
}

/// `background`: the shading under a paragraph's lines, for their automatic text colour.
fn item(doc: &Document, it: &Placed, opts: &DisplayOptions, alpha: f32, background: Option<Rgb>, out: &mut Vec<Draw>) {
    match it {
        Placed::Fill { rect, color } => out.push(Draw::Fill { rect: *rect, color: *color, alpha }),
        Placed::Rule { x0, y0, x1, y1, border } => out.push(rule(*x0, *y0, *x1, *y1, border, alpha)),
        Placed::Image { rect, media, crop, spin, adjust, geom, edge, .. } => spun(
            *spin,
            *rect,
            vec![Draw::Image {
                rect: *rect,
                media: media.clone(),
                crop: *crop,
                alpha: alpha * adjust.opacity.clamp(0.0, 1.0),
                adjust: *adjust,
                geom: geom.clone(),
                edge: edge.for_picture(doc.media.get(media).map(|m| m.as_slice())),
            }],
            out,
        ),
        Placed::Shape { rect, kind, fill, stroke, stroke_width, effects, freeform, spin, geom, gradient, dash, .. } => {
            let mut items = Vec::new();
            shape_draws(
                *rect,
                *kind,
                geom.as_ref(),
                gradient.as_ref(),
                *fill,
                (*stroke, *dash),
                *stroke_width,
                effects_in(*effects, *spin),
                freeform.as_deref(),
                opts,
                &mut items,
            );
            spun(*spin, *rect, items, out)
        }
        Placed::Graphic { rect, graphic, spin, .. } => spun(*spin, *rect, graphic_draws(doc, graphic, *rect, alpha), out),
        // Drawn with the items it bends (`placed`).
        Placed::Cell { .. } | Placed::Object { .. } | Placed::Warp { .. } | Placed::Flat { .. } | Placed::Clip { .. } => {}
        Placed::Lines { story, path, para, l0, l1, x, y, turn } if turn.is_turned() => {
            let mut items = Vec::new();
            lines(doc, *story, path, para, *l0, *l1, 0.0, 0.0, opts, alpha, background, &mut items);
            // Link areas are axis-aligned page rectangles: turned text gets none.
            for d in &mut items {
                if let Draw::Glyphs { link, .. } = d {
                    *link = None;
                }
            }
            if *turn == TextDirection::Down {
                items = upright_east_asian(items);
            }
            out.push(Draw::Turned { x: *x, y: *y, turn: *turn, items });
        }
        Placed::Lines { story, path, para, l0, l1, x, y, .. } => lines(doc, *story, path, para, *l0, *l1, *x, *y, opts, alpha, background, out),
    }
}

fn lines(
    doc: &Document,
    story: StoryRef,
    path: &Path,
    pl: &ParaLayout,
    l0: usize,
    l1: usize,
    x: f32,
    y: f32,
    opts: &DisplayOptions,
    alpha: f32,
    background: Option<Rgb>,
    out: &mut Vec<Draw>,
) {
    let Some(first) = pl.lines.get(l0) else { return };
    let para = doc.para(story, path);
    for li in l0..l1 {
        let Some(line) = pl.lines.get(li) else { continue };
        let top = y + (line.top - first.top);
        let base = top + (line.baseline - line.top);
        let bottom = top + line.height;
        // Commented text gets a soft shade (comment anchors are object markers).
        if opts.markup
            && opts.revisions.comments
            && let Some(p) = para
        {
            let mut open: Option<usize> = None;
            let mut ranges = Vec::new();
            for off in p.object_offsets() {
                match p.object_at(off) {
                    Some(InlineObject::CommentStart { .. }) => open = open.or(Some(off)),
                    Some(InlineObject::CommentEnd { .. }) => {
                        ranges.push((open.take().unwrap_or(0), off));
                    }
                    _ => {}
                }
            }
            if let Some(o) = open {
                ranges.push((o, p.len()));
            }
            for (a, b) in ranges {
                if b <= line.start || a >= line.stop {
                    continue;
                }
                for (x0, x1) in pl.x_spans(li, a.max(line.start), b.min(line.stop)) {
                    if x1 > x0 {
                        out.push(Draw::Fill { rect: Rect::new(x + x0, top, x1 - x0, line.height), color: Rgb(0xEF, 0xE3, 0xF7), alpha });
                    }
                }
            }
        }
        // Backgrounds first: highlight and character shading; character borders group equal adjacent borders.
        let mut open: Option<(Border, f32, f32, f32, f32)> = None;
        for k in line.c0..line.c1 {
            let (Some(c), Some(cx), Some(nx)) = (pl.clusters.get(k), line.cl_left(k), line.cl_right(k)) else { continue };
            let Some(st) = pl.styles.get(c.style as usize) else { continue };
            if c.kind == ClKind::Marker {
                continue;
            }
            let band = crate::two_lines::cluster_box(pl, line, k);
            let band_top = band.map_or(top, |rect| top + rect.y - line.top);
            let band_bottom = band.map_or(bottom, |rect| band_top + rect.h);
            let band_height = band.map_or(line.height, |rect| rect.h);
            if let Some(h) = st.rc.highlight.or(st.rc.shading) {
                out.push(Draw::Fill { rect: Rect::new(x + cx, band_top, (nx - cx).max(0.0), band_height), color: h, alpha });
            }
            let b = st.rc.border;
            if let Some((ob, x0, x1, y0, y1)) = open.as_mut()
                && Some(*ob) == b
                && (*y0 - band_top).abs() < 0.01
                && (*y1 - band_bottom).abs() < 0.01
            {
                // Right-to-left text grows leftwards.
                *x0 = x0.min(x + cx);
                *x1 = x1.max(x + nx);
                continue;
            }
            if let Some((ob, x0, x1, y0, y1)) = open.take() {
                char_box(&ob, x0, x1, y0, y1, alpha, out);
            }
            open = b.map(|b| (b, x + cx, x + nx, band_top, band_bottom));
        }
        if let Some((ob, x0, x1, y0, y1)) = open {
            char_box(&ob, x0, x1, y0, y1, alpha, out);
        }
        // Label.
        if li == 0
            && let Some(lab) = &pl.label
            && let Some(st) = pl.styles.get(lab.style as usize)
        {
            let gx = x + lab.x;
            out.push(Draw::Glyphs {
                face: st.face,
                size: st.size,
                horizontal_scale: st.width_scale,
                glyphs: lab.glyphs.iter().map(|g| (g.gid, gx + g.dx, base - st.shift - st.row_shift - g.dy)).collect(),
                color: text_color(&st.rc.color, st.rc.shading.or(st.rc.highlight).or(background)),
                alpha,
                synth_bold: st.synth_bold,
                synth_italic: st.synth_italic,
                text: lab.text.chars().map(|c| kition_doc_fonts::word::symbol_font_char(&st.rc.font, c).unwrap_or(c)).collect(),
                link: None,
                ranges: Vec::new(),
            });
        }
        // Hyphen at a hyphenated line end.
        if let Some((si, gid, adv)) = line.hyphen
            && let Some(st) = pl.styles.get(si as usize)
        {
            let hx = x + line.hyphen_x(adv);
            out.push(Draw::Glyphs {
                face: st.face,
                size: st.size,
                horizontal_scale: st.width_scale,
                glyphs: vec![(gid, hx, base - st.shift - st.row_shift)],
                color: text_color(&st.rc.color, st.rc.shading.or(st.rc.highlight).or(background)),
                alpha,
                synth_bold: st.synth_bold,
                synth_italic: st.synth_italic,
                text: "-".into(),
                link: None,
                ranges: Vec::new(),
            });
        }
        // Glyph runs grouped by style.
        let mut k = line.c0;
        while k < line.c1 {
            let Some(c) = pl.clusters.get(k) else { break };
            let si = c.style;
            let Some(st) = pl.styles.get(si as usize) else {
                k += 1;
                continue;
            };
            let mut glyphs = Vec::new();
            let mut text = String::new();
            let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
            let run_start = k;
            while k < line.c1 {
                let Some(c) = pl.clusters.get(k) else { break };
                if c.style != si {
                    break;
                }
                let cx = x + line.cl_left(k).unwrap_or(0.0);
                if matches!(c.kind, ClKind::Text | ClKind::Space) {
                    let a = text.len();
                    // An object's cluster (a field, a note number) stands for the text it shows.
                    let mut shown = None;
                    if let Ok(i) = pl.shown.binary_search_by_key(&k, |(i, _, _)| *i)
                        && let Some((_, s, r)) = pl.shown.get(i)
                    {
                        text.push_str(s);
                        shown = Some(r);
                    } else if let Some(p) = para {
                        text.push_str(p.text.get(c.start..c.end).unwrap_or(""));
                    }
                    let b = text.len();
                    let cg = pl.glyphs.get(c.g0 as usize..c.g1 as usize).unwrap_or(&[]);
                    if let Some(shown) = shown
                        && cg.len() > 1
                    {
                        // A whole field result is one cluster of many glyphs: each glyph gets the
                        // text it was shaped from (a ligature its letters), not the whole text as
                        // one ligature. Without that, a character each (the last takes any remainder).
                        let bounds: Vec<usize> = text.get(a..b).unwrap_or("").char_indices().map(|(i, _)| a + i).collect();
                        let known = shown.len() == cg.len();
                        for (n, g) in cg.iter().enumerate() {
                            let r = match shown.get(n).filter(|_| known) {
                                Some(r) => (a + r.start).min(b)..(a + r.end).min(b),
                                None => {
                                    let from = bounds.get(n).or(bounds.last()).copied().unwrap_or(a);
                                    let to = if n + 1 == cg.len() { b } else { bounds.get(n + 1).copied().unwrap_or(b) };
                                    from..to
                                }
                            };
                            glyphs.push((g.gid, cx + g.dx, base - st.shift - st.row_shift - g.dy));
                            ranges.push(r.start..r.end.max(r.start));
                        }
                        k += 1;
                        continue;
                    }
                    if cg.is_empty() {
                        // No glyph of its own (the second letter of a ligature such as لا): its
                        // text belongs to the glyphs before it.
                        let from = ranges.last().map(|r| r.start);
                        for r in ranges.iter_mut().rev().take_while(|r| Some(r.start) == from) {
                            r.end = b;
                        }
                    }
                    for g in cg {
                        glyphs.push((g.gid, cx + g.dx, base - st.shift - st.row_shift - g.dy));
                        ranges.push(a..b);
                    }
                }
                k += 1;
            }
            let run_end = k;
            let rc = &st.rc;
            let m = &opts.revisions;
            // Without markup a deletion is never drawn as ordinary text, even in a layout that
            // kept it; nor when deletions are hidden.
            if rc.del.is_some() && (!opts.markup || m.hides_deletions()) {
                continue;
            }
            let shown = opts.markup && m.insertions_deletions;
            let del = rc.del.filter(|_| shown);
            let ins = rc.ins.filter(|_| shown && del.is_none());
            let author = |r: u32| revision_color(doc.revisions.get(r as usize).map(|v| author_index(doc, &v.author)).unwrap_or(0));
            let color = match (ins, del) {
                (Some(r), _) => m.insert_color.unwrap_or_else(|| author(r)),
                (None, Some(r)) => m.delete_color.unwrap_or_else(|| author(r)),
                _ => text_color(&rc.color, rc.shading.or(rc.highlight).or(background)),
            };
            let ins_mark = ins.map(|_| m.insert_mark);
            let del_mark = del.map(|_| m.delete_mark);
            if del_mark == Some(DeleteMark::Caret) {
                // A caret where the text was, instead of the text.
                if let Some(cx) = line.cl_left(run_start).map(|c| x + c) {
                    let h = st.size * 0.3;
                    let w = (st.size / 18.0).max(0.6);
                    out.push(Draw::Line {
                        x0: cx - h * 0.6,
                        y0: base + 1.0,
                        x1: cx,
                        y1: base + 1.0 - h,
                        width: w,
                        color,
                        stroke: Stroke::Solid,
                        alpha,
                    });
                    out.push(Draw::Line {
                        x0: cx,
                        y0: base + 1.0 - h,
                        x1: cx + h * 0.6,
                        y1: base + 1.0,
                        width: w,
                        color,
                        stroke: Stroke::Solid,
                        alpha,
                    });
                }
                continue;
            }
            if del_mark == Some(DeleteMark::Hash) {
                // Each deleted character shows as #.
                let gid = st.face.glyph_for('#');
                for g in &mut glyphs {
                    g.0 = gid;
                }
            }
            if !glyphs.is_empty() {
                let run = Draw::Glyphs {
                    face: st.face,
                    size: st.size,
                    horizontal_scale: st.width_scale,
                    glyphs,
                    color,
                    alpha,
                    synth_bold: st.synth_bold || ins_mark == Some(InsertMark::Bold),
                    synth_italic: st.synth_italic || ins_mark == Some(InsertMark::Italic),
                    text,
                    link: rc.link.clone(),
                    ranges,
                };
                match rc.text_outline {
                    // Outlined letters (a visible `w14:textOutline`) are drawn as outlines, as WPS
                    // prints them: filled and stroked, not text.
                    Some(o) => {
                        let theme = &doc.settings.theme_colors;
                        let fill = match o.fill {
                            None => Some(color),
                            Some(paint) => paint.map(|p| p.resolve(theme)),
                        };
                        if let Some((_, paths)) = glyph_outlines(&run) {
                            let mut segs = Vec::new();
                            for p in paths {
                                path_segs(&p, &mut segs);
                            }
                            out.push(Draw::Path { segs, fill, stroke: Some(o.stroke.resolve(theme)), stroke_width: o.width, stroke_alpha: alpha });
                        }
                    }
                    // Letters with a blurred shadow, glow or reflection are ink, as WPS prints them
                    // as a picture (the effects aren't drawn).
                    None if rc.text_blur => out.extend(outlined(vec![run])),
                    None if pl.across.iter().any(|r| r.start == run_start) => out.push(across_line(run, x + line.cl_left(run_start).unwrap_or(0.0))),
                    None => out.push(run),
                }
            }
            // Decorations across the run (underline skips trailing spaces of the line). Right-to-left
            // text inside a run can split it into several visual spans.
            let mut end_k = run_end;
            while end_k > run_start
                && pl.clusters.get(end_k - 1).is_some_and(|c| matches!(c.kind, ClKind::Space | ClKind::Marker | ClKind::LineBreak))
                && end_k == line.c1
            {
                end_k -= 1;
            }
            let thick = (st.size / 18.0).max(0.5);
            let underline = match (ins_mark, del_mark) {
                (Some(InsertMark::Underline), _) | (_, Some(DeleteMark::Underline)) => Underline::Single,
                (Some(InsertMark::DoubleUnderline), _) => Underline::Double,
                _ => rc.underline,
            };
            let double = rc.double_strike || del_mark == Some(DeleteMark::DoubleStrikethrough);
            let struck = rc.strike || double || ins_mark == Some(InsertMark::Strikethrough) || del_mark == Some(DeleteMark::Strikethrough);
            let spans = if underline != Underline::None || struck { line.spans(run_start, end_k) } else { Vec::new() };
            for (x0, x1) in spans.into_iter().map(|(a, b)| (x + a, x + b)).filter(|(a, b)| b > a) {
                if underline != Underline::None {
                    let uy = base - st.shift - st.row_shift + st.size * 0.12;
                    let ucolor = rc.underline_color.unwrap_or(color);
                    if underline == Underline::Words {
                        for kk in run_start..end_k {
                            let Some(c) = pl.clusters.get(kk) else { continue };
                            if c.kind != ClKind::Text {
                                continue;
                            }
                            let a = x + line.cl_left(kk).unwrap_or(0.0);
                            let b = x + line.cl_right(kk).unwrap_or(a - x);
                            if a < x0 - 0.01 || b > x1 + 0.01 {
                                continue; // drawn with its own span
                            }
                            out.push(Draw::Line { x0: a, y0: uy, x1: b, y1: uy, width: thick, color: ucolor, stroke: Stroke::Solid, alpha });
                        }
                    } else {
                        let w = if underline == Underline::Thick { thick * 2.0 } else { thick };
                        out.push(Draw::Line { x0, y0: uy, x1, y1: uy, width: w, color: ucolor, stroke: stroke_of(underline), alpha });
                    }
                }
                if struck {
                    let sy = base - st.shift - st.row_shift - st.size * 0.28;
                    let stroke = if double { Stroke::Double } else { Stroke::Solid };
                    out.push(Draw::Line { x0, y0: sy, x1, y1: sy, width: thick, color, stroke, alpha });
                }
            }
        }
        // Proofing squiggles.
        for (a, b, grammar) in pl.issues.iter().filter(|_| opts.proofing) {
            if *b <= line.start || *a >= line.stop {
                continue;
            }
            for (x0, x1) in pl.x_spans(li, (*a).max(line.start), (*b).min(line.stop)) {
                if x1 - x0 < 1.0 {
                    continue;
                }
                let y = base + 2.5;
                let color = if *grammar { Rgb(0x2B, 0x57, 0xC0) } else { Rgb(0xE0, 0x24, 0x24) };
                out.push(Draw::Line {
                    x0: x + x0,
                    y0: y,
                    x1: x + x1,
                    y1: y,
                    width: 0.8,
                    color,
                    stroke: if *grammar { Stroke::Double } else { Stroke::Wave },
                    alpha,
                });
            }
        }
        // Tab leaders.
        for (k, leader) in &line.leaders {
            let (Some(c), Some(a), Some(b)) = (pl.clusters.get(*k), line.cl_left(*k), line.cl_right(*k)) else { continue };
            let Some(st) = pl.styles.get(c.style as usize) else { continue };
            let Some(ch) = leader.char() else { continue };
            let gid = st.face.glyph_for(ch);
            let adv = (st.face.advance(gid) as f32 * st.size / st.face.upem.max(1.0) as f32).max(1.0);
            // WPS for Mac steps dots a whole tenth of a point apart, the mark's advance rounded up
            // (10.5 pt Helvetica Neue's 2.92 pt full stop 3 pt apart, 11 pt's 3.06 pt 3.1 pt apart,
            // 12 pt PingFang's 3.17 pt 3.2 pt apart).
            let adv = if matches!(leader, TabLeader::Dot | TabLeader::MiddleDot) { (adv * 10.0 - 0.001).ceil() / 10.0 } else { adv };
            // As WPS fills it: from half a mark after the text, marks edge to edge, the last one
            // ending at or before the text after the tab.
            let (mut gx, end) = (x + a + adv * 0.5, x + b);
            let mut glyphs = Vec::new();
            while gx + adv <= end + 0.01 && glyphs.len() < 2000 {
                glyphs.push((gid, gx, base));
                gx += adv;
            }
            // An underscore leader is one unbroken line, as Word and WPS draw it, in fonts whose
            // underscores don't meet too: a rule over the marks' ink, the marks kept as text.
            let rule = (*leader == TabLeader::Underscore)
                .then(|| kition_doc_fonts::math::glyph_bounds(&st.face, gid))
                .flatten()
                .zip(glyphs.first().zip(glyphs.last()))
                .map(|((x0, y0, x1, y1), (first, last))| {
                    let k = st.size / st.face.upem.max(1.0) as f32;
                    let (top, bottom) = (base - y1 * k, base - y0 * k);
                    Draw::Line {
                        x0: first.1 + x0 * k,
                        y0: (top + bottom) / 2.0,
                        x1: last.1 + x1 * k,
                        y1: (top + bottom) / 2.0,
                        width: (bottom - top).max(0.25),
                        color: text_color(&st.rc.color, None),
                        stroke: Stroke::Solid,
                        alpha,
                    }
                });
            let text: String = std::iter::repeat_n(ch, glyphs.len()).collect();
            let bytes = ch.len_utf8();
            let ranges = (0..glyphs.len()).map(|index| index * bytes..(index + 1) * bytes).collect();
            out.push(Draw::Glyphs {
                face: st.face,
                size: st.size,
                horizontal_scale: st.width_scale,
                glyphs,
                color: text_color(&st.rc.color, st.rc.shading.or(st.rc.highlight).or(background)),
                alpha,
                synth_bold: false,
                synth_italic: false,
                text,
                link: None,
                ranges,
            });
            out.extend(rule);
        }
        // Inline objects.
        for k in line.c0..line.c1 {
            let Some(c) = pl.clusters.get(k) else { continue };
            let ClKind::Object(oi) = c.kind else { continue };
            if c.obj_h <= 0.0 {
                continue;
            }
            let cx = x + line.cl_left(k).unwrap_or(0.0);
            let obj = para.and_then(|p| p.objects.get(oi));
            let rect = inline_rect(obj, cx, base, c.adv, c.obj_h);
            let spin = obj.and_then(InlineObject::frame).map(|(_, _, f)| f.spin()).unwrap_or_default();
            match obj {
                Some(o @ (InlineObject::Image { .. } | InlineObject::Shape { .. } | InlineObject::Group { .. })) => {
                    object_draws(doc, o, rect, Spin::default(), alpha, opts, out)
                }
                Some(InlineObject::Graphic { graphic, alt, .. }) => {
                    let mut draws = Vec::new();
                    spun(spin, rect, graphic_draws(doc, graphic, rect, alpha), &mut draws);
                    out.push(Draw::Figure { alt: alt.clone(), kind: graphic.kind, draws })
                }
                Some(InlineObject::Equation { .. }) => {
                    if let Some((_, ml)) = pl.maths.iter().find(|(k, _)| *k == oi) {
                        equation(&ml.items, cx, base, alpha, opts.placeholders, out);
                    }
                }
                Some(InlineObject::Field { .. }) => {
                    if let Some((_, ml)) = pl.eq_fields.iter().find(|(k, _)| *k == oi) {
                        equation(&ml.items, cx, base, alpha, opts.placeholders, out);
                    }
                }
                _ => {}
            }
        }
        // Formatting marks.
        if opts.marks {
            let msize = pl.styles.first().map(|s| s.size).unwrap_or(11.0);
            for k in line.c0..line.c1 {
                let Some(c) = pl.clusters.get(k) else { continue };
                let cx = x + line.cl_left(k).unwrap_or(0.0);
                let nx = x + line.cl_right(k).unwrap_or(cx - x);
                let size = pl.styles.get(c.style as usize).map(|s| s.size).unwrap_or(msize);
                let arrow = if line.rtl { '←' } else { '→' };
                match c.kind {
                    ClKind::Space => {
                        out.push(Draw::Mark { x: (cx + nx) / 2.0 - size * 0.12, baseline: base - size * 0.08, size, ch: '·', color: None })
                    }
                    ClKind::Tab => {
                        out.push(Draw::Mark { x: cx + ((nx - cx) / 2.0 - size * 0.3).max(0.0), baseline: base, size, ch: arrow, color: None })
                    }
                    ClKind::LineBreak => out.push(Draw::Mark { x: cx + 1.0, baseline: base, size, ch: '↵', color: None }),
                    ClKind::PageBreak | ClKind::ColumnBreak => {
                        let label = if c.kind == ClKind::PageBreak { '⤓' } else { '⇥' };
                        out.push(Draw::Line {
                            x0: cx + 2.0,
                            y0: base - size * 0.3,
                            x1: x + line.right,
                            y1: base - size * 0.3,
                            width: 0.5,
                            color: Rgb(0x60, 0x60, 0x60),
                            stroke: Stroke::Dotted,
                            alpha,
                        });
                        out.push(Draw::Mark { x: cx + 1.0, baseline: base, size, ch: label, color: None });
                    }
                    _ => {}
                }
            }
            if line.end == LineEnd::Para {
                // The mark follows the line's visual end, past every glyph, even where the
                // logically last text runs against the paragraph's direction.
                let ex = x + line.visual_end_x();
                let size = pl.lines.first().map(|_| msize).unwrap_or(msize);
                // A right-to-left paragraph's mark sits at its end, on the left.
                let mx = if line.rtl { ex - 1.0 - size * 0.6 } else { ex + 1.0 };
                // A tracked (inserted or deleted) paragraph mark is drawn in its author's colour.
                let m = &opts.revisions;
                let color = para.filter(|_| opts.markup && m.insertions_deletions).and_then(|p| match (p.mark.ins, p.mark.del) {
                    (_, Some(r)) => Some(
                        m.delete_color
                            .unwrap_or_else(|| revision_color(doc.revisions.get(r as usize).map(|v| author_index(doc, &v.author)).unwrap_or(0))),
                    ),
                    (Some(r), None) => Some(
                        m.insert_color
                            .unwrap_or_else(|| revision_color(doc.revisions.get(r as usize).map(|v| author_index(doc, &v.author)).unwrap_or(0))),
                    ),
                    _ => None,
                });
                // The last paragraph of a section ends in a section break instead of a plain ¶.
                if let Some(start) = section_break_after(doc, story, path) {
                    let (x0, x1) = if line.rtl { (x + line.left, ex - 2.0) } else { (ex + 2.0, x + line.right) };
                    section_break_mark(start, x0, x1, base, size, alpha, out);
                } else {
                    out.push(Draw::Mark { x: mx, baseline: base, size, ch: '¶', color });
                }
            }
        }
        let _ = bottom;
    }
}

/// The kind of section break that ends at body paragraph `path`: the start type of the section
/// that follows it. `None` for any other paragraph, and for the document's last section.
fn section_break_after(doc: &Document, story: StoryRef, path: &Path) -> Option<SectionStart> {
    if story != StoryRef::Body || path.0.len() != 1 {
        return None;
    }
    let i = *path.0.first()? as usize;
    match doc.body.get(i).map(|b| &**b) {
        Some(Block::Para(p)) if p.section.is_some() => {}
        _ => return None,
    }
    let secs = doc.sections();
    let k = secs.iter().position(|(end, _)| *end == i)?;
    secs.get(k + 1).map(|(_, s)| s.start)
}

/// On-screen name of a section break.
pub fn section_break_label(start: SectionStart) -> &'static str {
    match start {
        SectionStart::NextPage => "Section Break (Next Page)",
        SectionStart::Continuous => "Section Break (Continuous)",
        SectionStart::EvenPage => "Section Break (Even Page)",
        SectionStart::OddPage => "Section Break (Odd Page)",
        SectionStart::NextColumn => "Section Break (Next Column)",
    }
}

/// Width of a [`Draw::MarkText`] label.
pub fn mark_text_width(text: &str, size: f32) -> f32 {
    let face = kition_doc_fonts::word::resolve("Source Sans 3", false, false).face;
    let k = size / face.upem.max(1.0) as f32;
    text.chars().map(|c| face.advance(face.glyph_for(c)) as f32 * k).sum()
}

/// A section break mark between `x0` and `x1`: a dotted double rule with the break's name centred
/// in it (just the name when there is no room for the rule).
fn section_break_mark(start: SectionStart, x0: f32, x1: f32, base: f32, size: f32, alpha: f32, out: &mut Vec<Draw>) {
    let text = section_break_label(start);
    let lsize = (size * 0.8).clamp(6.0, 11.0);
    let w = mark_text_width(text, lsize);
    let gap = lsize * 0.4;
    let mid = base - size * 0.3;
    let color = Rgb(0x60, 0x60, 0x60);
    if x1 - x0 <= w + 4.0 * gap {
        out.push(Draw::MarkText { x: x0, baseline: base, size: lsize, text: text.into() });
        return;
    }
    let tx = (x0 + x1 - w) / 2.0;
    for (a, b) in [(x0, tx - gap), (tx + w + gap, x1)] {
        for y in [mid - 1.2, mid + 1.2] {
            out.push(Draw::Line { x0: a, y0: y, x1: b, y1: y, width: 0.5, color, stroke: Stroke::Dotted, alpha });
        }
    }
    out.push(Draw::MarkText { x: tx, baseline: mid + lsize * 0.33, size: lsize, text: text.into() });
}

/// An equation's glyphs and rules with its origin at (`x`, `base`).
fn equation(items: &[MItem], x: f32, base: f32, alpha: f32, screen: bool, out: &mut Vec<Draw>) {
    for it in items {
        match it {
            MItem::Glyphs { face, size, color, synth_bold, synth_italic, glyphs, text } => out.push(Draw::Glyphs {
                face: *face,
                size: *size,
                horizontal_scale: 1.0,
                glyphs: glyphs.iter().map(|(g, gx, gy)| (*g, x + gx, base - gy)).collect(),
                color: (*color).into(),
                alpha,
                synth_bold: *synth_bold,
                synth_italic: *synth_italic,
                text: text.clone(),
                link: None,
                // Equation text maps back through the font (no per-glyph ranges from the math layout).
                ranges: Vec::new(),
            }),
            MItem::Rect { x: rx, y, w, h, color } => {
                out.push(Draw::Fill { rect: Rect::new(x + rx, base - y - h, *w, *h), color: (*color).into(), alpha })
            }
            MItem::Line { x0, y0, x1, y1, width, color, dotted } => out.push(Draw::Line {
                x0: x + x0,
                y0: base - y0,
                x1: x + x1,
                y1: base - y1,
                width: *width,
                color: (*color).into(),
                stroke: if *dotted { Stroke::Dotted } else { Stroke::Solid },
                alpha,
            }),
            MItem::ScreenOnly(inner) => {
                if screen {
                    equation(std::slice::from_ref(inner), x, base, alpha, screen, out);
                }
            }
        }
    }
}

/// The draws of a chart or diagram's items inside `rect` (page coordinates). The items were built
/// for the size `g.w` × `g.h` (0: `rect`'s own size), so they are scaled to `rect`; strokes and text
/// by the mean of the two scales.
fn graphic_draws(doc: &Document, g: &Graphic, rect: Rect, alpha: f32) -> Vec<Draw> {
    let (sx, sy) = (scale_of(rect.w, g.w), scale_of(rect.h, g.h));
    let k = (sx + sy) / 2.0;
    let inside = |r: &[f32; 4]| Rect::new(rect.x + r[0] * sx, rect.y + r[1] * sy, r[2] * sx, r[3] * sy);
    let mut out = Vec::with_capacity(g.items.len());
    for it in &g.items {
        match it {
            GraphicItem::Shape { rect: r, kind, fill, stroke, stroke_width } => out.push(Draw::Shape {
                rect: inside(r),
                kind: *kind,
                fill: *fill,
                stroke: *stroke,
                stroke_width: stroke_width * k,
                effects: Default::default(),
                geom: None,
                gradient: None,
                dash: None,
            }),
            GraphicItem::Path { segs, fill, stroke, stroke_width } => {
                let segs = page_segs(segs, |x, y| (rect.x + x * sx, rect.y + y * sy));
                out.push(Draw::Path { segs, fill: *fill, stroke: *stroke, stroke_width: stroke_width * k, stroke_alpha: 1.0 })
            }
            GraphicItem::Text { rect: r, .. } => out.extend(text_glyphs(doc, inside(r), it, alpha, k)),
            GraphicItem::Image { rect: r, media } => out.push(Draw::Image {
                rect: inside(r),
                media: media.clone(),
                crop: [0.0; 4],
                alpha,
                adjust: Default::default(),
                geom: None,
                edge: Default::default(),
            }),
        }
    }
    out
}

/// Scale from the size `built` points a graphic's items were made for to `len` points; 1 when
/// the built size is unknown (0).
fn scale_of(len: f32, built: f32) -> f32 {
    if built.is_finite() && built > 0.0 && len.is_finite() { (len / built).max(0.0) } else { 1.0 }
}

/// Path segments mapped to page coordinates by `map`, cleaned for the rasterizer and PDF: a segment
/// with a non-finite number is dropped, and so is anything before a move (or after a broken one).
fn page_segs(segs: &[PathSeg], map: impl Fn(f32, f32) -> (f32, f32)) -> Vec<PathSeg> {
    let fin = |p: (f32, f32)| p.0.is_finite() && p.1.is_finite();
    let mut out = Vec::with_capacity(segs.len());
    let mut open = false;
    for s in segs {
        match *s {
            PathSeg::Move(x, y) => {
                let p = map(x, y);
                open = fin(p);
                if open {
                    out.push(PathSeg::Move(p.0, p.1));
                }
            }
            PathSeg::Line(x, y) => {
                let p = map(x, y);
                if open && fin(p) {
                    out.push(PathSeg::Line(p.0, p.1));
                }
            }
            PathSeg::Cubic(a, b, c, d, e, f) => {
                let (p1, p2, p3) = (map(a, b), map(c, d), map(e, f));
                if open && fin(p1) && fin(p2) && fin(p3) {
                    out.push(PathSeg::Cubic(p1.0, p1.1, p2.0, p2.1, p3.0, p3.1));
                }
            }
            PathSeg::Close => {
                if open {
                    out.push(PathSeg::Close);
                }
                open = false;
            }
        }
    }
    out
}

/// A path of clean segments (see [`page_segs`]) as a kurbo path, for the rasteriser and PDF.
/// The draws of a shape at `rect`: a preset outline, or a freeform's paths (an ink stroke left out
/// under Review › Hide Ink).
#[allow(clippy::too_many_arguments)]
fn shape_draws(
    rect: Rect,
    kind: ShapeKind,
    geom: Option<&kition_doc_doc::graphic::PresetGeom>,
    gradient: Option<&std::sync::Arc<kition_doc_doc::graphic::ShapeGradient>>,
    fill: Option<Rgb>,
    (stroke, dash): (Option<Rgb>, Option<kition_doc_doc::graphic::LineDash>),
    stroke_width: f32,
    effects: kition_doc_doc::effects::ShapeEffects,
    freeform: Option<&kition_doc_doc::freeform::Freeform>,
    opts: &DisplayOptions,
    out: &mut Vec<Draw>,
) {
    let Some(f) = freeform.filter(|_| kind == ShapeKind::Freeform) else {
        out.push(Draw::Shape { rect, kind, fill, stroke, stroke_width, effects, geom: geom.cloned(), gradient: gradient.cloned(), dash });
        return;
    };
    if f.is_ink() && opts.hide_ink {
        return;
    }
    let width = if stroke_width.is_finite() { stroke_width.clamp(0.0, 200.0) } else { 0.75 };
    crate::freeform_display::draw(rect, f, fill, stroke, width, out);
}

pub fn seg_path(segs: &[PathSeg]) -> BezPath {
    let mut p = BezPath::new();
    for s in segs {
        match *s {
            PathSeg::Move(x, y) => p.move_to((x as f64, y as f64)),
            PathSeg::Line(x, y) => p.line_to((x as f64, y as f64)),
            PathSeg::Cubic(a, b, c, d, e, f) => p.curve_to((a as f64, b as f64), (c as f64, d as f64), (e as f64, f as f64)),
            PathSeg::Close => p.close_path(),
        }
    }
    p
}

/// One line of a `GraphicItem::Text` in `r`, shaped in its font (the document's body font when
/// unset), letters it lacks (Chinese in a chart titled in Calibri) in a fallback face as body text
/// takes them, aligned horizontally and centred vertically, at its size times `k`: one draw per
/// face. Empty when there is nothing to draw, or the item's rectangle or size isn't a finite number
/// (a degenerate scale: `r` itself is always finite, as [`Rect::new`] makes it). A placeholder's
/// prompt is drawn as outlines.
fn text_glyphs(doc: &Document, r: Rect, it: &GraphicItem, alpha: f32, k: f32) -> Vec<Draw> {
    let GraphicItem::Text { rect, text, size, color, bold, align, font, prompt } = it else { return Vec::new() };
    let size = *size * k;
    if !rect.iter().chain([&size]).all(|v| v.is_finite()) {
        return Vec::new();
    }
    let family = font.as_deref().filter(|f| !f.is_empty()).unwrap_or(&doc.settings.minor_font);
    let size = size.clamp(1.0, 400.0);
    let resolved = kition_doc_fonts::word::resolve_with_charset(family, doc.settings.font_charset(family), *bold, false);
    let primary = resolved.face;
    // Runs of the text by the face that draws them (`None`: the item's own).
    let mut runs: Vec<(usize, usize, Option<kition_doc_fonts::FaceRef>)> = Vec::new();
    for (i, c) in text.char_indices() {
        let last = runs.last().map(|(_, _, f)| *f);
        let face = if primary.covers(c) || c.is_whitespace() || c.is_control() {
            None
        } else if let Some(Some(f)) = last.filter(|f| f.is_some_and(|f| f.covers(c))) {
            Some(f)
        } else {
            kition_doc_fonts::FontDb::global().fallback_styled(c, primary.id(), *bold, false).map(|f| kition_doc_fonts::FaceRef::of(&f))
        };
        match runs.last_mut() {
            Some((_, end, f)) if f.map(|x| x.id()) == face.map(|x| x.id()) => *end = i + c.len_utf8(),
            _ => runs.push((i, i + c.len_utf8(), face)),
        }
    }
    let (ascent, descent) = kition_doc_fonts::word::line_metrics(primary.get());
    let em = size / primary.get().upem.max(1.0) as f32;
    let baseline = r.y + r.h / 2.0 + (ascent - descent) as f32 * em / 2.0;
    let shaped: Vec<_> = runs
        .iter()
        .filter_map(|(a, b, f)| {
            let face = f.unwrap_or(primary);
            let part = text.get(*a..*b)?;
            let glyphs = kition_doc_fonts::shape(face.get(), part, &[], |c| c);
            let k = size / face.get().upem.max(1.0) as f32;
            (!glyphs.is_empty()).then_some((face, f.is_none(), part, glyphs, k))
        })
        .collect();
    let width: f32 = shaped.iter().map(|(_, _, _, glyphs, k)| glyphs.iter().map(|g| g.x_advance as f32 * k).sum::<f32>()).sum();
    let mut pen = match align {
        TextAlign::Left => r.x,
        TextAlign::Center => r.x + (r.w - width) / 2.0,
        TextAlign::Right => r.x + r.w - width,
    };
    let mut out = Vec::with_capacity(shaped.len());
    for (face, own, part, glyphs, k) in shaped {
        let mut placed = Vec::with_capacity(glyphs.len());
        for g in &glyphs {
            placed.push((g.gid, pen + g.x_offset as f32 * k, baseline - g.y_offset as f32 * k));
            pen += g.x_advance as f32 * k;
        }
        out.push(Draw::Glyphs {
            face,
            size,
            horizontal_scale: 1.0,
            glyphs: placed,
            color: *color,
            alpha,
            // A fallback face is emboldened when it has no bold of its own, as in body text.
            synth_bold: if own { resolved.synth_bold } else { *bold && face.get().weight < 600.0 },
            synth_italic: false,
            text: part.to_string(),
            link: None,
            ranges: Vec::new(),
        });
    }
    if *prompt { outlined(out) } else { out }
}

fn author_index(doc: &Document, author: &str) -> u32 {
    let mut seen: Vec<&str> = Vec::new();
    for r in &doc.revisions {
        if !seen.contains(&r.author.as_str()) {
            seen.push(&r.author);
        }
    }
    seen.iter().position(|a| *a == author).unwrap_or(0) as u32
}

/// Automatic colour is black, or white on a dark background.
pub fn text_color(c: &TextColor, background: Option<Rgb>) -> Rgb {
    match c {
        TextColor::Rgb(r) => *r,
        TextColor::Auto => {
            if background.is_some_and(|b| b.luma() < 100.0) {
                Rgb::WHITE
            } else {
                Rgb::BLACK
            }
        }
    }
}

/// `items` turned by `spin` about the centre of `rect` (as they are, unturned).
fn spun(spin: Spin, rect: Rect, items: Vec<Draw>, out: &mut Vec<Draw>) {
    if spin.is_identity() {
        out.extend(items);
    } else {
        out.push(Draw::Rotated { cx: rect.x + rect.w / 2.0, cy: rect.y + rect.h / 2.0, spin, items });
    }
}

/// A picture, shape or group drawn in `rect` (its unturned frame), turned by its own spin about
/// the rect's centre; a group's members are turned inside it. `outer` is the spin it is already
/// drawn inside (its group's). Nothing for anything else.
fn object_draws(doc: &Document, o: &InlineObject, rect: Rect, outer: Spin, alpha: f32, opts: &DisplayOptions, out: &mut Vec<Draw>) {
    let own = o.frame().map(|(_, _, f)| f.spin()).unwrap_or_default();
    match o {
        InlineObject::Image { media, crop, adjust, geom, edge, .. } => spun(
            own,
            rect,
            vec![Draw::Image {
                rect,
                media: media.clone(),
                crop: *crop,
                alpha: alpha * adjust.opacity.clamp(0.0, 1.0),
                adjust: *adjust,
                geom: geom.clone(),
                edge: edge.for_picture(doc.media.get(media).map(|m| m.as_slice())),
            }],
            out,
        ),
        InlineObject::Shape { kind, fill, stroke, stroke_width, effects, freeform, geom, gradient, dash, .. } => {
            let effects = effects_in(*effects, own.within(outer));
            let mut items = Vec::new();
            shape_draws(
                rect,
                *kind,
                geom.as_ref(),
                gradient.as_ref(),
                *fill,
                (*stroke, *dash),
                *stroke_width,
                effects,
                freeform.as_deref(),
                opts,
                &mut items,
            );
            spun(own, rect, items, out)
        }
        InlineObject::Group { .. } => {
            let mut members = Vec::new();
            for ([x, y, w, h], c) in o.group_rects(rect.x, rect.y, rect.w, rect.h) {
                object_draws(doc, c, Rect::new(x, y, w, h), own.within(outer), alpha, opts, &mut members);
            }
            spun(own, rect, members, out)
        }
        // A chart or diagram member of a group.
        InlineObject::Graphic { graphic, alt, .. } => {
            let mut draws = Vec::new();
            spun(own, rect, graphic_draws(doc, graphic, rect, alpha), &mut draws);
            out.push(Draw::Figure { alt: alt.clone(), kind: graphic.kind, draws })
        }
        _ => {}
    }
}

/// A shape's `effects` as drawn turned by `spin` (all the turns it is drawn inside): glows and
/// soft edges turn with the shape, and so does a shadow's offset when it rotates with the shape
/// (`rotWithShape`); one that doesn't keeps its direction on the page, so its angle is turned back.
pub(crate) fn effects_in(effects: kition_doc_doc::effects::ShapeEffects, spin: Spin) -> kition_doc_doc::effects::ShapeEffects {
    let mut e = effects;
    if let Some(s) = e.shadow.as_mut().filter(|s| !s.rot_with_shape && !spin.is_identity()) {
        let a = s.angle.to_radians();
        let (dx, dy) = spin.unapply(0.0, 0.0, a.cos(), a.sin());
        s.angle = kition_doc_geom::normalize_degrees(dy.atan2(dx).to_degrees());
    }
    e
}

/// Where inline object `obj` is drawn, given its cluster's box (`adv` × `obj_h` standing on the
/// baseline at `cx`): inside the room kept for its effects.
/// A rotated one's unrotated frame, centred in that room.
pub(crate) fn inline_rect(obj: Option<&InlineObject>, cx: f32, base: f32, adv: f32, obj_h: f32) -> Rect {
    let frame = obj.and_then(InlineObject::frame);
    let [l, t, r, b] = frame.map_or([0.0; 4], |(_, _, float)| float.effect_extent());
    let room = Rect::new(cx + l, base - obj_h + t, (adv - l - r).max(0.0), (obj_h - t - b).max(0.0));
    let Some((w, h, float)) = frame.filter(|(_, _, f)| f.rot != 0.0) else { return room };
    // The room is the rotated bounds of the (possibly scaled-down) frame: undo the rotation.
    let (w, h) = (kition_doc_geom::finite(w).clamp(1.0, 4000.0), kition_doc_geom::finite(h).clamp(1.0, 4000.0));
    let (bw, _) = float.spin().extent(w, h);
    let k = if bw > 0.0 { room.w / bw } else { 1.0 };
    let (fw, fh) = (w * k, h * k);
    Rect::new(room.x + (room.w - fw) / 2.0, room.y + (room.h - fh) / 2.0, fw, fh)
}

/// Characters that stand upright in vertical (top-to-bottom) text, as Word and WPS draw a table
/// cell's vertical text: Chinese and Japanese characters, their punctuation and full-width forms,
/// and Hangul. Latin letters and digits lie on their side.
fn stands_upright(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF // Hangul jamo
        | 0x2E80..=0x2FDF // CJK radicals, Kangxi radicals
        | 0x3000..=0x303F // CJK symbols and punctuation
        | 0x3040..=0x30FF // hiragana, katakana
        | 0x3100..=0x31FF // bopomofo, Hangul compatibility jamo, kana extensions
        | 0x3200..=0x33FF // enclosed CJK, compatibility
        | 0x3400..=0x4DBF // CJK extension A
        | 0x4E00..=0x9FFF // CJK unified ideographs
        | 0xA960..=0xA97F // Hangul jamo extended A
        | 0xAC00..=0xD7FF // Hangul syllables, jamo extended B
        | 0xF900..=0xFAFF // CJK compatibility ideographs
        | 0xFE30..=0xFE4F // CJK compatibility forms
        | 0xFF01..=0xFF60 // full-width forms
        | 0xFFE0..=0xFFE6 // full-width signs
        | 0x20000..=0x3134F // CJK extensions B-G
    )
}

/// Text in a cell turned top-to-bottom, with the characters that stand upright in vertical text
/// (see [`stands_upright`]) turned back a quarter about the middle of their em box, each its own
/// run; the rest stays in its runs, lying along the line.
/// A run set across a vertical line (`w:eastAsianLayout w:vert`): its letters side by side,
/// squeezed into the one em it takes along the line from `x`, turned back a quarter about that em
/// so they read across the column (WPS for Mac).
fn across_line(run: Draw, x: f32) -> Draw {
    let Draw::Glyphs { face, size, horizontal_scale, mut glyphs, color, alpha, synth_bold, synth_italic, text, link, ranges } = run else {
        return run;
    };
    let k = size / face.upem.max(1.0) as f32 * horizontal_scale;
    let natural: f32 = glyphs.iter().map(|(gid, _, _)| face.advance(*gid) as f32 * k).sum();
    let squeeze = if natural > size { size / natural } else { 1.0 };
    let base = glyphs.first().map_or(0.0, |g| g.2);
    let mut gx = x + (size - natural * squeeze) / 2.0;
    for g in &mut glyphs {
        let adv = face.advance(g.0) as f32 * k * squeeze;
        (g.1, g.2) = (gx, base);
        gx += adv;
    }
    let run =
        Draw::Glyphs { face, size, horizontal_scale: horizontal_scale * squeeze, glyphs, color, alpha, synth_bold, synth_italic, text, link, ranges };
    // The em box stands from 0.88 em above the baseline to 0.12 em below it (as upright letters').
    Draw::Rotated { cx: x + size / 2.0, cy: base - 0.38 * size, spin: Spin::new(270.0, false, false), items: vec![run] }
}

fn upright_east_asian(items: Vec<Draw>) -> Vec<Draw> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Draw::Glyphs { face, size, horizontal_scale, glyphs, color, alpha, synth_bold, synth_italic, text, link, ranges } = item else {
            out.push(item);
            continue;
        };
        let upright = |i: usize| ranges.get(i).and_then(|r| text.get(r.clone())).is_some_and(|t| t.chars().next().is_some_and(stands_upright));
        if ranges.len() != glyphs.len() || !(0..glyphs.len()).any(upright) {
            out.push(Draw::Glyphs { face, size, horizontal_scale, glyphs, color, alpha, synth_bold, synth_italic, text, link, ranges });
            continue;
        }
        let k = size / face.upem.max(1.0) as f32;
        let run = |picked: Vec<usize>| {
            let mut sub = String::new();
            let mut sub_ranges = Vec::with_capacity(picked.len());
            for &i in &picked {
                let piece = ranges.get(i).and_then(|r| text.get(r.clone())).unwrap_or_default();
                sub_ranges.push(sub.len()..sub.len() + piece.len());
                sub.push_str(piece);
            }
            Draw::Glyphs {
                face,
                size,
                horizontal_scale,
                glyphs: picked.iter().filter_map(|&i| glyphs.get(i).copied()).collect(),
                color,
                alpha,
                synth_bold,
                synth_italic,
                text: sub,
                link: link.clone(),
                ranges: sub_ranges,
            }
        };
        let lying: Vec<usize> = (0..glyphs.len()).filter(|i| !upright(*i)).collect();
        if !lying.is_empty() {
            out.push(run(lying));
        }
        for (i, &(gid, gx, gy)) in glyphs.iter().enumerate() {
            if !upright(i) {
                continue;
            }
            let adv = face.advance(gid) as f32 * k * horizontal_scale;
            // The em box stands from 0.88 em above the baseline to 0.12 em below it.
            let (cx, cy) = (gx + adv / 2.0, gy - 0.38 * size);
            out.push(Draw::Rotated { cx, cy, spin: Spin::new(270.0, false, false), items: vec![run(vec![i])] });
        }
    }
    out
}
