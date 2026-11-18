//! `EQ` fields: the field code is a small layout language (ECMA-376 Part 1, the `EQ` field;
//! Word's behaviour in MS-OI29500). Text and switches that overstrike (`\o`), raise and lower
//! (`\s`), list (`\l`), stack a fraction (`\f`), bracket (`\b`), box (`\x`), displace (`\d`),
//! arrange an array (`\a`), and draw radicals (`\r`) and integrals (`\i`). Arguments are
//! separated by commas or semicolons; a backslash makes the next character literal (`\,`, `\(`,
//! `\\`). An uncached `EQ` field is drawn from its code, each character in its own code run's
//! formatting, so this module takes the shaping from its caller ([`EqText`]).
//!
//! Coordinates: x right from the field's left edge, y *up* from the baseline (as [`MItem`]).

use kition_doc_doc::props::Rgb;

use crate::math::{MItem, MathLayout};

/// Deepest switch nesting read; deeper code is read as text.
const MAX_DEPTH: usize = 32;
/// Most nodes read from one code (hostile files).
const MAX_NODES: usize = 4096;

/// A piece of an `EQ` field's code.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Node {
    /// Characters drawn as they are, with their byte offsets in the code.
    Text(Vec<(usize, char)>),
    /// A switch (`\o`, `\s`…), its options and its arguments.
    Switch(Switch),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Switch {
    /// The switch letter, lower case.
    pub kind: char,
    /// Byte offset of the switch in the code: generated characters (brackets, list separators)
    /// take the formatting there.
    pub at: usize,
    pub opts: Vec<Opt>,
    pub args: Vec<Vec<Node>>,
}

/// A switch option: `\up 6`, `\al`, `\lc\[`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Opt {
    pub name: String,
    pub value: Option<f32>,
    pub ch: Option<(usize, char)>,
}

impl Switch {
    fn opt(&self, name: &str) -> Option<&Opt> {
        self.opts.iter().rev().find(|o| o.name == name)
    }
    fn has(&self, name: &str) -> bool {
        self.opt(name).is_some()
    }
    /// An option's number, `default` when the option is given without one; `None` when absent.
    fn num(&self, name: &str, default: f32) -> Option<f32> {
        self.opt(name).map(|o| o.value.unwrap_or(default))
    }
}

/// Whether a field code is an `EQ` field.
pub(crate) fn is_eq(instr: &str) -> bool {
    let t = instr.trim_start();
    t.get(..2).is_some_and(|k| k.eq_ignore_ascii_case("EQ"))
        && t.get(2..).and_then(|r| r.chars().next()).is_none_or(|c| c.is_whitespace() || c == '\\')
}

/// A read `EQ` field code.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Eq {
    pub nodes: Vec<Node>,
    /// Formatting of a Phonetic Guide's guide text (the text `\s` raises or lowers).
    pub guide: Guide,
}

/// Word's Phonetic Guide stores ruby text as an `EQ` field whose general switches format the
/// guide: `\* hpsN` its size in half points (as `w:rubyPr/w:hps`), `\* "Font:name"` its font.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Guide {
    pub size: Option<f32>,
    pub font: Option<String>,
}

/// The nodes of an `EQ` field's code (`None` when it isn't one). Offsets are bytes of `instr`.
pub(crate) fn parse(instr: &str) -> Option<Eq> {
    if !is_eq(instr) {
        return None;
    }
    let lead = instr.len() - instr.trim_start().len();
    // Spaces closing the code delimit it; they are not text.
    let mut p = Parser { s: instr.trim_end(), i: lead + 2, nodes: 0, formats: Vec::new() };
    p.skip_ws();
    let nodes = p.seq(false, 0).0;
    let mut guide = Guide::default();
    for f in &p.formats {
        if let Some(n) = f.get(..3).filter(|k| k.eq_ignore_ascii_case("hps")).and_then(|_| f.get(3..)?.parse::<f32>().ok()) {
            guide.size = Some((n / 2.0).clamp(0.5, 1638.0)).filter(|v| v.is_finite());
        } else if let Some(name) = f.get(..5).filter(|k| k.eq_ignore_ascii_case("font:")).and_then(|_| f.get(5..)) {
            guide.font = Some(name.trim().to_string()).filter(|n| !n.is_empty());
        }
    }
    Some(Eq { nodes, guide })
}

