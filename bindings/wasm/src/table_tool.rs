//! A table's own gestures (plan "Doc usable by hand", stage 4; WPS W-N8, W-N9, W-N12; section 2
//! rows 23 and 25 and its notes): what a press on a table takes is the layout's to say
//! (`DocLayout::table_zone`: the borders within 4 screen pixels before everything, then the handle,
//! the band over the top edge, the margin left of the rows), before the text under it.
//!
//! - A column border dragged: a dashed line across the page follows the pointer and the table
//!   stays as it is; let go, one `table.columnWidth {by, column, next}` moves the border, the
//!   column to its right giving up what the left one gains (with Shift, or the table's last
//!   border, only the left column changes). A row's bottom border: `table.rowHeight {height,
//!   row}`. One command, one undo step; Escape drops the drag with nothing done. A double click on
//!   a column border fits the column to its text (`table.autofit {mode: contents, column}`).
//! - The margin left of a row selects the row, a drag there more rows; the band over the top edge
//!   selects the column under it, a drag more columns; the handle by the table's top-left corner
//!   (shown while the pointer is over the table or the caret in it) selects the table. Selecting
//!   is `pure` (`caret.set`, `table.selectTable`), a block of cells (`cells`).
//!
//! The table's commands act on the table at the caret, so a drag's command runs with the caret
//! put in the cell by the border for it and the selection put back after (both `pure`).

use kition_doc_engine::cmd::cells;
use kition_doc_engine::doc::{Path, Pos, StoryRef};
use kition_doc_engine::layout::hit::HANDLE;
use kition_doc_engine::layout::{DocLayout, TableZone};
use kition_doc_engine::{Selection, Session};
use kition_kit::tools::{PointerAction, ToolOverlay, ToolPointer};
use serde_json::{Value, json};

use crate::{BRAND, DocDocument, PageArea};

/// A border takes a press this far from it, in screen pixels at any zoom (section 2's notes).
const BORDER_PX: f64 = 4.0;
/// A drag shorter than this (points) moves no border.
const MIN_DRAG: f32 = 0.5;
/// The dashed line of a dragged border, the handle.
const GUIDE: [u8; 4] = [BRAND[0], BRAND[1], BRAND[2], 255];

/// The pointer over the band above a table's top edge: an arrow pointing down, drawn here (CSS has
/// none), the usual arrow where an image cursor is not shown.
const COLUMN_CURSOR: &str = "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='16' height='20' viewBox='0 0 16 20'%3E%3Cpath d='M5 1h6v10h4l-7 8-7-8h4z' fill='black' stroke='white' stroke-width='1.2' stroke-linejoin='round'/%3E%3C/svg%3E\") 8 19, default";

/// A press on a table not let go yet.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TableGesture {
    /// A border dragged: the right border of cell `cell` of row `row` (`column`), or that row's
    /// bottom border, at page point `border` on `page`; the pointer pressed at `press` and moved
    /// `delta` points across it since.
    Resize { story: StoryRef, table: Path, row: usize, cell: usize, column: bool, page: usize, border: f32, press: f32, delta: f32 },
    /// Rows selected from the margin, from row `anchor`.
    Rows { story: StoryRef, table: Path, anchor: usize },
    /// Columns selected from the band over the top edge, from cell `anchor` of the first row.
    Columns { story: StoryRef, table: Path, anchor: usize },
}

/// The table tool's state: session state, never in the file.
#[derive(Debug, Default)]
pub(crate) struct TableTool {
    pub(crate) gesture: Option<TableGesture>,
}

/// The top-level table a cell path is in.
fn top_table(path: &Path) -> Option<Path> {
    (path.0.len() >= 4).then(|| Path(path.0.iter().take(1).copied().collect()))
}

