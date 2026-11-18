//! Text, table and list problems met while building the Kition Showcase samples.

use kition_doc_engine::{
    Session,
    doc::{Block, Document, Path, StoryRef, Table, props::BorderStyle},
    layout::display::{Draw, page_display},
};
use serde_json::{Value, json};

fn session() -> Session {
    Session::new(Document::new())
}

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn body(s: &Session, i: usize) -> &kition_doc_engine::doc::Paragraph {
    s.doc.para(StoryRef::Body, &Path::top(i)).expect("a body paragraph")
}

fn table(s: &Session) -> &Table {
    s.doc.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t) } else { None }).expect("a table")
}

/// A footnote written by the command leaves the caret in the body after its reference; the text
/// typed there and the paragraph Enter starts aren't superscript. Without text the caret goes
/// into the note, and Close Header and Footer comes back to the reference, not the document start.
#[test]
fn a_footnote_leaves_the_caret_after_its_reference() {
    let mut s = session();
    run(&mut s, "text.insert", json!({"text": "First paragraph."}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "Kelp grows fast"}));
    run(&mut s, "references.footnote", json!({"text": "Up to half a metre a day."}));
    assert_eq!(s.sel.focus.story, StoryRef::Body, "the caret stays in the body");
    run(&mut s, "text.insert", json!({"text": " in summer."}));
    let p = body(&s, 1);
    let after = p.text.find(" in summer").expect("typed after the reference");
    assert_eq!(p.props_of_char(after).style, None, "text after the reference isn't in its style");
    run(&mut s, "caret.set", json!({"pos": {"story": "body", "path": [1], "off": after}}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "Next"}));
    let next = body(&s, 2);
    assert_eq!(next.props_of_char(0).style, None, "the next paragraph isn't superscript: {:?}", next.mark);

    // Without text: into the note, then back to just after its reference.
    run(&mut s, "caret.set", json!({"pos": {"story": "body", "path": [0], "off": 5}}));
    run(&mut s, "references.footnote", json!({}));
    assert_ne!(s.sel.focus.story, StoryRef::Body);
    run(&mut s, "text.insert", json!({"text": "A note."}));
    run(&mut s, "insert.closeHeader", json!({}));
    assert_eq!(s.sel.focus.story, StoryRef::Body);
    assert_eq!((s.sel.focus.path.clone(), s.sel.focus.off), (Path::top(0), 5 + 3), "just after the new reference mark");
}

/// Borders apply to the selected cells: a bottom border on the selected first row underlines
/// that row, leaving the table's outer edge and other cells' borders as they were.
#[test]
fn table_borders_apply_to_the_selected_cells() {
    let mut s = session();
    run(&mut s, "insert.table", json!({"rows": 3, "cols": 2}));
    run(&mut s, "table.borders", json!({"kind": "none"}));
    // A border on one cell (the caret's second-row cell, selected).
    run(&mut s, "text.tab", json!({}));
    run(&mut s, "text.tab", json!({}));
    run(&mut s, "text.insert", json!({"text": "Total"}));
    run(&mut s, "table.selectCell", json!({}));
    run(&mut s, "table.borders", json!({"kind": "right", "width": 2}));
    assert_eq!(table(&s).rows[1].cells[0].props.borders.and_then(|b| b.right).map(|b| b.width), Some(2.0));
    // The header row's bottom edge.
    run(&mut s, "select.collapse", json!({}));
    run(&mut s, "caret.docStart", json!({}));
    run(&mut s, "table.selectRow", json!({}));
    run(&mut s, "table.borders", json!({"kind": "bottom", "width": 1, "color": "0D5C63"}));
    let t = table(&s);
    for c in &t.rows[0].cells {
        let b = c.props.borders.and_then(|b| b.bottom).expect("a bottom border on the header row");
        assert_eq!((b.style, b.width), (BorderStyle::Single, 1.0));
    }
    assert!(t.props.borders.and_then(|b| b.bottom).is_none_or(|b| b.style == BorderStyle::None), "the table's bottom edge is not set");
    assert_eq!(t.rows[1].cells[0].props.borders.and_then(|b| b.right).map(|b| b.width), Some(2.0), "other cells keep their borders");
    assert!(t.rows[2].cells.iter().all(|c| c.props.borders.and_then(|b| b.bottom).is_none()));
    // With the whole table selected, the table's own bottom edge.
    run(&mut s, "table.selectTable", json!({}));
    run(&mut s, "table.borders", json!({"kind": "bottom", "width": 1.5}));
    let t = table(&s);
    assert_eq!(t.props.borders.and_then(|b| b.bottom).map(|b| b.width), Some(1.5));
    assert_eq!(t.rows[1].cells[0].props.borders.and_then(|b| b.right).map(|b| b.width), Some(2.0), "a bottom border keeps the right ones");
}

