//! Kition Doc layout: turns a [`Document`] into pages.
//!
//! - [`para`]: one paragraph → lines (shaping, first-fit line breaking, tabs, list labels,
//!   alignment). Results are memoised per paragraph revision in a [`LayoutCache`].
//! - [`layout`]: sections → pages and columns; tables; keep/widow rules; page breaks; headers
//!   and footers; page-number fields.
//! - [`display`]: a page → draw items (glyph runs, rules, fills, images) for the renderers.
//! - [`hit`]: point ↔ position, caret geometry, line navigation, selection rectangles.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod auto_spacing;
mod compression;
pub mod display;
mod eq_field;
pub mod fields;
mod fit_text;
mod freeform_display;
pub mod hit;
mod inline_groups;
pub mod kinsoku;
mod lookahead;
pub mod math;
pub mod para;
mod spacing_units;
mod table;
mod two_lines;

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use kition_doc_doc::graphic::{TextAnchor, TextFrame};
use kition_doc_doc::numbering::{Counters, Level};
use kition_doc_doc::para::{Anchor, Float, FloatAlign, InlineObject, Wrap};
use kition_doc_doc::props::{Border, CharProps, Rgb, TableFloat, TextDirection};
use kition_doc_doc::section::{SectionProps, SectionStart};
use kition_doc_doc::{Block, Blocks, Document, Paragraph, Path, StoryRef};
use kition_doc_geom::{Point, Rect, Spin};

pub use fields::FieldCtx;
pub use hit::VisualStep;
pub use para::{LineEnd, ParaLayout};
pub use table::{autofit_widths, measure_table_columns};

/// How the document is viewed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewMode {
    #[default]
    Print,
    /// One page as wide as the window, no page breaks.
    Web,
    /// Draft/Outline: like web, page breaks shown as rules.
    Draft,
}

#[derive(Clone, Debug, Default)]
pub struct LayoutOptions {
    pub view: ViewMode,
    /// Width of the window in Web/Draft views, points.
    pub web_width: f32,
    pub show_hidden: bool,
    /// Final text ("No Markup"): tracked deletions take no space and draw nothing.
    pub hide_deleted: bool,
    /// Check spelling and grammar (squiggles).
    pub proofing: bool,
}

/// Something placed on a page (page coordinates, points, y down).
#[derive(Clone, Debug)]
pub enum Placed {
    /// Lines `l0..l1` of a paragraph; `y` is the top of line `l0`; `x` the column's left edge.
    /// Turned lines (a table cell's text direction) run as `turn` says from the page point
    /// (`x`, `y`): see [`turn_point`].
    Lines {
        story: StoryRef,
        path: Path,
        para: Arc<ParaLayout>,
        l0: usize,
        l1: usize,
        x: f32,
        y: f32,
        turn: TextDirection,
    },
    Fill {
        rect: Rect,
        color: Rgb,
    },
    Rule {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        border: Border,
    },
    /// A picture in `rect`, turned about the rect's centre by `spin` (as are shapes and charts).
    Image {
        rect: Rect,
        media: String,
        crop: [f32; 4],
        /// Its colour adjustments.
        adjust: kition_doc_doc::graphic::ImageAdjust,
        /// The preset outline it is cut to (`None`: its rectangle).
        geom: Option<kition_doc_doc::graphic::PresetGeom>,
        /// Its line and effects.
        edge: kition_doc_doc::graphic::PictureEdge,
        story: StoryRef,
        path: Path,
        off: usize,
        spin: Spin,
    },
    Shape {
        rect: Rect,
        kind: kition_doc_doc::para::ShapeKind,
        fill: Option<Rgb>,
        stroke: Option<Rgb>,
        stroke_width: f32,
        /// A freeform's paths (ink strokes among them).
        freeform: Option<Arc<kition_doc_doc::freeform::Freeform>>,
        effects: kition_doc_doc::effects::ShapeEffects,
        spin: Spin,
        /// The preset outline when the kind does not name it.
        geom: Option<kition_doc_doc::graphic::PresetGeom>,
        /// A gradient fill (`fill` its first colour).
        gradient: Option<Arc<kition_doc_doc::graphic::ShapeGradient>>,
        /// Its outline's dash (`None`: solid).
        dash: Option<kition_doc_doc::graphic::LineDash>,
    },
    /// A floating chart or diagram, drawn from its items inside `rect`. The object is the U+FFFC at
    /// byte `off` of paragraph `path` (its alt text).
    Graphic {
        rect: Rect,
        graphic: Arc<kition_doc_doc::graphic::Graphic>,
        story: StoryRef,
        path: Path,
        off: usize,
        spin: Spin,
    },
    /// A table cell's area (for hit testing and cell selection).
    Cell {
        rect: Rect,
        table: Path,
        row: usize,
        cell: usize,
        story: StoryRef,
    },
    /// The area of a picture, shape or text box, for hit testing, selection handles and dragging
    /// (drawn by `Image`/`Shape`/`Lines`). The object is the U+FFFC at byte `off` of paragraph
    /// `path` in `story`. `rect` is its unrotated frame; `spin` turns it about the centre.
    Object {
        rect: Rect,
        spin: Spin,
        story: StoryRef,
        path: Path,
        off: usize,
        /// The text box story it shows (`Document::parts` id).
        text_box: Option<u32>,
        wrap: Wrap,
        /// Its paragraph's column left and top: where offsets relative to the column and the
        /// paragraph (`Anchor::Column` / `Anchor::Paragraph`) start.
        origin: kition_doc_geom::Point,
    },
    /// Art text (KitDrawing 1.3 DR26): the next `items` placed items, a text box's text inside
    /// its margins `rect`, are drawn bent by the preset text warp `warp`, turned with the shape
    /// by `spin` about `rect`'s centre. Hit testing and the caret use the items as laid out.
    Warp {
        rect: Rect,
        warp: kition_doc_doc::graphic::PresetGeom,
        items: usize,
        spin: kition_doc_geom::Spin,
    },
    /// A group WPS prints as one picture (one of its members has a blurred shadow, a glow or soft
    /// edges): the next `items` placed items, its text boxes' text inside the group's frame
    /// `rect`, are drawn as ink (their letters' outlines), not text. Hit testing and the caret use
    /// the items as laid out.
    Flat {
        rect: Rect,
        items: usize,
    },
    /// The next `items` placed items (a text box's text) are drawn clipped to `rect`, the box: a
    /// line that starts inside it and runs past its edge is cut off there, as WPS draws it.
    Clip {
        rect: Rect,
        items: usize,
    },
}

impl Placed {
    fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Placed::Lines { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            Placed::Fill { rect, .. }
            | Placed::Image { rect, .. }
            | Placed::Shape { rect, .. }
            | Placed::Graphic { rect, .. }
            | Placed::Cell { rect, .. }
            | Placed::Warp { rect, .. }
            | Placed::Flat { rect, .. }
            | Placed::Clip { rect, .. } => {
                rect.x += dx;
                rect.y += dy;
            }
            Placed::Object { rect, origin, .. } => {
                rect.x += dx;
                rect.y += dy;
                origin.x += dx;
                origin.y += dy;
            }
            Placed::Rule { x0, y0, x1, y1, .. } => {
                *x0 += dx;
                *x1 += dx;
                *y0 += dy;
                *y1 += dy;
            }
        }
    }

    /// The page area of turned lines (`None` for anything else).
    pub fn turned_bounds(&self) -> Option<Rect> {
        let Placed::Lines { para, l0, l1, x, y, turn, .. } = self else { return None };
        if !turn.is_turned() {
            return None;
        }
        let first = para.lines.get(*l0)?;
        let last = para.lines.get(l1.checked_sub(1)?)?;
        let len = para.lines.get(*l0..*l1).unwrap_or(&[]).iter().map(|l| l.right).fold(0.0f32, f32::max);
        Some(turn_rect(*turn, *x, *y, Rect::new(0.0, 0.0, len, last.top + last.height - first.top)))
    }

    /// Move an item laid out in a turned frame (a table cell's text running `turn`) onto the
    /// page, the frame's origin landing on page point (`x`, `y`).
    fn turn(&mut self, turn: TextDirection, x: f32, y: f32) {
        if !turn.is_turned() {
            self.translate(x, y);
            return;
        }
        match self {
            Placed::Lines { x: lx, y: ly, turn: t, .. } => {
                (*lx, *ly) = turn_point(turn, x, y, *lx, *ly);
                // Turned text inside turned text keeps its own direction (no 180° text).
                if !t.is_turned() {
                    *t = turn;
                }
            }
            Placed::Fill { rect, .. }
            | Placed::Image { rect, .. }
            | Placed::Shape { rect, .. }
            | Placed::Graphic { rect, .. }
            | Placed::Cell { rect, .. }
            | Placed::Warp { rect, .. }
            | Placed::Flat { rect, .. }
            | Placed::Clip { rect, .. } => {
                *rect = turn_rect(turn, x, y, *rect);
            }
            Placed::Object { rect, origin, .. } => {
                *rect = turn_rect(turn, x, y, *rect);
                (origin.x, origin.y) = turn_point(turn, x, y, origin.x, origin.y);
            }
            Placed::Rule { x0, y0, x1, y1, .. } => {
                (*x0, *y0) = turn_point(turn, x, y, *x0, *y0);
                (*x1, *y1) = turn_point(turn, x, y, *x1, *y1);
            }
        }
    }
}

/// Where point (`u`, `v`) of a frame turned `turn` lands on the page, the frame's origin being
/// page point (`x`, `y`): `u` runs along the lines, `v` across them (down for horizontal text).
pub fn turn_point(turn: TextDirection, x: f32, y: f32, u: f32, v: f32) -> (f32, f32) {
    match turn {
        TextDirection::Horizontal => (x + u, y + v),
        TextDirection::Down => (x - v, y + u),
        TextDirection::Up => (x + v, y - u),
    }
}

/// The inverse of [`turn_point`]: the frame point at page point (`px`, `py`).
pub fn unturn_point(turn: TextDirection, x: f32, y: f32, px: f32, py: f32) -> (f32, f32) {
    match turn {
        TextDirection::Horizontal => (px - x, py - y),
        TextDirection::Down => (py - y, x - px),
        TextDirection::Up => (y - py, px - x),
    }
}

/// The page rectangle a frame rectangle covers (see [`turn_point`]).
pub fn turn_rect(turn: TextDirection, x: f32, y: f32, r: Rect) -> Rect {
    match turn {
        TextDirection::Horizontal => Rect::new(x + r.x, y + r.y, r.w, r.h),
        TextDirection::Down => Rect::new(x - r.y - r.h, y + r.x, r.h, r.w),
        TextDirection::Up => Rect::new(x + r.y, y - r.x - r.w, r.h, r.w),
    }
}

/// One laid-out page.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub w: f32,
    pub h: f32,
    /// Index into `Document::sections()`.
    pub section: usize,
    /// Page number as displayed (after section restarts).
    pub number: u32,
    pub items: Vec<Placed>,
    /// Header / footer items (drawn dimmed while editing the body).
    pub header: Vec<Placed>,
    pub footer: Vec<Placed>,
    /// The body text area (margins).
    pub body: Rect,
    pub header_story: Option<u32>,
    pub footer_story: Option<u32>,
    /// Top-level body block index at the start of the page.
    pub first_block: usize,
    /// A blank page inserted so a section starts on an even or odd page: it shows no header or
    /// footer (measured on WPS references).
    pub blank: bool,
}

/// The whole layout.
#[derive(Clone, Debug, Default)]
pub struct DocLayout {
    pub pages: Vec<Page>,
    /// Index: (story, paragraph path) → (page, item index) of each placed piece, in order.
    pub index: HashMap<(StoryRef, Path), Vec<(usize, usize)>>,
    /// Layout time, milliseconds.
    pub ms: f64,
    /// Text box stories laid out (each time a box's text was).
    pub text_boxes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    rev: u64,
    width: u32,
    label: Option<String>,
    page: Option<(u32, u32, u32, u32)>,
    table: u64,
    notes: u64,
    excl: u64,
    eq: u32,
    grid: u32,
    chars: u32,
    vertical: bool,
    widen: Option<(usize, u32)>,
}

/// Memoised paragraph layouts.
#[derive(Default)]
pub struct LayoutCache {
    paras: HashMap<Key, Arc<ParaLayout>>,
    env: u64,
    used: HashSet<Key>,
    pub hits: u64,
    pub misses: u64,
}

impl LayoutCache {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.paras.len()
    }
    pub fn is_empty(&self) -> bool {
        self.paras.is_empty()
    }
    pub fn clear(&mut self) {
        self.paras.clear();
    }
}

fn hash_of<T: Hash>(t: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

fn env_hash(doc: &Document, opts: &LayoutOptions) -> u64 {
    let s = serde_json::to_string(&(
        &doc.styles,
        &doc.numbering,
        doc.settings.default_tab,
        &doc.settings.footnote_format,
        &doc.settings.char_spacing,
        &doc.settings.font_charsets,
        spacing_units::fixed_auto_spacing(doc),
    ))
    .unwrap_or_default();
    hash_of(&(s, opts.show_hidden, opts.hide_deleted, opts.proofing, kition_doc_proof::user_dictionary().len(), doc.settings.auto_hyphenation))
}

fn has_page_fields(p: &Paragraph) -> bool {
    p.objects.iter().any(|o| match o {
        InlineObject::Field { instr, code_fields, .. } => {
            let page_field = |instr: &str| matches!(fields::field_name(instr).as_str(), "PAGE" | "NUMPAGES" | "SECTIONPAGES" | "SECTION");
            page_field(instr) || (instr.trim_start().starts_with('=') && code_fields.iter().any(|f| page_field(&f.instr)))
        }
        _ => false,
    })
}

/// Walk state while paginating.
struct Ctx<'a> {
    doc: &'a Document,
    cache: &'a mut LayoutCache,
    opts: &'a LayoutOptions,
    counters: Counters,
    fields: FieldCtx,
    notes_hash: u64,
    numbers: HashMap<u32, Arc<ParaLayout>>,
    /// Automatic equation numbers so far.
    eq_count: u32,
    grid_pitch: Option<f32>,
    /// The section's character grid pitch, points (`w:docGrid` `linesAndChars`).
    char_pitch: Option<f32>,
    /// The section's text runs top to bottom (see `turned_frame`).
    vertical: bool,
    /// Bounds laying out text boxes inside text boxes, for the whole layout.
    boxes: kition_doc_doc::BoxBudget,
    table_starts: HashMap<usize, lookahead::TableStart>,
}

impl Ctx<'_> {
    /// A laid-out line number (cached).
    fn number_para(&mut self, n: u32) -> Arc<ParaLayout> {
        if let Some(p) = self.numbers.get(&n) {
            return p.clone();
        }
        // In the "Line Number" character style when the document has one, otherwise in the
        // document's default text formatting (WPS draws them so, in the text's colour).
        let style = self.doc.styles.find("line number").filter(|s| s.kind == kition_doc_doc::StyleKind::Character).map(|s| s.id.clone());
        let mut p = Paragraph::with_text(&n.to_string(), CharProps { style, ..Default::default() });
        p.props.space_after = Some(0.0);
        p.props.space_before = Some(0.0);
        let env = para::ParaEnv {
            doc: self.doc,
            width: 60.0,
            label: None,
            fields: &self.fields,
            show_hidden: false,
            hide_deleted: false,
            table: None,
            proofing: false,
            exclusions: &[],
            eq_number: 0,
            grid_pitch: None,
            char_pitch: None,
            vertical: false,
            widen: None,
        };
        let mut pl = para::layout_para(&p, &env);
        // No document paragraph holds the number: its clusters name the text they draw.
        pl.shown = pl
            .clusters
            .iter()
            .enumerate()
            .filter_map(|(i, c)| p.text.get(c.start..c.end).map(|t| (i, t.to_string(), vec![std::ops::Range { start: 0, end: t.len() }])))
            .collect();
        let pl = Arc::new(pl);
        self.numbers.insert(n, pl.clone());
        pl
    }

    /// The list label of `p`, advancing the list counters: call it once per paragraph, in
    /// document order (only for paragraphs that will show).
    fn next_label(&mut self, p: &Paragraph) -> Option<(String, Level)> {
        let n = p.props.numbering.or_else(|| self.doc.styles.resolve_para(&p.props).numbering).filter(|n| n.num != 0)?;
        self.counters.next_label(&self.doc.numbering, n.num, n.level)
    }

    /// Lay out `p` with its already counted `label`, so laying a paragraph out again (around
    /// floating objects, or after it moved) doesn't advance the list counters twice.
    fn para_labelled(
        &mut self,
        p: &Paragraph,
        width: f32,
        table: Option<&para::CellText>,
        exclusions: &[para::Exclusion],
        label: Option<(String, Level)>,
    ) -> Arc<ParaLayout> {
        self.para_widened(p, width, table, exclusions, label, None)
    }

    /// [`Ctx::para_labelled`], the lines from `widen.0` on `widen.1` wide (a paragraph
    /// continuing in a column of another width).
    fn para_widened(
        &mut self,
        p: &Paragraph,
        width: f32,
        table: Option<&para::CellText>,
        exclusions: &[para::Exclusion],
        label: Option<(String, Level)>,
        widen: Option<(usize, f32)>,
    ) -> Arc<ParaLayout> {
        let page = if has_page_fields(p) { Some(self.fields.page_key()) } else { None };
        // Automatic equation numbers count through the document.
        let eq_here: u32 = p
            .objects
            .iter()
            .map(|o| match o {
                InlineObject::Equation { math, display, .. } => kition_office_core::math::layout::auto_numbers(math, *display),
                _ => 0,
            })
            .sum();
        let eq_number = self.eq_count;
        self.eq_count = self.eq_count.saturating_add(eq_here);
        let key = Key {
            rev: p.rev,
            width: width.to_bits(),
            label: label.as_ref().map(|(t, l)| format!("{t}|{}|{}|{:?}|{:?}|{:?}|{:?}", l.indent, l.hanging, l.suffix, l.align, l.tab, l.chr)),
            page,
            table: table.map(|t| hash_of(&format!("{t:?}"))).unwrap_or(0),
            notes: if p.objects.iter().any(|o| matches!(o, InlineObject::NoteRef { .. })) { self.notes_hash } else { 0 },
            excl: if exclusions.is_empty() { 0 } else { hash_of(&format!("{exclusions:?}")) },
            eq: if eq_here > 0 { eq_number } else { 0 },
            grid: self.grid_pitch.unwrap_or(0.0).to_bits(),
            chars: self.char_pitch.unwrap_or(0.0).to_bits(),
            vertical: self.vertical,
            widen: widen.map(|(line, w)| (line, w.to_bits())),
        };
        self.cache.used.insert(key.clone());
        if let Some(pl) = self.cache.paras.get(&key) {
            self.cache.hits += 1;
            return pl.clone();
        }
        self.cache.misses += 1;
        let env = para::ParaEnv {
            doc: self.doc,
            width,
            label,
            fields: &self.fields,
            show_hidden: self.opts.show_hidden,
            hide_deleted: self.opts.hide_deleted,
            table,
            proofing: self.opts.proofing,
            exclusions,
            eq_number,
            grid_pitch: self.grid_pitch,
            char_pitch: self.char_pitch,
            vertical: self.vertical,
            widen,
        };
        let pl = Arc::new(para::layout_para(p, &env));
        self.cache.paras.insert(key, pl.clone());
        pl
    }
}