#[derive(PartialEq)]
enum End {
    Eof,
    Sep,
    Close,
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
    nodes: usize,
    /// The general switches' (`\*`) arguments, quotes removed.
    formats: Vec<String>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i..)?.chars().next()
    }
    fn peek2(&self) -> Option<char> {
        let mut it = self.s.get(self.i..)?.chars();
        it.next();
        it.next()
    }
    fn bump(&mut self) -> Option<(usize, char)> {
        let c = self.peek()?;
        let at = self.i;
        self.i += c.len_utf8();
        Some((at, c))
    }
    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }
    fn letters(&mut self) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek().filter(char::is_ascii_alphabetic) {
            out.push(c.to_ascii_lowercase());
            self.bump();
        }
        out
    }
    fn number(&mut self) -> Option<f32> {
        let start = self.i;
        if self.peek() == Some('-') {
            self.bump();
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            self.bump();
        }
        let v = self.s.get(start..self.i)?.parse::<f32>().ok().filter(|v| v.is_finite());
        if v.is_none() {
            self.i = start;
        }
        v
    }

    /// Nodes up to the end of the code or, inside an argument, its separator or closing parenthesis.
    fn seq(&mut self, in_arg: bool, depth: usize) -> (Vec<Node>, End) {
        let mut nodes = Vec::new();
        let mut text: Vec<(usize, char)> = Vec::new();
        // Parentheses that are text (an unescaped pair inside an argument).
        let mut open = 0usize;
        let end = loop {
            let Some(c) = self.peek() else { break End::Eof };
            if in_arg && open == 0 && (c == ',' || c == ';') {
                self.bump();
                break End::Sep;
            }
            if in_arg && c == ')' {
                if open == 0 {
                    self.bump();
                    break End::Close;
                }
                open -= 1;
            }
            if c == '(' {
                open += 1;
            }
            if c != '\\' {
                text.extend(self.bump());
                continue;
            }
            if !in_arg && self.peek2() == Some('*') {
                // A general switch formats the field and draws nothing, nor do the spaces around it.
                while text.last().is_some_and(|c| c.1.is_whitespace()) {
                    text.pop();
                }
                self.bump();
                self.bump();
                self.general_switch();
                continue;
            }
            match self.peek2() {
                Some(n) if n.is_ascii_alphabetic() => {
                    let at = self.i;
                    self.bump();
                    let name = self.letters();
                    let kind = name.chars().next().unwrap_or('?');
                    if name.len() == 1 && "abdfilorsx".contains(kind) && depth < MAX_DEPTH && self.nodes < MAX_NODES {
                        self.nodes += 1;
                        if !text.is_empty() {
                            nodes.push(Node::Text(std::mem::take(&mut text)));
                        }
                        nodes.push(Node::Switch(self.switch(kind, at, depth + 1)));
                    }
                    // Unknown switches draw nothing.
                }
                Some(_) => {
                    self.bump();
                    text.extend(self.bump());
                }
                None => text.extend(self.bump()),
            }
        };
        if !text.is_empty() {
            nodes.push(Node::Text(text));
        }
        (nodes, end)
    }

    /// The argument of a general switch (`\* hps18`, `\* "Font:name"`), and the spaces after it.
    fn general_switch(&mut self) {
        self.skip_ws();
        let mut arg = String::new();
        if self.peek() == Some('"') {
            self.bump();
            while let Some((_, c)) = self.bump() {
                if c == '"' {
                    break;
                }
                arg.push(c);
            }
        } else {
            while let Some(c) = self.peek().filter(|c| !c.is_whitespace() && *c != '\\') {
                arg.push(c);
                self.bump();
            }
        }
        if self.formats.len() < 64 {
            self.formats.push(arg);
        }
        self.skip_ws();
    }

    fn switch(&mut self, kind: char, at: usize, depth: usize) -> Switch {
        let mut sw = Switch { kind, at, opts: Vec::new(), args: Vec::new() };
        loop {
            self.skip_ws();
            match self.peek() {
                Some('\\') if self.peek2().is_some_and(|c| c.is_ascii_alphabetic()) => {
                    self.bump();
                    let name = self.letters();
                    let mut opt = Opt { name, value: None, ch: None };
                    if matches!(opt.name.as_str(), "lc" | "rc" | "bc" | "fc" | "vc") {
                        if self.peek() == Some('\\') {
                            self.bump();
                        }
                        opt.ch = self.bump();
                    } else {
                        self.skip_ws();
                        opt.value = self.number();
                    }
                    sw.opts.push(opt);
                }
                Some('(') => {
                    self.bump();
                    loop {
                        let (arg, end) = self.seq(true, depth);
                        sw.args.push(arg);
                        if end != End::Sep || sw.args.len() > MAX_NODES {
                            break;
                        }
                    }
                    return sw;
                }
                _ => return sw,
            }
        }
    }
}

