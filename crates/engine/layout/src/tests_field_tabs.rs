use super::*;
use crate::display::{DisplayOptions, Draw, page_display};
use kition_doc_doc::Pos;
use kition_doc_doc::props::{LineSpacing, TabAlign, TabLeader, TabStop};

fn document(text: &str, field: bool) -> Document {
    let mut doc = Document::from_text(if field { "" } else { text });
    doc.styles.default_chr.font = Some("Source Sans 3".into());
    doc.styles.default_chr.size = Some(12.0);
    doc.styles.default_para.space_before = Some(0.0);
    doc.styles.default_para.space_after = Some(0.0);
    doc.styles.default_para.line_spacing = Some(LineSpacing::Exactly(20.0));
    let p = doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    p.props.tabs = Some(vec![TabStop { pos: 200.0, align: TabAlign::Right, leader: TabLeader::Dot }]);
    if field {
        p.insert_object(
            0,
            InlineObject::Field { instr: "REF heading".into(), result: text.into(), locked: true, code_runs: Vec::new(), code_fields: Vec::new() },
            &Default::default(),
        )
        .unwrap();
    }
    doc
}

fn measure(doc: &Document) -> DocLayout {
    layout(doc, &mut LayoutCache::new(), &Default::default())
}

fn paragraph(layout: &DocLayout) -> &ParaLayout {
    layout.pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Placed::Lines { para, .. } => Some(para.as_ref()),
            _ => None,
        })
        .unwrap()
}

fn glyphs(doc: &Document, layout: &DocLayout) -> Vec<(String, u32, f32, f32)> {
    page_display(doc, &layout.pages[0], &Default::default())
        .into_iter()
        .flat_map(|draw| match draw {
            Draw::Glyphs { face, glyphs, .. } => glyphs.into_iter().map(|(g, x, y)| (face.family.clone(), g, x, y)).collect(),
            _ => Vec::new(),
        })
        .collect()
}

fn same_glyphs(actual: &[(String, u32, f32, f32)], expected: &[(String, u32, f32, f32)]) {
    assert_eq!(actual.len(), expected.len(), "field and paragraph text draw the same glyphs, including leaders");
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!((&actual.0, actual.1), (&expected.0, expected.1));
        assert!((actual.2 - expected.2).abs() < 0.01 && (actual.3 - expected.3).abs() < 0.01, "{actual:?} versus {expected:?}");
    }
}

#[test]
fn field_result_tabs_use_the_paragraph_stop_and_draw_the_same_leaders() {
    let field = document("Heading\t7", true);
    let plain = document("Heading\t7", false);
    let layout = measure(&field);
    let p = paragraph(&layout);
    assert!(p.clusters.iter().any(|c| c.kind == para::ClKind::Tab));
    assert_eq!(p.lines[0].leaders.len(), 1);
    assert!((p.lines[0].end_x() - 200.0).abs() < 0.01);
    same_glyphs(&glyphs(&field, &layout), &glyphs(&plain, &measure(&plain)));
    assert_eq!(field.para(StoryRef::Body, &Path::top(0)).unwrap().objects[0].plain_text(), "Heading\t7");
}

#[test]
fn field_result_tabs_keep_a_trailing_tab_before_another_field() {
    let mut field = document("Heading\t", true);
    let p = field.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    p.insert_object(
        p.text.len(),
        InlineObject::Field { instr: "REF page".into(), result: "7".into(), locked: false, code_runs: Vec::new(), code_fields: Vec::new() },
        &Default::default(),
    )
    .unwrap();
    let plain = document("Heading\t7", false);
    same_glyphs(&glyphs(&field, &measure(&field)), &glyphs(&plain, &measure(&plain)));
}

#[test]
fn field_result_tabs_preserve_empty_segments_and_multiple_stops() {
    for text in ["\tHeading", "A\t\t7", "A\tB\t", "\t\t"] {
        let mut field = document(text, true);
        let mut plain = document(text, false);
        for doc in [&mut field, &mut plain] {
            doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props.tabs = Some(vec![
                TabStop { pos: 60.0, align: TabAlign::Left, leader: TabLeader::Dot },
                TabStop { pos: 200.0, align: TabAlign::Right, leader: TabLeader::Hyphen },
            ]);
        }
        same_glyphs(&glyphs(&field, &measure(&field)), &glyphs(&plain, &measure(&plain)));
    }
}

