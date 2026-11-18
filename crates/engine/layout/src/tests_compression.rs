use super::*;
use kition_doc_doc::props::Align;

fn paragraph(text: &str, width: f32, mode: Option<&str>, align: Align) -> Arc<ParaLayout> {
    let mut doc = Document::from_text(text);
    doc.styles.default_chr.size = Some(20.0);
    doc.settings.char_spacing = mode.map(str::to_string);
    doc.last_section.page_w = width + 144.0;
    let props = &mut doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props;
    props.align = Some(align);
    props.auto_space_de = Some(false);
    props.auto_space_dn = Some(false);
    let result = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    let Placed::Lines { para, .. } = &result.pages[0].items[0] else { panic!() };
    assert_eq!(doc.body[0].as_para().unwrap().text, text);
    para.clone()
}

/// [`paragraph`] justified, its text in East Asian language `lang` (`w:lang/@w:eastAsia`).
fn paragraph_in(text: &str, width: f32, lang: Option<&str>) -> Arc<ParaLayout> {
    let mut doc = Document::from_text(text);
    doc.styles.default_chr.size = Some(20.0);
    doc.styles.default_chr.lang_ea = lang.map(str::to_string);
    doc.last_section.page_w = width + 144.0;
    let props = &mut doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props;
    props.align = Some(Align::Justify);
    props.auto_space_de = Some(false);
    props.auto_space_dn = Some(false);
    let result = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    let Placed::Lines { para, .. } = &result.pages[0].items[0] else { panic!() };
    para.clone()
}

fn width(paragraph: &ParaLayout) -> f32 {
    let line = &paragraph.lines[0];
    line.end_x() - line.xs[0]
}

#[test]
fn punctuation_compression_fits_a_small_overflow_without_changing_glyph_advances() {
    let text = "A\u{ff1a}B\u{ff0c}C";
    let plain = paragraph(text, 468.0, None, Align::Left);
    let available = width(&plain) - 4.0;
    for alignment in [Align::Left, Align::Center, Align::Right, Align::Justify] {
        assert_eq!(paragraph(text, available, None, alignment).lines.len(), 2);
        let compressed = paragraph(text, available, Some("compressPunctuation"), alignment);
        assert_eq!(compressed.lines.len(), 1, "{alignment:?}: full-width punctuation should absorb the small overflow");
        assert!((compressed.lines[0].end_x() - available).abs() < 0.01);
        assert_eq!(
            compressed.clusters.iter().map(|cluster| cluster.adv).collect::<Vec<_>>(),
            plain.clusters.iter().map(|cluster| cluster.adv).collect::<Vec<_>>()
        );
    }
}

#[test]
fn punctuation_compression_leaves_roomy_lines_and_ascii_punctuation_alone() {
    for text in ["A\u{ff1a}B\u{ff0c}C", "A:B,C"] {
        let plain = paragraph(text, 468.0, None, Align::Left);
        let compressed = paragraph(text, 468.0, Some("compressPunctuation"), Align::Left);
        assert_eq!(plain.lines[0].xs, compressed.lines[0].xs);
    }
    let text = "A:B,C";
    let available = width(&paragraph(text, 468.0, None, Align::Left)) - 1.0;
    assert_eq!(
        paragraph(text, available, None, Align::Left).lines.len(),
        paragraph(text, available, Some("compressPunctuation"), Align::Left).lines.len()
    );
}

#[test]
fn punctuation_compression_has_a_bounded_capacity_and_honors_the_setting() {
    let text = "A\u{ff1a}B\u{ff0c}C";
    let natural = paragraph(text, 468.0, None, Align::Left);
    let capacity = natural.clusters[1].adv.min(20.0) / 2.0 + natural.clusters[3].adv.min(20.0) / 2.0;
    let available = width(&natural) - capacity - 2.0;
    assert!(paragraph(text, available, Some("compressPunctuation"), Align::Left).lines.len() > 1);
    let available = width(&natural) - 4.0;
    assert_eq!(paragraph(text, available, Some("doNotCompress"), Align::Left).lines.len(), 2);
    assert_eq!(paragraph(text, available, Some("compressPunctuationAndJapaneseKana"), Align::Left).lines.len(), 1);
}