/// Which of `p`'s objects (by index) the layout leaves out ([`para::left_out`]): anchored in hidden
/// text unless `show_hidden`, or in a tracked deletion with `hide_deleted`. `table_chr` is the
/// cell's table-style character formatting ([`para::CellText::chr`]), resolved as the paragraph is
/// laid out with (`StyleSheet::resolve_char_in`).
fn left_out_objects(doc: &Document, p: &Paragraph, table_chr: Option<&CharProps>, show_hidden: bool, hide_deleted: bool) -> impl Fn(usize) -> bool {
    let mut left = Vec::new();
    // Nothing can be left out with hidden text shown and markup on.
    if (hide_deleted || !show_hidden) && !p.objects.is_empty() {
        let style = p.props.style.as_deref();
        let is_left = |c: &CharProps| {
            let rc = doc.styles.resolve_char_in(style, table_chr, c);
            para::left_out(&rc, show_hidden, hide_deleted)
        };
        // Runs and objects are both in order: one walk finds each object's run (past the last run,
        // the paragraph mark, as `props_of_char` gives), resolving a run's formatting only once.
        let mut runs = p.run_ranges().peekable();
        let mut last: Option<(usize, bool)> = None;
        for off in p.object_offsets() {
            while runs.next_if(|(r, _)| r.end <= off).is_some() {}
            let l = match runs.peek() {
                Some((r, c)) => match last {
                    Some((end, l)) if end == r.end => l,
                    _ => {
                        let l = is_left(c);
                        last = Some((r.end, l));
                        l
                    }
                },
                None => is_left(&p.mark),
            };
            left.push(l);
        }
    }
    move |k| left.get(k).copied().unwrap_or(false)
}

/// Note part ids in document order → numbers (footnotes and endnotes numbered separately). With
/// `hide_deleted`, notes whose reference mark is a tracked deletion are left out, as in the final text.
fn note_numbers(doc: &Document, hide_deleted: bool) -> HashMap<u32, u32> {
    let mut m = HashMap::new();
    let (mut f, mut e) = (0u32, 0u32);
    // Reading order, text boxes included where they are. A hidden reference mark still takes
    // its number, as in Word; a deleted one (or one in a deleted text box) does not.
    let mut cur: Option<(usize, Vec<bool>)> = None;
    doc.objects_in_reading_order_at(&mut |p, k| {
        let key = p as *const Paragraph as usize;
        if cur.as_ref().is_none_or(|(at, _)| *at != key) {
            let deleted = left_out_objects(doc, p, None, true, hide_deleted);
            cur = Some((key, (0..p.objects.len()).map(deleted).collect()));
        }
        if cur.as_ref().is_some_and(|(_, deleted)| deleted.get(k).copied().unwrap_or(false)) {
            return false;
        }
        if let Some(InlineObject::NoteRef { kind, id, .. }) = p.objects.get(k) {
            let n = match kind {
                kition_doc_doc::para::NoteKind::Footnote => {
                    f += 1;
                    f
                }
                kition_doc_doc::para::NoteKind::Endnote => {
                    e += 1;
                    e
                }
            };
            m.insert(*id, n);
        }
        true
    });
    m
}

/// Additional distance after the preceding paragraph's raw space-after was applied.
fn paragraph_spacing(current: &kition_doc_doc::resolve::ResolvedPara, previous: Option<(&str, bool, f32)>, before: f32) -> f32 {
    let Some((style, contextual, after)) = previous else { return before };
    let same = style == current.style;
    let effective_after = if same && contextual { 0.0 } else { after };
    let effective_before = if same && current.contextual_spacing { 0.0 } else { before };
    effective_before.max(effective_after) - after
}

/// Lay out a block list into a free-standing box of `width` (no page breaks): table cells,
/// headers, footers, notes, text boxes. Returns items relative to (0, 0) and the height. `frame`
/// says where the box sits on its page, for floating objects positioned relative to the page or
/// its margins; without it (cells, notes, text boxes) they're positioned in the box. Its lines,
/// text boxes' included, follow the section's line grid, as WPS for Mac spaces them.
#[allow(clippy::too_many_arguments)]
fn layout_box(
    ctx: &mut Ctx,
    story: StoryRef,
    blocks: &Blocks,
    prefix: &[u32],
    width: f32,
    table: Option<&para::CellText>,
    depth: usize,
    frame: Option<PageFrame>,
) -> BoxLayout {
    let mut items = Vec::new();
    // Pictures behind the text go first; `behind` counts them.
    let mut behind = 0;
    // Wrap areas of the box's floating objects.
    let mut excl: Vec<(Rect, para::Clear)> = Vec::new();
    // Areas of the box's floating tables, which other floating tables may have to avoid.
    let mut float_tables: Vec<Rect> = Vec::new();
    let mut y = 0.0f32;
    let mut prev_after = 0.0f32;
    let mut prev_style: Option<(String, bool)> = None;
    for (i, b) in blocks.iter().enumerate() {
        let mut path = prefix.to_vec();
        path.push(i as u32);
        match &**b {
            Block::Para(p) => {
                let label = ctx.next_label(p);
                let mut pl = ctx.para_labelled(p, width, table, &[], label.clone());
                let ctxl = pl.rp.contextual_spacing;
                let neighbors = (i.checked_sub(1).and_then(|index| blocks.get(index)).map(AsRef::as_ref), blocks.get(i + 1).map(AsRef::as_ref));
                // Only table-cell callers supply a path prefix; standalone stories
                // (headers, notes and text boxes) start with an empty path.
                let (before, after) = spacing_units::margins(ctx.doc, &pl.rp, neighbors, table, i == 0, !prefix.is_empty() && i + 1 == blocks.len());
                // Adjacent paragraph constraints overlap. Each contextual flag suppresses
                // only that paragraph's side of the gap (ISO 29500, 17.3.1.9 and 17.3.1.33).
                let before = paragraph_spacing(&pl.rp, prev_style.as_ref().map(|(s, c)| (s.as_str(), *c, prev_after)), before);
                let room = border_room(ctx.doc, &pl.rp, neighbors, table);
                let top = y;
                y += before + room.top;
                // Floating objects anchored here: place them, then wrap the text around them.
                // One left out of the layout (hidden, or deleted in the final text) takes no room.
                let mut floats = HashMap::new();
                let left_out = left_out_objects(ctx.doc, p, table.map(|t| &t.chr), ctx.opts.show_hidden, ctx.opts.hide_deleted);
                for (oi, o) in p.objects.iter().enumerate() {
                    let Some((w, h, float)) = floating(o) else { continue };
                    if left_out(oi) {
                        continue;
                    }
                    let w = rel_width(float, frame.map(|f| f.sect), w);
                    let (w, h) = placed_size(ctx, o, w, h, depth);
                    let r = float_rect(frame, (0.0, width), (top, y), w, h, float);
                    excl.extend(wrap_areas(r, float));
                    floats.insert(oi, r);
                }
                let rel = rel_exclusions(&excl, 0.0, y);
                if !rel.is_empty() {
                    pl = ctx.para_labelled(p, width, table, &rel, label);
                }
                let lead = pl.lines.first().map_or(0.0, |l| l.top.max(0.0));
                push_para(&mut items, story, &path, &pl, 0, pl.lines.len(), 0.0, y + lead, width, room);
                // Pictures, shapes and text boxes: drawn, their areas and text boxes' text.
                if !p.objects.is_empty() {
                    let at = ObjFrame { page: frame, col: (0.0, width), para_y: (top, y), table_chr: table.map(|t| &t.chr), shifts: &[] };
                    let (back, front) = place_objects(ctx, story, &path, p, &pl, (0, pl.lines.len()), (0.0, y + lead), &at, &floats, depth).flatten();
                    let at = behind.min(items.len());
                    behind += back.len();
                    items.splice(at..at, back);
                    items.extend(front);
                }
                y += pl.height + room.bottom + after;
                prev_after = after;
                prev_style = Some((pl.rp.style.clone(), ctxl));
            }
            Block::Table(t) => {
                if depth > 8 {
                    continue;
                }
                let tl = table::layout_table(ctx, story, t, &path, width, depth + 1);
                let (tx, ty) = match t.props.float {
                    // A floating table stands at its own position and takes no room; the text
                    // after it wraps around it.
                    Some(f) => {
                        let legacy = if ctx.doc.settings.compat_mode < 15 { table::first_cell_left_margin(ctx, t) } else { 0.0 };
                        let (r, area) = box_float_table_rect(frame, width, y, &tl, &f, legacy, &float_tables);
                        excl.push((area, para::Clear::Beside));
                        float_tables.push(area);
                        (r.x, r.y)
                    }
                    // A table in the text flow starts below the floating objects in its way.
                    None => (tl.x, below(&excl, y, tl.x, tl.x + tl.width)),
                };
                let mut row_y = ty;
                for row in &tl.rows {
                    for it in &row.items {
                        let mut it = it.clone();
                        it.translate(tx, row_y);
                        items.push(it);
                    }
                    row_y += row.height;
                }
                if t.props.float.is_none() {
                    y = row_y;
                    prev_after = 0.0;
                    prev_style = None;
                }
            }
        }
    }
    let floats_bottom = float_tables.iter().map(|r| r.bottom()).fold(0.0, f32::max);
    BoxLayout { items, height: y.max(0.0), floats_bottom }
}

/// A laid-out box: items relative to its top-left, the height of its text flow, and where its
/// floating tables end (a table cell grows to hold them).
struct BoxLayout {
    items: Vec<Placed>,
    height: f32,
    floats_bottom: f32,
}

/// Where floating table `tl` goes in a box `width` wide whose next paragraph starts at `y`, and
/// the area text keeps clear of. Positions relative to the page or its margins need the box's
/// `frame`; without one (a table cell) they're relative to the box.
fn box_float_table_rect(
    frame: Option<PageFrame>,
    width: f32,
    y: f32,
    tl: &table::TableLayout,
    f: &TableFloat,
    legacy: f32,
    float_tables: &[Rect],
) -> (Rect, Rect) {
    let h = tl.rows.iter().map(|r| r.height).sum::<f32>().clamp(0.0, 100_000.0);
    let w = tl.width.clamp(1.0, 100_000.0);
    let page_x = |s: &SectionProps| match f.h_rel {
        Anchor::Page => Some((0.0, s.page_w)),
        Anchor::Margin => Some((s.margin_left + s.gutter, s.text_width())),
        _ => None,
    };
    let page_y = |s: &SectionProps| match f.v_rel {
        Anchor::Page => Some((0.0, s.page_h)),
        Anchor::Margin => Some((s.margin_top, s.text_height())),
        _ => None,
    };
    let h_area = frame.and_then(|fr| page_x(fr.sect).map(|(a, e)| (a - fr.origin.0, e))).unwrap_or((0.0, width));
    // In a cell an offset from the page or margin has nothing to measure from: the table stands
    // where the text is.
    let (v_area, y_off) = match frame.and_then(|fr| page_y(fr.sect).map(|(a, e)| (a - fr.origin.1, e))) {
        Some(area) => (area, f.y),
        None if matches!(f.v_rel, Anchor::Page | Anchor::Margin) => ((y, 0.0), 0.0),
        None => ((y, 0.0), f.y),
    };
    // Nor is there an area to align in: the table starts where the text is.
    let v_align = if frame.is_none() && matches!(f.v_rel, Anchor::Page | Anchor::Margin) { None } else { f.v_align };
    let x = align_in(h_area, f.x, w, f.h_align) - if f.h_align.is_none() { legacy } else { 0.0 };
    let r = clear_of_float_tables(Rect::new(x, align_in(v_area, y_off, h, v_align), w, h), f, legacy, h_area.0, float_tables);
    let [dl, dt, dr, db] = f.dist_from_text();
    (r, r.inset(-dl, -dt, -dr, -db))
}

/// Floating table `f` at `r`, moved below the floating tables in its way when it may not
/// overlap them. In Word 2013+ layout one placed by an offset keeps its distance from the text's
/// left edge (`text_left`).
fn clear_of_float_tables(mut r: Rect, f: &TableFloat, legacy: f32, text_left: f32, others: &[Rect]) -> Rect {
    let [dl, dt, dr, db] = f.dist_from_text();
    let keeps_text_distance = legacy == 0.0 && f.h_align.is_none() && f.x >= 0.0 && matches!(f.h_rel, Anchor::Margin | Anchor::Column);
    for _ in 0..64 {
        let area = r.inset(-dl, -dt, -dr, -db);
        let Some(below) = others.iter().filter(|o| !f.overlap && o.intersects(&area)).map(|o| o.bottom()).reduce(f32::max) else { break };
        r.y = below + dt;
        if keeps_text_distance {
            r.x = r.x.max(text_left + dl);
        }
    }
    r
}

/// A text box's internal margins (Word's defaults: 0.1" left/right, 0.05" top/bottom).
/// A shape's text frame: its own, else the default (top, DrawingML's margins).
fn text_frame(obj: &InlineObject) -> TextFrame {
    match obj {
        InlineObject::Shape { body: Some(frame), .. } => *frame,
        _ => TextFrame::default(),
    }
}

/// Keep what fits in a box `height` tall (items relative to its top): lines that don't fit are
/// dropped (whole lines; a line that only partly fits too) and so is anything starting below.
/// The first line always stays, so even a tiny box shows the start of its text.
fn fit_box(items: Vec<Placed>, height: f32) -> Vec<Placed> {
    let limit = height + 0.5;
    let mut out = Vec::with_capacity(items.len());
    let mut kept_line = false;
    for it in items {
        if let Some(r) = it.turned_bounds() {
            // Turned text (in a table cell) stays whole.
            if r.y < limit {
                out.push(it);
            }
            continue;
        }
        match it {
            Placed::Lines { story, path, para, l0, l1, x, y, turn } => {
                let Some(first) = para.lines.get(l0) else { continue };
                let mut end = l0;
                for k in l0..l1 {
                    let Some(l) = para.lines.get(k) else { break };
                    // A line that starts inside the box is kept and drawn cut off at its edge;
                    // one that starts below it is hidden (as WPS draws them).
                    if y + (l.top - first.top) >= limit && kept_line {
                        break;
                    }
                    end = k + 1;
                    kept_line = true;
                }
                if end > l0 {
                    out.push(Placed::Lines { story, path, para, l0, l1: end, x, y, turn });
                }
            }
            Placed::Fill { rect, color } if rect.y < limit => {
                out.push(Placed::Fill { rect: Rect::new(rect.x, rect.y, rect.w, rect.h.min(limit - rect.y)), color });
            }
            Placed::Rule { x0, y0, x1, y1, border } if y0.min(y1) < limit => {
                out.push(Placed::Rule { x0, y0: y0.min(limit), x1, y1: y1.min(limit), border });
            }
            Placed::Image { rect, .. }
            | Placed::Shape { rect, .. }
            | Placed::Graphic { rect, .. }
            | Placed::Cell { rect, .. }
            | Placed::Object { rect, .. }
                if rect.y < limit =>
            {
                out.push(it);
            }
            _ => {}
        }
    }
    out
}

/// The room a paragraph's borders take above its first line and below its last, outside its
/// space before and after (WPS for Mac references). Paragraphs in a row with the same borders
/// share them: the first gets the top border's space and width, the last the bottom's, and between
/// them a between border takes its space on both sides plus its width.
#[derive(Clone, Copy, Debug, Default)]
struct BorderRoom {
    top: f32,
    bottom: f32,
    /// The paragraph before has the same borders.
    joined_above: bool,
    /// The paragraph after has the same borders.
    joined_below: bool,
}

fn border_room(
    doc: &Document,
    rp: &kition_doc_doc::resolve::ResolvedPara,
    neighbors: (Option<&Block>, Option<&Block>),
    table: Option<&para::CellText>,
) -> BorderRoom {
    let Some(borders) = rp.borders.as_ref() else { return BorderRoom::default() };
    let same = |block: Option<&Block>| match block {
        Some(Block::Para(other)) => doc.styles.resolve_para_in(&other.props, table.map(|t| &t.para)).borders.as_ref() == Some(borders),
        _ => false,
    };
    let band = |border: Option<Border>| border.filter(Border::is_visible).map_or(0.0, |b| (b.space + border_extent(&b)).clamp(0.0, 1000.0));
    let (joined_above, joined_below) = (same(neighbors.0), same(neighbors.1));
    let top = if joined_above {
        borders.between.filter(Border::is_visible).map_or(0.0, |b| (b.space * 2.0 + border_extent(&b)).clamp(0.0, 1000.0))
    } else {
        band(borders.top)
    };
    let bottom = if joined_below { 0.0 } else { band(borders.bottom) };
    BorderRoom { top, bottom, joined_above, joined_below }
}

/// How much room a paragraph border's line takes across: composite styles take more than their
/// width (WPS for Mac, synthetic calibration at 0.75 and 1.5 pt): a double line three widths, a
/// triple five, a wave 2.16 pt more and a double wave 4.56 pt more.
fn border_extent(b: &Border) -> f32 {
    use kition_doc_doc::props::BorderStyle;
    let w = b.width.max(0.0);
    match b.style {
        BorderStyle::Double => w * 3.0,
        BorderStyle::Triple => w * 5.0,
        BorderStyle::Wave => w + 2.16,
        BorderStyle::DoubleWave => w + 4.56,
        _ => w,
    }
}

