//! Charts and smart graphics, kept as neutral items ready to draw. A chart is edited through its
//! part (`KitDrawing 1.1`, the shared chart codec in `kition-office-core`); a diagram is shown only.

use serde::{Deserialize, Serialize};

use crate::para::ShapeKind;
use crate::props::Rgb;

/// A chart or a smart graphic, as items ready to draw.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Graphic {
    pub kind: GraphicKind,
    /// What to draw, in points from the object's top-left corner.
    #[serde(default)]
    pub items: Vec<GraphicItem>,
    /// The size (points) the items were built for. Drawn scaled to the object's rectangle when
    /// that differs (layout shrinks oversized inline objects); 0 means the rectangle's own size.
    #[serde(default)]
    pub w: f32,
    #[serde(default)]
    pub h: f32,
    /// Where the graphic was read from, so saving writes the chart or diagram itself back (not
    /// only these items). `None` for graphics made some other way.
    #[serde(skip)]
    pub source: Option<std::sync::Arc<Embedded>>,
    /// The object's id in the document (Kit Harness Protocol `KitDrawing` DR2), kept in DOCX as
    /// `wp:docPr/@id`; 0 until one is given (`Document::assign_object_ids`).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub uid: u32,
    /// The part (`c:chartSpace`) of a chart made in Kition Doc, as it was made; `None` for one
    /// read from a file (its part is the one `source` names). Never changed by an edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart: Option<std::sync::Arc<String>>,
    /// A chart as edited (`KitDrawing 1.1`): its part is its made or read part patched to this
    /// description when it is drawn or saved, so concurrent edits of different members merge (each
    /// member is its own entry in the replica). `None`: as its part says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<Box<kition_office_core::chart::ChartSpec>>,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// What an object read from a file needs to be written back as it was: its own markup in the
/// source format and the package parts that markup refers to. Never edited; Kition Doc only moves
/// and resizes the object around it. The parts themselves (bytes, content type and their own
/// relationships) are kept in [`crate::Document::passthrough`] under their part names.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Embedded {
    /// The object's markup as a standalone XML element with its namespace declarations (DOCX:
    /// the `a:graphic` of a chart or smart graphic, the `w:object` of an OLE object).
    pub xml: String,
    /// The relationships it uses, by the ids in `xml` (or in its parts, for a smart graphic's drawing).
    pub refs: Vec<EmbeddedRef>,
}

/// One relationship an [`Embedded`] object uses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmbeddedRef {
    /// The relationship id the markup uses.
    pub id: String,
    /// Relationship type (a URI).
    pub kind: String,
    /// The part name (a `Document::passthrough` key), or the URL of an external target.
    pub target: String,
    pub external: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GraphicKind {
    #[default]
    Chart,
    Diagram,
}

