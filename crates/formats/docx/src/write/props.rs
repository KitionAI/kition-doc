//! Property writers, in ECMA-376 schema element order.

use kition_doc_doc::props::{
    Align, Border, BorderStyle, Borders, CellProps, CharProps, HeightRule, Highlight, LineSpacing, ParaProps, Rgb, RowProps, TabAlign, TabLeader,
    TableFloat, TableLook, TableProps, TextColor, ThemeColor, VAlign, VMerge, VertAlign,
};

use crate::units::{n, twips};
use crate::xml::W;

fn toggle(w: &mut W, name: &str, v: Option<bool>) {
    match v {
        Some(true) => w.empty(name, &[]),
        Some(false) => w.val(name, "0"),
        None => {}
    }
}

fn round(v: f32) -> i64 {
    let v = kition_doc_geom::finite(v);
    v.round().clamp(-1e9, 1e9) as i64
}

/// `w:rPr` content (without the wrapper). Link and revision marks are not run properties.
pub fn rpr_inner(w: &mut W, c: &CharProps) {
    if let Some(s) = &c.style {
        w.val("w:rStyle", s);
    }
    // The complex-script font is written only when set: without it, Persian/Arabic text keeps
    // the style's. Complex-script bold, italic and size default to the plain ones (they come in
    // pairs, see `CharProps::overlay`), so text formatted here looks the same in Word.
    // The East Asian font and the script hint likewise.
    // Theme fonts are written while the font next to each still is the one they stood for (Word
    // reads them over the names, which are written too).
    let ft = c.font_theme.as_deref();
    let latin_theme = ft.filter(|t| t.font == c.font && (t.ascii.is_some() || t.h_ansi.is_some()));
    let mut fonts: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = &c.font {
        fonts.extend([("w:ascii", f.as_str()), ("w:hAnsi", f.as_str())]);
    }
    if let Some(t) = latin_theme {
        if let Some(a) = &t.ascii {
            fonts.push(("w:asciiTheme", a));
        }
        if let Some(a) = &t.h_ansi {
            fonts.push(("w:hAnsiTheme", a));
        }
    }
    if let Some(ea) = &c.font_ea {
        fonts.push(("w:eastAsia", ea));
    }
    if let Some(a) = ft.filter(|t| t.font_ea == c.font_ea).and_then(|t| t.east_asia.as_deref()) {
        fonts.push(("w:eastAsiaTheme", a));
    }
    if let Some(cs) = &c.font_cs {
        fonts.push(("w:cs", cs));
    }
    if let Some(a) = ft.filter(|t| t.font_cs == c.font_cs).and_then(|t| t.cs.as_deref()) {
        fonts.push(("w:cstheme", a));
    }
    if let Some(hint) = &c.font_hint {
        fonts.push(("w:hint", hint));
    }
    if !fonts.is_empty() {
        w.empty("w:rFonts", &fonts);
    }
    toggle(w, "w:b", c.bold);
    toggle(w, "w:bCs", c.bold_cs.or(c.bold));
    toggle(w, "w:i", c.italic);
    toggle(w, "w:iCs", c.italic_cs.or(c.italic));
    toggle(w, "w:caps", c.caps);
    toggle(w, "w:smallCaps", c.small_caps);
    toggle(w, "w:strike", c.strike);
    toggle(w, "w:dstrike", c.double_strike);
    toggle(w, "w:outline", c.outline);
    toggle(w, "w:shadow", c.shadow);
    toggle(w, "w:emboss", c.emboss);
    toggle(w, "w:imprint", c.engrave);
    toggle(w, "w:noProof", c.no_proof);
    toggle(w, "w:vanish", c.hidden);
    match c.color {
        Some(TextColor::Auto) => w.val("w:color", "auto"),
        Some(TextColor::Rgb(rgb)) => {
            let mut a = vec![("w:val", rgb.hex())];
            a.extend(theme_attrs(c.color_theme.as_ref(), Some(rgb), THEME_COLOR));
            w.empty_owned("w:color", &a);
        }
        None => {}
    }
    if let Some(s) = c.spacing {
        w.val("w:spacing", &twips(s.clamp(-1584.0, 1584.0)));
    }
    if let Some(s) = c.scale {
        w.val("w:w", &n(round(s.clamp(1.0, 600.0))));
    }
    if let Some(fit) = c.fit_text.filter(|fit| fit.width.is_finite() && fit.width > 0.0) {
        let width = twips(fit.width);
        if let Some(id) = fit.id {
            w.empty("w:fitText", &[("w:val", &width), ("w:id", &id.to_string())]);
        } else {
            w.empty("w:fitText", &[("w:val", &width)]);
        }
    }
    if let Some(layout) = c.east_asian_layout {
        let mut attrs = Vec::new();
        if let Some(id) = layout.id {
            attrs.push(("w:id", id.to_string()));
        }
        if let Some(value) = layout.combine {
            attrs.push(("w:combine", if value { "1" } else { "0" }.to_string()));
        }
        if let Some(value) = layout.combine_brackets {
            attrs.push(("w:combineBrackets", value.to_ooxml().to_string()));
        }
        if let Some(value) = layout.vertical {
            attrs.push(("w:vert", if value { "1" } else { "0" }.to_string()));
        }
        if let Some(value) = layout.vertical_compress {
            attrs.push(("w:vertCompress", if value { "1" } else { "0" }.to_string()));
        }
        let refs: Vec<_> = attrs.iter().map(|(key, value)| (*key, value.as_str())).collect();
        w.empty("w:eastAsianLayout", &refs);
    }
    if let Some(k) = c.kern {
        w.val("w:kern", &n(kition_doc_geom::to_half_points(k.clamp(0.0, 1638.0))));
    }
    if let Some(p) = c.position {
        w.val("w:position", &n(kition_doc_geom::to_half_points(p.clamp(-1584.0, 1584.0))));
    }
    let half = |s: f32| n(kition_doc_geom::to_half_points(s.clamp(1.0, 1638.0)).max(2));
    if let Some(s) = c.size {
        w.val("w:sz", &half(s));
    }
    if let Some(s) = c.size_cs.or(c.size) {
        w.val("w:szCs", &half(s));
    }
    if let Some(h) = c.highlight {
        w.val("w:highlight", if h == Highlight::None { "none" } else { h.ooxml() });
    }
    if let Some(u) = c.underline {
        match c.underline_color {
            Some(col) => w.empty("w:u", &[("w:val", u.ooxml()), ("w:color", &col.hex())]),
            None => w.val("w:u", u.ooxml()),
        }
    }
    if let Some(b) = &c.border {
        border_el(w, "w:bdr", b);
    }
    if let Some(s) = c.shading {
        shd(w, s, c.shading_theme.as_ref());
    }
    if let Some(v) = c.vert_align {
        w.val(
            "w:vertAlign",
            match v {
                VertAlign::Baseline => "baseline",
                VertAlign::Superscript => "superscript",
                VertAlign::Subscript => "subscript",
            },
        );
    }
    toggle(w, "w:rtl", c.rtl);
    toggle(w, "w:cs", c.cs);
    let mut lang: Vec<(&str, &str)> = Vec::new();
    for (attr, value) in [("w:val", &c.lang), ("w:eastAsia", &c.lang_ea), ("w:bidi", &c.lang_bidi)] {
        if let Some(v) = value {
            lang.push((attr, v));
        }
    }
    if !lang.is_empty() {
        w.empty("w:lang", &lang);
    }
    // Word 2010 text effects come last (before a tracked change), as kept.
    if let Some(fx) = c.text_effects.as_deref() {
        w.raw(&text_effects(fx));
    }
}