/// Paragraph lines plus their shading and borders.
#[allow(clippy::too_many_arguments)]
fn push_para(
    items: &mut Vec<Placed>,
    story: StoryRef,
    path: &[u32],
    pl: &Arc<ParaLayout>,
    l0: usize,
    l1: usize,
    x: f32,
    y: f32,
    width: f32,
    room: BorderRoom,
) {
    let (Some(first), Some(last)) = (pl.lines.get(l0), l1.checked_sub(1).and_then(|k| pl.lines.get(k))) else {
        return;
    };
    let h = last.top + last.height - first.top;
    // Indents are logical: a right-to-left paragraph's start indent (and hanging indent) is on the right.
    let start = pl.rp.indent_left.min(pl.rp.indent_left + pl.rp.indent_first);
    let (li, ri) = if pl.rp.bidi { (pl.rp.indent_right, start) } else { (start, pl.rp.indent_right) };
    let left = x + li;
    let right = x + width - ri;
    if let Some(c) = pl.rp.shading {
        let pad = pl.rp.borders.as_ref().map(|b| b.left.map(|l| l.space).unwrap_or(4.0)).unwrap_or(0.0);
        items.push(Placed::Fill { rect: Rect::new(left - pad, y, right - left + pad * 2.0, h), color: c });
    }
    items.push(Placed::Lines { story, path: Path(path.to_vec()), para: pl.clone(), l0, l1, x, y, turn: TextDirection::Horizontal });
    if let Some(b) = &pl.rp.borders {
        let sp = |o: &Option<Border>| o.map(|b| b.space).unwrap_or(0.0);
        let (lx, rx) = (left - sp(&b.left), right + sp(&b.right));
        let (ty, by) = (y - sp(&b.top), y + h + sp(&b.bottom));
        // In a row of paragraphs with the same borders, the top border goes above the first and
        // the between border above the others; the bottom border below the last.
        let above = if room.joined_above { b.between } else { b.top };
        let ty = if room.joined_above { y - sp(&b.between) } else { ty };
        if let Some(t) = above.filter(|_| l0 == 0)
            && t.is_visible()
        {
            items.push(Placed::Rule { x0: lx, y0: ty, x1: rx, y1: ty, border: t });
        }
        if let Some(t) = b.bottom.filter(|_| l1 >= pl.lines.len() && !room.joined_below)
            && t.is_visible()
        {
            items.push(Placed::Rule { x0: lx, y0: by, x1: rx, y1: by, border: t });
        }
        if let Some(t) = b.left.filter(Border::is_visible) {
            items.push(Placed::Rule { x0: lx, y0: ty, x1: lx, y1: by, border: t });
        }
        if let Some(t) = b.right.filter(Border::is_visible) {
            items.push(Placed::Rule { x0: rx, y0: ty, x1: rx, y1: by, border: t });
        }
    }
}

struct PageBuilder<'a> {
    pages: Vec<Page>,
    /// Floating objects placed on each page (by page index), drawn in stacking order once the
    /// page is done ([`stack_floats`]).
    floats: HashMap<usize, Vec<Layered>>,
    sect: &'a SectionProps,
    sect_idx: usize,
    col: usize,
    cols: Vec<(f32, f32)>,
    y: f32,
    top: f32,
    bottom: f32,
    number: u32,
    /// The number the next page takes instead of counting on (a section that restarts its
    /// numbering, possibly at 0).
    next_number: Option<u32>,
    web: bool,
    /// Footnotes waiting for the bottom of the current page (first the rest of notes continued
    /// from the page before).
    notes: Vec<NoteBox>,
    /// The parts of footnotes that didn't fit on their page, continued on the next one.
    carry: Vec<NoteBox>,
    /// Page bottom before footnotes took space.
    orig_bottom: f32,
    /// The previous paragraph: (style, contextual spacing, space after) for contextual spacing.
    prev: Option<(String, bool, f32)>,
    /// Wrap areas of floating objects on this page (see [`wrap_areas`]).
    excl: Vec<(Rect, para::Clear)>,
    /// The wrap areas the paragraph being placed was laid out around (relative to it).
    para_excl: Vec<para::Exclusion>,
    /// The body's empty paragraphs that only end a section (by block): they take no room.
    marks: std::collections::HashSet<usize>,
    /// Where the floating objects on this page that may not overlap others stand, by anchor
    /// (block, object).
    apart: Vec<((usize, usize), Rect)>,
    /// A page (by index) whose columns end higher: the last page of a section being balanced
    /// (see [`balance_last_page`]).
    balance_at: Option<(usize, f32)>,
    /// Wrap areas of the floating tables on this page, which others may not overlap.
    float_tables: Vec<Rect>,
    /// Line numbering counter.
    line_no: u32,
    /// Index of the first body item of the current page (vertical alignment shifts from here).
    page_items_start: usize,
    /// The current section's first block.
    section_first: usize,
    /// The next section continues on the same page.
    next_continuous: bool,
    /// The current one-column layout began below the page's body top (a continuous section):
    /// its start is not a column top, so text that doesn't fit there moves to the next page.
    mid_page: bool,
    /// The page began at a page break that ended the previous paragraph.
    after_break: bool,
    /// Where the current section's footers start (default, even and first page): the body ends
    /// above a footer that reaches past the bottom margin.
    footer_tops: [f32; 3],
    /// Even pages take the even footer.
    even_odd: bool,
    /// The next page is the first of its section.
    section_start: bool,
    /// The columns placed on the current page since the page or a continuous section began
    /// there: the lines between them are drawn when they end.
    region: ColumnRegion,
    /// The vertical alignment of the section that started the current page: a page a next
    /// section's break ends keeps its own section's alignment.
    page_valign: kition_doc_doc::props::VAlign,
    /// Floating objects known from an earlier pass to reach back into an earlier column of their
    /// page (see [`PageBuilder::reach_back`]): their wrap areas join the page's from its start, so
    /// the text before their anchors wraps around them too, and they keep their places.
    seeds: ReachBack,
    /// Floating objects whose wrap areas reach into an earlier column of their page, by anchor
    /// (block, object).
    reach_back: ReachBack,
    /// How far the objects kept in place (see [`PageBuilder::seeds`]) are from where their
    /// anchors put them now, by anchor (block, object): where offsets from the anchor start for
    /// them.
    pin_shift: HashMap<(usize, usize), Point>,
}

/// A floating object reaching back into an earlier column of its page, by anchor (block, object
/// index): its page, its frame and its wrap area (and whether text only goes above and below).
type ReachBack = HashMap<(usize, usize), (usize, Rect, Rect, para::Clear)>;

/// Gap above the footnote separator and its length.
const NOTE_SEP: f32 = 12.0;
/// The most pages added after the body for footnotes still continuing.
const MAX_NOTE_PAGES: usize = 10_000;

/// A footnote waiting for the bottom of a page: its items laid out from y = 0, and its height.
#[derive(Clone)]
struct NoteBox {
    id: u32,
    items: Vec<Placed>,
    h: f32,
    /// The rest of a note that didn't fit on an earlier page (drawn below the continuation
    /// separator).
    cont: bool,
}

/// The vertical extent of an item laid out in a horizontal frame.
/// Columns on one page between where a page or a continuous section began and where it ended.
#[derive(Clone, Debug, Default)]
struct ColumnRegion {
    /// Lines between the columns are drawn (and there is more than one column).
    separator: bool,
    /// The left edge of the text area the column boxes are measured from.
    x: f32,
    cols: Vec<(f32, f32)>,
    top: f32,
    /// The bottom margin's edge.
    bottom: f32,
    /// The page's first item placed in the region.
    start: usize,
}

impl ColumnRegion {
    fn start(s: &SectionProps, top: f32, start: usize, web: bool) -> Self {
        ColumnRegion {
            separator: !web && s.columns.separator && s.columns.count > 1,
            x: s.margin_left + s.gutter,
            cols: s.column_boxes(),
            top,
            bottom: s.page_h - s.margin_bottom,
            start,
        }
    }
}

/// Text, tables and pictures of the body: what makes a column hold something.
/// Whether `it` is the line of a body paragraph that only ends a section (block in `marks`).
fn is_mark(marks: &std::collections::HashSet<usize>, it: &Placed) -> bool {
    matches!(it, Placed::Lines { story: StoryRef::Body, path, .. } if path.0.len() == 1 && path.0.first().is_some_and(|b| marks.contains(&(*b as usize))))
}

fn region_content(it: &Placed) -> bool {
    match it {
        Placed::Lines { story, .. } => *story == StoryRef::Body,
        Placed::Cell { .. } | Placed::Image { .. } | Placed::Shape { .. } | Placed::Graphic { .. } | Placed::Object { .. } => true,
        _ => false,
    }
}

/// The left edge of an item.
fn item_left(it: &Placed) -> Option<f32> {
    match it {
        Placed::Lines { x, .. } => Some(*x),
        Placed::Cell { rect, .. }
        | Placed::Image { rect, .. }
        | Placed::Shape { rect, .. }
        | Placed::Graphic { rect, .. }
        | Placed::Object { rect, .. } => Some(rect.x),
        _ => None,
    }
}

fn v_span(it: &Placed) -> (f32, f32) {
    if let Some(r) = it.turned_bounds() {
        return (r.y, r.y + r.h);
    }
    match it {
        Placed::Lines { para, l0, l1, y, .. } => (*y, item_bottom(*y, para, *l0, *l1).unwrap_or(*y)),
        Placed::Fill { rect, .. }
        | Placed::Image { rect, .. }
        | Placed::Shape { rect, .. }
        | Placed::Graphic { rect, .. }
        | Placed::Cell { rect, .. }
        | Placed::Object { rect, .. }
        | Placed::Warp { rect, .. }
        | Placed::Flat { rect, .. }
        | Placed::Clip { rect, .. } => (rect.y, rect.y + rect.h),
        Placed::Rule { y0, y1, .. } => (y0.min(*y1), y0.max(*y1)),
    }
}

impl NoteBox {
    /// Lines of the note's own paragraphs, which may be split between pages (not those of its
    /// tables or text boxes).
    fn splittable(&self, it: &Placed) -> bool {
        matches!(it, Placed::Lines { story: StoryRef::Part(s), path, turn: TextDirection::Horizontal, .. } if *s == self.id && path.0.len() == 1)
    }

    /// Where the note may be split, ascending: the bottoms of its lines that have more lines
    /// below them and don't cut through a picture, table or other unsplittable item.
    fn cuts(&self) -> Vec<f32> {
        let blocked: Vec<(f32, f32)> =
            self.items.iter().filter(|it| !self.splittable(it) && !matches!(it, Placed::Fill { .. } | Placed::Rule { .. })).map(v_span).collect();
        let mut last_top = f32::MIN;
        let mut cuts = Vec::new();
        for it in &self.items {
            let Placed::Lines { para, l0, l1, y, .. } = it else { continue };
            if !self.splittable(it) {
                continue;
            }
            let Some(first) = para.lines.get(*l0) else { continue };
            for l in para.lines.get(*l0..*l1).unwrap_or(&[]) {
                last_top = last_top.max(y + l.top - first.top);
                cuts.push(y + l.top - first.top + l.height);
            }
        }
        cuts.retain(|&b| b > 0.01 && b <= last_top + 0.01 && !blocked.iter().any(|&(t, e)| t < b - 0.01 && e > b + 0.01));
        cuts.sort_by(f32::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        cuts
    }

    /// The least of the note that must stay with its reference: its first line (all of it if it
    /// can't be split).
    fn first_piece(&self) -> f32 {
        self.cuts().first().copied().unwrap_or(self.h).min(self.h)
    }

    /// Split the note at line bottom `cut`: the part above (`cut` tall) and the rest, moved up to
    /// start at 0 and marked as a continuation.
    fn split(self, cut: f32) -> (NoteBox, NoteBox) {
        let (mut head, mut tail) = (Vec::new(), Vec::new());
        for it in self.items {
            let (top, bottom) = v_span(&it);
            if bottom <= cut + 0.01 {
                head.push(it);
                continue;
            }
            if top >= cut - 0.01 {
                tail.push(it);
                continue;
            }
            match it {
                Placed::Lines { story, path, para, l0, l1, x, y, turn } => {
                    let Some(first) = para.lines.get(l0) else { continue };
                    let ft = first.top;
                    // The first line that ends below the cut starts the tail.
                    let k = (l0..l1).find(|&k| para.lines.get(k).is_none_or(|l| y + l.top - ft + l.height > cut + 0.01)).unwrap_or(l1);
                    let ky = para.lines.get(k).map_or(y, |l| y + l.top - ft);
                    if k > l0 {
                        head.push(Placed::Lines { story, path: path.clone(), para: para.clone(), l0, l1: k, x, y, turn });
                    }
                    if k < l1 {
                        tail.push(Placed::Lines { story, path, para, l0: k, l1, x, y: ky, turn });
                    }
                }
                Placed::Fill { rect, color } => {
                    head.push(Placed::Fill { rect: Rect::new(rect.x, rect.y, rect.w, cut - rect.y), color });
                    tail.push(Placed::Fill { rect: Rect::new(rect.x, cut, rect.w, rect.y + rect.h - cut), color });
                }
                Placed::Rule { x0, y0, x1, y1, border } => {
                    head.push(Placed::Rule { x0, y0: y0.min(cut), x1, y1: y1.min(cut), border });
                    tail.push(Placed::Rule { x0, y0: y0.max(cut), x1, y1: y1.max(cut), border });
                }
                // Not split (cuts avoid them): stays above.
                other => head.push(other),
            }
        }
        // The rest starts at its first line (the space between paragraphs goes with the break).
        let start = tail.iter().filter(|it| matches!(it, Placed::Lines { .. })).map(|it| v_span(it).0).fold(f32::MAX, f32::min);
        let start = if start == f32::MAX { cut } else { start.max(cut) };
        for it in &mut tail {
            it.translate(0.0, -start);
            // Shading cut at the break starts at the rest's top.
            if let Placed::Fill { rect, .. } = it
                && rect.y < 0.0
            {
                rect.h = (rect.h + rect.y).max(0.0);
                rect.y = 0.0;
            }
        }
        let rest = (self.h - start).max(0.0);
        (NoteBox { id: self.id, items: head, h: cut, cont: self.cont }, NoteBox { id: self.id, items: tail, h: rest, cont: true })
    }
}

impl PageBuilder<'_> {
    /// Space footnotes take at the bottom of the page.
    fn notes_h(&self) -> f32 {
        if self.notes.is_empty() { 0.0 } else { NOTE_SEP + self.notes.iter().map(|n| n.h).sum::<f32>() }
    }
    /// The lowest bottom of the body text and tables on the current page.
    fn body_floor(&self) -> f32 {
        let Some(pg) = self.pages.last() else { return self.top };
        pg.items
            .iter()
            .skip(self.page_items_start)
            .filter(|it| matches!(it, Placed::Lines { story: StoryRef::Body, .. } | Placed::Cell { story: StoryRef::Body, .. }))
            .map(|it| v_span(it).1)
            .fold(self.top, f32::max)
    }
    /// Place the collected footnotes at the bottom of the current page. When they don't all fit
    /// below the body (`bottom`), the note that doesn't is split at a line and its rest, with
    /// the notes after it, continues on the next page.
    fn flush_notes(&mut self) {
        if self.notes.is_empty() {
            return;
        }
        let total = self.notes_h();
        let x = self.sect.margin_left + self.sect.gutter;
        // The note area starts below the text (a page of continued notes alone: at its top).
        let area = self.bottom.max(self.body_floor());
        let fits = total <= self.orig_bottom - area + 0.01;
        let top = if fits { self.orig_bottom - total } else { area };
        let mut y = top + NOTE_SEP;
        let notes = std::mem::take(&mut self.notes);
        // A page that starts with a continued note gets the full-width continuation separator.
        let cont = notes.first().is_some_and(|n| n.cont);
        let mut placed = Vec::new();
        let mut rest = notes.into_iter();
        while let Some(n) = rest.next() {
            let room = self.orig_bottom - y;
            if fits || n.h <= room + 0.01 {
                let ny = y;
                y += n.h;
                placed.push((n, ny));
                continue;
            }
            // Split at the last line that fits. The page's first note always leaves something
            // here (its first line, or all of it if it can't be split), so continuing ends.
            let first = placed.is_empty();
            let cuts = n.cuts();
            match cuts.iter().rev().find(|&&c| c <= room + 0.01).or(cuts.first().filter(|_| first)).copied() {
                Some(c) => {
                    let (head, tail) = n.split(c);
                    placed.push((head, y));
                    self.carry.push(tail);
                }
                None if first => placed.push((n, y)),
                None => self.carry.push(n),
            }
            self.carry.extend(rest.by_ref());
            break;
        }
        if let Some(pg) = self.pages.last_mut() {
            let len = if cont { self.sect.text_width() } else { 144.0 };
            let sy = top + NOTE_SEP / 2.0;
            pg.items.push(Placed::Rule { x0: x, y0: sy, x1: x + len, y1: sy, border: Border::single(0.5) });
            for (n, ny) in placed {
                for mut it in n.items {
                    it.translate(x, ny);
                    pg.items.push(it);
                }
            }
        }
    }
    /// Section vertical alignment: move the page's body down (centre/bottom).
    fn apply_valign(&mut self) {
        let k = match self.page_valign {
            kition_doc_doc::props::VAlign::Top => return,
            kition_doc_doc::props::VAlign::Center => 0.5,
            kition_doc_doc::props::VAlign::Bottom => 1.0,
        };
        if self.web {
            return;
        }
        let space = (self.orig_bottom - self.notes_h() - self.y).max(0.0) * k;
        let start = self.page_items_start;
        if let Some(pg) = self.pages.last_mut() {
            for it in pg.items.iter_mut().skip(start) {
                it.translate(0.0, space);
            }
        }
    }
    fn new_page(&mut self, first_block: usize, body_top: f32) {
        self.end_region(None);
        self.apply_valign();
        self.flush_notes();
        self.excl.clear();
        let page = self.pages.len();
        let mut seeds: Vec<(Rect, para::Clear)> = self.seeds.values().filter(|s| s.0 == page).map(|s| (s.2, s.3)).collect();
        seeds.sort_by(|a, b| a.0.y.total_cmp(&b.0.y).then(a.0.x.total_cmp(&b.0.x)));
        self.excl.extend(seeds);
        self.float_tables.clear();
        self.apart.clear();
        if self.sect.line_numbers.as_ref().is_some_and(|l| l.restart == kition_doc_doc::section::LineNumberRestart::Page) {
            self.line_no = 0;
        }
        let s = self.sect;
        self.number = self.next_number.take().unwrap_or_else(|| self.number.saturating_add(1));
        let (w, h) = if self.web { (s.page_w, f32::MAX / 4.0) } else { (s.page_w, s.page_h) };
        let body = Rect::new(s.margin_left + s.gutter, body_top, s.text_width(), (s.page_h - s.margin_bottom - body_top).max(36.0));
        let mut decor = Vec::new();
        if !self.web {
            // Page borders, measured from the page edge (Word's default), so they surround the
            // header, footer and line numbers.
            if let Some(b) = &s.page_borders {
                let sp = |e: &Option<Border>| e.map(|x| x.space).unwrap_or(24.0);
                let (x0, x1, y0, y1) = (sp(&b.left), s.page_w - sp(&b.right), sp(&b.top), s.page_h - sp(&b.bottom));
                for (e, a, bb) in
                    [(b.top, (x0, y0), (x1, y0)), (b.bottom, (x0, y1), (x1, y1)), (b.left, (x0, y0), (x0, y1)), (b.right, (x1, y0), (x1, y1))]
                {
                    if let Some(e) = e.filter(Border::is_visible) {
                        decor.push(Placed::Rule { x0: a.0, y0: a.1, x1: bb.0, y1: bb.1, border: e });
                    }
                }
            }
        }
        self.page_items_start = decor.len();
        self.region = ColumnRegion::start(s, body_top, decor.len(), self.web);
        self.page_valign = s.valign;
        self.pages.push(Page { w, h, section: self.sect_idx, number: self.number, body, first_block, items: decor, ..Default::default() });
        self.col = 0;
        self.cols = s.column_boxes();
        self.top = body_top;
        self.mid_page = false;
        // Word ends the body above a footer that reaches past the bottom margin, no gap.
        let footer_top = match (std::mem::take(&mut self.section_start) && s.title_page, self.even_odd && self.number.is_multiple_of(2)) {
            (true, _) => self.footer_tops[2],
            (false, true) => self.footer_tops[1],
            (false, false) => self.footer_tops[0],
        };
        self.bottom = if self.web { f32::MAX / 8.0 } else { (s.page_h - s.margin_bottom).min(footer_top).max(body_top + 36.0) };
        if let Some((_, bottom)) = self.balance_at.filter(|(at, _)| *at + 1 == self.pages.len()) {
            self.bottom = self.bottom.min(bottom).max(body_top + 1.0);
        }
        self.orig_bottom = self.bottom;
        self.y = self.top;
        // Notes continued from the page before come first in this page's note area.
        if !self.carry.is_empty() && !self.web {
            self.notes = std::mem::take(&mut self.carry);
            self.bottom -= self.notes_h();
        }
    }
    /// Ends the current page's column region: lines between its columns (Layout › Columns › Line
    /// between), each beside a column that holds something, as WPS for Mac draws them. A region
    /// the page ends runs from its top to the bottom margin; one a continuous section break ends
    /// (`Some(bottom)`), to the bottom of its tallest column.
    fn end_region(&mut self, continued: Option<f32>) {
        let region = std::mem::take(&mut self.region);
        if !region.separator {
            return;
        }
        let Some(page) = self.pages.last_mut() else { return };
        let items = page.items.get(region.start..).unwrap_or(&[]);
        let bottom = match continued {
            // The columns' text: below it, the space after their last paragraphs and the mark
            // ending the section take no part (WPS for Mac).
            Some(y) => items.iter().filter(|it| region_content(it) && !is_mark(&self.marks, it)).map(|it| v_span(it).1).reduce(f32::max).unwrap_or(y),
            None => region.bottom,
        };
        let mut rules = Vec::new();
        for pair in region.cols.windows(2) {
            let [(ax, aw), (bx, bw)] = pair else { continue };
            let (left, right) = (region.x + bx, region.x + bx + bw);
            let filled = items.iter().filter(|it| region_content(it)).any(|it| item_left(it).is_some_and(|x| x >= left - 0.5 && x < right));
            if filled && bottom > region.top {
                let x = region.x + (ax + aw + bx) / 2.0;
                rules.push(Placed::Rule { x0: x, y0: region.top, x1: x, y1: bottom, border: Border::single(0.5) });
            }
        }
        page.items.extend(rules);
    }
    fn col_x(&self) -> f32 {
        self.sect.margin_left + self.sect.gutter + self.cols.get(self.col).map(|c| c.0).unwrap_or(0.0)
    }
    fn col_w(&self) -> f32 {
        self.cols.get(self.col).map(|c| c.1).unwrap_or_else(|| self.sect.text_width())
    }
    fn at_top(&self) -> bool {
        self.y <= self.top + 0.01 && !(self.mid_page && self.cols.len() <= 1)
    }
    /// Move to the next column or page.
    fn advance(&mut self, block: usize, body_top: f32) {
        if self.col + 1 < self.cols.len() {
            self.col += 1;
            self.y = self.top;
        } else {
            self.new_page(block, body_top);
        }
    }
    fn page(&mut self) -> Option<&mut Page> {
        self.pages.last_mut()
    }