/// Row height with the whole table (or several rows) selected sets every selected row.
#[test]
fn row_height_sets_every_selected_row() {
    let mut s = session();
    run(&mut s, "insert.table", json!({"rows": 3, "cols": 2}));
    run(&mut s, "table.selectTable", json!({}));
    run(&mut s, "table.rowHeight", json!({"height": 30, "rule": "exact"}));
    let heights: Vec<Option<f32>> = table(&s).rows.iter().map(|r| r.props.height).collect();
    assert_eq!(heights, [Some(30.0); 3]);
}

/// `list.define` refuses number formats it can't draw instead of drawing something else, and
/// draws Chinese digit numbering (ideographDigital) as Word does.
#[test]
fn list_formats_are_checked() {
    let mut s = session();
    run(&mut s, "text.insert", json!({"text": "Item"}));
    let err = s.run("list.define", &json!({"levels": [{"format": "hebrew2", "text": "%1."}]})).expect_err("an unknown format").to_string();
    assert!(err.contains("hebrew2") && err.contains("ideographDigital"), "{err}");
    run(&mut s, "list.define", json!({"levels": [{"format": "ideographDigital", "text": "%1、"}]}));
    use kition_doc_engine::doc::section::NumFormat;
    assert_eq!(NumFormat::IdeographDigital.format(1), "一");
    assert_eq!(NumFormat::IdeographDigital.format(10), "一〇");
    assert_eq!(NumFormat::IdeographDigital.format(205), "二〇五");
}

/// The word count counts each Chinese character as a word, as Word does.
#[test]
fn chinese_characters_are_words() {
    let mut s = session();
    run(&mut s, "text.insert", json!({"text": "第一条 租赁物：出租方将位于星河路的房屋出租给承租方。Lease 2026"}));
    let count = run(&mut s, "review.wordCount", json!({}));
    // 第一条 (3) + 租赁物 (3) + 出租方将位于星河路的房屋出租给承租方 (18) + Lease + 2026.
    assert_eq!(count["words"], 3 + 3 + 18 + 2, "{count}");
}

/// An underscore tab leader is one unbroken line (a rule over the marks), whatever the font's
/// underscores look like; the marks stay as the line's text.
#[test]
fn an_underscore_leader_is_one_line() {
    let mut s = session();
    run(&mut s, "para.tabs", json!({"tabs": [{"pos": 300, "align": "right", "leader": "underscore"}]}));
    run(&mut s, "text.insert", json!({"text": "Name"}));
    run(&mut s, "text.tab", json!({}));
    run(&mut s, "text.insert", json!({"text": "1"}));
    let layout = s.layout();
    let draws = page_display(&s.doc, &layout.pages[0], &Default::default());
    let underscores = draws.iter().any(|d| matches!(d, Draw::Glyphs { text, .. } if text.starts_with("___")));
    assert!(underscores, "the leader's marks are drawn as text");
    let rule = draws.iter().find_map(|d| if let Draw::Line { x0, x1, .. } = d { Some(x1 - x0) } else { None });
    assert!(rule.is_some_and(|w| w > 150.0), "one rule across the tab: {rule:?}");
}

/// In a table cell whose text runs top to bottom, Chinese characters stand upright (each turned
/// back a quarter); Latin text lies along the line.
#[test]
fn vertical_cell_text_keeps_chinese_upright() {
    let mut s = session();
    run(&mut s, "insert.table", json!({"rows": 1, "cols": 2}));
    run(&mut s, "text.insert", json!({"text": "竖排文字 AB"}));
    run(&mut s, "table.textDirection", json!({"value": "down"}));
    let layout = s.layout();
    let draws = page_display(&s.doc, &layout.pages[0], &Default::default());
    let turned: Vec<&Draw> = draws.iter().filter_map(|d| if let Draw::Turned { items, .. } = d { Some(items) } else { None }).flatten().collect();
    let upright = turned.iter().filter(|d| matches!(d, Draw::Rotated { items, .. } if matches!(items.first(), Some(Draw::Glyphs { text, .. }) if text.chars().all(|c| "竖排文字".contains(c))))).count();
    assert_eq!(upright, 4, "each Chinese character is turned upright");
    let lying = turned.iter().any(|d| matches!(d, Draw::Glyphs { text, .. } if text.contains('A')));
    assert!(lying, "Latin letters lie along the line");
}
