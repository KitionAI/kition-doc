//! Blocks of whole table cells (plan "Doc usable by hand", stage 4; WPS W-N7, W-N10, W-N11;
//! walkthrough H7, D13): a selection whose two ends are in two different cells of one table
//! selects the rectangle of cells between them, as the Table Layout commands already read it (by
//! cell index, rows top to bottom), not the text that runs between them.
//!
//! The block is read from the selection itself, so it is session state as the selection is: never
//! in the file, the replica or a document revision, and no command reads more than the selection
//! to know it. On a block:
//!
//! - typing (and pasting, a line break, any text-family edit) replaces the first cell's content
//!   only; the other cells keep theirs (WPS untested; plan stage 4 item 2, rule 4: the cautious
//!   way, nothing out of sight is lost);
//! - Backspace and Delete empty the selected cells and keep them selected; Delete on the whole
//!   table deletes the table (WPS W-N10), Backspace there only empties it, and on whole rows
//!   Backspace empties them (W-N11);
//! - Copy takes a table of just those cells; Cut empties them (the whole table: removes it);
//! - character formatting formats each cell's text (`format::apply`), paragraph formatting and the
//!   case, clear and reset commands run once per cell;
//! - Shift+Right at a cell's end first selects the cell's content, then grows into the next cell
//!   (W-N7; section 2 row 21), and on a block goes on cell by cell.
//!
//! Any other edit starts at the first cell (the selection there collapses), and the table
//! commands read the block as they always read a selection across cells.

use kition_doc_doc::edit::Fragment;
use kition_doc_doc::{Block, Blocks, Document, Path, Pos, StoryRef};
use serde_json::{Value, json};

use super::sel_result;
use crate::{CmdResult, Selection, Session, p};

/// A block of whole cells of one table: rows `rows.0..=rows.1`, and in each the cells with index
/// `cols.0..=cols.1` (as many as the row has).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellBlock {
    pub story: StoryRef,
    /// The table's path.
    pub table: Path,
    pub rows: (usize, usize),
    pub cols: (usize, usize),
}

impl CellBlock {
    /// The `(row, cell)` pairs of the block the table has, row by row.
    pub fn cells(&self, doc: &Document) -> Vec<(usize, usize)> {
        let Some(t) = doc.table(self.story, &self.table) else { return Vec::new() };
        let mut out = Vec::new();
        for r in self.rows.0..=self.rows.1 {
            let Some(row) = t.rows.get(r) else { break };
            for c in self.cols.0..=self.cols.1.min(row.cells.len().saturating_sub(1)) {
                out.push((r, c));
            }
        }
        out
    }

    /// Whether every cell of the rows the block covers is in it.
    pub fn whole_rows(&self, doc: &Document) -> bool {
        let Some(t) = doc.table(self.story, &self.table) else { return false };
        self.cols.0 == 0 && (self.rows.0..=self.rows.1).all(|r| t.rows.get(r).is_some_and(|row| self.cols.1 + 1 >= row.cells.len()))
    }

    /// Whether the block is the whole table.
    pub fn whole_table(&self, doc: &Document) -> bool {
        let rows = doc.table(self.story, &self.table).map_or(0, |t| t.rows.len());
        self.rows.0 == 0 && self.rows.1 + 1 >= rows && self.whole_rows(doc)
    }

    /// Where the content of cell `(r, c)` starts and ends.
    pub fn cell_ends(&self, doc: &Document, r: usize, c: usize) -> Option<(Pos, Pos)> {
        cell_ends(doc, self.story, &self.table, r, c)
    }

    /// Each cell's content, from its start to its end, row by row.
    pub fn ranges(&self, doc: &Document) -> Vec<(Pos, Pos)> {
        self.cells(doc).into_iter().filter_map(|(r, c)| self.cell_ends(doc, r, c)).collect()
    }

    /// The first cell's content (the block's top-left cell).
    pub fn first(&self, doc: &Document) -> Option<(Pos, Pos)> {
        let (r, c) = self.cells(doc).into_iter().next()?;
        self.cell_ends(doc, r, c)
    }
}