/// A laid-out piece: width, extent above and below the baseline, and what to draw.
#[derive(Clone, Debug, Default)]
pub(crate) struct EqBox {
    pub w: f32,
    pub a: f32,
    pub d: f32,
    pub items: Vec<MItem>,
}

impl EqBox {
    /// Draws `o` at (`dx`, `dy`) into this box; extents grow, the width is the caller's.
    fn put(&mut self, o: EqBox, dx: f32, dy: f32) {
        self.a = self.a.max(o.a + dy);
        self.d = self.d.max(o.d - dy);
        for mut it in o.items {
            it.shift(dx, dy);
            self.items.push(it);
        }
    }
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: Rgb) {
        self.items.push(MItem::Line { x0, y0, x1, y1, width, color: color.into(), dotted: false });
        self.a = self.a.max(y0.max(y1) + width / 2.0);
        self.d = self.d.max(-(y0.min(y1) - width / 2.0));
    }
}

impl EqBox {
    /// Appends `o` to the right.
    pub(crate) fn join(&mut self, o: EqBox) {
        let x = self.w;
        self.w += o.w;
        self.put(o, x, 0.0);
    }
}

fn hcat(parts: Vec<EqBox>) -> EqBox {
    let mut out = EqBox::default();
    for b in parts {
        out.join(b);
    }
    out
}

/// Shaping for an `EQ` field's text.
pub(crate) trait EqText {
    /// Code characters (consecutive in the code), each in its code run's formatting.
    fn text(&mut self, chars: &[(usize, char)]) -> EqBox;
    /// One character the switch draws itself (a bracket, a list separator) in the formatting at
    /// `at`, at `size` points when given.
    fn glyph(&mut self, ch: char, at: usize, size: Option<f32>) -> EqBox;
    /// Font size and colour at `at`, for rules and spacing.
    fn metrics(&mut self, at: usize) -> (f32, Rgb);
    /// Whether text is laid out as a Phonetic Guide's guide text from now; returns the previous state.
    fn guide(&mut self, on: bool) -> bool;
}

/// Lays out an `EQ` field's nodes.
pub(crate) fn layout(eq: &Eq, t: &mut dyn EqText) -> MathLayout {
    let b = seq(&eq.nodes, t);
    MathLayout { width: b.w.max(0.0), ascent: b.a.max(0.0), descent: b.d.max(0.0), items: b.items, slots: Vec::new() }
}

fn seq(nodes: &[Node], t: &mut dyn EqText) -> EqBox {
    hcat(nodes.iter().map(|n| node(n, t)).collect())
}

