use super::*;
use kition_kit::testing::check_contract;
use kition_kit::tools::PointerAction;

#[test]
fn page_margin_guides_follow_layout_without_entering_document_pixels() {
    let mut doc = DocDocument::create("Margins.docx", 0, 0).unwrap();
    let color = [200, 196, 190, 255];
    for params in [json!({ "preset": "normal" }), json!({ "top": 50, "bottom": 60, "left": 80, "right": 40 })] {
        doc.execute("page.margins", params).unwrap();
        let before = doc.revision();
        let layout = doc.layout();
        let area = DocDocument::areas(&layout)[0];
        let body = layout.pages[0].body;
        let x = area.x + f64::from(body.x);
        let y = area.y + f64::from(body.y);
        let guides: Vec<_> =
            doc.overlays().into_iter().filter(|overlay| matches!(overlay, ToolOverlay::Line { color: c, .. } if *c == color)).collect();
        assert_eq!(guides.len(), 8, "four corners have two strokes each");
        assert!(guides.iter().any(
            |line| matches!(line, ToolOverlay::Line { ax, ay, bx, by, dashed: false, .. } if *ax == x - 18.0 && *ay == y && *bx == x && *by == y)
        ));
        assert!(guides.iter().any(
            |line| matches!(line, ToolOverlay::Line { ax, ay, bx, by, dashed: false, .. } if *ax == x && *ay == y - 18.0 && *bx == x && *by == y)
        ));
        let pixel = doc.render(x - 9.0, y, 1.0, 1, 1).unwrap();
        assert_eq!(pixel, [255, 255, 255, 255], "margin guides are editor overlays only");
        assert_eq!(doc.revision(), before, "showing guides never edits the document");
    }
}

/// A document with one paragraph of text.
fn fixture() -> Vec<u8> {
    let mut doc = DocDocument::create("Letter.docx", 0, 0).unwrap();
    KitTools::text(&mut doc, "Dear reader, this is a test.", 1.0).unwrap();
    doc.save(Some("Letter.docx")).unwrap()
}

#[test]
fn keeps_the_kit_contract() {
    check_contract::<DocDocument>(&fixture(), "Letter.docx", |doc| doc.execute("text.insert", json!({ "text": " More text." })).map(|_| ())).unwrap();
}

fn event(action: PointerAction, x: f64, y: f64, shift: bool) -> ToolPointer {
    ToolPointer { action, x, y, mods: ToolMods { shift, ..ToolMods::default() }, pressure: 1.0, zoom: 1.0 }
}

fn click(doc: &mut DocDocument, x: f64, y: f64, shift: bool) -> ToolResponse {
    doc.pointer(&event(PointerAction::Down, x, y, shift)).unwrap();
    doc.pointer(&event(PointerAction::Up, x, y, shift)).unwrap()
}

fn text(doc: &DocDocument) -> String {
    doc.session.borrow().doc.plain_text(kition_doc_engine::doc::StoryRef::Body)
}

/// Where the first line of text is on the canvas: the caret at the start of the document.
fn first_line(doc: &mut DocDocument) -> (f64, f64, f64) {
    doc.execute("caret.docStart", json!({})).unwrap();
    let caret = doc.state(0.0, 0.0, ToolMods::default(), 1.0);
    let Some(ToolOverlay::Line { ax, ay, by, .. }) = caret.overlays.first().cloned() else { panic!("no caret: {:?}", caret.overlays) };
    (ax, ay, by)
}

#[test]
fn stacks_pages_and_draws_text_on_paper() {
    let doc = DocDocument::open("Letter.docx", &fixture()).unwrap();
    assert_eq!(doc.width(), 612 + 80);
    // White paper, the pasteboard around it, and dark text near the top-left margin.
    let paper = doc.render(300.0, 500.0, 1.0, 1, 1).unwrap();
    assert_eq!(paper, [255, 255, 255, 255]);
    assert_eq!(doc.render(10.0, 10.0, 1.0, 1, 1).unwrap(), PASTEBOARD);
    let top = doc.render(40.0, 40.0, 1.0, 612, 200).unwrap();
    assert!(top.as_chunks::<4>().0.iter().any(|px| px[0] < 100), "the text is drawn");
    // Zoomed in, the page is drawn at that scale with the same paper.
    let sharp = doc.render(299.5, 499.5, 3.0, 4, 4).unwrap();
    assert_eq!(&sharp[..4], &[255, 255, 255, 255]);
}

#[test]
fn clicks_place_the_caret_and_typing_and_keys_edit() {
    let mut doc = DocDocument::open("Letter.docx", &fixture()).unwrap();
    assert!(!doc.dirty());
    let (x, top, bottom) = first_line(&mut doc);
    let middle = (top + bottom) / 2.0;
    // A click at the start of the line, then typing there.
    let response = click(&mut doc, x, middle, false);
    assert!(response.wants_text && response.busy);
    KitTools::text(&mut doc, "Hi! ", 1.0).unwrap();
    assert!(text(&doc).starts_with("Hi! Dear reader"), "{}", text(&doc));
    assert!(doc.dirty());
    // Backspace through the engine's own shortcut table.
    doc.key("Backspace", ToolMods::default(), 1.0).unwrap();
    assert!(text(&doc).starts_with("Hi!Dear"), "{}", text(&doc));
    // Shift+End selects to the end of the line; the selection is drawn.
    let selected = doc.key("End", ToolMods { shift: true, ..ToolMods::default() }, 1.0).unwrap();
    assert!(selected.overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Highlight { .. })));
    assert!(doc.inspect().unwrap()["selected"].as_u64().unwrap_or_default() > 10);
    // Cmd+Z undoes.
    doc.key("z", ToolMods { command: true, ..ToolMods::default() }, 1.0).unwrap();
    assert!(text(&doc).starts_with("Hi! Dear"), "{}", text(&doc));
}