/// The cell block `sel` selects, if it is one: its ends in two different cells of the same table
/// (the outermost table where they part).
pub fn of(doc: &Document, sel: &Selection) -> Option<CellBlock> {
    let (a, b) = (&sel.anchor, &sel.focus);
    if a.story != b.story {
        return None;
    }
    let (pa, pb) = (&a.path.0, &b.path.0);
    // A cell's paragraph inside the table at `pa[..k]` has the row and the cell at `k` and `k + 1`.
    let mut k = 1usize;
    while pa.len() >= k + 3 && pb.len() >= k + 3 && pa.get(..k) == pb.get(..k) {
        let table = Path(pa.get(..k)?.to_vec());
        doc.table(a.story, &table)?;
        let (ra, ca) = (*pa.get(k)? as usize, *pa.get(k + 1)? as usize);
        let (rb, cb) = (*pb.get(k)? as usize, *pb.get(k + 1)? as usize);
        if (ra, ca) != (rb, cb) {
            return Some(CellBlock { story: a.story, table, rows: (ra.min(rb), ra.max(rb)), cols: (ca.min(cb), ca.max(cb)) });
        }
        // The same cell: a block can only be in a table inside it.
        if pa.get(k + 2) != pb.get(k + 2) {
            return None;
        }
        k += 3;
    }
    None
}

/// The session's cell block, if its selection is one.
pub fn block(s: &Session) -> Option<CellBlock> {
    of(&s.doc, &s.sel)
}

/// The first and the last paragraph of `blocks` (inside a table there, its first and last cell's),
/// as paths under `prefix`, with the last paragraph's length.
fn first_para(mut prefix: Vec<u32>, blocks: &Blocks, depth: usize) -> Option<Path> {
    let block = blocks.first()?;
    prefix.push(0);
    match &**block {
        Block::Para(_) => Some(Path(prefix)),
        Block::Table(t) if depth < 16 => {
            prefix.extend([0, 0]);
            first_para(prefix, &t.rows.first()?.cells.first()?.blocks, depth + 1)
        }
        Block::Table(_) => None,
    }
}

fn last_para(mut prefix: Vec<u32>, blocks: &Blocks, depth: usize) -> Option<(Path, usize)> {
    let i = blocks.len().checked_sub(1)?;
    prefix.push(u32::try_from(i).ok()?);
    match &**blocks.get(i)? {
        Block::Para(p) => Some((Path(prefix), p.len())),
        Block::Table(t) if depth < 16 => {
            let r = t.rows.len().checked_sub(1)?;
            let row = t.rows.get(r)?;
            let c = row.cells.len().checked_sub(1)?;
            prefix.extend([u32::try_from(r).ok()?, u32::try_from(c).ok()?]);
            last_para(prefix, &row.cells.get(c)?.blocks, depth + 1)
        }
        Block::Table(_) => None,
    }
}

/// Where the content of cell `(r, c)` of the table at `table` starts and ends.
pub fn cell_ends(doc: &Document, story: StoryRef, table: &Path, r: usize, c: usize) -> Option<(Pos, Pos)> {
    let cell = doc.table(story, table)?.rows.get(r)?.cells.get(c)?;
    let mut prefix = table.0.clone();
    prefix.extend([u32::try_from(r).ok()?, u32::try_from(c).ok()?]);
    let start = first_para(prefix.clone(), &cell.blocks, 0)?;
    let (end, len) = last_para(prefix, &cell.blocks, 0)?;
    Some((Pos { story, path: start, off: 0 }, Pos { story, path: end, off: len }))
}

