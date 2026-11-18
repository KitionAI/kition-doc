//! Kition Doc document model.
//!
//! A [`Document`] holds the main story (`body`) and secondary stories (`parts`: headers,
//! footers, notes, comments, text boxes). A story is a list of [`Block`]s: paragraphs and tables;
//! table cells hold block lists of their own. Blocks are `Arc`-shared, so cloning a document is
//! cheap (undo snapshots, handing a frozen copy to the renderer) and edits copy only what they
//! touch (`Arc::make_mut`).
//!
//! Positions ([`Pos`]) name a story, a [`Path`] to a paragraph and a byte offset in it. Paths
//! compare in document order.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod bidi;
pub mod edit;
pub mod effects;
pub mod encoding;
pub mod fields;
pub mod freeform;
pub mod graphic;
pub mod numbering;
pub mod outline;
pub mod para;
pub mod props;
pub mod resolve;
pub mod section;
pub mod styles;
pub mod table;
pub mod theme;

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use fields::FieldRange;
pub use numbering::{ListKind, Numbering};
pub use para::{InlineObject, Paragraph, Run, field_code_runs};
pub use props::{Align, CharProps, ParaProps, Rgb, TextColor};
pub use section::SectionProps;
pub use styles::{Style, StyleKind, StyleSheet};
pub use table::{Cell, Row, Table};

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum DocError {
    #[error("offset {0} is not a character boundary in the paragraph")]
    BadOffset(usize),
    #[error("no paragraph at {0}")]
    BadPath(String),
    #[error("no story {0:?}")]
    BadStory(StoryRef),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, DocError>;

/// A block in a story. (Blocks are always behind an `Arc`, so the size difference is fine.)
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Block {
    Para(Paragraph),
    Table(Table),
}

impl Block {
    pub fn as_para(&self) -> Option<&Paragraph> {
        match self {
            Block::Para(p) => Some(p),
            Block::Table(_) => None,
        }
    }
    pub fn as_table(&self) -> Option<&Table> {
        match self {
            Block::Table(t) => Some(t),
            Block::Para(_) => None,
        }
    }
}

pub type Blocks = Vec<Arc<Block>>;

/// Text boxes inside text boxes count (and show) this many levels deep.
pub const MAX_TEXT_BOX_DEPTH: usize = 4;

/// Equations are numbered from here up (`Document::assign_object_ids`), apart from the drawing
/// objects' and pictures' numbers, which a file keeps (`wp:docPr/@id`) and new ones take from 1.
pub const EQUATION_IDS: u32 = 1_000_000;

/// Bounds the work of expanding text boxes inside text boxes (layout, walks, saving), whatever a
/// document says: a box isn't expanded inside itself, nesting stops at [`MAX_TEXT_BOX_DEPTH`], and
/// an outermost box expands at most [`BoxBudget::MAX_NESTED`] boxes inside it. (A box whose
/// shapes all show that same box, or chains of boxes each showing the next many times, would
/// otherwise grow exponentially.) Outermost boxes aren't capped: their number is the document's
/// (or its pages', for headers), so the work stays linear and no box is left blank for budget.
#[derive(Debug, Default)]
pub struct BoxBudget {
    open: Vec<u32>,
    nested: usize,
    total: usize,
}

impl BoxBudget {
    /// Boxes expanded inside one outermost box.
    pub const MAX_NESTED: usize = 64;

    /// Start expanding text box story `id`; false if it mustn't be (pair a true with `leave`).
    pub fn enter(&mut self, id: u32) -> bool {
        if self.open.contains(&id) || self.open.len() >= MAX_TEXT_BOX_DEPTH {
            return false;
        }
        if self.open.is_empty() {
            self.nested = 0;
        } else if self.nested >= Self::MAX_NESTED {
            return false;
        } else {
            self.nested += 1;
        }
        self.total += 1;
        self.open.push(id);
        true
    }

    /// Done expanding the box last entered.
    pub fn leave(&mut self) {
        self.open.pop();
    }

    /// Boxes expanded so far.
    pub fn total(&self) -> usize {
        self.total
    }
}

pub fn para_block(p: Paragraph) -> Arc<Block> {
    Arc::new(Block::Para(p))
}

/// Which story a position is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum StoryRef {
    #[default]
    Body,
    Part(u32),
}

/// Path to a paragraph: `[block]` in a story, `[block, row, cell, block]` inside a table cell,
/// three more indexes per nesting level. Lexicographic order = document order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub struct Path(pub Vec<u32>);

impl Path {
    pub fn top(i: usize) -> Path {
        Path(vec![i as u32])
    }
    /// The block index within its container.
    pub fn last(&self) -> usize {
        self.0.last().copied().unwrap_or(0) as usize
    }
    pub fn with_last(&self, i: usize) -> Path {
        let mut p = self.clone();
        if let Some(l) = p.0.last_mut() {
            *l = i as u32;
        }
        p
    }
    /// The container prefix (everything but the last index).
    pub fn parent(&self) -> &[u32] {
        self.0.split_last().map(|(_, r)| r).unwrap_or(&[])
    }
    /// Nesting depth (0 = top-level story block).
    pub fn depth(&self) -> usize {
        self.0.len().saturating_sub(1) / 3
    }
    /// The enclosing table cell (table block path, row, cell) if any.
    pub fn cell(&self) -> Option<(Path, usize, usize)> {
        if self.0.len() < 4 {
            return None;
        }
        let n = self.0.len();
        let t = Path(self.0.get(..n - 3)?.to_vec());
        Some((t, *self.0.get(n - 3)? as usize, *self.0.get(n - 2)? as usize))
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: Vec<String> = self.0.iter().map(|i| i.to_string()).collect();
        write!(f, "{}", s.join("/"))
    }
}

/// A position in a document.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub struct Pos {
    pub story: StoryRef,
    pub path: Path,
    pub off: usize,
}

