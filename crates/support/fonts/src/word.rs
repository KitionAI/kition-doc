//! Word-processor font resolution: map a requested family + bold/italic to an available face,
//! with metric-compatible substitutes for common proprietary document fonts, and Word-style line
//! metrics (Windows ascent/descent, which is what Word uses for single line spacing).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use skrifa::raw::TableProvider;

use crate::{FaceRef, FontDb};

/// A requested font resolved to a face.
#[derive(Clone, Copy, Debug)]
pub struct Resolved {
    pub face: FaceRef,
    /// The face isn't bold but bold was asked for: embolden when drawing.
    pub synth_bold: bool,
    /// The face isn't italic but italic was asked for: slant when drawing.
    pub synth_italic: bool,
    /// The requested family wasn't available.
    pub substituted: bool,
}

/// 宋体 and its relatives: WPS draws them in a Song face one em from ascent to descent (its own
/// HYShuSongErKW on macOS), so the installed STSong (华文宋体) comes first, before Songti SC,
/// whose taller line height would double lines snapped to a document grid; then open Song faces.
const SONG: &[&str] = &["STSong", "Songti SC", "Noto Serif CJK SC", "Source Han Serif SC", "Source Han Serif CN"];

/// 黑体: WPS's own Hei face is about one em tall; Heiti SC is the installed face closest to it.
const HEI: &[&str] = &["Heiti SC", "STHeiti"];
/// 楷体: the installed STKaiti (华文楷体), one em tall.
const KAI: &[&str] = &["STKaiti", "Kaiti SC"];
/// 仿宋: STFangsong (华文仿宋) when installed, else a Song face of the same one-em height.
const FANGSONG: &[&str] = &["STFangsong", "STSong", "Songti SC"];

/// 新細明體 (PMingLiU) and its relatives: WPS for Mac draws them in the installed Songti TC, whose
/// Windows ascent plus descent is one em.
const MING: &[&str] = &["Songti TC", "STSong", "Noto Serif CJK TC", "Source Han Serif TC"];

/// The faces to try for a missing Chinese family Word documents name (lowercased), as WPS for
/// Mac draws them (a synthetic calibration of its exports): macOS faces by their Chinese names,
/// then Song, Ming, Hei, Kai or FangSong by name, and other Chinese names by the style they name
/// (方正小标宋简体 is a Song face, 方正黑体_GBK a Hei one). `None` for other families: WPS draws
/// them in its sans fallback, the English names KaiTi_GB2312 and FangSong_GB2312 included (the
/// latter at its 微软雅黑 face's line height, [`measured_line_metrics`]).
fn regional_list(f: &str) -> Option<&'static [&'static str]> {
    match f {
        "冬青黑体简体中文" | "冬青黑体" => Some(&["Hiragino Sans GB"]),
        "华文黑体" | "华文细黑" | "stxihei" => Some(&["STXihei", "STHeiti", "Heiti SC"]),
        "华文行楷" | "stxingkai" => Some(&["Xingkai SC", "STXingkai"]),
        "隶书" | "lisu" => Some(&["Baoli SC"]),
        // Japanese names WPS for Mac draws in Hiragino; it draws other Japanese names (MS Gothic,
        // 游明朝, Yu Gothic…) in its sans fallback.
        "ｍｓ 明朝" | "ＭＳ 明朝" | "ms mincho" | "meiryo" => Some(&["Hiragino Sans", "Hiragino Kaku Gothic ProN"]),
        "ヒラギノ角ゴ pro w3" | "ヒラギノ角ゴ pro" | "ヒラギノ角ゴ pron w3" | "ヒラギノ角ゴ pron" => {
            Some(&["Hiragino Kaku Gothic Pro", "Hiragino Kaku Gothic ProN", "Hiragino Sans"])
        }
        "宋体" | "simsun" | "新宋体" | "nsimsun" | "华文中宋" | "stzhongsong" => Some(SONG),
        "pmingliu" | "mingliu" | "新細明體" | "細明體" | "新细明体" | "细明体" => Some(MING),
        "黑体" | "simhei" => Some(HEI),
        "楷体" | "kaiti" => Some(KAI),
        "仿宋" | "fangsong" => Some(FANGSONG),
        "kaiti_gb2312" | "fangsong_gb2312" => None,
        // 微软雅黑 is drawn in WPS's own YaHei face: the sans fallback stands in for it.
        _ if f.contains("雅黑") => None,
        _ if f.contains("仿宋") => Some(FANGSONG),
        _ if f.contains('宋') => Some(SONG),
        _ if f.contains('黑') => Some(HEI),
        _ if f.contains('楷') => Some(KAI),
        _ => None,
    }
}