    /// The state of the current page, to place a section on it again (see [`balance_columns`]).
    fn snapshot(&self) -> PageSnapshot {
        let page = self.pages.len().checked_sub(1);
        PageSnapshot {
            pages: self.pages.len(),
            page: page.and_then(|i| self.pages.get(i)).cloned(),
            floats: page.and_then(|i| self.floats.get(&i)).cloned(),
            notes: self.notes.clone(),
            carry: self.carry.clone(),
            excl: self.excl.clone(),
            float_tables: self.float_tables.clone(),
            apart: self.apart.clone(),
            prev: self.prev.clone(),
            col: self.col,
            y: self.y,
            top: self.top,
            bottom: self.bottom,
            orig_bottom: self.orig_bottom,
            number: self.number,
            line_no: self.line_no,
            page_items_start: self.page_items_start,
            mid_page: self.mid_page,
            after_break: self.after_break,
            region: self.region.clone(),
            page_valign: self.page_valign,
        }
    }

    fn restore(&mut self, s: &PageSnapshot) {
        self.pages.truncate(s.pages);
        if let (Some(page), Some(last)) = (&s.page, self.pages.last_mut()) {
            *last = page.clone();
        }
        let current = s.pages.saturating_sub(1);
        self.floats.retain(|k, _| *k < current);
        if let Some(f) = &s.floats {
            self.floats.insert(current, f.clone());
        }
        self.notes = s.notes.clone();
        self.carry = s.carry.clone();
        self.excl = s.excl.clone();
        self.float_tables = s.float_tables.clone();
        self.apart = s.apart.clone();
        self.prev = s.prev.clone();
        (self.col, self.y, self.top, self.bottom, self.orig_bottom) = (s.col, s.y, s.top, s.bottom, s.orig_bottom);
        (self.number, self.line_no, self.page_items_start, self.mid_page) = (s.number, s.line_no, s.page_items_start, s.mid_page);
        self.after_break = s.after_break;
        self.region = s.region.clone();
        self.page_valign = s.page_valign;
    }
}

/// [`PageBuilder`] state on one page, before a section was placed there.
struct PageSnapshot {
    pages: usize,
    page: Option<Page>,
    floats: Option<Vec<Layered>>,
    notes: Vec<NoteBox>,
    carry: Vec<NoteBox>,
    excl: Vec<(Rect, para::Clear)>,
    float_tables: Vec<Rect>,
    apart: Vec<((usize, usize), Rect)>,
    prev: Option<(String, bool, f32)>,
    col: usize,
    y: f32,
    top: f32,
    bottom: f32,
    orig_bottom: f32,
    number: u32,
    line_no: u32,
    page_items_start: usize,
    mid_page: bool,
    after_break: bool,
    region: ColumnRegion,
    page_valign: kition_doc_doc::props::VAlign,
}

/// The walk state a section's placement changes besides the page.
struct WalkSnapshot {
    counters: kition_doc_doc::numbering::Counters,
    fields: FieldCtx,
    eq_count: u32,
}

impl Ctx<'_> {
    fn snapshot(&self) -> WalkSnapshot {
        WalkSnapshot { counters: self.counters.clone(), fields: self.fields.clone(), eq_count: self.eq_count }
    }
    fn restore(&mut self, s: &WalkSnapshot) {
        self.counters = s.counters.clone();
        self.fields = s.fields.clone();
        self.eq_count = s.eq_count;
    }
}

/// Places blocks `first..=last` of a section.
fn place_blocks(ctx: &mut Ctx, pb: &mut PageBuilder, first: usize, last: usize, body_top: f32) {
    for block in first..=last {
        let Some(b) = ctx.doc.body.get(block) else { break };
        match &**b {
            Block::Para(p) => place_para(ctx, pb, p, block, last, body_top),
            Block::Table(t) => place_table(ctx, pb, t, block, body_top),
        }
    }
}

/// Paragraph splits on the current page (in its items from `from` on) that break the paragraph's
/// own rules: a paragraph kept together split anyway, or a one-line piece of a paragraph under
/// widow control. Splits at explicit column and page breaks are the paragraph's own.
fn broken_splits(pb: &PageBuilder, from: usize) -> usize {
    let Some(page) = pb.pages.last() else { return 0 };
    let mut pieces: HashMap<&Path, Vec<(&ParaLayout, usize, usize)>> = HashMap::new();
    for it in page.items.iter().skip(from) {
        if let Placed::Lines { story: StoryRef::Body, path, para, l0, l1, .. } = it {
            pieces.entry(path).or_default().push((para, *l0, *l1));
        }
    }
    let mut broken = 0;
    for parts in pieces.values().filter(|parts| parts.len() > 1) {
        for pair in parts.windows(2) {
            let [(para, a0, a1), (_, b0, b1)] = pair else { continue };
            let forced =
                a1.checked_sub(1).and_then(|k| para.lines.get(k)).is_some_and(|l| matches!(l.end, LineEnd::ColumnBreak | LineEnd::PageBreak));
            if forced {
                continue;
            }
            let single = a1.saturating_sub(*a0) == 1 || b1.saturating_sub(*b0) == 1;
            if para.rp.keep_lines || (para.rp.widow_control && para.lines.len() >= 2 && single) {
                broken += 1;
            }
        }
    }
    broken
}