/// Arguments joined by commas: a list (`\l`), or the extra arguments of a one-argument switch.
fn joined(sw: &Switch, args: &[Vec<Node>], t: &mut dyn EqText) -> EqBox {
    let mut parts = Vec::new();
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            parts.push(t.glyph(',', sw.at, None));
        }
        parts.push(seq(a, t));
    }
    hcat(parts)
}

fn node(n: &Node, t: &mut dyn EqText) -> EqBox {
    let sw = match n {
        Node::Text(chars) => return t.text(chars),
        Node::Switch(sw) => sw,
    };
    let (size, color) = t.metrics(sw.at);
    let rule = (size / 20.0).max(0.5);
    // The math axis (the middle of a fraction rule, the centre of an array).
    let axis = size * 0.25;
    match sw.kind {
        'o' => {
            let parts: Vec<EqBox> = sw.args.iter().map(|a| seq(a, t)).collect();
            let w = parts.iter().map(|p| p.w).fold(0.0, f32::max);
            let mut out = EqBox { w, ..Default::default() };
            for p in parts {
                let x = if sw.has("al") {
                    0.0
                } else if sw.has("ar") {
                    w - p.w
                } else {
                    (w - p.w) / 2.0
                };
                out.put(p, x, 0.0);
            }
            out
        }
        's' => {
            let up = sw.num("up", 2.0).map(|v| v.clamp(-1584.0, 1584.0));
            let down = sw.num("do", 2.0).map(|v| v.clamp(-1584.0, 1584.0));
            let shift = up.unwrap_or(0.0) - down.unwrap_or(0.0);
            // Several elements stack downwards from the first.
            let mut out = EqBox::default();
            let mut dy = shift;
            let mut above: Option<f32> = None;
            let was = t.guide(true);
            for a in &sw.args {
                let p = seq(a, t);
                if let Some(d) = above {
                    dy -= d + p.a;
                }
                above = Some(p.d);
                out.w = out.w.max(p.w);
                out.put(p, 0.0, dy);
            }
            t.guide(was);
            // Extra space above and below the line.
            out.a += sw.num("ai", 0.0).map_or(0.0, |v| v.clamp(0.0, 127.0));
            out.d += sw.num("di", 0.0).map_or(0.0, |v| v.clamp(0.0, 127.0));
            out
        }
        'l' => joined(sw, &sw.args, t),
        'f' => {
            let num = sw.args.first().map(|a| seq(a, t)).unwrap_or_default();
            let den = sw.args.get(1).map(|a| seq(a, t)).unwrap_or_default();
            let gap = size * 0.1;
            let w = num.w.max(den.w) + 2.0 * gap;
            let mut out = EqBox { w, ..Default::default() };
            let (nw, dw) = (num.w, den.w);
            let num_y = axis + rule / 2.0 + gap + num.d;
            let den_y = axis - rule / 2.0 - gap - den.a;
            out.put(num, (w - nw) / 2.0, num_y);
            out.put(den, (w - dw) / 2.0, den_y);
            out.items.push(MItem::Rect { x: 0.0, y: axis - rule / 2.0, w, h: rule, color: color.into() });
            out
        }
        'b' => {
            let inner = joined(sw, &sw.args, t);
            let both = sw.opt("bc").and_then(|o| o.ch);
            let left = sw.opt("lc").and_then(|o| o.ch).or(both).map_or('(', |c| c.1);
            let right = sw.opt("rc").and_then(|o| o.ch).map(|c| c.1).or(both.map(|c| mirror(c.1))).unwrap_or(')');
            let tall = inner.a + inner.d;
            let bracket = |t: &mut dyn EqText, c: char| {
                let natural = t.glyph(c, sw.at, None);
                let h = natural.a + natural.d;
                let grow = if h > 0.0 && tall > h { Some(size * tall / h) } else { None };
                let g = if grow.is_some() { t.glyph(c, sw.at, grow) } else { natural };
                // Centred on the content.
                let dy = (inner.a - inner.d) / 2.0 - (g.a - g.d) / 2.0;
                (g, if grow.is_some() { dy } else { 0.0 })
            };
            let (l, ly) = bracket(t, left);
            let (r, ry) = bracket(t, right);
            let mut out = EqBox::default();
            let (lw, iw, rw) = (l.w, inner.w, r.w);
            out.put(l, 0.0, ly);
            out.put(inner, lw, 0.0);
            out.put(r, lw + iw, ry);
            out.w = lw + iw + rw;
            out
        }
        'x' => {
            let inner = joined(sw, &sw.args, t);
            let any = ["to", "bo", "le", "ri"].iter().any(|s| sw.has(s));
            let side = |s: &str| !any || sw.has(s);
            let pad = rule;
            let (top, bottom) = (inner.a + pad, -inner.d - pad);
            let iw = inner.w;
            let mut out = EqBox::default();
            let x0 = if side("le") { pad } else { 0.0 };
            out.put(inner, x0, 0.0);
            out.w = x0 + iw + if side("ri") { pad } else { 0.0 };
            let w = out.w;
            if side("to") {
                out.line(0.0, top, w, top, rule, color);
            }
            if side("bo") {
                out.line(0.0, bottom, w, bottom, rule, color);
            }
            if side("le") {
                out.line(rule / 2.0, bottom, rule / 2.0, top, rule, color);
            }
            if side("ri") {
                out.line(w - rule / 2.0, bottom, w - rule / 2.0, top, rule, color);
            }
            out
        }
        'd' => {
            let mut out = joined(sw, &sw.args, t);
            let fo = sw.num("fo", 0.0).unwrap_or(0.0);
            let ba = sw.num("ba", 0.0).unwrap_or(0.0);
            // A line cannot run backwards here: a net move to the left draws nothing.
            let w = (out.w + fo - ba).clamp(0.0, 1584.0);
            if sw.has("li") && w > 0.0 {
                out.line(0.0, -rule, w, -rule, rule, color);
            }
            out.w = w;
            out
        }
        'a' => array(sw, t, axis),
        'r' => {
            let (deg, body) = match sw.args.as_slice() {
                [] => (None, EqBox::default()),
                [b] => (None, seq(b, t)),
                [d, b, ..] => (Some(seq(d, t)), seq(b, t)),
            };
            let gap = size * 0.1;
            let top = body.a + gap + rule;
            let bottom = -body.d;
            let hook = size * 0.5;
            let mut out = EqBox::default();
            let mut x = 0.0;
            if let Some(d) = deg {
                // The degree sits in the crook of the sign.
                let (lift, dw) = ((top + bottom) / 2.0 + d.d, d.w);
                out.put(d, 0.0, lift);
                x = (dw - hook * 0.5).max(0.0);
            }
            let mid = bottom + (top - bottom) * 0.4;
            out.line(x, mid, x + hook * 0.3, mid + rule, rule, color);
            out.line(x + hook * 0.3, mid + rule, x + hook * 0.55, bottom, rule, color);
            out.line(x + hook * 0.55, bottom, x + hook, top, rule, color);
            let bw = body.w;
            out.put(body, x + hook + gap, 0.0);
            out.w = x + hook + gap + bw + gap;
            out.line(x + hook, top, out.w, top, rule, color);
            out
        }
        'i' => integral(sw, t, size),
        _ => joined(sw, &sw.args, t),
    }
}

