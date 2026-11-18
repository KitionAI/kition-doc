//! Section properties: page setup, columns, headers/footers, page numbering.

use serde::{Deserialize, Serialize};

use crate::props::{Borders, TextDirection, VAlign};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SectionStart {
    #[default]
    NextPage,
    Continuous,
    EvenPage,
    OddPage,
    NextColumn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum NumFormat {
    #[default]
    Decimal,
    UpperRoman,
    LowerRoman,
    UpperLetter,
    LowerLetter,
    Ordinal,
    CardinalText,
    OrdinalText,
    DecimalZero,
    Bullet,
    None,
    /// Chinese counting numerals: one to ten, then ten-one for eleven, one-hundred and so on.
    ChineseCounting,
    /// Chinese counting numerals with thousands (the same numerals for values below ten thousand).
    ChineseCountingThousand,
    /// Chinese legal (financial) numerals, which always write the leading one before ten.
    ChineseLegalSimplified,
    /// Enclosed circled numbers (U+2460 to U+2473), plain digits beyond twenty.
    DecimalEnclosedCircle,
    /// Enclosed circled numbers for Chinese lists, the same as [`NumFormat::DecimalEnclosedCircle`].
    DecimalEnclosedCircleChinese,
    /// Chinese digits written place by place, without units: 一, 二 … 一〇, 一一.
    IdeographDigital,
}

impl NumFormat {
    pub fn ooxml(self) -> &'static str {
        match self {
            NumFormat::Decimal => "decimal",
            NumFormat::UpperRoman => "upperRoman",
            NumFormat::LowerRoman => "lowerRoman",
            NumFormat::UpperLetter => "upperLetter",
            NumFormat::LowerLetter => "lowerLetter",
            NumFormat::Ordinal => "ordinal",
            NumFormat::CardinalText => "cardinalText",
            NumFormat::OrdinalText => "ordinalText",
            NumFormat::DecimalZero => "decimalZero",
            NumFormat::Bullet => "bullet",
            NumFormat::None => "none",
            NumFormat::ChineseCounting => "chineseCounting",
            NumFormat::ChineseCountingThousand => "chineseCountingThousand",
            NumFormat::ChineseLegalSimplified => "chineseLegalSimplified",
            NumFormat::DecimalEnclosedCircle => "decimalEnclosedCircle",
            NumFormat::DecimalEnclosedCircleChinese => "decimalEnclosedCircleChinese",
            NumFormat::IdeographDigital => "ideographDigital",
        }
    }
    /// A format named by its `w:numFmt` value; unknown names are Arabic numerals (decimal).
    pub fn from_ooxml(s: &str) -> NumFormat {
        NumFormat::parse(s).unwrap_or(NumFormat::Decimal)
    }
    /// The format named by its `w:numFmt` value, if it is one Kition Doc draws.
    pub fn parse(s: &str) -> Option<NumFormat> {
        NumFormat::ALL.into_iter().find(|f| f.ooxml() == s)
    }
    /// Every format Kition Doc draws.
    pub const ALL: [NumFormat; 17] = [
        NumFormat::Decimal,
        NumFormat::UpperRoman,
        NumFormat::LowerRoman,
        NumFormat::UpperLetter,
        NumFormat::LowerLetter,
        NumFormat::Ordinal,
        NumFormat::CardinalText,
        NumFormat::OrdinalText,
        NumFormat::DecimalZero,
        NumFormat::Bullet,
        NumFormat::None,
        NumFormat::ChineseCounting,
        NumFormat::ChineseCountingThousand,
        NumFormat::ChineseLegalSimplified,
        NumFormat::DecimalEnclosedCircle,
        NumFormat::DecimalEnclosedCircleChinese,
        NumFormat::IdeographDigital,
    ];
    /// Format `n` (1-based).
    pub fn format(self, n: u32) -> String {
        match self {
            NumFormat::Decimal => n.to_string(),
            NumFormat::DecimalZero => format!("{n:02}"),
            NumFormat::UpperRoman => roman(n).to_uppercase(),
            NumFormat::LowerRoman => roman(n),
            NumFormat::UpperLetter => letters(n).to_uppercase(),
            NumFormat::LowerLetter => letters(n),
            NumFormat::Ordinal => format!("{n}{}", ordinal_suffix(n)),
            NumFormat::CardinalText => cardinal_text(n),
            NumFormat::OrdinalText => ordinal_text(n),
            NumFormat::Bullet | NumFormat::None => String::new(),
            NumFormat::ChineseCounting | NumFormat::ChineseCountingThousand => chinese(n, &COUNTING_DIGITS, &COUNTING_UNITS, false),
            NumFormat::ChineseLegalSimplified => chinese(n, &LEGAL_DIGITS, &LEGAL_UNITS, true),
            NumFormat::IdeographDigital => {
                n.to_string().chars().map(|d| d.to_digit(10).and_then(|d| COUNTING_DIGITS.get(d as usize)).copied().unwrap_or(d)).collect()
            }
            NumFormat::DecimalEnclosedCircle | NumFormat::DecimalEnclosedCircleChinese => match n {
                1..=20 => char::from_u32(0x2460 + n - 1).map_or_else(|| n.to_string(), String::from),
                _ => n.to_string(),
            },
        }
    }
}