impl Pos {
    pub fn new(story: StoryRef, path: Path, off: usize) -> Pos {
        Pos { story, path, off }
    }
    pub fn body(block: usize, off: usize) -> Pos {
        Pos { story: StoryRef::Body, path: Path::top(block), off }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PartKind {
    #[default]
    Header,
    Footer,
    Footnote,
    Endnote,
    Comment,
    TextBox,
}

/// A secondary story.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Part {
    pub kind: PartKind,
    pub blocks: Blocks,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Comment {
    /// Who wrote it, as people see them (`w:author`).
    pub author: String,
    /// The principal id of who wrote it (Kition platform rule M23: a comment's `author` in the
    /// replica), when it is not `author` itself: a comment a signed-in person made in Kition.
    /// Empty for comments read from a file. Files do not keep it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author_id: String,
    pub initials: String,
    /// ISO 8601.
    pub date: String,
    /// Reply to this comment id.
    pub parent: Option<u32>,
    pub resolved: bool,
    /// `Document::parts` id of the comment text.
    pub part: u32,
    /// The `w14:paraId` of the comment's last paragraph in the file it was read from (eight hex
    /// digits), which `commentsExtended`, `commentsIds` and replies name it by; written back while
    /// it is still unique. Empty for comments made in Kition (the writer gives them one).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub para_id: String,
    /// The comment's durable id (`w16cid:durableId` in `commentsIds.xml`, eight hex digits), which
    /// `commentsExtensible.xml` names it by. Empty when the file had none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub durable_id: String,
    /// When the comment was made, in UTC (`w16cex:dateUtc` in `commentsExtensible.xml`; `date` is
    /// local time). Empty when the file had none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub date_utc: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RevisionKind {
    #[default]
    Insert,
    Delete,
    Format,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Revision {
    pub kind: RevisionKind,
    pub author: String,
    pub date: String,
}

/// A bibliography source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Source {
    /// Unique tag cited by CITATION fields.
    pub tag: String,
    /// book, article, website, report, film, other
    pub kind: String,
    /// "Last, First; Last, First"
    pub author: String,
    pub title: String,
    pub year: String,
    pub publisher: String,
    pub city: String,
    pub journal: String,
    pub volume: String,
    pub pages: String,
    pub url: String,
}

/// A custom document property (File › Info › Properties › Custom). Citation managers keep
/// their per-document preferences here (Zotero: `ZOTERO_PREF_1`, `ZOTERO_PREF_2`…).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct CustomProp {
    pub name: String,
    /// OOXML variant type: `lpwstr`, `i4`, `r8`, `bool`, `filetime`…, or `raw` when `value` is
    /// the property's XML content kept verbatim (vectors, blobs).
    pub kind: String,
    pub value: String,
}

/// Document properties (File › Info).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct CoreProps {
    pub title: String,
    pub subject: String,
    pub creator: String,
    pub keywords: String,
    pub description: String,
    pub category: String,
    pub last_modified_by: String,
    pub created: String,
    pub modified: String,
    pub revision: u32,
}

/// `SOURCE_DATE_EPOCH` (seconds since 1970, the reproducible-builds convention), when it is set
/// to a whole number: the time a headless build uses instead of the clock, so documents it saves
/// (core properties' created and modified times, comment and revision dates, date fields) and
/// PDFs it exports are the same bytes on every run. `None` on the web, which has no environment.
pub fn source_date_epoch() -> Option<u64> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("SOURCE_DATE_EPOCH").ok().as_deref().and_then(parse_source_date_epoch)
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// A `SOURCE_DATE_EPOCH` value: a non-negative whole number of seconds, else `None` (ignored).
pub fn parse_source_date_epoch(value: &str) -> Option<u64> {
    let value = value.trim();
    (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())).then(|| value.parse().ok()).flatten()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub track_changes: bool,
    /// Default tab stop interval, points.
    pub default_tab: f32,
    pub even_odd_headers: bool,
    pub mirror_margins: bool,
    pub auto_hyphenation: bool,
    pub page_color: Option<Rgb>,
    pub watermark: Option<Watermark>,
    /// Theme fonts (major = headings, minor = body).
    pub major_font: String,
    pub minor_font: String,
    /// Theme colours: dk1 lt1 dk2 lt2 accent1..6 hlink folHlink.
    pub theme_colors: Vec<Rgb>,
    pub theme_name: String,
    pub footnote_format: section::NumFormat,
    pub endnote_format: section::NumFormat,
    pub protection: Option<String>,
    /// Word compatibility mode the document is laid out in (`compatibilityMode`): 15 for Word
    /// 2013 and later, which places a table's border at the margin rather than its text and lets
    /// justified lines shrink their spaces to fit more text.
    pub compat_mode: u32,
    /// Drawing grid spacing in points, across and down (View › Gridlines); ECMA-376's default is 1/8 inch.
    pub grid_h: f32,
    pub grid_v: f32,
    /// Equation options (`m:mathPr`), when the document has them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub math: Option<kition_office_core::math::MathProps>,
    /// Legacy layout options of `w:compat` the document turns on (`spaceForUL`, `useFELayout`,
    /// `balanceSingleByteDoubleByteWidth`…), by element name in file order. Kept for round trip.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub compat_flags: Vec<String>,
    /// `w:characterSpacingControl`: how East Asian punctuation is compressed
    /// (`compressPunctuation`, `compressPunctuationAndJapaneseKana`). `None` = `doNotCompress`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub char_spacing: Option<String>,
    /// `w:themeFontLang`: the languages the theme's fonts are picked for. Kept for round trip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_font_lang: Option<ThemeFontLang>,
    /// Font-table character sets, used when a declared family is unavailable.
    /// None leaves an opaque original table intact; an empty map explicitly clears its sets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_charsets: Option<BTreeMap<String, u8>>,
}

pub const MAX_FONT_DECLARATIONS: usize = 4096;

impl Settings {
    pub fn font_charset(&self, family: &str) -> Option<u8> {
        let charsets = self.font_charsets.as_ref()?;
        charsets
            .get(family)
            .copied()
            .or_else(|| charsets.iter().take(MAX_FONT_DECLARATIONS).find(|(name, _)| name.eq_ignore_ascii_case(family)).map(|(_, value)| *value))
    }
}

/// The languages a document's theme fonts are picked for (`w:themeFontLang`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ThemeFontLang {
    /// Latin text (`w:val`), e.g. `en-US`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latin: Option<String>,
    /// East Asian text (`w:eastAsia`), e.g. `zh-CN`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub east_asia: Option<String>,
    /// Complex-script text (`w:bidi`), e.g. `ar-SA`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidi: Option<String>,
}

/// Word's compatibility mode for documents that don't state one.
pub const LEGACY_COMPAT_MODE: u32 = 12;

/// The compatibility mode of documents created by Word 2013 and later, and by Kition Doc.
pub const COMPAT_MODE_CURRENT: u32 = 15;

/// ECMA-376 §17.15.1.44/45: 180 twentieths of a point when a document doesn't say.
pub const DEFAULT_GRID: f32 = 9.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Watermark {
    pub text: String,
    pub font: String,
    pub color: Rgb,
    pub diagonal: bool,
    pub semitransparent: bool,
}

impl Default for Watermark {
    fn default() -> Self {
        Watermark { text: "DRAFT".into(), font: styles::BODY_FONT.into(), color: Rgb(0xC0, 0xC0, 0xC0), diagonal: true, semitransparent: true }
    }
}