/// Substitutes to try, in order, for a family that isn't installed, as today's WPS for Mac draws
/// them (a synthetic calibration of its exports): Calibri and Aptos in Helvetica Neue (after the
/// metric-compatible Carlito), and every other missing Latin family (Cambria, Consolas, Segoe UI,
/// Garamond…) in PingFang SC. Where PingFang SC isn't installed, open fonts that are
/// metric-compatible or of the same style follow (Caladea ↔ Cambria, Liberation ↔ Arial / Times
/// New Roman / Courier New), then common system fonts, then our bundled families.
fn substitutes(family: &str) -> &'static [&'static str] {
    const CALIBRI: &[&str] =
        &["Carlito", "Helvetica Neue", "Times New Roman", "Arial", "Liberation Sans", "Noto Sans", "DejaVu Sans", "Source Sans 3"];
    const SANS: &[&str] = &["PingFang SC", "Helvetica Neue", "Arial", "Liberation Sans", "Noto Sans", "DejaVu Sans", "Source Sans 3"];
    const SERIF: &[&str] =
        &["PingFang SC", "Caladea", "Times New Roman", "Liberation Serif", "Georgia", "Noto Serif", "DejaVu Serif", "Source Serif 4"];
    const MONO: &[&str] = &["PingFang SC", "Cousine", "Liberation Mono", "Menlo", "Courier New", "DejaVu Sans Mono", "JetBrains Mono"];
    const COURIER: &[&str] = &["Courier New", "Cousine", "Liberation Mono", "Menlo", "DejaVu Sans Mono", "JetBrains Mono"];
    // Equations: WPS draws them in a math face of its own, not the missing-family fallback.
    const MATH: &[&str] = &["Caladea", "Times New Roman", "Liberation Serif", "Georgia", "Noto Serif", "DejaVu Serif", "Source Serif 4"];
    const ARIAL: &[&str] = &["Arimo", "Liberation Sans", "Helvetica", "Helvetica Neue", "Source Sans 3"];
    const TIMES: &[&str] = &["Tinos", "Liberation Serif", "Times", "Source Serif 4"];
    const SYMBOL: &[&str] = &["Apple Symbols", "Segoe UI Symbol", "DejaVu Sans", "Noto Sans Symbols", "Source Sans 3"];
    // Persian / Arabic document fonts. Naskh (book) faces such as B Nazanin, B Lotus, B Zar and
    // Traditional Arabic, then the sans faces (B Yekan, B Titr, IRANSans) people use for
    // headings and screens. Open fonts with full Persian coverage first.
    const NASKH: &[&str] = &["Noto Naskh Arabic", "XB Zar", "XB Niloofar", "Amiri", "Vazirmatn", "Noto Sans Arabic", "DejaVu Sans", "Source Serif 4"];
    const ARABIC_SANS: &[&str] = &["Vazirmatn", "Sahel", "Shabnam", "Samim", "Noto Sans Arabic", "Noto Naskh Arabic", "DejaVu Sans", "Source Sans 3"];
    let f = family.to_ascii_lowercase();
    if is_persian_naskh(&f) {
        return NASKH;
    }
    if is_persian_sans(&f) {
        return ARABIC_SANS;
    }
    if let Some(list) = regional_list(&f) {
        return list;
    }
    match f.as_str() {
        "calibri" | "calibri light" | "aptos" | "aptos display" | "aptos narrow" => CALIBRI,
        "cambria math" => MATH,
        "arial" | "helvetica" | "arial nova" => ARIAL,
        "times new roman" | "times" => TIMES,
        "courier new" | "courier" => COURIER,
        "consolas" | "lucida console" | "cascadia code" | "cascadia mono" => MONO,
        "symbol" | "wingdings" | "segoe ui symbol" | "webdings" => SYMBOL,
        "cambria"
        | "georgia"
        | "garamond"
        | "book antiqua"
        | "palatino linotype"
        | "constantia"
        | "baskerville old face"
        | "century"
        | "century schoolbook"
        | "bookman old style"
        | "sitka text"
        | "sitka"
        | "aptos serif" => SERIF,
        _ if f.contains("mono") || f.contains("code") => MONO,
        _ if f.contains("serif") && !f.contains("sans") => SERIF,
        _ => SANS,
    }
}