impl GraphicKind {
    /// What the graphic is, in words: the alt text of one that has none of its own.
    pub fn noun(self) -> &'static str {
        match self {
            GraphicKind::Chart => "chart",
            GraphicKind::Diagram => "diagram",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GraphicItem {
    /// A preset shape filling `rect` ([x, y, w, h]).
    Shape {
        rect: [f32; 4],
        kind: ShapeKind,
        fill: Option<Rgb>,
        stroke: Option<Rgb>,
        #[serde(default)]
        stroke_width: f32,
    },
    /// A path: polygons, pie slices, line series.
    Path {
        segs: Vec<PathSeg>,
        fill: Option<Rgb>,
        stroke: Option<Rgb>,
        #[serde(default)]
        stroke_width: f32,
    },
    /// One line of text in `rect`, aligned horizontally by `align`, centred vertically.
    Text {
        rect: [f32; 4],
        text: String,
        size: f32,
        color: Rgb,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        align: TextAlign,
        #[serde(default)]
        font: Option<String>,
        /// A placeholder's prompt (an empty smart graphic node's "[Text]"): drawn as outlines, not as
        /// document text, so it is never selected, searched or exported as text.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        prompt: bool,
    },
    /// A picture (`Document::media` key) stretched into `rect`.
    Image { rect: [f32; 4], media: String },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathSeg {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// Where a shape's text sits up and down within it (`wps:bodyPr@anchor`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAnchor {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// A shape's text frame (`wps:bodyPr`): where its text sits up and down, and its margins (left,
/// top, right, bottom, points).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextFrame {
    pub anchor: TextAnchor,
    pub insets: [f32; 4],
    /// The box takes its text's size (`wrap="none"` with `a:spAutoFit`): its text is not
    /// wrapped, and the box is as wide as its widest line and as tall as its text.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fit: bool,
    /// The text is not wrapped but the box keeps its size (`wrap="none"` without
    /// `a:spAutoFit`): each line runs on past the box's margins.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_wrap: bool,
    /// The box keeps its width and takes its wrapped text's height (`a:spAutoFit` with the text
    /// wrapped), growing or shrinking as WPS sizes it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fit_height: bool,
}

impl TextFrame {
    /// DrawingML's default margins: 0.1 inch left and right, 0.05 inch top and bottom.
    pub const DEFAULT_INSETS: [f32; 4] = [7.2, 3.6, 7.2, 3.6];
}

impl Default for TextFrame {
    fn default() -> Self {
        Self { anchor: TextAnchor::Top, insets: Self::DEFAULT_INSETS, fit: false, no_wrap: false, fit_height: false }
    }
}

/// A shape outline's DrawingML preset dash (`a:ln/a:prstDash/@val`, ECMA-376 Part 1, 20.1.10.48);
/// a solid outline has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineDash {
    Dot,
    Dash,
    LgDash,
    DashDot,
    LgDashDot,
    LgDashDotDot,
    SysDash,
    SysDot,
    SysDashDot,
    SysDashDotDot,
}

impl LineDash {
    pub const ALL: [LineDash; 10] = [
        LineDash::Dot,
        LineDash::Dash,
        LineDash::LgDash,
        LineDash::DashDot,
        LineDash::LgDashDot,
        LineDash::LgDashDotDot,
        LineDash::SysDash,
        LineDash::SysDot,
        LineDash::SysDashDot,
        LineDash::SysDashDotDot,
    ];

    /// Its `ST_PresetLineDashVal` name.
    pub fn name(self) -> &'static str {
        match self {
            LineDash::Dot => "dot",
            LineDash::Dash => "dash",
            LineDash::LgDash => "lgDash",
            LineDash::DashDot => "dashDot",
            LineDash::LgDashDot => "lgDashDot",
            LineDash::LgDashDotDot => "lgDashDotDot",
            LineDash::SysDash => "sysDash",
            LineDash::SysDot => "sysDot",
            LineDash::SysDashDot => "sysDashDot",
            LineDash::SysDashDotDot => "sysDashDotDot",
        }
    }

    /// The dash named `name`; `None` for `solid` and names that are not presets.
    pub fn parse(name: &str) -> Option<LineDash> {
        LineDash::ALL.into_iter().find(|d| d.name() == name)
    }

    /// Dash and gap lengths in multiples of the line's width, as the preset's definition draws
    /// them (`dash` is 4 on and 3 off, `sysDot` 1 on and 1 off…).
    pub fn pattern(self) -> &'static [f32] {
        match self {
            LineDash::Dot => &[1.0, 3.0],
            LineDash::Dash => &[4.0, 3.0],
            LineDash::LgDash => &[8.0, 3.0],
            LineDash::DashDot => &[4.0, 3.0, 1.0, 3.0],
            LineDash::LgDashDot => &[8.0, 3.0, 1.0, 3.0],
            LineDash::LgDashDotDot => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
            LineDash::SysDash => &[3.0, 1.0],
            LineDash::SysDot => &[1.0, 1.0],
            LineDash::SysDashDot => &[3.0, 1.0, 1.0, 1.0],
            LineDash::SysDashDotDot => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// A DrawingML preset outline by name (`a:prstGeom/@prst`) with its adjust values by guide name
/// (`a:gd` `adj`, `adj1`… with `val N`), for shapes whose kind does not name it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetGeom {
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub adj: Vec<(String, f64)>,
}

impl PresetGeom {
    /// The adjust values in the catalog's guide order; a guide not given takes the preset's default.
    pub fn adjust_values(&self) -> Vec<f64> {
        let mut out: Vec<Option<f64>> = Vec::new();
        for (guide, value) in self.adj.iter().take(16) {
            let index = match guide.strip_prefix("adj") {
                Some("") => Some(0),
                Some(n) => n.parse::<usize>().ok().and_then(|n| n.checked_sub(1)),
                None => None,
            };
            if let Some(i) = index.filter(|i| *i < 16)
                && value.is_finite()
            {
                if out.len() <= i {
                    out.resize(i + 1, None);
                }
                if let Some(slot) = out.get_mut(i) {
                    *slot = Some(*value);
                }
            }
        }
        out.into_iter().map(|v| v.unwrap_or(f64::NAN)).collect()
    }
}

/// One colour of a shape's gradient at `pos` (0 to 1 along it).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub pos: f32,
    pub color: crate::props::Rgb,
}

/// A shape's gradient fill (DrawingML `a:gradFill`): its stops in order, and a linear direction
/// (`angle`, degrees clockwise from left to right) or a `path` (`circle`, `rect`, `shape`) from the
/// centre.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapeGradient {
    pub stops: Vec<GradientStop>,
    pub angle: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl ShapeGradient {
    /// The colour at `t` (0 to 1) along the gradient.
    pub fn at(&self, t: f32) -> crate::props::Rgb {
        let Some(first) = self.stops.first() else { return crate::props::Rgb(0, 0, 0) };
        if t <= first.pos {
            return first.color;
        }
        for w in self.stops.windows(2) {
            let (a, b) = (w[0], w[1]);
            if t <= b.pos {
                let k = if b.pos > a.pos { (t - a.pos) / (b.pos - a.pos) } else { 1.0 };
                let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * k).round().clamp(0.0, 255.0) as u8;
                return crate::props::Rgb(mix(a.color.0, b.color.0), mix(a.color.1, b.color.1), mix(a.color.2, b.color.2));
            }
        }
        self.stops.last().map_or(first.color, |s| s.color)
    }
}