/// Our default theme colours.
pub const THEME_COLORS: [Rgb; 12] = {
    // The default Office theme the kits share (`kition_office_core::drawingml::THEME`).
    let t = kition_office_core::drawingml::THEME;
    let mut out = [Rgb(0, 0, 0); 12];
    let mut i = 0;
    while i < 12 {
        out[i] = Rgb(t[i].0, t[i].1, t[i].2);
        i += 1;
    }
    out
};

impl Default for Settings {
    fn default() -> Self {
        Settings {
            track_changes: false,
            default_tab: 36.0,
            even_odd_headers: false,
            mirror_margins: false,
            auto_hyphenation: false,
            page_color: None,
            watermark: None,
            major_font: styles::HEADING_FONT.into(),
            minor_font: styles::BODY_FONT.into(),
            theme_colors: THEME_COLORS.to_vec(),
            theme_name: "Classic".into(),
            footnote_format: section::NumFormat::Decimal,
            endnote_format: section::NumFormat::LowerRoman,
            protection: None,
            compat_mode: COMPAT_MODE_CURRENT,
            grid_h: DEFAULT_GRID,
            grid_v: DEFAULT_GRID,
            math: None,
            compat_flags: Vec::new(),
            char_spacing: None,
            theme_font_lang: None,
            font_charsets: None,
        }
    }
}

/// A document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Document {
    pub body: Blocks,
    /// Properties of the last section (OOXML's body-level `w:sectPr`).
    pub last_section: SectionProps,
    pub parts: BTreeMap<u32, Part>,
    pub styles: StyleSheet,
    pub numbering: Numbering,
    pub comments: BTreeMap<u32, Comment>,
    pub revisions: Vec<Revision>,
    pub settings: Settings,
    pub core: CoreProps,
    /// Bibliography sources (References › Manage Sources).
    pub sources: Vec<Source>,
    /// Custom document properties, in file order.
    pub custom_props: Vec<CustomProp>,
    /// Embedded media (images) by key.
    #[serde(skip)]
    pub media: BTreeMap<String, Arc<Vec<u8>>>,
    /// Source-format parts we keep verbatim for round-trip (e.g. custom XML), by part name.
    #[serde(skip)]
    pub passthrough: BTreeMap<String, Arc<Vec<u8>>>,
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}

impl Document {
    /// A blank document: one empty Normal paragraph, Letter, built-in styles.
    pub fn new() -> Self {
        Document {
            body: vec![para_block(Paragraph::new())],
            last_section: SectionProps::default(),
            parts: BTreeMap::new(),
            styles: StyleSheet::builtin(),
            numbering: Numbering::default(),
            comments: BTreeMap::new(),
            revisions: Vec::new(),
            settings: Settings::default(),
            core: CoreProps::default(),
            sources: Vec::new(),
            custom_props: Vec::new(),
            media: BTreeMap::new(),
            passthrough: BTreeMap::new(),
        }
    }

    /// A document from plain text, one paragraph per line.
    pub fn from_text(text: &str) -> Self {
        let mut d = Document::new();
        d.body = text.split('\n').map(|l| para_block(Paragraph::with_text(l.trim_end_matches('\r'), CharProps::default()))).collect();
        d.ensure_nonempty();
        d
    }

    /// Every story must hold at least one paragraph, and every cell too.
    pub fn ensure_nonempty(&mut self) {
        fn fix(bl: &mut Blocks) {
            if !bl.iter().any(|b| matches!(**b, Block::Para(_))) || matches!(bl.last().map(|b| &**b), Some(Block::Table(_))) {
                bl.push(para_block(Paragraph::new()));
            }
            for b in bl.iter_mut() {
                if let Block::Table(t) = &**b {
                    let needs = t
                        .rows
                        .iter()
                        .any(|r| r.cells.iter().any(|c| c.blocks.is_empty() || matches!(c.blocks.last().map(|b| &**b), Some(Block::Table(_)))));
                    if needs && let Block::Table(t) = Arc::make_mut(b) {
                        for r in &mut t.rows {
                            for c in &mut r.cells {
                                fix_cell(&mut c.blocks);
                            }
                        }
                    }
                }
            }
        }
        fn fix_cell(bl: &mut Blocks) {
            if bl.is_empty() || matches!(bl.last().map(|b| &**b), Some(Block::Table(_))) {
                bl.push(para_block(Paragraph::new()));
            }
        }
        fix(&mut self.body);
        for p in self.parts.values_mut() {
            if p.blocks.is_empty() {
                p.blocks.push(para_block(Paragraph::new()));
            }
        }
    }

    pub fn story(&self, s: StoryRef) -> Option<&Blocks> {
        match s {
            StoryRef::Body => Some(&self.body),
            StoryRef::Part(id) => self.parts.get(&id).map(|p| &p.blocks),
        }
    }
    pub fn story_mut(&mut self, s: StoryRef) -> Result<&mut Blocks> {
        match s {
            StoryRef::Body => Ok(&mut self.body),
            StoryRef::Part(id) => self.parts.get_mut(&id).map(|p| &mut p.blocks).ok_or(DocError::BadStory(s)),
        }
    }

    /// The block list that contains the block at `path`.
    pub fn container(&self, s: StoryRef, path: &Path) -> Option<&Blocks> {
        let mut bl = self.story(s)?;
        let parent = path.parent();
        if !parent.len().is_multiple_of(3) {
            return None;
        }
        for ch in parent.chunks(3) {
            let [b, r, c] = ch else { return None };
            let Block::Table(t) = &**bl.get(*b as usize)? else { return None };
            bl = &t.rows.get(*r as usize)?.cells.get(*c as usize)?.blocks;
        }
        Some(bl)
    }

    pub fn container_mut(&mut self, s: StoryRef, path: &Path) -> Result<&mut Blocks> {
        let bad = || DocError::BadPath(path.to_string());
        let parent: Vec<u32> = path.parent().to_vec();
        if !parent.len().is_multiple_of(3) {
            return Err(bad());
        }
        let mut bl = self.story_mut(s)?;
        for ch in parent.chunks(3) {
            let [b, r, c] = ch else { return Err(bad()) };
            let blk = bl.get_mut(*b as usize).ok_or_else(bad)?;
            let Block::Table(t) = Arc::make_mut(blk) else { return Err(bad()) };
            bl = &mut t.rows.get_mut(*r as usize).ok_or_else(bad)?.cells.get_mut(*c as usize).ok_or_else(bad)?.blocks;
        }
        Ok(bl)
    }

    pub fn block(&self, s: StoryRef, path: &Path) -> Option<&Block> {
        self.container(s, path)?.get(path.last()).map(|b| &**b)
    }

    pub fn para(&self, s: StoryRef, path: &Path) -> Option<&Paragraph> {
        self.block(s, path)?.as_para()
    }
    pub fn para_at(&self, p: &Pos) -> Option<&Paragraph> {
        self.para(p.story, &p.path)
    }