#[test]
fn punctuation_compression_toggle_invalidates_paragraph_layout_cache() {
    let text = "A\u{ff1a}B\u{ff0c}C";
    let available = width(&paragraph(text, 468.0, None, Align::Left)) - 4.0;
    let mut doc = Document::from_text(text);
    doc.styles.default_chr.size = Some(20.0);
    doc.last_section.page_w = available + 144.0;
    let props = &mut doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props;
    props.auto_space_de = Some(false);
    props.auto_space_dn = Some(false);
    let mut cache = LayoutCache::new();
    let line_count = |doc: &Document, cache: &mut LayoutCache| {
        let result = layout(doc, cache, &LayoutOptions::default());
        let Placed::Lines { para, .. } = &result.pages[0].items[0] else { panic!() };
        para.lines.len()
    };
    assert_eq!(line_count(&doc, &mut cache), 2);
    doc.settings.char_spacing = Some("compressPunctuation".into());
    assert_eq!(line_count(&doc, &mut cache), 1);
    doc.settings.char_spacing = None;
    assert_eq!(line_count(&doc, &mut cache), 2);
}

#[test]
fn punctuation_compression_squeezes_each_mark_by_a_fifth_of_its_size() {
    // (The name is the one cases/regressions/issue-punctuation-squeeze runs.) WPS for Mac
    // (synthetic calibration of this text): a line's full-width marks give up half the largest
    // one's size in all. Two marks at 20 pt absorb 10 pt of overflow but not 11 pt.
    let text = "A\u{ff1a}B\u{ff0c}C";
    let natural = width(&paragraph(text, 468.0, None, Align::Left));
    assert_eq!(paragraph(text, natural - 9.9, Some("compressPunctuation"), Align::Left).lines.len(), 1);
    assert_eq!(paragraph(text, natural - 11.0, Some("compressPunctuation"), Align::Left).lines.len(), 2);
}

#[test]
fn punctuation_hangs_past_the_right_edge_at_a_line_end() {
    // WPS references: a comma that ends a full line of Chinese text hangs past the right margin
    // (paragraph `overflowPunct`, on by default) instead of wrapping.
    let text = "\u{4e00}\u{4e8c}\u{4e09}\u{56db}\u{ff0c}\u{4e94}\u{516d}";
    let four = width(&paragraph("\u{4e00}\u{4e8c}\u{4e09}\u{56db}", 468.0, None, Align::Left));
    let available = four + 2.0;
    let hung = paragraph_in(text, available, Some("zh-CN"));
    let first = &hung.lines[0];
    assert_eq!(first.stop - first.start, "\u{4e00}\u{4e8c}\u{4e09}\u{56db}\u{ff0c}".len(), "the comma ends the first line");
    let (start, end) = (first.xs[4] - first.xs[0], first.xs[5] - first.xs[0]);
    assert!(start <= available + 0.01 && end > available, "the comma starts inside the margin and hangs past it: {:?}", first.xs);
    // Without overflowing punctuation the comma can't end that line.
    let mut doc = Document::from_text(text);
    doc.styles.default_chr.size = Some(20.0);
    doc.styles.default_chr.lang_ea = Some("zh-CN".to_string());
    doc.last_section.page_w = available + 144.0;
    let props = &mut doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props;
    props.overflow_punct = Some(false);
    props.auto_space_de = Some(false);
    props.auto_space_dn = Some(false);
    let result = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    let Placed::Lines { para, .. } = &result.pages[0].items[0] else { panic!() };
    assert!(para.lines[0].stop - para.lines[0].start < "\u{4e00}\u{4e8c}\u{4e09}\u{56db}\u{ff0c}".len());
}