/// Escape cancels first (KitPointer 4.4): `inspect` says whether it has something to cancel, so
/// the editor leaves an Escape with nothing to cancel to the host (in Focus, it leaves Focus).
#[test]
fn inspect_says_when_escape_has_something_to_cancel() {
    let mut doc = DocDocument::open("Letter.docx", &fixture()).unwrap();
    let escape = |doc: &DocDocument| doc.inspect().unwrap()["escape"].as_bool();
    assert_eq!(escape(&doc), Some(false), "a caret alone has nothing to cancel");
    doc.key("End", ToolMods { shift: true, ..ToolMods::default() }, 1.0).unwrap();
    assert_eq!(escape(&doc), Some(true), "a selection");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(escape(&doc), Some(false), "Escape collapsed the selection");
    doc.execute("ink.shape", json!({ "preset": "rect" })).unwrap();
    assert_eq!(escape(&doc), Some(true), "a shape chosen to draw");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(escape(&doc), Some(false), "Escape put the shape down");
}

#[test]
fn a_drag_selects_and_double_click_selects_a_word() {
    let mut doc = DocDocument::open("Letter.docx", &fixture()).unwrap();
    let (x, top, bottom) = first_line(&mut doc);
    let middle = (top + bottom) / 2.0;
    doc.pointer(&event(PointerAction::Down, x, middle, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, x + 60.0, middle, false)).unwrap();
    doc.pointer(&event(PointerAction::Up, x + 60.0, middle, false)).unwrap();
    assert!(doc.inspect().unwrap()["selected"].as_u64().unwrap_or_default() >= 5);
    assert!(!doc.dirty(), "selecting changes nothing");
    doc.pointer(&event(PointerAction::DoubleClick, x + 5.0, middle, false)).unwrap();
    assert_eq!(doc.session.borrow().selected_text().trim(), "Dear");
}

#[test]
fn creates_an_empty_document_and_saves_docx() {
    let doc = DocDocument::create("New.docx", 0, 0).unwrap();
    assert!(!doc.dirty());
    assert_eq!(doc.inspect().unwrap()["pages"].as_array().map(Vec::len), Some(1));
    assert_eq!(&doc.save(None).unwrap()[..2], b"PK");
    assert!(doc.commands().iter().all(|command| command.menu.as_ref().is_none_or(|menu| menu[0] != "Editing")));
}

#[test]
fn paints_the_backdrop_and_reports_where_the_caret_is() {
    let mut doc = DocDocument::open("Letter.docx", &fixture()).unwrap();
    doc.set_backdrop("#f6f5f4").unwrap();
    assert!(doc.set_backdrop("green").is_err());
    // The corner of the canvas is outside every page.
    let corner = doc.render(0.0, 0.0, 1.0, 4, 4).unwrap();
    assert_eq!(&corner[..4], &[0xf6, 0xf5, 0xf4, 255]);

    let inspected = doc.inspect().unwrap();
    assert_eq!(inspected["story"], json!("body"));
    assert_eq!(inspected["equation"], json!(false));
    doc.execute("header.edit", json!({})).unwrap();
    assert_eq!(doc.inspect().unwrap()["story"], json!("headerFooter"));
}

#[test]
fn takes_a_font_for_text_its_own_faces_do_not_cover() {
    assert_eq!(add_font(b"not a font"), 0);
    let fonts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../kition-fonts/fonts/droid-sans-fallback/DroidSansFallbackFull.ttf");
    let Ok(bytes) = std::fs::read(&fonts) else { return };
    // Idempotent either way: a native build with `KITION_FONTS_DIR` set already embeds this exact
    // face, so `add_font` correctly reports 0 new faces added (deduplicated by family and style);
    // a web build embeds none, so it reports faces added. Either way, the face resolves afterward.
    add_font(&bytes);
    // Other tests in this process may have loaded an installed Chinese face (PingFang SC for a
    // missing Latin family) that the fallback search prefers, so look the added face up by name.
    let db = kition_doc_fonts::FontDb::global();
    let added = db.face("Droid Sans Fallback", "Regular");
    assert!(added.named("Droid Sans Fallback") && added.covers('极'));
    assert!(db.fallback_for('极', u32::MAX).is_some_and(|face| face.covers('极')));
}

#[test]
fn the_catalog_flags_as_mutating_exactly_what_changes_the_document() {
    let doc = DocDocument::create("Flags.docx", 0, 0).unwrap();
    let mutates = |id: &str| doc.commands().into_iter().find(|command| command.id == id).unwrap_or_else(|| panic!("no {id}")).mutates;
    // `file.compare` and `file.combine` write the comparison into this document as tracked changes.
    for id in
        ["text.insert", "edit.paste", "edit.undo", "edit.redo", "file.new", "file.open", "file.save", "header.edit", "file.compare", "file.combine"]
    {
        assert!(mutates(id), "{id} changes the document");
    }
    // The reads the command naming standard moved into `file`: a read-only session runs them.
    for id in [
        "edit.selectAll",
        "edit.copy",
        "file.exportPdf",
        "file.print",
        "file.inspect",
        "file.text",
        "file.paragraph",
        "file.selection",
        "file.layout",
    ] {
        assert!(!mutates(id), "{id} does not change the document");
    }
}

#[test]
fn commands_flagged_as_not_mutating_never_commit_to_the_replica() {
    let bytes = fixture();
    let commands = DocDocument::open("Letter.docx", &bytes).unwrap().commands();
    // `file` commands and commands that take a path are left out: the test must not touch files.
    for command in commands.iter().filter(|command| {
        !command.mutates
            && kition_doc_engine::cmd::family::family(&command.id) != "file"
            && !command.params.as_deref().unwrap_or_default().contains("path")
    }) {
        let mut doc = DocDocument::open("Letter.docx", &bytes).unwrap();
        doc.execute("edit.selectAll", json!({})).unwrap();
        let before = doc.version().unwrap();
        let _ = doc.execute(&command.id, json!({}));
        assert_eq!(doc.version().unwrap(), before, "{} committed to the replica", command.id);
    }
}