/// The closing partner of an opening bracket (the same character otherwise).
fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        '\u{3008}' => '\u{3009}',
        '\u{300A}' => '\u{300B}',
        '\u{3010}' => '\u{3011}',
        '\u{FF08}' => '\u{FF09}',
        c => c,
    }
}

/// `\a`: elements in rows of `\co` columns, aligned within each column, centred on the axis.
fn array(sw: &Switch, t: &mut dyn EqText, axis: f32) -> EqBox {
    let cols = sw.num("co", 1.0).map_or(1, |v| v.clamp(1.0, 64.0) as usize);
    let vs = sw.num("vs", 0.0).map_or(0.0, |v| v.clamp(0.0, 127.0));
    let hs = sw.num("hs", 0.0).map_or(0.0, |v| v.clamp(0.0, 127.0));
    let cells: Vec<EqBox> = sw.args.iter().map(|a| seq(a, t)).collect();
    let mut widths = vec![0.0f32; cols];
    for (i, c) in cells.iter().enumerate() {
        if let Some(w) = widths.get_mut(i % cols) {
            *w = w.max(c.w);
        }
    }
    let rows: Vec<&[EqBox]> = cells.chunks(cols).collect();
    let metrics: Vec<(f32, f32)> =
        rows.iter().map(|r| (r.iter().map(|c| c.a).fold(0.0, f32::max), r.iter().map(|c| c.d).fold(0.0, f32::max))).collect();
    let total: f32 = metrics.iter().map(|(a, d)| a + d).sum::<f32>() + vs * metrics.len().saturating_sub(1) as f32;
    let mut out = EqBox { w: widths.iter().sum::<f32>() + hs * cols.saturating_sub(1) as f32, ..Default::default() };
    let mut top = axis + total / 2.0;
    for (row, (a, d)) in rows.into_iter().zip(metrics) {
        let base = top - a;
        let mut x = 0.0;
        for (k, c) in row.iter().enumerate() {
            let cw = widths.get(k).copied().unwrap_or(0.0);
            let dx = if sw.has("al") {
                0.0
            } else if sw.has("ar") {
                cw - c.w
            } else {
                (cw - c.w) / 2.0
            };
            out.put(c.clone(), x + dx, base);
            x += cw + hs;
        }
        top = base - d - vs;
    }
    out
}