/// Columns before a continuous section break are balanced: a multi-column section that ends on
/// the page it started on is placed again with the lowest column bottom that still holds it on
/// that page, so its columns end level, and the next section starts below the tallest one
/// (ECMA-376 Part 1, §17.6.22: the next section continues on the same page). The section's own
/// placement rules (keep with next, widow control, column breaks) apply at every trial bottom.
fn balance_columns(ctx: &mut Ctx, pb: &mut PageBuilder, first: usize, last: usize, body_top: f32, page: &PageSnapshot, walk: &WalkSnapshot) {
    if pb.cols.len() < 2 || pb.web {
        return;
    }
    if pb.pages.len() != page.pages {
        balance_last_page(ctx, pb, first, last, body_top, page, walk);
        return;
    }
    let items = page.page.as_ref().map_or(0, |p| p.items.len());
    // A lower bottom may only split paragraphs as their own rules allow (as the section's
    // unbalanced placement did): never a paragraph kept together, nor leaving a single line of
    // a paragraph under widow control.
    let allowed = broken_splits(pb, items);
    let fits = |ctx: &mut Ctx, pb: &mut PageBuilder, bottom: f32| {
        pb.restore(page);
        ctx.restore(walk);
        pb.bottom = bottom.min(page.bottom);
        place_blocks(ctx, pb, first, last, body_top);
        pb.pages.len() == page.pages && broken_splits(pb, items) <= allowed
    };
    // The lowest bottom that holds the section on this page, to half a point.
    let (mut lo, mut hi) = (page.y, page.bottom);
    for _ in 0..24 {
        if hi - lo <= 0.5 {
            break;
        }
        let mid = (lo + hi) / 2.0;
        if fits(ctx, pb, mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    if !fits(ctx, pb, hi) {
        // Only possible if placement isn't monotonic in the bottom: place it as before.
        pb.restore(page);
        ctx.restore(walk);
        place_blocks(ctx, pb, first, last, body_top);
        return;
    }
    // The section ends below its tallest column; the empty paragraph that only ends it takes no
    // room there either (see `place_para`).
    let tallest = pb
        .pages
        .last()
        .map(|p| p.items.iter().skip(items).filter(|it| !is_mark(&pb.marks, it)).map(|it| v_span(it).1).fold(pb.y, f32::max))
        .unwrap_or(pb.y);
    pb.bottom += page.bottom - hi;
    pb.y = tallest;
}

/// [`balance_columns`] for a section that runs on to later pages: its last page is balanced,
/// the section placed again from its start with that page's columns ending at the lowest bottom
/// that still holds the rest of it there (WPS for Mac: luan's four-column section).
fn balance_last_page(ctx: &mut Ctx, pb: &mut PageBuilder, first: usize, last: usize, body_top: f32, page: &PageSnapshot, walk: &WalkSnapshot) {
    let target = pb.pages.len().saturating_sub(1);
    let natural = pb.bottom;
    let top = pb.top;
    let allowed = broken_splits(pb, 0);
    let fits = |ctx: &mut Ctx, pb: &mut PageBuilder, bottom: f32| {
        pb.restore(page);
        ctx.restore(walk);
        pb.balance_at = Some((target, bottom));
        place_blocks(ctx, pb, first, last, body_top);
        pb.balance_at = None;
        pb.pages.len() == target + 1 && broken_splits(pb, 0) <= allowed
    };
    let (mut lo, mut hi) = (top, natural);
    for _ in 0..24 {
        if hi - lo <= 0.5 {
            break;
        }
        let mid = (lo + hi) / 2.0;
        if fits(ctx, pb, mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    if !fits(ctx, pb, hi) {
        pb.restore(page);
        ctx.restore(walk);
        place_blocks(ctx, pb, first, last, body_top);
        return;
    }
    let tallest = pb
        .pages
        .last()
        .map(|p| p.items.iter().filter(|it| region_content(it) && !is_mark(&pb.marks, it)).map(|it| v_span(it).1).fold(pb.y, f32::max))
        .unwrap_or(pb.y);
    pb.bottom += natural - hi;
    pb.y = tallest;
}

/// Lay out the whole document.
pub fn layout(doc: &Document, cache: &mut LayoutCache, opts: &LayoutOptions) -> DocLayout {
    let t0 = now_ms();
    let (first, reach) = layout_pass(doc, cache, opts, ReachBack::new());
    if reach.is_empty() {
        return first;
    }
    // A floating object reaching back into an earlier column of its page, or above its
    // paragraph: text there wraps around it too, as Word lays the page out again with the
    // object where it is. Objects that then land elsewhere (another page or column) are let go
    // and the pages laid out again, a few times at most; if none settle, the first layout
    // stands. Objects newly reaching back are left for the next edit to settle.
    let mut seeds = reach;
    for _ in 0..MAX_REACH_BACK_PASSES {
        let (next, again) = layout_pass(doc, cache, opts, seeds.clone());
        let settled: ReachBack = seeds
            .iter()
            .filter(|(key, (page, rect, _, _))| again.get(key).is_some_and(|(p, r, _, _)| p == page && r == rect))
            .map(|(k, v)| (*k, *v))
            .collect();
        if settled.len() == seeds.len() {
            return DocLayout { ms: now_ms() - t0, ..next };
        }
        if settled.is_empty() {
            break;
        }
        seeds = settled;
    }
    DocLayout { ms: now_ms() - t0, ..first }
}

/// How many times the pages are laid out again for floating objects reaching back (see
/// [`layout`]).
const MAX_REACH_BACK_PASSES: usize = 3;

/// One layout of the document, with wrap areas `seeds` known on their pages before their text;
/// with the floating objects reaching back into earlier columns (see [`PageBuilder::reach_back`]).
fn layout_pass(doc: &Document, cache: &mut LayoutCache, opts: &LayoutOptions, seeds: ReachBack) -> (DocLayout, ReachBack) {
    let t0 = now_ms();
    let env = env_hash(doc, opts);
    if env != cache.env {
        cache.paras.clear();
        cache.env = env;
    }
    cache.used.clear();
    let notes = note_numbers(doc, opts.hide_deleted);
    let notes_hash = hash_of(&{
        let mut v: Vec<_> = notes.iter().map(|(a, b)| (*a, *b)).collect();
        v.sort();
        v
    });
    let mut ctx = Ctx {
        doc,
        cache,
        opts,
        counters: Counters::default(),
        fields: FieldCtx {
            page: 1,
            pages: 1,
            section_pages: 1,
            section: 1,
            notes: Arc::new(notes),
            title: doc.core.title.as_str().into(),
            author: doc.core.creator.as_str().into(),
            ..Default::default()
        },
        notes_hash,
        numbers: HashMap::new(),
        eq_count: 0,
        grid_pitch: None,
        char_pitch: None,
        vertical: false,
        boxes: kition_doc_doc::BoxBudget::default(),
        table_starts: HashMap::new(),
    };
    let sections = doc.sections();
    let web = opts.view != ViewMode::Print;
    let default_sect = SectionProps::default();
    let mut web_sect;
    let first_sect = match sections.first() {
        Some((_, s)) => *s,
        None => &default_sect,
    };
    let sect_ref = if web {
        web_sect = first_sect.clone();
        web_sect.page_w = opts.web_width.max(144.0);
        web_sect.margin_left = 18.0;
        web_sect.margin_right = 18.0;
        web_sect.margin_top = 18.0;
        web_sect.gutter = 0.0;
        web_sect.columns = Default::default();
        &web_sect
    } else {
        first_sect
    };
    // Turned sections are laid out on their page turned upright (see `turned_frame`).
    let frames: Vec<Option<SectionProps>> = sections.iter().map(|(_, s)| (!web).then(|| turned_frame(s)).flatten()).collect();
    let mut pb = PageBuilder {
        pages: Vec::new(),
        floats: HashMap::new(),
        sect: sect_ref,
        sect_idx: 0,
        col: 0,
        cols: Vec::new(),
        y: 0.0,
        top: 0.0,
        bottom: 0.0,
        number: 0,
        next_number: None,
        web,
        notes: Vec::new(),
        carry: Vec::new(),
        orig_bottom: 0.0,
        prev: None,
        excl: Vec::new(),
        para_excl: Vec::new(),
        apart: Vec::new(),
        balance_at: None,
        marks: doc
            .body
            .iter()
            .enumerate()
            .filter(|(_, b)| matches!(&***b, Block::Para(p) if p.section.is_some() && p.text.is_empty()))
            .map(|(i, _)| i)
            .collect(),
        float_tables: Vec::new(),
        line_no: 0,
        page_items_start: 0,
        section_first: 0,
        next_continuous: false,
        mid_page: false,
        after_break: false,
        footer_tops: [f32::MAX; 3],
        even_odd: doc.settings.even_odd_headers,
        section_start: false,
        region: ColumnRegion::default(),
        page_valign: kition_doc_doc::props::VAlign::Top,
        seeds,
        reach_back: HashMap::new(),
        pin_shift: HashMap::new(),
    };
    let mut block = 0usize;
    for (si, (end, sect)) in sections.iter().enumerate() {
        let frame = frames.get(si).and_then(Option::as_ref);
        let sect: &SectionProps = if web { sect_ref } else { frame.unwrap_or(sect) };
        pb.sect = sect;
        pb.sect_idx = si;
        ctx.grid_pitch = if web { None } else { sect.grid.as_ref().and_then(|grid| grid.line_pitch_points()) };
        // Characters snap to the grid by the default paragraph style's size.
        let base = doc.styles.resolve_char_in(None, None, &CharProps::default()).size;
        ctx.char_pitch = if web { None } else { sect.grid.as_ref().and_then(|grid| grid.char_pitch_points(base)) };
        ctx.vertical = frame.is_some();
        // A turned section's headers and footers stay upright, clear of its turned body.
        let body_top = if web || frame.is_some() { sect.margin_top } else { body_top_for(&mut ctx, sect, sect.headers.default) };
        let first_top = if web || frame.is_some() || !sect.title_page { body_top } else { body_top_for(&mut ctx, sect, sect.headers.first) };
        if frame.is_some() {
            pb.footer_tops = [f32::MAX; 3];
        } else if !web {
            // The footers this section's pages show (inherited from earlier sections as Word links them).
            let mut footers = sect.footers;
            for (_, earlier) in sections.get(..si).into_iter().flatten().rev() {
                footers.default = footers.default.or(earlier.footers.default);
                footers.first = footers.first.or(earlier.footers.first);
                footers.even = footers.even.or(earlier.footers.even);
            }
            pb.footer_tops = [
                footer_top_for(&mut ctx, sect, footers.default),
                footer_top_for(&mut ctx, sect, footers.even.or(footers.default)),
                footer_top_for(&mut ctx, sect, footers.first),
            ];
        }
        pb.section_start = true;
        let restart = sect.page_num_start;
        if sect.line_numbers.as_ref().is_some_and(|l| l.restart == kition_doc_doc::section::LineNumberRestart::Section) {
            pb.line_no = 0;
        }
        let start = if si == 0 { SectionStart::NextPage } else { sect.start };
        if web && si > 0 {
            // one long page
        } else if pb.pages.is_empty() || start != SectionStart::Continuous {
            if restart.is_some() {
                pb.next_number = restart;
            }
            pb.new_page(block, first_top);
            if !web && matches!(start, SectionStart::EvenPage | SectionStart::OddPage) {
                let want_even = start == SectionStart::EvenPage;
                if pb.number.is_multiple_of(2) != want_even {
                    if let Some(page) = pb.pages.last_mut() {
                        page.blank = true;
                    }
                    pb.new_page(block, body_top);
                }
            }
        } else if pb.y >= pb.bottom - 0.01 {
            // Continuous, but the page is full: the section starts the next page.
            pb.new_page(block, body_top);
        } else {
            // Continuous: new column layout below the current text.
            let y = pb.y;
            pb.end_region(Some(y));
            let start = pb.pages.last().map_or(0, |page| page.items.len());
            pb.region = ColumnRegion::start(sect, y, start, web);
            pb.cols = sect.column_boxes();
            pb.col = 0;
            pb.mid_page = pb.y > pb.pages.last().map_or(pb.y, |p| p.body.y) + 0.01;
            pb.top = pb.y;
        }
        let last = (*end).min(doc.body.len().saturating_sub(1));
        pb.section_first = block;
        pb.next_continuous = !web && sections.get(si + 1).is_some_and(|(_, next)| next.start == SectionStart::Continuous);
        let balance = !web && sect.columns.count > 1 && sections.get(si + 1).is_some_and(|(_, next)| next.start == SectionStart::Continuous);
        let before = balance.then(|| (pb.snapshot(), ctx.snapshot()));
        if block <= last {
            place_blocks(&mut ctx, &mut pb, block, last, body_top);
            if let Some((page, walk)) = &before {
                balance_columns(&mut ctx, &mut pb, block, last, body_top, page, walk);
            }
        }
        block = block.max(last + 1);
    }
    if pb.pages.is_empty() {
        pb.new_page(0, sect_ref.margin_top);
    }
    // Endnotes after the last paragraph.
    let endnotes: Vec<u32> = {
        let mut ids: Vec<(u32, u32)> = ctx
            .fields
            .notes
            .iter()
            .filter(|(id, _)| doc.parts.get(id).is_some_and(|p| p.kind == kition_doc_doc::PartKind::Endnote))
            .map(|(a, b)| (*b, *a))
            .collect();
        ids.sort();
        ids.into_iter().map(|(_, id)| id).collect()
    };
    if !endnotes.is_empty() && !web {
        pb.y += 12.0;
        let (x, ry) = (pb.col_x(), pb.y);
        if let Some(pg) = pb.page() {
            pg.items.push(Placed::Rule {
                x0: x,
                y0: ry,
                x1: x + 144.0,
                y1: ry,
                border: Border { style: kition_doc_doc::props::BorderStyle::Single, width: 0.5, color: None, space: 0.0, theme: None },
            });
        }
        pb.y += 8.0;
        for id in endnotes {
            let Some(part) = doc.parts.get(&id) else { continue };
            let blocks = part.blocks.clone();
            let BoxLayout { items, height: h, .. } = layout_box(&mut ctx, StoryRef::Part(id), &blocks, &[], pb.col_w(), None, 0, None);
            if pb.y + h > pb.bottom && !pb.at_top() {
                pb.advance(block, pb.top);
            }
            let (x, y) = (pb.col_x(), pb.y);
            if let Some(pg) = pb.page() {
                for mut it in items {
                    it.translate(x, y);
                    pg.items.push(it);
                }
            }
            pb.y += h;
        }
    }
    pb.end_region(None);
    pb.apply_valign();
    pb.flush_notes();
    // Footnotes still continuing after the last page get pages of their own. Each page takes at
    // least a line of the first note, so this ends; the cap is a backstop.
    let last_block = doc.body.len().saturating_sub(1);
    for _ in 0..MAX_NOTE_PAGES {
        if pb.carry.is_empty() {
            break;
        }
        let top = pb.top;
        pb.new_page(last_block, top);
        pb.flush_notes();
    }
    pb.carry.clear();
    let reach_back = std::mem::take(&mut pb.reach_back);
    let mut pages = pb.pages;
    for (i, floats) in pb.floats {
        if let Some(page) = pages.get_mut(i) {
            stack_floats(page, floats);
        }
    }
    for page in &mut pages {
        if let Some((_, sect)) = sections.get(page.section).filter(|_| frames.get(page.section).is_some_and(Option::is_some)) {
            turn_page(page, sect);
        }
    }
    if web && let Some(p) = pages.first_mut() {
        let bottom = p
            .items
            .iter()
            .filter_map(|it| if let Placed::Lines { y, para, l0, l1, .. } = it { item_bottom(*y, para, *l0, *l1) } else { None })
            .fold(0.0f32, f32::max);
        p.h = bottom + 36.0;
    }
    if !web {
        headers_footers(&mut ctx, &mut pages, &sections);
    }
    // Evict paragraph layouts that weren't used this pass if the cache grew large.
    if ctx.cache.paras.len() > 4 * ctx.cache.used.len().max(1024) {
        let used = std::mem::take(&mut ctx.cache.used);
        ctx.cache.paras.retain(|k, _| used.contains(k));
        ctx.cache.used = used;
    }
    let mut index: HashMap<(StoryRef, Path), Vec<(usize, usize)>> = HashMap::new();
    for (pi, p) in pages.iter().enumerate() {
        for (ii, it) in p.items.iter().enumerate() {
            if let Placed::Lines { story, path, .. } = it {
                index.entry((*story, path.clone())).or_default().push((pi, ii));
            }
        }
    }
    (DocLayout { pages, index, ms: now_ms() - t0, text_boxes: ctx.boxes.total() }, reach_back)
}

/// The page a turned section (`w:textDirection` on its `sectPr`) is laid out on: its page turned
/// so that its lines run across, the margins turning with it. `None` for an upright section.
fn turned_frame(s: &SectionProps) -> Option<SectionProps> {
    let mut f = s.clone();
    (f.page_w, f.page_h) = (s.page_h, s.page_w);
    // The frame's left, right, top and bottom margins: where its lines start and end, and the
    // side they stack from.
    [f.margin_left, f.margin_right, f.margin_top, f.margin_bottom] = match s.text_direction {
        TextDirection::Horizontal => return None,
        TextDirection::Down => [s.margin_top, s.margin_bottom, s.margin_right + s.gutter, s.margin_left],
        TextDirection::Up => [s.margin_bottom, s.margin_top, s.margin_left + s.gutter, s.margin_right],
    };
    f.gutter = 0.0;
    f.page_borders = None;
    Some(f)
}

/// Turn a page laid out on its section's turned frame (see [`turned_frame`]) onto the paper.
fn turn_page(page: &mut Page, sect: &SectionProps) {
    let (x, y) = match sect.text_direction {
        TextDirection::Horizontal => return,
        TextDirection::Down => (sect.page_w, 0.0),
        TextDirection::Up => (0.0, sect.page_h),
    };
    for it in &mut page.items {
        it.turn(sect.text_direction, x, y);
    }
    page.body = turn_rect(sect.text_direction, x, y, page.body);
    (page.w, page.h) = (sect.page_w, sect.page_h);
}

fn item_bottom(y: f32, para: &ParaLayout, l0: usize, l1: usize) -> Option<f32> {
    let f = para.lines.get(l0)?;
    let l = para.lines.get(l1.checked_sub(1)?)?;
    Some(y + l.top + l.height - f.top)
}

/// Where body text starts: the top margin, or below the header if it's taller.
fn body_top_for(ctx: &mut Ctx, sect: &SectionProps, header: Option<u32>) -> f32 {
    let Some(id) = header else { return sect.margin_top };
    let Some(part) = ctx.doc.parts.get(&id) else { return sect.margin_top };
    let blocks = part.blocks.clone();
    // Placed as `headers_footers` draws it, so page-relative floats wrap the same way.
    let frame = PageFrame { sect, origin: (sect.margin_left + sect.gutter, sect.header), even: false };
    let h = layout_box(ctx, StoryRef::Part(id), &blocks, &[], sect.text_width(), None, 0, Some(frame)).height;
    // Word starts the body right below a header that reaches past the top margin, no gap.
    sect.margin_top.max(sect.header + h)
}

/// Where a section's footer starts: its height up from the footer distance (`f32::MAX` without one).
fn footer_top_for(ctx: &mut Ctx, sect: &SectionProps, footer: Option<u32>) -> f32 {
    let Some(id) = footer else { return f32::MAX };
    let Some(part) = ctx.doc.parts.get(&id) else { return f32::MAX };
    let blocks = part.blocks.clone();
    let h = layout_box(ctx, StoryRef::Part(id), &blocks, &[], sect.text_width(), None, 0, None).height;
    sect.page_h - sect.footer - h
}

/// Where a box of laid-out items sits on its page, for floating objects positioned relative to
/// the page or its margins.
#[derive(Clone, Copy)]
struct PageFrame<'a> {
    sect: &'a SectionProps,
    /// Page position of the box's (0, 0); (0, 0) for body text.
    origin: (f32, f32),
    /// An even page: inside and outside alignment and margins mirror (inside is right).
    even: bool,
}

/// Room a text box that fits its text keeps beyond its widest line, so the line laid out again in
/// it never wraps.
const FIT_SLACK: f32 = 0.1;

/// [`layout_box`] for measuring only: the list counters and equation numbers are left as they
/// were, so a text box measured before it is laid out numbers its lists from 1, as WPS does.
fn measure_box(ctx: &mut Ctx, story: StoryRef, blocks: &Blocks, width: f32, depth: usize) -> BoxLayout {
    let (counters, eq_count) = (ctx.counters.clone(), ctx.eq_count);
    let laid = layout_box(ctx, story, blocks, &[], width, None, depth, None);
    (ctx.counters, ctx.eq_count) = (counters, eq_count);
    laid
}

/// The size (`w` × `h`) a floating object is placed at: its own, or for a text box that fits its
/// text, as WPS sizes it: unwrapped (`wrap="none"` with `a:spAutoFit`), its widest line and its
/// text's height within its margins; wrapped (`a:spAutoFit` alone), its own width and its text's
/// height.
fn placed_size(ctx: &mut Ctx, o: &InlineObject, w: f32, h: f32, depth: usize) -> (f32, f32) {
    let InlineObject::Shape { story: Some(id), body: Some(frame), .. } = o else { return (w, h) };
    let Some(blocks) =
        ctx.doc.parts.get(id).filter(|p| (frame.fit || frame.fit_height) && p.kind == kition_doc_doc::PartKind::TextBox).map(|p| p.blocks.clone())
    else {
        return (w, h);
    };
    if !ctx.boxes.enter(*id) {
        return (w, h);
    }
    let [il, it, ir, ib] = frame.insets;
    let size = if frame.fit {
        // Laid out unwrapped: wide enough for any line.
        let laid = measure_box(ctx, StoryRef::Part(*id), &blocks, MAX_FIT_WIDTH, depth + 1);
        (widest_line(&laid.items) + il + ir + FIT_SLACK, laid.height + it + ib)
    } else {
        let laid = measure_box(ctx, StoryRef::Part(*id), &blocks, (w - il - ir).max(12.0), depth + 1);
        (w, laid.height + it + ib)
    };
    ctx.boxes.leave();
    size
}

/// A floating object's width from its share of an area of the page (`wp14:sizeRelH`), as WPS
/// for Mac sizes it; else its stored width `w`.
fn rel_width(float: &Float, sect: Option<&SectionProps>, w: f32) -> f32 {
    let (Some(rel), Some(s)) = (float.rel_width, sect) else { return w };
    let area = match rel.from {
        Anchor::Page => s.page_w,
        Anchor::LeftMargin | Anchor::InsideMargin => s.margin_left + s.gutter,
        Anchor::RightMargin | Anchor::OutsideMargin => s.margin_right,
        _ => s.text_width(),
    };
    let width = area * rel.share;
    if width.is_finite() && width > 0.0 { width.clamp(1.0, 4000.0) } else { w }
}

/// The width of the widest laid-out line in `items`: its indent and its letters, wherever its
/// alignment set them.
fn widest_line(items: &[Placed]) -> f32 {
    items
        .iter()
        .filter_map(|item| match item {
            Placed::Lines { para, l0, l1, .. } => para
                .lines
                .get(*l0..*l1)
                .map(|lines| lines.iter().map(|l| l.left.max(0.0) + l.end_x() - l.xs.first().copied().unwrap_or(l.left)).fold(0.0f32, f32::max)),
            _ => None,
        })
        .fold(0.0f32, f32::max)
}

/// The width a text box that fits its text is laid out in to measure its lines, points.
const MAX_FIT_WIDTH: f32 = 4000.0;

/// A floating (not inline) picture, chart or shape: its size and placement.
fn floating(o: &InlineObject) -> Option<(f32, f32, &Float)> {
    o.frame().filter(|(_, _, float)| float.wrap != Wrap::Inline)
}

/// Where a floating object goes, in the coordinates of the box `frame` places on the page.
/// `col` is the column (or cell) left edge and width, `y0` the anchor paragraph's top. Without a
/// frame (table cells, text boxes, notes) positions relative to the page or margins fall back
/// to the box itself.
/// `ys` is the anchor paragraph's top above its space before and its first line's top: WPS
/// measures an offset from the paragraph from the former, wherever the spacing puts the text.
fn float_rect(frame: Option<PageFrame>, col: (f32, f32), ys: (f32, f32), w: f32, h: f32, float: &Float) -> Rect {
    let (w, h) = (w.clamp(1.0, 4000.0), h.clamp(1.0, 4000.0));
    // Each axis: the reference area's start and extent, in page coordinates.
    let even = frame.is_some_and(|f| f.even);
    let page_x = |s: &SectionProps| {
        let left = s.margin_left + s.gutter;
        let (left_area, right_area) = ((0.0, left), (s.page_w - s.margin_right, s.margin_right));
        match float.h_rel {
            Anchor::Page => Some((0.0, s.page_w)),
            Anchor::Margin => Some((left, s.text_width())),
            Anchor::LeftMargin => Some(left_area),
            Anchor::RightMargin => Some(right_area),
            // Inside is the left margin on odd pages and the right one on even pages.
            Anchor::InsideMargin => Some(if even { right_area } else { left_area }),
            Anchor::OutsideMargin => Some(if even { left_area } else { right_area }),
            _ => None,
        }
    };
    let page_y = |s: &SectionProps| match float.v_rel {
        Anchor::Page => Some((0.0, s.page_h)),
        Anchor::Margin => Some((s.margin_top, s.page_h - s.margin_top - s.margin_bottom)),
        Anchor::TopMargin | Anchor::InsideMargin => Some((0.0, s.margin_top)),
        Anchor::BottomMargin | Anchor::OutsideMargin => Some((s.page_h - s.margin_bottom, s.margin_bottom)),
        _ => None,
    };
    let hx = frame.and_then(|f| page_x(f.sect).map(|(a, e)| (a - f.origin.0, e))).unwrap_or(col);
    let y0 = if float.v_rel == Anchor::Paragraph { ys.0 } else { ys.1 };
    let vy = frame.and_then(|f| page_y(f.sect).map(|(a, e)| (a - f.origin.1, e))).unwrap_or((y0, 0.0));
    // Inside and outside alignment mirror on even pages (ECMA-376 Part 1, ST_AlignH).
    let h_align = match float.h_align {
        Some(FloatAlign::Inside) if even => Some(FloatAlign::End),
        Some(FloatAlign::Outside) if even => Some(FloatAlign::Start),
        other => other,
    };
    let mut y = align_in(vy, float.y, h, float.v_align);
    // An object text wraps around, positioned from its paragraph or line, that would end below
    // the page is moved up to end at the page's bottom edge, when it fits on a page; one text
    // doesn't wrap around stays where it is put, past the page if so (as WPS places them).
    if let Some(f) = frame
        && matches!(float.v_rel, Anchor::Paragraph | Anchor::Line)
        && !matches!(float.wrap, Wrap::BehindText | Wrap::InFrontOfText)
        && h <= f.sect.page_h
    {
        y = y.min(f.sect.page_h - f.origin.1 - h);
    }
    Rect::new(align_in(hx, float.x, w, h_align), y, w, h)
}

/// Where an object of `size` starts on one axis of `area` (start, extent): at `offset` from the
/// area's start, or aligned in it when `align` is set.
fn align_in((start, extent): (f32, f32), offset: f32, size: f32, align: Option<FloatAlign>) -> f32 {
    match align {
        None => start + if offset.is_finite() { offset.clamp(-31_680.0, 31_680.0) } else { 0.0 },
        Some(FloatAlign::Start | FloatAlign::Inside) => start,
        Some(FloatAlign::Center) => start + (extent - size) / 2.0,
        Some(FloatAlign::End | FloatAlign::Outside) => start + extent - size,
    }
}

/// The areas text keeps clear of around a floating object at `r` (its rectangle grown by the
/// distances from text), each with whether text may only go above and below it; none when text
/// flows over or under the object. A tight or through wrap follows the object's wrap polygon,
/// as WPS for Mac sets it: each line keeps clear of the polygon's span over the line's height,
/// sliced finely enough to follow it.
fn wrap_areas(r: Rect, float: &Float) -> Vec<(Rect, para::Clear)> {
    if matches!(float.wrap, Wrap::Inline | Wrap::BehindText | Wrap::InFrontOfText) {
        return Vec::new();
    }
    let d = |v: f32| if v.is_finite() { v.clamp(0.0, 1584.0) } else { 0.0 };
    let (side, top, bottom) = (d(float.dist), d(float.dist_top), d(float.dist_bottom));
    let polygon = float.wrap_polygon.filter(|_| matches!(float.wrap, Wrap::Tight | Wrap::Through) && float.spin().deg == 0.0);
    if let Some(polygon) = polygon.filter(|_| r.w > 0.0 && r.h > 0.0 && r.h.is_finite() && r.w.is_finite()) {
        // Heights and widths in shares of the frame, flips undone.
        let unflip = |v: f32, flip: bool| if flip { 1.0 - v } else { v };
        let (v_top, v_bottom) = polygon.points().iter().fold((f32::MAX, f32::MIN), |(a, b), [_, y]| {
            let y = unflip(*y, float.flip_v);
            (a.min(y), b.max(y))
        });
        // The outline keeps text clear only by its distance at the sides (WPS for Mac: luan's
        // tight text box 3.6 pt above and below lets the next line's text run beside it).
        let (top, bottom) = (0.0, 0.0);
        let (y0, y1) = (r.y + v_top * r.h - top, r.y + v_bottom * r.h + bottom);
        let slices = ((y1 - y0) / POLYGON_SLICE).ceil().clamp(0.0, 8000.0) as usize;
        let mut areas = Vec::with_capacity(slices);
        for k in 0..slices {
            let s0 = y0 + k as f32 * POLYGON_SLICE;
            let s1 = (s0 + POLYGON_SLICE).min(y1);
            // The polygon within the distances above and below of the slice.
            let (a, b) = (unflip((s0 - bottom - r.y) / r.h, float.flip_v), unflip((s1 + top - r.y) / r.h, float.flip_v));
            let Some((l, rr)) = polygon.span(a.min(b), a.max(b)) else { continue };
            let (l, rr) = (unflip(l, float.flip_h), unflip(rr, float.flip_h));
            let (x0, x1) = (r.x + l.min(rr) * r.w - side, r.x + l.max(rr) * r.w + side);
            areas.push((Rect::new(x0, s0, x1 - x0, s1 - s0), para::Clear::Outline));
        }
        return areas;
    }
    // A rotated object keeps text clear of its rotated bounds, and of its effects (shadows,
    // outlines: `wp:effectExtent`, as WPS for Mac wraps luan's top-and-bottom text box).
    let r = float.spin().bounds(r);
    let [el, et, er, eb] = float.effect_extent();
    let r = Rect::new(r.x - el, r.y - et, r.w + el + er, r.h + et + eb);
    let clear = if float.wrap == Wrap::TopAndBottom { para::Clear::Below } else { para::Clear::Beside };
    vec![(Rect::new(r.x - side, r.y - top, r.w + side * 2.0, r.h + top + bottom), clear)]
}

/// Height of the slices a wrap polygon is cut into (see [`wrap_areas`]), points.
const POLYGON_SLICE: f32 = 0.5;

/// `r` moved down below the objects in `apart` (other than `key`'s own) it overlaps.
fn below_apart(mut r: Rect, apart: &[((usize, usize), Rect)], key: (usize, usize)) -> Rect {
    for _ in 0..64 {
        let overlap = |o: &Rect| o.x < r.right() - 0.01 && r.x < o.right() - 0.01 && o.y < r.bottom() - 0.01 && r.y < o.bottom() - 0.01;
        let Some(bottom) = apart.iter().filter(|(k, o)| *k != key && overlap(o)).map(|(_, o)| o.bottom()).reduce(f32::max) else { break };
        r.y = bottom;
    }
    r
}

/// The box around wrap areas, and how text keeps clear of it.
fn wrap_bounds(areas: &[(Rect, para::Clear)]) -> Option<(Rect, para::Clear)> {
    let (first, rest) = areas.split_first()?;
    let r = rest.iter().fold(first.0, |a, (b, _)| {
        let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
        Rect::new(x0, y0, a.right().max(b.right()) - x0, a.bottom().max(b.bottom()) - y0)
    });
    Some((r, first.1))
}

/// Wrap areas relative to a paragraph whose column starts at `x0` and whose top is `y0`.
fn rel_exclusions(excl: &[(Rect, para::Clear)], x0: f32, y0: f32) -> Vec<para::Exclusion> {
    excl.iter()
        .map(|(r, clear)| para::Exclusion { top: r.y - y0, bottom: r.bottom() - y0, left: r.x - x0, right: r.right() - x0, clear: *clear })
        .filter(|e| e.bottom > 0.0)
        .collect()
}

/// The items drawing floating object `o` at `rect` (a group: its members), turned by its own
/// spin inside `outer` (its group's, about `outer`'s centre point).
fn float_items(o: &InlineObject, rect: Rect, outer: (Spin, Point), story: StoryRef, path: &[u32], off: usize) -> Vec<Placed> {
    let own = o.frame().map(|(_, _, f)| f.spin()).unwrap_or_default();
    let (ospin, oc) = outer;
    let rect = carried(rect, ospin, oc);
    let spin = own.within(ospin);
    match o {
        InlineObject::Image { media, crop, adjust, geom, edge, .. } => vec![Placed::Image {
            rect,
            media: media.clone(),
            crop: *crop,
            adjust: *adjust,
            geom: geom.clone(),
            edge: *edge,
            story,
            path: Path(path.to_vec()),
            off,
            spin,
        }],
        InlineObject::Shape { kind, fill, stroke, stroke_width, effects, freeform, geom, gradient, dash, .. } => {
            vec![Placed::Shape {
                rect,
                kind: *kind,
                fill: *fill,
                stroke: *stroke,
                stroke_width: *stroke_width,
                effects: *effects,
                freeform: freeform.clone(),
                spin,
                geom: geom.clone(),
                gradient: gradient.clone(),
                dash: *dash,
            }]
        }
        InlineObject::Graphic { graphic, .. } => {
            vec![Placed::Graphic { rect, graphic: graphic.clone(), story, path: Path(path.to_vec()), off, spin }]
        }
        InlineObject::Group { .. } => {
            let centre = Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
            group_members(o, rect).into_iter().flat_map(|(r, c)| float_items(c, r, (spin, centre), story, path, off)).collect()
        }
        _ => Vec::new(),
    }
}

/// A group's members, each with its rectangle when the group is at `rect`.
/// `rect` moved (not turned) to where `spin` about `centre` takes its centre: a group member's
/// frame inside its turned group.
fn carried(rect: Rect, spin: Spin, centre: Point) -> Rect {
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let (nx, ny) = spin.apply(centre.x, centre.y, cx, cy);
    Rect::new(rect.x + nx - cx, rect.y + ny - cy, rect.w, rect.h)
}

fn group_members(o: &InlineObject, rect: Rect) -> Vec<(Rect, &InlineObject)> {
    o.group_rects(rect.x, rect.y, rect.w, rect.h).into_iter().map(|([x, y, w, h], c)| (Rect::new(x, y, w, h), c)).collect()
}

/// Position the floating objects anchored in `p` for a paragraph whose text starts at `y0` in
/// the current column, add their wrap areas to the page (replacing any from an earlier attempt,
/// from `excl_mark` on) and lay the paragraph out around them, with its list `label`.
fn anchor_floats(
    ctx: &mut Ctx,
    pb: &mut PageBuilder,
    p: &Paragraph,
    block: usize,
    (top, y0): (f32, f32),
    excl_mark: usize,
    label: &Option<(String, Level)>,
) -> (HashMap<usize, Rect>, Arc<ParaLayout>) {
    pb.excl.truncate(excl_mark);
    ctx.fields.page = pb.number;
    let (col_x, width) = (pb.col_x(), pb.col_w());
    let mut rects = HashMap::new();
    if !pb.web {
        let frame = PageFrame { sect: pb.sect, origin: (0.0, 0.0), even: pb.number.is_multiple_of(2) };
        // An object left out of the layout (hidden, or deleted in the final text) is not placed, so
        // it takes no room either.
        let left_out = left_out_objects(ctx.doc, p, None, ctx.opts.show_hidden, ctx.opts.hide_deleted);
        for (oi, o) in p.objects.iter().enumerate() {
            let Some((w, h, float)) = floating(o) else { continue };
            if left_out(oi) {
                continue;
            }
            let w = rel_width(float, Some(pb.sect), w);
            let (w, h) = placed_size(ctx, o, w, h, 0);
            // Where an earlier pass put it, when it reached back into an earlier column of
            // this page (see `layout`).
            let pinned = pb.seeds.get(&(block, oi)).filter(|s| s.0 + 1 == pb.pages.len()).map(|s| s.1);
            let natural = float_rect(Some(frame), (col_x, width), (top, y0), w, h, float);
            // One that may not overlap others moves down below those it would (Word and WPS).
            let natural = if float.keep_apart { below_apart(natural, &pb.apart, (block, oi)) } else { natural };
            let r = match pinned {
                Some(pin) => {
                    pb.pin_shift.insert((block, oi), Point::new(pin.x - natural.x, pin.y - natural.y));
                    pin
                }
                None => {
                    pb.pin_shift.remove(&(block, oi));
                    natural
                }
            };
            rects.insert(oi, r);
            if float.keep_apart {
                pb.apart.retain(|(key, _)| *key != (block, oi));
                pb.apart.push(((block, oi), r));
            }
            pb.excl.extend(wrap_areas(r, float));
        }
    }
    let rel = rel_exclusions(&pb.excl, col_x, y0);
    pb.para_excl = rel.clone();
    (rects, ctx.para_labelled(p, width, None, &rel, label.clone()))
}

/// Where a paragraph's floating objects are positioned from, when they aren't placed already:
/// the box on its page, the column (left, width) and the paragraph's top. Also where offsets
/// relative to the column and paragraph start, for dragging.
#[derive(Clone, Copy)]
struct ObjFrame<'a> {
    page: Option<PageFrame<'a>>,
    col: (f32, f32),
    /// The paragraph's top above its space before, and its first line's top (see `float_rect`).
    para_y: (f32, f32),
    /// A table's character formatting under the paragraph's (see `left_out_objects`).
    table_chr: Option<&'a CharProps>,
    /// Objects kept where an earlier pass put them, by index, and how far that is from where
    /// their anchor puts them: offsets from the column and paragraph start that much further.
    shifts: &'a [(usize, Point)],
}

/// One floating object's items in a layer, with its place in the stack (`Float::z`; `None`
/// below the others) and whether it is behind the text.
#[derive(Clone)]
struct Layered {
    behind: bool,
    z: Option<i64>,
    items: Vec<Placed>,
}

/// What [`place_objects`] places: floating objects by layer and stack, and what stays with the
/// paragraph's lines (inline objects' areas and text boxes' text).
#[derive(Default)]
struct PlacedObjects {
    floats: Vec<Layered>,
    flow: Vec<Placed>,
}

impl PlacedObjects {
    /// Everything as (behind the text, in front), floats in stacking order, for a story laid
    /// out in one piece (a text box, a header).
    fn flatten(self) -> (Vec<Placed>, Vec<Placed>) {
        let (behind, mut front) = split_layers(self.floats);
        front.extend(self.flow);
        (behind, front)
    }
}

/// Floating objects' items, (behind the text, in front), each layer bottom to top: by `z`, then
/// in the order they were placed.
fn split_layers(mut floats: Vec<Layered>) -> (Vec<Placed>, Vec<Placed>) {
    floats.sort_by_key(|l| l.z.unwrap_or(i64::MIN));
    let (mut behind, mut front) = (Vec::new(), Vec::new());
    for l in floats {
        if l.behind { behind.extend(l.items) } else { front.extend(l.items) }
    }
    (behind, front)
}

/// A finished page's floating objects: those behind the text under everything else on the
/// page, those in front over it, each in stacking order.
fn stack_floats(page: &mut Page, floats: Vec<Layered>) {
    let (behind, front) = split_layers(floats);
    page.items.splice(0..0, behind);
    page.items.extend(front);
}

/// The pictures, shapes and text boxes in lines `l0..l1` of paragraph `p` (story `story`, at
/// `path`), whose lines start at (x, y): floating ones drawn, every one's area (hit testing,
/// selection) and text boxes' text. Floating rects come from `floats` (by object index) or are
/// worked out from `at`. Returns what goes behind the text, and what goes in front.
#[allow(clippy::too_many_arguments)]
fn place_objects(
    ctx: &mut Ctx,
    story: StoryRef,
    path: &[u32],
    p: &Paragraph,
    pl: &ParaLayout,
    (l0, l1): (usize, usize),
    (x, y): (f32, f32),
    at: &ObjFrame,
    floats: &HashMap<usize, Rect>,
    depth: usize,
) -> PlacedObjects {
    let mut out = PlacedObjects::default();
    let Some(fl) = pl.lines.get(l0) else { return out };
    // Objects left out of the layout (hidden, or deleted in the final text) aren't placed.
    let left_out = left_out_objects(ctx.doc, p, at.table_chr, ctx.opts.show_hidden, ctx.opts.hide_deleted);
    for li in l0..l1 {
        let Some(line) = pl.lines.get(li) else { continue };
        for k in line.c0..line.c1 {
            let Some(c) = pl.clusters.get(k) else { continue };
            let para::ClKind::Object(oi) = c.kind else { continue };
            if left_out(oi) {
                continue;
            }
            let Some(obj) = p.objects.get(oi) else { continue };
            let Some((w, h, float)) = obj.frame() else { continue };
            let floating = float.wrap != Wrap::Inline;
            let rect = if floating {
                match floats.get(&oi) {
                    Some(r) => *r,
                    None => {
                        let w = rel_width(float, at.page.map(|f| f.sect), w);
                        let (w, h) = placed_size(ctx, obj, w, h, depth);
                        float_rect(at.page, at.col, at.para_y, w, h, float)
                    }
                }
            } else {
                let cx = x + line.cl_left(k).unwrap_or(0.0);
                display::inline_rect(Some(obj), cx, y + (line.baseline - fl.top), c.adv, c.obj_h)
            };
            let behind_text = float.wrap == Wrap::BehindText;
            // A floating object's drawing (in its layer), its area and its text boxes' text go
            // in the stack together; an inline one's area and text stay with its line.
            let mut drawn: Vec<Placed> = Vec::new();
            let mut over: Vec<Placed> = Vec::new();
            if floating {
                drawn.extend(float_items(obj, rect, (Spin::default(), Point::default()), story, path, c.start));
            }
            let text_box = match obj {
                InlineObject::Shape { story: Some(id), .. } if ctx.doc.parts.get(id).is_some_and(|p| p.kind == kition_doc_doc::PartKind::TextBox) => {
                    Some(*id)
                }
                _ => None,
            };
            over.push(Placed::Object {
                rect,
                spin: float.spin(),
                story,
                path: Path(path.to_vec()),
                off: c.start,
                text_box,
                wrap: float.wrap,
                origin: {
                    let shift = at.shifts.iter().find(|(i, _)| *i == oi).map_or(Point::new(0.0, 0.0), |(_, d)| *d);
                    let y = if float.v_rel == Anchor::Paragraph { at.para_y.0 } else { at.para_y.1 };
                    Point::new(at.col.0 + shift.x, y + shift.y)
                },
            });
            // Its text (a group: its text boxes', each where the group's turn carries it; the
            // text itself stays upright), unless the box budget says no (a box inside itself,
            // too deep, too many).
            let centre = Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
            let flat = flattened(obj);
            let boxes: Vec<(u32, Rect, TextFrame)> = match obj {
                InlineObject::Group { .. } => group_members(obj, rect)
                    .into_iter()
                    .filter_map(|(r, c)| {
                        c.text_box()
                            .filter(|id| ctx.doc.parts.get(id).is_some_and(|p| p.kind == kition_doc_doc::PartKind::TextBox))
                            .map(|id| (id, carried(r, float.spin(), centre), text_frame(c)))
                    })
                    .collect(),
                _ => text_box.map(|id| (id, rect, text_frame(obj))).into_iter().collect(),
            };
            for (id, rect, frame) in boxes {
                if !ctx.boxes.enter(id) {
                    continue;
                }
                // The text within the frame's margins, set up and down by its anchor.
                let [il, it, ir, ib] = frame.insets;
                let mut margins = Rect::new(rect.x + il, rect.y + it, (rect.w - il - ir).max(12.0), (rect.h - it - ib).max(0.0));
                let blocks = ctx.doc.parts.get(&id).map(|p| p.blocks.clone()).unwrap_or_default();
                // Text that isn't wrapped is laid out as wide as its widest line: a box that fits its
                // text takes that size (whatever its fields show now), one that doesn't keeps its own.
                let mut width = margins.w;
                if frame.fit || frame.no_wrap {
                    let unwrapped = measure_box(ctx, StoryRef::Part(id), &blocks, MAX_FIT_WIDTH, depth + 1);
                    width = width.max(widest_line(&unwrapped.items) + FIT_SLACK);
                    if frame.fit {
                        margins.w = width;
                        margins.h = margins.h.max(unwrapped.height);
                    }
                }
                let laid = layout_box(ctx, StoryRef::Part(id), &blocks, &[], width, None, depth + 1, None);
                ctx.boxes.leave();
                let layer = if behind_text { &mut drawn } else { &mut over };
                // Lines that start below the margins are hidden; the rest are clipped to the box.
                let fitted = fit_box(laid.items, margins.h);
                let spare = (margins.h - laid.height).max(0.0);
                let down = match frame.anchor {
                    TextAnchor::Top => 0.0,
                    TextAnchor::Middle => spare / 2.0,
                    TextAnchor::Bottom => spare,
                };
                let warp = match obj {
                    InlineObject::Shape { warp: Some(w), .. } if kition_office_core::warp::is_warp(&w.name) => {
                        Some(Placed::Warp { rect: margins, warp: w.clone(), items: fitted.len(), spin: float.spin() })
                    }
                    _ => None,
                };
                // Only a box whose last line runs past it gets a clip.
                let clip = warp.is_none()
                    && fitted.iter().any(|it| match it {
                        Placed::Lines { para, l0, l1, y, .. } => match (para.lines.get(*l0), para.lines.get(l1.wrapping_sub(1))) {
                            (Some(first), Some(last)) => y + last.top + last.height - first.top > margins.h + 0.5,
                            _ => false,
                        },
                        _ => false,
                    });
                if flat {
                    layer.push(Placed::Flat { rect, items: fitted.len() + usize::from(warp.is_some()) + usize::from(clip) });
                }
                layer.extend(warp);
                if clip {
                    layer.push(Placed::Clip { rect, items: fitted.len() });
                }
                for mut item in fitted {
                    item.translate(margins.x, margins.y + down);
                    layer.push(item);
                }
            }
            if floating {
                if behind_text {
                    out.floats.push(Layered { behind: true, z: float.z, items: drawn });
                    out.floats.push(Layered { behind: false, z: float.z, items: over });
                } else {
                    drawn.extend(over);
                    out.floats.push(Layered { behind: false, z: float.z, items: drawn });
                }
            } else {
                out.flow.extend(drawn);
                out.flow.extend(over);
            }
        }
    }
    out
}

/// Whether WPS prints a group as one picture: one of its members (at any depth) has a blurred
/// shadow, a glow or soft edges, which PDF can't draw.
fn flattened(o: &InlineObject) -> bool {
    let InlineObject::Group { children, .. } = o else { return false };
    children.iter().any(|c| match &c.obj {
        InlineObject::Shape { effects, .. } => effects.shadow.is_some_and(|s| s.blur > 0.0) || effects.glow.is_some() || effects.soft_edge.is_some(),
        group => flattened(group),
    })
}

/// Footnotes referenced in a paragraph laid out as `pl`: in its text, and inside its text boxes
/// (on the box's cluster), as (cluster, note id).
fn para_notes(doc: &Document, p: &Paragraph, pl: &ParaLayout) -> Vec<(usize, u32)> {
    let mut notes = pl.notes.clone();
    if p.objects.iter().any(|o| !o.text_boxes().is_empty()) {
        for (ci, c) in pl.clusters.iter().enumerate() {
            if let para::ClKind::Object(oi) = c.kind
                && let Some(o) = p.objects.get(oi)
            {
                for id in o.text_boxes() {
                    notes.extend(doc.notes_in_text_box(id).into_iter().map(|n| (ci, n)));
                }
            }
        }
    }
    notes
}

fn place_para(ctx: &mut Ctx, pb: &mut PageBuilder, p: &Paragraph, block: usize, section_end: usize, body_top: f32) {
    // The empty paragraph that only ends a section takes no room: the next section continues
    // right after the text before it, or starts its new page with no blank page between, even
    // when the mark would not fit at the foot of a full page (measured on WPS references). It is
    // still laid out, where it starts, so the caret can reach it.
    if p.section.is_some() && p.text.is_empty() && ctx.doc.styles.resolve_para(&p.props).numbering.is_none() {
        // Taking no room, it never moves to another column or page. Before a section on a new
        // page it is still the paragraph the next one's space before collapses with.
        let (y, prev, bottom) = (pb.y, pb.prev.clone(), pb.bottom);
        pb.bottom = f32::MAX / 4.0;
        place_para_lines(ctx, pb, p, block, section_end, body_top);
        pb.bottom = bottom;
        if pb.next_continuous {
            (pb.y, pb.prev) = (y, prev);
        }
        return;
    }
    // A paragraph holding only a page break stays at the foot of a full page rather than making
    // a blank page of its own (measured on WPS references).
    if p.text.chars().all(|c| c == kition_doc_doc::para::PAGE_BREAK)
        && !p.text.is_empty()
        && ctx.doc.styles.resolve_para(&p.props).numbering.is_none()
    {
        let (pages, bottom) = (pb.pages.len(), pb.bottom);
        pb.bottom = f32::MAX / 4.0;
        place_para_lines(ctx, pb, p, block, section_end, body_top);
        // The break started the next page, which has its own foot.
        if pb.pages.len() == pages {
            pb.bottom = bottom;
        }
        return;
    }
    place_para_lines(ctx, pb, p, block, section_end, body_top);
}

/// Notes which of paragraph `p`'s floating objects (placed at `rects`, from the paragraph's
/// column starting at `col_x`, column `at.1` of page `at.0 - 1`) reach back into an earlier column
/// of the page, whose text was laid out without them (see [`PageBuilder::reach_back`]).
fn note_reach_back(
    pb: &mut PageBuilder,
    p: &Paragraph,
    block: usize,
    rects: &HashMap<usize, Rect>,
    (pages, col): (usize, usize),
    (col_x, top): (f32, f32),
) {
    for (oi, o) in p.objects.iter().enumerate() {
        let area = match (rects.get(&oi), floating(o)) {
            (Some(r), Some((_, _, float))) if pages > 0 => {
                wrap_bounds(&wrap_areas(*r, float)).filter(|(a, _)| (col > 0 && a.x < col_x - 0.5) || a.y < top - 0.5).map(|(a, tb)| (*r, a, tb))
            }
            _ => None,
        };
        match area {
            Some((rect, area, tb)) => {
                pb.reach_back.insert((block, oi), (pages - 1, rect, area, tb));
            }
            None => {
                pb.reach_back.remove(&(block, oi));
            }
        }
    }
}

fn place_para_lines(ctx: &mut Ctx, pb: &mut PageBuilder, p: &Paragraph, block: usize, section_end: usize, body_top: f32) {
    let width = pb.col_w();
    // Floating objects anchored here join the page's wrap areas before the text is laid out; if
    // the paragraph then starts somewhere else (another column or page, or contextual spacing
    // drops its space before), they are placed again there.
    let space_before = ctx.doc.styles.resolve_para(&p.props).space_before;
    let mut excl_mark = pb.excl.len();
    let guess = pb.y + space_before;
    let label = ctx.next_label(p);
    let (mut float_rects, mut pl) = anchor_floats(ctx, pb, p, block, (pb.y, guess), excl_mark, &label);
    // The anchor paragraph's top (above and below its space before) and column the floats were
    // placed from (offsets relative to the column are measured from it, wherever the anchor's own
    // line ends up).
    let mut anchor_y = (pb.y, guess);
    let mut anchor_col = (pb.col_x(), pb.col_w());
    let mut anchor_at = (pb.pages.len(), pb.col);
    note_reach_back(pb, p, block, &float_rects, anchor_at, (anchor_col.0, anchor_y.0));
    let spot = |pb: &PageBuilder| (pb.pages.len(), pb.col);
    let placed_at = spot(pb);
    if pl.rp.page_break_before && !pb.at_top() && !pb.web {
        pb.new_page(block, body_top);
    }
    let previous = pb.prev.take();
    let at_top = pb.at_top();
    let after_break = std::mem::take(&mut pb.after_break);
    // The first paragraph of a continuous section starting mid-page: text that doesn't fit there
    // moves on, but keeping it with the next paragraph or its own lines together doesn't move it
    // (measured on WPS references).
    let section_head = pb.mid_page && block == pb.section_first && pb.y <= pb.top + 0.01;
    let neighbors =
        (block.checked_sub(1).and_then(|index| ctx.doc.body.get(index)).map(AsRef::as_ref), ctx.doc.body.get(block + 1).map(AsRef::as_ref));
    let (before, after) = spacing_units::margins(ctx.doc, &pl.rp, neighbors, None, at_top, false);
    // At the top of a column the space before is dropped after the page's text flowed on, but a
    // section's first paragraph keeps the part of its space before beyond the space after that
    // ended the previous section (measured on WPS references).
    let starts_section = block == pb.section_first && block > 0;
    // The first paragraph after a page break that ended the paragraph before drops its space
    // before, as one that text flowed on to the page does (WPS for Mac references).
    let before = if at_top && after_break && !starts_section { before.min(0.0) } else { before };
    let collapses = !at_top || starts_section;
    let before = paragraph_spacing(&pl.rp, previous.as_ref().filter(|_| collapses).map(|(s, c, a)| (s.as_str(), *c, *a)), before);
    let room = border_room(ctx.doc, &pl.rp, neighbors, None);
    let mut top = pb.y;
    pb.y += before + room.top;
    // Moved to a new column or page top (below its space before): a column start for its lines.
    let mut moved = false;
    // Keep lines together: if it doesn't fit but would on an empty column, move it.
    if pl.rp.keep_lines || pl.rp.keep_next {
        let (following, gap) = if pl.rp.keep_next && !pl.lines.iter().any(|line| matches!(line.end, LineEnd::PageBreak | LineEnd::ColumnBreak)) {
            lookahead::height(ctx, block, section_end, width, &pl.rp, pb.bottom - pb.top - pl.height, pb.bottom - pb.top)
        } else {
            (0.0, 0.0)
        };
        let mut need = pl.height + following;
        // The next paragraph starts below what this one's floats leave no room beside, its space
        // before taken up there: WPS for Mac moves a heading on when its text box pushes the next
        // paragraph to the next page.
        if following > gap && pb.excl.len() > excl_mark {
            let own = rel_exclusions(pb.excl.get(excl_mark..).unwrap_or(&[]), pb.col_x(), pb.y);
            let (start, rest) = (pl.height + gap, following - gap);
            let mut next = start;
            for _ in 0..16 {
                match para::row_spans(&own, next, rest, 0.0, 0.0, width) {
                    Err(below) if below > next => next = below,
                    _ => break,
                }
            }
            if next > start {
                need = need.max(next + rest);
            }
        }
        let capacity = pb.bottom - pb.top;
        let continues_chain = block.checked_sub(1).and_then(|index| ctx.doc.body.get(index)).is_some_and(|previous| {
            matches!(&**previous, Block::Para(paragraph) if paragraph.section.is_none() && ctx.doc.styles.resolve_para(&paragraph.props).keep_next)
        });
        let starts_oversized_chain = following > 0.0 && need > capacity && !continues_chain;
        if pb.y + need > pb.bottom && !at_top && !section_head && (need <= capacity || starts_oversized_chain) {
            pb.advance(block, body_top);
            top = pb.y;
            pb.y += if starts_section { before } else { pl.rp.space_before.min(0.0) };
            moved = true;
        }
    }
    // Non-wrapping floats also follow the paragraph's final top after spacing is resolved.
    if spot(pb) != placed_at || ((pb.y - guess).abs() > 0.01 && (!pb.excl.is_empty() || !float_rects.is_empty())) {
        // A new page starts with its own wrap areas (those of objects reaching back to it).
        if pb.pages.len() != placed_at.0 {
            excl_mark = pb.excl.len();
        }
        let y0 = (top, pb.y);
        (float_rects, pl) = anchor_floats(ctx, pb, p, block, y0, excl_mark, &label);
        anchor_y = y0;
        anchor_col = (pb.col_x(), pb.col_w());
        anchor_at = (pb.pages.len(), pb.col);
        note_reach_back(pb, p, block, &float_rects, anchor_at, (anchor_col.0, anchor_y.0));
    }
    let mut n = pl.lines.len();
    let mut l0 = 0;
    // How wide the paragraph was laid out, and the lines still to place.
    let (mut base_width, mut laid_width) = (anchor_col.1, anchor_col.1);
    while l0 < n {
        // Leading spacing does not occupy an otherwise empty column. An oversized
        // first line must overflow here instead of manufacturing a blank page.
        let column_start = pb.at_top() || (l0 == 0 && (at_top || moved));
        // How many lines fit?
        let Some(first) = pl.lines.get(l0) else { break };
        // A paragraph's first line may start below its top (pushed down by a floating object).
        let lead = if l0 == 0 { first.top.max(0.0) } else { 0.0 };
        let mut l1 = l0;
        let mut forced_break = None;
        let mut new_notes: Vec<NoteBox> = Vec::new();
        let notes = para_notes(ctx.doc, p, &pl);
        while l1 < n {
            let Some(l) = pl.lines.get(l1) else { break };
            // Footnotes referenced on this line go to the bottom of this page.
            let mut line_notes = Vec::new();
            if !pb.web {
                for (ci, id) in &notes {
                    if *ci >= l.c0
                        && *ci < l.c1
                        && !pb.notes.iter().chain(new_notes.iter()).any(|x| x.id == *id)
                        && let Some(part) = ctx.doc.parts.get(id).filter(|p| p.kind == kition_doc_doc::PartKind::Footnote)
                    {
                        let blocks = part.blocks.clone();
                        let BoxLayout { items, height: h, .. } =
                            layout_box(ctx, StoryRef::Part(*id), &blocks, &[], pb.sect.text_width(), None, 0, None);
                        line_notes.push(NoteBox { id: *id, items, h, cont: false });
                    }
                }
            }
            let extra: f32 = line_notes.iter().map(|n| n.h).sum::<f32>()
                + if pb.notes.is_empty() && new_notes.is_empty() && !line_notes.is_empty() { NOTE_SEP } else { 0.0 };
            let limit = pb.bottom
                - new_notes.iter().map(|n| n.h).sum::<f32>()
                - if pb.notes.is_empty() && !new_notes.is_empty() { NOTE_SEP } else { 0.0 }
                - extra;
            // A line fits when its text ends above the limit; the room a grid or line spacing
            // adds below the text may reach into the bottom margin (measured on WPS references).
            let bottom = pb.y + lead + (l.top + l.height - first.top).min(l.baseline + l.descent - first.top);
            // The paragraph's last line fits with its bottom border's room below it (WPS for Mac).
            let bottom = if l1 + 1 >= n { bottom + room.bottom } else { bottom };
            // Notes too long for the rest of the page continue on the next one: the line still
            // fits if the notes before its last one, and that one's first line, fit below it.
            // The notes then take the rest of the page.
            let split = bottom > limit + 0.01
                && line_notes.split_last().is_some_and(|(last, others)| {
                    let before: f32 = pb.notes.iter().chain(&new_notes).chain(others).map(|n| n.h).sum();
                    pb.body_floor().max(bottom) + NOTE_SEP + before + last.first_piece() <= pb.orig_bottom + 0.01
                });
            if bottom > limit + 0.01 && l1 > l0 && !split {
                break;
            }
            if bottom > limit + 0.01 && l1 == l0 && !column_start && !split {
                break;
            }
            new_notes.extend(line_notes);
            l1 += 1;
            if matches!(l.end, LineEnd::PageBreak | LineEnd::ColumnBreak) && !pb.web {
                forced_break = Some(l.end);
                break;
            }
            if split {
                break;
            }
        }
        // Widow/orphan control (2-line minimum at either end).
        if forced_break.is_none() && l1 < n && pl.rp.widow_control && n >= 2 {
            if l1 - l0 == 1 && l0 == 0 && !column_start {
                l1 = l0; // orphan: move the first line to the next page
            } else if n - l1 == 1 && l1 - l0 >= 3 {
                l1 -= 1; // widow: take one more line along
            }
        }
        // A row (text both sides of a floating object) stays together.
        while l1 > l0 && l1 < n && pl.lines.get(l1).is_some_and(|l| l.beside) {
            l1 -= 1;
        }
        if l1 == l0 {
            if column_start {
                l1 = l0 + 1; // can't fit even one line on an empty page: overflow
            } else {
                pb.advance(block, body_top);
                if l0 == 0 {
                    // A section's first paragraph takes its collapsed space before to the new top.
                    let top = pb.y;
                    if starts_section {
                        pb.y += before;
                    }
                    moved = true;
                    if pb.pages.len() != anchor_at.0 {
                        excl_mark = pb.excl.len();
                    }
                    let y0 = (top, pb.y);
                    (float_rects, pl) = anchor_floats(ctx, pb, p, block, y0, excl_mark, &label);
                    anchor_y = y0;
                    anchor_col = (pb.col_x(), pb.col_w());
                    anchor_at = (pb.pages.len(), pb.col);
                    note_reach_back(pb, p, block, &float_rects, anchor_at, (anchor_col.0, anchor_y.0));
                    n = pl.lines.len();
                    (base_width, laid_width) = (pb.col_w(), pb.col_w());
                }
                continue;
            }
        }
        // Keep only the notes of the lines that are placed.
        let placed_end = pl.lines.get(l1.saturating_sub(1)).map(|l| l.c1).unwrap_or(0);
        new_notes.retain(|n| notes.iter().any(|(ci, nid)| *nid == n.id && *ci < placed_end));
        let before = pb.notes_h();
        pb.notes.extend(new_notes);
        pb.bottom -= pb.notes_h() - before;
        let x = pb.col_x();
        let y = pb.y + lead;
        // Notes that no longer fit below the text (a long one, or the rest of one continued from
        // the page before) get what's left under it and continue on the next page.
        if !pb.notes.is_empty()
            && !pb.web
            && let Some(placed_bottom) = item_bottom(y, &pl, l0, l1)
            && placed_bottom > pb.bottom + 0.01
        {
            pb.bottom = pb.body_floor().max(placed_bottom).min(pb.orig_bottom);
        }
        let mut items = Vec::new();
        push_para(&mut items, StoryRef::Body, &[block as u32], &pl, l0, l1, x, y, laid_width, room);
        // Pictures, shapes and text boxes in these lines (web view: floats placed by this piece).
        let shifts: Vec<(usize, Point)> = pb.pin_shift.iter().filter(|((b, _), _)| *b == block).map(|((_, oi), d)| (*oi, *d)).collect();
        let at = ObjFrame {
            page: Some(PageFrame { sect: pb.sect, origin: (0.0, 0.0), even: pb.number.is_multiple_of(2) }),
            col: if pb.web { (x, width) } else { anchor_col },
            para_y: if pb.web { (y, y) } else { anchor_y },
            table_chr: None,
            shifts: &shifts,
        };
        let placed = place_objects(ctx, StoryRef::Body, &[block as u32], p, &pl, (l0, l1), (x, y), &at, &float_rects, 0);
        items.extend(placed.flow);
        // Line numbers in the left margin.
        // The empty paragraph that only carries a section break has no line number.
        let section_mark = p.section.is_some() && p.text.trim().is_empty();
        if let Some(ln) = pb
            .sect
            .line_numbers
            .clone()
            .filter(|_| !pb.web && !section_mark && pl.rp.style != "Header" && p.props.suppress_line_numbers != Some(true))
            && let Some(first) = pl.lines.get(l0)
        {
            for li in l0..l1 {
                let Some(line) = pl.lines.get(li).filter(|l| !l.beside) else { continue };
                pb.line_no = pb.line_no.saturating_add(1);
                let n = pb.line_no.saturating_add(ln.start.saturating_sub(1));
                if ln.count_by > 1 && !n.is_multiple_of(ln.count_by) {
                    continue;
                }
                let num = ctx.number_para(n);
                let w = num.lines.first().and_then(|l| l.xs.last().copied()).unwrap_or(10.0);
                let dist = if ln.distance > 0.0 { ln.distance } else { 18.0 };
                let ly = y + (line.top - first.top) + (line.baseline - line.top) - num.lines.first().map(|l| l.baseline).unwrap_or(10.0);
                items.push(Placed::Lines {
                    story: StoryRef::Part(u32::MAX),
                    path: Path(vec![]),
                    para: num.clone(),
                    l0: 0,
                    l1: 1,
                    x: pb.sect.margin_left + pb.sect.gutter - dist - w,
                    y: ly,
                    turn: TextDirection::Horizontal,
                });
            }
        }
        if let Some(pg) = pb.page() {
            pg.items.extend(items);
        }
        if !placed.floats.is_empty() && !pb.pages.is_empty() {
            let page = pb.pages.len() - 1;
            pb.floats.entry(page).or_default().extend(placed.floats);
        }
        if let (Some(f), Some(l)) = (pl.lines.get(l0), pl.lines.get(l1 - 1)) {
            pb.y = y + (l.top + l.height - f.top);
        }
        l0 = l1;
        match forced_break {
            Some(LineEnd::PageBreak) => {
                // A page break right before the empty paragraph that ends its section, when the next
                // section starts a new page: the two make one break, the mark staying on this page
                // (WPS for Mac: no page holding only the mark).
                let mark_next = l0 >= n
                    && block + 1 == section_end
                    && !pb.next_continuous
                    && ctx.doc.body.get(block + 1).is_some_and(|b| matches!(&**b, Block::Para(q) if q.section.is_some() && q.text.trim().is_empty()));
                if mark_next {
                    pb.prev = None;
                    return;
                }
                pb.new_page(block, body_top);
                // A page break ending the paragraph leaves its space after on the page it ends.
                if l0 >= n {
                    pb.prev = None;
                    pb.after_break = true;
                    return;
                }
            }
            Some(LineEnd::ColumnBreak) => pb.advance(block, body_top),
            _ if l0 < n => pb.advance(block, body_top),
            _ => {}
        }
        // The rest runs as wide as the column it continues in (unequal columns).
        if l0 < n && (pb.col_w() - laid_width).abs() > 0.01 {
            let rel = std::mem::take(&mut pb.para_excl);
            pl = ctx.para_widened(p, base_width, None, &rel, label.clone(), Some((l0, pb.col_w())));
            pb.para_excl = rel;
            laid_width = pb.col_w();
            n = pl.lines.len();
        }
    }
    pb.y += room.bottom + after;
    pb.prev = Some((pl.rp.style.clone(), pl.rp.contextual_spacing, after));
}

fn place_table(ctx: &mut Ctx, pb: &mut PageBuilder, t: &kition_doc_doc::Table, block: usize, body_top: f32) {
    pb.prev = None;
    pb.after_break = false;
    let width = pb.col_w();
    let mut tl = table::layout_table(ctx, StoryRef::Body, t, &[block as u32], width, 1);
    if let Some(f) = t.props.float.filter(|_| !pb.web) {
        // Before Word 2013 layout (compatibility mode 15) an offset places the first cell's text,
        // so the edge sits a cell margin further out.
        let legacy = if ctx.doc.settings.compat_mode < 15 { table::first_cell_left_margin(ctx, t) } else { 0.0 };
        let h: f32 = tl.rows.iter().map(|r| r.height).sum();
        if h <= pb.bottom - pb.top + 0.01 && place_floating_table(pb, &tl, &f, legacy) {
            return;
        }
        // Too tall for the rest of the page below its anchor: WPS for Mac starts it where its
        // offset puts it and carries the rows that don't fit to the next page's top, still
        // floating, the text after it starting there and wrapping around them (synthetic
        // references, word-ab5cc).
        if h <= pb.bottom - pb.top + 0.01 && f.v_rel == Anchor::Paragraph && f.v_align.is_none() && f.y.is_finite() {
            split_floating_table(pb, &tl, &f, legacy, block, body_top);
            return;
        }
        // Too tall for a page: Word lets it run across pages like a table in the text, from its
        // own left edge.
        tl.x = float_table_x(pb, tl.width, &f, legacy) - pb.col_x();
    }
    // A table in the text flow starts below the floating objects in its way.
    below_floats(pb, pb.col_x() + tl.x, pb.col_x() + tl.x + tl.width);
    let header_rows: Vec<usize> = (0..t.rows.len()).take_while(|r| t.rows.get(*r).is_some_and(|row| row.props.header)).collect();
    let place = |pb: &mut PageBuilder, row: &table::RowLayout| {
        let (x, y) = (pb.col_x() + tl.x, pb.y);
        if let Some(pg) = pb.page() {
            for it in &row.items {
                let mut it = it.clone();
                it.translate(x, y);
                pg.items.push(it);
            }
        }
        pb.y += row.height;
    };
    for (ri, row) in tl.rows.iter().enumerate() {
        let splittable = !header_rows.contains(&ri) && !t.rows.get(ri).is_some_and(|r| r.props.cant_split);
        let mut rest: Option<table::RowLayout> = None;
        // On a new page holding only the repeated header rows.
        let mut fresh = false;
        // Footnotes referenced in the row go to the foot of the page it starts on, leaving the
        // rows that much less room (WPS for Mac: luan's small table with a footnote).
        let mut notes = row_notes(ctx, pb, t, ri);
        let room = |pb: &PageBuilder, notes: &[NoteBox]| {
            pb.bottom - if notes.is_empty() { 0.0 } else { notes.iter().map(|n| n.h).sum::<f32>() + if pb.notes.is_empty() { NOTE_SEP } else { 0.0 } }
        };
        let take_notes = |pb: &mut PageBuilder, notes: &mut Vec<NoteBox>| {
            if !notes.is_empty() {
                let before = pb.notes_h();
                pb.notes.append(notes);
                pb.bottom -= pb.notes_h() - before;
            }
        };
        // Each pass places the part of the row that fits, then breaks the page.
        for _ in 0..1000 {
            let cur = rest.as_ref().unwrap_or(row);
            if pb.y + cur.height <= room(pb, &notes) + 0.01 {
                break;
            }
            let split = if splittable { table::split_row(cur, room(pb, &notes) - pb.y) } else { None };
            match split {
                Some((a, b)) => {
                    take_notes(pb, &mut notes);
                    place(pb, &a);
                    rest = Some(b);
                }
                // Like Word, a row that does not fit on an empty page, or under the header rows
                // repeated there, is placed anyway and runs into the bottom margin.
                None if pb.at_top() || fresh => break,
                None => {}
            }
            pb.advance(block, body_top);
            // Repeat header rows.
            if !header_rows.contains(&ri) {
                for hr in &header_rows {
                    if let Some(h) = tl.rows.get(*hr) {
                        place(pb, h);
                    }
                }
            }
            fresh = true;
        }
        take_notes(pb, &mut notes);
        place(pb, rest.as_ref().unwrap_or(row));
    }
}

/// The footnotes referenced in row `ri` of `t` (its cells' paragraphs and nested tables, in order)
/// that aren't at the page's foot yet, laid out.
fn row_notes(ctx: &mut Ctx, pb: &PageBuilder, t: &kition_doc_doc::Table, ri: usize) -> Vec<NoteBox> {
    fn collect(blocks: &Blocks, ids: &mut Vec<u32>, depth: usize) {
        for block in blocks {
            match &**block {
                Block::Para(p) => {
                    ids.extend(p.objects.iter().filter_map(|o| if let InlineObject::NoteRef { id, .. } = o { Some(*id) } else { None }))
                }
                Block::Table(t) if depth < 8 => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            collect(&cell.blocks, ids, depth + 1);
                        }
                    }
                }
                Block::Table(_) => {}
            }
        }
    }
    let mut ids = Vec::new();
    if pb.web {
        return Vec::new();
    }
    for cell in t.rows.get(ri).map(|r| r.cells.as_slice()).unwrap_or(&[]) {
        collect(&cell.blocks, &mut ids, 0);
    }
    let mut notes: Vec<NoteBox> = Vec::new();
    for id in ids {
        if pb.notes.iter().chain(&notes).any(|n| n.id == id) {
            continue;
        }
        let Some(blocks) = ctx.doc.parts.get(&id).filter(|p| p.kind == kition_doc_doc::PartKind::Footnote).map(|p| p.blocks.clone()) else {
            continue;
        };
        let BoxLayout { items, height: h, .. } = layout_box(ctx, StoryRef::Part(id), &blocks, &[], pb.sect.text_width(), None, 0, None);
        notes.push(NoteBox { id, items, h, cont: false });
    }
    notes
}

