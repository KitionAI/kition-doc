//! Keep-with-next measurements use isolated walk state and the real table row splitter.

use std::collections::HashMap;

use kition_doc_doc::{Table, numbering::Counters, resolve::ResolvedPara};

use crate::{Block, Ctx, LayoutCache, LayoutOptions, LineEnd, Placed, StoryRef, ViewMode, border_room, paragraph_spacing, spacing_units, table};

#[derive(PartialEq, Eq)]
struct TableKey {
    width: u32,
    grid: Option<u32>,
    page: (u32, u32, u32, u32),
    equation: u32,
    counters: Counters,
}

pub(super) struct TableStart {
    key: TableKey,
    first: f32,
    total: f32,
}

impl TableStart {
    fn reserved(&self, floating: bool, capacity: f32) -> f32 {
        if floating && self.total <= capacity + 0.01 { 0.0 } else { self.first }
    }
}

fn first_piece(row: &table::RowLayout, whole: bool) -> f32 {
    if whole {
        return row.height;
    }
    let mut cuts = Vec::new();
    for item in &row.items {
        if let Some(rect) = item.turned_bounds() {
            cuts.push(rect.bottom());
            continue;
        }
        match item {
            Placed::Lines { para, l0, l1, y, .. } => {
                let base = para.lines.get(*l0).map_or(0.0, |line| line.top);
                for line in para.lines.get(*l0..*l1).unwrap_or(&[]) {
                    cuts.push(y + line.top - base + line.height);
                }
            }
            Placed::Image { rect, .. } | Placed::Shape { rect, .. } | Placed::Graphic { rect, .. } => cuts.push(rect.bottom()),
            _ => {}
        }
    }
    cuts.retain(|cut| cut.is_finite() && *cut > 0.0 && *cut < row.height);
    cuts.sort_by(f32::total_cmp);
    cuts.dedup();
    // Blank extra row height cannot necessarily split even if its text fits. Use the
    // placement splitter's decision rather than assuming that the first text line suffices.
    cuts.into_iter().find(|cut| table::split_row(row, *cut).is_some()).unwrap_or(row.height)
}

fn table_height(ctx: &mut Ctx, peek: &mut Ctx, table: &Table, block: usize, width: f32, capacity: f32) -> f32 {
    let key = TableKey {
        width: width.to_bits(),
        grid: peek.grid_pitch.map(f32::to_bits),
        page: peek.fields.page_key(),
        equation: peek.eq_count,
        counters: peek.counters.clone(),
    };
    let floating = table.props.float.is_some() && ctx.opts.view == ViewMode::Print;
    if let Some(cached) = ctx.table_starts.get(&block).filter(|cached| cached.key == key) {
        return cached.reserved(floating, capacity);
    }
    let layout = table::layout_table(peek, StoryRef::Body, table, &[block as u32], width, 1);
    let whole = table.rows.first().is_some_and(|row| row.props.cant_split || row.props.header);
    let measured = TableStart {
        key,
        first: layout.rows.first().map_or(0.0, |row| first_piece(row, whole)),
        total: layout.rows.iter().map(|row| row.height).sum(),
    };
    let reserved = measured.reserved(floating, capacity);
    ctx.table_starts.insert(block, measured);
    reserved
}

/// Reserve a fitting chain, bounded by an empty column and explicit/section breaks.
pub(super) fn height(
    ctx: &mut Ctx,
    block: usize,
    section_end: usize,
    width: f32,
    previous: &ResolvedPara,
    available: f32,
    capacity: f32,
) -> (f32, f32) {
    let doc = ctx.doc;
    let options = LayoutOptions { proofing: false, ..ctx.opts.clone() };
    let mut cache = LayoutCache::new();
    let mut peek = Ctx {
        doc,
        cache: &mut cache,
        opts: &options,
        counters: ctx.counters.clone(),
        fields: ctx.fields.clone(),
        notes_hash: ctx.notes_hash,
        numbers: HashMap::new(),
        eq_count: ctx.eq_count,
        grid_pitch: ctx.grid_pitch,
        char_pitch: ctx.char_pitch,
        vertical: ctx.vertical,
        boxes: Default::default(),
        table_starts: HashMap::new(),
    };
    let mut previous = previous.clone();
    let following = doc.body.get(block.saturating_add(1)).map(AsRef::as_ref);
    previous.space_after = spacing_units::margins(doc, &previous, (None, following), None, false, false).1;
    let mut height = 0.0;
    let mut first_gap = None;
    for index in block.saturating_add(1)..=section_end {
        let Some(next) = doc.body.get(index) else { break };
        match &**next {
            Block::Para(paragraph) => {
                let label = peek.next_label(paragraph);
                let layout = peek.para_labelled(paragraph, width, None, &[], label);
                if layout.rp.page_break_before {
                    break;
                }
                let neighbors =
                    (index.checked_sub(1).and_then(|index| doc.body.get(index)).map(AsRef::as_ref), doc.body.get(index + 1).map(AsRef::as_ref));
                let (before, after) = spacing_units::margins(doc, &layout.rp, neighbors, None, false, false);
                let room = border_room(doc, &layout.rp, neighbors, None);
                let gap = previous.space_after
                    + paragraph_spacing(&layout.rp, Some((&previous.style, previous.contextual_spacing, previous.space_after)), before)
                    + room.top;
                let forced = layout.lines.iter().find(|line| matches!(line.end, LineEnd::PageBreak | LineEnd::ColumnBreak));
                let continues = layout.rp.keep_next && forced.is_none() && index < section_end;
                let reserved = if continues || layout.rp.keep_lines {
                    forced.map_or(layout.height + room.bottom, |line| line.top + line.height)
                } else {
                    // The lines the paragraph needs on this page for the one before to stay with
                    // it: its first line, or its first two under widow control, which moves a
                    // single first line on (Word's orphan rule).
                    let lines = if layout.rp.widow_control && layout.lines.len() >= 2 { 2 } else { 1 };
                    let lines = layout
                        .lines
                        .iter()
                        .position(|line| matches!(line.end, LineEnd::PageBreak | LineEnd::ColumnBreak))
                        .map_or(lines, |at| lines.min(at + 1));
                    layout.lines.get(lines - 1).map_or(0.0, |line| line.top + line.height)
                };
                first_gap.get_or_insert(gap);
                height += gap + reserved;
                if !continues || height > available {
                    break;
                }
                previous = layout.rp.clone();
                previous.space_after = after;
            }
            Block::Table(table) => {
                let first = table_height(ctx, &mut peek, table, index, width, capacity);
                if first > 0.0 {
                    first_gap.get_or_insert(previous.space_after);
                    height += previous.space_after + first;
                }
                break;
            }
        }
    }
    (height, first_gap.unwrap_or(0.0))
}
