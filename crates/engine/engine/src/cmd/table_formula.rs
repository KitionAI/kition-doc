//! Word's table formulas (Table Layout › Formula, `=` fields in table cells): arithmetic
//! (`+ - * / ^ %`, comparisons), cell references (`B2`, `B2:C4`, `B:B`, `3:3`), the positional
//! ranges `ABOVE`, `BELOW`, `LEFT` and `RIGHT`, Word's functions, and the `\#` number picture
//! (`\# "£#,##0.00"`). A cell's number is its text read as a number, allowing currency signs,
//! thousands separators, a percent sign and parentheses for negatives ("£360.50", "(45)").

use kition_doc_doc::{Cell, Table};

/// Deepest nesting of parentheses and function calls a formula may use.
const MAX_DEPTH: u32 = 64;
/// Most cells one range may name (a table has at most 63 columns; rows are capped here).
const MAX_CELLS: usize = 100_000;

/// The text a formula shows in the cell at `row`, `col` (from 0) of `table`: the value formatted
/// by its `\#` picture, else as Word shows a plain number. An error names what is wrong.
pub fn evaluate(code: &str, table: &Table, row: usize, col: usize) -> Result<String, String> {
    let code = code.trim();
    let body = code.strip_prefix('=').unwrap_or(code);
    let (expression, picture) = split_switches(body)?;
    let mut parser = Parser { text: expression.chars().collect(), at: 0, depth: 0, table, row, col };
    let value = parser.comparison()?;
    parser.skip_space();
    if parser.at < parser.text.len() {
        return Err(format!("Syntax Error, {}", parser.text.get(parser.at..).map(|r| r.iter().collect::<String>()).unwrap_or_default()));
    }
    if !value.is_finite() {
        return Err("Undefined result".into());
    }
    Ok(match picture {
        Some(picture) => format_picture(value, &picture),
        None => plain(value),
    })
}

/// A number as Word shows a formula's result without a picture: whole numbers without a decimal
/// point, others to two places.
pub fn plain(value: f64) -> String {
    let rounded = (value * 1e10).round() / 1e10;
    if rounded.fract() == 0.0 && rounded.abs() < 1e15 { format!("{}", rounded as i64) } else { format!("{rounded:.2}") }
}

/// The expression before the switches, and the `\#` picture when there is one.
fn split_switches(body: &str) -> Result<(&str, Option<String>), String> {
    let Some(at) = body.find('\\') else { return Ok((body, None)) };
    let (expression, mut rest) = body.split_at(at);
    let mut picture = None;
    while let Some(switch) = rest.strip_prefix('\\') {
        let mut chars = switch.chars();
        let kind = chars.next().unwrap_or(' ');
        let after = chars.as_str().trim_start();
        let (argument, next) = if let Some(quoted) = after.strip_prefix('"') {
            let end = quoted.find('"').ok_or("Missing closing quote in the number picture")?;
            (quoted.get(..end).unwrap_or(""), quoted.get(end + 1..).unwrap_or(""))
        } else {
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            (after.get(..end).unwrap_or(""), after.get(end..).unwrap_or(""))
        };
        if kind == '#' {
            picture = Some(argument.to_owned());
        }
        rest = next.trim_start();
    }
    Ok((expression, picture))
}

/// A cell's number, when its text is one: digits with an optional decimal point, thousands
/// separators, currency signs, a sign or parentheses for negatives and a percent sign.
pub fn cell_number(text: &str) -> Option<f64> {
    let mut t = text.trim();
    let mut negative = false;
    if let Some(inner) = t.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
        negative = true;
        t = inner.trim();
    }
    let is_currency = |c: char| matches!(c, '$' | '£' | '€' | '¥' | '¢' | '₩' | '₹' | '₽' | '₺' | '₪' | '₫' | '₱' | '฿' | '₴' | '₦' | '₡' | '﷼');
    let t = t.trim_matches(|c: char| is_currency(c) || c.is_whitespace() || c == '%');
    let (t, minus) = match t.strip_prefix('-').or_else(|| t.strip_prefix('\u{2212}')) {
        Some(rest) => (rest.trim_start_matches(|c: char| is_currency(c) || c.is_whitespace()), true),
        None => (t, false),
    };
    if t.is_empty() || !t.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let digits: String = t.chars().filter(|c| *c != ',').collect();
    if !digits.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let value: f64 = digits.parse().ok()?;
    Some(if negative != minus { -value } else { value })
}