/// Naskh-style Persian/Arabic document fonts (the "B …" family names come from the common
/// Persian font packs; matched by name only).
fn is_persian_naskh(f: &str) -> bool {
    const NAMES: &[&str] = &[
        "b nazanin",
        "b lotus",
        "b zar",
        "b mitra",
        "b badr",
        "b yagut",
        "b roya",
        "b compset",
        "b nazanin bold",
        "b lotus bold",
        "nazanin",
        "lotus",
        "zar",
        "mitra",
        "badr",
        "yagut",
        "roya",
        "ir nazanin",
        "ir lotus",
        "ir zar",
        "ir mitra",
        "ir badr",
        "irnazanin",
        "irlotus",
        "irzar",
        "irmitra",
        "irbadr",
        "traditional arabic",
        "simplified arabic",
        "arabic typesetting",
        "sakkal majalla",
        "adobe arabic",
        "andalus",
        "times new roman (arabic)",
    ];
    NAMES.contains(&f)
}

/// Sans Persian/Arabic fonts for headings and screens.
fn is_persian_sans(f: &str) -> bool {
    const NAMES: &[&str] = &[
        "b yekan",
        "b titr",
        "b homa",
        "b koodak",
        "b traffic",
        "b jadid",
        "b elham",
        "b kamran",
        "b davat",
        "b ferdosi",
        "yekan",
        "titr",
        "homa",
        "koodak",
        "traffic",
        "ir titr",
        "ir homa",
        "irtitr",
        "irhoma",
        "iransans",
        "iransansweb",
        "iran sans",
        "iranyekan",
        "iran yekan",
        "iranyekanweb",
        "dana",
        "estedad",
        "dubai",
        "aldhabi",
        "urdu typesetting",
        "arabic transparent",
    ];
    NAMES.contains(&f) || f.starts_with("iransans") || f.starts_with("iranyekan")
}

fn style_name(bold: bool, italic: bool) -> &'static str {
    match (bold, italic) {
        (false, false) => "Regular",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (true, true) => "Bold Italic",
    }
}

/// Resolve `family` with bold/italic to a face (memoised).
pub fn resolve(family: &str, bold: bool, italic: bool) -> Resolved {
    static CACHE: OnceLock<Mutex<HashMap<(String, bool, bool), (u64, Resolved)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (family.to_string(), bold, italic);
    let db = FontDb::global();
    let revision = db.revision();
    if let Some((cached_revision, r)) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key)
        && *cached_revision == revision
    {
        return *r;
    }
    let available = |name: &str| db.has_family(name).then(|| db.face(name, style_name(bold, italic))).filter(|face| face.named(name));
    let requested = available(family);
    let substituted = requested.is_none();
    let face = requested
        .or_else(|| substitutes(family).iter().find_map(|name| available(name)))
        .unwrap_or_else(|| db.face(crate::FALLBACK_FAMILY, style_name(bold, italic)));
    let r = Resolved { synth_bold: bold && face.weight < 600.0, synth_italic: italic && !face.italic, substituted, face: FaceRef::of(&face) };
    // A font may load while resolving. Keeping the revision observed before the lookup
    // makes that entry stale instead of caching a result across a concurrent registration.
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(key, (revision, r));
    r
}

/// The regional repertoire declared by a Word font-table character set.
pub fn charset_language(charset: u8) -> Option<&'static str> {
    match charset {
        0x80 => Some("ja"),
        0x81 | 0x82 => Some("ko"),
        0x86 => Some("zh-CN"),
        0x88 => Some("zh-TW"),
        _ => None,
    }
}

pub(crate) fn charset_fallback(db: &FontDb, charset: u8, bold: bool, italic: bool) -> Option<Resolved> {
    let language = charset_language(charset)?;
    let sample = match language {
        "ja" => '\u{3042}',
        "ko" => '\u{ac00}',
        _ => '\u{4e2d}',
    };
    east_asian_fallback(db, Some(language), sample, bold, italic)
}