const COUNTING_DIGITS: [char; 10] =
    ['\u{3007}', '\u{4E00}', '\u{4E8C}', '\u{4E09}', '\u{56DB}', '\u{4E94}', '\u{516D}', '\u{4E03}', '\u{516B}', '\u{4E5D}'];
/// Ten, hundred, thousand, ten thousand.
const COUNTING_UNITS: [char; 4] = ['\u{5341}', '\u{767E}', '\u{5343}', '\u{4E07}'];
const LEGAL_DIGITS: [char; 10] =
    ['\u{96F6}', '\u{58F9}', '\u{8D30}', '\u{53C1}', '\u{8086}', '\u{4F0D}', '\u{9646}', '\u{67D2}', '\u{634C}', '\u{7396}'];
const LEGAL_UNITS: [char; 4] = ['\u{62FE}', '\u{4F70}', '\u{4EDF}', '\u{4E07}'];
/// The zero written between non-zero places (one-hundred-zero-one), in counting and legal numerals.
const COUNTING_ZERO: char = '\u{96F6}';

/// `n` in Chinese numerals with place units: zeros inside the number are written once, trailing
/// zeros are dropped, and counting numerals write ten to nineteen without a leading one (ten-one);
/// legal numerals always write it (one-ten-one). Values from 100,000,000 on use plain digits.
fn chinese(n: u32, digits: &[char; 10], units: &[char; 4], legal: bool) -> String {
    if n == 0 {
        return digits[0].to_string();
    }
    if n >= 100_000_000 {
        return n.to_string();
    }
    // Below ten thousand: thousands, hundreds, tens, ones; `lead`: higher places come before.
    let group = |v: u32, lead: bool| -> String {
        let mut out = String::new();
        let mut pending_zero = false;
        for (place, unit) in [(1000, Some(units[2])), (100, Some(units[1])), (10, Some(units[0])), (1, None)] {
            let d = (v / place % 10) as usize;
            if d == 0 {
                pending_zero = !out.is_empty() || lead;
                continue;
            }
            if pending_zero && !out.is_empty() {
                out.push(COUNTING_ZERO);
            }
            pending_zero = false;
            let bare_ten = !legal && place == 10 && d == 1 && out.is_empty() && !lead;
            if !bare_ten {
                out.push(digits[d]);
            }
            out.extend(unit);
        }
        out
    };
    let (high, low) = (n / 10_000, n % 10_000);
    let mut out = String::new();
    if high > 0 {
        out.push_str(&group(high, false));
        out.push(units[3]);
    }
    if low > 0 {
        if high > 0 && low < 1000 {
            out.push(COUNTING_ZERO);
        }
        out.push_str(&group(low, high > 0));
    }
    out
}

fn ordinal_suffix(n: u32) -> &'static str {
    match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    }
}