    pub fn para_mut(&mut self, s: StoryRef, path: &Path) -> Result<&mut Paragraph> {
        let i = path.last();
        let bl = self.container_mut(s, path)?;
        match bl.get_mut(i).map(Arc::make_mut) {
            Some(Block::Para(p)) => Ok(p),
            _ => Err(DocError::BadPath(path.to_string())),
        }
    }

    pub fn table(&self, s: StoryRef, path: &Path) -> Option<&Table> {
        self.block(s, path)?.as_table()
    }
    pub fn table_mut(&mut self, s: StoryRef, path: &Path) -> Result<&mut Table> {
        let i = path.last();
        let bl = self.container_mut(s, path)?;
        match bl.get_mut(i).map(Arc::make_mut) {
            Some(Block::Table(t)) => Ok(t),
            _ => Err(DocError::BadPath(path.to_string())),
        }
    }

    /// Paths of every paragraph in a story, in document order (tables descended, depth-limited).
    pub fn para_paths(&self, s: StoryRef) -> Vec<Path> {
        let mut out = Vec::new();
        if let Some(bl) = self.story(s) {
            walk(bl, &mut Vec::new(), &mut out, 0);
        }
        out
    }

    /// First and last positions of a story.
    pub fn start_of(&self, s: StoryRef) -> Pos {
        let path = self.para_paths(s).into_iter().next().unwrap_or_else(|| Path::top(0));
        Pos { story: s, path, off: 0 }
    }
    pub fn end_of(&self, s: StoryRef) -> Pos {
        let path = self.para_paths(s).pop().unwrap_or_else(|| Path::top(0));
        let off = self.para(s, &path).map(|p| p.len()).unwrap_or(0);
        Pos { story: s, path, off }
    }

    /// The position just after the shape that owns text box story `part` (body first, then the
    /// other parts), for leaving the text box.
    pub fn text_box_anchor(&self, part: u32) -> Option<Pos> {
        let stories = std::iter::once(StoryRef::Body).chain(self.parts.keys().filter(|k| **k != part).map(|k| StoryRef::Part(*k)));
        for story in stories {
            for path in self.para_paths(story) {
                let Some(p) = self.para(story, &path) else { continue };
                for off in p.object_offsets() {
                    if p.object_at(off).is_some_and(|o| o.text_boxes().contains(&part)) {
                        return Some(Pos { story, path, off: off + para::OBJ.len_utf8() });
                    }
                }
            }
        }
        None
    }

    /// The next / previous paragraph path in document order.
    pub fn next_para(&self, s: StoryRef, path: &Path) -> Option<Path> {
        let all = self.para_paths(s);
        let i = all.binary_search(path).ok()?;
        all.get(i + 1).cloned()
    }
    pub fn prev_para(&self, s: StoryRef, path: &Path) -> Option<Path> {
        let all = self.para_paths(s);
        let i = all.binary_search(path).ok()?;
        i.checked_sub(1).and_then(|j| all.get(j).cloned())
    }

    /// Clamp a position into the document (valid path, char boundary).
    pub fn clamp(&self, p: &Pos) -> Pos {
        if let Some(para) = self.para_at(p) {
            return Pos { story: p.story, path: p.path.clone(), off: para.clamp(p.off) };
        }
        let story = if self.story(p.story).is_some() { p.story } else { StoryRef::Body };
        let all = self.para_paths(story);
        let path = all.iter().rev().find(|q| **q <= p.path).or(all.first()).cloned().unwrap_or_else(|| Path::top(0));
        let off = self.para(story, &path).map(|q| q.clamp(p.off)).unwrap_or(0);
        Pos { story, path, off }
    }

    /// The whole text of a story, paragraphs separated by `\n` (tables: cells by `\t`).
    pub fn plain_text(&self, s: StoryRef) -> String {
        let mut out = String::new();
        if let Some(bl) = self.story(s) {
            text_of(bl, &mut out, 0);
        }
        if out.ends_with('\n') {
            out.pop();
        }
        out
    }

    /// Section properties in effect for each top-level body block (index → section number), and
    /// the list of sections.
    pub fn sections(&self) -> Vec<(usize, &SectionProps)> {
        let mut v = Vec::new();
        for (i, b) in self.body.iter().enumerate() {
            if let Block::Para(p) = &**b
                && let Some(s) = &p.section
            {
                v.push((i, &**s));
            }
        }
        v.push((self.body.len().saturating_sub(1), &self.last_section));
        v
    }

    /// The section that contains top-level body block `block` (index into `sections()`).
    pub fn section_index_of(&self, block: usize) -> usize {
        self.sections().iter().position(|(end, _)| block <= *end).unwrap_or(0)
    }

    /// Mutable section properties for the section containing body block `block`.
    pub fn section_mut(&mut self, block: usize) -> &mut SectionProps {
        let ends: Vec<usize> = self.sections().iter().map(|(e, _)| *e).collect();
        let idx = ends.iter().position(|e| block <= *e).unwrap_or(ends.len().saturating_sub(1));
        if idx + 1 < ends.len()
            && let Some(e) = ends.get(idx)
            && let Some(Block::Para(p)) = self.body.get_mut(*e).map(Arc::make_mut)
            && let Some(s) = p.section.as_mut()
        {
            return s;
        }
        &mut self.last_section
    }

    /// Allocate a new part id and insert the part.
    pub fn add_part(&mut self, kind: PartKind, blocks: Blocks) -> u32 {
        let id = self.parts.keys().next_back().map(|k| k + 1).unwrap_or(1);
        let mut part = Part { kind, blocks };
        if part.blocks.is_empty() {
            part.blocks.push(para_block(Paragraph::new()));
        }
        self.parts.insert(id, part);
        id
    }

    /// Count words like Word's status bar (whitespace-separated tokens containing a letter or digit).
    pub fn word_count(&self) -> usize {
        count_words(&self.plain_text(StoryRef::Body))
    }

    /// Word's default count: the body plus its text boxes, footnotes and endnotes.
    pub fn word_count_including_notes(&self) -> usize {
        self.counted_stories().iter().map(|s| count_words(&self.plain_text(*s))).sum()
    }

    /// The body, then the text boxes, footnotes and endnotes it shows (nested ones too), in
    /// document order. Headers, footers, comments and stories nothing points at aren't included.
    pub fn counted_stories(&self) -> Vec<StoryRef> {
        self.reachable(vec![StoryRef::Body], &|o| match o {
            InlineObject::NoteRef { id, .. } => vec![(*id, &[PartKind::Footnote, PartKind::Endnote][..])],
            o => o.text_boxes().into_iter().map(|id| (id, &[PartKind::TextBox][..])).collect(),
        })
    }

