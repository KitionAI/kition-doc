//! References tab: table of contents, footnotes/endnotes, captions; field updates.

use kition_doc_doc::para::{InlineObject, NoteKind};
use kition_doc_doc::props::{CharProps, NumRef, TabAlign, TabLeader, TabStop};
use kition_doc_doc::{Block, Paragraph, PartKind, Path, Pos, StoryRef, para_block};
use serde_json::{Value, json};

use super::{delete_selection, sel_result};
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("toc.insert", "Table of Contents", "References › Table of Contents", insert_toc)
            .params(r#"{"levels"?: 1-9, "title"?: string}"#),
        CommandSpec::new("toc.update", "Update Table", "References › Table of Contents", |s, _| {
            update_toc(s)?;
            sel_result(s)
        }),
        CommandSpec::new("toc.remove", "Remove Table of Contents", "References › Table of Contents", |s, _| {
            if let Some((start, end)) = toc_range(s) {
                for i in (start..=end).rev() {
                    s.doc.remove_block(StoryRef::Body, &Path::top(i))?;
                }
                s.clamp_selection();
            }
            sel_result(s)
        }),
        CommandSpec::new("toc.addText", "Add Text", "References › Table of Contents", |s, v| {
            let level = p::u64(v, "level").unwrap_or(0).min(9) as u8;
            super::para::fmt(s, &|pp| pp.outline_level = if level == 0 { Some(9) } else { Some(level - 1) })
        })
        .params(r#"{"level": 0 (do not show) | 1-9}"#),
        CommandSpec::new("footnote.insert", "Insert Footnote", "References › Footnotes", |s, v| note(s, v, NoteKind::Footnote))
            .key("Mod+Alt+F")
            .params(r#"{"text"?: string} (with text the caret stays in the body after the reference; without, it goes into the note, and header.close brings it back)"#),
        CommandSpec::new("endnote.insert", "Insert Endnote", "References › Footnotes", |s, v| note(s, v, NoteKind::Endnote))
            .key("Mod+Alt+D")
            .params(r#"{"text"?: string} (with text the caret stays in the body after the reference; without, it goes into the note, and header.close brings it back)"#),
        CommandSpec::new("footnote.next", "Next Footnote", "References › Footnotes", |s, _| {
            let caret = s.sel.focus.clone();
            let refs = note_refs(s);
            if let Some(p) = refs.iter().find(|p| **p > caret).or(refs.first()) {
                s.sel = Selection::caret(p.clone());
            }
            sel_result(s)
        })
        .pure(),
        CommandSpec::new("footnote.show", "Show Notes", "References › Footnotes", |s, _| {
            let mut out = Vec::new();
            for (id, part) in &s.doc.parts {
                if matches!(part.kind, PartKind::Footnote | PartKind::Endnote) {
                    out.push(json!({"id": id, "kind": part.kind, "text": s.doc.plain_text(StoryRef::Part(*id))}));
                }
            }
            Ok(Value::Array(out))
        })
        .pure(),
        CommandSpec::new("caption.insert", "Insert Caption", "References › Captions", caption)
            .params(r#"{"label"?: "Figure|Table|Equation", "text"?: string}"#),
        CommandSpec::new("field.updateAll", "Update Field", "References", |s, _| {
            update_fields(s)?;
            sel_result(s)
        })
        .key("F9"),
    ]
}

/// A heading the table of contents lists: its path, outline level, text and list number.
struct Heading {
    path: Path,
    level: u8,
    text: String,
    /// The number its list gives it ("1.2"), as the page shows it; bullets are left out.
    number: Option<String>,
}

/// Headings (outline level < levels) in the body, with their list numbers: every numbered
/// paragraph counts, in document order, as the page numbers them.
fn headings(s: &Session, levels: u8) -> Vec<Heading> {
    let mut v = Vec::new();
    let mut counters = kition_doc_doc::numbering::Counters::default();
    for path in s.doc.para_paths(StoryRef::Body) {
        let Some(p) = s.doc.para(StoryRef::Body, &path) else { continue };
        let style = p.props.style.as_deref().unwrap_or("Normal");
        if style.starts_with("TOC") && style != "TOCHeading" {
            continue;
        }
        let rp = s.doc.styles.resolve_para(&p.props);
        let number = p
            .props
            .numbering
            .or(rp.numbering)
            .filter(|n| n.num != 0)
            .and_then(|n| counters.next_label(&s.doc.numbering, n.num, n.level))
            .filter(|(_, level)| level.format != kition_doc_doc::section::NumFormat::Bullet)
            .map(|(label, _)| label);
        if path.depth() > 0 || style == "TOCHeading" {
            continue;
        }
        if let Some(level) = rp.outline_level
            && level < levels
        {
            let text = p.plain_text().replace(['\u{000C}', '\n', '\t'], " ").trim().to_string();
            if !text.is_empty() {
                v.push(Heading { path, level, text, number: number.filter(|n| !n.trim().is_empty()) });
            }
        }
    }
    v
}

/// The TOC block range in the body: the paragraph holding the TOC field and the TOC entries after it.
fn toc_range(s: &Session) -> Option<(usize, usize)> {
    let start = s.doc.body.iter().position(|b| {
        b.as_para().is_some_and(|p| {
            p.objects.iter().any(|o| matches!(o, InlineObject::Field { instr, .. } if instr.trim_start().to_ascii_uppercase().starts_with("TOC")))
        })
    })?;
    let mut end = start;
    while let Some(Block::Para(p)) = s.doc.body.get(end + 1).map(|b| &**b) {
        if p.props.style.as_deref().is_some_and(|st| st.starts_with("TOC") && st != "TOCHeading") {
            end += 1;
        } else {
            break;
        }
    }
    Some((start, end))
}

fn toc_levels(s: &Session, start: usize) -> u8 {
    s.doc
        .body
        .get(start)
        .and_then(|b| b.as_para())
        .and_then(|p| {
            p.objects.iter().find_map(|o| match o {
                InlineObject::Field { instr, .. } => instr
                    .split("\\o")
                    .nth(1)
                    .and_then(|r| r.trim().trim_matches('"').split('-').nth(1))
                    .and_then(|n| n.trim_matches('"').parse::<u8>().ok()),
                _ => None,
            })
        })
        .unwrap_or(3)
        .clamp(1, 9)
}

fn insert_toc(s: &mut Session, v: &Value) -> CmdResult {
    if toc_range(s).is_some() {
        update_toc(s)?;
        return sel_result(s);
    }
    let levels = p::u64(v, "levels").unwrap_or(3).clamp(1, 9) as u8;
    let title = p::str(v, "title").unwrap_or("Contents").to_string();
    let at = delete_selection(s)?;
    if at.story != StoryRef::Body || at.path.depth() > 0 {
        return Err(CmdError::Failed("a table of contents goes in the main text".into()));
    }
    // Title paragraph carrying the TOC field marker; never numbered like the Heading 1 its
    // style is based on.
    let mut head = Paragraph::with_text(&title, CharProps::default()).styled("TOCHeading");
    head.props.numbering = Some(NumRef { num: 0, level: 0 });
    head.insert_object(
        head.len(),
        InlineObject::Field {
            instr: format!("TOC \\o \"1-{levels}\" \\h \\z \\u"),
            result: String::new(),
            locked: false,
            code_runs: Vec::new(),
            code_fields: Vec::new(),
        },
        &CharProps::default(),
    )?;
    let i = if s.doc.para_at(&at).is_some_and(|p| p.is_empty()) { at.path.last() } else { s.doc.split_paragraph(&at)?.path.last() };
    s.doc.insert_block(StoryRef::Body, &Path::top(i), Block::Para(head))?;
    update_toc(s)?;
    sel_result(s)
}

/// Regenerate the TOC entries with page numbers (two layout passes: entries change pagination).
pub fn update_toc(s: &mut Session) -> Result<(), CmdError> {
    let Some((start, end)) = toc_range(s) else { return Ok(()) };
    let levels = toc_levels(s, start);
    for i in (start + 1..=end).rev() {
        s.doc.remove_block(StoryRef::Body, &Path::top(i))?;
    }
    let width = super::page::sect(s).text_width();
    let entries = headings(s, levels);
    // A heading typed after a manual page break starts with that break; its page is where the
    // text after the break lands, so look the page up past any leading breaks.
    let leads: Vec<usize> =
        entries.iter().map(|h| s.doc.para(StoryRef::Body, &h.path).map_or(0, |p| p.text.bytes().take_while(|b| *b == 0x0C).count())).collect();
    for pass in 0..2 {
        // Pass 1 lays out with the entries in place, so headings below the TOC sit `entries.len()`
        // blocks further down and their pages account for the TOC's own length.
        s.touch();
        let l = s.layout();
        if pass == 1 {
            // Replace the page numbers now that the entries exist.
            for _ in 0..entries.len() {
                s.doc.remove_block(StoryRef::Body, &Path::top(start + 1))?;
            }
        }
        let shift = if pass == 0 { 0 } else { entries.len() };
        for (k, h) in entries.iter().enumerate() {
            let target = Path::top(h.path.last() + if h.path.last() > start { shift } else { 0 });
            let off = leads.get(k).copied().unwrap_or(0);
            let page = l.caret(&Pos { story: StoryRef::Body, path: target, off }).and_then(|c| l.pages.get(c.page)).map(|pg| pg.number).unwrap_or(1);
            let style = format!("TOC{}", h.level + 1);
            let leader = TabStop { pos: width - 0.5, align: TabAlign::Right, leader: TabLeader::Dot };
            // A numbered heading's entry is "number, tab, text": as Word writes it, the text
            // starts at a left tab past the entry's indent (22 pt at level 1, 11 pt more a level).
            let (line, tabs) = match &h.number {
                Some(number) => {
                    let indent =
                        s.doc.styles.resolve_para(&kition_doc_doc::ParaProps { style: Some(style.clone()), ..Default::default() }).indent_left;
                    let at = indent + 22.0 + 11.0 * f32::from(h.level);
                    (format!("{number}\t{}\t{page}", h.text), vec![TabStop { pos: at, align: TabAlign::Left, leader: TabLeader::None }, leader])
                }
                None => (format!("{}\t{page}", h.text), vec![leader]),
            };
            let mut para = Paragraph::with_text(&line, CharProps::default()).styled(&style);
            para.props.tabs = Some(tabs);
            s.doc.insert_block(StoryRef::Body, &Path::top(start + 1 + k), Block::Para(para))?;
        }
    }
    s.touch();
    Ok(())
}

fn note(s: &mut Session, v: &Value, kind: NoteKind) -> CmdResult {
    let text = p::str(v, "text").unwrap_or("").to_string();
    let (pk, style, refstyle) = match kind {
        NoteKind::Footnote => (PartKind::Footnote, "FootnoteText", "FootnoteReference"),
        NoteKind::Endnote => (PartKind::Endnote, "EndnoteText", "EndnoteReference"),
    };
    let mut para = Paragraph::new().styled(style);
    let id = s.doc.add_part(pk, Vec::new());
    para.insert_object(
        0,
        InlineObject::NoteRef { kind, id, custom: String::new() },
        &CharProps { style: Some(refstyle.into()), ..Default::default() },
    )?;
    let len = para.len();
    para.insert_text(len, &format!(" {text}"), &CharProps::default())?;
    s.doc.set_story(StoryRef::Part(id), vec![para_block(para)])?;
    let at = delete_selection(s)?;
    let after = s.doc.insert_object(
        &at,
        InlineObject::NoteRef { kind, id, custom: String::new() },
        &CharProps { style: Some(refstyle.into()), ..Default::default() },
    )?;
    if text.is_empty() {
        // Like Word, the caret moves into the new note to write it; leaving the note (Close
        // Header and Footer) comes back to just after its reference.
        s.body_return = Some(after).filter(|p| p.story == StoryRef::Body);
        s.sel = Selection::caret(s.doc.end_of(StoryRef::Part(id)));
    } else {
        // The note is written already: the caret goes on in the text after its reference.
        s.sel = Selection::caret(after);
    }
    Ok(json!({"id": id}))
}

fn note_refs(s: &Session) -> Vec<Pos> {
    let mut v = Vec::new();
    for path in s.doc.para_paths(StoryRef::Body) {
        let Some(p) = s.doc.para(StoryRef::Body, &path) else { continue };
        for o in p.object_offsets() {
            if matches!(p.object_at(o), Some(InlineObject::NoteRef { .. })) {
                v.push(Pos { story: StoryRef::Body, path: path.clone(), off: o });
            }
        }
    }
    v
}

fn caption(s: &mut Session, v: &Value) -> CmdResult {
    let label = p::str(v, "label").unwrap_or("Figure").to_string();
    let text = p::str(v, "text").unwrap_or("").to_string();
    // Number = existing captions with this label before the caret + 1.
    let caret = s.sel.focus.clone();
    let n = s
        .doc
        .para_paths(StoryRef::Body)
        .into_iter()
        .filter(|q| *q < caret.path)
        .filter(|q| s.doc.para(StoryRef::Body, q).is_some_and(|p| p.props.style.as_deref() == Some("Caption") && p.text.starts_with(&label)))
        .count()
        + 1;
    let f = s.sel.focus.clone();
    let len = s.doc.para_at(&f).map(|p| p.len()).unwrap_or(0);
    let at = Pos { off: len, ..f };
    let new = s.doc.split_paragraph(&at)?;
    let para = s.doc.para_mut(new.story, &new.path)?;
    para.props = kition_doc_doc::ParaProps { style: Some("Caption".into()), ..Default::default() };
    para.mark = CharProps::default();
    let base = format!("{label} ");
    para.insert_text(0, &base, &CharProps::default())?;
    para.insert_object(
        base.len(),
        InlineObject::Field {
            instr: format!("SEQ {label} \\* ARABIC"),
            result: n.to_string(),
            locked: false,
            code_runs: Vec::new(),
            code_fields: Vec::new(),
        },
        &CharProps::default(),
    )?;
    let end = para.len();
    if !text.is_empty() {
        para.insert_text(end, &format!(": {text}"), &CharProps::default())?;
    }
    let off = para.len();
    s.sel = Selection::caret(Pos { off, ..new });
    sel_result(s)
}

/// Update fields in the body: dates, SEQ numbering, cross-references, TOC.
pub fn update_fields(s: &mut Session) -> Result<(), CmdError> {
    let mut seq: std::collections::HashMap<String, u32> = Default::default();
    let mut pages: std::collections::HashMap<String, u32> = Default::default();
    for (name, pos) in s.doc.bookmarks() {
        let l = s.layout();
        let page = l.caret(&pos).and_then(|c| l.pages.get(c.page)).map(|p| p.number).unwrap_or(1);
        pages.entry(name).or_insert(page);
    }
    for path in s.doc.para_paths(StoryRef::Body) {
        let Some(p) = s.doc.para(StoryRef::Body, &path) else { continue };
        if !p.objects.iter().any(|o| matches!(o, InlineObject::Field { .. })) {
            continue;
        }
        let mut updates: Vec<(usize, String)> = Vec::new();
        for (k, o) in p.objects.iter().enumerate() {
            let InlineObject::Field { instr, locked: false, .. } = o else { continue };
            // A table formula, computed again from its table's cells (in document order, so a
            // total sees the subtotals above it already updated).
            if instr.trim_start().starts_with('=') {
                if let Some((tp, r, c)) = path.cell()
                    && let Some(t) = s.doc.table(StoryRef::Body, &tp)
                    && let Ok(value) = super::table_formula::evaluate(instr, t, r, c)
                {
                    updates.push((k, value));
                }
                continue;
            }
            let name = kition_doc_layout::fields::field_name(instr);
            match name.as_str() {
                "DATE" | "TIME" | "CREATEDATE" | "SAVEDATE" | "PRINTDATE" => {
                    let pic = date_picture(instr).unwrap_or_else(|| if name == "TIME" { "h:mm am/pm".into() } else { "M/d/yyyy".into() });
                    updates.push((k, super::insert::format_date(&pic)));
                }
                "SEQ" => {
                    let id = instr.split_whitespace().nth(1).unwrap_or("").to_string();
                    let n = seq.entry(id).or_insert(0);
                    *n += 1;
                    updates.push((k, n.to_string()));
                }
                "REF" | "PAGEREF" => {
                    let target = instr.split_whitespace().nth(1).unwrap_or("");
                    let r = if name == "REF" { bookmark_text(&s.doc, target) } else { pages.get(target).map(u32::to_string) };
                    if let Some(r) = r {
                        updates.push((k, r));
                    }
                }
                "TITLE" => updates.push((k, s.doc.core.title.clone())),
                "AUTHOR" => updates.push((k, s.doc.core.creator.clone())),
                "NUMWORDS" => updates.push((k, s.doc.word_count().to_string())),
                _ => {}
            }
        }
        if updates.is_empty() {
            continue;
        }
        let para = s.doc.para_mut(StoryRef::Body, &path)?;
        for (k, r) in updates {
            if let Some(InlineObject::Field { result, .. }) = para.objects.get_mut(k) {
                *result = r;
            }
        }
        para.touch();
    }
    super::citations::update_citations(s)?;
    update_toc(s)
}

/// A field's `\@` date picture: the quoted text after it (switches may follow), or one word.
pub fn date_picture(instr: &str) -> Option<String> {
    let rest = instr.split_once("\\@")?.1.trim_start();
    let pic = match rest.strip_prefix('"') {
        Some(q) => q.split('"').next().unwrap_or(""),
        None => rest.split_whitespace().next().unwrap_or(""),
    };
    (!pic.is_empty()).then(|| pic.to_string())
}

/// The text a bookmark marks (field results included), up to its end in the same paragraph or
/// the paragraph's end. `None` when there is no such bookmark.
pub fn bookmark_text(doc: &kition_doc_doc::Document, name: &str) -> Option<String> {
    let (_, start) = doc.bookmarks().into_iter().find(|(n, _)| n == name)?;
    let para = doc.para(start.story, &start.path)?;
    let end = para
        .object_offsets()
        .into_iter()
        .find(|o| *o > start.off && matches!(para.object_at(*o), Some(InlineObject::BookmarkEnd { name: n }) if n == name))
        .unwrap_or(para.len());
    Some(doc.copy_range(&start, &Pos { off: end, ..start.clone() }).plain_text().trim().to_string())
}
