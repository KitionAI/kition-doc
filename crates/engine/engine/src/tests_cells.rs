//! Blocks of whole table cells and the table's borders by hand (plan "Doc usable by hand", stage
//! 4; walkthrough H6, H7, D13; WPS W-N7, W-N10, W-N11, W-N12).

use kition_doc_doc::{Path, Pos, StoryRef};
use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn at(path: &[u32], off: usize) -> Value {
    json!({ "story": "body", "path": path, "off": off })
}

/// A paragraph, then a 3x3 table whose cells say A1..C3, then a paragraph.
fn table() -> Session {
    let mut s = Session::new(kition_doc_doc::Document::new());
    run(&mut s, "text.insert", json!({"text": "Before"}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "After"}));
    run(&mut s, "caret.set", json!({"pos": at(&[1], 0)}));
    run(&mut s, "table.insert", json!({"rows": 3, "cols": 3}));
    for r in 0..3u32 {
        for c in 0..3u32 {
            run(&mut s, "caret.set", json!({"pos": at(&[1, r, c, 0], 0)}));
            let name = format!("{}{}", ["A", "B", "C"][c as usize], r + 1);
            run(&mut s, "text.insert", json!({"text": name, "raw": true}));
        }
    }
    s
}

fn cell(s: &Session, r: u32, c: u32) -> String {
    s.doc.para(StoryRef::Body, &Path(vec![1, r, c, 0])).map(|p| p.text.clone()).unwrap_or_default()
}

fn drag(s: &mut Session, from: (u32, u32), to: (u32, u32)) {
    run(s, "caret.set", json!({"pos": at(&[1, from.0, from.1, 0], 1)}));
    run(s, "caret.set", json!({"pos": at(&[1, to.0, to.1, 0], 1), "extend": true}));
}

#[test]
fn a_selection_across_cells_is_the_block_between_them() {
    let mut s = table();
    drag(&mut s, (0, 0), (1, 1));
    let block = crate::cmd::cells::block(&s).expect("a block");
    assert_eq!((block.rows, block.cols), ((0, 1), (0, 1)));
    assert_eq!(block.cells(&s.doc), vec![(0, 0), (0, 1), (1, 0), (1, 1)], "C1 is not in it");
    // Dragged up and to the right: the same rectangle.
    drag(&mut s, (1, 0), (0, 1));
    assert_eq!(crate::cmd::cells::block(&s).map(|b| (b.rows, b.cols)), Some(((0, 1), (0, 1))));
    // In one cell, or from outside the table: no block.
    drag(&mut s, (0, 0), (0, 0));
    assert!(crate::cmd::cells::block(&s).is_none());
    run(&mut s, "select.range", json!({"anchor": at(&[0], 0), "focus": at(&[1, 1, 1, 0], 1)}));
    assert!(crate::cmd::cells::block(&s).is_none());
}

#[test]
fn typing_on_a_block_replaces_only_the_first_cell() {
    let mut s = table();
    drag(&mut s, (0, 0), (1, 1));
    let rev = s.rev();
    run(&mut s, "text.insert", json!({"text": "Z"}));
    assert_eq!(cell(&s, 0, 0), "Z");
    for (r, c, was) in [(0, 1, "B1"), (1, 0, "A2"), (1, 1, "B2"), (0, 2, "C1")] {
        assert_eq!(cell(&s, r, c), was, "the other cells keep their text");
    }
    assert!(s.rev() > rev);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(cell(&s, 0, 0), "A1", "one undo step");
}

#[test]
fn backspace_and_delete_empty_the_block_and_keep_it_selected() {
    for key in ["text.backspace", "text.delete"] {
        let mut s = table();
        drag(&mut s, (0, 0), (1, 1));
        run(&mut s, key, json!({}));
        for (r, c) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            assert_eq!(cell(&s, r, c), "", "{key} empties {r},{c}");
        }
        assert_eq!(cell(&s, 0, 2), "C1", "{key}: C1 is outside the block");
        assert_eq!(cell(&s, 2, 0), "A3");
        assert!(crate::cmd::cells::block(&s).is_some(), "{key}: the block stays selected");
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(cell(&s, 1, 1), "B2", "{key}: one undo step");
    }
}

