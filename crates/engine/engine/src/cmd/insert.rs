//! Insert tab: pages, tables, pictures, shapes, links, bookmarks, headers/footers, text, symbols.

use kition_doc_doc::para::{Float, InlineObject, ShapeKind, Wrap};
use kition_doc_doc::props::{Align, CharProps, Rgb, TabAlign, TabStop, TextColor};
use kition_doc_doc::{Block, Paragraph, PartKind, Path, Pos, StoryRef, Table, para_block};
use serde_json::{Value, json};

use super::{delete_selection, sel_result, split_para, type_text};
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("page.insertBreak", "Page Break", "Insert › Pages", |s, _| {
            type_text(s, "\u{000C}")?;
            sel_result(s)
        }),
        CommandSpec::new("page.insertBlank", "Blank Page", "Insert › Pages", |s, _| {
            type_text(s, "\u{000C}")?;
            let at = s.sel.focus.clone();
            let at = split_para(s, &at)?;
            s.doc.insert_text(&at, "\u{000C}", &CharProps::default())?;
            s.sel = Selection::caret(at);
            sel_result(s)
        }),
        CommandSpec::new("page.insertCover", "Cover Page", "Insert › Pages", cover_page).params(r#"{"title"?: string, "subtitle"?: string, "author"?: string, "date"?: string}"#),
        CommandSpec::new("table.insert", "Table", "Insert › Tables", table).params(r#"{"rows": n, "cols": n, "style"?: string}"#),
        CommandSpec::new("image.insert", "Pictures", "Insert › Illustrations", picture).params(r#"{"path"?: string, "data"?: base64, "width"?: pt, "alt"?: string}"#),
        CommandSpec::new("shape.insert", "Shapes", "Insert › Illustrations", shape)
            .params(r#"{"kind": "rectangle|roundedRectangle|ellipse|triangle|diamond|line|arrow|star|heart", "width"?: pt, "height"?: pt, "fill"?: "RRGGBB", "stroke"?: "RRGGBB"}"#),
        CommandSpec::new("textBox.insert", "Text Box", "Insert › Text", text_box)
            .params(r#"{"text"?: string, "width"?: pt, "height"?: pt}"#)
            .when(|s| {
                // As in Word: no text box inside a text box.
                matches!(s.sel.focus.story, StoryRef::Part(id) if s.doc.parts.get(&id).is_some_and(|p| p.kind == PartKind::TextBox))
                    .then_some("text boxes can't go inside a text box")
            }),
        CommandSpec::new("link.insert", "Link", "Insert › Links", link).key("Mod+K").params(r#"{"url": string, "text"?: string}"#),
        CommandSpec::new("link.remove", "Remove Hyperlink", "Insert › Links", |s, _| super::format::apply(s, &|c| {
            c.link = None;
            if c.style.as_deref() == Some("Hyperlink") {
                c.style = None;
            }
        })),
        CommandSpec::new("bookmark.insert", "Bookmark", "Insert › Links", bookmark).params(r#"{"name": string}"#),
        CommandSpec::new("header.insert", "Header", "Insert › Header & Footer", |s, v| header_footer(s, v, true)).params(r#"{"text"?: string, "preset"?: "blank|blankThree|title"}"#),
        CommandSpec::new("footer.insert", "Footer", "Insert › Header & Footer", |s, v| header_footer(s, v, false)).params(r#"{"text"?: string, "preset"?: "blank|blankThree|pageNumber"}"#),
        CommandSpec::new("pageNumber.insert", "Page Number", "Insert › Header & Footer", page_number).params(r#"{"position"?: "top|bottom|current", "align"?: "left|center|right", "format"?: "x of y"}"#),
        CommandSpec::new("header.edit", "Edit Header", "Insert › Header & Footer › Header", |s, _| edit_hf(s, true)),
        CommandSpec::new("footer.edit", "Edit Footer", "Insert › Header & Footer › Footer", |s, _| edit_hf(s, false)),
        // Like Word, back to where the caret was in the body (a note's reference, for a note).
        CommandSpec::new("header.close", "Close Header and Footer", "Header & Footer", |s, _| {
            if s.sel.focus.story != StoryRef::Body {
                let back = s.body_return.clone().filter(|p| p.story == StoryRef::Body).map(|p| s.doc.clamp(&p));
                s.sel = Selection::caret(back.unwrap_or_else(|| s.doc.start_of(StoryRef::Body)));
            }
            sel_result(s)
        })
        .pure(),
        CommandSpec::new("header.remove", "Remove Header", "Insert › Header & Footer › Header", |s, _| {
            s.doc.last_section.headers = Default::default();
            for path in s.doc.para_paths(StoryRef::Body) {
                if s.doc.para(StoryRef::Body, &path).is_some_and(|p| p.section.is_some())
                    && let Ok(p) = s.doc.para_mut(StoryRef::Body, &path)
                    && let Some(sec) = p.section.as_mut()
                {
                    sec.headers = Default::default();
                }
            }
            sel_result(s)
        }),
        CommandSpec::new("footer.remove", "Remove Footer", "Insert › Header & Footer › Footer", |s, _| {
            s.doc.last_section.footers = Default::default();
            sel_result(s)
        }),
        CommandSpec::new("field.insertDateTime", "Date & Time", "Insert › Text", date_time).params(r#"{"format"?: "M/d/yyyy", "update"?: bool}"#),
        CommandSpec::new("symbol.insert", "Symbol", "Insert › Symbols", symbol).params(r#"{"char": string, "font"?: family (the symbol's own font)}"#),
        CommandSpec::new("field.insert", "Field", "Insert › Text › Quick Parts", field).key("Mod+F9").params(r#"{"instr": string, "result"?: string}"#),
        CommandSpec::new("dropCap.set", "Drop Cap", "Insert › Text", |s, v| {
            let lines = p::u64(v, "lines").unwrap_or(3).min(10) as u8;
            super::para::fmt(s, &|p| p.drop_cap = Some(lines))
        })
        .params(r#"{"lines"?: n (0 = none)}"#),
        CommandSpec::new("horizontalLine.insert", "Horizontal Line", "Home › Paragraph › Borders", |s, _| {
            super::para::fmt(s, &|p| {
                p.borders = Some(kition_doc_doc::props::Borders { bottom: Some(kition_doc_doc::props::Border::single(1.5)), ..Default::default() })
            })
        }),
        // Art text: a floating text box without fill or outline, its text large and coloured,
        // optionally bent by a preset text warp (KitDrawing 1.3 DR26), as `drawing.insert` makes it.
        CommandSpec::new("artText.insert", "Art text", "Insert › Text", |s, v| {
            let text = p::str(v, "text").unwrap_or("Your text here").to_string();
            let runs = json!([{ "text": text, "size": 36, "bold": true, "color": { "rgb": "#156082" } }]);
            let mut params = json!({
                "kind": "textBox",
                "frame": { "w": 360, "h": 72 },
                "paragraphs": [{ "align": "center", "runs": runs }],
                "fill": { "none": true },
                "stroke": { "none": true },
            });
            if let Some(w) = v.get("warp") {
                params["warp"] = w.clone();
            }
            s.run("drawing.insert", &params)
        })
        .params(r#"{"text"?: string, "warp"?: preset (textArchUp, textWave1, textInflate…)}"#),
        CommandSpec::new("text.insertFromFile", "Text from File", "Insert › Text › Object", |s, v| {
            let path = p::req_str(v, "path")?;
            let doc = crate::io::open_path(std::path::Path::new(path)).map_err(CmdError::Failed)?;
            let frag = doc.body_fragment();
            let at = delete_selection(s)?;
            let end = s.doc.insert_fragment(&at, &frag)?;
            s.sel = Selection::caret(end);
            sel_result(s)
        })
        .params(r#"{"path": string}"#),
    ]
}

/// Insert a block after the caret's paragraph (splitting it if the caret is mid-paragraph).
fn insert_block_at_caret(s: &mut Session, block: Block) -> Result<Path, CmdError> {
    let at = delete_selection(s)?;
    let len = s.doc.para_at(&at).map(|p| p.len()).unwrap_or(0);
    let insert_at = if at.off == 0 {
        at.path.clone()
    } else {
        let new = s.doc.split_paragraph(&at)?;
        if at.off >= len { at.path.with_last(at.path.last() + 1) } else { new.path }
    };
    s.doc.insert_block(at.story, &insert_at, block)?;
    // Word keeps a paragraph after a table.
    let next = insert_at.with_last(insert_at.last() + 1);
    if s.doc.block(at.story, &next).is_none() {
        s.doc.insert_block(at.story, &next, Block::Para(Paragraph::new()))?;
    }
    // An empty paragraph we inserted before? Remove if the table took its place at the start.
    Ok(insert_at)
}

fn table(s: &mut Session, v: &Value) -> CmdResult {
    let rows = p::u64(v, "rows").unwrap_or(2).clamp(1, 1000) as usize;
    let cols = p::u64(v, "cols").unwrap_or(2).clamp(1, 63) as usize;
    let width = s.doc.sections().first().map(|(_, sp)| sp.text_width()).unwrap_or(468.0);
    let mut t = Table::new(rows, cols, width);
    if let Some(st) = p::str(v, "style") {
        let id = s.doc.styles.find(st).map(|x| x.id.clone()).ok_or_else(|| CmdError::Params(format!("no table style `{st}`")))?;
        t.props.style = Some(id);
    }
    let story = s.sel.focus.story;
    let path = insert_block_at_caret(s, Block::Table(t))?;
    let mut first = path.0.clone();
    first.extend([0, 0, 0]);
    s.sel = Selection::caret(Pos { story, path: Path(first), off: 0 });
    Ok(json!({"table": path.0}))
}

/// Minimal base64 (standard alphabet, padding optional, whitespace ignored).
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut n = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((buf >> n) as u8);
            buf &= (1 << n) - 1;
        }
    }
    Some(out)
}

pub fn base64_encode(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for ch in b.chunks(3) {
        let n =
            (ch.first().copied().unwrap_or(0) as u32) << 16 | (ch.get(1).copied().unwrap_or(0) as u32) << 8 | ch.get(2).copied().unwrap_or(0) as u32;
        for i in 0..4 {
            if i <= ch.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn picture(s: &mut Session, v: &Value) -> CmdResult {
    let bytes = if let Some(d) = p::str(v, "data") {
        base64_decode(d).ok_or_else(|| CmdError::Params("bad base64 data".into()))?
    } else if let Some(path) = p::str(v, "path") {
        // Through the host in a hosted session, by its path in the workspace (KHP A29).
        crate::disk::read_lent(path).map_err(CmdError::Failed)?
    } else {
        s.ui_requests.push(json!({"open": "insertPicture"}));
        return sel_result(s);
    };
    if bytes.len() > 200 << 20 {
        return Err(CmdError::Failed("image is larger than 200 MB".into()));
    }
    let (pw, ph) =
        kition_doc_render::image_size(&bytes).ok_or_else(|| CmdError::Failed("not a supported image (PNG, JPEG, GIF, WebP, BMP)".into()))?;
    let ext = match bytes.get(..4) {
        Some([0x89, b'P', b'N', b'G']) => "png",
        Some([0xFF, 0xD8, ..]) => "jpeg",
        Some([b'G', b'I', b'F', _]) => "gif",
        Some([b'R', b'I', b'F', b'F']) => "webp",
        Some([b'B', b'M', ..]) => "bmp",
        _ => "png",
    };
    let key = s.doc.add_media(bytes, ext);
    // Natural size at 96 ppi, fitted to the text width.
    let max_w = s.doc.sections().first().map(|(_, sp)| sp.text_width()).unwrap_or(468.0);
    let (mut w, mut h) = (pw as f32 * 0.75, ph as f32 * 0.75);
    if let Some(want) = p::f32(v, "width") {
        let k = want.max(4.0) / w.max(1.0);
        w *= k;
        h *= k;
    }
    if w > max_w {
        h *= max_w / w;
        w = max_w;
    }
    let props = s.typing_props();
    let at = delete_selection(s)?;
    let obj = InlineObject::Image {
        edge: Default::default(),
        geom: None,
        media: key.clone(),
        w,
        h,
        alt: p::str(v, "alt").unwrap_or("").to_string(),
        float: Float::default(),
        crop: [0.0; 4],
        ole: None,
        adjust: Default::default(),
        uid: 0,
    };
    let end = s.doc.insert_object(&at, obj, &props)?;
    s.sel = Selection { anchor: at, focus: end };
    Ok(json!({"media": key, "width": w, "height": h}))
}

fn shape(s: &mut Session, v: &Value) -> CmdResult {
    let kind: ShapeKind =
        serde_json::from_value(v.get("kind").cloned().unwrap_or(json!("rectangle"))).map_err(|e| CmdError::Params(e.to_string()))?;
    let w = p::f32(v, "width").unwrap_or(108.0).clamp(4.0, 2000.0);
    let h = p::f32(v, "height").unwrap_or(if kind == ShapeKind::Line { 1.0 } else { 72.0 }).clamp(1.0, 2000.0);
    let fill = match p::str(v, "fill") {
        Some(c) => Rgb::parse(c),
        None if kind == ShapeKind::Line => None,
        None => Some(Rgb(0x15, 0x60, 0x82)),
    };
    let stroke = p::str(v, "stroke").and_then(Rgb::parse).or(Some(Rgb(0x0E, 0x40, 0x5A)));
    let props = s.typing_props();
    let at = delete_selection(s)?;
    let obj = InlineObject::Shape {
        vml: None,
        kind,
        w,
        h,
        fill,
        stroke,
        stroke_width: 1.0,
        float: Float { wrap: Wrap::Inline, ..Default::default() },
        story: None,
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    let end = s.doc.insert_object(&at, obj, &props)?;
    s.sel = Selection { anchor: at, focus: end };
    sel_result(s)
}

fn text_box(s: &mut Session, v: &Value) -> CmdResult {
    let text = p::str(v, "text").unwrap_or("").to_string();
    let w = p::f32(v, "width").unwrap_or(144.0).clamp(18.0, 2000.0);
    let h = p::f32(v, "height").unwrap_or(72.0).clamp(18.0, 2000.0);
    let id = s.doc.add_part(PartKind::TextBox, vec![para_block(Paragraph::with_text(&text, CharProps::default()))]);
    let props = s.typing_props();
    let at = delete_selection(s)?;
    let obj = InlineObject::Shape {
        vml: None,
        kind: ShapeKind::TextBox,
        w,
        h,
        fill: Some(Rgb::WHITE),
        stroke: Some(Rgb::BLACK),
        stroke_width: 0.75,
        float: Float::default(),
        story: Some(id),
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    s.doc.insert_object(&at, obj, &props)?;
    // Like Word, type straight into the new box.
    s.sel = Selection::caret(s.doc.end_of(StoryRef::Part(id)));
    Ok(json!({"story": id}))
}

fn link(s: &mut Session, v: &Value) -> CmdResult {
    let Some(url) = p::str(v, "url").map(str::to_string) else {
        s.ui_requests.push(json!({"open": "link"}));
        return sel_result(s);
    };
    if url.is_empty() || url.len() > 4096 {
        return Err(CmdError::Params("bad url".into()));
    }
    let style = Some("Hyperlink".to_string());
    if s.sel.is_collapsed() {
        let text = p::str(v, "text").unwrap_or(&url).to_string();
        let mut props = s.typing_props();
        props.link = Some(url.clone());
        props.style = style;
        let at = s.sel.focus.clone();
        let end = s.doc.insert_text(&at, &text, &props)?;
        s.sel = Selection::caret(end);
        s.pending = Some(CharProps::default());
    } else {
        let (a, b) = s.sel.ordered();
        s.doc.format_range(&a, &b, &|c| {
            c.link = Some(url.clone());
            c.style = style.clone();
        })?;
    }
    sel_result(s)
}

fn bookmark(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?.trim().to_string();
    if name.is_empty() || name.contains(' ') || name.len() > 40 || name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Err(CmdError::Params("bookmark names start with a letter, have no spaces and at most 40 characters".into()));
    }
    // Replace an existing bookmark of the same name.
    for (_, pos) in s.doc.bookmarks().into_iter().filter(|(n, _)| *n == name).rev() {
        let para = s.doc.para_mut(pos.story, &pos.path)?;
        para.delete(pos.off, pos.off + kition_doc_doc::para::OBJ.len_utf8())?;
        let offs: Vec<usize> = para.object_offsets();
        if let Some(e) = offs.into_iter().find(|o| matches!(para.object_at(*o), Some(InlineObject::BookmarkEnd { name: n }) if *n == name)) {
            para.delete(e, e + kition_doc_doc::para::OBJ.len_utf8())?;
        }
    }
    let (a, b) = s.sel.ordered();
    let props = CharProps::default();
    let end = s.doc.insert_object(&b, InlineObject::BookmarkEnd { name: name.clone() }, &props)?;
    s.doc.insert_object(&a, InlineObject::BookmarkStart { name: name.clone() }, &props)?;
    let _ = end;
    sel_result(s)
}

fn hf_part(s: &mut Session, header: bool) -> u32 {
    let block = s.sel.focus.path.0.first().copied().unwrap_or(0) as usize;
    let sect = s.doc.section_mut(block).clone();
    let existing = if header { sect.headers.default } else { sect.footers.default };
    if let Some(id) = existing.filter(|id| s.doc.parts.contains_key(id)) {
        return id;
    }
    let style = if header { "Header" } else { "Footer" };
    let id = s.doc.add_part(if header { PartKind::Header } else { PartKind::Footer }, vec![para_block(Paragraph::new().styled(style))]);
    let sect = s.doc.section_mut(block);
    if header {
        sect.headers.default = Some(id);
    } else {
        sect.footers.default = Some(id);
    }
    id
}

fn header_footer(s: &mut Session, v: &Value, header: bool) -> CmdResult {
    let id = hf_part(s, header);
    let style = if header { "Header" } else { "Footer" };
    if let Some(text) = p::str(v, "text") {
        let blocks = text.split('\n').map(|l| para_block(Paragraph::with_text(l, CharProps::default()).styled(style))).collect();
        s.doc.set_story(StoryRef::Part(id), blocks)?;
    } else if let Some(preset) = p::str(v, "preset") {
        let mut para = Paragraph::new().styled(style);
        match preset {
            "blankThree" => {
                para.insert_text(0, "[Type here]\t[Type here]\t[Type here]", &CharProps::default())?;
            }
            "title" => {
                para.insert_text(0, if s.doc.core.title.is_empty() { "[Document title]" } else { &s.doc.core.title }, &CharProps::default())?;
                para.props.align = Some(Align::Center);
            }
            "pageNumber" => {
                para.props.align = Some(Align::Center);
                para.insert_object(
                    0,
                    InlineObject::Field { instr: "PAGE".into(), result: "1".into(), locked: false, code_runs: Vec::new(), code_fields: Vec::new() },
                    &CharProps::default(),
                )?;
            }
            _ => {
                para.insert_text(0, "[Type here]", &CharProps::default())?;
            }
        }
        s.doc.set_story(StoryRef::Part(id), vec![para_block(para)])?;
    }
    s.sel = Selection::caret(s.doc.start_of(StoryRef::Part(id)));
    Ok(json!({"story": id}))
}

fn edit_hf(s: &mut Session, header: bool) -> CmdResult {
    let id = hf_part(s, header);
    s.touch();
    s.sel = Selection::caret(s.doc.end_of(StoryRef::Part(id)));
    Ok(json!({"story": id}))
}

fn page_number(s: &mut Session, v: &Value) -> CmdResult {
    let position = p::str(v, "position").unwrap_or("bottom");
    let align_of = |a: &str| match a {
        "left" => Align::Left,
        "right" => Align::Right,
        _ => Align::Center,
    };
    let align = align_of(p::str(v, "align").unwrap_or("center"));
    let x_of_y = p::str(v, "format").is_some_and(|f| f.contains("of"));
    let field = InlineObject::Field { instr: "PAGE".into(), result: "1".into(), locked: false, code_runs: Vec::new(), code_fields: Vec::new() };
    if position == "current" {
        // At the caret, in its formatting: "Page X of Y" with bold numbers like the header and
        // footer's, and the caret's paragraph aligned when `align` is given.
        let props = s.typing_props();
        let at = delete_selection(s)?;
        let mut end = at.clone();
        if x_of_y {
            let bold = CharProps { bold: Some(true), ..props.clone() };
            end = s.doc.insert_text(&end, "Page ", &props)?;
            end = s.doc.insert_object(&end, field, &bold)?;
            end = s.doc.insert_text(&end, " of ", &props)?;
            let pages =
                InlineObject::Field { instr: "NUMPAGES".into(), result: "1".into(), locked: false, code_runs: Vec::new(), code_fields: Vec::new() };
            end = s.doc.insert_object(&end, pages, &bold)?;
        } else {
            end = s.doc.insert_object(&end, field, &props)?;
        }
        if let Some(a) = p::str(v, "align") {
            let para = s.doc.para_mut(end.story, &end.path)?;
            para.props.align = Some(align_of(a));
            para.touch();
        }
        s.sel = Selection::caret(end);
        return sel_result(s);
    }
    let header = position == "top";
    let id = hf_part(s, header);
    let mut para = Paragraph::new().styled(if header { "Header" } else { "Footer" });
    para.props.align = Some(align);
    para.props.tabs = Some(vec![TabStop { pos: 0.0, align: TabAlign::Clear, leader: Default::default() }]);
    if x_of_y {
        para.insert_text(0, "Page  of ", &CharProps::default())?;
        para.insert_object(5, field, &CharProps { bold: Some(true), ..Default::default() })?;
        let end = para.len();
        para.insert_object(
            end,
            InlineObject::Field { instr: "NUMPAGES".into(), result: "1".into(), locked: false, code_runs: Vec::new(), code_fields: Vec::new() },
            &CharProps { bold: Some(true), ..Default::default() },
        )?;
    } else {
        para.insert_object(0, field, &CharProps::default())?;
    }
    s.doc.set_story(StoryRef::Part(id), vec![para_block(para)])?;
    Ok(json!({"story": id}))
}

fn date_time(s: &mut Session, v: &Value) -> CmdResult {
    let fmt = p::str(v, "format").unwrap_or("M/d/yyyy");
    let text = format_date(fmt);
    if p::bool(v, "update").unwrap_or(false) {
        let props = s.typing_props();
        let at = delete_selection(s)?;
        let end = s.doc.insert_object(
            &at,
            InlineObject::Field { instr: format!("DATE \\@ \"{fmt}\""), result: text, locked: false, code_runs: Vec::new(), code_fields: Vec::new() },
            &props,
        )?;
        s.sel = Selection::caret(end);
    } else {
        type_text(s, &text)?;
    }
    sel_result(s)
}

/// Format today's date with a Word picture (`M/d/yyyy`, `MMMM d, yyyy`, `yyyy-MM-dd`, `dddd`…).
pub fn format_date(fmt: &str) -> String {
    let iso = super::now_iso();
    let y: i64 = iso.get(0..4).and_then(|x| x.parse().ok()).unwrap_or(2026);
    let m: usize = iso.get(5..7).and_then(|x| x.parse().ok()).unwrap_or(1);
    let d: u32 = iso.get(8..10).and_then(|x| x.parse().ok()).unwrap_or(1);
    let hh: u32 = iso.get(11..13).and_then(|x| x.parse().ok()).unwrap_or(0);
    let mm: u32 = iso.get(14..16).and_then(|x| x.parse().ok()).unwrap_or(0);
    let ss: u32 = iso.get(17..19).and_then(|x| x.parse().ok()).unwrap_or(0);
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    const DAYS: [&str; 7] = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
    // Days since the epoch, for the weekday.
    let days_since = (super::now_unix() / 86_400) as usize;
    let month = MONTHS.get(m.saturating_sub(1)).copied().unwrap_or("January");
    let day = DAYS.get(days_since % 7).copied().unwrap_or("Monday");
    let mut out = String::new();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars.get(i).copied().unwrap_or(' ');
        // `am/pm` (any case) is one token: AM or PM.
        if chars.get(i..i + 5).is_some_and(|w| w.iter().collect::<String>().eq_ignore_ascii_case("am/pm")) {
            out.push_str(if hh < 12 { "AM" } else { "PM" });
            i += 5;
            continue;
        }
        let mut n = 1;
        while chars.get(i + n) == Some(&c) {
            n += 1;
        }
        match (c, n) {
            ('y', 4..) => out.push_str(&format!("{y:04}")),
            ('y', _) => out.push_str(&format!("{:02}", y % 100)),
            ('M', 4..) => out.push_str(month),
            ('M', 3) => out.push_str(month.get(..3).unwrap_or(month)),
            ('M', 2) => out.push_str(&format!("{m:02}")),
            ('M', 1) => out.push_str(&m.to_string()),
            ('d', 4..) => out.push_str(day),
            ('d', 3) => out.push_str(day.get(..3).unwrap_or(day)),
            ('d', 2) => out.push_str(&format!("{d:02}")),
            ('d', 1) => out.push_str(&d.to_string()),
            ('H', 2..) => out.push_str(&format!("{hh:02}")),
            ('H', _) => out.push_str(&hh.to_string()),
            ('h', _) => out.push_str(&(if hh.is_multiple_of(12) { 12 } else { hh % 12 }).to_string()),
            ('m', 2..) => out.push_str(&format!("{mm:02}")),
            ('m', _) => out.push_str(&mm.to_string()),
            ('s', 2..) => out.push_str(&format!("{ss:02}")),
            ('s', _) => out.push_str(&ss.to_string()),
            ('a' | 'A', _) => out.push_str(if hh < 12 { "AM" } else { "PM" }),
            _ => {
                for _ in 0..n {
                    out.push(c);
                }
            }
        }
        i += n;
    }
    out
}

/// Insert › Symbol: the character at the caret, in its own font when `font` names one (the rest of
/// the text keeps the caret's font).
fn symbol(s: &mut Session, v: &Value) -> CmdResult {
    let c = p::req_str(v, "char")?;
    if c.is_empty() || c.chars().count() > 64 {
        return Err(CmdError::Params("char: 1–64 characters".into()));
    }
    let font = p::str(v, "font").map(str::trim).filter(|f| !f.is_empty());
    let Some(font) = font else {
        type_text(s, c)?;
        return sel_result(s);
    };
    if font.len() > 256 {
        return Err(CmdError::Params("font: a family name".into()));
    }
    let base = s.typing_props();
    let mut props = base.clone();
    props.font = Some(font.to_string());
    s.pending = Some(props);
    let r = type_text(s, c);
    // Typing on after the symbol goes on in the text's own font.
    s.pending = Some(base);
    r?;
    sel_result(s)
}

fn field(s: &mut Session, v: &Value) -> CmdResult {
    let instr = p::req_str(v, "instr")?.trim().to_string();
    if instr.is_empty() || instr.len() > 4096 {
        return Err(CmdError::Params("bad field code".into()));
    }
    let result = p::str(v, "result").unwrap_or("").to_string();
    let props = s.typing_props();
    let at = delete_selection(s)?;
    let end =
        s.doc.insert_object(&at, InlineObject::Field { instr, result, locked: false, code_runs: Vec::new(), code_fields: Vec::new() }, &props)?;
    s.sel = Selection::caret(end);
    let _ = super::references::update_fields(s);
    sel_result(s)
}

/// Insert › Cover Page: one page of its own paragraphs before the document, with the title (the
/// document's Title property, else a prompt), subtitle (its Subject), author (its Author, else the
/// user) and, when given, a date; the page ends with a page break. The cover's paragraphs are new
/// blocks, so they never take on the paragraph the caret is in, and their spacing is set from the
/// first section's page so the cover fits on its page at any page size. The caret stays where it
/// was, as in Word.
fn cover_page(s: &mut Session, v: &Value) -> CmdResult {
    let or = |key: &str, own: &str, prompt: &str| {
        p::str(v, key).map(str::to_owned).unwrap_or_else(|| if own.is_empty() { prompt.to_owned() } else { own.to_owned() })
    };
    let title = or("title", &s.doc.core.title, "Document Title");
    let subtitle = or("subtitle", &s.doc.core.subject, "Document subtitle");
    let author = or("author", &s.doc.core.creator, &s.author);
    let date = p::str(v, "date").filter(|d| !d.is_empty());
    let (page_h, top, bottom) = s.doc.sections().first().map(|(_, sp)| (sp.page_h, sp.margin_top, sp.margin_bottom)).unwrap_or((792.0, 72.0, 72.0));
    // The room between the margins; the title starts a fifth of the way down and the author's
    // lines sit two fifths below it, leaving the rest for the text and its spacing.
    let room = (page_h - top - bottom).max(72.0);
    let accent = Rgb(0x15, 0x60, 0x82);
    let spaced = |mut p: Paragraph, before: f32| {
        p.props.space_before = Some(before);
        p.props.keep_next = Some(true);
        p
    };
    // 44 pt on a Letter or A4 page, smaller on small pages.
    let title_size = (room / 14.0).clamp(24.0, 44.0).round();
    let mut t = spaced(
        Paragraph::with_text(&title, CharProps { size: Some(title_size), color: Some(TextColor::Rgb(accent)), ..Default::default() }).styled("Title"),
        (room * 0.2).round(),
    );
    t.props.borders = Some(kition_doc_doc::props::Borders {
        bottom: Some(kition_doc_doc::props::Border {
            style: kition_doc_doc::props::BorderStyle::Single,
            width: 2.0,
            color: Some(accent),
            space: 6.0,
            theme: None,
        }),
        ..Default::default()
    });
    let mut blocks = vec![t, Paragraph::with_text(&subtitle, CharProps::default()).styled("Subtitle")];
    blocks.push(spaced(
        Paragraph::with_text(&author, CharProps { bold: Some(true), color: Some(TextColor::Rgb(accent)), ..Default::default() }),
        (room * 0.4).round(),
    ));
    if let Some(date) = date {
        blocks.push(Paragraph::with_text(date, CharProps::default()));
    }
    if let Some(last) = blocks.last_mut() {
        last.props.keep_next = None;
        let end = last.len();
        last.insert_text(end, "\u{000C}", &CharProps::default())?;
    }
    let n = blocks.len() as u32;
    for (i, block) in blocks.into_iter().enumerate() {
        s.doc.insert_block(StoryRef::Body, &Path::top(i), Block::Para(block))?;
    }
    // The caret and selection stay on the text they were on, now `n` blocks further down.
    for pos in [&mut s.sel.anchor, &mut s.sel.focus] {
        if pos.story == StoryRef::Body
            && let Some(first) = pos.path.0.first_mut()
        {
            *first = first.saturating_add(n);
        }
    }
    sel_result(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trip() {
        let data = b"hello world!?";
        assert_eq!(base64_decode(&base64_encode(data)).unwrap(), data);
        assert!(base64_decode("!!!").is_none());
    }

    /// Insert › Symbol with a font: only the symbol takes it, the caret's font carries on after.
    #[test]
    fn symbol_takes_its_own_font() {
        let mut s = Session::new(kition_doc_doc::Document::from_text("ab"));
        s.run("caret.docEnd", &json!({})).unwrap();
        s.run("symbol.insert", &json!({"char": "\u{2665}", "font": "DejaVu Sans"})).unwrap();
        s.run("text.insert", &json!({"text": "c"})).unwrap();
        let p = s.doc.para(StoryRef::Body, &Path::top(0)).unwrap();
        assert_eq!(p.text, "ab\u{2665}c");
        assert_eq!(p.props_of_char(2).font.as_deref(), Some("DejaVu Sans"), "the symbol");
        assert_eq!(p.props_of_char(5).font, None, "typing after it (byte offsets)");
        assert!(s.run("symbol.insert", &json!({"char": ""})).is_err());
    }

    #[test]
    fn date_pictures() {
        let d = format_date("yyyy-MM-dd");
        assert_eq!(d.len(), 10);
        assert!(format_date("MMMM d, yyyy").contains(", "));
        let t = format_date("h:mm:ss am/pm");
        assert!(t.ends_with(" AM") || t.ends_with(" PM"), "{t}");
        assert_eq!(t.matches(':').count(), 2, "{t}");
        use super::super::references::date_picture;
        assert_eq!(date_picture(r#"DATE \@ "MMMM d, yyyy" \* MERGEFORMAT"#).as_deref(), Some("MMMM d, yyyy"));
        assert_eq!(date_picture(r#"TIME \@ HH:mm"#).as_deref(), Some("HH:mm"));
        assert_eq!(date_picture("DATE"), None);
    }
}
