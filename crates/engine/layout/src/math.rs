//! Equation layout for Doc: kition-office-core's shared layout
//! ([`kition_office_core::math::layout`]) set in Doc's fonts.

use kition_doc_doc::props::{Rgb, TextColor};
use kition_doc_doc::resolve::ResolvedChar;
use kition_doc_fonts::math::glyph_bounds;
use kition_doc_fonts::{FaceRef, FontDb};
use kition_office_core::math::layout::{self, Base, MathFonts, Shaped, TextFace};
use kition_office_core::math::table::MathTable;
use kition_office_core::math::{Math, MathProps};
use std::sync::Arc;

/// A laid-out equation, in Doc's faces.
pub type MathLayout = layout::MathLayout<FaceRef>;
/// Something a laid-out equation draws, in Doc's faces.
pub type MItem = layout::MItem<FaceRef>;

/// Doc's fonts as equation layout needs them.
struct DocFonts;

impl MathFonts for DocFonts {
    type Face = FaceRef;
    fn math_face(&self, family: &str) -> (FaceRef, Option<Arc<MathTable>>) {
        kition_doc_fonts::math::resolve_math(family)
    }
    fn text_face(&self, family: &str, bold: bool, italic: bool) -> TextFace<FaceRef> {
        let r = kition_doc_fonts::word::resolve(family, bold, italic);
        TextFace { face: r.face, synth_bold: r.synth_bold, synth_italic: r.synth_italic }
    }
    fn fallback(&self, c: char, face: &FaceRef) -> Option<FaceRef> {
        FontDb::global().fallback_for(c, face.id()).map(|f| FaceRef::of(&f))
    }
    fn same(&self, a: &FaceRef, b: &FaceRef) -> bool {
        a.id() == b.id()
    }
    fn covers(&self, face: &FaceRef, c: char) -> bool {
        face.covers(c)
    }
    fn glyph(&self, face: &FaceRef, c: char) -> u32 {
        face.glyph_for(c)
    }
    fn upem(&self, face: &FaceRef) -> f32 {
        face.upem as f32
    }
    fn advance(&self, face: &FaceRef, gid: u32) -> f32 {
        face.advance(gid) as f32
    }
    fn bounds(&self, face: &FaceRef, gid: u32) -> Option<(f32, f32, f32, f32)> {
        glyph_bounds(face, gid)
    }
    fn weight(&self, face: &FaceRef) -> f32 {
        face.get().weight
    }
    fn shape(&self, face: &FaceRef, text: &str, script_level: u8) -> Vec<Shaped> {
        let feats = if script_level > 0 { vec![kition_doc_fonts::feature_value(b"ssty", script_level as u32)] } else { Vec::new() };
        kition_doc_fonts::shape(face, text, &feats, |c| c)
            .into_iter()
            .map(|g| Shaped {
                gid: g.gid,
                cluster: g.cluster,
                x_advance: g.x_advance as f32,
                x_offset: g.x_offset as f32,
                y_offset: g.y_offset as f32,
            })
            .collect()
    }
}