/// Pages are drawn as printed (thumbnails, agents' screenshots): headers and footers at full
/// strength and no squiggles; the editor turns on its editing look.
#[test]
fn renders_pages_as_printed_unless_the_editor_asks_for_its_editing_look() {
    let mut doc = DocDocument::create("Look.docx", 0, 0).unwrap();
    doc.execute("header.edit", json!({})).unwrap();
    doc.execute("text.insert", json!({ "text": "HEADER HEADER HEADER" })).unwrap();
    doc.execute("header.close", json!({})).unwrap();
    doc.execute("caret.docStart", json!({})).unwrap();
    doc.execute("text.insert", json!({ "text": "Thiss sentense has mispelled wurds." })).unwrap();
    let layout = doc.layout();
    let area = DocDocument::areas(&layout)[0];
    let page = |doc: &DocDocument| {
        doc.render_area(area.x as i32, area.y as i32, (area.x + area.w) as i32, (area.y + area.h) as i32, area.w as u32, area.h as u32).unwrap()
    };
    // The darkest pixel above the body (the header) and whether any pixel is squiggle red.
    let header_ink = |pixels: &[u8]| pixels.as_chunks::<4>().0.iter().take(area.w as usize * 72).map(|px| px[0].min(px[1]).min(px[2])).min().unwrap();
    let red = |pixels: &[u8]| pixels.as_chunks::<4>().0.iter().any(|px| u16::from(px[0]) > u16::from(px[1]) + 50 && px[1] == px[2]);
    let printed = page(&doc);
    assert!(header_ink(&printed) < 60, "the header is drawn at full strength: {}", header_ink(&printed));
    assert!(!red(&printed), "no spelling squiggles as printed");
    doc.set_editing(true);
    let editing = page(&doc);
    assert!(header_ink(&editing) > 100, "the editor dims the header while the body is edited: {}", header_ink(&editing));
    assert!(red(&editing), "the editor shows spelling squiggles");
}

/// Editor chrome takes Kition Doc's brand (`ui.brand.light`, #2b5cd9), not the host's purple: the
/// text selection and a selected shape's frame.
#[test]
fn the_selection_and_object_frames_take_the_brand() {
    let mut doc = DocDocument::open("Letter.docx", &fixture()).unwrap();
    doc.execute("caret.docStart", json!({})).unwrap();
    let selected = doc.key("End", ToolMods { shift: true, ..ToolMods::default() }, 1.0).unwrap();
    let colors: Vec<_> =
        selected.overlays.iter().filter_map(|o| if let ToolOverlay::Highlight { color, .. } = o { Some(*color) } else { None }).collect();
    assert!(!colors.is_empty());
    assert!(colors.iter().all(|c| *c == [0x2b, 0x5c, 0xd9, 64]), "{colors:?}");
    doc.execute("select.collapse", json!({ "end": true })).unwrap();
    doc.execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "x": 200, "y": 300, "w": 120, "h": 60 } })).unwrap();
    let area = DocDocument::areas(&doc.layout())[0];
    let response = click(&mut doc, area.x + 260.0, area.y + 330.0, false);
    let frames: Vec<_> = response.overlays.iter().filter_map(|o| if let ToolOverlay::Path { color, .. } = o { Some(*color) } else { None }).collect();
    assert!(!frames.is_empty(), "the shape is selected: {:?}", response.overlays);
    assert!(frames.iter().all(|c| c[..3] == [0x2b, 0x5c, 0xd9]), "{frames:?}");
}

/// Over the text the pointer is the I-beam; the caret spans the text of its line (the glyphs'
/// ink lies within it), and a drag that does not move the caret leaves the revision alone.
#[test]
fn the_pointer_is_the_text_cursor_the_caret_lines_up_with_the_text_and_drags_do_not_repaint() {
    let mut doc = DocDocument::open("Letter.docx", &fixture()).unwrap();
    let (x, top, bottom) = first_line(&mut doc);
    let middle = (top + bottom) / 2.0;
    let over = doc.pointer(&event(PointerAction::Move, x + 20.0, middle, false)).unwrap();
    assert_eq!(over.cursor, "text");
    // The ink of the first characters ("Dear") lies between the caret's ends.
    let (x0, y0, w, h) = (x.floor(), (top - 20.0).floor(), 30u32, (bottom - top + 40.0).ceil() as u32);
    let pixels = doc.render(x0, y0, 1.0, w, h).unwrap();
    let rows: Vec<f64> =
        (0..h).filter(|r| (0..w).any(|c| pixels.get(((r * w + c) * 4) as usize).is_some_and(|v| *v < 128))).map(|r| y0 + f64::from(r)).collect();
    let (ink_top, ink_bottom) = (rows.first().copied().unwrap(), rows.last().copied().unwrap() + 1.0);
    assert!(top <= ink_top + 1.0 && bottom >= ink_bottom - 1.0, "caret {top}..{bottom}, ink {ink_top}..{ink_bottom}");
    assert!(bottom - top < (ink_bottom - ink_top) * 2.0, "the caret is the text's height, not the box's");
    // A drag selects; a second report at the same character changes nothing.
    doc.pointer(&event(PointerAction::Down, x, middle, false)).unwrap();
    let held = doc.pointer(&event(PointerAction::Drag, x + 40.0, middle, false)).unwrap();
    assert_eq!(held.cursor, "text");
    let before = doc.revision();
    doc.pointer(&event(PointerAction::Drag, x + 40.2, middle, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, x + 80.0, middle, false)).unwrap();
    assert_eq!(doc.revision(), before, "selecting never bumps the revision; the overlays follow the caret");
    doc.pointer(&event(PointerAction::Up, x + 80.0, middle, false)).unwrap();
}

/// A document of `count` short paragraphs, opened in the editor.
fn paragraphs(count: usize) -> DocDocument {
    let mut doc = DocDocument::create("Long.docx", 0, 0).unwrap();
    for index in 0..count {
        if index > 0 {
            doc.execute("text.newParagraph", json!({})).unwrap();
        }
        doc.execute("text.insert", json!({ "text": format!("Paragraph {} of the stage 0B test, long enough to read.", index + 1) })).unwrap();
    }
    doc.execute("caret.docStart", json!({})).unwrap();
    doc
}

/// The caret the engine reports in `inspect()` (stage 0B, R3): where it is on the canvas, the same
/// place the drawn caret is, for the selection's moving end too; it changes no revision.
#[test]
fn inspect_reports_where_the_caret_is_on_the_canvas() {
    let mut doc = paragraphs(80);
    doc.execute(
        "select.range",
        json!({ "anchor": { "story": "body", "path": [3], "off": 0 }, "focus": { "story": "body", "path": [70], "off": 4 } }),
    )
    .unwrap();
    let before = doc.revision();
    let info = doc.inspect().unwrap();
    let caret = &info["caret"];
    assert_eq!(caret["page"], info["page"], "{info}");
    assert!(caret["page"].as_u64().unwrap() >= 1, "the moving end is on a later page: {caret}");
    doc.execute("select.collapse", json!({ "end": true })).unwrap();
    let drawn = doc.state(0.0, 0.0, ToolMods::default(), 1.0);
    let Some(ToolOverlay::Line { ax, ay, by, .. }) = drawn.overlays.first().cloned() else { panic!("no caret: {:?}", drawn.overlays) };
    let caret = doc.inspect().unwrap()["caret"].clone();
    assert!((caret["x"].as_f64().unwrap() - ax).abs() < 0.01 && (caret["top"].as_f64().unwrap() - ay).abs() < 0.01, "{caret} vs ({ax}, {ay})");
    assert!((caret["height"].as_f64().unwrap() - (by - ay)).abs() < 0.01);
    assert_eq!(doc.revision(), before, "reporting the caret is no edit");
}