#[test]
fn delete_removes_a_whole_table_backspace_only_empties_it() {
    let mut s = table();
    run(&mut s, "caret.set", json!({"pos": at(&[1, 1, 1, 0], 0)}));
    run(&mut s, "table.selectTable", json!({}));
    assert!(crate::cmd::cells::block(&s).is_some_and(|b| b.whole_table(&s.doc)));
    run(&mut s, "text.backspace", json!({}));
    assert!(s.doc.table(StoryRef::Body, &Path(vec![1])).is_some(), "Backspace keeps the table (WPS W-N10)");
    assert_eq!((cell(&s, 0, 0), cell(&s, 2, 2)), (String::new(), String::new()));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(cell(&s, 2, 2), "C3");
    run(&mut s, "table.selectTable", json!({}));
    run(&mut s, "text.delete", json!({}));
    assert!(s.doc.table(StoryRef::Body, &Path(vec![1])).is_none(), "Delete removes it");
    let texts: Vec<String> = s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| p.text.clone()).collect();
    assert_eq!(texts, vec!["Before".to_string(), "After".to_string()]);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(cell(&s, 0, 0), "A1", "one undo step");
}

#[test]
fn backspace_on_whole_rows_empties_them_and_keeps_the_rows() {
    let mut s = table();
    run(&mut s, "caret.set", json!({"pos": at(&[1, 1, 0, 0], 0)}));
    run(&mut s, "table.selectRow", json!({}));
    run(&mut s, "text.backspace", json!({}));
    assert_eq!(s.doc.table(StoryRef::Body, &Path(vec![1])).map(|t| t.rows.len()), Some(3), "WPS W-N11");
    assert_eq!((cell(&s, 1, 0), cell(&s, 1, 2), cell(&s, 0, 0)), (String::new(), String::new(), "A1".into()));
}

#[test]
fn formatting_a_block_formats_only_its_cells() {
    let mut s = table();
    drag(&mut s, (0, 0), (1, 1));
    run(&mut s, "format.bold", json!({}));
    let bold = |s: &Session, r: u32, c: u32| {
        let p = s.doc.para(StoryRef::Body, &Path(vec![1, r, c, 0])).unwrap();
        p.props_at(0).bold.unwrap_or(false)
    };
    assert!(bold(&s, 0, 0) && bold(&s, 1, 1));
    assert!(!bold(&s, 0, 2), "C1 is between the ends, not in the block");
}

#[test]
fn shift_right_at_a_cells_end_selects_the_cell_then_the_next() {
    let mut s = table();
    run(&mut s, "caret.set", json!({"pos": at(&[1, 0, 0, 0], 1)}));
    run(&mut s, "caret.right", json!({"extend": true}));
    assert_eq!(s.selected_text(), "1");
    // At the end: the whole cell first.
    run(&mut s, "caret.right", json!({"extend": true}));
    assert_eq!(s.selected_text(), "A1");
    assert!(crate::cmd::cells::block(&s).is_none());
    run(&mut s, "caret.right", json!({"extend": true}));
    assert_eq!(crate::cmd::cells::block(&s).map(|b| (b.rows, b.cols)), Some(((0, 0), (0, 1))));
    run(&mut s, "caret.right", json!({"extend": true}));
    assert_eq!(crate::cmd::cells::block(&s).map(|b| (b.rows, b.cols)), Some(((0, 0), (0, 2))));
    // Past the row's end, on into the next row.
    run(&mut s, "caret.right", json!({"extend": true}));
    assert_eq!(crate::cmd::cells::block(&s).map(|b| (b.rows, b.cols)), Some(((0, 1), (0, 2))));
}