/// A missing declared East Asian family keeps its repertoire for Latin text and
/// paragraph marks too. Installed requested families always take precedence.
pub fn resolve_with_charset(family: &str, charset: Option<u8>, bold: bool, italic: bool) -> Resolved {
    let requested = resolve(family, bold, italic);
    if !requested.substituted {
        return requested;
    }
    if let Some(song) = regional_substitute(FontDb::global(), family, '\u{4e2d}', bold, italic) {
        return song;
    }
    charset.and_then(|value| charset_fallback(FontDb::global(), value, bold, italic)).unwrap_or(requested)
}

/// Resolve an East Asian font slot before falling back by individual character. A
/// Latin face loaded earlier must not choose the regional forms of Han characters.
pub fn resolve_east_asian(family: &str, language: Option<&str>, sample: char, bold: bool, italic: bool) -> Resolved {
    let requested = resolve(family, bold, italic);
    if !requested.substituted && (sample.is_whitespace() || requested.face.covers(sample)) {
        return requested;
    }
    if let Some(song) = regional_substitute(FontDb::global(), family, sample, bold, italic) {
        return song;
    }
    // An installed font without the character (a Latin font named for Chinese text), or the face
    // standing in for a known family without it (Hiragino for MS Mincho has no 导): WPS for Mac
    // draws the Chinese in a Song face; only other missing families go to the sans fallback.
    if (!requested.substituted || regional_list(&family.to_ascii_lowercase()).is_some())
        && simplified_chinese(language, sample)
        && let Some(song) = regional_substitute(FontDb::global(), "宋体", sample, bold, italic)
    {
        return song;
    }
    east_asian_fallback(FontDb::global(), language, sample, bold, italic).unwrap_or(requested)
}

/// The face for a character the run's font lacks (outside complex scripts): WPS for Mac draws
/// arrows, enclosed numbers and other symbols in its Song face, then PingFang SC (corpus
/// references). The installed one-em STSong stands in for its Song face. `None`: none covers it.
pub fn symbol_fallback(c: char, bold: bool, italic: bool) -> Option<Resolved> {
    symbol_fallback_in(FontDb::global(), c, bold, italic)
}

pub(crate) fn symbol_fallback_in(db: &FontDb, c: char, bold: bool, italic: bool) -> Option<Resolved> {
    ["STSong", "PingFang SC"].iter().find_map(|candidate| {
        if !db.has_family(candidate) {
            return None;
        }
        let face = db.face(candidate, style_name(bold, italic));
        (face.named(candidate) && face.covers(c)).then(|| Resolved {
            synth_bold: bold && face.weight < 600.0,
            synth_italic: italic && !face.italic,
            substituted: true,
            face: FaceRef::of(&face),
        })
    })
}

/// Whether [`east_asian_fallback`] would pick a Simplified Chinese face for `sample`.
fn simplified_chinese(language: Option<&str>, sample: char) -> bool {
    let language = language.unwrap_or("").to_ascii_lowercase();
    !(language.starts_with("ja")
        || language.starts_with("ko")
        || language.starts_with("zh-tw")
        || language.starts_with("zh-hk")
        || language.contains("hant")
        || matches!(sample as u32, 0x3040..=0x30ff | 0x31f0..=0x31ff | 0x1100..=0x11ff | 0x3130..=0x318f | 0xac00..=0xd7af))
}

/// A missing Chinese family of a known style (宋体, 新細明體, 黑体, 楷体, 仿宋 and their relatives) drawn in
/// the first installed face of its style that has `sample`, before the sans regional fallback.
/// `None` for other families.
pub(crate) fn regional_substitute(db: &FontDb, family: &str, sample: char, bold: bool, italic: bool) -> Option<Resolved> {
    regional_list(&family.to_ascii_lowercase())?.iter().find_map(|candidate| {
        if !db.has_family(candidate) {
            return None;
        }
        let face = db.face(candidate, style_name(bold, italic));
        (face.named(candidate) && (sample.is_whitespace() || face.covers(sample))).then(|| Resolved {
            synth_bold: bold && face.weight < 600.0,
            synth_italic: italic && !face.italic,
            substituted: true,
            face: FaceRef::of(&face),
        })
    })
}

