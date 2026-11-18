//! Field results computed at layout time (page numbers) and note numbering.

use std::collections::HashMap;
use std::sync::Arc;

use kition_doc_doc::para::CodeField;
use kition_doc_doc::section::NumFormat;

/// Context for fields that depend on where text lands.
#[derive(Clone, Debug, Default)]
pub struct FieldCtx {
    /// Current page number (as numbered) and its format.
    pub page: u32,
    pub page_format: NumFormat,
    /// Total pages in the document.
    pub pages: u32,
    /// Pages in the current section.
    pub section_pages: u32,
    pub section: u32,
    /// Note part id → number, in document order.
    pub notes: Arc<HashMap<u32, u32>>,
    /// Document title / author for TITLE / AUTHOR fields.
    pub title: Arc<str>,
    pub author: Arc<str>,
    pub filename: Arc<str>,
}

impl FieldCtx {
    pub fn note_number(&self, id: u32) -> u32 {
        self.notes.get(&id).copied().unwrap_or(1)
    }
    /// A key for caches: the parts of the context a page-dependent paragraph depends on.
    pub fn page_key(&self) -> (u32, u32, u32, u32) {
        (self.page, self.pages, self.section_pages, self.section)
    }
}

/// The field's name (first word of the instruction, upper-cased).
pub fn field_name(instr: &str) -> String {
    instr.split_whitespace().next().unwrap_or("").trim_start_matches('=').to_ascii_uppercase()
}

/// `\* FORMAT` switch value (Arabic, roman, ROMAN, alphabetic, ALPHABETIC, CardText, Ordinal…).
fn format_switch(instr: &str) -> Option<NumFormat> {
    let mut it = instr.split_whitespace();
    while let Some(w) = it.next() {
        if w == "\\*" {
            let v = it.next()?;
            return Some(match v {
                "roman" => NumFormat::LowerRoman,
                "ROMAN" | "Roman" => NumFormat::UpperRoman,
                "alphabetic" => NumFormat::LowerLetter,
                "ALPHABETIC" | "Alphabetic" => NumFormat::UpperLetter,
                "CardText" | "cardtext" => NumFormat::CardinalText,
                "OrdText" | "ordtext" => NumFormat::OrdinalText,
                "Ordinal" | "ordinal" => NumFormat::Ordinal,
                "Arabic" | "arabic" => NumFormat::Decimal,
                _ => continue,
            });
        }
    }
    None
}

/// Display text for a field whose code may hold other fields (see [`CodeField`]), and whether it
/// depends on the page it lands on. A formula (`=`) is computed again from its nested fields'
/// current values; anything it can't compute keeps the saved result.
pub fn field_text_with(instr: &str, code_fields: &[CodeField], result: &str, ctx: &FieldCtx) -> (String, bool) {
    if !instr.trim_start().starts_with('=') {
        return field_text(instr, result, ctx);
    }
    let mut code = String::with_capacity(instr.len());
    let (mut at, mut page_dependent) = (0usize, false);
    for nested in code_fields {
        let end = nested.start.saturating_add(nested.len);
        let (Some(before), Some(cached)) = (instr.get(at..nested.start), instr.get(nested.start..end)) else {
            return (result.to_string(), false);
        };
        let (value, dependent) = field_text(&nested.instr, cached, ctx);
        code.push_str(before);
        code.push_str(&value);
        page_dependent |= dependent;
        at = end;
    }
    code.push_str(instr.get(at..).unwrap_or(""));
    match formula_value(&code) {
        Some(value) => (value, page_dependent),
        None => (result.to_string(), false),
    }
}

/// The value of a formula field's arithmetic (`= 4 + 1`), formatted as Word shows a plain number;
/// `None` for anything else (functions, bookmarks, comparisons, number pictures, errors).
fn formula_value(code: &str) -> Option<String> {
    let body = code.trim_start().strip_prefix('=')?;
    // Only the general-format switch is ignored; a `\#` picture would change how the number shows.
    let (expression, switches) = body.split_once('\\').unwrap_or((body, ""));
    if !switches.is_empty() && !switches.trim_start().starts_with('*') {
        return None;
    }
    let mut parser = Formula { text: expression.as_bytes(), at: 0, depth: 0 };
    let value = parser.sum()?;
    parser.skip_space();
    if parser.at != parser.text.len() || !value.is_finite() {
        return None;
    }
    let rounded = (value * 1e10).round() / 1e10;
    Some(if rounded.fract() == 0.0 && rounded.abs() < 1e15 { format!("{}", rounded as i64) } else { format!("{rounded}") })
}

/// Recursive-descent arithmetic over `+ - * / ^`, parentheses and decimal numbers.
struct Formula<'a> {
    text: &'a [u8],
    at: usize,
    depth: u32,
}