/// Lay out an equation set at the size and colour of `rc` (see [`layout::layout_equation`]).
pub fn layout_equation(math: &Math, linear: &str, rc: &ResolvedChar, display: bool, avail: f32, props: &MathProps, counter: &mut u32) -> MathLayout {
    let color = match rc.color {
        TextColor::Rgb(c) => c,
        TextColor::Auto => Rgb::BLACK,
    };
    let text = Base { size: rc.draw_size(), color: kition_office_core::model::Rgb(color.0, color.1, color.2), font: rc.font.clone() };
    layout::layout_equation(&DocFonts, math, linear, &text, display, avail, props, counter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_office_core::math::{MNode, MRun, MScr, math_alnum};

    fn rc() -> ResolvedChar {
        kition_doc_doc::styles::StyleSheet::default().resolve_char(None, &Default::default())
    }

    fn lay(lin: &str, display: bool) -> MathLayout {
        layout_equation(&Math::default(), lin, &rc(), display, 400.0, &MathProps::default(), &mut 0)
    }

    /// The glyphs of a laid-out equation by line: (baseline, [(x, text)]), top line first.
    fn lines(m: &MathLayout) -> Vec<(f32, Vec<(f32, String)>)> {
        let mut out: Vec<(f32, Vec<(f32, String)>)> = Vec::new();
        for it in &m.items {
            if let MItem::Glyphs { glyphs, text, .. } = it
                && let Some(&(_, x, y)) = glyphs.first()
            {
                match out.iter_mut().find(|l| (l.0 - y).abs() < 0.5) {
                    Some(l) => l.1.push((x, text.clone())),
                    None => out.push((y, vec![(x, text.clone())])),
                }
            }
        }
        out.sort_by(|a, b| b.0.total_cmp(&a.0));
        for l in &mut out {
            l.1.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        out
    }

    fn lay_in(math: &Math, lin: &str, display: bool, avail: f32, mp: &MathProps) -> MathLayout {
        layout_equation(math, lin, &rc(), display, avail, mp, &mut 0)
    }

    #[test]
    fn long_display_equations_wrap_before_operators_and_indent() {
        let sum = "y=a+b+c+d+e+f+g+h+i+j+k+l+m+n+o+p+q+r+s+t+u+v+w";
        let mp = MathProps::default();
        let full = lay_in(&Math::default(), sum, true, 1e4, &mp);
        assert_eq!(lines(&full).len(), 1, "fits on one line when there is room");
        let avail = full.width * 0.45;
        assert!(avail > 2.0 * mp.wrap_indent, "room for the indent: {avail}");
        let m = lay_in(&Math::default(), sum, true, avail, &mp);
        let ls = lines(&m);
        assert!(ls.len() >= 2, "{ls:?}");
        assert!((m.width - avail).abs() < 0.01, "the wrapped equation fills the line: {} vs {avail}", m.width);
        for (k, (_, gs)) in ls.iter().enumerate() {
            let (x0, first) = gs.first().cloned().unwrap();
            if k == 0 {
                // Math italic 𝑦 when a font has it; plain y where none does (FreeBSD CI has no fonts).
                assert!(x0.abs() < 0.01 && (first == "𝑦" || first == "y"), "{gs:?}");
            } else {
                // Continuation lines start with the operator, one wrap indent in.
                assert!((x0 - mp.wrap_indent).abs() < 0.01 && first == "+", "line {k}: {gs:?}");
            }
            for (x, t) in gs {
                assert!(*x < avail, "line {k}: {t} at {x} past {avail}");
            }
        }
        // Breaks after the operator, and the operator repeated, as the settings say.
        let after = MathProps { brk_bin: kition_office_core::math::BrkBin::After, ..Default::default() };
        let ls = lines(&lay_in(&Math::default(), sum, true, avail, &after));
        assert!(ls.len() >= 2 && ls[0].1.last().is_some_and(|g| g.1 == "+") && ls[1].1[0].1 != "+", "{ls:?}");
        let repeat = MathProps { brk_bin: kition_office_core::math::BrkBin::Repeat, wrap_right: true, ..Default::default() };
        let m = lay_in(&Math::default(), sum, true, avail, &repeat);
        let ls = lines(&m);
        assert!(ls.len() >= 2 && ls[0].1.last().is_some_and(|g| g.1 == "+") && ls[1].1[0].1 == "+", "{ls:?}");
        // Inline equations never wrap.
        assert_eq!(lines(&lay_in(&Math::default(), sum, false, avail, &mp)).len(), 1);
        // A term wider than the line overflows it rather than breaking inside.
        let frac = lay_in(&Math::default(), "y=(a+b+c+d+e+f+g+h+i+j+k+l+m+n+o+p+q+r+s+t+u+v+w)/2", true, avail, &mp);
        assert!(frac.width > avail && frac.items.iter().any(|i| matches!(i, MItem::Rect { .. })), "{}", frac.width);
    }

    #[test]
    fn manual_breaks_start_new_lines() {
        let mp = MathProps::default();
        let nodes = vec![MNode::Run(MRun::new("a=b")), MNode::Run(MRun { brk: Some(0), ..MRun::new("+c") })];
        let m = lay_in(&Math { nodes, ..Default::default() }, "", true, 400.0, &mp);
        let ls = lines(&m);
        assert_eq!(ls.len(), 2, "{ls:?}");
        assert!((ls[1].1[0].0 - mp.wrap_indent).abs() < 0.01 && ls[1].1[0].1 == "+", "{ls:?}");
        assert!(m.width < 400.0, "a manually broken equation is as wide as its lines");
        // `m:alnAt`: the new line's operator sits under the first line's first operator.
        let nodes = vec![MNode::Run(MRun::new("a=b")), MNode::Run(MRun { brk: Some(1), ..MRun::new("+c") })];
        let ls = lines(&lay_in(&Math { nodes, ..Default::default() }, "", true, 400.0, &mp));
        let eq_x = ls[0].1.iter().find(|g| g.1 == "=").unwrap().0;
        assert!((ls[1].1[0].0 - eq_x).abs() < 0.01, "{ls:?}");
    }

    #[test]
    fn short_display_equations_stay_on_one_line() {
        let mp = MathProps::default();
        let one = lay_in(&Math::default(), "a+b=c", true, 400.0, &mp);
        assert_eq!(lines(&one).len(), 1);
        let plain = lay_in(&Math::default(), "a+b=c", false, 400.0, &mp);
        assert!(one.width < 400.0 && (one.width - plain.width).abs() < 0.01, "{} vs {}", one.width, plain.width);
    }

    #[test]
    fn fraction_is_stacked_with_a_rule() {
        let flat = lay("ab", true);
        let f = lay("a/b", true);
        assert!(f.ascent > flat.ascent + 2.0 && f.descent > flat.descent + 2.0, "{f:?}");
        assert!(f.items.iter().any(|i| matches!(i, MItem::Rect { .. })), "fraction rule");
        assert!(f.width < flat.width * 1.2, "stacked, not side by side");
    }

    #[test]
    fn superscript_is_raised_and_smaller() {
        let m = lay("x^2", false);
        let sizes: Vec<(f32, f32)> = m
            .items
            .iter()
            .filter_map(|i| match i {
                MItem::Glyphs { size, glyphs, .. } => glyphs.first().map(|g| (*size, g.2)),
                _ => None,
            })
            .collect();
        assert_eq!(sizes.len(), 2, "{m:?}");
        assert!(sizes[1].0 < sizes[0].0 && sizes[1].1 > 2.0, "{sizes:?}");
    }

    #[test]
    fn operators_get_spaces_and_unary_minus_does_not() {
        let tight = lay("ab", false);
        let spaced = lay("a+b", false);
        let unary = lay("−b", false);
        let plus_w = lay("+", false).width;
        assert!(spaced.width > tight.width + plus_w + 2.0, "medium spaces around +");
        assert!(unary.width < lay("b", false).width + plus_w + 1.0, "no space after a unary minus");
    }

    #[test]
    fn display_operators_are_big_with_limits_over_and_under() {
        let d = lay("∑_(i=1)^n▒i", true);
        let t = lay("∑_(i=1)^n▒i", false);
        assert!(d.ascent + d.descent > t.ascent + t.descent, "display sum taller");
        assert!(d.width < t.width + 4.0, "limits stacked in display");
    }

    #[test]
    fn delimiters_grow_around_tall_content() {
        let small = lay("(x)", true);
        let big = lay("((a)/(b))", true);
        let h = |m: &MathLayout| m.ascent + m.descent;
        assert!(h(&big) > h(&small) * 1.4, "{} vs {}", h(&big), h(&small));
    }

    #[test]
    fn every_structure_lays_out() {
        use kition_office_core::math::*;
        let x = || vec![MNode::Run(MRun::new("x"))];
        let nodes = vec![
            MNode::Acc { chr: '\u{302}', e: x() },
            MNode::Acc { chr: '\u{305}', e: x() },
            MNode::Bar { top: false, e: x() },
            MNode::BorderBox { hide: [false; 4], strike: [true; 4], e: x() },
            MNode::Boxed { e: x() },
            MNode::GroupChr { chr: '⏟', top: false, e: x() },
            MNode::EqArr { rows: vec![vec![MNode::Run(MRun::new("x&=1"))], vec![MNode::Run(MRun::new("y&=22"))]] },
            MNode::Matrix { rows: vec![vec![x(), x()], vec![x(), Vec::new()]], col_jc: vec![] },
            MNode::Phant { show: false, zero_wid: false, zero_asc: false, zero_desc: true, e: x() },
            MNode::Lim { upper: false, e: vec![MNode::Run(MRun::plain("lim"))], lim: x() },
            MNode::Rad { deg: x(), deg_hide: false, e: x() },
            MNode::Script { kind: ScriptKind::Pre, base: x(), sub: x(), sup: x() },
            MNode::Frac { kind: FracKind::Skewed, num: x(), den: x() },
            MNode::Frac { kind: FracKind::Linear, num: x(), den: x() },
            MNode::Frac { kind: FracKind::NoBar, num: x(), den: x() },
            MNode::Delim { beg: None, end: Some('|'), sep: Some('|'), grow: true, shp_match: true, elems: vec![x(), x()] },
            MNode::Run(MRun { text: "text".into(), nor: true, ..Default::default() }),
            MNode::Run(MRun { text: "R".into(), scr: MScr::DoubleStruck, ..Default::default() }),
        ];
        for n in nodes {
            let m = layout_equation(&Math { nodes: vec![n.clone()], ..Default::default() }, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
            assert!(m.width > 0.0 && m.ascent + m.descent > 0.0, "{n:?} → {m:?}");
            for it in &m.items {
                if let MItem::Glyphs { glyphs, .. } = it {
                    assert!(glyphs.iter().all(|g| g.1.is_finite() && g.2.is_finite()));
                }
            }
        }
    }

    #[test]
    fn normal_text_falls_back_for_letters_its_font_lacks() {
        // An empty equation's placeholder (`m:nor` text in Times New Roman, as Word writes it):
        // its Chinese letters are drawn in a face that has them, never as missing glyphs.
        let text = "Type \u{5728}\u{6b64}\u{5904}";
        let run = MRun { text: text.into(), nor: true, font: Some("Times New Roman".into()), ..Default::default() };
        let m = layout_equation(&Math { nodes: vec![MNode::Run(run)], ..Default::default() }, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
        let mut drawn = 0;
        for it in &m.items {
            if let MItem::Glyphs { face, glyphs, .. } = it {
                assert!(glyphs.iter().all(|g| g.0 != 0), "a missing glyph in {}", face.get().family);
                drawn += glyphs.len();
            }
        }
        assert!(drawn >= text.chars().filter(|c| !c.is_whitespace()).count(), "{drawn}");
    }

    #[test]
    fn caret_slots_cover_every_position() {
        use kition_office_core::math::edit::{MathPos, move_right, units};
        let math = Math { nodes: kition_office_core::math::parse_linear("x=(-b±√(b^2-4ac))/2a+■(a&b@c&d)"), ..Default::default() };
        let m = layout_equation(&math, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
        // Walking the caret through the equation, every position has a slot.
        let mut p = MathPos::default();
        let mut seen = 0;
        loop {
            let s = m.slots.iter().find(|s| s.path == p.path && s.off == p.off);
            assert!(s.is_some(), "no slot for {p:?}");
            seen += 1;
            match move_right(&math.nodes, &p) {
                Some(n) => p = n,
                None => break,
            }
            assert!(seen < 500);
        }
        assert_eq!(p.off, units(&math.nodes));
        // Hit testing finds the denominator below the bar.
        let den = m.slots.iter().find(|s| s.path == vec![(1, 1)]).unwrap();
        let hit = m.hit(den.x + 1.0, den.y).unwrap();
        assert_eq!(hit.path, vec![(1, 1)]);
    }

    #[test]
    fn empty_slots_show_placeholders_on_screen_only() {
        let math = Math { nodes: kition_office_core::math::parse_linear("⬚/⬚"), ..Default::default() };
        let m = layout_equation(&math, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
        assert!(m.items.iter().all(|i| matches!(i, MItem::ScreenOnly(_) | MItem::Rect { .. })), "{:?}", m.items);
        assert!(m.slots.iter().any(|s| s.path == vec![(0, 0)] && s.off == 0));
        let empty = layout_equation(&Math::default(), "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
        assert!(empty.width > 10.0 && empty.items.iter().all(|i| matches!(i, MItem::ScreenOnly(_))));
    }

    #[test]
    fn numbered_equations_fill_the_line_with_the_number_at_the_right() {
        let math = Math { nodes: kition_office_core::math::parse_linear("E=mc^2#(1)"), ..Default::default() };
        let m = layout_equation(&math, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
        assert!((m.width - 400.0).abs() < 0.01, "{}", m.width);
        let xs: Vec<f32> =
            m.items.iter().filter_map(|i| if let MItem::Glyphs { glyphs, .. } = i { glyphs.first().map(|g| g.1) } else { None }).collect();
        let max = xs.iter().cloned().fold(0.0f32, f32::max);
        assert!(max > 350.0, "number at the right margin: {xs:?}");
        let min = xs.iter().cloned().fold(f32::MAX, f32::min);
        assert!(min > 120.0 && min < 220.0, "equation centred: {xs:?}");
        // Automatic numbers count.
        let auto = Math { nodes: kition_office_core::math::parse_linear("a=b#"), ..Default::default() };
        let mut n = 4;
        let _ = layout_equation(&auto, "", &rc(), true, 400.0, &MathProps::default(), &mut n);
        assert_eq!(n, 5);
        assert_eq!(kition_office_core::math::layout::auto_numbers(&auto, true), 1);
    }

    #[test]
    fn math_alphabets() {
        assert_eq!(math_alnum('x', false, true, MScr::Roman), '𝑥');
        assert_eq!(math_alnum('h', false, true, MScr::Roman), 'ℎ');
        assert_eq!(math_alnum('R', false, false, MScr::DoubleStruck), 'ℝ');
        assert_eq!(math_alnum('α', false, true, MScr::Roman), '𝛼');
        assert_eq!(math_alnum('2', false, true, MScr::Roman), '2');
        assert_eq!(math_alnum('A', true, false, MScr::Roman), '𝐀');
    }

    #[test]
    fn hostile_trees_never_panic() {
        use kition_office_core::math::*;
        // Deep nesting.
        let mut n = vec![MNode::Run(MRun::new("x"))];
        for _ in 0..500 {
            n = vec![MNode::Frac { kind: FracKind::Bar, num: n.clone(), den: Vec::new() }];
        }
        let _ = layout_equation(&Math { nodes: n, ..Default::default() }, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
        // Empty everything.
        let empty = vec![
            MNode::Nary { chr: '\0', lim_loc: None, grow: true, sub_hide: false, sup_hide: false, sub: vec![], sup: vec![], e: vec![] },
            MNode::Delim { beg: Some('\u{10FFFF}'), end: None, sep: None, grow: true, shp_match: false, elems: vec![] },
            MNode::Matrix { rows: vec![vec![]], col_jc: vec![] },
            MNode::EqArr { rows: vec![] },
            MNode::Run(MRun { text: String::new(), size: Some(f32::NAN), ..Default::default() }),
        ];
        let m = layout_equation(&Math { nodes: empty, ..Default::default() }, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
        assert!(m.width.is_finite() && m.ascent.is_finite());
        // Breaks everywhere, odd wrap settings, no room at all.
        let runs: Vec<MNode> = (0..2000).map(|i| MNode::Run(MRun { brk: (i % 3 == 0).then_some((i % 256) as u8), ..MRun::new("−+=") })).collect();
        for avail in [0.0, f32::NAN, 1e9] {
            for brk_bin in [BrkBin::Before, kition_office_core::math::BrkBin::After, kition_office_core::math::BrkBin::Repeat] {
                let mp = MathProps { brk_bin, brk_bin_sub: BrkBinSub::PlusMinus, wrap_indent: f32::INFINITY, wrap_right: avail == 0.0 };
                let m = layout_equation(&Math { nodes: runs.clone(), ..Default::default() }, "", &rc(), true, avail, &mp, &mut 0);
                assert!(m.width.is_finite() && m.descent.is_finite());
            }
        }
    }
}
