use super::*;
use kition_kit::testing::check_contract;

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
    for id in ["text.insert", "edit.paste", "edit.undo", "edit.redo", "file.new", "file.open", "file.save", "header.edit"] {
        assert!(mutates(id), "{id} changes the document");
    }
    for id in ["edit.selectAll", "edit.copy", "file.exportPdf", "file.print", "file.inspect"] {
        assert!(!mutates(id), "{id} does not change the document");
    }
}

#[test]
fn commands_flagged_as_not_mutating_never_commit_to_the_replica() {
    let bytes = fixture();
    let commands = DocDocument::open("Letter.docx", &bytes).unwrap().commands();
    // `file` commands and commands that take a path are left out: the test must not touch files.
    for command in commands
        .iter()
        .filter(|command| !command.mutates && !command.id.starts_with("file.") && !command.params.as_deref().unwrap_or_default().contains("path"))
    {
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