/// Place floating table `tl` (`w:tblpPr`): at its own position, which takes no room in the text
/// flow; the text after it wraps around it. `false`: one positioned from its anchor runs past
/// the page's foot, and is left to run on in the text flow (as WPS for Mac sets it).
fn place_floating_table(pb: &mut PageBuilder, tl: &table::TableLayout, f: &TableFloat, legacy: f32) -> bool {
    let h: f32 = tl.rows.iter().map(|r| r.height).sum::<f32>().clamp(0.0, 100_000.0);
    let w = tl.width.clamp(1.0, 100_000.0);
    let [dl, dt, dr, db] = f.dist_from_text();
    {
        let s = pb.sect;
        let h_area = float_table_h_area(pb, f);
        // The text a table anchored to it stands in: where the next paragraph starts.
        let v_area = match f.v_rel {
            Anchor::Page => (0.0, s.page_h),
            Anchor::Margin => (s.margin_top, s.text_height()),
            _ => (pb.y, 0.0),
        };
        let x = float_table_x(pb, w, f, legacy);
        let r = clear_of_float_tables(Rect::new(x, align_in(v_area, f.y, h, f.v_align), w, h), f, legacy, h_area.0, &pb.float_tables);
        if r.bottom() > pb.bottom + 0.01 && !pb.at_top() && f.v_rel == Anchor::Paragraph {
            return false;
        }
        if let Some(pg) = pb.page() {
            let mut y = r.y;
            for row in &tl.rows {
                for it in &row.items {
                    let mut it = it.clone();
                    it.translate(r.x, y);
                    pg.items.push(it);
                }
                y += row.height;
            }
        }
        let area = r.inset(-dl, -dt, -dr, -db);
        pb.excl.push((area, para::Clear::Beside));
        pb.float_tables.push(area);
    }
    true
}

