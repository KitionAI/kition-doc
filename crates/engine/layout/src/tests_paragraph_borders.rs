use super::*;
use kition_doc_doc::props::{Border, BorderStyle, Borders, LineSpacing};

fn document(lines: usize) -> Document {
    let text = (0..lines).map(|k| format!("Line {k}")).collect::<Vec<_>>().join("\n");
    let mut doc = Document::from_text(&text);
    doc.styles.default_chr.font = Some("Source Sans 3".into());
    doc.styles.default_chr.size = Some(10.5);
    for index in 0..lines {
        let properties = &mut doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props;
        properties.line_spacing = Some(LineSpacing::Multiple(1.0));
        properties.space_before = Some(0.0);
        properties.space_after = Some(0.0);
    }
    doc
}

fn rule(width: f32, space: f32) -> Option<Border> {
    Some(Border { style: BorderStyle::Single, width, color: None, space, theme: None })
}

fn border(doc: &mut Document, index: usize, borders: &Borders) {
    doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props.borders = Some(*borders);
}

/// Each paragraph's top on the first page.
fn tops(doc: &Document) -> Vec<f32> {
    let result = layout(doc, &mut LayoutCache::new(), &LayoutOptions::default());
    result.pages[0].items.iter().filter_map(|item| if let Placed::Lines { y, .. } = item { Some(*y) } else { None }).collect()
}

#[test]
fn a_paragraphs_top_and_bottom_borders_take_their_space_and_width() {
    // WPS for Mac: a 0.75 pt rule 6 pt from the text adds 6.75 pt above and below the paragraph.
    let plain = tops(&document(3));
    let mut doc = document(3);
    border(&mut doc, 1, &Borders { top: rule(0.75, 6.0), bottom: rule(1.5, 1.0), ..Default::default() });
    let bordered = tops(&doc);
    assert!((bordered[1] - plain[1] - 6.75).abs() < 0.01, "{bordered:?} {plain:?}");
    assert!((bordered[2] - plain[2] - 6.75 - 2.5).abs() < 0.01, "{bordered:?} {plain:?}");
}

#[test]
fn paragraphs_with_the_same_borders_share_them_with_a_between_border_inside() {
    let plain = tops(&document(4));
    let shared = Borders { top: rule(0.75, 6.0), bottom: rule(0.75, 6.0), ..Default::default() };
    let mut doc = document(4);
    border(&mut doc, 1, &shared);
    border(&mut doc, 2, &shared);
    let grouped = tops(&doc);
    // Room above the first and below the last only.
    assert!((grouped[2] - grouped[1] - (plain[2] - plain[1])).abs() < 0.01, "{grouped:?}");
    assert!((grouped[3] - plain[3] - 13.5).abs() < 0.01, "{grouped:?}");
    // A between border takes its space on both sides plus its width.
    let between = Borders { between: rule(0.75, 6.0), ..shared };
    let mut doc = document(4);
    border(&mut doc, 1, &between);
    border(&mut doc, 2, &between);
    let split = tops(&doc);
    assert!((split[2] - split[1] - (plain[2] - plain[1]) - 12.75).abs() < 0.01, "{split:?}");
}

#[test]
fn composite_border_styles_take_more_room_than_their_width() {
    // WPS for Mac (synthetic calibration at 0.75 and 1.5 pt): a double line takes three widths, a
    // triple five, a wave 2.16 pt more and a double wave 4.56 pt more.
    let plain = tops(&document(2));
    for (style, extent) in [
        (BorderStyle::Single, 1.5),
        (BorderStyle::Double, 4.5),
        (BorderStyle::Triple, 7.5),
        (BorderStyle::Wave, 3.66),
        (BorderStyle::DoubleWave, 6.06),
    ] {
        let mut doc = document(2);
        let top = Some(Border { style, width: 1.5, color: None, space: 1.0, theme: None });
        border(&mut doc, 1, &Borders { top, ..Default::default() });
        let moved = tops(&doc)[1] - plain[1];
        assert!((moved - 1.0 - extent).abs() < 0.01, "{style:?}: {moved}");
    }
}

#[test]
fn a_paragraphs_last_line_fits_only_with_its_bottom_borders_room() {
    // WPS for Mac: a bordered paragraph's last line moves to the next page when its text fits above
    // the page's bottom margin but the bottom border's room below it doesn't.
    let mut moved_for_the_border = false;
    for filler in 30..60 {
        let mut doc = document(filler + 1);
        border(&mut doc, filler, &Borders { bottom: rule(0.75, 20.0), ..Default::default() });
        let l = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
        let limit = doc.last_section.page_h - doc.last_section.margin_bottom;
        let bordered = l.pages.iter().enumerate().find_map(|(page, p)| {
            p.items.iter().find_map(|i| match i {
                Placed::Lines { y, para, path, .. } if path.0 == [filler as u32] => Some((page, y + para.lines[0].baseline + para.lines[0].descent)),
                _ => None,
            })
        });
        let Some((page, text_end)) = bordered else { panic!("no bordered line") };
        if page == 0 {
            assert!(text_end + 20.75 <= limit + 0.01, "filler {filler}: its border band runs past the margin ({text_end})");
        } else {
            // Its text alone would have fitted where the last filler line ended.
            let prev_end = l.pages[0]
                .items
                .iter()
                .filter_map(|i| if let Placed::Lines { y, para, .. } = i { Some(y + para.height) } else { None })
                .fold(0.0f32, f32::max);
            moved_for_the_border |= prev_end + 15.0 <= limit;
        }
    }
    assert!(moved_for_the_border, "some line moved on only because of its border band");
}
