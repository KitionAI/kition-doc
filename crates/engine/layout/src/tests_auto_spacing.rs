use super::*;
use kition_doc_doc::props::{Align, ParaProps};

const HAN: &str = "\u{4e2d}";

fn paragraph(text: &str, latin: bool, numbers: bool, split_runs: bool, width: f32) -> Arc<ParaLayout> {
    let mut doc = Document::from_text(text);
    doc.styles.default_chr.size = Some(20.0);
    doc.last_section.page_w = width + 144.0;
    let source = doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    source.props = ParaProps { align: Some(Align::Left), auto_space_de: Some(latin), auto_space_dn: Some(numbers), ..Default::default() };
    if split_runs {
        for (offset, c) in text.char_indices() {
            doc.format_range(&kition_doc_doc::Pos::body(0, offset), &kition_doc_doc::Pos::body(0, offset + c.len_utf8()), &|props| {
                props.bold = Some(offset % 2 == 0);
            })
            .unwrap();
        }
    }
    let layout = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    let Placed::Lines { para, .. } = &layout.pages[0].items[0] else { panic!() };
    para.clone()
}

fn width(para: &ParaLayout) -> f32 {
    para.lines[0].end_x() - para.lines[0].xs[0]
}

#[test]
fn automatic_east_asian_spacing_honors_independent_flags_across_runs() {
    for split in [false, true] {
        let text = format!("{HAN}A{HAN}1{HAN}");
        let plain = width(&paragraph(&text, false, false, split, 468.0));
        let latin = width(&paragraph(&text, true, false, split, 468.0));
        let numbers = width(&paragraph(&text, false, true, split, 468.0));
        let both = width(&paragraph(&text, true, true, split, 468.0));
        assert!((latin - plain - 10.0).abs() < 0.01, "two Latin boundaries: {}", latin - plain);
        assert!((numbers - plain - 10.0).abs() < 0.01, "two numeric boundaries: {}", numbers - plain);
        assert!((both - plain - 20.0).abs() < 0.01);
    }
}

#[test]
fn automatic_east_asian_spacing_does_not_duplicate_spaces_or_pad_line_edges() {
    for text in [format!("{HAN} A {HAN}"), format!("{HAN}\tA\t{HAN}"), format!("{HAN}\u{ff21}{HAN}"), format!("{HAN},A")] {
        assert!((width(&paragraph(&text, true, true, false, 468.0)) - width(&paragraph(&text, false, false, false, 468.0))).abs() < 0.01);
    }
    let text = format!("{HAN}A");
    let plain = paragraph(&text, false, false, false, 468.0);
    let wrapped = paragraph(&text, true, true, false, width(&plain) + 1.0);
    assert_eq!(wrapped.lines.len(), 2, "automatic spacing must participate in wrapping");
    assert!(wrapped.lines.iter().all(|line| line.xs[0].abs() < 0.01), "wrapped lines must not inherit leading spacing");
    let first = &wrapped.clusters[0];
    assert!((wrapped.lines[0].end_x() - first.adv).abs() < 0.01, "no trailing spacing at a wrap");
}

#[test]
fn automatic_east_asian_spacing_does_not_follow_hidden_markers_onto_a_new_line() {
    let plain = paragraph(&format!("{HAN}A"), false, false, false, 468.0);
    let mut doc = Document::from_text(&format!("{HAN}XA"));
    doc.styles.default_chr.size = Some(20.0);
    doc.last_section.page_w = width(&plain) + 145.0;
    doc.format_range(&kition_doc_doc::Pos::body(0, HAN.len()), &kition_doc_doc::Pos::body(0, HAN.len() + 1), &|props| {
        props.hidden = Some(true);
    })
    .unwrap();
    let layout = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    let Placed::Lines { para, .. } = &layout.pages[0].items[0] else { panic!() };
    assert_eq!(para.lines.len(), 2);
    let second = &para.lines[1];
    let visible = para.clusters.iter().position(|cluster| cluster.start == HAN.len() + 1).unwrap();
    assert!(second.xs[visible - second.c0].abs() < 0.01, "the first visible cluster must start at the margin");
    assert_eq!(doc.body[0].as_para().unwrap().text, format!("{HAN}XA"));
}

#[test]
fn automatic_east_asian_spacing_respects_the_font_slot_of_numeric_symbols() {
    fn measured(text: &str, enabled: bool) -> Arc<ParaLayout> {
        let mut doc = Document::from_text(text);
        doc.styles.default_chr.size = Some(20.0);
        doc.styles.default_chr.font_hint = Some("eastAsia".into());
        let props = &mut doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props;
        props.auto_space_dn = Some(enabled);
        props.auto_space_de = Some(enabled);
        let result = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
        let Placed::Lines { para, .. } = &result.pages[0].items[0] else { panic!() };
        para.clone()
    }
    // Enclosed and Roman numerals in the East Asian slot are not ASCII digit runs.
    for symbol in ['\u{2460}', '\u{2776}', '\u{2160}'] {
        for text in [format!("{symbol}{HAN}"), format!("{HAN}{symbol}")] {
            let plain = width(&measured(&text, false));
            let automatic = width(&measured(&text, true));
            assert!((automatic - plain).abs() < 0.01, "{symbol:?}: unexpected numeric spacing {}", automatic - plain);
        }
        let latin = format!("{symbol}A");
        assert!((width(&measured(&latin, true)) - width(&measured(&latin, false)) - 5.0).abs() < 0.01);
    }
    let text = format!("1{HAN}");
    assert!((width(&measured(&text, true)) - width(&measured(&text, false)) - 5.0).abs() < 0.01);
}