/// A connector's glued ends, each the shape's id (`uid`) and its connection site.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Connection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<(u32, u32)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<(u32, u32)>,
}

/// A picture's outline and effects (`pic:spPr` `a:ln` and `a:effectLst`): a line around it, and
/// a shadow, glow or soft edges drawn from its silhouette.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PictureEdge {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<Rgb>,
    /// The line's width, points.
    pub line_width: f32,
    #[serde(skip_serializing_if = "crate::effects::ShapeEffects::is_empty")]
    pub effects: crate::effects::ShapeEffects,
}

impl PictureEdge {
    /// No line and no effects.
    pub fn is_empty(&self) -> bool {
        self.line.is_none() && self.effects.is_empty()
    }

    /// The edge as drawn for a picture of file `bytes`: a picture that may have see-through
    /// pixels casts its shadow and glow from them, not from its frame, so those aren't drawn.
    pub fn for_picture(&self, bytes: Option<&[u8]>) -> PictureEdge {
        match bytes {
            Some(b) if !may_be_transparent(b) => *self,
            _ => PictureEdge { effects: crate::effects::ShapeEffects::default(), ..*self },
        }
    }
}

/// Whether a picture file may have see-through pixels: not a JPEG, nor a PNG without an alpha
/// channel or a transparent colour (`tRNS`).
pub fn may_be_transparent(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return false;
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        // The IHDR colour type: 4 (grey and alpha) or 6 (RGBA) carry alpha.
        let alpha = matches!(bytes.get(25), Some(4 | 6));
        let trns = bytes.windows(4).take(1 << 20).any(|w| w == b"tRNS");
        return alpha || trns;
    }
    true
}

/// A picture's colour adjustments (DrawingML `a:blip` effects): brightness and contrast
/// (`a:lum`, each -1..1; Word's "washout" is +0.7 and -0.7), grey scale (`a:grayscl`), black and
/// white at a threshold (`a:biLevel`, 0..1) and opacity (`a:alphaModFix`, 0..1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ImageAdjust {
    pub bright: f32,
    pub contrast: f32,
    pub gray: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bilevel: Option<f32>,
    pub opacity: f32,
}

impl Default for ImageAdjust {
    fn default() -> Self {
        ImageAdjust { bright: 0.0, contrast: 0.0, gray: false, bilevel: None, opacity: 1.0 }
    }
}

impl ImageAdjust {
    /// Whether the picture is drawn as it is.
    pub fn is_identity(&self) -> bool {
        *self == ImageAdjust::default()
    }

    /// Whether its colours change (opacity aside).
    pub fn changes_colours(&self) -> bool {
        self.bright != 0.0 || self.contrast != 0.0 || self.gray || self.bilevel.is_some()
    }