    /// Drop the text box stories no box shows any more (a deleted or cut box's text), nested
    /// ones too. Undo snapshots and the clipboard keep their own copies. Free for a document
    /// without text boxes; otherwise one pass over its paragraphs. Returns how many went.
    pub fn prune_text_boxes(&mut self) -> usize {
        if !self.parts.values().any(|p| p.kind == PartKind::TextBox) {
            return 0;
        }
        let roots = std::iter::once(StoryRef::Body)
            .chain(self.parts.iter().filter(|(_, p)| p.kind != PartKind::TextBox).map(|(id, _)| StoryRef::Part(*id)))
            .collect();
        let live = self.reachable(roots, &|o| o.text_boxes().into_iter().map(|id| (id, &[PartKind::TextBox][..])).collect());
        let before = self.parts.len();
        self.parts.retain(|id, p| p.kind != PartKind::TextBox || live.contains(&StoryRef::Part(*id)));
        before - self.parts.len()
    }

    /// Drop the footnotes and endnotes no reference points at (from the body, headers, footers,
    /// comments, text boxes or other notes, nested ones too): Word never shows them and does not
    /// save them. Returns how many went.
    pub fn prune_notes(&mut self) -> usize {
        let note = |k: PartKind| matches!(k, PartKind::Footnote | PartKind::Endnote);
        if !self.parts.values().any(|p| note(p.kind)) {
            return 0;
        }
        let roots = std::iter::once(StoryRef::Body)
            .chain(self.parts.iter().filter(|(_, p)| !note(p.kind) && p.kind != PartKind::TextBox).map(|(id, _)| StoryRef::Part(*id)))
            .collect();
        let live = self.reachable(roots, &|o| match o {
            InlineObject::NoteRef { id, .. } => vec![(*id, &[PartKind::Footnote, PartKind::Endnote][..])],
            o => o.text_boxes().into_iter().map(|id| (id, &[PartKind::TextBox][..])).collect(),
        });
        let before = self.parts.len();
        self.parts.retain(|id, p| !note(p.kind) || live.contains(&StoryRef::Part(*id)));
        before - self.parts.len()
    }

    /// Every inline object in reading order: the body's, with each text box's objects where the
    /// box is (nested boxes too, within a [`BoxBudget`]).
    pub fn objects_in_reading_order(&self, f: &mut dyn FnMut(&InlineObject)) {
        self.walk_objects(&self.body, &mut BoxBudget::default(), &mut |p, k| {
            if let Some(o) = p.objects.get(k) {
                f(o);
            }
            true
        });
    }

    /// Like [`Document::objects_in_reading_order`], with each object's paragraph and index;
    /// `f` returns whether to go into the object's text box (false: the box is left out).
    pub fn objects_in_reading_order_at(&self, f: &mut dyn FnMut(&Paragraph, usize) -> bool) {
        self.walk_objects(&self.body, &mut BoxBudget::default(), f);
    }

    /// The footnotes and endnotes referenced inside text box story `part` (nested boxes too), in
    /// reading order.
    pub fn notes_in_text_box(&self, part: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut budget = BoxBudget::default();
        if let Some(p) = self.parts.get(&part).filter(|p| p.kind == PartKind::TextBox)
            && budget.enter(part)
        {
            self.walk_objects(&p.blocks, &mut budget, &mut |p, k| {
                if let Some(InlineObject::NoteRef { id, .. }) = p.objects.get(k) {
                    out.push(*id);
                }
                true
            });
            budget.leave();
        }
        out
    }

    fn walk_objects(&self, blocks: &Blocks, budget: &mut BoxBudget, f: &mut dyn FnMut(&Paragraph, usize) -> bool) {
        for b in blocks {
            edit::each_para(b, 0, &mut |p| {
                for (k, o) in p.objects.iter().enumerate() {
                    if !f(p, k) {
                        continue;
                    }
                    for id in o.text_boxes() {
                        if let Some(part) = self.parts.get(&id).filter(|p| p.kind == PartKind::TextBox)
                            && budget.enter(id)
                        {
                            self.walk_objects(&part.blocks, budget, f);
                            budget.leave();
                        }
                    }
                }
            });
        }
    }

    /// `roots`, then every story their objects lead to (`follow`: an object's story id and the
    /// part kinds that count), transitively, in the order found.
    fn reachable(&self, roots: Vec<StoryRef>, follow: &dyn Fn(&InlineObject) -> Vec<(u32, &'static [PartKind])>) -> Vec<StoryRef> {
        let mut out = roots;
        let mut seen: std::collections::BTreeSet<StoryRef> = out.iter().copied().collect();
        let mut i = 0;
        while let Some(s) = out.get(i).copied() {
            i += 1;
            let mut found = Vec::new();
            for b in self.story(s).into_iter().flatten() {
                edit::each_para(b, 0, &mut |p| {
                    found.extend(p.objects.iter().flat_map(follow));
                });
            }
            for (id, kinds) in found {
                if self.parts.get(&id).is_some_and(|p| kinds.contains(&p.kind)) && seen.insert(StoryRef::Part(id)) {
                    out.push(StoryRef::Part(id));
                }
            }
        }
        out
    }

    /// Number of paragraphs in the body (all depths).
    pub fn paragraph_count(&self) -> usize {
        self.para_paths(StoryRef::Body).len()
    }

    /// Add a media item; returns its key (deduplicated by content).
    pub fn add_media(&mut self, bytes: Vec<u8>, ext: &str) -> String {
        if let Some((k, _)) = self.media.iter().find(|(_, v)| v.as_slice() == bytes.as_slice()) {
            return k.clone();
        }
        let n = self.media.len() + 1;
        let mut key = format!("image{n}.{ext}");
        let mut i = n;
        while self.media.contains_key(&key) {
            i += 1;
            key = format!("image{i}.{ext}");
        }
        self.media.insert(key.clone(), Arc::new(bytes));
        key
    }

    /// The value of a custom property (names compare case-insensitively, as in Word).
    pub fn custom_prop(&self, name: &str) -> Option<&str> {
        self.custom_props.iter().find(|p| p.name.eq_ignore_ascii_case(name)).map(|p| p.value.as_str())
    }

    /// Set a custom text property, replacing one of the same name in place.
    pub fn set_custom_prop(&mut self, name: &str, value: &str) {
        match self.custom_props.iter_mut().find(|p| p.name.eq_ignore_ascii_case(name)) {
            Some(p) => {
                p.kind = "lpwstr".into();
                p.value = value.to_string();
            }
            None => self.custom_props.push(CustomProp { name: name.to_string(), kind: "lpwstr".into(), value: value.to_string() }),
        }
    }

    /// Remove a custom property. Returns whether it existed.
    pub fn remove_custom_prop(&mut self, name: &str) -> bool {
        let n = self.custom_props.len();
        self.custom_props.retain(|p| !p.name.eq_ignore_ascii_case(name));
        self.custom_props.len() != n
    }

    /// Bookmark names in the body, in order.
    pub fn bookmarks(&self) -> Vec<(String, Pos)> {
        let mut out = Vec::new();
        for path in self.para_paths(StoryRef::Body) {
            if let Some(p) = self.para(StoryRef::Body, &path) {
                for off in p.object_offsets() {
                    if let Some(InlineObject::BookmarkStart { name }) = p.object_at(off) {
                        out.push((name.clone(), Pos { story: StoryRef::Body, path: path.clone(), off }));
                    }
                }
            }
        }
        out
    }
}

/// Words in `s`, as Word counts them: every Chinese or Japanese character (Han ideographs and
/// kana) is a word, and so is every run of other text between spaces and those characters that
/// holds a letter or digit.
pub fn count_words(s: &str) -> usize {
    s.split(|c: char| c.is_whitespace() || c == para::OBJ)
        .map(|token| {
            let ideographs = token.chars().filter(|c| is_counted_ideograph(*c)).count();
            let others = token.split(is_counted_ideograph).filter(|w| w.chars().any(char::is_alphanumeric)).count();
            ideographs + others
        })
        .sum()
}

/// Characters Word counts one word each: CJK ideographs (with extensions and compatibility
/// forms), hiragana and katakana.
fn is_counted_ideograph(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF // hiragana, katakana
        | 0x31F0..=0x31FF // katakana phonetic extensions
        | 0x3400..=0x4DBF // CJK extension A
        | 0x4E00..=0x9FFF // CJK unified ideographs
        | 0xF900..=0xFAFF // CJK compatibility ideographs
        | 0xFF66..=0xFF9F // halfwidth katakana
        | 0x20000..=0x3134F // CJK extensions B-G
    ) && !matches!(c, '\u{30A0}' | '\u{30FB}')
}