/// Kept text effects as written: only `w14` elements, re-serialized (whatever else a document
/// carries there is dropped), within [`super::MAX_TEXT_EFFECTS`].
fn text_effects(fx: &str) -> String {
    if fx.len() > super::MAX_TEXT_EFFECTS {
        return String::new();
    }
    let wrapped = format!(r#"<x xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml">{fx}</x>"#);
    let Ok(root) = crate::xml::parse(wrapped.as_bytes()) else { return String::new() };
    root.els().filter(|e| e.name.starts_with("w14:")).map(|e| e.to_xml()).collect()
}

/// Does `c` produce any `w:rPr` content (besides a tracked change)?
pub fn has_rpr(c: &CharProps) -> bool {
    !c.formatting().is_empty()
}

/// Attributes of a tracked change element: `w:id`, `w:author`, `w:date`.
pub type ChangeAttrs = Vec<(&'static str, String)>;

/// Open a tracked change element (`w:rPrChange`, `w:pPrChange`, `w:tblPrChange`…).
pub fn open_change(w: &mut W, tag: &str, a: &ChangeAttrs) {
    let refs: Vec<(&str, &str)> = a.iter().map(|(k, v)| (*k, v.as_str())).collect();
    w.open(tag, &refs);
}

/// A tracked change element holding the properties before the change: `<tag attrs><inner>…</inner></tag>`.
fn change_el(w: &mut W, tag: &str, inner: &str, a: &ChangeAttrs, body: impl FnOnce(&mut W)) {
    open_change(w, tag, a);
    w.open(inner, &[]);
    body(w);
    w.close(inner);
    w.close(tag);
}

pub fn rpr(w: &mut W, c: &CharProps) {
    if has_rpr(c) {
        w.open("w:rPr", &[]);
        rpr_inner(w, c);
        w.close("w:rPr");
    }
}

pub fn align_val(a: Align) -> &'static str {
    match a {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
        Align::Justify => "both",
        Align::Distribute => "distribute",
    }
}

fn border_el(w: &mut W, name: &str, b: &Border) {
    let sz = n(round(b.width.clamp(0.0, 12.0) * 8.0));
    let space = n(round(b.space.clamp(0.0, 31.0)));
    let color = b.color.map(|c| c.hex()).unwrap_or_else(|| "auto".into());
    let style = if b.style == BorderStyle::None { "nil" } else { b.style.ooxml() };
    let mut a = vec![("w:val", style.to_string()), ("w:sz", sz), ("w:space", space), ("w:color", color)];
    a.extend(theme_attrs(b.theme.as_ref(), b.color, THEME_COLOR));
    w.empty_owned(name, &a);
}

/// Attribute names of a theme colour: the colour, its tint, its shade.
pub const THEME_COLOR: [&str; 3] = ["w:themeColor", "w:themeTint", "w:themeShade"];
/// Attribute names of a shading's theme fill.
pub const THEME_FILL: [&str; 3] = ["w:themeFill", "w:themeFillTint", "w:themeFillShade"];

/// The attributes of `theme` when it still stands for `value` (see [`ThemeColor`]).
pub fn theme_attrs(theme: Option<&ThemeColor>, value: Option<Rgb>, names: [&'static str; 3]) -> Vec<(&'static str, String)> {
    let Some(t) = theme.filter(|t| t.holds(value)) else { return Vec::new() };
    let mut a = vec![(names[0], t.slot.ooxml().to_string())];
    if let Some(v) = t.tint {
        a.push((names[1], format!("{v:02X}")));
    }
    if let Some(v) = t.shade {
        a.push((names[2], format!("{v:02X}")));
    }
    a
}

/// A solid `w:shd` fill, with its theme colour while that holds.
pub fn shd(w: &mut W, fill: Rgb, theme: Option<&ThemeColor>) {
    let mut a = vec![("w:val", "clear".to_string()), ("w:color", "auto".to_string()), ("w:fill", fill.hex())];
    a.extend(theme_attrs(theme, Some(fill), THEME_FILL));
    w.empty_owned("w:shd", &a);
}

/// Borders element. `inner` names the between/insideH element (`w:between` for paragraphs,
/// `w:insideH` for tables/cells; `None` for page borders).
pub fn borders(w: &mut W, tag: &str, b: &Borders, inner: Option<&str>, extra: &[(&str, &str)]) {
    w.open(tag, extra);
    if let Some(x) = &b.top {
        border_el(w, "w:top", x);
    }
    if let Some(x) = &b.left {
        border_el(w, "w:left", x);
    }
    if let Some(x) = &b.bottom {
        border_el(w, "w:bottom", x);
    }
    if let Some(x) = &b.right {
        border_el(w, "w:right", x);
    }
    if let Some(name) = inner {
        if let Some(x) = &b.between {
            border_el(w, name, x);
        }
        if name == "w:insideH"
            && let Some(x) = &b.inside_v
        {
            border_el(w, "w:insideV", x);
        }
    }
    w.close(tag);
}

/// `w:pPr` content except the paragraph mark `w:rPr` and `w:sectPr`, which the caller appends.
/// `w:pPr` content (without the wrapper); `num_change` are the attributes of a tracked list
/// numbering change (`w:numberingChange`, written in `w:numPr`).
pub fn ppr_inner(w: &mut W, p: &ParaProps, framed: bool, num_change: Option<&ChangeAttrs>) {
    if let Some(s) = &p.style {
        w.val("w:pStyle", s);
    }
    toggle(w, "w:keepNext", p.keep_next);
    toggle(w, "w:keepLines", p.keep_lines);
    toggle(w, "w:pageBreakBefore", p.page_break_before);
    if framed && let Some(lines) = p.drop_cap.filter(|l| *l > 0) {
        let lines = n(lines.min(10) as i64);
        let space = p.drop_cap_space.filter(|s| s.is_finite() && *s > 0.0).map(|s| n((s * 20.0).round().min(31680.0) as i64));
        let mut attrs = vec![("w:dropCap", "drop"), ("w:lines", lines.as_str())];
        if let Some(space) = &space {
            attrs.push(("w:hSpace", space.as_str()));
        }
        attrs.extend([("w:wrap", "around"), ("w:vAnchor", "text"), ("w:hAnchor", "text")]);
        w.empty("w:framePr", &attrs);
    }
    toggle(w, "w:widowControl", p.widow_control);
    if let Some(nr) = p.numbering {
        w.open("w:numPr", &[]);
        w.val("w:ilvl", &n(nr.level.min(8) as i64));
        w.val("w:numId", &n(nr.num as i64));
        if let Some(a) = num_change {
            let refs: Vec<(&str, &str)> = a.iter().map(|(k, v)| (*k, v.as_str())).collect();
            w.empty("w:numberingChange", &refs);
        }
        w.close("w:numPr");
    }
    toggle(w, "w:suppressLineNumbers", p.suppress_line_numbers);
    if let Some(b) = &p.borders {
        borders(w, "w:pBdr", b, Some("w:between"), &[]);
    }
    if let Some(s) = p.shading {
        shd(w, s, p.shading_theme.as_ref());
    }
    if let Some(tabs) = &p.tabs
        && !tabs.is_empty()
    {
        w.open("w:tabs", &[]);
        for t in tabs.iter().take(64) {
            let val = match t.align {
                TabAlign::Left => "left",
                TabAlign::Center => "center",
                TabAlign::Right => "right",
                TabAlign::Decimal => "decimal",
                TabAlign::Bar => "bar",
                TabAlign::Clear => "clear",
            };
            let pos = twips(t.pos);
            match t.leader {
                TabLeader::None => w.empty("w:tab", &[("w:val", val), ("w:pos", &pos)]),
                l => {
                    let lv = match l {
                        TabLeader::Dot => "dot",
                        TabLeader::Hyphen => "hyphen",
                        TabLeader::Underscore => "underscore",
                        _ => "middleDot",
                    };
                    w.empty("w:tab", &[("w:val", val), ("w:leader", lv), ("w:pos", &pos)]);
                }
            }
        }
        w.close("w:tabs");
    }
    toggle(w, "w:suppressAutoHyphens", p.suppress_hyphens);
    toggle(w, "w:kinsoku", p.kinsoku);
    toggle(w, "w:wordWrap", p.word_wrap);
    toggle(w, "w:overflowPunct", p.overflow_punct);
    toggle(w, "w:topLinePunct", p.top_line_punct);
    toggle(w, "w:autoSpaceDE", p.auto_space_de);
    toggle(w, "w:autoSpaceDN", p.auto_space_dn);
    toggle(w, "w:bidi", p.bidi);
    toggle(w, "w:adjustRightInd", p.adjust_right_ind);
    toggle(w, "w:snapToGrid", p.snap_to_grid);
    if p.space_before.is_some()
        || p.space_after.is_some()
        || p.line_spacing.is_some()
        || p.space_before_lines.is_some()
        || p.space_after_lines.is_some()
        || p.space_before_auto.is_some()
        || p.space_after_auto.is_some()
    {
        let before = p.space_before.map(|v| twips(v.max(0.0)));
        let after = p.space_after.map(|v| twips(v.max(0.0)));
        let before_lines = p.space_before_lines.map(|value| value.to_string());
        let after_lines = p.space_after_lines.map(|value| value.to_string());
        let line = p.line_spacing.map(|l| match l {
            LineSpacing::Multiple(m) => (n(round(m.clamp(0.06, 132.0) * 240.0)), "auto"),
            LineSpacing::AtLeast(v) => (twips(v.abs()), "atLeast"),
            LineSpacing::Exactly(v) => (twips(v.abs()), "exact"),
        });
        let mut a: Vec<(&str, &str)> = Vec::new();
        if let Some(b) = &before {
            a.push(("w:before", b));
        }
        if let Some(x) = &after {
            a.push(("w:after", x));
        }
        if let Some(value) = &before_lines {
            a.push(("w:beforeLines", value));
        }
        if let Some(value) = &after_lines {
            a.push(("w:afterLines", value));
        }
        if let Some(value) = p.space_before_auto {
            a.push(("w:beforeAutospacing", if value { "1" } else { "0" }));
        }
        if let Some(value) = p.space_after_auto {
            a.push(("w:afterAutospacing", if value { "1" } else { "0" }));
        }
        if let Some((l, r)) = &line {
            a.push(("w:line", l));
            a.push(("w:lineRule", r));
        }
        w.empty("w:spacing", &a);
    }
    if p.indent_left.is_some() || p.indent_right.is_some() || p.indent_first.is_some() {
        let l = p.indent_left.map(twips);
        let r = p.indent_right.map(twips);
        let f = p.indent_first.map(|v| (v < 0.0, twips(v.abs())));
        let mut a: Vec<(&str, &str)> = Vec::new();
        if let Some(l) = &l {
            a.push(("w:left", l));
        }
        if let Some(r) = &r {
            a.push(("w:right", r));
        }
        if let Some((hang, v)) = &f {
            a.push((if *hang { "w:hanging" } else { "w:firstLine" }, v));
        }
        w.empty("w:ind", &a);
    }
    toggle(w, "w:contextualSpacing", p.contextual_spacing);
    if let Some(a) = p.align {
        w.val("w:jc", align_val(a));
    }
    if let Some(t) = &p.text_alignment {
        w.val("w:textAlignment", t);
    }
    if let Some(l) = p.outline_level {
        w.val("w:outlineLvl", &n(l.min(9) as i64));
    }
}

pub fn margins(w: &mut W, tag: &str, m: &[f32; 4]) {
    w.open(tag, &[]);
    for (name, v) in ["w:top", "w:left", "w:bottom", "w:right"].iter().zip(m.iter()) {
        w.empty(name, &[("w:w", &twips(v.clamp(0.0, 1584.0))), ("w:type", "dxa")]);
    }
    w.close(tag);
}

pub fn look_attrs(l: &TableLook) -> Vec<(&'static str, String)> {
    let mut bits = 0u32;
    if l.header_row {
        bits |= 0x0020;
    }
    if l.total_row {
        bits |= 0x0040;
    }
    if l.first_column {
        bits |= 0x0080;
    }
    if l.last_column {
        bits |= 0x0100;
    }
    if !l.banded_rows {
        bits |= 0x0200;
    }
    if !l.banded_columns {
        bits |= 0x0400;
    }
    let b = |v: bool| if v { "1".to_string() } else { "0".to_string() };
    vec![
        ("w:val", format!("{bits:04X}")),
        ("w:firstRow", b(l.header_row)),
        ("w:lastRow", b(l.total_row)),
        ("w:firstColumn", b(l.first_column)),
        ("w:lastColumn", b(l.last_column)),
        ("w:noHBand", b(!l.banded_rows)),
        ("w:noVBand", b(!l.banded_columns)),
    ]
}

/// `w:tblPr`, with the properties before a tracked change last when `change` gives its attributes.
pub fn tblpr(w: &mut W, t: &TableProps, change: Option<&ChangeAttrs>) {
    w.open("w:tblPr", &[]);
    tblpr_inner(w, t);
    if let (Some(a), Some(ch)) = (change, &t.fmt_change) {
        change_el(w, "w:tblPrChange", "w:tblPr", a, |w| tblpr_inner(w, &ch.old));
    }
    w.close("w:tblPr");
}

fn tblpr_inner(w: &mut W, t: &TableProps) {
    if let Some(s) = &t.style {
        w.val("w:tblStyle", s);
    }
    if let Some(f) = &t.float {
        table_float(w, f);
    }
    if let Some(p) = t.width_pct {
        w.empty("w:tblW", &[("w:w", &n(round(p.clamp(0.0, 1000.0) * 50.0))), ("w:type", "pct")]);
    } else if let Some(v) = t.width {
        w.empty("w:tblW", &[("w:w", &twips(v.max(0.0))), ("w:type", "dxa")]);
    } else {
        w.empty("w:tblW", &[("w:w", "0"), ("w:type", "auto")]);
    }
    if let Some(a) = t.align {
        w.val("w:jc", align_val(if a == Align::Justify || a == Align::Distribute { Align::Left } else { a }));
    }
    if let Some(s) = t.cell_spacing.filter(|s| s.is_finite() && *s > 0.0) {
        w.empty("w:tblCellSpacing", &[("w:w", &twips(s)), ("w:type", "dxa")]);
    }
    if let Some(i) = t.indent {
        w.empty("w:tblInd", &[("w:w", &twips(i)), ("w:type", "dxa")]);
    }
    if let Some(b) = &t.borders {
        borders(w, "w:tblBorders", b, Some("w:insideH"), &[]);
    }
    if let Some(s) = t.shading {
        shd(w, s, t.shading_theme.as_ref());
    }
    if t.fixed {
        w.empty("w:tblLayout", &[("w:type", "fixed")]);
    }
    if let Some(m) = &t.cell_margins {
        margins(w, "w:tblCellMar", m);
    }
    let look = look_attrs(&t.look);
    let refs: Vec<(&str, &str)> = look.iter().map(|(k, v)| (*k, v.as_str())).collect();
    w.empty("w:tblLook", &refs);
    if let Some(c) = &t.caption {
        w.val("w:tblCaption", c);
    }
}

/// `w:trPr` (when the row has any), with a tracked change last as for [`tblpr`].
pub fn trpr(w: &mut W, r: &RowProps, change: Option<&ChangeAttrs>) {
    let change = change.zip(r.fmt_change.as_ref());
    if r.height.is_none() && !r.header && !r.cant_split && change.is_none() {
        return;
    }
    w.open("w:trPr", &[]);
    trpr_inner(w, r);
    if let Some((a, ch)) = change {
        change_el(w, "w:trPrChange", "w:trPr", a, |w| trpr_inner(w, &ch.old));
    }
    w.close("w:trPr");
}

fn trpr_inner(w: &mut W, r: &RowProps) {
    if r.cant_split {
        w.empty("w:cantSplit", &[]);
    }
    if let Some(h) = r.height {
        let rule = match r.height_rule {
            HeightRule::Auto => "auto",
            HeightRule::AtLeast => "atLeast",
            HeightRule::Exact => "exact",
        };
        w.empty("w:trHeight", &[("w:val", &twips(h.abs())), ("w:hRule", rule)]);
    }
    if r.header {
        w.empty("w:tblHeader", &[]);
    }
}

/// `w:tcPr`, with a tracked change last as for [`tblpr`].
pub fn tcpr(w: &mut W, c: &CellProps, change: Option<&ChangeAttrs>) {
    w.open("w:tcPr", &[]);
    tcpr_inner(w, c);
    if let (Some(a), Some(ch)) = (change, &c.fmt_change) {
        change_el(w, "w:tcPrChange", "w:tcPr", a, |w| tcpr_inner(w, &ch.old));
    }
    w.close("w:tcPr");
}

fn tcpr_inner(w: &mut W, c: &CellProps) {
    match (c.width, c.width_pct) {
        (Some(v), _) => w.empty("w:tcW", &[("w:w", &twips(v.max(0.0))), ("w:type", "dxa")]),
        (None, Some(p)) => w.empty("w:tcW", &[("w:w", &n((p.clamp(0.0, 100.0) * 50.0).round() as i64)), ("w:type", "pct")]),
        (None, None) => w.empty("w:tcW", &[("w:w", "0"), ("w:type", "auto")]),
    }
    if c.span > 1 {
        w.val("w:gridSpan", &n(c.span.min(63) as i64));
    }
    match c.vmerge {
        VMerge::Restart => w.val("w:vMerge", "restart"),
        VMerge::Continue => w.empty("w:vMerge", &[]),
        VMerge::None => {}
    }
    if let Some(b) = &c.borders {
        borders(w, "w:tcBorders", b, Some("w:insideH"), &[]);
    }
    if let Some(s) = c.shading {
        shd(w, s, c.shading_theme.as_ref());
    }
    if c.no_wrap {
        w.empty("w:noWrap", &[]);
    }
    if let Some(m) = &c.margins {
        margins(w, "w:tcMar", m);
    }
    if c.text_direction.is_turned() {
        w.val("w:textDirection", c.text_direction.ooxml());
    }
    match c.valign {
        VAlign::Top if c.valign_set => w.val("w:vAlign", "top"),
        VAlign::Top => {}
        VAlign::Center => w.val("w:vAlign", "center"),
        VAlign::Bottom => w.val("w:vAlign", "bottom"),
    }
}

/// A row's kept table property exceptions (`RowProps::exceptions`) as written: a `w:tblPrEx` of
/// WordprocessingML elements only, re-serialized; anything else is dropped.
pub fn tblpr_ex(w: &mut W, xml: &str) {
    if xml.len() > super::MAX_KEPT_PROPS {
        return;
    }
    let wrapped = format!(r#"<x xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{xml}</x>"#);
    let Ok(root) = crate::xml::parse(wrapped.as_bytes()) else { return };
    let mut ex = root.els().filter(|e| e.name == "w:tblPrEx");
    if let (Some(e), None) = (ex.next(), ex.next())
        && only_wml(e, 0)
    {
        w.raw(&e.to_xml());
    }
}

/// Whether `e` and everything in it are WordprocessingML elements and attributes.
fn only_wml(e: &crate::xml::El, depth: usize) -> bool {
    depth <= crate::xml::MAX_DEPTH
        && e.name.starts_with("w:")
        && e.attrs.iter().all(|(k, _)| k.starts_with("w:"))
        && e.els().all(|c| only_wml(c, depth + 1))
}

/// `w:tblpPr` and `w:tblOverlap` for a floating table.
fn table_float(w: &mut W, f: &TableFloat) {
    use kition_doc_doc::para::Anchor;
    let rel = |a: Anchor, text: &'static str| match a {
        Anchor::Page => "page",
        Anchor::Margin => "margin",
        _ => text,
    };
    let [left, top, right, bottom] = f.dist_from_text().map(twips);
    let fin = |v: f32| twips(if v.is_finite() { v.clamp(-31_680.0, 31_680.0) } else { 0.0 });
    let (x, y) = (fin(f.x), fin(f.y));
    let mut a: Vec<(&str, &str)> = vec![
        ("w:leftFromText", &left),
        ("w:rightFromText", &right),
        ("w:topFromText", &top),
        ("w:bottomFromText", &bottom),
        ("w:vertAnchor", rel(f.v_rel, "text")),
        ("w:horzAnchor", rel(f.h_rel, "text")),
    ];
    match f.h_align {
        Some(h) => a.push(("w:tblpXSpec", h.ooxml(true))),
        None => a.push(("w:tblpX", &x)),
    }
    match f.v_align {
        Some(v) => a.push(("w:tblpYSpec", v.ooxml(false))),
        None => a.push(("w:tblpY", &y)),
    }
    w.empty("w:tblpPr", &a);
    if !f.overlap {
        w.val("w:tblOverlap", "never");
    }
}