/// Run a command with the cell block in mind (called by `Session::run` for every command); without
/// one, the column selection's turn (`column::dispatch`).
pub(crate) fn dispatch(s: &mut Session, id: &str, mutates: bool, run: fn(&mut Session, &Value) -> CmdResult, v: &Value) -> CmdResult {
    if s.column_segments().is_some() || s.math.is_some() {
        return super::column::dispatch(s, id, mutates, run, v);
    }
    if id == "caret.right" && p::bool(v, "extend") == Some(true) && !s.column_mode {
        if let Some(answer) = shift_right(s) {
            return answer;
        }
        return super::column::dispatch(s, id, mutates, run, v);
    }
    let Some(block) = block(s) else { return super::column::dispatch(s, id, mutates, run, v) };
    match id {
        "text.backspace" | "text.delete" | "text.deleteWordBack" | "text.deleteWordForward" => {
            if id == "text.delete" && block.whole_table(&s.doc) {
                return remove_table(s, &block);
            }
            clear(s, &block)?;
            sel_result(s)
        }
        "edit.copy" => copy(s, &block),
        "edit.cut" => {
            let answer = copy(s, &block)?;
            if block.whole_table(&s.doc) {
                remove_table(s, &block)?;
            } else {
                clear(s, &block)?;
            }
            Ok(answer)
        }
        "format.changeCase" | "format.clear" | "format.resetChar" => per_cell(s, run, v, &block),
        _ if id.starts_with("paragraph.") || id.starts_with("list.") => per_cell(s, run, v, &block),
        _ if id.starts_with("format.") || id.starts_with("table.") => run(s, v),
        _ if mutates && (super::family::family(id) == "text" || id.starts_with("edit.paste")) => {
            // Typing replaces the first cell's content; the others keep theirs.
            let Some((a, b)) = block.first(&s.doc) else { return run(s, v) };
            s.sel = Selection { anchor: a, focus: b };
            run(s, v)
        }
        _ if mutates => {
            let Some((a, _)) = block.first(&s.doc) else { return run(s, v) };
            s.sel = Selection::caret(a);
            run(s, v)
        }
        _ => run(s, v),
    }
}

/// Shift+Right at the end of a cell (WPS W-N7): the cell's content first, then the next cell too;
/// on a block, the next cell. `None`: an ordinary Shift+Right.
fn shift_right(s: &mut Session) -> Option<CmdResult> {
    let focus = s.sel.focus.clone();
    let (table, r, c) = focus.path.cell()?;
    let story = focus.story;
    let next = |doc: &Document| -> Option<(usize, usize)> {
        let t = doc.table(story, &table)?;
        let len = t.rows.get(r)?.cells.len();
        if c + 1 < len {
            Some((r, c + 1))
        } else if r + 1 < t.rows.len() {
            Some((r + 1, 0))
        } else {
            None
        }
    };
    let grow = |s: &mut Session| -> CmdResult {
        if let Some((nr, nc)) = next(&s.doc)
            && let Some((_, end)) = cell_ends(&s.doc, story, &table, nr, nc)
        {
            s.sel.focus = end;
            // On into the next row: the rows so far are selected whole (the anchor moves to its
            // row's last cell, so the rectangle spans every column).
            if nr != r
                && let Some((_, ar, _)) = cell_in(&table, &s.sel.anchor)
                && let Some(last) = s.doc.table(story, &table).and_then(|t| t.rows.get(ar)).map(|row| row.cells.len().saturating_sub(1))
                && let Some((start, _)) = cell_ends(&s.doc, story, &table, ar, last)
            {
                s.sel.anchor = start;
            }
        }
        s.goal_x = None;
        s.pending = None;
        sel_result(s)
    };
    if block(s).is_some() {
        return Some(grow(s));
    }
    let (start, end) = cell_ends(&s.doc, story, &table, r, c)?;
    if focus != end {
        return None;
    }
    let (a, b) = s.sel.ordered();
    if a == start && b == end {
        return Some(grow(s));
    }
    s.sel = Selection { anchor: start, focus: end };
    s.goal_x = None;
    s.pending = None;
    Some(sel_result(s))
}