#[test]
fn field_result_tabs_preserve_font_runs_inside_the_result() {
    let mut field = document("A\u{2014}B\t7", true);
    let mut plain = document("A\u{2014}B\t7", false);
    for doc in [&mut field, &mut plain] {
        doc.styles.default_chr.font = Some("Source Serif 4".into());
        doc.styles.default_chr.font_ea = Some("Source Sans 3".into());
        doc.styles.default_chr.font_hint = Some("eastAsia".into());
        doc.styles.default_chr.lang_ea = Some("zh-CN".into());
    }
    same_glyphs(&glyphs(&field, &measure(&field)), &glyphs(&plain, &measure(&plain)));
}

#[test]
fn field_result_tabs_keep_carets_at_object_boundaries_after_wrapping() {
    let mut doc = document("Heading\t7", true);
    doc.last_section.page_w = 100.0;
    doc.last_section.margin_left = 10.0;
    doc.last_section.margin_right = 10.0;
    doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props.tabs =
        Some(vec![TabStop { pos: 120.0, align: TabAlign::Left, leader: TabLeader::None }]);
    let layout = measure(&doc);
    let p = paragraph(&layout);
    assert_eq!(p.lines.len(), 2);
    assert_eq!(p.line_of(0), 0, "the caret before the field stays on its first line");
    assert_eq!(p.line_of(3), 1, "the caret after the field follows its last line");
    assert!(layout.caret(&Pos::body(0, 0)).unwrap().top < layout.caret(&Pos::body(0, 3)).unwrap().top);
    for line in 0..p.lines.len() {
        let offsets = p.line_offsets(line);
        assert!(offsets.iter().all(|offset| [0, 3].contains(offset)));
        assert!(offsets.windows(2).all(|pair| pair[0] != pair[1]), "an atomic field has no duplicate keyboard positions");
        for x in [0.0, 20.0, 60.0, 80.0] {
            assert!([0, 3].contains(&p.off_at_x(line, x)));
        }
    }
}

#[test]
fn field_result_tabs_leaders_export_their_actual_unicode_text() {
    let doc = document("Heading\t7", false);
    let layout = measure(&doc);
    let draws = page_display(&doc, &layout.pages[0], &DisplayOptions::default());
    let leaders: Vec<_> = draws
        .iter()
        .filter_map(|draw| match draw {
            Draw::Glyphs { face, glyphs, text, ranges, .. } if glyphs.len() > 10 && glyphs.iter().all(|(g, _, _)| *g == face.glyph_for('.')) => {
                Some((glyphs, text, ranges))
            }
            _ => None,
        })
        .collect();
    assert_eq!(leaders.len(), 1);
    let (glyphs, text, ranges) = leaders[0];
    assert_eq!(text, &".".repeat(glyphs.len()));
    assert_eq!(ranges.len(), glyphs.len());
    for (index, range) in ranges.iter().enumerate() {
        assert_eq!(*range, index..index + 1);
    }
}

#[test]
fn field_result_tabs_plain_fields_keep_ligature_text_ranges() {
    let doc = document("office ffi", true);
    let layout = measure(&doc);
    assert_eq!(paragraph(&layout).clusters.len(), 1, "a plain single-font field retains its atomic cluster");
    let draws = page_display(&doc, &layout.pages[0], &Default::default());
    let mut printed = String::new();
    for draw in draws {
        if let Draw::Glyphs { glyphs, text, ranges, .. } = draw {
            assert_eq!(ranges.len(), glyphs.len());
            for range in ranges {
                assert!(text.get(range).is_some(), "glyph text ranges must remain UTF-8 boundaries");
            }
            printed.push_str(&text);
        }
    }
    assert_eq!(printed, "office ffi");
}

#[test]
fn field_result_tabs_middle_dot_leaders_keep_utf8_ranges() {
    let mut doc = document("Heading\t7", true);
    doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props.tabs.as_mut().unwrap()[0].leader = TabLeader::MiddleDot;
    let layout = measure(&doc);
    let draws = page_display(&doc, &layout.pages[0], &Default::default());
    let (glyphs, text, ranges) = draws
        .iter()
        .find_map(|draw| match draw {
            Draw::Glyphs { glyphs, text, ranges, .. } if !text.is_empty() && text.chars().all(|ch| ch == '\u{00b7}') => Some((glyphs, text, ranges)),
            _ => None,
        })
        .unwrap();
    assert!(glyphs.len() > 10);
    assert_eq!(ranges.len(), glyphs.len());
    for (index, range) in ranges.iter().enumerate() {
        assert_eq!(*range, index * 2..(index + 1) * 2);
        assert_eq!(text.get(range.clone()), Some("\u{00b7}"));
    }
}