fn cell_text(cell: &Cell) -> String {
    cell.blocks.iter().filter_map(|b| b.as_para().map(|p| p.plain_text())).collect::<Vec<_>>().join("\n")
}

struct Parser<'a> {
    text: Vec<char>,
    at: usize,
    depth: u32,
    table: &'a Table,
    row: usize,
    col: usize,
}

/// A function argument: one value, or the numbers of a range of cells.
enum Arg {
    Value(f64),
    Cells(Vec<f64>),
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.text.get(self.at).copied()
    }
    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }
    fn eat(&mut self, c: char) -> bool {
        self.skip_space();
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn error(&self) -> String {
        let rest: String = self.text.get(self.at..).map(|r| r.iter().collect()).unwrap_or_default();
        if rest.is_empty() { "Syntax Error".into() } else { format!("Syntax Error, {rest}") }
    }
    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { Err("Formula too deeply nested".into()) } else { Ok(()) }
    }

    fn comparison(&mut self) -> Result<f64, String> {
        let left = self.sum()?;
        self.skip_space();
        let op: String = match (self.peek(), self.text.get(self.at + 1).copied()) {
            (Some('<'), Some('=')) | (Some('>'), Some('=')) | (Some('<'), Some('>')) => {
                self.at += 2;
                self.text.get(self.at - 2..self.at).map(|o| o.iter().collect()).unwrap_or_default()
            }
            (Some(c @ ('<' | '>' | '=')), _) => {
                self.at += 1;
                c.to_string()
            }
            _ => return Ok(left),
        };
        let right = self.sum()?;
        let holds = match op.as_str() {
            "<" => left < right,
            ">" => left > right,
            "<=" => left <= right,
            ">=" => left >= right,
            "<>" => left != right,
            _ => left == right,
        };
        Ok(if holds { 1.0 } else { 0.0 })
    }
    fn sum(&mut self) -> Result<f64, String> {
        let mut value = self.product()?;
        loop {
            if self.eat('+') {
                value += self.product()?;
            } else if self.eat('-') {
                value -= self.product()?;
            } else {
                return Ok(value);
            }
        }
    }
    fn product(&mut self) -> Result<f64, String> {
        let mut value = self.power()?;
        loop {
            if self.eat('*') {
                value *= self.power()?;
            } else if self.eat('/') {
                let divisor = self.power()?;
                if divisor == 0.0 {
                    return Err("Zero Divide".into());
                }
                value /= divisor;
            } else {
                return Ok(value);
            }
        }
    }
    fn power(&mut self) -> Result<f64, String> {
        let base = self.unary()?;
        if self.eat('^') { Ok(base.powf(self.unary()?)) } else { Ok(base) }
    }
    fn unary(&mut self) -> Result<f64, String> {
        self.enter()?;
        let value = if self.eat('-') {
            self.unary().map(|v| -v)
        } else if self.eat('+') {
            self.unary()
        } else {
            self.postfix()
        };
        self.depth -= 1;
        value
    }
    fn postfix(&mut self) -> Result<f64, String> {
        let value = self.primary()?;
        Ok(if self.eat('%') { value / 100.0 } else { value })
    }
    fn primary(&mut self) -> Result<f64, String> {
        self.skip_space();
        if self.eat('(') {
            let inner = self.comparison()?;
            return if self.eat(')') { Ok(inner) } else { Err(self.error()) };
        }
        if self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            return self.number();
        }
        let start = self.at;
        let word = self.word();
        if word.is_empty() {
            return Err(self.error());
        }
        if self.eat('(') {
            return self.function(&word);
        }
        match word.as_str() {
            "TRUE" => return Ok(1.0),
            "FALSE" => return Ok(0.0),
            _ => {}
        }
        // A single cell: its number, or 0 when it has none (an empty cell counts as 0).
        self.at = start;
        match self.range()? {
            Some(cells) if cells.len() == 1 => Ok(cells.first().copied().flatten().unwrap_or(0.0)),
            Some(_) => Err(format!("Syntax Error, {word}: a range needs a function such as SUM")),
            None => Err(self.error()),
        }
    }
    fn number(&mut self) -> Result<f64, String> {
        let start = self.at;
        // No thousands separators: a comma separates a function's arguments.
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            self.at += 1;
        }
        let digits: String = self.text.get(start..self.at).map(|d| d.iter().collect()).unwrap_or_default();
        digits.parse().map_err(|_| format!("Syntax Error, {digits}"))
    }
    /// Letters and digits (a function name, `ABOVE`, a cell reference), upper-cased.
    fn word(&mut self) -> String {
        self.skip_space();
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
            self.at += 1;
        }
        self.text.get(start..self.at).map(|w| w.iter().collect::<String>().to_ascii_uppercase()).unwrap_or_default()
    }

    fn function(&mut self, name: &str) -> Result<f64, String> {
        self.enter()?;
        let mut args = Vec::new();
        if !self.eat(')') {
            loop {
                args.push(self.argument()?);
                if self.eat(',') || self.eat(';') {
                    continue;
                }
                if self.eat(')') {
                    break;
                }
                return Err(self.error());
            }
        }
        self.depth -= 1;
        let numbers: Vec<f64> = args
            .iter()
            .flat_map(|a| match a {
                Arg::Value(v) => vec![*v],
                Arg::Cells(c) => c.clone(),
            })
            .collect();
        let scalar = |i: usize| match args.get(i) {
            Some(Arg::Value(v)) => Ok(*v),
            Some(Arg::Cells(c)) if c.len() == 1 => Ok(c.first().copied().unwrap_or(0.0)),
            _ => Err(format!("Syntax Error, {name} needs a value")),
        };
        Ok(match name {
            "SUM" => numbers.iter().sum(),
            "AVERAGE" => {
                if numbers.is_empty() {
                    0.0
                } else {
                    numbers.iter().sum::<f64>() / numbers.len() as f64
                }
            }
            "COUNT" => numbers.len() as f64,
            "MAX" => numbers.iter().copied().reduce(f64::max).unwrap_or(0.0),
            "MIN" => numbers.iter().copied().reduce(f64::min).unwrap_or(0.0),
            "PRODUCT" => {
                if numbers.is_empty() {
                    0.0
                } else {
                    numbers.iter().product()
                }
            }
            "ABS" => scalar(0)?.abs(),
            "INT" => scalar(0)?.trunc(),
            "SIGN" => {
                let v = scalar(0)?;
                if v > 0.0 {
                    1.0
                } else if v < 0.0 {
                    -1.0
                } else {
                    0.0
                }
            }
            "MOD" => {
                let d = scalar(1)?;
                if d == 0.0 {
                    return Err("Zero Divide".into());
                }
                scalar(0)? % d
            }
            "ROUND" => {
                let places = scalar(1)?.clamp(-15.0, 15.0) as i32;
                let scale = 10f64.powi(places);
                (scalar(0)? * scale).round() / scale
            }
            "IF" => {
                if scalar(0)? != 0.0 {
                    scalar(1)?
                } else {
                    scalar(2)?
                }
            }
            "AND" => f64::from(u8::from(scalar(0)? != 0.0 && scalar(1)? != 0.0)),
            "OR" => f64::from(u8::from(scalar(0)? != 0.0 || scalar(1)? != 0.0)),
            "NOT" => f64::from(u8::from(scalar(0)? == 0.0)),
            _ => return Err(format!("Syntax Error, {name} is not a formula function")),
        })
    }

    /// A function argument: a range (`ABOVE`, `B2:B5`, `B:B`, `B2`) gives its cells' numbers;
    /// anything else is an expression.
    fn argument(&mut self) -> Result<Arg, String> {
        let start = self.at;
        if let Some(cells) = self.range()? {
            self.skip_space();
            // A lone cell may go on as an expression (`B2*2`).
            if matches!(self.peek(), Some(',' | ';' | ')')) {
                return Ok(Arg::Cells(cells.into_iter().flatten().collect()));
            }
            self.at = start;
        }
        self.at = start;
        Ok(Arg::Value(self.comparison()?))
    }

    /// A range of cells at the parser, read when there is one: each cell's number (`None` when
    /// its text is not one). The parser is left after it; `None` (parser unmoved) otherwise.
    fn range(&mut self) -> Result<Option<Vec<Option<f64>>>, String> {
        let start = self.at;
        let word = self.word();
        let rows = self.table.rows.len();
        let cols_of = |r: usize| self.table.rows.get(r).map_or(0, |row| row.cells.len());
        let cells: Vec<(usize, usize)> = match word.as_str() {
            "ABOVE" => (0..self.row).rev().map(|r| (r, self.col)).collect(),
            "BELOW" => (self.row + 1..rows).map(|r| (r, self.col)).collect(),
            "LEFT" => (0..self.col).rev().map(|c| (self.row, c)).collect(),
            "RIGHT" => (self.col + 1..cols_of(self.row)).map(|c| (self.row, c)).collect(),
            _ => {
                self.at = start;
                let Some(first) = self.reference() else {
                    self.at = start;
                    return Ok(None);
                };
                let second = if self.eat(':') { Some(self.reference().ok_or_else(|| self.error())?) } else { None };
                let (r0, r1, c0, c1) = match (first, second) {
                    ((Some(c), Some(r)), None) => (r, r, c, c),
                    ((c0, r0), Some((c1, r1))) => {
                        let rows_of = |a: Option<usize>, b: Option<usize>| match (a, b) {
                            (Some(a), Some(b)) => (a.min(b), a.max(b)),
                            _ => (0, rows.saturating_sub(1)),
                        };
                        let (r0, r1) = rows_of(r0, r1);
                        let (c0, c1) = match (c0, c1) {
                            (Some(a), Some(b)) => (a.min(b), a.max(b)),
                            _ => (0, 62),
                        };
                        (r0, r1, c0, c1)
                    }
                    _ => {
                        self.at = start;
                        return Ok(None);
                    }
                };
                let mut cells = Vec::new();
                for r in r0..=r1.min(rows.saturating_sub(1)) {
                    for c in c0..=c1.min(cols_of(r).saturating_sub(1)) {
                        if cells.len() >= MAX_CELLS {
                            return Err("Range too large".into());
                        }
                        cells.push((r, c));
                    }
                }
                cells
            }
        };
        Ok(Some(
            cells
                .into_iter()
                .map(|(r, c)| self.table.rows.get(r).and_then(|row| row.cells.get(c)).and_then(|cell| cell_number(&cell_text(cell))))
                .collect(),
        ))
    }

    /// A cell reference's column and row (from 0): `B2`, a whole column `B`, or a whole row `2`.
    fn reference(&mut self) -> Option<(Option<usize>, Option<usize>)> {
        self.skip_space();
        let start = self.at;
        let mut col: Option<usize> = None;
        while let Some(c) = self.peek().filter(char::is_ascii_alphabetic) {
            let digit = (c.to_ascii_uppercase() as usize).checked_sub('A' as usize)? + 1;
            col = Some(col.unwrap_or(0).checked_mul(26)?.checked_add(digit)?);
            self.at += 1;
        }
        let mut row: Option<usize> = None;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
            row = Some(row.unwrap_or(0).checked_mul(10)?.checked_add(d as usize)?);
            self.at += 1;
        }
        // Letters must not run on into a name (`SUM`), and a reference names a column or a row.
        if self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == '(') || (col.is_none() && row.is_none()) {
            self.at = start;
            return None;
        }
        if col.is_some_and(|c| c > 63) || row == Some(0) {
            self.at = start;
            return None;
        }
        Some((col.map(|c| c - 1), row.map(|r| r - 1)))
    }
}