#[test]
fn punctuation_hangs_only_in_east_asian_language_text() {
    // WPS references (the same text made by Word with `w:eastAsia="zh-CN"` runs, and by WPS
    // with an `en-US` East Asian language): the comma hangs only where the text's East Asian
    // language is Chinese, Japanese or Korean; with another language or none it wraps.
    let text = "\u{4e00}\u{4e8c}\u{4e09}\u{56db}\u{ff0c}\u{4e94}\u{516d}";
    let four = width(&paragraph("\u{4e00}\u{4e8c}\u{4e09}\u{56db}", 468.0, None, Align::Left));
    let ends = |lang: Option<&str>| {
        let para = paragraph_in(text, four + 2.0, lang);
        para.lines[0].stop - para.lines[0].start == "\u{4e00}\u{4e8c}\u{4e09}\u{56db}\u{ff0c}".len()
    };
    assert!(ends(Some("zh-CN")) && ends(Some("ja-JP")) && ends(Some("ko-KR")));
    assert!(!ends(Some("en-US")) && !ends(None));
}

/// [`paragraph`] with automatic East Asian–Latin spacing on, left aligned.
fn spaced(text: &str, width: f32, mode: Option<&str>) -> Arc<ParaLayout> {
    let mut doc = Document::from_text(text);
    doc.styles.default_chr.size = Some(20.0);
    doc.settings.char_spacing = mode.map(str::to_string);
    doc.last_section.page_w = width + 144.0;
    let props = &mut doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props;
    props.align = Some(Align::Left);
    props.auto_space_de = Some(true);
    props.auto_space_dn = Some(true);
    let result = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    let Placed::Lines { para, .. } = &result.pages[0].items[0] else { panic!() };
    para.clone()
}

#[test]
fn compression_takes_back_part_of_each_gap_between_chinese_and_digits() {
    // WPS for Mac (synthetic calibration at 10.5 pt, scaled to 20 pt here): with punctuation
    // compression a quarter-em gap between Chinese and a digit gives up 0.48 of itself to fit a
    // line, so one 5 pt gap takes back 2.3 pt but not 2.5 pt, and the gaps close up on the line.
    let text = "-\u{5c0f}\u{6570}\u{4f4d}\u{6570}3";
    let natural = width(&spaced(text, 468.0, None));
    assert_eq!(spaced(text, natural - 2.3, None).lines.len(), 2, "without compression the gap stays whole");
    let fitted = spaced(text, natural - 2.3, Some("compressPunctuation"));
    assert_eq!(fitted.lines.len(), 1);
    assert!((width(&fitted) - (natural - 2.3)).abs() < 0.01, "{}", width(&fitted));
    assert_eq!(spaced(text, natural - 2.5, Some("compressPunctuation")).lines.len(), 2);
    // A final ideograph may reach no more than 0.35 of itself past the edge for the gaps to bring
    // back, however many gaps the line has; a final digit, as much as its advance.
    let text = "\u{6570}\u{6570}3\u{6570}\u{6570}3\u{6570}";
    let natural = width(&spaced(text, 468.0, None));
    assert_eq!(spaced(text, natural - 6.8, Some("compressPunctuation")).lines.len(), 1);
    assert_eq!(spaced(text, natural - 7.2, Some("compressPunctuation")).lines.len(), 2);
    // A line with full-width punctuation compresses only the punctuation: a comma's 10 pt.
    let text = "\u{6570}\u{ff0c}\u{6570}3\u{6570}";
    let natural = width(&spaced(text, 468.0, None));
    assert_eq!(spaced(text, natural - 9.9, Some("compressPunctuation")).lines.len(), 1);
    assert_eq!(spaced(text, natural - 10.5, Some("compressPunctuation")).lines.len(), 2);
}