/// The keys of a Mac and of the others (stage 0B, R5; WPS W-D3–W-D5): the editor says which, and
/// the same key runs the command of that keyboard.
#[test]
fn the_keys_follow_the_platform_the_editor_names() {
    let command = ToolMods { command: true, ..ToolMods::default() };
    let alt = ToolMods { alt: true, ..ToolMods::default() };
    let shift_command = ToolMods { command: true, shift: true, ..ToolMods::default() };
    let at = |doc: &DocDocument| doc.session.borrow().sel.focus.off;
    let mut doc = DocDocument::open("Letter.docx", &fixture()).unwrap();
    let len = text(&doc).len();
    // Elsewhere Ctrl+Right is a word on, Alt+Right too.
    doc.execute("caret.docStart", json!({})).unwrap();
    doc.key("ArrowRight", command, 1.0).unwrap();
    assert_eq!(at(&doc), "Dear ".len(), "Ctrl+Right: a word");
    doc.key("ArrowRight", alt, 1.0).unwrap();
    assert_eq!(at(&doc), "Dear reader".len(), "Alt+Right: a word (to its end, before the comma)");
    // On a Mac Cmd+Right is the end of the line, Cmd+Left its start, Option+Right a word.
    doc.set_mac(true);
    doc.execute("caret.docStart", json!({})).unwrap();
    doc.key("ArrowRight", command, 1.0).unwrap();
    assert_eq!(at(&doc), len, "Cmd+Right: the end of the line");
    doc.key("ArrowLeft", command, 1.0).unwrap();
    assert_eq!(at(&doc), 0, "Cmd+Left: its start");
    doc.key("ArrowRight", alt, 1.0).unwrap();
    assert_eq!(at(&doc), "Dear ".len(), "Option+Right: a word");
    doc.key("ArrowRight", shift_command, 1.0).unwrap();
    assert_eq!(doc.inspect().unwrap()["selected"], json!(len - "Dear ".len()), "Shift+Cmd+Right selects to the end of the line");
    // The catalogue shows each command one key, the platform's first.
    let shortcut = |doc: &DocDocument, id: &str| doc.commands().into_iter().find(|c| c.id == id).and_then(|c| c.shortcut);
    assert_eq!(shortcut(&doc, "edit.redo").as_deref(), Some("Cmd+Shift+Z"));
    assert_eq!(shortcut(&doc, "caret.home").as_deref(), Some("Home"));
    assert_eq!(shortcut(&doc, "format.increaseFontSize").as_deref(), Some("Cmd+]"));
    assert_eq!(shortcut(&doc, "text.optionalHyphen"), None, "Cmd+- types nothing");
    doc.set_mac(false);
    assert_eq!(shortcut(&doc, "edit.redo").as_deref(), Some("Cmd+Y"));
    assert!(doc.commands().iter().filter_map(|c| c.shortcut.as_deref()).all(|key| !key.contains(" / ")), "no `A / B` reaches the frame");
}

/// Page Down and Page Up by a screen (stage 0B, C7; WPS W-D7–W-D9): the caret goes about that far
/// and keeps its place across the line, Shift selects; without a distance it goes to the start of
/// the next page (Cmd+Page Down, W-D8). No revision changes.
#[test]
fn page_down_goes_a_screen_keeping_the_place_across() {
    let mut doc = paragraphs(120);
    let before = doc.revision();
    doc.execute("caret.set", json!({ "pos": { "story": "body", "path": [2], "off": 12 } })).unwrap();
    let caret = |doc: &DocDocument| doc.inspect().unwrap()["caret"].clone();
    let start = caret(&doc);
    let screen = 500.0;
    doc.execute("caret.pageDown", json!({ "distance": screen })).unwrap();
    let down = caret(&doc);
    let went = down["top"].as_f64().unwrap() - start["top"].as_f64().unwrap();
    assert!(went > screen * 0.8 && went < screen * 1.2, "went {went} for {screen}");
    assert!((down["x"].as_f64().unwrap() - start["x"].as_f64().unwrap()).abs() < 8.0, "{start} → {down}");
    // Again: the place across is the one it started from, not the nearest character's.
    doc.execute("caret.pageDown", json!({ "distance": screen })).unwrap();
    doc.execute("caret.pageUp", json!({ "distance": screen })).unwrap();
    doc.execute("caret.pageUp", json!({ "distance": screen })).unwrap();
    let back = caret(&doc);
    assert!((back["top"].as_f64().unwrap() - start["top"].as_f64().unwrap()).abs() < 30.0, "{start} → {back}");
    // Shift: a selection.
    doc.execute("caret.pageDown", json!({ "distance": screen, "extend": true })).unwrap();
    assert!(doc.inspect().unwrap()["selected"].as_u64().unwrap() > 100);
    // Past the end: the end of the text; before the start: the first line.
    doc.execute("caret.pageDown", json!({ "distance": 100_000 })).unwrap();
    assert_eq!(doc.session.borrow().sel.focus, doc.session.borrow().doc.end_of(kition_doc_engine::doc::StoryRef::Body));
    doc.execute("caret.pageUp", json!({ "distance": 100_000 })).unwrap();
    assert_eq!(doc.session.borrow().sel.focus.path.last(), 0);
    // No distance: the start of the next page's first line.
    doc.execute("caret.docStart", json!({})).unwrap();
    doc.execute("caret.pageDown", json!({})).unwrap();
    let next = caret(&doc);
    assert_eq!(next["page"], json!(1));
    let (path, off) = {
        let s = doc.session.borrow();
        (s.sel.focus.path.clone(), s.sel.focus.off)
    };
    let first = doc.layout().hit(1, 0.0, 0.0, kition_doc_engine::doc::StoryRef::Body).unwrap();
    assert_eq!((path, off), (first.path.clone(), first.off), "the first line of page 2, at its start");
    assert!(doc.execute("caret.pageDown", json!({ "distance": -5 })).is_err(), "a distance is positive");
    assert_eq!(doc.revision(), before, "paging is no edit");
}