pub(crate) fn east_asian_fallback(db: &FontDb, language: Option<&str>, sample: char, bold: bool, italic: bool) -> Option<Resolved> {
    let language = language.unwrap_or("").to_ascii_lowercase();
    let candidates: &[&str] = if language.starts_with("ja") || matches!(sample as u32, 0x3040..=0x30ff | 0x31f0..=0x31ff) {
        &["Hiragino Mincho ProN", "Yu Mincho", "Noto Serif CJK JP", "Source Han Serif JP", "Hiragino Sans", "Noto Sans CJK JP", "Source Han Sans JP"]
    } else if language.starts_with("ko") || matches!(sample as u32, 0x1100..=0x11ff | 0x3130..=0x318f | 0xac00..=0xd7af) {
        &["Apple SD Gothic Neo", "Malgun Gothic", "Noto Sans CJK KR", "Source Han Sans KR"]
    } else if language.starts_with("zh-tw") || language.starts_with("zh-hk") || language.contains("hant") {
        &["PingFang TC", "Microsoft JhengHei", "Noto Sans CJK TC", "Source Han Sans TC", "Heiti TC"]
    } else {
        &["PingFang SC", "Microsoft YaHei", "Noto Sans CJK SC", "Source Han Sans SC", "Source Han Sans CN", "Heiti SC", "Droid Sans Fallback"]
    };
    for candidate in candidates {
        if db.has_family(candidate) {
            let face = db.face(candidate, style_name(bold, italic));
            if face.named(candidate) && (sample.is_whitespace() || face.covers(sample)) {
                return Some(Resolved {
                    synth_bold: bold && face.weight < 600.0,
                    synth_italic: italic && !face.italic,
                    substituted: true,
                    face: FaceRef::of(&face),
                });
            }
        }
    }
    None
}

/// The standard Unicode character for a Symbol or Wingdings character stored the way Word stores
/// it, as the private-use code U+F000 + the font's own code (U+F0B7 is the Symbol bullet). Used
/// when that font isn't installed: a substitute draws something else, or nothing, at the
/// private-use code. `None` for other fonts and for codes not in the table.
pub fn symbol_font_char(family: &str, c: char) -> Option<char> {
    let code = u32::from(c).checked_sub(0xF000)?;
    let u = match (family.to_ascii_lowercase().as_str(), code) {
        // Adobe Symbol encoding.
        ("symbol", 0xA7) => '\u{2663}',
        ("symbol", 0xA8) => '\u{2666}',
        ("symbol", 0xA9) => '\u{2665}',
        ("symbol", 0xAA) => '\u{2660}',
        ("symbol", 0xAE) => '\u{2192}',
        ("symbol", 0xB7) => '\u{2022}',
        ("symbol", 0xD7) => '\u{22C5}',
        ("symbol", 0xE0) => '\u{25CA}',
        // Wingdings characters common as list bullets.
        ("wingdings", 0x6C) => '\u{25CF}',
        ("wingdings", 0x6E) => '\u{25A0}',
        ("wingdings", 0x76) => '\u{2756}',
        ("wingdings", 0xA7) => '\u{25AA}',
        ("wingdings", 0xD8) => '\u{27A2}',
        ("wingdings", 0xFC) => '\u{2714}',
        _ => return None,
    };
    Some(u)
}

/// Single line spacing of a font for East Asian text, as a multiple of its Windows ascent plus
/// descent (measured in Word across East Asian fonts; WPS lays such fonts out the same way).
const EAST_ASIAN_LINE_FACTOR: f64 = 1.3;

/// OS/2 `ulCodePageRange1` bits of the East Asian code pages: 932 (Japanese), 936 (Chinese
/// Simplified), 949 (Korean Wansung), 950 (Chinese Traditional) and 1361 (Korean Johab).
const EAST_ASIAN_CODE_PAGES: u32 = 0b1_1111 << 17;

/// Whether a font declares an East Asian code page, which is what makes a font one for East
/// Asian text in its line height, whatever text it draws.
fn declares_east_asian_code_page(f: &skrifa::FontRef<'_>) -> bool {
    f.os2().ok().and_then(|o| o.ul_code_page_range_1()).is_some_and(|pages| pages & EAST_ASIAN_CODE_PAGES != 0)
}