/// Place floating table `tl` positioned from its anchor that runs past the page's foot: the rows
/// that fit where its offset puts it, the rest at the next page's top, both floating (text wraps
/// around them); the text after it continues on the next page.
fn split_floating_table(pb: &mut PageBuilder, tl: &table::TableLayout, f: &TableFloat, legacy: f32, block: usize, body_top: f32) {
    let [dl, dt, dr, db] = f.dist_from_text();
    let w = tl.width.clamp(1.0, 100_000.0);
    let x = float_table_x(pb, w, f, legacy);
    let top = pb.y + f.y.clamp(0.0, (pb.bottom - pb.y).max(0.0));
    let fits = tl.rows.iter().scan(top, |y, row| {
        *y += row.height;
        Some(*y)
    });
    let first = fits.take_while(|y| *y <= pb.bottom + 0.01).count();
    let place = |pb: &mut PageBuilder, rows: &[table::RowLayout], y0: f32| {
        let mut y = y0;
        if let Some(pg) = pb.page() {
            for row in rows {
                for it in &row.items {
                    let mut it = it.clone();
                    it.translate(x, y);
                    pg.items.push(it);
                }
                y += row.height;
            }
        }
        let area = Rect::new(x, y0, w, y - y0).inset(-dl, -dt, -dr, -db);
        pb.excl.push((area, para::Clear::Beside));
        pb.float_tables.push(area);
    };
    let (here, rest) = tl.rows.split_at(first.min(tl.rows.len()));
    if !here.is_empty() {
        place(pb, here, top);
    }
    pb.advance(block, body_top);
    if !rest.is_empty() {
        let y = pb.top;
        place(pb, rest, y);
    }
}