/// Tab between table cells is no undo step (stage 0B, I3; WPS W-X7): 1 Tab 2 Tab 3 is three steps,
/// and three Undos take all three back; the Tab in the last cell, which adds a row, is one.
#[test]
fn tab_between_cells_is_no_undo_step() {
    let mut doc = DocDocument::create("Table.docx", 0, 0).unwrap();
    doc.execute("table.insert", json!({ "rows": 2, "cols": 2 })).unwrap();
    doc.execute("caret.set", json!({ "pos": { "story": "body", "path": [0, 0, 0, 0], "off": 0 } })).unwrap();
    let depth = |doc: &DocDocument| doc.session.borrow().undo_depth();
    let start = depth(&doc);
    let revision = doc.revision();
    doc.key("Tab", ToolMods::default(), 1.0).unwrap();
    assert_eq!(depth(&doc), start, "Tab to the next cell is no step");
    assert_eq!(doc.revision(), revision, "nor an edit");
    doc.key("Tab", ToolMods { shift: true, ..ToolMods::default() }, 1.0).unwrap();
    assert_eq!(depth(&doc), start, "nor is Shift+Tab back");
    for (step, typed) in ["1", "2", "3"].iter().enumerate() {
        KitTools::text(&mut doc, typed, 1.0).unwrap();
        if step < 2 {
            doc.key("Tab", ToolMods::default(), 1.0).unwrap();
        }
    }
    assert_eq!(depth(&doc), start + 3, "three cells typed, three steps");
    for _ in 0..3 {
        doc.execute("edit.undo", json!({})).unwrap();
    }
    assert!(!text(&doc).contains(['1', '2', '3']), "three Undos take all three back: {:?}", text(&doc));
    // The last cell's Tab adds a row: one step.
    doc.execute("caret.set", json!({ "pos": { "story": "body", "path": [0, 1, 1, 0], "off": 0 } })).unwrap();
    let rows = |doc: &DocDocument| doc.inspect().unwrap()["blocks"][0]["rows"].as_u64();
    let before = depth(&doc);
    doc.key("Tab", ToolMods::default(), 1.0).unwrap();
    assert_eq!(rows(&doc), Some(3));
    assert_eq!(depth(&doc), before + 1, "adding the row is a step");
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(rows(&doc), Some(2));
}

/// Copy answers HTML beside the text (stage 0B, J1): the editor puts both on the system clipboard,
/// so another application keeps the bold; a picture alone answers none.
#[test]
fn copy_answers_its_html_too() {
    let mut doc = DocDocument::create("Bold.docx", 0, 0).unwrap();
    KitTools::text(&mut doc, "plain bold", 1.0).unwrap();
    doc.execute(
        "select.range",
        json!({ "anchor": { "story": "body", "path": [0], "off": 6 }, "focus": { "story": "body", "path": [0], "off": 10 } }),
    )
    .unwrap();
    doc.execute("format.bold", json!({})).unwrap();
    doc.execute(
        "select.range",
        json!({ "anchor": { "story": "body", "path": [0], "off": 0 }, "focus": { "story": "body", "path": [0], "off": 10 } }),
    )
    .unwrap();
    let answer = doc.execute("edit.copy", json!({})).unwrap();
    assert_eq!(answer["text"], "plain bold");
    let html = answer["html"].as_str().unwrap();
    assert!(html.contains("bold") && (html.contains("<b>") || html.contains("<strong>") || html.contains("font-weight")), "{html}");
    doc.execute("select.collapse", json!({})).unwrap();
    assert_eq!(doc.execute("edit.copy", json!({})).unwrap()["text"], "", "nothing selected copies nothing");
}

// ---- Stage 1 of "Doc usable by hand": the mouse's gestures (text_tool.rs).

/// Where character `off` of body paragraph `block` is on the canvas: (x, the line's middle), from
/// the caret the engine reports there.
fn at_char(doc: &mut DocDocument, block: usize, off: usize) -> (f64, f64) {
    doc.execute("caret.set", json!({ "pos": { "story": "body", "path": [block], "off": off } })).unwrap();
    let caret = doc.inspect().unwrap()["caret"].clone();
    (caret["x"].as_f64().unwrap(), caret["top"].as_f64().unwrap() + caret["height"].as_f64().unwrap() / 2.0)
}

fn with_mods(action: PointerAction, (x, y): (f64, f64), mods: ToolMods) -> ToolPointer {
    ToolPointer { action, x, y, mods, pressure: 1.0, zoom: 1.0 }
}

/// A press and its release at `at`, `clock` milliseconds into the test.
fn press(doc: &mut DocDocument, at: (f64, f64), clock: f64, mods: ToolMods) {
    doc.text_tool.clock = Some(clock);
    doc.pointer(&with_mods(PointerAction::Down, at, mods)).unwrap();
    doc.pointer(&with_mods(PointerAction::Up, at, mods)).unwrap();
}

fn selected(doc: &DocDocument) -> String {
    doc.session.borrow().selected_text()
}

fn ends(doc: &DocDocument) -> ((usize, usize), (usize, usize)) {
    let session = doc.session.borrow();
    let (a, b) = session.sel.ordered();
    ((a.path.last(), a.off), (b.path.last(), b.off))
}