/// Line metrics for a face in font units: (ascent, descent), both positive, the way word
/// processors measure single line spacing: hhea values plus line gap (OS/2 usWinAscent and
/// usWinDescent when those are missing), and for a font that declares an East Asian code page,
/// Line metrics (ascent, descent, in em) WPS for Mac gives families it draws in faces of its own
/// that nothing installed here matches, measured on its exports (a synthetic calibration, 9 to 22
/// pt, single lines): 仿宋 in a face 1.113 em from Windows ascent to descent, 微软雅黑 in one 1.216
/// em, each with the East Asian single-line factor (about 1.45 and 1.58 em). `None` for families
/// whose stand-in already measures as WPS draws them.
pub fn measured_line_metrics(family: &str) -> Option<(f64, f64)> {
    let (ascent, descent) = match family.to_ascii_lowercase().as_str() {
        // FangSong_GB2312 (the English name) is drawn in WPS's 微软雅黑 face.
        "微软雅黑" | "microsoft yahei" | "微软雅黑 light" | "microsoft yahei light" | "microsoft yahei ui" | "fangsong_gb2312" => {
            (0.973, 0.243)
        }
        "fangsong" => (0.863, 0.25),
        f if f.contains("仿宋") => (0.863, 0.25),
        // WPS's own Song, Hei and Kai faces stand 0.969 em above the baseline and 0.329 em below.
        f if !f.starts_with("华文") && regional_list(f).is_some_and(|l| [SONG, HEI, KAI].iter().any(|s| std::ptr::eq(*s, l))) => {
            return Some((0.969, 0.329));
        }
        _ => return None,
    };
    let extra = (ascent + descent) * (EAST_ASIAN_LINE_FACTOR - 1.0) / 2.0;
    Some((ascent + extra, descent + extra))
}