/// Lowercase roman numerals (0 → empty; values above 3999 use repeated `m`).
pub fn roman(mut n: u32) -> String {
    const T: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut s = String::new();
    n = n.min(50_000);
    for (v, r) in T {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

/// Word-style letters: a..z, aa..zz, aaa.. (repeated letter, not base-26).
pub fn letters(n: u32) -> String {
    if n == 0 {
        return String::new();
    }
    let n = n.min(780);
    let k = (n - 1) / 26 + 1;
    let c = (b'a' + ((n - 1) % 26) as u8) as char;
    std::iter::repeat_n(c, k as usize).collect()
}

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];

/// English cardinal words, capitalised first letter (`Twenty-One`).
pub fn cardinal_text(n: u32) -> String {
    fn words(n: u32) -> String {
        match n {
            0..=19 => ONES.get(n as usize).copied().unwrap_or("").to_string(),
            20..=99 => {
                let t = TENS.get((n / 10) as usize).copied().unwrap_or("");
                if n.is_multiple_of(10) { t.to_string() } else { format!("{t}-{}", ONES.get((n % 10) as usize).copied().unwrap_or("")) }
            }
            100..=999 => {
                let h = format!("{} hundred", words(n / 100));
                if n.is_multiple_of(100) { h } else { format!("{h} {}", words(n % 100)) }
            }
            _ => {
                let th = format!("{} thousand", words(n / 1000));
                if n.is_multiple_of(1000) { th } else { format!("{th} {}", words(n % 1000)) }
            }
        }
    }
    capitalize(&words(n.min(999_999)))
}

/// English ordinal words (`Twenty-First`).
pub fn ordinal_text(n: u32) -> String {
    let c = cardinal_text(n).to_lowercase();
    let (head, last) = match c.rfind(['-', ' ']) {
        Some(i) => (c.get(..=i).unwrap_or(""), c.get(i + 1..).unwrap_or("")),
        None => ("", c.as_str()),
    };
    let last = match last {
        "one" => "first".to_string(),
        "two" => "second".to_string(),
        "three" => "third".to_string(),
        "five" => "fifth".to_string(),
        "eight" => "eighth".to_string(),
        "nine" => "ninth".to_string(),
        "twelve" => "twelfth".to_string(),
        l if l.ends_with('y') => format!("{}ieth", l.trim_end_matches('y')),
        l => format!("{l}th"),
    };
    capitalize(&format!("{head}{last}"))
}

fn capitalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut up = true;
    for c in s.chars() {
        if up {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        up = c == '-' || c == ' ';
    }
    out
}

/// Header/footer story ids (in `Document::parts`) for a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HeaderSet {
    pub default: Option<u32>,
    pub first: Option<u32>,
    pub even: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Columns {
    pub count: u32,
    /// Space between columns, points.
    pub space: f32,
    /// Line between columns.
    pub separator: bool,
    /// Unequal widths (width, space after) per column; empty = equal.
    pub widths: Vec<(f32, f32)>,
}

impl Default for Columns {
    fn default() -> Self {
        Columns { count: 1, space: 36.0, separator: false, widths: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LineNumbering {
    pub count_by: u32,
    pub start: u32,
    /// Distance from text, points (0 = auto).
    pub distance: f32,
    /// Restart each page / section / continuous.
    pub restart: LineNumberRestart,
}

impl Default for LineNumbering {
    fn default() -> Self {
        LineNumbering { count_by: 1, start: 1, distance: 0.0, restart: LineNumberRestart::Page }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LineNumberRestart {
    #[default]
    Page,
    Section,
    Continuous,
}

/// Page setup and section-level settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct DocumentGrid {
    /// OOXML grid type; absent means the standard's default (no grid).
    pub kind: Option<String>,
    /// Line pitch in twentieths of a point, retained exactly for round trips.
    pub line_pitch: Option<i32>,
    /// Signed character-pitch adjustment in 1/4096 points; retained for round trips.
    pub char_space: Option<i32>,
}

impl DocumentGrid {
    pub fn line_pitch_points(&self) -> Option<f32> {
        if !matches!(self.kind.as_deref(), Some("lines" | "linesAndChars")) {
            return None;
        }
        self.line_pitch.filter(|pitch| (1..=31_680).contains(pitch)).map(|pitch| pitch as f32 / 20.0)
    }
    /// The character grid's pitch for text whose normal size is `base` points: that size plus the
    /// grid's adjustment (`w:charSpace`, 1/4096 pt). `None` without a character grid.
    pub fn char_pitch_points(&self, base: f32) -> Option<f32> {
        if !matches!(self.kind.as_deref(), Some("linesAndChars" | "snapToChars")) {
            return None;
        }
        let pitch = base + self.char_space.unwrap_or(0) as f32 / 4096.0;
        (pitch.is_finite() && pitch >= 1.0).then(|| pitch.min(1584.0))
    }
}

/// Page setup and section-level settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SectionProps {
    pub start: SectionStart,
    /// Page width/height, points (already swapped for landscape).
    pub page_w: f32,
    pub page_h: f32,
    pub landscape: bool,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    /// Distance from the page edge to the header / footer.
    pub header: f32,
    pub footer: f32,
    pub gutter: f32,
    /// Mirror margins (inside/outside) — set from the document settings.
    pub columns: Columns,
    pub headers: HeaderSet,
    pub footers: HeaderSet,
    /// Different first page header/footer.
    pub title_page: bool,
    pub page_num_start: Option<u32>,
    pub page_num_format: NumFormat,
    pub line_numbers: Option<LineNumbering>,
    pub valign: VAlign,
    pub page_borders: Option<Borders>,
    /// Text direction / bidi section.
    pub rtl: bool,
    /// Which way the body's text runs (`w:textDirection`): turned sections set their lines top
    /// to bottom, stacked from right to left (or bottom to top, stacked left to right).
    #[serde(skip_serializing_if = "TextDirection::is_horizontal")]
    pub text_direction: TextDirection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<DocumentGrid>,
    /// Tracked change of the section's properties (`w:sectPrChange`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fmt_change: Option<Box<crate::props::PropChange<SectionProps>>>,
}

impl Default for SectionProps {
    /// Word's default: US Letter, 1" margins, 0.5" header/footer distance.
    fn default() -> Self {
        SectionProps {
            start: SectionStart::NextPage,
            page_w: 612.0,
            page_h: 792.0,
            landscape: false,
            margin_top: 72.0,
            margin_bottom: 72.0,
            margin_left: 72.0,
            margin_right: 72.0,
            header: 36.0,
            footer: 36.0,
            gutter: 0.0,
            columns: Columns::default(),
            headers: HeaderSet::default(),
            footers: HeaderSet::default(),
            title_page: false,
            page_num_start: None,
            page_num_format: NumFormat::Decimal,
            line_numbers: None,
            valign: VAlign::Top,
            page_borders: None,
            rtl: false,
            text_direction: TextDirection::Horizontal,
            grid: None,
            fmt_change: None,
        }
    }
}

impl SectionProps {
    /// Width of the text area, points.
    pub fn text_width(&self) -> f32 {
        (self.page_w - self.margin_left - self.margin_right - self.gutter).max(18.0)
    }
    pub fn text_height(&self) -> f32 {
        (self.page_h - self.margin_top - self.margin_bottom).max(18.0)
    }
    /// Swap to landscape/portrait (keeps margins relative to the paper as Word does: rotates them).
    pub fn set_landscape(&mut self, on: bool) {
        if on == self.landscape {
            return;
        }
        self.landscape = on;
        std::mem::swap(&mut self.page_w, &mut self.page_h);
        let (t, b, l, r) = (self.margin_top, self.margin_bottom, self.margin_left, self.margin_right);
        if on {
            self.margin_top = l;
            self.margin_bottom = r;
            self.margin_left = b;
            self.margin_right = t;
        } else {
            self.margin_left = t;
            self.margin_right = b;
            self.margin_bottom = l;
            self.margin_top = r;
        }
    }
    /// Column (x offset from the text area's left, width) for each column.
    pub fn column_boxes(&self) -> Vec<(f32, f32)> {
        let tw = self.text_width();
        let n = self.columns.count.clamp(1, 45) as usize;
        if !self.columns.widths.is_empty() && self.columns.widths.len() == n {
            let mut x = 0.0;
            return self
                .columns
                .widths
                .iter()
                .map(|(w, sp)| {
                    let b = (x, w.max(18.0));
                    x += w + sp;
                    b
                })
                .collect();
        }
        let space = self.columns.space.max(0.0);
        let w = ((tw - space * (n as f32 - 1.0)) / n as f32).max(18.0);
        (0..n).map(|i| (i as f32 * (w + space), w)).collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn chinese_numerals_write_places_zeros_and_tens() {
        use super::NumFormat::{ChineseCounting, ChineseLegalSimplified};
        let digits = |s: &str| s.chars().map(|c| c as u32).collect::<Vec<_>>();
        // 10, 11, 20, 101, 110, 1001, 10010 in counting numerals.
        for (n, want) in [
            (10, vec![0x5341]),
            (11, vec![0x5341, 0x4E00]),
            (20, vec![0x4E8C, 0x5341]),
            (101, vec![0x4E00, 0x767E, 0x96F6, 0x4E00]),
            (110, vec![0x4E00, 0x767E, 0x4E00, 0x5341]),
            (1001, vec![0x4E00, 0x5343, 0x96F6, 0x4E00]),
            (10010, vec![0x4E00, 0x4E07, 0x96F6, 0x4E00, 0x5341]),
        ] {
            assert_eq!(digits(&ChineseCounting.format(n)), want, "{n}");
        }
        // Legal numerals always write the leading one: 10, 11, 105.
        for (n, want) in [(10, vec![0x58F9, 0x62FE]), (11, vec![0x58F9, 0x62FE, 0x58F9]), (105, vec![0x58F9, 0x4F70, 0x96F6, 0x4F0D])] {
            assert_eq!(digits(&ChineseLegalSimplified.format(n)), want, "{n}");
        }
        assert_eq!(ChineseCounting.format(123_456_789), "123456789");
    }

    use super::*;

    #[test]
    fn number_formats() {
        assert_eq!(NumFormat::UpperRoman.format(1994), "MCMXCIV");
        assert_eq!(NumFormat::LowerLetter.format(27), "aa");
        assert_eq!(NumFormat::LowerLetter.format(0), "");
        assert_eq!(NumFormat::Ordinal.format(12), "12th");
        assert_eq!(NumFormat::Ordinal.format(22), "22nd");
        assert_eq!(NumFormat::CardinalText.format(21), "Twenty-One");
        assert_eq!(NumFormat::OrdinalText.format(21), "Twenty-First");
        assert_eq!(NumFormat::OrdinalText.format(20), "Twentieth");
        assert_eq!(NumFormat::OrdinalText.format(112), "One Hundred Twelfth");
        assert_eq!(NumFormat::DecimalZero.format(3), "03");
        assert!(!NumFormat::UpperRoman.format(u32::MAX).is_empty());
        assert_eq!(NumFormat::LowerLetter.format(u32::MAX).len(), 30);
    }

    #[test]
    fn landscape_swaps() {
        let mut s = SectionProps { margin_left: 50.0, ..Default::default() };
        s.set_landscape(true);
        assert_eq!((s.page_w, s.page_h), (792.0, 612.0));
        assert_eq!(s.margin_top, 50.0);
        s.set_landscape(false);
        assert_eq!(s.margin_left, 50.0);
        assert_eq!((s.page_w, s.page_h), (612.0, 792.0));
    }

    #[test]
    fn columns_split() {
        let s = SectionProps { columns: Columns { count: 2, space: 36.0, ..Default::default() }, ..Default::default() };
        let b = s.column_boxes();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0], (0.0, 216.0));
        assert_eq!(b[1], (252.0, 216.0));
    }
}