/// Presses near in time and place are one more click (WPS W-C1, W-C4): a caret, the word with its
/// space, the paragraph with its mark, a fourth press the paragraph still; presses far apart in
/// time or place start again at one. The browser's double click after two counted presses adds
/// nothing; selecting never edits.
#[test]
fn clicks_count_into_word_and_paragraph() {
    let mut doc = paragraphs(4);
    let before = doc.revision();
    let word = at_char(&mut doc, 1, 12);
    let none = ToolMods::default();
    press(&mut doc, word, 0.0, none);
    assert_eq!(selected(&doc), "");
    press(&mut doc, word, 200.0, none);
    assert_eq!(selected(&doc), "of ", "the word with the space after it");
    doc.pointer(&with_mods(PointerAction::DoubleClick, word, none)).unwrap();
    assert_eq!(selected(&doc), "of ", "the browser's double click adds nothing");
    press(&mut doc, word, 400.0, none);
    assert_eq!(ends(&doc), ((1, 0), (2, 0)), "three clicks: the paragraph with its mark");
    press(&mut doc, word, 600.0, none);
    assert_eq!(ends(&doc), ((1, 0), (2, 0)), "a fourth click keeps the paragraph");
    press(&mut doc, word, 1400.0, none);
    assert_eq!(selected(&doc), "", "too late: one click again");
    press(&mut doc, word, 1600.0, none);
    press(&mut doc, (word.0 + 20.0, word.1), 1700.0, none);
    assert_eq!(selected(&doc), "", "too far: one click again");
    assert_eq!(doc.revision(), before, "selecting is no edit");
}

/// A double click held and dragged grows by whole words, the first word kept whichever way it
/// goes (WPS W-C5); a triple click dragged grows by paragraphs; a plain drag stays by characters
/// across word edges (W-B2).
#[test]
fn a_held_multi_click_drags_by_its_unit() {
    let mut doc = paragraphs(4);
    let none = ToolMods::default();
    // "Paragraph 2 of the stage 0B test, …": double click "of", drag into "stage".
    let of = at_char(&mut doc, 1, 13);
    let stage = at_char(&mut doc, 1, 21);
    let start = at_char(&mut doc, 1, 2);
    press(&mut doc, of, 0.0, none);
    doc.text_tool.clock = Some(100.0);
    doc.pointer(&with_mods(PointerAction::Down, of, none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Drag, stage, none)).unwrap();
    assert_eq!(selected(&doc), "of the stage ");
    doc.pointer(&with_mods(PointerAction::Drag, start, none)).unwrap();
    assert_eq!(selected(&doc), "Paragraph 2 of ", "back past it: the first word stays, the anchor at its end");
    doc.pointer(&with_mods(PointerAction::Up, start, none)).unwrap();
    doc.pointer(&with_mods(PointerAction::DoubleClick, start, none)).unwrap();
    assert_eq!(selected(&doc), "Paragraph 2 of ", "the browser's double click after the drag changes nothing");
    // Three clicks, dragged into the next paragraph: both paragraphs whole.
    let below = at_char(&mut doc, 2, 5);
    press(&mut doc, of, 2000.0, none);
    press(&mut doc, of, 2100.0, none);
    doc.text_tool.clock = Some(2200.0);
    doc.pointer(&with_mods(PointerAction::Down, of, none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Drag, below, none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Up, below, none)).unwrap();
    assert_eq!(ends(&doc), ((1, 0), (3, 0)));
    // One click dragged from inside a word: exact characters.
    let inside = at_char(&mut doc, 1, 15);
    doc.text_tool.clock = Some(5000.0);
    doc.pointer(&with_mods(PointerAction::Down, inside, none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Drag, stage, none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Up, stage, none)).unwrap();
    assert_eq!(selected(&doc), "the st");
}

/// The selection bar, left of the text (WPS W-A8): one click the line, two the paragraph, three
/// the whole document; Shift grows by lines; a drag grows by lines. Within a few pixels of the
/// text a press still puts the caret.
#[test]
fn the_selection_bar_selects_lines_paragraphs_and_all() {
    let mut doc = paragraphs(5);
    let none = ToolMods::default();
    let area = DocDocument::areas(&doc.layout())[0];
    let (left, y1) = at_char(&mut doc, 1, 0);
    let (_, y3) = at_char(&mut doc, 3, 0);
    let bar = |y: f64| (area.x + 20.0, y);
    press(&mut doc, bar(y1), 0.0, none);
    assert_eq!(ends(&doc), ((1, 0), (2, 0)), "the line (a one-line paragraph: with its mark)");
    assert_eq!(doc.inspect().unwrap()["all"], json!(false));
    press(&mut doc, bar(y1), 100.0, none);
    assert_eq!(ends(&doc), ((1, 0), (2, 0)), "two clicks: the paragraph");
    press(&mut doc, bar(y1), 200.0, none);
    assert_eq!(ends(&doc), ((0, 0), (4, doc.session.borrow().doc.para_at(&kition_doc_engine::doc::Pos::body(4, 0)).unwrap().len())));
    assert_eq!(doc.inspect().unwrap()["all"], json!(true), "the whole document: the view stays");
    // Shift in the bar grows by lines from the anchor.
    press(&mut doc, bar(y1), 1000.0, none);
    press(&mut doc, bar(y3), 2000.0, ToolMods { shift: true, ..ToolMods::default() });
    assert_eq!(ends(&doc), ((1, 0), (4, 0)));
    // A drag in the bar grows by lines.
    doc.text_tool.clock = Some(3000.0);
    doc.pointer(&with_mods(PointerAction::Down, bar(y3), none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Drag, bar(y1), none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Up, bar(y1), none)).unwrap();
    assert_eq!(ends(&doc), ((1, 0), (4, 0)), "upwards: the first line kept, the line above taken whole");
    // Right beside the text it is a click in the text.
    press(&mut doc, (left - 2.0, y3), 4000.0, none);
    assert_eq!(ends(&doc), ((3, 0), (3, 0)));
}

/// Cmd+click selects the paragraph's text without its mark (WPS W-C7).
#[test]
fn cmd_click_selects_the_paragraphs_text() {
    let mut doc = paragraphs(3);
    let at = at_char(&mut doc, 1, 20);
    press(&mut doc, at, 0.0, ToolMods { command: true, ..ToolMods::default() });
    assert_eq!(selected(&doc), "Paragraph 2 of the stage 0B test, long enough to read.");
}

/// A drag that goes on to another page selects into it: the page is the pointer's at each sample.
#[test]
fn a_drag_selects_across_pages() {
    let mut doc = paragraphs(3);
    doc.execute("caret.set", json!({ "pos": { "story": "body", "path": [1], "off": 0 } })).unwrap();
    doc.execute("page.insertBreak", json!({})).unwrap();
    assert_eq!(DocDocument::areas(&doc.layout()).len(), 2);
    let last = doc.session.borrow().doc.body.len() - 1;
    let from = at_char(&mut doc, 0, 3);
    let to = at_char(&mut doc, last, 6);
    assert_eq!(doc.inspect().unwrap()["caret"]["page"], json!(1));
    doc.text_tool.clock = Some(0.0);
    doc.pointer(&with_mods(PointerAction::Down, from, ToolMods::default())).unwrap();
    doc.pointer(&with_mods(PointerAction::Drag, to, ToolMods::default())).unwrap();
    doc.pointer(&with_mods(PointerAction::Up, to, ToolMods::default())).unwrap();
    assert_eq!(ends(&doc), ((0, 3), (last, 6)));
}