/// Empty every cell of the block (each keeps one empty paragraph, its first one's formatting);
/// tracked, the text is marked deleted. The block stays selected.
fn clear(s: &mut Session, block: &CellBlock) -> Result<(), crate::CmdError> {
    let ranges = block.ranges(&s.doc);
    for (a, b) in ranges.iter().rev() {
        if a == b {
            continue;
        }
        if s.doc.settings.track_changes {
            super::track_delete(s, a, b)?;
        } else {
            s.doc.delete_range(a, b)?;
        }
    }
    // What is left of each cell: its corners' first paragraphs, emptied.
    let corner = |s: &Session, pos: &Pos| -> Option<Pos> {
        let (_, r, c) = cell_in(&block.table, pos)?;
        cell_ends(&s.doc, block.story, &block.table, r, c).map(|(start, _)| start)
    };
    let anchor = corner(s, &s.sel.anchor);
    let focus = corner(s, &s.sel.focus);
    if let (Some(anchor), Some(focus)) = (anchor, focus) {
        s.sel = Selection { anchor, focus };
    }
    s.goal_x = None;
    s.pending = None;
    Ok(())
}

/// The cell of the table at `table` that `pos` is in.
fn cell_in(table: &Path, pos: &Pos) -> Option<(Path, usize, usize)> {
    let k = table.0.len();
    if pos.path.0.len() < k + 3 || pos.path.0.get(..k) != Some(table.0.as_slice()) {
        return None;
    }
    Some((table.clone(), *pos.path.0.get(k)? as usize, *pos.path.0.get(k + 1)? as usize))
}

/// Delete the block's table (Delete on the whole table, WPS W-N10).
fn remove_table(s: &mut Session, block: &CellBlock) -> CmdResult {
    s.doc.remove_block(block.story, &block.table)?;
    s.sel = Selection::caret(s.doc.clamp(&Pos { story: block.story, path: block.table.clone(), off: 0 }));
    s.goal_x = None;
    s.pending = None;
    sel_result(s)
}

/// Copy: a table of just the block's cells (their widths kept); the plain text has a tab between
/// cells and a line break between rows.
fn copy(s: &mut Session, block: &CellBlock) -> CmdResult {
    let Some(t) = s.doc.table(block.story, &block.table) else { return Ok(json!({"text": ""})) };
    let mut sub = t.clone();
    let (g0, g1) = {
        let g0 = t.grid_col(block.rows.0, block.cols.0);
        let width: usize = t
            .rows
            .get(block.rows.0)
            .map(|row| row.cells.iter().skip(block.cols.0).take(block.cols.1.saturating_sub(block.cols.0) + 1).map(|c| c.span()).sum())
            .unwrap_or(1);
        (g0, g0.saturating_add(width))
    };
    sub.rows = t
        .rows
        .iter()
        .skip(block.rows.0)
        .take(block.rows.1 - block.rows.0 + 1)
        .map(|row| {
            let mut row = row.clone();
            row.cells = row.cells.iter().skip(block.cols.0).take(block.cols.1 - block.cols.0 + 1).cloned().collect();
            row
        })
        .collect();
    if let Some(grid) = t.grid.get(g0..g1.min(t.grid.len())).filter(|g| !g.is_empty()) {
        sub.grid = grid.to_vec();
    }
    sub.props.width = None;
    sub.props.width_pct = None;
    let rows: Vec<String> = sub
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| {
                    let mut d = Document::new();
                    d.body = cell.blocks.clone();
                    d.plain_text(StoryRef::Body).trim_end_matches(['\n', '\r']).replace('\n', " ")
                })
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect();
    let frag = Fragment { blocks: vec![Block::Table(sub)], ..Default::default() };
    s.clipboard_text = rows.join("\n");
    s.clip_history.push(&frag, &s.clipboard_text);
    let html = super::edit::copied_html(s, &frag);
    s.clipboard = Some(frag);
    Ok(json!({"text": s.clipboard_text, "html": html}))
}

/// Run a selection command once per cell, then select the block again.
fn per_cell(s: &mut Session, run: fn(&mut Session, &Value) -> CmdResult, v: &Value, block: &CellBlock) -> CmdResult {
    let sel = s.sel.clone();
    for (a, b) in block.ranges(&s.doc).into_iter().rev() {
        s.sel = Selection { anchor: a, focus: b };
        if let Err(e) = run(s, v) {
            s.sel = sel;
            return Err(e);
        }
    }
    s.sel = sel;
    sel_result(s)
}