/// `\i`: an integral (or sum, product) sign with lower and upper limits and the integrand.
fn integral(sw: &Switch, t: &mut dyn EqText, size: f32) -> EqBox {
    let lower = sw.args.first().map(|a| seq(a, t)).unwrap_or_default();
    let upper = sw.args.get(1).map(|a| seq(a, t)).unwrap_or_default();
    let body = sw.args.get(2).map(|a| seq(a, t)).unwrap_or_default();
    let symbol = if let Some((_, c)) = sw.opt("fc").and_then(|o| o.ch).or(sw.opt("vc").and_then(|o| o.ch)) {
        c
    } else if sw.has("su") {
        '\u{2211}'
    } else if sw.has("pr") {
        '\u{220F}'
    } else {
        '\u{222B}'
    };
    let sym_size = if sw.has("fc") { size } else { (size * 1.5).max(body.a + body.d) };
    let sign = t.glyph(symbol, sw.at, Some(sym_size));
    let mut out = EqBox::default();
    let (sa, sd, sw_w) = (sign.a, sign.d, sign.w);
    out.put(sign, 0.0, 0.0);
    let (lw, uw) = (lower.w, upper.w);
    let x = if sw.has("in") {
        // Limits to the right of the sign.
        let (la, ud) = (lower.a, upper.d);
        out.put(lower, sw_w, -sd + la * 0.5);
        out.put(upper, sw_w, sa - ud);
        sw_w + lw.max(uw)
    } else {
        let w = sw_w.max(lw).max(uw);
        let (la, ud) = (lower.a, upper.d);
        out.put(lower, (w - lw) / 2.0, -sd - la);
        out.put(upper, (w - uw) / 2.0, sa + ud);
        w
    };
    let bw = body.w;
    out.put(body, x, 0.0);
    out.w = x + bw;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| match n {
                Node::Text(c) => c.iter().map(|x| x.1).collect(),
                Node::Switch(s) => format!("\\{}{:?}", s.kind, s.args.iter().map(|a| text_of(a).concat()).collect::<Vec<_>>()),
            })
            .collect()
    }

    #[test]
    fn parses_switches_options_arguments_and_escapes() {
        let code = r"EQ \o\ar(\s\up 6(AB),\s\do2(C\,D))x\(\\";
        let nodes = parse(code).unwrap().nodes;
        assert_eq!(nodes.len(), 2);
        assert_eq!(text_of(&nodes)[1], r"x(\");
        let Node::Switch(o) = &nodes[0] else { panic!() };
        assert!(o.has("ar"));
        let Node::Switch(up) = &o.args[0][0] else { panic!() };
        assert_eq!(up.num("up", 2.0), Some(6.0));
        let Node::Switch(down) = &o.args[1][0] else { panic!() };
        assert_eq!(down.num("do", 2.0), Some(2.0));
        assert_eq!(text_of(&down.args[0]), ["C,D"]);
        // Offsets point into the code, where the characters' formatting is.
        let Node::Text(ab) = &up.args[0][0] else { panic!() };
        assert_eq!(&code[ab[0].0..ab[0].0 + 1], "A");
    }

    #[test]
    fn bracket_characters_and_semicolons() {
        let nodes = parse(r"eq \b\lc\[\rc\}(a;b)").unwrap().nodes;
        let Node::Switch(b) = &nodes[0] else { panic!() };
        assert_eq!(b.opt("lc").and_then(|o| o.ch).map(|c| c.1), Some('['));
        assert_eq!(b.opt("rc").and_then(|o| o.ch).map(|c| c.1), Some('}'));
        assert_eq!(b.args.len(), 2);
    }

    #[test]
    fn general_switches_format_the_guide_and_draw_nothing() {
        let eq = parse(r#"EQ \* jc0 \* "Font:Some Face" \* hps18 \o\ad(\s\up 17(ab),c) "#).unwrap();
        assert_eq!(eq.guide, Guide { size: Some(9.0), font: Some("Some Face".into()) });
        assert_eq!(eq.nodes.len(), 1, "{:?}", eq.nodes);
        assert!(matches!(&eq.nodes[0], Node::Switch(s) if s.kind == 'o' && s.has("ad")));
    }

    #[test]
    fn only_eq_codes_parse() {
        assert!(parse("PAGE").is_none());
        assert!(parse("EQUATION").is_none());
        assert!(parse(" EQ").is_some_and(|e| e.nodes.is_empty()));
    }

    /// Text as boxes half an em wide per character, for layouts without fonts.
    struct Fixed;

    impl EqText for Fixed {
        fn text(&mut self, chars: &[(usize, char)]) -> EqBox {
            EqBox { w: chars.len() as f32 * 6.0, a: 9.0, d: 3.0, items: Vec::new() }
        }
        fn glyph(&mut self, _: char, _: usize, size: Option<f32>) -> EqBox {
            let s = size.unwrap_or(12.0);
            EqBox { w: s / 2.0, a: s * 0.75, d: s * 0.25, items: Vec::new() }
        }
        fn metrics(&mut self, _: usize) -> (f32, Rgb) {
            (12.0, Rgb::BLACK)
        }
        fn guide(&mut self, _: bool) -> bool {
            false
        }
    }

    #[test]
    fn overstrike_raise_and_lower_set_the_extents() {
        let m = layout(&parse(r"EQ \o(\s\up 6(AAA),\s\do 2(B))").unwrap(), &mut Fixed);
        assert_eq!(m.width, 18.0);
        assert_eq!((m.ascent, m.descent), (15.0, 5.0));
    }

    #[test]
    fn hostile_codes_never_panic() {
        let deep = format!("EQ {}x", r"\o(".repeat(5000));
        let wide = format!("EQ \\a\\co64({})", "1,".repeat(20000));
        for code in [
            deep.as_str(),
            wide.as_str(),
            r"EQ \",
            r"EQ \s\up",
            r"EQ \b\lc\",
            "EQ \\d\\fo-99999()",
            "EQ \\a\\co0(1,2,3)",
            "EQ \\i\\vc\\(,,)",
            "EQ \\s\\up1e30(x)",
            "EQ \\r(,,)\\x\\to()\\f()",
        ] {
            let m = layout(&parse(code).unwrap_or_default(), &mut Fixed);
            assert!(m.width.is_finite() && m.ascent.is_finite() && m.descent.is_finite(), "{code}");
        }
    }
}