#[test]
fn copying_a_block_copies_a_table_of_its_cells() {
    let mut s = table();
    drag(&mut s, (0, 0), (1, 1));
    let answer = run(&mut s, "edit.copy", json!({}));
    assert_eq!(answer["text"], json!("A1\tB1\nA2\tB2"));
    // Pasted after the table it is a 2x2 table.
    run(&mut s, "caret.set", json!({"pos": at(&[2], 5)}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "edit.paste", json!({}));
    let tables: Vec<_> = s
        .doc
        .body
        .iter()
        .filter_map(|b| match &**b {
            kition_doc_doc::Block::Table(t) => Some((t.rows.len(), t.rows.first().map_or(0, |r| r.cells.len()))),
            _ => None,
        })
        .collect();
    assert_eq!(tables, vec![(3, 3), (2, 2)], "H9");
}

#[test]
fn a_dragged_border_moves_between_two_columns_and_keeps_the_table_width() {
    let mut s = table();
    run(&mut s, "caret.set", json!({"pos": at(&[1, 0, 0, 0], 0)}));
    let grid = |s: &Session| s.doc.table(StoryRef::Body, &Path(vec![1])).unwrap().grid.clone();
    let before = grid(&s);
    let depth = s.undo_depth();
    run(&mut s, "table.columnWidth", json!({"by": 30, "column": 0, "next": true}));
    let after = grid(&s);
    assert!((after[0] - before[0] - 30.0).abs() < 0.6, "{before:?} -> {after:?}");
    assert!((after[1] - before[1] + 30.0).abs() < 0.6);
    assert!((after.iter().sum::<f32>() - before.iter().sum::<f32>()).abs() < 0.6, "the table keeps its width");
    assert_eq!(s.undo_depth(), depth + 1, "one undo step");
    // Without `next` (Shift, or the last border) only that column changes.
    run(&mut s, "table.columnWidth", json!({"by": -20, "column": 2}));
    let last = grid(&s);
    assert!((last[2] - after[2] + 20.0).abs() < 0.6 && (last[1] - after[1]).abs() < 0.6);
    // Never narrower than 6 points: the neighbour gives up only what it has.
    run(&mut s, "table.columnWidth", json!({"by": 10_000, "column": 0, "next": true}));
    assert!(grid(&s)[1] >= 5.9);
    assert!(s.run("table.columnWidth", &json!({"by": 5, "column": 9})).is_err());
    assert!(s.run("table.columnWidth", &json!({"by": "x"})).is_err());
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(grid(&s), before);
}

#[test]
fn a_dragged_row_border_sets_that_rows_height() {
    let mut s = table();
    run(&mut s, "caret.set", json!({"pos": at(&[1, 0, 0, 0], 0)}));
    run(&mut s, "table.rowHeight", json!({"height": 40, "row": 2}));
    let t = s.doc.table(StoryRef::Body, &Path(vec![1])).unwrap();
    assert_eq!(t.rows[2].props.height, Some(40.0));
    assert_eq!(t.rows[0].props.height, None, "the caret's row is not touched");
    assert!(s.run("table.rowHeight", &json!({"height": 40, "row": 7})).is_err());
}

#[test]
fn a_double_clicked_border_fits_one_column() {
    let mut s = table();
    run(&mut s, "caret.set", json!({"pos": at(&[1, 0, 0, 0], 0)}));
    let before = s.doc.table(StoryRef::Body, &Path(vec![1])).unwrap().grid.clone();
    run(&mut s, "table.autofit", json!({"mode": "contents", "column": 0}));
    let after = s.doc.table(StoryRef::Body, &Path(vec![1])).unwrap().grid.clone();
    assert!(after[0] < before[0], "A1..A3 need less than a third of the page: {after:?}");
    assert!((after[1] - before[1]).abs() < 0.6 && (after[2] - before[2]).abs() < 0.6, "the others keep theirs");
    assert!(s.run("table.autofit", &json!({"mode": "contents", "column": 3})).is_err());
}

#[test]
fn the_block_never_reaches_the_document() {
    let mut s = table();
    let rev = s.rev();
    drag(&mut s, (0, 0), (2, 2));
    assert_eq!(s.rev(), rev, "selecting cells is no edit");
    let _ = Pos::default();
}

/// The by-hand parameters and every command on a block of cells, with junk, never panic.
#[test]
fn hostile_params_on_a_block_never_panic() {
    let junk = [
        json!({"by": 1e308, "column": 1e9, "next": "x"}),
        json!({"by": -1e308, "column": -1, "next": true}),
        json!({"by": f64::NAN}),
        json!({"height": 1e308, "row": 1e12}),
        json!({"height": 30, "row": "x", "rule": 5}),
        json!({"mode": "contents", "column": 1e9}),
        json!({"mode": "contents", "column": -3}),
    ];
    for id in ["table.columnWidth", "table.rowHeight", "table.autofit"] {
        for j in &junk {
            let mut s = table();
            run(&mut s, "caret.set", json!({"pos": at(&[1, 1, 1, 0], 0)}));
            let _ = s.run(id, j);
            s.clamp_selection();
            let _ = s.layout();
        }
    }
    let reg = crate::cmd::registry();
    for spec in reg.all() {
        if crate::cmd::family::family(spec.id) == "file" || spec.id == "image.insert" || spec.id == "text.insertFromFile" {
            continue;
        }
        let mut s = table();
        drag(&mut s, (2, 2), (0, 1));
        let _ = s.run(spec.id, &json!({}));
        s.clamp_selection();
        let _ = s.layout();
    }
}