/// `value` written with a Word numeric picture (`\# "£#,##0.00"`, `\# "0.0%"`,
/// `\# "#,##0;(#,##0);-"`): `0` is a digit always shown, `#` one shown when needed, `,` groups
/// thousands, `.` is the decimal point; other text (and text in single quotes) is shown as it is.
/// Sections after `;` are for negative numbers and zero.
pub fn format_picture(value: f64, picture: &str) -> String {
    let sections = split_sections(picture);
    let section_for = |i: usize| sections.get(i).map(String::as_str);
    let (section, signed) = match (section_for(1), section_for(2)) {
        (Some(negative), _) if value < 0.0 => (negative, false),
        (_, Some(zero)) if value == 0.0 => (zero, false),
        _ => (section_for(0).unwrap_or(""), value < 0.0),
    };
    let mut prefix = String::new();
    let mut pattern = String::new();
    let mut suffix = String::new();
    let chars: Vec<char> = section.chars().collect();
    let (mut quoted, mut state) = (false, 0u8); // 0: before the digits, 1: in them, 2: after
    for (i, &c) in chars.iter().enumerate() {
        if c == '\'' {
            quoted = !quoted;
            continue;
        }
        let starts = "#0x".contains(c) || (".,".contains(c) && chars.get(i + 1).is_some_and(|n| "#0".contains(*n)));
        match state {
            0 if !quoted && starts => {
                state = 1;
                pattern.push(c);
            }
            0 => prefix.push(c),
            1 if !quoted && "#0x,.".contains(c) => pattern.push(c),
            _ => {
                state = 2;
                suffix.push(c);
            }
        }
    }
    // A section without digits ("-" for zero) is shown as it is; an empty picture as a plain number.
    if pattern.is_empty() {
        return if section.is_empty() { plain(value) } else { prefix };
    }
    let (int_pattern, frac_pattern) = pattern.split_once('.').unwrap_or((&pattern, ""));
    let frac_max = frac_pattern.chars().filter(|c| "0#x".contains(*c)).count().min(15);
    let frac_min = frac_pattern.chars().filter(|c| *c == '0').count().min(frac_max);
    let int_min = int_pattern.chars().filter(|c| *c == '0').count().min(30);
    let grouped = int_pattern.contains(',');
    let scale = 10f64.powi(frac_max as i32);
    let rounded = (value.abs() * scale).round() / scale;
    let text = format!("{rounded:.frac_max$}");
    let (int_digits, frac_digits) = text.split_once('.').unwrap_or((&text, ""));
    let mut int_digits = int_digits.trim_start_matches('0').to_owned();
    while int_digits.len() < int_min {
        int_digits.insert(0, '0');
    }
    if grouped {
        let mut out = String::new();
        for (i, c) in int_digits.chars().enumerate() {
            if i > 0 && (int_digits.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        int_digits = out;
    }
    let mut frac_digits = frac_digits.to_owned();
    while frac_digits.len() > frac_min && frac_digits.ends_with('0') {
        frac_digits.pop();
    }
    let number = if frac_digits.is_empty() { int_digits } else { format!("{int_digits}.{frac_digits}") };
    let number = if number.is_empty() { "0".to_owned() } else { number };
    let sign = if signed && rounded != 0.0 { "-" } else { "" };
    format!("{sign}{prefix}{number}{suffix}")
}

/// A picture's sections, split at `;` outside single quotes.
fn split_sections(picture: &str) -> Vec<String> {
    let mut sections = vec![String::new()];
    let mut quoted = false;
    for c in picture.chars() {
        if c == '\'' {
            quoted = !quoted;
        }
        if c == ';' && !quoted {
            sections.push(String::new());
        } else if let Some(last) = sections.last_mut() {
            last.push(c);
        }
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_numbers_allow_currency_and_separators() {
        assert_eq!(cell_number("£360.50"), Some(360.5));
        assert_eq!(cell_number(" $1,200 "), Some(1200.0));
        assert_eq!(cell_number("(45)"), Some(-45.0));
        assert_eq!(cell_number("-€ 12.5"), Some(-12.5));
        assert_eq!(cell_number("12%"), Some(12.0));
        for text in ["", "Amount", "12 pcs", "1.2.3", "£"] {
            assert_eq!(cell_number(text), None, "{text}");
        }
    }

    #[test]
    fn number_pictures() {
        assert_eq!(format_picture(1008.9, "£#,##0.00"), "£1,008.90");
        assert_eq!(format_picture(0.5, "0.0%"), "0.5%");
        assert_eq!(format_picture(-1234.0, "#,##0;(#,##0);-"), "(1,234)");
        assert_eq!(format_picture(0.0, "#,##0;(#,##0);-"), "-");
        assert_eq!(format_picture(-3.456, "0.00"), "-3.46");
        assert_eq!(format_picture(7.0, "#,##0.##"), "7");
        assert_eq!(format_picture(1234567.891, "#,##0.00 'GBP'"), "1,234,567.89 GBP");
    }
}