fn walk(bl: &Blocks, prefix: &mut Vec<u32>, out: &mut Vec<Path>, depth: usize) {
    if depth > 16 {
        return;
    }
    for (i, b) in bl.iter().enumerate() {
        prefix.push(i as u32);
        match &**b {
            Block::Para(_) => out.push(Path(prefix.clone())),
            Block::Table(t) => {
                for (r, row) in t.rows.iter().enumerate() {
                    for (c, cell) in row.cells.iter().enumerate() {
                        prefix.push(r as u32);
                        prefix.push(c as u32);
                        walk(&cell.blocks, prefix, out, depth + 1);
                        prefix.pop();
                        prefix.pop();
                    }
                }
            }
        }
        prefix.pop();
    }
}

fn text_of(bl: &Blocks, out: &mut String, depth: usize) {
    if depth > 16 {
        return;
    }
    for b in bl {
        match &**b {
            Block::Para(p) => {
                out.push_str(&p.plain_text());
                out.push('\n');
            }
            Block::Table(t) => {
                for row in &t.rows {
                    let mut cells = Vec::new();
                    for c in &row.cells {
                        let mut s = String::new();
                        text_of(&c.blocks, &mut s, depth + 1);
                        cells.push(s.trim_end_matches('\n').replace('\n', " "));
                    }
                    out.push_str(&cells.join("\t"));
                    out.push('\n');
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;

impl Document {
    /// Every drawing object (shape, text box, chart: [`InlineObject::object_uid`]) in every story
    /// (body, headers, footers, notes, text boxes, comment texts), with where it is.
    pub fn shapes(&self) -> Vec<(Pos, &InlineObject)> {
        let mut out = Vec::new();
        let stories = std::iter::once(StoryRef::Body).chain(self.parts.keys().map(|k| StoryRef::Part(*k)));
        for story in stories {
            for path in self.para_paths(story) {
                let Some(para) = self.para(story, &path) else { continue };
                for (o, off) in para.objects.iter().zip(para.object_offsets()) {
                    if o.object_uid().is_some() {
                        out.push((Pos { story, path: path.clone(), off }, o));
                    }
                }
            }
        }
        out
    }

    /// Where the drawing object with id `uid` is.
    pub fn find_shape(&self, uid: u32) -> Option<Pos> {
        if uid == 0 {
            return None;
        }
        self.shapes().into_iter().find(|(_, o)| o.object_uid() == Some(uid)).map(|(p, _)| p)
    }

    /// Gives every paragraph and table row a unique `w14:paraId` (C8-land-14-doc: the stable id
    /// tools name it by, written into the file on save). In story order (the body, then each part
    /// by id; inside a table each row before its cells), the first holder of a valid id keeps it;
    /// a paragraph or row without one, or repeating an earlier one (the second half of a split, a
    /// paste, a copy), gets a fresh id. A fresh id is derived from the item's place and text and
    /// probed upward past every id in use, so the same document always gets the same ids and two
    /// writers adding different paragraphs rarely pick the same one. Answers whether it changed
    /// anything.
    pub fn ensure_para_ids(&mut self) -> bool {
        use std::collections::HashSet;
        fn valid(id: Option<u32>) -> Option<u32> {
            id.filter(|v| *v > 0 && *v < 0x8000_0000)
        }
        /// Walks `blocks` collecting ids in order; `fix` paths (block index chains) need a fresh one.
        fn walk(blocks: &Blocks, depth: usize, held: &mut HashSet<u32>, used: &mut HashSet<u32>, missing: &mut usize) {
            for b in blocks {
                match &**b {
                    Block::Para(p) => match valid(p.props.para_id) {
                        Some(v) if used.insert(v) => {
                            held.insert(v);
                        }
                        _ => *missing += 1,
                    },
                    Block::Table(t) if depth < 16 => {
                        for r in &t.rows {
                            match valid(r.props.para_id) {
                                Some(v) if used.insert(v) => {
                                    held.insert(v);
                                }
                                _ => *missing += 1,
                            }
                            for c in &r.cells {
                                walk(&c.blocks, depth + 1, held, used, missing);
                            }
                        }
                    }
                    Block::Table(_) => {}
                }
            }
        }
        fn fresh(used: &mut HashSet<u32>, seed: u64) -> u32 {
            let mut v = 0x1000_0000u32 | ((seed as u32) & 0x0FFF_FFFF);
            loop {
                if v >= 0x8000_0000 || v == 0 {
                    v = 0x1000_0000;
                }
                if used.insert(v) {
                    return v;
                }
                v += 1;
            }
        }
        fn seed_of(story: u64, index: usize, text: &str) -> u64 {
            // FNV-1a: deterministic across processes and platforms.
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for byte in story.to_le_bytes().iter().chain((index as u64).to_le_bytes().iter()).chain(text.as_bytes()) {
                h ^= u64::from(*byte);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
            h ^ (h >> 32)
        }
        /// Whether a row or paragraph of `t` lacks an id or repeats one in `seen` (which it extends).
        fn table_needs(t: &table::Table, depth: usize, seen: &mut HashSet<u32>) -> bool {
            fn blocks_need(blocks: &Blocks, depth: usize, seen: &mut HashSet<u32>) -> bool {
                blocks.iter().any(|b| match &**b {
                    Block::Para(p) => valid(p.props.para_id).is_none_or(|v| !seen.insert(v)),
                    Block::Table(t) if depth < 16 => table_needs(t, depth, seen),
                    Block::Table(_) => false,
                })
            }
            t.rows
                .iter()
                .any(|r| valid(r.props.para_id).is_none_or(|v| !seen.insert(v)) || r.cells.iter().any(|c| blocks_need(&c.blocks, depth + 1, seen)))
        }
        /// How many rows and paragraphs `t` holds (the fresh-id counter's steps).
        fn table_items(t: &table::Table, depth: usize) -> usize {
            fn blocks_items(blocks: &Blocks, depth: usize) -> usize {
                blocks
                    .iter()
                    .map(|b| match &**b {
                        Block::Para(_) => 1,
                        Block::Table(t) if depth < 16 => table_items(t, depth),
                        Block::Table(_) => 0,
                    })
                    .sum()
            }
            t.rows.iter().map(|r| 1 + r.cells.iter().map(|c| blocks_items(&c.blocks, depth + 1)).sum::<usize>()).sum()
        }
        fn fix(blocks: &mut Blocks, depth: usize, story: u64, seen: &mut HashSet<u32>, used: &mut HashSet<u32>, counter: &mut usize) -> bool {
            let mut changed = false;
            for b in blocks.iter_mut() {
                let needs = |id: Option<u32>, seen: &HashSet<u32>| valid(id).is_none_or(|v| seen.contains(&v));
                match &**b {
                    Block::Para(p) => {
                        *counter += 1;
                        if needs(p.props.para_id, seen) {
                            let v = fresh(used, seed_of(story, *counter, &p.text));
                            if let Block::Para(p) = Arc::make_mut(b) {
                                p.props.para_id = Some(v);
                            }
                            changed = true;
                            seen.insert(v);
                        } else if let Some(v) = valid(p.props.para_id) {
                            seen.insert(v);
                        }
                    }
                    Block::Table(t) if depth < 16 => {
                        // Copy the table only when one of its rows or paragraphs needs an id.
                        let mut probe = seen.clone();
                        if !table_needs(t, depth, &mut probe) {
                            *seen = probe;
                            *counter += table_items(t, depth);
                            continue;
                        }
                        if let Block::Table(t) = Arc::make_mut(b) {
                            for r in &mut t.rows {
                                *counter += 1;
                                if needs(r.props.para_id, seen) {
                                    let v = fresh(used, seed_of(story, *counter, "row"));
                                    r.props.para_id = Some(v);
                                    seen.insert(v);
                                } else if let Some(v) = valid(r.props.para_id) {
                                    seen.insert(v);
                                }
                                for c in &mut r.cells {
                                    fix(&mut c.blocks, depth + 1, story, seen, used, counter);
                                }
                            }
                        }
                        changed = true;
                    }
                    Block::Table(_) => {}
                }
            }
            changed
        }
        let mut held = HashSet::new();
        let mut used = HashSet::new();
        let mut missing = 0usize;
        walk(&self.body, 0, &mut held, &mut used, &mut missing);
        for part in self.parts.values() {
            walk(&part.blocks, 0, &mut held, &mut used, &mut missing);
        }
        if missing == 0 {
            return false;
        }
        // Every id in use anywhere is taken; `seen` follows story order for first-keeps.
        let mut used: HashSet<u32> = held;
        let mut seen = HashSet::new();
        let mut counter = 0usize;
        let mut changed = fix(&mut self.body, 0, 0, &mut seen, &mut used, &mut counter);
        for (id, part) in self.parts.iter_mut() {
            changed |= fix(&mut part.blocks, 0, u64::from(*id) + 1, &mut seen, &mut used, &mut counter);
        }
        changed
    }

    /// Gives every comment a unique durable id (`w16cid:durableId`, eight hex digits below
    /// 0x80000000), its tool id (C8-doc-6), which the writer keeps in `commentsIds.xml`. In
    /// comment order, the first holder of a valid id keeps it; a comment without one, or repeating
    /// an earlier one, gets one derived from what it is (its paragraph id, author, date and text)
    /// and probed upward past every id in use, so one file always reads with the same ids and two
    /// replicas give a comment they both got the same one. Answers whether it changed anything.
    pub fn ensure_comment_ids(&mut self) -> bool {
        let mut changed = false;
        for (id, durable) in self.comment_durable_ids() {
            if let Some(c) = self.comments.get_mut(&id)
                && c.durable_id != durable
            {
                c.durable_id = durable;
                changed = true;
            }
        }
        changed
    }

    /// The durable id every comment has or gets ([`Self::ensure_comment_ids`]), as eight hex
    /// digits: what a session gives it and what a save writes for it (`commentsIds.xml`).
    pub fn comment_durable_ids(&self) -> BTreeMap<u32, String> {
        fn valid(id: &str) -> Option<u32> {
            (id.len() == 8 && id.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| u32::from_str_radix(id, 16).ok())
                .flatten()
                .filter(|v| *v > 0 && *v < 0x8000_0000)
        }
        let mut used: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut out: BTreeMap<u32, String> = BTreeMap::new();
        let mut missing: Vec<u32> = Vec::new();
        for (id, c) in &self.comments {
            match valid(&c.durable_id) {
                Some(v) if used.insert(v) => {
                    out.insert(*id, c.durable_id.clone());
                }
                _ => missing.push(*id),
            }
        }
        for id in missing {
            let Some(c) = self.comments.get(&id) else { continue };
            let text = self.plain_text(StoryRef::Part(c.part));
            // FNV-1a of what the comment is.
            let mut hash: u32 = 0x811c_9dc5;
            for part in [c.para_id.as_str(), c.author.as_str(), c.date.as_str(), text.as_str()] {
                for byte in part.as_bytes().iter().chain([0u8].iter()) {
                    hash ^= u32::from(*byte);
                    hash = hash.wrapping_mul(0x0100_0193);
                }
            }
            let mut v = (hash & 0x7FFF_FFFF).max(1);
            while !used.insert(v) {
                v = if v >= 0x7FFF_FFFE { 1 } else { v + 1 };
            }
            out.insert(id, format!("{v:08X}"));
        }
        out
    }

    /// Each part's name (C8-doc-6), the end of its tool id (`part:<name>`), from what a save and a
    /// reread keep (part numbers do not survive them: a reread numbers the parts it finds in its
    /// own order): a header or footer by the section that names it (from 1, in body order) and
    /// its slot (`header-1-default`, `footer-2-first`, `header-1-even`; the first section naming
    /// a part names it); a footnote or endnote by its place among the references to notes of its
    /// kind, in story order (`footnote-1`); a text box by its shape's number (`textbox-7`); a
    /// comment's text by the comment's durable id (`comment-0A1B2C3D`). A part nothing names has
    /// none.
    pub fn part_names(&self) -> BTreeMap<u32, String> {
        let mut out: BTreeMap<u32, String> = BTreeMap::new();
        let sections = self.body.iter().filter_map(|b| b.as_para().and_then(|p| p.section.as_deref())).chain(std::iter::once(&self.last_section));
        for (n, section) in sections.enumerate() {
            for (set, kind) in [(&section.headers, "header"), (&section.footers, "footer")] {
                for (slot, part) in [("default", set.default), ("first", set.first), ("even", set.even)] {
                    if let Some(part) = part {
                        out.entry(part).or_insert_with(|| format!("{kind}-{}-{slot}", n + 1));
                    }
                }
            }
        }
        let mut notes = [0usize; 2];
        let stories = std::iter::once(StoryRef::Body).chain(self.parts.keys().map(|k| StoryRef::Part(*k)));
        for story in stories {
            for path in self.para_paths(story) {
                let Some(para) = self.para(story, &path) else { continue };
                for o in &para.objects {
                    match o {
                        InlineObject::NoteRef { kind, id, .. } if !out.contains_key(id) => {
                            let (slot, name) = match kind {
                                para::NoteKind::Footnote => (0, "footnote"),
                                para::NoteKind::Endnote => (1, "endnote"),
                            };
                            notes[slot] += 1;
                            out.insert(*id, format!("{name}-{}", notes[slot]));
                        }
                        InlineObject::Shape { story: Some(part), uid, .. } if *uid > 0 => {
                            out.entry(*part).or_insert_with(|| format!("textbox-{uid}"));
                        }
                        _ => {}
                    }
                }
            }
        }
        for c in self.comments.values().filter(|c| !c.durable_id.is_empty()) {
            out.entry(c.part).or_insert_with(|| format!("comment-{}", c.durable_id));
        }
        out.retain(|id, _| self.parts.contains_key(id));
        out
    }

    /// Every tool id a kit's session keeps (C8-land-14-doc, C8-doc-6): paragraphs' and rows' ids
    /// ([`Self::ensure_para_ids`]), comments' durable ids ([`Self::ensure_comment_ids`]), and the
    /// numbers of drawing objects, pictures and equations ([`Self::assign_object_ids`]). Answers
    /// whether it changed anything.
    pub fn ensure_tool_ids(&mut self) -> bool {
        let paragraphs = self.ensure_para_ids();
        let mut comment_paragraphs = false;
        for comment in self.comments.values_mut() {
            if let Some(id) = self
                .parts
                .get(&comment.part)
                .and_then(|part| part.blocks.iter().rev().find_map(|block| block.as_para()))
                .and_then(|p| p.props.para_id)
            {
                let id = format!("{id:08X}");
                if comment.para_id != id {
                    comment.para_id = id;
                    comment_paragraphs = true;
                }
            }
        }
        let comments = self.ensure_comment_ids();
        let objects = self.assign_object_ids();
        paragraphs || comment_paragraphs || comments || objects
    }

    /// The numbered objects ([`InlineObject::tool_uid`]: drawing objects, pictures, equations) of
    /// every story, in story order.
    pub fn numbered(&self) -> Vec<(Pos, &InlineObject)> {
        let mut out = Vec::new();
        let stories = std::iter::once(StoryRef::Body).chain(self.parts.keys().map(|k| StoryRef::Part(*k)));
        for story in stories {
            for path in self.para_paths(story) {
                let Some(para) = self.para(story, &path) else { continue };
                for (o, off) in para.objects.iter().zip(para.object_offsets()) {
                    if o.tool_uid().is_some() {
                        out.push((Pos { story, path: path.clone(), off }, o));
                    }
                }
            }
        }
        out
    }

    /// Gives every drawing object and picture without an id, or with an id an earlier one (in
    /// story order) already has, a new one above every id in use (Kit Harness Protocol `KitDrawing
    /// 1.0` DR2: copies get new ids; a file's `wp:docPr/@id` repeated is repaired the same way on
    /// every read). Equations, which OMML keeps no id for, are numbered by their place among the
    /// document's equations: the k-th is [`EQUATION_IDS`] + k (past any number a drawing has), so
    /// one file reads with the same numbers every time and the document in memory has the
    /// numbers its save reads back with (C8-doc-6). Answers whether it changed anything.
    pub fn assign_object_ids(&mut self) -> bool {
        let numbered: Vec<(Pos, u32, bool)> =
            self.numbered().into_iter().filter_map(|(p, o)| o.tool_uid().map(|u| (p, u, matches!(o, InlineObject::Equation { .. })))).collect();
        // Groups' members keep their ids too: a new id is above theirs, and theirs are taken.
        let members: Vec<u32> = self
            .shapes()
            .into_iter()
            .flat_map(|(_, o)| match o {
                InlineObject::Group { children, .. } => children.iter().filter_map(|c| c.obj.object_uid()).filter(|u| *u > 0).collect(),
                _ => Vec::new(),
            })
            .collect();
        let drawn = || numbered.iter().filter(|(_, _, eq)| !eq);
        let mut next = drawn().map(|(_, u, _)| *u).filter(|u| *u < EQUATION_IDS).chain(members.iter().copied()).max().unwrap_or(0);
        let mut seen: std::collections::HashSet<u32> = members.into_iter().collect();
        let mut wanted: Vec<(Pos, u32)> = Vec::new();
        for (pos, uid, _) in drawn() {
            if *uid != 0 && seen.insert(*uid) {
                continue;
            }
            next = next.saturating_add(1);
            while !seen.insert(next) && next < u32::MAX {
                next = next.saturating_add(1);
            }
            wanted.push((pos.clone(), next));
        }
        let mut k = EQUATION_IDS;
        for (pos, uid, _) in numbered.iter().filter(|(_, _, eq)| *eq) {
            k = k.saturating_add(1);
            while seen.contains(&k) && k < u32::MAX {
                k = k.saturating_add(1);
            }
            seen.insert(k);
            if *uid != k {
                wanted.push((pos.clone(), k));
            }
        }
        let mut changed = false;
        for (pos, uid) in wanted {
            if let Ok(para) = self.para_mut(pos.story, &pos.path)
                && let Some(o) = para.object_at_mut(pos.off)
            {
                o.set_tool_uid(uid);
                para.touch();
                changed = true;
            }
        }
        changed
    }
}