    /// Adjust straight (not premultiplied) RGBA pixels in place, on the stored values: contrast
    /// about the middle grey (`v' = ½ + (v - ½)·(1 + c)` for less contrast, `÷ (1 - c)` for more),
    /// then brightness added (`+ b`, by `b·(1 + c/2)` with less contrast); then grey scale and the
    /// black-and-white threshold. Opacity is the drawing's, not the pixels'. (Measured on the
    /// pictures in WPS's PDFs: contrast ±60-72 %, brightness +42 % and -36 %, and washout, which
    /// maps black to about 0.80 and white to 1.0.)
    pub fn apply(&self, rgba: &mut [u8]) {
        if !self.changes_colours() {
            return;
        }
        let c = if self.contrast.is_finite() { self.contrast.clamp(-1.0, 0.99) } else { 0.0 };
        let b = if self.bright.is_finite() { self.bright.clamp(-1.0, 1.0) } else { 0.0 };
        let scale = if c < 0.0 { 1.0 + c } else { 1.0 / (1.0 - c) };
        let lift = b * if c < 0.0 { 1.0 + c / 2.0 } else { 1.0 };
        let tone = |v: f32| ((v - 0.5) * scale + 0.5 + lift).clamp(0.0, 1.0);
        for px in rgba.as_chunks_mut::<4>().0 {
            let mut rgb = [px[0], px[1], px[2]].map(|v| tone(f32::from(v) / 255.0));
            if self.gray || self.bilevel.is_some() {
                let y = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
                let y = match self.bilevel {
                    Some(t) => {
                        if y >= t {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    None => y,
                };
                rgb = [y; 3];
            }
            for (slot, v) in px.iter_mut().zip(rgb) {
                *slot = (v * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod adjust_tests {
    use super::ImageAdjust;

    #[test]
    fn brightness_and_contrast_follow_wps() {
        // As WPS prints them (measured on its PDF's pictures): contrast scales about the middle
        // grey, brightness adds, both on the stored values.
        let one = |adjust: ImageAdjust, v: u8| {
            let mut px = [v, v, v, 255];
            adjust.apply(&mut px);
            px[0]
        };
        let near = |got: u8, want: f32| (f32::from(got) - want * 255.0).abs() <= 3.0;
        let less = ImageAdjust { contrast: -0.6, ..Default::default() };
        assert!(near(one(less, 0), 0.3) && near(one(less, 255), 0.7), "less contrast");
        let more = ImageAdjust { contrast: 0.72, ..Default::default() };
        assert!(near(one(more, 115), 0.325) && one(more, 40) == 0 && one(more, 220) == 255, "more contrast");
        let brighter = ImageAdjust { bright: 0.42, ..Default::default() };
        assert!(near(one(brighter, 128), 0.922) && one(brighter, 200) == 255, "brighter");
        let darker = ImageAdjust { bright: -0.36, ..Default::default() };
        assert!(near(one(darker, 255), 0.64) && one(darker, 60) == 0, "darker");
        // Washout: a middle grey comes out nearly white.
        let washout = ImageAdjust { bright: 0.7, contrast: -0.7, ..Default::default() };
        assert!(near(one(washout, 128), 0.955), "washout");
    }

    #[test]
    fn washout_keeps_a_faint_picture() {
        // Word's washout (+70 % brightness, -70 % contrast): black becomes a light grey and white
        // stays white, still apart (WPS's printed pictures: black about 0.80, white 1.0).
        let washout = ImageAdjust { bright: 0.7, contrast: -0.7, ..Default::default() };
        let mut px = [0, 0, 0, 255, 255, 255, 255, 255];
        washout.apply(&mut px);
        assert!((198..=212).contains(&px[0]) && px[4] >= 250 && px[0] < px[4], "{px:?}");
        assert_eq!((px[3], px[7]), (255, 255), "opacity is not the pixels'");
        let mut gray = [255, 0, 0, 255];
        ImageAdjust { gray: true, ..Default::default() }.apply(&mut gray);
        assert!(gray[0] == gray[1] && gray[1] == gray[2] && (70..=80).contains(&gray[0]), "{gray:?}");
        let mut bw = [200, 200, 200, 255, 40, 40, 40, 255];
        ImageAdjust { bilevel: Some(0.5), ..Default::default() }.apply(&mut bw);
        assert_eq!(bw, [255, 255, 255, 255, 0, 0, 0, 255]);
    }
}
