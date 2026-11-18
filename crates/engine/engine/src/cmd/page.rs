//! Layout tab: page setup (margins, orientation, size, columns), breaks, line numbers, hyphenation.

use kition_doc_doc::section::{Columns, LineNumbering, SectionProps, SectionStart};
use kition_doc_doc::{Block, Pos, StoryRef};
use serde_json::{Value, json};

use super::sel_result;
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("layout.margins", "Margins", "Layout › Page Setup", margins).params(
            r#"{"preset"?: "normal|narrow|moderate|wide|mirrored|office2003", "top"?: pt, "bottom"?: pt, "left"?: pt, "right"?: pt, "gutter"?: pt}"#,
        ),
        CommandSpec::new("layout.orientation", "Orientation", "Layout › Page Setup", |s, v| {
            let land = match p::str(v, "value") {
                Some("landscape") => true,
                Some("portrait") => false,
                _ => !sect(s).landscape,
            };
            with_sect(s, |x| x.set_landscape(land))
        })
        .params(r#"{"value": "portrait|landscape"}"#),
        CommandSpec::new("layout.size", "Size", "Layout › Page Setup", size).params(
            r#"{"name"?: "Letter|Legal|A4|…" (keeps the orientation), "width"?: pt, "height"?: pt (as given: wider than tall is landscape)}"#,
        ),
        CommandSpec::new("layout.columns", "Columns", "Layout › Page Setup", columns)
            .params(
                r#"{"count": 1-12, "space"?: pt, "separator"?: bool, "preset"?: "left|right", "widths"?: [[width pt, space after pt], …] (unequal columns; sets the count), "apply"?: "section|document|forward"}"#,
            ),
        CommandSpec::new("layout.break", "Breaks", "Layout › Page Setup", breaks)
            .params(r#"{"kind": "page|column|textWrapping|nextPage|continuous|evenPage|oddPage"}"#),
        CommandSpec::new("layout.lineNumbers", "Line Numbers", "Layout › Page Setup", |s, v| {
            let mode = p::str(v, "value").unwrap_or("continuous");
            with_sect(s, |x| {
                x.line_numbers = match mode {
                    "none" => None,
                    "restartPage" => Some(LineNumbering { restart: kition_doc_doc::section::LineNumberRestart::Page, ..Default::default() }),
                    "restartSection" => Some(LineNumbering { restart: kition_doc_doc::section::LineNumberRestart::Section, ..Default::default() }),
                    _ => Some(LineNumbering { restart: kition_doc_doc::section::LineNumberRestart::Continuous, ..Default::default() }),
                }
            })
        })
        .params(r#"{"value": "none|continuous|restartPage|restartSection"}"#),
        CommandSpec::new("layout.hyphenation", "Hyphenation", "Layout › Page Setup", |s, v| {
            let on = p::bool(v, "value").unwrap_or(!s.doc.settings.auto_hyphenation);
            s.doc.settings.auto_hyphenation = on;
            Ok(json!({"value": on}))
        }),
        CommandSpec::new("layout.pageSetup", "Page Setup", "Layout › Page Setup", |s, v| {
            if let Some(props) = v.get("section") {
                // Only the fields given change; the others keep the section's values.
                with_section_fields(&sect(s), props)?;
                return with_sect(s, |x| {
                    if let Ok(changed) = with_section_fields(x, props) {
                        *x = changed;
                    }
                });
            }
            s.ui_requests.push(json!({"open": "pageSetup"}));
            Ok(serde_json::to_value(sect(s)).unwrap_or(Value::Null))
        })
        .params(r#"{"section"?: SectionProps (the fields to change, as layout.section answers them; the others are kept)}"#),
        CommandSpec::new("layout.verticalAlign", "Vertical Alignment", "Layout › Page Setup › Layout", |s, v| {
            let va = match p::str(v, "value") {
                Some("top") => kition_doc_doc::props::VAlign::Top,
                Some("center") => kition_doc_doc::props::VAlign::Center,
                Some("bottom") => kition_doc_doc::props::VAlign::Bottom,
                other => return Err(CmdError::Params(format!("value must be top, center or bottom, not {}", other.unwrap_or("nothing")))),
            };
            with_sect(s, |x| x.valign = va)
        })
        .params(r#"{"value": "top|center|bottom"}"#),
        CommandSpec::new("layout.differentFirstPage", "Different First Page", "Header & Footer › Options", |s, v| {
            let on = p::bool(v, "value").unwrap_or(!sect(s).title_page);
            with_sect(s, |x| x.title_page = on)
        }),
        CommandSpec::new("layout.differentOddEven", "Different Odd & Even Pages", "Header & Footer › Options", |s, v| {
            s.doc.settings.even_odd_headers = p::bool(v, "value").unwrap_or(!s.doc.settings.even_odd_headers);
            sel_result(s)
        }),
        CommandSpec::new("layout.pageNumberFormat", "Format Page Numbers", "Insert › Header & Footer › Page Number", |s, v| {
            let fmt = p::str(v, "format").map(kition_doc_doc::section::NumFormat::from_ooxml);
            let start = p::u64(v, "start").map(|x| x.min(100_000) as u32);
            with_sect(s, |x| {
                if let Some(f) = fmt {
                    x.page_num_format = f;
                }
                x.page_num_start = start;
            })
        })
        .params(r#"{"format"?: "decimal|lowerRoman|upperRoman|lowerLetter|upperLetter", "start"?: n}"#),
        CommandSpec::new("layout.section", "Section Properties", "Layout › Page Setup", |s, _| {
            Ok(serde_json::to_value(sect(s)).unwrap_or(Value::Null))
        })
        .pure(),
    ]
}

/// The body block the selection's section is counted from: the selection's in the body; in a
/// header, footer or note, the body place the caret came from (`body_return`). A path in another
/// story numbers that story's blocks, not the body's (C8-doc-5: a caret in a header changed the
/// section of body block 0). `None`: no body place to go by.
fn body_block(s: &Session, at: &Pos) -> Option<usize> {
    let pos = if at.story == StoryRef::Body { Some(at) } else { s.body_return.as_ref().filter(|p| p.story == StoryRef::Body) };
    pos.map(|p| p.path.0.first().copied().unwrap_or(0) as usize)
}

fn block_of(s: &Session) -> usize {
    body_block(s, &s.sel.focus).unwrap_or(0)
}

pub fn sect(s: &Session) -> SectionProps {
    let i = s.doc.section_index_of(block_of(s));
    s.doc.sections().get(i).map(|(_, x)| (*x).clone()).unwrap_or_default()
}

/// Apply to the section(s) of the selection.
fn with_sect(s: &mut Session, f: impl Fn(&mut SectionProps)) -> CmdResult {
    let (a, b) = s.sel.ordered();
    let (Some(i0), Some(i1)) = (body_block(s, &a), body_block(s, &b)) else {
        return Err(CmdError::Params("the selection is in no section: put the caret in the body, or open the header or footer from it".into()));
    };
    let ends: Vec<usize> = s.doc.sections().iter().map(|(e, _)| *e).collect();
    let first = ends.iter().position(|e| i0 <= *e).unwrap_or(0);
    let last = ends.iter().position(|e| i1 <= *e).unwrap_or(first);
    for k in first..=last {
        let block = ends.get(k).copied().unwrap_or(0);
        f(s.doc.section_mut(block));
    }
    Ok(serde_json::to_value(sect(s)).unwrap_or(Value::Null))
}

fn margins(s: &mut Session, v: &Value) -> CmdResult {
    let preset = p::str(v, "preset");
    let (t, b, l, r) = match preset {
        Some("normal") => (72.0, 72.0, 72.0, 72.0),
        Some("narrow") => (36.0, 36.0, 36.0, 36.0),
        Some("moderate") => (72.0, 72.0, 54.0, 54.0),
        Some("wide") => (72.0, 72.0, 144.0, 144.0),
        Some("mirrored") => (72.0, 72.0, 90.0, 72.0),
        Some("office2003") => (72.0, 72.0, 90.0, 90.0),
        Some(x) => return Err(CmdError::Params(format!("unknown preset `{x}`"))),
        None => {
            let c = sect(s);
            (
                p::f32(v, "top").unwrap_or(c.margin_top),
                p::f32(v, "bottom").unwrap_or(c.margin_bottom),
                p::f32(v, "left").unwrap_or(c.margin_left),
                p::f32(v, "right").unwrap_or(c.margin_right),
            )
        }
    };
    let gutter = p::f32(v, "gutter");
    let cur = sect(s);
    if l + r + gutter.unwrap_or(cur.gutter) > cur.page_w - 36.0 || t + b > cur.page_h - 36.0 {
        return Err(CmdError::Params("margins leave no room for text".into()));
    }
    // A preset picks mirrored or not; plain numbers (a ruler drag, an agent) keep the setting.
    if preset.is_some() {
        s.doc.settings.mirror_margins = preset == Some("mirrored");
    }
    with_sect(s, |x| {
        x.margin_top = t.max(0.0);
        x.margin_bottom = b.max(0.0);
        x.margin_left = l.max(0.0);
        x.margin_right = r.max(0.0);
        if let Some(g) = gutter {
            x.gutter = g.max(0.0);
        }
    })
}

fn size(s: &mut Session, v: &Value) -> CmdResult {
    let (w, h) = if let Some(n) = p::str(v, "name") {
        let (_, w, h) = kition_doc_geom::PAPER_SIZES
            .iter()
            .find(|(name, _, _)| name.eq_ignore_ascii_case(n))
            .ok_or_else(|| CmdError::Params(format!("unknown paper `{n}`")))?;
        (*w, *h)
    } else {
        (p::req_f32(v, "width")?, p::req_f32(v, "height")?)
    };
    if !(72.0..=1584.0).contains(&w) || !(72.0..=1584.0).contains(&h) {
        return Err(CmdError::Params("page size must be 1\"–22\"".into()));
    }
    // A named paper keeps the section's orientation; a width and height are the page as given.
    let named = p::str(v, "name").is_some();
    with_sect(s, |x| {
        let landscape = if named { x.landscape } else { w > h };
        let (pw, ph) = if landscape { (w.max(h), w.min(h)) } else { (w.min(h), w.max(h)) };
        x.page_w = pw;
        x.page_h = ph;
        x.landscape = landscape;
    })
}

/// `current` with the fields of `fields` (a JSON object of [`SectionProps`] fields, nested
/// objects merged field by field) changed; a field the section doesn't have is refused.
fn with_section_fields(current: &SectionProps, fields: &Value) -> Result<SectionProps, CmdError> {
    fn merge(into: &mut Value, from: &Value, path: &str) -> Result<(), CmdError> {
        let (Some(into), Some(from)) = (into.as_object_mut(), from.as_object()) else {
            return Err(CmdError::Params(format!("{path} must be an object")));
        };
        for (key, value) in from {
            match into.get_mut(key) {
                Some(slot) if slot.is_object() && value.is_object() => merge(slot, value, &format!("{path}.{key}"))?,
                Some(slot) => *slot = value.clone(),
                // Optional fields that are left out while unset.
                None if key == "grid" && path == "section" => {
                    into.insert(key.clone(), value.clone());
                }
                None => return Err(CmdError::Params(format!("{path} has no field `{key}`"))),
            }
        }
        Ok(())
    }
    let mut merged = serde_json::to_value(current).map_err(|e| CmdError::Failed(e.to_string()))?;
    merge(&mut merged, fields, "section")?;
    serde_json::from_value(merged).map_err(|e| CmdError::Params(e.to_string()))
}

fn columns(s: &mut Session, v: &Value) -> CmdResult {
    let preset = p::str(v, "preset");
    let space = p::f32(v, "space").unwrap_or(36.0).clamp(0.0, 300.0);
    let sep = p::bool(v, "separator").unwrap_or(false);
    let tw = sect(s).text_width();
    // Unequal columns: (width, space after) each, as the Columns dialog sends them.
    let custom = match v.get("widths").filter(|w| !w.is_null()) {
        Some(w) => Some(column_widths(w, tw)?),
        None => None,
    };
    let count = match &custom {
        Some(c) => c.len() as u64,
        None => p::u64(v, "count").unwrap_or(match preset {
            Some("left") | Some("right") => 2,
            _ => 1,
        }),
    };
    if !(1..=12).contains(&count) {
        return Err(CmdError::Params("columns must be 1–12".into()));
    }
    let widths = match (custom, preset) {
        (Some(c), _) if c.len() > 1 => c,
        (Some(_), _) => Vec::new(),
        (None, Some("left")) => vec![((tw - space) / 3.0, space), ((tw - space) * 2.0 / 3.0, 0.0)],
        (None, Some("right")) => vec![((tw - space) * 2.0 / 3.0, space), ((tw - space) / 3.0, 0.0)],
        _ => Vec::new(),
    };
    // Equal columns keep one spacing; unequal ones remember the first gap for a later switch back.
    let space = widths.first().map(|w| w.1).unwrap_or(space);
    let cols = Columns { count: count as u32, space, separator: sep, widths };
    match p::str(v, "apply").unwrap_or("section") {
        "section" => with_sect(s, |x| x.columns = cols.clone()),
        "document" => {
            let ends: Vec<usize> = s.doc.sections().iter().map(|(e, _)| *e).collect();
            for e in ends {
                s.doc.section_mut(e).columns = cols.clone();
            }
            Ok(serde_json::to_value(sect(s)).unwrap_or(Value::Null))
        }
        // From the caret on: a continuous section break first, then the new section's columns.
        "forward" => {
            let start = s.sel.ordered().0;
            s.sel = Selection::caret(start);
            breaks(s, &json!({"kind": "continuous"}))?;
            with_sect(s, |x| x.columns = cols.clone())
        }
        x => Err(CmdError::Params(format!("unknown apply `{x}` (section, document or forward)"))),
    }
}

/// `[[width, space after], …]` in points: 1–12 columns, each at least 0.25", that fit the text
/// width (`tw`).
fn column_widths(v: &Value, tw: f32) -> Result<Vec<(f32, f32)>, CmdError> {
    let bad = || CmdError::Params("widths: [[width, space after], …] in points, 1–12 columns".into());
    let a = v.as_array().filter(|a| (1..=12).contains(&a.len())).ok_or_else(bad)?;
    let mut out = Vec::with_capacity(a.len());
    for (i, c) in a.iter().enumerate() {
        let num = |k: usize| c.get(k).and_then(Value::as_f64).map(|x| x as f32).filter(|x| x.is_finite());
        let w = num(0).ok_or_else(bad)?;
        // The last column has no space after it.
        let sp = if i + 1 == a.len() { 0.0 } else { num(1).unwrap_or(0.0) };
        if w < 18.0 || !(0.0..=300.0).contains(&sp) {
            return Err(CmdError::Params("each column must be at least 0.25\" wide, with 0–300 pt after it".into()));
        }
        out.push((w, sp));
    }
    let total: f32 = out.iter().map(|(w, sp)| w + sp).sum();
    if total > tw + 1.0 {
        return Err(CmdError::Params(format!("the columns ({total:.0} pt) are wider than the text ({tw:.0} pt)")));
    }
    Ok(out)
}

fn breaks(s: &mut Session, v: &Value) -> CmdResult {
    let kind = p::str(v, "kind").unwrap_or("page");
    let start = match kind {
        "page" => return super::text::specs().iter().find(|c| c.id == "text.pageBreak").map(|c| (c.run)(s, v)).unwrap_or_else(|| sel_result(s)),
        "column" => return super::text::specs().iter().find(|c| c.id == "text.columnBreak").map(|c| (c.run)(s, v)).unwrap_or_else(|| sel_result(s)),
        "textWrapping" => {
            return super::text::specs().iter().find(|c| c.id == "text.lineBreak").map(|c| (c.run)(s, v)).unwrap_or_else(|| sel_result(s));
        }
        "nextPage" => SectionStart::NextPage,
        "continuous" => SectionStart::Continuous,
        "evenPage" => SectionStart::EvenPage,
        "oddPage" => SectionStart::OddPage,
        x => return Err(CmdError::Params(format!("unknown break `{x}`"))),
    };
    if s.sel.focus.story != kition_doc_doc::StoryRef::Body || s.sel.focus.path.depth() > 0 {
        return Err(CmdError::Failed("section breaks go in the main text".into()));
    }
    // Section break: the paragraph before the caret ends a section with the current props; the
    // new section (after) takes them too and starts as requested.
    let at = super::delete_selection(s)?;
    let cur = sect(s);
    let new = s.doc.split_paragraph(&at)?;
    let ended = at.path.clone();
    let para = s.doc.para_mut(at.story, &ended)?;
    let mut props = cur;
    // The start type belongs to the section that begins after the break.
    let next_block = new.path.last();
    para.section = Some(Box::new(props.clone()));
    props.start = start;
    *s.doc.section_mut(next_block) = props;
    s.sel = Selection::caret(Pos { off: 0, ..new });
    let _ = Block::Para;
    sel_result(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Session {
        Session::new(kition_doc_doc::Document::from_text("one\ntwo\nthree"))
    }

    /// More Columns: unequal widths with a line between, as one undo step.
    #[test]
    fn unequal_columns_with_a_line_between() {
        let mut s = doc();
        let tw = sect(&s).text_width();
        s.run("layout.columns", &json!({"widths": [[150.0, 24.0], [tw - 174.0, 0.0]], "separator": true})).unwrap();
        let c = sect(&s).columns;
        assert_eq!((c.count, c.separator, c.widths.len()), (2, true, 2));
        assert_eq!(c.widths.first(), Some(&(150.0, 24.0)));
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(sect(&s).columns, Columns::default());
        // Too wide, too narrow, too many, or nonsense: refused.
        for bad in [json!([[tw, 36.0], [100.0, 0.0]]), json!([[5.0, 0.0], [100.0, 0.0]]), json!(vec![[40.0, 0.0]; 13]), json!("x")] {
            assert!(s.run("layout.columns", &json!({"widths": bad})).is_err(), "{bad}");
        }
    }

    /// Apply to: the whole document, or from the caret on (a continuous section break first).
    #[test]
    fn columns_apply_to_the_document_or_from_the_caret_on() {
        let mut s = doc();
        s.run("layout.break", &json!({"kind": "nextPage"})).unwrap();
        s.run("layout.columns", &json!({"count": 3, "apply": "document"})).unwrap();
        assert!(s.doc.sections().iter().all(|(_, x)| x.columns.count == 3));
        let mut s = doc();
        s.run("caret.docEnd", &json!({})).unwrap();
        s.run("layout.columns", &json!({"count": 2, "apply": "forward"})).unwrap();
        let counts: Vec<u32> = s.doc.sections().iter().map(|(_, x)| x.columns.count).collect();
        assert_eq!(counts, vec![1, 2]);
        assert_eq!(s.doc.last_section.start, SectionStart::Continuous);
        assert!(s.run("layout.columns", &json!({"count": 2, "apply": "elsewhere"})).is_err());
    }
}