impl DocDocument {
    /// A pointer event on a table (see the module docs); answers whether the table took it.
    pub(crate) fn table_pointer(&mut self, event: &ToolPointer, layout: &DocLayout, areas: &[PageArea]) -> Result<bool, String> {
        let zoom = event.zoom.max(0.05);
        match event.action {
            PointerAction::Down => {
                self.table_tool.gesture = None;
                let page = Self::page_at(areas, event.y);
                let Some(area) = areas.get(page).copied() else { return Ok(false) };
                let (x, y) = ((event.x - area.x) as f32, (event.y - area.y) as f32);
                let story = self.session.borrow().sel.focus.story;
                let Some(zone) = layout.table_zone(page, x, y, (BORDER_PX / zoom) as f32, story) else { return Ok(false) };
                match zone {
                    TableZone::ColumnBorder { story, table, row, cell, x: border } => {
                        self.table_tool.gesture =
                            Some(TableGesture::Resize { story, table, row, cell, column: true, page, border, press: x, delta: 0.0 });
                    }
                    TableZone::RowBorder { story, table, row, y: border } => {
                        self.table_tool.gesture =
                            Some(TableGesture::Resize { story, table, row, cell: 0, column: false, page, border, press: y, delta: 0.0 });
                    }
                    TableZone::Row { story, table, row } => {
                        // Shift grows the rows already selected in this table (or from the caret's).
                        let anchor = if event.mods.shift { self.anchor_cell(&table).map_or(row, |(r, _)| r) } else { row };
                        self.select_rows(story, &table, anchor, row)?;
                        self.table_tool.gesture = Some(TableGesture::Rows { story, table, anchor });
                    }
                    TableZone::Column { story, table, cell } => {
                        let anchor = if event.mods.shift { self.anchor_cell(&table).map_or(cell, |(_, c)| c) } else { cell };
                        self.select_columns(story, &table, anchor, cell)?;
                        self.table_tool.gesture = Some(TableGesture::Columns { story, table, anchor });
                    }
                    TableZone::Handle { story, table } => {
                        self.select_rows(story, &table, 0, usize::MAX)?;
                    }
                }
                Ok(true)
            }
            PointerAction::Drag => {
                let Some(gesture) = self.table_tool.gesture.clone() else { return Ok(false) };
                match gesture {
                    TableGesture::Resize { story, table, row, cell, column, page, border, press, .. } => {
                        // In the coordinates of the page the drag started on.
                        let Some(area) = areas.get(page) else { return Ok(true) };
                        let at = if column { (event.x - area.x) as f32 } else { (event.y - area.y) as f32 };
                        let delta = if at.is_finite() { (at - press).clamp(-2000.0, 2000.0) } else { 0.0 };
                        self.table_tool.gesture = Some(TableGesture::Resize { story, table, row, cell, column, page, border, press, delta });
                    }
                    TableGesture::Rows { story, table, anchor } => {
                        if let Some((row, _)) = self.cell_under(layout, areas, event, story, &table) {
                            self.select_rows(story, &table, anchor, row)?;
                        }
                    }
                    TableGesture::Columns { story, table, anchor } => {
                        if let Some((_, cell)) = self.cell_under(layout, areas, event, story, &table) {
                            self.select_columns(story, &table, anchor, cell)?;
                        }
                    }
                }
                Ok(true)
            }
            PointerAction::Up => {
                let Some(gesture) = self.table_tool.gesture.take() else { return Ok(false) };
                if let TableGesture::Resize { story, table, row, cell, column, page, delta, .. } = gesture
                    && delta.abs() >= MIN_DRAG
                {
                    let (command, params) = if column {
                        let Some((g, last)) = self.border_column(story, &table, row, cell) else { return Ok(true) };
                        // Shift, read as it is let go, and the last border: only the left column.
                        ("table.columnWidth", json!({ "by": delta, "column": g, "next": !event.mods.shift && !last }))
                    } else {
                        let height = layout
                            .table_cells(story, &table, page..=page)
                            .into_iter()
                            .filter(|(_, r, ..)| *r == row)
                            .map(|(.., rect)| rect.h)
                            .fold(0.0f32, f32::max);
                        ("table.rowHeight", json!({ "height": (height + delta).clamp(4.0, 1584.0), "row": row }))
                    };
                    self.run_in_cell(story, &table, row, cell, command, &params)?;
                }
                Ok(true)
            }
            PointerAction::DoubleClick => {
                let page = Self::page_at(areas, event.y);
                let Some(area) = areas.get(page).copied() else { return Ok(false) };
                let (x, y) = ((event.x - area.x) as f32, (event.y - area.y) as f32);
                let story = self.session.borrow().sel.focus.story;
                match layout.table_zone(page, x, y, (BORDER_PX / zoom) as f32, story) {
                    Some(TableZone::ColumnBorder { story, table, row, cell, .. }) => {
                        self.table_tool.gesture = None;
                        if let Some((g, _)) = self.border_column(story, &table, row, cell) {
                            self.run_in_cell(story, &table, row, cell, "table.autofit", &json!({ "mode": "contents", "column": g }))?;
                        }
                        Ok(true)
                    }
                    // The other zones took their presses already.
                    Some(_) => Ok(true),
                    None => Ok(false),
                }
            }
            PointerAction::Move => Ok(false),
        }
    }