/// Dragging the selected text (WPS W-K1-W-K4): past three pixels it is carried, a dashed bar shows
/// where it lands, letting go moves it there in one step and leaves it selected; Cmd copies; Alt
/// moves; let go on itself or Escape, nothing moves; a press let go where it went down puts the
/// caret there.
#[test]
fn dragging_the_selected_text_moves_or_copies_it() {
    let mut doc = paragraphs(3);
    let none = ToolMods::default();
    let text = |doc: &DocDocument, block: usize| doc.session.borrow().doc.para_at(&kition_doc_engine::doc::Pos::body(block, 0)).unwrap().text.clone();
    let original = text(&doc, 1);
    let end = at_char(&mut doc, 1, original.len());
    let word = at_char(&mut doc, 1, 12);
    doc.execute(
        "select.range",
        json!({ "anchor": { "story": "body", "path": [1], "off": 11 }, "focus": { "story": "body", "path": [1], "off": 14 } }),
    )
    .unwrap();
    assert_eq!(selected(&doc), " of");
    let over = doc.pointer(&with_mods(PointerAction::Move, word, none)).unwrap();
    assert_eq!(over.cursor, "default", "the arrow over the selected text");
    doc.text_tool.clock = Some(0.0);
    doc.pointer(&with_mods(PointerAction::Down, word, none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Drag, (word.0 + 1.0, word.1), none)).unwrap();
    let held = doc.pointer(&with_mods(PointerAction::Drag, end, none)).unwrap();
    let bars = held.overlays.iter().filter(|o| matches!(o, ToolOverlay::Line { dashed: true, color, .. } if *color == CARET)).count();
    assert_eq!(bars, 1, "a dashed bar where the text lands: {:?}", held.overlays);
    assert_eq!(selected(&doc), " of", "the selection stays while the text is carried");
    doc.pointer(&with_mods(PointerAction::Up, end, none)).unwrap();
    assert_eq!(text(&doc, 1), "Paragraph 2 the stage 0B test, long enough to read. of");
    assert_eq!(selected(&doc), " of", "selected where it landed");
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(text(&doc, 1), original, "one step");
    // Cmd: a copy.
    doc.execute(
        "select.range",
        json!({ "anchor": { "story": "body", "path": [1], "off": 11 }, "focus": { "story": "body", "path": [1], "off": 14 } }),
    )
    .unwrap();
    let cmd = ToolMods { command: true, ..ToolMods::default() };
    doc.text_tool.clock = Some(5000.0);
    doc.pointer(&with_mods(PointerAction::Down, word, none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Drag, end, cmd)).unwrap();
    let copying = doc.pointer(&with_mods(PointerAction::Drag, end, cmd)).unwrap();
    assert_eq!(copying.cursor, "copy");
    doc.pointer(&with_mods(PointerAction::Up, end, cmd)).unwrap();
    assert_eq!(text(&doc, 1), format!("{original} of"));
    doc.execute("edit.undo", json!({})).unwrap();
    // Escape while carried: nothing moves, the selection stays.
    doc.execute(
        "select.range",
        json!({ "anchor": { "story": "body", "path": [1], "off": 11 }, "focus": { "story": "body", "path": [1], "off": 14 } }),
    )
    .unwrap();
    doc.text_tool.clock = Some(10_000.0);
    doc.pointer(&with_mods(PointerAction::Down, word, none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Drag, end, none)).unwrap();
    doc.key("Escape", none, 1.0).unwrap();
    doc.pointer(&with_mods(PointerAction::Up, end, none)).unwrap();
    assert_eq!(text(&doc, 1), original);
    assert_eq!(selected(&doc), " of");
    // Let go on itself: it stays. A press let go where it went down: the caret there.
    doc.text_tool.clock = Some(15_000.0);
    doc.pointer(&with_mods(PointerAction::Down, word, none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Drag, (word.0 + 8.0, word.1), none)).unwrap();
    doc.pointer(&with_mods(PointerAction::Up, (word.0 + 8.0, word.1), none)).unwrap();
    assert_eq!(text(&doc, 1), original);
    assert_eq!(selected(&doc), " of");
    press(&mut doc, word, 20_000.0, none);
    assert_eq!(selected(&doc), "");
    assert_eq!(ends(&doc).0, (1, 12));
}

/// The pointer's shape is the place's (walkthrough B11): the I-beam over text, the bar's arrow
/// left of it, the arrow off the pages, `copy` while the format painter is held; Escape puts the
/// painter down, and `inspect` says it is held.
#[test]
fn the_pointer_shape_follows_the_place_and_the_painter() {
    let mut doc = paragraphs(3);
    let none = ToolMods::default();
    let area = DocDocument::areas(&doc.layout())[0];
    let word = at_char(&mut doc, 1, 5);
    let cursor = |doc: &mut DocDocument, at: (f64, f64)| doc.pointer(&with_mods(PointerAction::Move, at, none)).unwrap().cursor;
    assert_eq!(cursor(&mut doc, word), "text");
    assert!(cursor(&mut doc, (area.x + 20.0, word.1)).starts_with("url("), "the selection bar's arrow");
    assert_eq!(cursor(&mut doc, (area.x + area.w + 20.0, word.1)), "default", "off the page");
    doc.execute("edit.formatPainter", json!({})).unwrap();
    assert_eq!(doc.inspect().unwrap()["painter"], json!("once"));
    assert_eq!(doc.inspect().unwrap()["escape"], json!(true));
    assert_eq!(cursor(&mut doc, word), "copy");
    doc.key("Escape", none, 1.0).unwrap();
    assert_eq!(doc.inspect().unwrap()["painter"], Value::Null);
    doc.execute("edit.formatPainter", json!({ "sticky": true })).unwrap();
    assert_eq!(doc.inspect().unwrap()["painter"], json!("sticky"));
}

