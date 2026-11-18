use super::*;
use kition_doc_doc::{props::LineSpacing, section::DocumentGrid};

fn document(size: f32, multiple: f32, grid: bool) -> Document {
    let mut doc = Document::from_text("First line\nSecond line");
    doc.styles.default_chr.font = Some("Source Sans 3".into());
    doc.styles.default_chr.size = Some(size);
    if grid {
        doc.last_section.grid = Some(DocumentGrid { kind: Some("lines".into()), line_pitch: Some(312), char_space: None });
    }
    for index in 0..2 {
        let properties = &mut doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props;
        properties.line_spacing = Some(LineSpacing::Multiple(multiple));
        properties.space_before = Some(0.0);
        properties.space_after = Some(0.0);
    }
    doc
}

fn metrics(doc: &Document) -> Vec<(f32, f32)> {
    let result = layout(doc, &mut LayoutCache::new(), &LayoutOptions::default());
    result.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Placed::Lines { para, y, .. } => Some((para.lines[0].height, y + para.lines[0].baseline)),
            _ => None,
        })
        .collect()
}

#[test]
fn grid_line_multiples_scale_the_pitch_before_bounding_by_text_height() {
    // Independent WPS baselines: 15 pt headings at 413/240 spacing use two grid rows;
    // 12 pt body text at triple spacing uses three rows, rather than four.
    for (size, multiple, expected) in [(15.0, 413.0 / 240.0, 31.2), (12.0, 3.0, 46.8)] {
        let lines = metrics(&document(size, multiple, true));
        assert_eq!(lines.len(), 2);
        assert!((lines[0].0 - expected).abs() < 0.01, "size={size}, multiple={multiple}, height={}", lines[0].0);
        assert!((lines[1].1 - lines[0].1 - expected).abs() < 0.01);
    }
}

#[test]
fn grid_line_multiples_keep_glyphs_centered_using_unscaled_font_metrics() {
    let single = metrics(&document(12.0, 1.0, true));
    let triple = metrics(&document(12.0, 3.0, true));
    let added = triple[0].0 - single[0].0;
    assert!((triple[0].1 - single[0].1 - added / 2.0).abs() < 0.01);
}

#[test]
fn grid_line_multiples_retain_free_spacing_and_allow_large_glyphs() {
    let single = metrics(&document(15.0, 1.0, false));
    let triple = metrics(&document(15.0, 3.0, false));
    assert!((triple[0].0 - single[0].0 * 3.0).abs() < 0.01);
    let mut opted_out = document(15.0, 3.0, true);
    opted_out.styles.default_para.snap_to_grid = Some(false);
    assert_eq!(metrics(&opted_out), triple);
    let natural = metrics(&document(90.0, 1.0, false))[0].0;
    let grid = metrics(&document(90.0, 413.0 / 240.0, true))[0].0;
    assert!(grid >= natural);
    assert!((grid - (natural / 15.6).ceil() * 15.6).abs() < 0.01);
}

#[test]
fn free_line_multiples_add_their_room_below_the_text() {
    // WPS references: off a line grid, a 1.15 line's first baseline sits one ascent below the
    // line's top, as at single spacing; the added room is below the text.
    let single = metrics(&document(12.0, 1.0, false));
    for multiple in [1.15, 1.5, 3.0] {
        let spaced = metrics(&document(12.0, multiple, false));
        assert!((spaced[0].1 - single[0].1).abs() < 0.01, "{multiple}: {spaced:?} {single:?}");
        assert!((spaced[0].0 - single[0].0 * multiple).abs() < 0.01);
        assert!((spaced[1].1 - spaced[0].1 - spaced[0].0).abs() < 0.01);
    }
}

#[test]
fn a_line_that_fits_one_grid_row_takes_the_pitch_times_its_multiple() {
    // WPS for Mac: text whose single line fits one 15.6 pt
    // row is spaced at the pitch times the multiple, not rounded up to whole rows: 1.3 lines
    // measure 20.28 pt and a table of contents' 259/240 lines 16.84 pt.
    for (size, multiple, expected) in [(10.5, 1.3, 15.6 * 1.3), (10.0, 259.0 / 240.0, 15.6 * 259.0 / 240.0)] {
        let lines = metrics(&document(size, multiple, true));
        assert_eq!(lines.len(), 2);
        assert!((lines[0].0 - expected).abs() < 0.01, "size={size}, multiple={multiple}, height={}", lines[0].0);
        assert!((lines[1].1 - lines[0].1 - expected).abs() < 0.01, "step {}", lines[1].1 - lines[0].1);
    }
}

#[test]
fn an_at_least_height_on_the_grid_is_not_rounded_to_rows() {
    // WPS for Mac: "at least 18 pt" lines on a 15.6 pt grid are 18 pt apart with the text at the
    // bottom; "at least 12 pt" keeps the text's one row.
    let lines = |minimum: f32| {
        let mut doc = document(9.0, 1.0, true);
        for index in 0..2 {
            doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props.line_spacing = Some(LineSpacing::AtLeast(minimum));
        }
        let result = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
        let Placed::Lines { para, .. } = &result.pages[0].items[0] else { panic!() };
        let line = &para.lines[0];
        (line.height, line.baseline - line.top, line.descent)
    };
    let (height, baseline, descent) = lines(18.0);
    assert!((height - 18.0).abs() < 0.01 && (baseline - (18.0 - descent)).abs() < 0.01, "{height} {baseline} {descent}");
    assert!((lines(12.0).0 - 15.6).abs() < 0.01);
}