impl Formula<'_> {
    const MAX_DEPTH: u32 = 64;

    fn skip_space(&mut self) {
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.skip_space();
        if self.text.get(self.at) == Some(&c) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn sum(&mut self) -> Option<f64> {
        let mut value = self.product()?;
        loop {
            if self.eat(b'+') {
                value += self.product()?;
            } else if self.eat(b'-') {
                value -= self.product()?;
            } else {
                return Some(value);
            }
        }
    }
    fn product(&mut self) -> Option<f64> {
        let mut value = self.power()?;
        loop {
            if self.eat(b'*') {
                value *= self.power()?;
            } else if self.eat(b'/') {
                let divisor = self.power()?;
                if divisor == 0.0 {
                    return None;
                }
                value /= divisor;
            } else {
                return Some(value);
            }
        }
    }
    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if self.eat(b'^') { Some(base.powf(self.unary()?)) } else { Some(base) }
    }
    fn unary(&mut self) -> Option<f64> {
        self.depth += 1;
        if self.depth > Self::MAX_DEPTH {
            return None;
        }
        let value = if self.eat(b'-') {
            self.unary().map(|v| -v)
        } else if self.eat(b'+') {
            self.unary()
        } else if self.eat(b'(') {
            let inner = self.sum()?;
            if self.eat(b')') { Some(inner) } else { None }
        } else {
            self.number()
        };
        self.depth -= 1;
        value
    }
    fn number(&mut self) -> Option<f64> {
        self.skip_space();
        let start = self.at;
        while self.text.get(self.at).is_some_and(|c| c.is_ascii_digit() || *c == b'.') {
            self.at += 1;
        }
        std::str::from_utf8(self.text.get(start..self.at)?).ok()?.parse().ok()
    }
}

/// Display text for a field and whether it depends on the page it lands on.
pub fn field_text(instr: &str, result: &str, ctx: &FieldCtx) -> (String, bool) {
    let name = field_name(instr);
    let fmt = format_switch(instr);
    match name.as_str() {
        "PAGE" => (fmt.unwrap_or(ctx.page_format).format(ctx.page), true),
        "NUMPAGES" => (fmt.unwrap_or(NumFormat::Decimal).format(ctx.pages), true),
        "SECTIONPAGES" => (fmt.unwrap_or(NumFormat::Decimal).format(ctx.section_pages), true),
        "SECTION" => (fmt.unwrap_or(NumFormat::Decimal).format(ctx.section), true),
        "TITLE" if result.is_empty() => (ctx.title.to_string(), false),
        "AUTHOR" if result.is_empty() => (ctx.author.to_string(), false),
        "FILENAME" if result.is_empty() => (ctx.filename.to_string(), false),
        _ => (result.to_string(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_fields() {
        let ctx = FieldCtx { page: 4, pages: 12, ..Default::default() };
        assert_eq!(field_text("PAGE", "", &ctx), ("4".into(), true));
        assert_eq!(field_text(" PAGE \\* ROMAN ", "", &ctx).0, "IV");
        assert_eq!(field_text("NUMPAGES \\* CardText", "", &ctx).0, "Twelve");
        assert_eq!(field_text("DATE \\@ \"M/d/yyyy\"", "1/2/2026", &ctx), ("1/2/2026".into(), false));
        assert_eq!(field_name("=SUM(ABOVE)"), "SUM(ABOVE)");
    }

    #[test]
    fn formulas_compute_with_the_fields_nested_in_their_code() {
        let ctx = FieldCtx { page: 4, pages: 12, ..Default::default() };
        let page = |start, len| vec![CodeField { start, len, instr: "page".into() }];
        assert_eq!(field_text_with("=6", &page(1, 1), "6", &ctx), ("4".into(), true));
        assert_eq!(field_text_with("= + 1", &page(1, 0), "7", &ctx), ("5".into(), true));
        assert_eq!(field_text_with("=2*(3+1)-1.5 \\* MERGEFORMAT", &[], "x", &ctx), ("6.5".into(), false));
        // Functions, bookmarks and errors keep the result Word saved.
        assert_eq!(field_text_with("=SUM(ABOVE)", &[], "42", &ctx), ("42".into(), false));
        assert_eq!(field_text_with("=1/0", &[], "x", &ctx), ("x".into(), false));
        assert_eq!(field_text_with("=", &[], "x", &ctx), ("x".into(), false));
        let deep = format!("={}1{}", "(".repeat(10_000), ")".repeat(10_000));
        assert_eq!(field_text_with(&deep, &[], "kept", &ctx).0, "kept");
        // A nested range outside the code is ignored, never sliced.
        assert_eq!(field_text_with("=6", &page(5, 9), "6", &ctx).0, "6");
    }
}