/// The area a floating table's `x` is measured in: its start and width on the page.
fn float_table_h_area(pb: &PageBuilder, f: &TableFloat) -> (f32, f32) {
    let s = pb.sect;
    match f.h_rel {
        Anchor::Page => (0.0, s.page_w),
        Anchor::Margin => (s.margin_left + s.gutter, s.text_width()),
        _ => (pb.col_x(), pb.col_w()),
    }
}

/// A floating table's left edge on the page (`legacy`: the first cell's left margin before Word
/// 2013 layout, when an offset places the cell's text).
fn float_table_x(pb: &PageBuilder, w: f32, f: &TableFloat, legacy: f32) -> f32 {
    let x = align_in(float_table_h_area(pb, f), f.x, w, f.h_align);
    if f.h_align.is_none() { x - legacy } else { x }
}

/// Move `pb.y` below the floating objects in the way of something spanning `x0..x1` that
/// starts there (a table in the text flow doesn't wrap around them).
fn below_floats(pb: &mut PageBuilder, x0: f32, x1: f32) {
    pb.y = below(&pb.excl, pb.y, x0, x1);
}

/// `y`, moved below the wrap areas in `excl` in the way of something spanning `x0..x1` that
/// starts there.
fn below(excl: &[(Rect, para::Clear)], mut y: f32, x0: f32, x1: f32) -> f32 {
    for _ in 0..64 {
        let in_the_way = excl.iter().filter(|(r, _)| r.y <= y + 0.01 && r.bottom() > y && r.x < x1 && r.right() > x0);
        let Some(bottom) = in_the_way.map(|(r, _)| r.bottom()).reduce(f32::max) else { break };
        y = bottom;
    }
    y
}

fn headers_footers(ctx: &mut Ctx, pages: &mut [Page], sections: &[(usize, &SectionProps)]) {
    let total = pages.len() as u32;
    // Pages per section.
    let mut per_section: HashMap<usize, u32> = HashMap::new();
    for p in pages.iter() {
        *per_section.entry(p.section).or_default() += 1;
    }
    let even_odd = ctx.doc.settings.even_odd_headers;
    let mut first_of_section: HashSet<usize> = HashSet::new();
    let mut seen = HashSet::new();
    for (i, p) in pages.iter().enumerate() {
        if !p.blank && seen.insert(p.section) {
            first_of_section.insert(i);
        }
    }
    for (i, page) in pages.iter_mut().enumerate() {
        let Some((_, sect)) = sections.get(page.section) else { continue };
        if page.blank {
            continue;
        }
        ctx.grid_pitch = sect.grid.as_ref().and_then(|grid| grid.line_pitch_points());
        // Inherit header/footer references from earlier sections (Word's "link to previous").
        let pick = |get: &dyn Fn(&SectionProps) -> kition_doc_doc::section::HeaderSet| -> Option<u32> {
            let mut set = get(sect);
            for k in (0..page.section).rev() {
                if set.default.is_some() && set.first.is_some() && set.even.is_some() {
                    break;
                }
                if let Some((_, s)) = sections.get(k) {
                    let o = get(s);
                    set.default = set.default.or(o.default);
                    set.first = set.first.or(o.first);
                    set.even = set.even.or(o.even);
                }
            }
            if sect.title_page && first_of_section.contains(&i) {
                return set.first;
            }
            if even_odd && page.number % 2 == 0 {
                return set.even.or(set.default);
            }
            set.default
        };
        let hid = pick(&|s: &SectionProps| s.headers);
        let fid = pick(&|s: &SectionProps| s.footers);
        ctx.fields.page = page.number;
        ctx.fields.page_format = sect.page_num_format;
        ctx.fields.pages = total;
        ctx.fields.section_pages = per_section.get(&page.section).copied().unwrap_or(1);
        ctx.fields.section = page.section as u32 + 1;
        let w = sect.text_width();
        let x = sect.margin_left + sect.gutter;
        if let Some(id) = hid
            && let Some(part) = ctx.doc.parts.get(&id)
        {
            let blocks = part.blocks.clone();
            let frame = PageFrame { sect, origin: (x, sect.header), even: page.number.is_multiple_of(2) };
            let mut items = layout_box(ctx, StoryRef::Part(id), &blocks, &[], w, None, 0, Some(frame)).items;
            for it in &mut items {
                it.translate(x, sect.header);
            }
            page.header = items;
            page.header_story = Some(id);
        }
        if let Some(id) = fid
            && let Some(part) = ctx.doc.parts.get(&id)
        {
            let blocks = part.blocks.clone();
            // The footer's height fixes where it sits, which page-relative floats need.
            let h = layout_box(ctx, StoryRef::Part(id), &blocks, &[], w, None, 0, None).height;
            let frame = PageFrame { sect, origin: (x, sect.page_h - sect.footer - h), even: page.number.is_multiple_of(2) };
            let mut items = layout_box(ctx, StoryRef::Part(id), &blocks, &[], w, None, 0, Some(frame)).items;
            for it in &mut items {
                it.translate(x, sect.page_h - sect.footer - h);
            }
            page.footer = items;
            page.footer_story = Some(id);
        }
    }
}

/// Is an object floating (not laid out inline)?
pub fn is_floating(o: &InlineObject) -> bool {
    o.is_floating()
}

/// Milliseconds since an arbitrary epoch (monotonic on native; the browser's clock on the web).
pub fn now_ms() -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        T0.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1000.0
    }
    // `Instant::now()` panics on wasm32-unknown-unknown, so ask the browser's clock.
    #[cfg(target_arch = "wasm32")]
    {
        let ms = js_sys::Date::now();
        if ms.is_finite() { ms } else { 0.0 }
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_auto_paragraph_spacing;
#[cfg(test)]
mod tests_auto_spacing;
#[cfg(test)]
mod tests_bidi;
#[cfg(test)]
mod tests_column_start;
#[cfg(test)]
mod tests_compression;
#[cfg(test)]
mod tests_control_marks;
#[cfg(test)]
mod tests_field_tabs;
#[cfg(test)]
mod tests_font_slots;
#[cfg(test)]
mod tests_grid_multiples;
#[cfg(test)]
mod tests_keep_chain;
#[cfg(test)]
mod tests_keep_list;
#[cfg(test)]
mod tests_keep_table;
#[cfg(test)]
mod tests_paragraph_borders;