/// A right-click (plan stage 2A, walkthrough R1; WPS W-S1-W-S4, Word where WPS is untested): on the
/// selected text the selection stays; elsewhere the caret goes there; on a picture it is selected;
/// in a table the menu knows it; on a misspelled word the menu has its suggestions; with no point
/// (Shift+F10) the selection stays and the menu opens at the caret's foot. Nothing of it edits.
#[test]
fn a_right_click_settles_the_selection_and_says_what_the_menu_is_for() {
    let mut doc = paragraphs(3);
    let revision = doc.revision();
    let inside = at_char(&mut doc, 1, 12);
    let outside = at_char(&mut doc, 0, 4);
    doc.execute(
        "select.range",
        json!({ "anchor": { "story": "body", "path": [1], "off": 10 }, "focus": { "story": "body", "path": [1], "off": 16 } }),
    )
    .unwrap();
    let kept = selected(&doc);
    let answer = doc.context_menu(Some(inside), 1.0).unwrap();
    assert_eq!(selected(&doc), kept, "a right-click on the selection keeps it");
    assert_eq!(answer["target"], json!("text"));
    assert_eq!(answer["selected"], json!(true));
    assert_eq!(answer["table"], json!(false));
    assert_eq!(answer["at"], json!([inside.0, inside.1]), "the menu opens where it was pressed");
    let answer = doc.context_menu(Some(outside), 1.0).unwrap();
    assert_eq!(selected(&doc), "", "elsewhere the caret goes there");
    assert_eq!(ends(&doc), ((0, 4), (0, 4)));
    assert_eq!(answer["selected"], json!(false));
    assert_eq!(answer["spelling"], Value::Null);
    // Shift+F10: the selection stays, the menu opens at the caret's foot.
    doc.execute(
        "select.range",
        json!({ "anchor": { "story": "body", "path": [1], "off": 10 }, "focus": { "story": "body", "path": [1], "off": 16 } }),
    )
    .unwrap();
    let caret = doc.inspect().unwrap()["caret"].clone();
    let answer = doc.context_menu(None, 1.0).unwrap();
    assert_eq!(selected(&doc), kept);
    let foot = caret["top"].as_f64().unwrap() + caret["height"].as_f64().unwrap();
    assert_eq!(answer["at"], json!([caret["x"].as_f64().unwrap(), foot]));
    assert_eq!(doc.revision(), revision, "settling the selection edits nothing");
    // A press held with the left button: a right-click changes nothing.
    doc.text_tool.clock = Some(0.0);
    doc.pointer(&with_mods(PointerAction::Down, inside, ToolMods::default())).unwrap();
    doc.context_menu(Some(outside), 1.0).unwrap();
    assert_eq!(selected(&doc), kept, "not while a press is held");
    doc.pointer(&with_mods(PointerAction::Up, inside, ToolMods::default())).unwrap();
}

#[test]
fn a_right_click_on_a_misspelled_word_offers_its_suggestions() {
    let mut doc = DocDocument::create("Spell.docx", 0, 0).unwrap();
    doc.execute("text.insert", json!({ "text": "This is wrnog here." })).unwrap();
    let word = at_char(&mut doc, 0, 10);
    doc.execute("caret.docStart", json!({})).unwrap();
    let answer = doc.context_menu(Some(word), 1.0).unwrap();
    let spelling = &answer["spelling"];
    assert_eq!(spelling["word"], json!("wrnog"), "{answer}");
    assert_eq!(spelling["kind"], json!("spelling"));
    let suggestions = spelling["suggestions"].as_array().unwrap();
    assert!(!suggestions.is_empty() && suggestions.len() <= 5, "{suggestions:?}");
    assert!(suggestions.contains(&json!("wrong")), "{suggestions:?}");
    // The menu's row: the issue selected, then the suggestion, one undo step.
    doc.execute("select.range", json!({ "anchor": spelling["start"], "focus": spelling["end"] })).unwrap();
    doc.execute("spelling.applySuggestion", json!({ "text": "wrong" })).unwrap();
    assert_eq!(text(&doc), "This is wrong here.");
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(text(&doc), "This is wrnog here.");
    // Checking as you type off: no squiggle, no suggestions.
    doc.execute("spelling.proofing", json!({ "value": false })).unwrap();
    assert_eq!(doc.context_menu(Some(word), 1.0).unwrap()["spelling"], Value::Null);
}

#[test]
fn a_right_click_selects_a_picture_and_knows_a_table() {
    let mut doc = paragraphs(2);
    doc.execute("caret.docEnd", json!({})).unwrap();
    doc.execute("text.newParagraph", json!({})).unwrap();
    let picture = image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 30, 30, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes.into_inner());
    doc.execute("image.insert", json!({ "data": data, "width": 60 })).unwrap();
    doc.execute("select.range", json!({ "anchor": { "story": "body", "path": [2], "off": 0 }, "focus": { "story": "body", "path": [2], "off": 3 } }))
        .unwrap();
    let (hit, _, _) = doc.selected_object().unwrap();
    let area = DocDocument::areas(&doc.layout())[hit.page];
    let middle = (area.x + f64::from(hit.rect.x + hit.rect.w / 2.0), area.y + f64::from(hit.rect.y + hit.rect.h / 2.0));
    doc.execute("caret.docStart", json!({})).unwrap();
    let answer = doc.context_menu(Some(middle), 1.0).unwrap();
    assert_eq!(answer["target"], json!("picture"), "{answer}");
    assert!(doc.selected_object().is_some(), "the picture is selected");
    // Shift+F10 on it: the menu opens at its middle.
    let answer = doc.context_menu(None, 1.0).unwrap();
    assert_eq!(answer["at"], json!([middle.0, middle.1]));
    // A table's cell: the caret goes in, and the menu knows it.
    doc.execute("caret.docStart", json!({})).unwrap();
    doc.execute("table.insert", json!({ "rows": 2, "cols": 2 })).unwrap();
    let cell = {
        let caret = doc.inspect().unwrap()["caret"].clone();
        (caret["x"].as_f64().unwrap() + 2.0, caret["top"].as_f64().unwrap() + caret["height"].as_f64().unwrap() / 2.0)
    };
    doc.execute("caret.docEnd", json!({})).unwrap();
    let answer = doc.context_menu(Some(cell), 1.0).unwrap();
    assert_eq!(answer["table"], json!(true), "{answer}");
    assert_eq!(answer["target"], json!("text"));
}