    /// Escape while a border is dragged: the drag is dropped, nothing done. Answers whether there
    /// was one.
    pub(crate) fn cancel_table_gesture(&mut self) -> bool {
        self.table_tool.gesture.take().is_some()
    }

    /// The grid column whose right border is cell `cell`'s of row `row`, and whether it is the
    /// table's last border.
    fn border_column(&self, story: StoryRef, table: &Path, row: usize, cell: usize) -> Option<(usize, bool)> {
        let session = self.session.borrow();
        let t = session.doc.table(story, table)?;
        let span = t.rows.get(row)?.cells.get(cell)?.span().max(1);
        let g = t.grid_col(row, cell) + span - 1;
        Some((g, g + 1 >= t.grid.len()))
    }

    /// Runs a table command with the caret in cell `(row, cell)`, then puts the selection back.
    fn run_in_cell(&mut self, story: StoryRef, table: &Path, row: usize, cell: usize, command: &str, params: &Value) -> Result<(), String> {
        let (saved, start) = {
            let session = self.session.borrow();
            (session.sel.clone(), cells::cell_ends(&session.doc, story, table, row, cell).map(|(start, _)| start))
        };
        let Some(start) = start else { return Ok(()) };
        let run = |session: &mut Session, command: &str, params: &Value| session.run(command, params).map(|_| ()).map_err(|e| e.to_string());
        run(&mut self.session(), "caret.set", &json!({ "pos": kition_doc_engine::cmd::pos_json(&start) }))?;
        let done = run(&mut self.session(), command, params);
        // The border moved; the text did not, so the selection is still where it was.
        let mut session = self.session();
        let doc = session.doc.clone();
        let valid = |p: &Pos| doc.para_at(p).is_some_and(|q| p.off <= q.len());
        if valid(&saved.anchor) && valid(&saved.focus) {
            run(
                &mut session,
                "select.range",
                &json!({ "anchor": kition_doc_engine::cmd::pos_json(&saved.anchor), "focus": kition_doc_engine::cmd::pos_json(&saved.focus) }),
            )?;
        }
        done
    }

    /// The cell of `table` the selection's anchor is in (a Shift press grows from it).
    fn anchor_cell(&self, table: &Path) -> Option<(usize, usize)> {
        let session = self.session.borrow();
        let path = &session.sel.anchor.path.0;
        let k = table.0.len();
        if path.len() < k + 3 || path.get(..k) != Some(table.0.as_slice()) {
            return None;
        }
        Some((*path.get(k)? as usize, *path.get(k + 1)? as usize))
    }

    /// Selects rows `a` to `b` of `table` whole (`usize::MAX`: to the last).
    fn select_rows(&mut self, story: StoryRef, table: &Path, a: usize, b: usize) -> Result<(), String> {
        let ends = {
            let session = self.session.borrow();
            let Some(t) = session.doc.table(story, table) else { return Ok(()) };
            let last = t.rows.len().saturating_sub(1);
            let (a, b) = (a.min(last), b.min(last));
            let first_cell = |r: usize| cells::cell_ends(&session.doc, story, table, r, 0);
            let last_cell =
                |r: usize| t.rows.get(r).and_then(|row| cells::cell_ends(&session.doc, story, table, r, row.cells.len().saturating_sub(1)));
            // Downwards the rows run from the first cell of the first to the last cell of the last;
            // upwards from the last cell of the anchor's row (the rectangle is every column).
            if a <= b { first_cell(a).zip(last_cell(b)).map(|(s, e)| (s.0, e.1)) } else { last_cell(a).zip(first_cell(b)).map(|(s, e)| (s.1, e.0)) }
        };
        self.select(ends)
    }