#[test]
fn field_result_tabs_decimal_points_stay_on_the_stop_when_the_line_finishes() {
    for field in [false, true] {
        let mut doc = document("Heading\t12.50", field);
        doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props.tabs.as_mut().unwrap()[0].align = TabAlign::Decimal;
        let layout = measure(&doc);
        let p = paragraph(&layout);
        let decimal = p.clusters.iter().position(|cluster| cluster.dot).unwrap();
        assert!(
            (p.lines[0].cl_left(decimal).unwrap() - 200.0).abs() < 0.01,
            "field={field}: the decimal point, rather than the final digit, belongs at the stop"
        );
        assert!(p.lines[0].end_x() > 200.0);
    }
}

#[test]
fn field_result_tabs_center_the_complete_following_segment() {
    for field in [false, true] {
        let mut doc = document("Heading\t123", field);
        doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props.tabs.as_mut().unwrap()[0].align = TabAlign::Center;
        let layout = measure(&doc);
        let p = paragraph(&layout);
        let tab = p.clusters.iter().position(|cluster| cluster.kind == para::ClKind::Tab).unwrap();
        let left = p.lines[0].cl_right(tab).unwrap();
        assert!(((left + p.lines[0].end_x()) * 0.5 - 200.0).abs() < 0.01);
    }
}

#[test]
fn a_tab_leader_runs_from_half_a_mark_after_the_text_up_to_the_next_text() {
    // As WPS fills a contents line: the first leader mark half a mark's width after the text,
    // then marks edge to edge, the last one ending at or before the text after the tab; no grid
    // pushes the first mark further along.
    for title in ["Title", "Titles", "Intro"] {
        let doc = document(&format!("{title}\t7"), false);
        let layout = measure(&doc);
        let draws = page_display(&doc, &layout.pages[0], &Default::default());
        let runs: Vec<(String, Vec<f32>, f32)> = draws
            .iter()
            .filter_map(|d| match d {
                Draw::Glyphs { glyphs, text, face, size, .. } => {
                    let adv = face.advance(glyphs.first()?.0) as f32 * size / face.upem.max(1.0) as f32;
                    Some((text.clone(), glyphs.iter().map(|g| g.1).collect(), adv))
                }
                _ => None,
            })
            .collect();
        let (_, dots, adv) = runs.iter().find(|(t, _, _)| t.starts_with('.')).expect("a leader");
        let text_end = {
            let p = paragraph(&layout);
            let line = &p.lines[0];
            let k = p.clusters.iter().position(|c| matches!(c.kind, crate::para::ClKind::Tab)).unwrap();
            line.cl_left(k).unwrap()
        };
        let number = runs.iter().find(|(t, _, _)| t.ends_with('7')).and_then(|(_, xs, _)| xs.last().copied()).expect("the number");
        let first = dots[0] - (layout.pages[0].body.x + text_end);
        assert!((first - adv / 2.0).abs() < 0.05, "{title}: first mark {first} after the text, not half of {adv}");
        let last_end = dots.last().unwrap() + adv;
        assert!(last_end <= number + 0.05 && number - last_end < *adv, "{title}: last mark ends at {last_end}, number at {number}");
    }
}

#[test]
fn dot_leaders_step_a_whole_tenth_of_a_point() {
    // WPS for Mac steps a dot leader's marks a whole tenth of a point apart, the mark's advance
    // rounded up (luan's contents: 11 pt Helvetica Neue's 3.06 pt full stop 3.1 pt apart).
    for size in [10.5, 11.0, 12.0] {
        let mut doc = document("Title\t7", false);
        doc.styles.default_chr.size = Some(size);
        let layout = measure(&doc);
        let draws = page_display(&doc, &layout.pages[0], &Default::default());
        let (dots, adv) = draws
            .iter()
            .find_map(|d| match d {
                Draw::Glyphs { glyphs, text, face, size, .. } if text.starts_with('.') => {
                    Some((glyphs.iter().map(|g| g.1).collect::<Vec<_>>(), face.advance(glyphs.first()?.0) as f32 * size / face.upem.max(1.0) as f32))
                }
                _ => None,
            })
            .expect("a leader");
        let step = (adv * 10.0 - 0.001).ceil() / 10.0;
        assert!(step > adv - 0.001 && step - adv < 0.1);
        for pair in dots.windows(2) {
            assert!((pair[1] - pair[0] - step).abs() < 0.001, "{size} pt: marks {} apart, not {step}", pair[1] - pair[0]);
        }
    }
}
