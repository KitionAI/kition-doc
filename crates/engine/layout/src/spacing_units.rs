//! Resolve automatic and line-unit paragraph spacing before story placement.

use kition_doc_doc::{Block, Document, resolve::ResolvedPara};

use crate::para::{CellText, ParaEnv, ParaLayout};

pub(super) fn fixed_auto_spacing(doc: &Document) -> bool {
    doc.settings.compat_flags.iter().any(|flag| flag == "doNotUseHTMLParagraphAutoSpacing")
}

/// Automatic margins depend on container and list boundaries, not shaped glyphs.
/// Keep them outside the paragraph cache so an unchanged paragraph can move.
pub(super) fn margins(
    doc: &Document,
    paragraph: &ResolvedPara,
    neighbors: (Option<&Block>, Option<&Block>),
    table: Option<&CellText>,
    at_start: bool,
    at_cell_end: bool,
) -> (f32, f32) {
    if fixed_auto_spacing(doc) {
        return (paragraph.space_before, paragraph.space_after);
    }
    let same_list = |block: Option<&Block>| {
        let Some(current) = paragraph.numbering else { return false };
        let Some(Block::Para(other)) = block else { return false };
        doc.styles.resolve_para_in(&other.props, table.map(|text| &text.para)).numbering.is_some_and(|next| next.num == current.num)
    };
    let before = if paragraph.space_before_auto && (at_start || same_list(neighbors.0)) { 0.0 } else { paragraph.space_before };
    let after = if paragraph.space_after_auto && (at_cell_end || same_list(neighbors.1)) { 0.0 } else { paragraph.space_after };
    (before, after)
}

/// The height of a line of paragraph spacing (`w:beforeLines`, `w:afterLines`) without a line grid.
const LINE_UNIT: f32 = 12.0;

pub(super) fn apply(paragraph: &mut ParaLayout, environment: &ParaEnv) {
    let grid = environment.grid_pitch.filter(|pitch| paragraph.rp.snap_to_grid && pitch.is_finite() && *pitch > 0.0);
    // A line unit is the grid's pitch, or 12 pt without a line grid whatever the paragraph's
    // fonts and size (WPS for Mac references).
    let line = grid.unwrap_or(LINE_UNIT);
    let (before, after) = (line, line);
    // Automatic HTML margins override both line and point constraints. The legacy
    // fixed policy specifies 5pt before and 10pt after (ISO 29500,
    // doNotUseHTMLParagraphAutoSpacing).
    let fixed = fixed_auto_spacing(environment.doc);
    if paragraph.rp.space_before_auto {
        paragraph.rp.space_before = if fixed { 5.0 } else { 14.0 };
    } else if let Some(lines) = paragraph.rp.space_before_lines.filter(|lines| *lines > 0) {
        // Nonzero line units outrank points even when inherited (MS-OI29500
        // 2.1.60); zero units leave the point constraint active.
        paragraph.rp.space_before = (before * (lines as f32 / 100.0)).clamp(0.0, 1584.0);
    }
    if paragraph.rp.space_after_auto {
        paragraph.rp.space_after = if fixed { 10.0 } else { 14.0 };
    } else if let Some(lines) = paragraph.rp.space_after_lines.filter(|lines| *lines > 0) {
        paragraph.rp.space_after = (after * (lines as f32 / 100.0)).clamp(0.0, 1584.0);
    }
}