    /// Selects the columns of cells `a` to `b` of the first row, down to the last row.
    fn select_columns(&mut self, story: StoryRef, table: &Path, a: usize, b: usize) -> Result<(), String> {
        let ends = {
            let session = self.session.borrow();
            let Some(t) = session.doc.table(story, table) else { return Ok(()) };
            let last = t.rows.len().saturating_sub(1);
            let width = |r: usize| t.rows.get(r).map_or(0, |row| row.cells.len().saturating_sub(1));
            let start = cells::cell_ends(&session.doc, story, table, 0, a.min(width(0)));
            let end = cells::cell_ends(&session.doc, story, table, last, b.min(width(last)));
            start.zip(end).map(|(s, e)| if a <= b { (s.0, e.1) } else { (s.1, e.0) })
        };
        self.select(ends)
    }

    fn select(&mut self, ends: Option<(Pos, Pos)>) -> Result<(), String> {
        let Some((anchor, focus)) = ends else { return Ok(()) };
        if self.session.borrow().sel == (Selection { anchor: anchor.clone(), focus: focus.clone() }) {
            return Ok(());
        }
        let pos = kition_doc_engine::cmd::pos_json;
        let mut session = self.session();
        session.run("caret.set", &json!({ "pos": pos(&anchor) })).map_err(|e| e.to_string())?;
        session.run("caret.set", &json!({ "pos": pos(&focus), "extend": true })).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// The `(row, cell)` of `table` under the pointer (across: the row; along the top: the column).
    fn cell_under(&self, layout: &DocLayout, areas: &[PageArea], event: &ToolPointer, story: StoryRef, table: &Path) -> Option<(usize, usize)> {
        let page = Self::page_at(areas, event.y);
        let area = areas.get(page)?;
        let (x, y) = ((event.x - area.x) as f32, (event.y - area.y) as f32);
        let cells = layout.table_cells(story, table, page..=page);
        let row = cells.iter().find(|(.., r)| y >= r.y && y <= r.y + r.h).map(|(_, row, ..)| *row);
        let cell =
            cells.iter().filter(|(_, r, ..)| Some(*r) == row || row.is_none()).find(|(.., r)| x >= r.x && x <= r.x + r.w).map(|(_, _, c, _)| *c);
        // Above or below the table: its first or last row.
        let row = row.or_else(|| {
            let top = cells.iter().map(|(.., r)| r.y).fold(f32::MAX, f32::min);
            let rows = self.session.borrow().doc.table(story, table).map_or(0, |t| t.rows.len());
            Some(if y < top { 0 } else { rows.saturating_sub(1) })
        })?;
        Some((row, cell.unwrap_or(0)))
    }

    /// The shading of a block of cells: each whole cell (stage 4; walkthrough H7).
    pub(crate) fn block_rects(layout: &DocLayout, session: &Session, block: &cells::CellBlock) -> Vec<(usize, [f32; 4])> {
        let page = |pos: Option<Pos>| pos.and_then(|pos| layout.caret_on(&pos, session.page_hint)).map(|c| c.page);
        let Some(first) = page(block.first(&session.doc).map(|(a, _)| a)) else { return Vec::new() };
        let last_cell = block.cells(&session.doc).last().copied();
        let last = page(last_cell.and_then(|(r, c)| block.cell_ends(&session.doc, r, c)).map(|(_, b)| b)).unwrap_or(first).max(first);
        let inside = block.cells(&session.doc);
        layout
            .table_cells(block.story, &block.table, first..=last.min(first + 400))
            .into_iter()
            .filter(|(_, r, c, _)| inside.contains(&(*r, *c)))
            .map(|(page, _, _, rect)| (page, [rect.x, rect.y, rect.x + rect.w, rect.y + rect.h]))
            .collect()
    }

    /// A dragged border's dashed line across the page; the handle of the table the pointer is over
    /// or the caret is in.
    pub(crate) fn table_overlays(&self, layout: &DocLayout, areas: &[PageArea], out: &mut Vec<ToolOverlay>) {
        if let Some(TableGesture::Resize { column, page, border, delta, .. }) = &self.table_tool.gesture {
            let Some(area) = areas.get(*page) else { return };
            let at = f64::from(*border + *delta);
            if *column {
                out.push(ToolOverlay::Line { ax: area.x + at, ay: area.y, bx: area.x + at, by: area.y + area.h, color: GUIDE, dashed: true });
            } else {
                out.push(ToolOverlay::Line { ax: area.x, ay: area.y + at, bx: area.x + area.w, by: area.y + at, color: GUIDE, dashed: true });
            }
            return;
        }
        let Some((story, table, page)) = self.handle_table(layout, areas) else { return };
        let Some((frame, true)) = layout.table_frame(page, story, &table) else { return };
        let Some(area) = areas.get(page) else { return };
        let x0 = area.x + f64::from(frame.x - 2.0 - HANDLE);
        let y0 = area.y + f64::from(frame.y - 2.0 - HANDLE);
        let s = f64::from(HANDLE);
        let line = |ax: f64, ay: f64, bx: f64, by: f64| ToolOverlay::Line { ax, ay, bx, by, color: GUIDE, dashed: false };
        out.extend([
            line(x0, y0, x0 + s, y0),
            line(x0 + s, y0, x0 + s, y0 + s),
            line(x0 + s, y0 + s, x0, y0 + s),
            line(x0, y0 + s, x0, y0),
            // A cross inside: it moves nothing yet, but selects the table.
            line(x0 + s / 2.0, y0 + 1.5, x0 + s / 2.0, y0 + s - 1.5),
            line(x0 + 1.5, y0 + s / 2.0, x0 + s - 1.5, y0 + s / 2.0),
        ]);
    }

    /// The table whose handle shows, and the page its first row is on: the one under the pointer
    /// (or near its handle), else the caret's.
    fn handle_table(&self, layout: &DocLayout, areas: &[PageArea]) -> Option<(StoryRef, Path, usize)> {
        let session = self.session.borrow();
        let story = session.sel.focus.story;
        let first_page = |table: &Path| {
            let (start, _) = cells::cell_ends(&session.doc, story, table, 0, 0)?;
            layout.caret_on(&start, session.page_hint).map(|c| c.page)
        };
        if let Some((x, y, _, zoom)) = self.text_tool.at {
            let page = Self::page_at(areas, y);
            if let Some(area) = areas.get(page) {
                let (px, py) = ((x - area.x) as f32, (y - area.y) as f32);
                let over = layout.cell_at(page, px, py).filter(|(s, ..)| *s == story).and_then(|(_, t, ..)| t.0.first().map(|b| Path(vec![*b])));
                let near = match layout.table_zone(page, px, py, (BORDER_PX / zoom) as f32, story) {
                    Some(TableZone::Handle { table, .. } | TableZone::Column { table, .. } | TableZone::Row { table, .. }) => Some(table),
                    _ => None,
                };
                if let Some(table) = near.or(over)
                    && let Some(page) = first_page(&table)
                {
                    return Some((story, table, page));
                }
            }
        }
        let table = top_table(&session.sel.focus.path)?;
        let page = first_page(&table)?;
        Some((story, table, page))
    }

    /// The pointer's shape over a table (`None`: not over one of its zones): the arrows of a
    /// border, the selection bar's arrow left of a row, a down arrow over a column, the move cross
    /// on the handle; while a border is dragged, its arrows.
    pub(crate) fn table_cursor(&self, layout: &DocLayout, areas: &[PageArea]) -> Option<String> {
        match &self.table_tool.gesture {
            Some(TableGesture::Resize { column: true, .. }) => return Some("col-resize".into()),
            Some(TableGesture::Resize { column: false, .. }) => return Some("row-resize".into()),
            Some(TableGesture::Rows { .. }) => return Some(crate::text_tool::BAR_CURSOR.into()),
            Some(TableGesture::Columns { .. }) => return Some(COLUMN_CURSOR.into()),
            None => {}
        }
        if self.text_tool.gesture.is_some() {
            return None;
        }
        let (x, y, _, zoom) = self.text_tool.at?;
        let page = Self::page_at(areas, y);
        let area = areas.get(page)?;
        let story = self.session.borrow().sel.focus.story;
        let zone = layout.table_zone(page, (x - area.x) as f32, (y - area.y) as f32, (BORDER_PX / zoom) as f32, story)?;
        Some(
            match zone {
                TableZone::ColumnBorder { .. } => "col-resize",
                TableZone::RowBorder { .. } => "row-resize",
                TableZone::Row { .. } => crate::text_tool::BAR_CURSOR,
                TableZone::Column { .. } => COLUMN_CURSOR,
                TableZone::Handle { .. } => "move",
            }
            .into(),
        )
    }
}