/// 1.3 times its Windows ascent plus descent with the extra split evenly above and below.
pub fn line_metrics(face: &crate::FontFace) -> (f64, f64) {
    static CACHE: OnceLock<Mutex<HashMap<u32, (f64, f64)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&face.id()) {
        return *v;
    }
    let mut v = (face.ascent.max(1.0), face.descent.abs());
    if let Some(f) = face.skrifa() {
        // hhea ascender + descender + line gap (Word on macOS); OS/2 win metrics only when the
        // hhea values are missing, and never more than twice the em.
        // The line gap goes above the text: beside an East Asian font, a Latin font's ascent plus
        // its gap is what stands taller (WPS for Mac references).
        let hhea = f.hhea().ok().map(|h| {
            let gap = h.line_gap().to_i16().max(0) as f64;
            (h.ascender().to_i16() as f64 + gap, (h.descender().to_i16() as f64).abs())
        });
        let win = f.os2().ok().map(|o| (o.us_win_ascent() as f64, o.us_win_descent() as f64));
        let ok = |m: &(f64, f64)| m.0 > 0.0 && m.0 + m.1 <= face.upem * 2.0;
        if let Some((a, d)) = win.filter(ok).filter(|_| declares_east_asian_code_page(&f)) {
            let extra = (a + d) * (EAST_ASIAN_LINE_FACTOR - 1.0) / 2.0;
            v = (a + extra, d + extra);
        } else if let Some(m) = hhea.filter(ok) {
            v = m;
        } else if let Some(m) = win.filter(ok) {
            v = m;
        }
    }
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(face.id(), v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_latin_families_fall_back_to_pingfang_except_calibri_and_aptos() {
        // WPS for Mac draws Calibri and Aptos in Helvetica Neue (after the metric-compatible
        // Carlito) and every other missing Latin family in PingFang SC (synthetic calibration).
        for family in ["Calibri", "Calibri Light", "Aptos"] {
            assert_eq!(&substitutes(family)[..2], &["Carlito", "Helvetica Neue"], "{family}");
        }
        for family in ["Segoe UI", "Cambria", "Garamond", "Consolas", "Source Code Pro", "Definitely Not A Font 123"] {
            assert_eq!(substitutes(family).first(), Some(&"PingFang SC"), "{family}");
        }
        // Equations keep a serif face; Courier New and Times New Roman their own.
        assert_ne!(substitutes("Cambria Math").first(), Some(&"PingFang SC"));
        assert_eq!(substitutes("Courier New").first(), Some(&"Courier New"));
        assert_eq!(substitutes("Times New Roman").first(), Some(&"Tinos"));
    }

    #[test]
    fn song_families_fall_back_to_a_song_face_with_song_metrics() {
        // WPS draws 宋体 and its relatives in a one-em Song face: the installed STSong (华文宋体)
        // comes before the taller Songti SC.
        for family in ["宋体", "SimSun", "simsun", "新宋体", "NSimSun", "华文中宋", "STZhongsong"] {
            assert_eq!(substitutes(family).first(), Some(&"STSong"), "{family}");
            assert!(substitutes(family).contains(&"Songti SC"), "{family}");
        }
        assert_ne!(substitutes("微软雅黑").first(), Some(&"STSong"));
    }

    #[test]
    fn hei_kai_and_fangsong_families_fall_back_to_a_face_of_their_own_style() {
        // WPS draws 黑体, 楷体 and 仿宋 in its own one-em faces of the same style; the installed
        // macOS faces closest in style and height come first.
        for family in ["黑体", "SimHei"] {
            assert_eq!(substitutes(family).first(), Some(&"Heiti SC"), "{family}");
        }
        for family in ["楷体", "KaiTi", "楷体_GB2312"] {
            assert_eq!(substitutes(family).first(), Some(&"STKaiti"), "{family}");
        }
        for family in ["仿宋", "FangSong", "仿宋_GB2312"] {
            assert_eq!(substitutes(family).first(), Some(&"STFangsong"), "{family}");
            assert!(substitutes(family).contains(&"STSong"), "{family}");
        }
    }

    #[test]
    fn chinese_names_resolve_to_the_style_they_name_or_the_macos_face_they_name() {
        // WPS for Mac's export (synthetic calibration): 方正 and other Chinese names are drawn in
        // the Song, FangSong, Hei or Kai face their name says; macOS faces by their Chinese names.
        for (family, expected) in [
            ("方正小标宋简体", "STSong"),
            ("方正大标宋公文", "STSong"),
            ("方正仿宋_GBK", "STFangsong"),
            ("方正黑体_GBK", "Heiti SC"),
            ("方正楷体_GBK", "STKaiti"),
            ("冬青黑体简体中文", "Hiragino Sans GB"),
            ("华文黑体", "STXihei"),
            ("华文行楷", "Xingkai SC"),
            ("隶书", "Baoli SC"),
        ] {
            assert_eq!(substitutes(family).first(), Some(&expected), "{family}");
        }
        // Names of no style, and 微软雅黑, go to the sans fallback.
        for family in ["幼圆", "方正魏碑", "微软雅黑"] {
            assert_eq!(regional_list(&family.to_ascii_lowercase()), None, "{family}");
        }
        let total = measured_line_metrics("方正仿宋_GBK").map(|(a, d)| a + d);
        assert!(total.is_some_and(|t| (t - 1.45).abs() < 0.01), "{total:?}");
    }

    #[test]
    fn the_english_gb2312_names_are_drawn_in_the_sans_fallback() {
        // WPS for Mac draws KaiTi_GB2312 in PingFang SC and FangSong_GB2312 in its 微软雅黑 face,
        // unlike their Chinese names 楷体_GB2312 and 仿宋_GB2312.
        for family in ["KaiTi_GB2312", "FangSong_GB2312"] {
            assert_eq!(regional_list(&family.to_ascii_lowercase()), None, "{family}");
        }
        assert_eq!(measured_line_metrics("FangSong_GB2312"), measured_line_metrics("微软雅黑"));
        assert_eq!(measured_line_metrics("KaiTi_GB2312"), None);
    }

    #[test]
    fn ms_mincho_and_meiryo_are_drawn_in_hiragino_other_japanese_names_in_the_sans_fallback() {
        // WPS for Mac's export (synthetic calibration, Chinese text at 10.5 and 16 pt): ＭＳ 明朝,
        // MS Mincho and Meiryo in Hiragino Sans (characters it lacks in a Song face); ヒラギノ角ゴ
        // Pro W3 is the installed Hiragino Kaku Gothic Pro; MS Gothic, 游明朝 and the others go to
        // the sans fallback.
        for family in ["ＭＳ 明朝", "MS Mincho", "Meiryo"] {
            assert_eq!(substitutes(family).first(), Some(&"Hiragino Sans"), "{family}");
        }
        assert_eq!(substitutes("ヒラギノ角ゴ Pro W3").first(), Some(&"Hiragino Kaku Gothic Pro"));
        for family in ["ＭＳ ゴシック", "MS Gothic", "游明朝", "Yu Gothic", "メイリオ"] {
            assert_eq!(regional_list(&family.to_ascii_lowercase()), None, "{family}");
        }
    }

    #[test]
    fn ming_families_fall_back_to_a_traditional_song_face() {
        // WPS for Mac draws 新細明體 (PMingLiU) in the installed Songti TC.
        for family in ["PMingLiU", "MingLiU", "新細明體", "細明體", "新细明体"] {
            assert_eq!(substitutes(family).first(), Some(&"Songti TC"), "{family}");
        }
    }

    #[test]
    fn wps_line_heights_are_measured_for_families_it_draws_in_its_own_faces() {
        // Single lines measured in WPS for Mac's export (synthetic calibration, 9 to 22 pt): 仿宋
        // about 1.45 em, 微软雅黑 about 1.58 em, whatever face stands in for them here.
        let total = |family: &str| measured_line_metrics(family).map(|(a, d)| a + d);
        for family in ["仿宋", "FangSong", "仿宋_GB2312"] {
            assert!(total(family).is_some_and(|t| (t - 1.45).abs() < 0.01), "{family}: {:?}", total(family));
        }
        for family in ["微软雅黑", "Microsoft YaHei", "微软雅黑 Light", "FangSong_GB2312"] {
            assert!(total(family).is_some_and(|t| (t - 1.58).abs() < 0.01), "{family}: {:?}", total(family));
        }
        // WPS's own Song, Hei and Kai faces: 0.969 em above the baseline, 0.329 em below, for 宋体 and
        // the names drawn in them (the installed stand-ins stand taller). Installed faces by their Chinese names keep theirs.
        for family in ["宋体", "SimSun", "黑体", "楷体", "方正小标宋简体", "方正黑体_GBK"] {
            assert_eq!(measured_line_metrics(family), Some((0.969, 0.329)), "{family}");
        }
        assert_eq!(measured_line_metrics("华文宋体"), None, "the installed STSong measures as itself");
    }

    #[test]
    fn a_latin_fonts_line_gap_goes_above_the_text() {
        // Beside an East Asian font a Latin font's ascent plus its whole line gap is what stands
        // taller (WPS for Mac: a 12 pt Song line with Helvetica Neue text takes two grid rows).
        let font = resolve("Source Sans 3", false, false).face.get();
        let f = font.skrifa().unwrap();
        let h = f.hhea().unwrap();
        let (gap, ascent, descent) = (h.line_gap().to_i16().max(0) as f64, h.ascender().to_i16() as f64, (h.descender().to_i16() as f64).abs());
        assert_eq!(line_metrics(font), (ascent + gap, descent));
    }

    #[test]
    fn unknown_family_substitutes() {
        let r = resolve("Definitely Not A Font 123", false, false);
        assert!(r.substituted);
        let b = resolve("Source Serif 4", true, false);
        assert!(!b.substituted);
        assert!(!b.synth_bold);
        let (a, d) = line_metrics(&b.face);
        assert!(a > 0.0 && d >= 0.0);
    }

    #[test]
    fn symbol_font_chars_map_to_unicode() {
        assert_eq!(symbol_font_char("Symbol", '\u{F0B7}'), Some('\u{2022}'));
        assert_eq!(symbol_font_char("symbol", '\u{F0B7}'), Some('\u{2022}'));
        assert_eq!(symbol_font_char("Wingdings", '\u{F0A7}'), Some('\u{25AA}'));
        assert_eq!(symbol_font_char("Wingdings", '\u{F0D8}'), Some('\u{27A2}'));
        // Same code, other font; unknown codes; ordinary characters.
        assert_eq!(symbol_font_char("Arial", '\u{F0B7}'), None);
        assert_eq!(symbol_font_char("Symbol", '\u{F041}'), None);
        assert_eq!(symbol_font_char("Symbol", 'a'), None);
        assert_eq!(symbol_font_char("Symbol", '\u{10FFFF}'), None);
    }

    #[test]
    fn resolves_weights_named_as_their_own_family() {
        // Word stores a Medium face as the family "Inter Medium".
        let r = resolve("Inter Medium", false, false);
        assert!(!r.substituted && !r.synth_bold && !r.synth_italic);
        assert_eq!((r.face.family.as_str(), r.face.style.as_str()), ("Inter", "Medium"));
        // Bold and italic within it are drawn from the Medium face.
        let b = resolve("Inter Medium", true, true);
        assert_eq!(b.face.style, "Medium");
        assert!(!b.substituted && b.synth_bold && b.synth_italic);
    }

    #[test]
    fn synthesizes_missing_styles() {
        // JetBrains Mono ships only Regular here.
        let r = resolve("JetBrains Mono", true, true);
        assert!(r.synth_bold && r.synth_italic);
    }
}
