use super::*;
use kition_doc_doc::para::InlineObject;
use kition_doc_doc::props::{Align, Border, BorderStyle, Borders, ParaProps, TextDirection};
use kition_doc_doc::{Pos, Table};

fn lay(doc: &Document) -> DocLayout {
    let mut c = LayoutCache::new();
    layout(doc, &mut c, &LayoutOptions::default())
}

#[test]
fn keep_next_reserves_collapsed_spacing_without_reserving_contextual_gaps() {
    for (contextual, expected_page) in [(false, 1), (true, 0)] {
        let mut doc = Document::from_text("Filler\nHeading\nBody");
        doc.last_section.page_h = 140.0;
        doc.last_section.margin_top = 10.0;
        doc.last_section.margin_bottom = 10.0;
        for index in 0..3 {
            doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props = ParaProps {
                space_before: Some(0.0),
                space_after: Some(if index == 1 { 20.0 } else { 0.0 }),
                line_spacing: Some(kition_doc_doc::props::LineSpacing::Exactly(if index == 0 { 80.0 } else { 20.0 })),
                keep_next: Some(index == 1),
                contextual_spacing: Some(index == 1 && contextual),
                ..Default::default()
            };
        }
        let layout = lay(&doc);
        let page_of = |block| {
            layout
                .pages
                .iter()
                .position(|page| {
                    page.items.iter().any(|item| matches!(item, Placed::Lines { story: StoryRef::Body, path, .. } if *path == Path::top(block)))
                })
                .unwrap()
        };
        assert_eq!(page_of(0), 0);
        assert_eq!(page_of(1), expected_page, "heading must reserve its effective paragraph gap");
        assert_eq!(page_of(2), expected_page, "heading and following line must remain together");
    }
}

#[test]
fn keep_next_reserves_collapsed_spacing_before_a_table() {
    let mut doc = Document::from_text("Filler\nHeading");
    doc.last_section.page_h = 140.0;
    doc.last_section.margin_top = 10.0;
    doc.last_section.margin_bottom = 10.0;
    for index in 0..2 {
        doc.para_mut(StoryRef::Body, &Path::top(index)).unwrap().props = ParaProps {
            space_before: Some(0.0),
            space_after: Some(if index == 1 { 20.0 } else { 0.0 }),
            line_spacing: Some(kition_doc_doc::props::LineSpacing::Exactly(if index == 0 { 76.0 } else { 20.0 })),
            keep_next: Some(index == 1),
            ..Default::default()
        };
    }
    doc.body.push(Arc::new(Block::Table(Table::new(1, 1, 468.0))));
    let layout = lay(&doc);
    let heading = layout
        .pages
        .iter()
        .position(|page| page.items.iter().any(|item| matches!(item, Placed::Lines { path, .. } if *path == Path::top(1))))
        .unwrap();
    let table = layout
        .pages
        .iter()
        .position(|page| page.items.iter().any(|item| matches!(item, Placed::Cell { table, .. } if *table == Path::top(2))))
        .unwrap();
    assert_eq!(heading, 1);
    assert_eq!(table, heading);
}

#[test]
fn paragraph_spacing_collapses_before_and_after_in_body_and_cells() {
    let mut d = Document::from_text("First\nSecond");
    d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props = ParaProps { space_after: Some(12.0), space_before: Some(0.0), ..Default::default() };
    d.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props = ParaProps { space_before: Some(4.0), space_after: Some(0.0), ..Default::default() };
    let gap = |items: &[Placed]| {
        let lines: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                Placed::Lines { y, para, .. } => Some((*y, para.height)),
                _ => None,
            })
            .collect();
        lines[1].0 - lines[0].0 - lines[0].1
    };
    assert!((gap(&lay(&d).pages[0].items) - 12.0).abs() < 0.01);
    let mut table = Table::new(1, 1, 468.0);
    table.rows[0].cells[0].blocks = d.body.clone();
    d.body = vec![Arc::new(Block::Table(table))];
    assert!((gap(&lay(&d).pages[0].items) - 12.0).abs() < 0.01);
}

#[test]
fn paragraph_spacing_contextual_flag_suppresses_only_its_own_side() {
    for (first_contextual, second_contextual, expected) in [(true, false, 4.0), (false, true, 12.0), (true, true, 0.0)] {
        let mut d = Document::from_text("First\nSecond");
        d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props =
            ParaProps { space_before: Some(0.0), space_after: Some(12.0), contextual_spacing: Some(first_contextual), ..Default::default() };
        d.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props =
            ParaProps { space_before: Some(4.0), space_after: Some(0.0), contextual_spacing: Some(second_contextual), ..Default::default() };
        let layout = lay(&d);
        let lines: Vec<_> = layout.pages[0]
            .items
            .iter()
            .filter_map(|item| match item {
                Placed::Lines { y, para, .. } => Some((*y, para.height)),
                _ => None,
            })
            .collect();
        let actual = lines[1].0 - lines[0].0 - lines[0].1;
        assert!((actual - expected).abs() < 0.01, "context flags {first_contextual}/{second_contextual}: {actual}");
    }
}

#[test]
fn east_asian_font_slot_is_independent_of_latin_font() {
    let mut d = Document::from_text("\u{3000}A\u{3000}");
    d.styles.default_chr.font = Some("Source Serif 4".into());
    d.styles.default_chr.font_ea = Some("Source Sans 3".into());
    let layout = lay(&d);
    let Placed::Lines { para, .. } = &layout.pages[0].items[0] else { panic!() };
    let family = |offset| {
        let cluster = para.clusters.iter().find(|c| c.start == offset).unwrap();
        para.styles[cluster.style as usize].face.family.as_str()
    };
    assert_eq!(family(0), "Source Sans 3");
    assert_eq!(family(3), "Source Serif 4");
    assert_eq!(family(4), "Source Sans 3");
}

fn lines_of(l: &DocLayout) -> usize {
    l.pages.iter().flat_map(|p| p.items.iter()).map(|it| if let Placed::Lines { l0, l1, .. } = it { l1 - l0 } else { 0 }).sum()
}

#[test]
fn empty_doc_has_one_page() {
    let d = Document::new();
    let l = lay(&d);
    assert_eq!(l.pages.len(), 1);
    assert_eq!(lines_of(&l), 1);
    let c = l.caret(&Pos::body(0, 0)).unwrap();
    assert_eq!(c.page, 0);
    assert!((c.x - 72.0).abs() < 0.5, "{c:?}");
    assert!(c.top >= 72.0 && c.height > 8.0);
}

#[test]
fn long_paragraph_wraps_and_paginates() {
    let text = "The quick brown fox jumps over the lazy dog. ".repeat(400);
    let d = Document::from_text(&text);
    let l = lay(&d);
    assert!(l.pages.len() >= 3, "pages {}", l.pages.len());
    // Every line fits the text width.
    for p in &l.pages {
        for it in &p.items {
            if let Placed::Lines { para, l0, l1, x, .. } = it {
                for li in *l0..*l1 {
                    let line = &para.lines[li];
                    let w = para.x_of(li, line.stop).unwrap() + x;
                    assert!(w <= 612.0 - 72.0 + 30.0, "line too wide: {w}");
                }
            }
        }
    }
}

#[test]
fn hit_and_caret_agree() {
    let d = Document::from_text("Hello world, this is Kition Doc.\nSecond paragraph here.");
    let l = lay(&d);
    for off in [0, 5, 12, 31] {
        let pos = Pos::body(0, off);
        let c = l.caret(&pos).unwrap();
        let back = l.hit(c.page, c.x + 0.1, c.top + c.height / 2.0, StoryRef::Body).unwrap();
        assert_eq!(back, pos, "off {off}");
    }
    let c1 = l.caret(&Pos::body(1, 0)).unwrap();
    let (down, _) = l.vertical(&Pos::body(0, 0), None, 1, 0).unwrap();
    assert_eq!(down.path, Path::top(1));
    assert!(c1.top > l.caret(&Pos::body(0, 0)).unwrap().top);
    let (up, _) = l.vertical(&Pos::body(1, 3), None, -1, 0).unwrap();
    assert_eq!(up.path, Path::top(0));
}

#[test]
fn deletions_leave_the_final_text_layout() {
    // "Keep " + deleted "DELETEDTEXT " (5..17) + inserted "INSERTED".
    let mut d = Document::from_text("Keep DELETEDTEXT INSERTED");
    d.format_range(&Pos::body(0, 5), &Pos::body(0, 17), &|c| c.del = Some(0)).unwrap();
    d.format_range(&Pos::body(0, 17), &Pos::body(0, 25), &|c| c.ins = Some(0)).unwrap();
    let texts = |l: &DocLayout, markup: bool| -> (String, usize) {
        let items = display::page_display(&d, &l.pages[0], &display::DisplayOptions { markup, ..Default::default() });
        let text = items.iter().filter_map(|i| if let display::Draw::Glyphs { text, .. } = i { Some(text.as_str()) } else { None }).collect();
        (text, items.iter().filter(|i| matches!(i, display::Draw::Line { .. })).count())
    };
    // Markup: the deletion is laid out and struck through, the insertion underlined, and the
    // changed line has a bar in the margin.
    let full = lay(&d);
    assert_eq!(texts(&full, true), ("Keep DELETEDTEXT INSERTED".into(), 3));
    // Without markup a full layout still never prints the deletion as plain text.
    assert!(!texts(&full, false).0.contains("DELETED"));
    // The final layout gives the deletion no width.
    let fin = layout(&d, &mut LayoutCache::new(), &LayoutOptions { hide_deleted: true, ..Default::default() });
    assert_eq!(texts(&fin, false), ("Keep INSERTED".into(), 0));
    let at = |off| fin.caret(&Pos::body(0, off)).unwrap();
    assert!((at(5).x - at(17).x).abs() < 0.01, "{:?} {:?}", at(5), at(17));
    // Every offset maps to a caret, and clicks land outside the hidden text.
    for off in 0..=25 {
        let c = at(off);
        let back = fin.hit(c.page, c.x + 0.1, c.top + c.height / 2.0, StoryRef::Body).unwrap();
        assert!(back.off <= 5 || back.off >= 17, "off {off} hit {back:?}");
    }
}

#[test]
fn deleted_soft_hyphen_is_no_hyphenation_point_without_markup() {
    // "ab\u{ad}cdef" in a column where "ab-" fits but the whole word does not; only the soft
    // hyphen is track-deleted.
    let mut d = Document::from_text("ab\u{ad}cdef");
    d.format_range(&Pos::body(0, 2), &Pos::body(0, 4), &|c| c.del = Some(0)).unwrap();
    d.format_paragraphs(&Pos::body(0, 0), &Pos::body(0, 0), &|p| p.indent_right = Some(468.0 - 26.0)).unwrap();
    let hyphens = |hide_deleted: bool| {
        let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions { hide_deleted, ..Default::default() });
        let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
        para.lines.iter().filter(|l| l.hyphen.is_some()).count()
    };
    // With markup the deleted soft hyphen is still there, and the word breaks at it.
    assert_eq!(hyphens(false), 1);
    // Final text: the soft hyphen is gone, so is the hyphen.
    assert_eq!(hyphens(true), 0);
}

#[test]
fn deleted_text_adds_no_line_break_opportunity_without_markup() {
    // "xx ab\u{ad}cdef" in a column where "xx ab-" and "abcdef" fit but "xx abcdef" does not; only
    // the soft hyphen (5..7) is track-deleted.
    let mut d = Document::from_text("xx ab\u{ad}cdef");
    d.format_range(&Pos::body(0, 5), &Pos::body(0, 7), &|c| c.del = Some(0)).unwrap();
    d.format_paragraphs(&Pos::body(0, 0), &Pos::body(0, 0), &|p| p.indent_right = Some(468.0 - 40.0)).unwrap();
    let line_starts = |hide_deleted: bool| {
        let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions { hide_deleted, ..Default::default() });
        let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
        para.lines.iter().map(|l| para.clusters.get(l.c0).map_or(usize::MAX, |c| c.start)).collect::<Vec<_>>()
    };
    // With markup the word breaks at the (deleted) soft hyphen.
    assert_eq!(line_starts(false), vec![0, 7]);
    // Final text: "abcdef" is one word, so the line breaks at the space before it.
    assert_eq!(line_starts(true), vec![0, 3]);
}

#[test]
fn hidden_soft_hyphen_is_no_hyphenation_point_when_hidden_text_is_not_shown() {
    // As with deletions: "ab\u{ad}cdef" where only the soft hyphen is hidden text.
    let mut d = Document::from_text("ab\u{ad}cdef");
    d.format_range(&Pos::body(0, 2), &Pos::body(0, 4), &|c| c.hidden = Some(true)).unwrap();
    d.format_paragraphs(&Pos::body(0, 0), &Pos::body(0, 0), &|p| p.indent_right = Some(468.0 - 26.0)).unwrap();
    let hyphens = |show_hidden: bool| {
        let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions { show_hidden, ..Default::default() });
        let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
        para.lines.iter().filter(|l| l.hyphen.is_some()).count()
    };
    // Hidden text shown: the soft hyphen is there, and the word breaks at it.
    assert_eq!(hyphens(true), 1);
    // Not shown (as printed): no soft hyphen, so no hyphen. Word breaks "abc" / "def" here.
    assert_eq!(hyphens(false), 0);
}

#[test]
fn hidden_text_adds_no_line_break_opportunity_when_not_shown() {
    // "xx ab" + hidden soft hyphen or space + "cdef", in a column where "xx ab-" and "abcdef" fit
    // but "xx abcdef" does not. Measured in Word (hidden text not printed): "xx" / "abcdef" both times.
    for hidden in ["\u{ad}", " "] {
        let mut d = Document::from_text(&format!("xx ab{hidden}cdef"));
        let end = 5 + hidden.len();
        d.format_range(&Pos::body(0, 5), &Pos::body(0, end), &|c| c.hidden = Some(true)).unwrap();
        d.format_paragraphs(&Pos::body(0, 0), &Pos::body(0, 0), &|p| p.indent_right = Some(468.0 - 40.0)).unwrap();
        let line_starts = |show_hidden: bool| {
            let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions { show_hidden, ..Default::default() });
            let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
            para.lines.iter().map(|l| para.clusters.get(l.c0).map_or(usize::MAX, |c| c.start)).collect::<Vec<_>>()
        };
        // Shown, the word breaks at the hidden soft hyphen or space.
        assert_eq!(line_starts(true), vec![0, end], "{hidden:?}");
        // Not shown, "abcdef" is one word, so the line breaks at the space before it.
        assert_eq!(line_starts(false), vec![0, 3], "{hidden:?}");
    }
}

/// How much longer laying out `doc(4n)` takes than `doc(n)` (fastest of three each, after a
/// warm-up): ~4 when linear, ~16 when quadratic, whatever the machine.
/// How much bigger the second document is in the linear-time tests. At 8x a linear layout takes
/// about 8x as long and a quadratic one about 64x, so a limit of 24x stays far from both, however
/// busy the machine running the tests is.
const SCALE: usize = 8;

fn layout_time_ratio(doc: impl Fn(usize) -> Document, n: usize, opts: &LayoutOptions) -> (f64, f64, f64) {
    let fastest = |d: &Document| {
        (0..3)
            .map(|_| {
                let t = std::time::Instant::now();
                layout(d, &mut LayoutCache::new(), opts);
                t.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    };
    let (small, big) = (doc(n), doc(SCALE * n));
    layout(&small, &mut LayoutCache::new(), opts);
    let (t1, tk) = (fastest(&small), fastest(&big));
    (t1, tk, tk / t1.max(1e-9))
}

#[test]
fn hidden_separators_with_automatic_hyphenation_lay_out_in_linear_time() {
    // "hyphenation " n times with every space hidden: one joined word of 11n letters reaches the
    // hyphenator, whose limit checks once rescanned the word for every candidate point.
    let doc = |n: usize| {
        let mut d = Document::from_text(&"hyphenation ".repeat(n));
        d.settings.auto_hyphenation = true;
        for k in 0..n {
            let at = 12 * k + 11;
            d.format_range(&Pos::body(0, at), &Pos::body(0, at + 1), &|c| c.hidden = Some(true)).unwrap();
        }
        d
    };
    let (t1, tk, ratio) = layout_time_ratio(doc, 800, &LayoutOptions::default());
    eprintln!("t(n) {t1:.4} s, t(8n) {tk:.4} s, ratio {ratio:.1}");
    assert!(ratio < 24.0, "8x the text took {ratio:.1}x the time ({t1:.4} s → {tk:.4} s): not linear");
}

#[test]
fn many_hidden_float_anchors_lay_out_in_linear_time() {
    // n square-wrapped shapes, each anchored in its own hidden run (bold alternating, so runs do not
    // merge), markup on: finding each anchor's formatting once rescanned the runs from the start.
    let doc = |n: usize| {
        let float = kition_doc_doc::para::Float {
            wrap: kition_doc_doc::para::Wrap::Square,
            h_rel: kition_doc_doc::para::Anchor::Column,
            v_rel: kition_doc_doc::para::Anchor::Paragraph,
            x: 0.0,
            y: 0.0,
            dist: 9.0,
            dist_top: 9.0,
            dist_bottom: 9.0,
            ..Default::default()
        };
        let shape = InlineObject::Shape {
            vml: None,
            kind: kition_doc_doc::para::ShapeKind::Rectangle,
            w: 20.0,
            h: 20.0,
            fill: None,
            stroke: None,
            stroke_width: 1.0,
            float,
            story: None,
            freeform: None,
            effects: Default::default(),
            geom: None,
            uid: 0,
            gradient: None,
            connection: None,
            warp: None,
            body: None,
            dash: None,
        };
        let obj = kition_doc_doc::para::OBJ.to_string();
        let mut p = kition_doc_doc::Paragraph::with_text("Text ", Default::default());
        p.text.push_str(&obj.repeat(n));
        p.objects = vec![shape; n];
        p.runs = std::iter::once(kition_doc_doc::Run { len: 5, props: Default::default() })
            .chain((0..n).map(|i| kition_doc_doc::Run {
                len: obj.len(),
                props: kition_doc_doc::CharProps { hidden: Some(true), bold: Some(i % 2 == 0), ..Default::default() },
            }))
            .collect();
        let mut d = Document::from_text("");
        d.body = vec![kition_doc_doc::para_block(p)];
        d
    };
    // Correct first: hidden anchors place no shape, shown ones do.
    let shapes = |show_hidden: bool| {
        let l = layout(&doc(50), &mut LayoutCache::new(), &LayoutOptions { show_hidden, ..Default::default() });
        l.pages.iter().flat_map(|p| p.items.iter()).filter(|i| matches!(i, Placed::Shape { .. })).count()
    };
    assert_eq!((shapes(false), shapes(true)), (0, 50));
    let (t1, tk, ratio) = layout_time_ratio(doc, 10_000, &LayoutOptions::default());
    eprintln!("t(n) {t1:.4} s, t(8n) {tk:.4} s, ratio {ratio:.1}");
    assert!(ratio < 24.0, "8x the anchors took {ratio:.1}x the time ({t1:.4} s → {tk:.4} s): not linear");
}

#[test]
fn fragmented_hidden_text_lays_out_like_the_visible_text() {
    // Visible words with a hidden multi-byte char after every visible one, formatted in alternating
    // runs (bold on and off) so no two left-out runs are adjacent or merge. The plain paragraph has
    // the same visible runs without the hidden chars, so both lay out the same widths whatever font
    // the default font resolves to.
    // Soft hyphens give the hyphenation points (automatic hyphenation is off, as in WPS for Mac).
    let words = "Hyph\u{ad}en\u{ad}ation wraps inter\u{ad}na\u{ad}tional words across nar\u{ad}row col\u{ad}umns of text ".repeat(6);
    let mut d = Document::from_text("");
    d.settings.auto_hyphenation = true;
    let mut plain = Document::from_text("");
    plain.settings.auto_hyphenation = true;
    let mut text = String::new();
    let mut kept = Vec::new();
    for (i, c) in words.chars().enumerate() {
        kept.push(text.len());
        text.push(c);
        let at = text.len();
        text.push('é');
        let n = d.para(StoryRef::Body, &Path::top(0)).unwrap().len();
        let bold = kition_doc_doc::CharProps { bold: Some(i % 2 == 0), ..Default::default() };
        d.insert_text(&Pos::body(0, n), &c.to_string(), &bold).unwrap();
        let hidden = kition_doc_doc::CharProps { hidden: Some(true), bold: Some(i % 2 == 1), ..Default::default() };
        d.insert_text(&Pos::body(0, at), "é", &hidden).unwrap();
        let n = plain.para(StoryRef::Body, &Path::top(0)).unwrap().len();
        plain.insert_text(&Pos::body(0, n), &c.to_string(), &bold).unwrap();
    }
    assert_eq!(d.para(StoryRef::Body, &Path::top(0)).unwrap().text, text);
    assert_eq!(plain.para(StoryRef::Body, &Path::top(0)).unwrap().text, words);
    for doc in [&mut d, &mut plain] {
        doc.format_paragraphs(&Pos::body(0, 0), &Pos::body(0, 0), &|p| p.indent_right = Some(468.0 - 90.0)).unwrap();
    }
    // Line starts and hyphenation points, as paragraph offsets.
    let shape = |d: &Document| {
        let l = layout(d, &mut LayoutCache::new(), &LayoutOptions::default());
        let paras: Vec<_> = l
            .pages
            .iter()
            .flat_map(|p| p.items.iter())
            .filter_map(|i| if let Placed::Lines { para, .. } = i { Some(para.clone()) } else { None })
            .collect();
        let para = &paras[0];
        let starts: Vec<usize> = para.lines.iter().map(|l| para.clusters.get(l.c0).map_or(usize::MAX, |c| c.start)).collect();
        let hyph: Vec<usize> = para.hyph_after.iter().map(|k| para.clusters[*k as usize].end).collect();
        (starts, hyph)
    };
    let (starts, hyph) = shape(&plain);
    assert!(starts.len() > 5 && !hyph.is_empty(), "{starts:?} {hyph:?}");
    // Mapped to the fragmented paragraph: a line starts after the previous visible char (with the
    // hidden one that follows it, which takes no room), a hyphen point is after its char.
    let chars: Vec<char> = words.chars().collect();
    // The fragmented paragraph's offset right after the visible char that ends at plain byte `b`.
    let after = |b: usize| {
        let i = words[..b].chars().count() - 1;
        kept[i] + chars[i].len_utf8()
    };
    let want_starts: Vec<usize> = starts.iter().map(|s| if *s == 0 { 0 } else { after(*s) }).collect();
    let want_hyph: Vec<usize> = hyph.iter().map(|e| after(*e)).collect();
    assert_eq!(shape(&d), (want_starts, want_hyph));
}

#[test]
fn automatic_hyphenation_sees_only_the_text_laid_out() {
    // Paragraph-text ends of the clusters after which a line may end with a hyphen.
    let hyph_ends = |d: &Document, opts: &LayoutOptions| {
        let l = layout(d, &mut LayoutCache::new(), opts);
        let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
        para.hyph_after.iter().map(|k| para.clusters[*k as usize].end).collect::<Vec<_>>()
    };
    let doc = |text: &str| {
        let mut d = Document::from_text(text);
        d.settings.auto_hyphenation = true;
        d
    };
    // (text, hidden bytes, deleted bytes, the text as laid out without them).
    let cases = [
        // A hidden prefix: "table" must not break after its first letter as "acceptable" may.
        ("acceptable", 0..5, 0..0, "table"),
        // A hidden separator joins two words into one.
        ("foot ball", 4..5, 0..0, "football"),
        // Hidden text and a tracked deletion together.
        ("accXeptYable", 7..8, 3..4, "acceptable"),
    ];
    let hidden_opts = LayoutOptions { hide_deleted: true, ..Default::default() };
    for (text, hidden, deleted, visible) in cases {
        let mut d = doc(text);
        d.format_range(&Pos::body(0, hidden.start), &Pos::body(0, hidden.end), &|c| c.hidden = Some(true)).unwrap();
        if !deleted.is_empty() {
            d.format_range(&Pos::body(0, deleted.start), &Pos::body(0, deleted.end), &|c| c.del = Some(0)).unwrap();
        }
        // The same points as the visible text alone, at the matching paragraph offsets.
        let kept: Vec<usize> = (0..text.len()).filter(|i| !hidden.contains(i) && !deleted.contains(i)).collect();
        let want: Vec<usize> = hyph_ends(&doc(visible), &hidden_opts).iter().map(|e| kept[e - 1] + 1).collect();
        assert_eq!(hyph_ends(&d, &hidden_opts), want, "{text:?} as {visible:?}");
        // Hidden text shown and markup shown: the whole text is hyphenated as before.
        let all = LayoutOptions { show_hidden: true, ..Default::default() };
        assert_eq!(hyph_ends(&d, &all), hyph_ends(&doc(text), &all), "{text:?} shown");
    }
}

#[test]
fn page_break_char_starts_new_page() {
    let mut d = Document::from_text("one");
    d.insert_text(&Pos::body(0, 3), "\u{000C}", &Default::default()).unwrap();
    let l = lay(&d);
    assert_eq!(l.pages.len(), 2);
}

#[test]
fn a_page_break_ending_its_paragraph_keeps_the_mark_on_its_page() {
    // WPS references (and Word 2013+): a paragraph whose last character is a page break keeps
    // its mark on the break's line, so the next paragraph starts the next page at its top.
    let mut d = Document::from_text("one\ntwo");
    d.insert_text(&Pos::body(0, 3), "\u{000C}", &Default::default()).unwrap();
    let l = lay(&d);
    assert_eq!(l.pages.len(), 2);
    let first = l.pages[1]
        .items
        .iter()
        .find_map(|i| if let Placed::Lines { path, y, para, .. } = i { Some((path.0.clone(), y + para.lines[0].top)) } else { None });
    let (path, top) = first.expect("a line on page two");
    assert_eq!(path, vec![1], "page two starts with the next paragraph");
    assert!((top - 72.0).abs() < 0.01, "{top}");
}

#[test]
fn a_page_break_alone_on_a_full_page_makes_no_blank_page() {
    // WPS references: a page filled to its last line, then a paragraph holding only a page
    // break: the break stays at the foot of the full page, and the next paragraph starts the
    // second page, not a third.
    let doc = |n: usize, tail: &str| {
        let lines: Vec<String> = (0..n).map(|i| format!("Line {i}")).collect();
        let mut d = Document::from_text(&format!("{}{tail}", lines.join("\n")));
        for i in 0..d.body.len() {
            let props = &mut d.para_mut(StoryRef::Body, &Path::top(i)).unwrap().props;
            props.space_before = Some(0.0);
            props.space_after = Some(0.0);
        }
        d
    };
    // As many lines as fill the first page.
    let probe = lay(&doc(120, ""));
    let full = probe.pages[0].items.iter().filter(|i| matches!(i, Placed::Lines { .. })).count();
    let d = doc(full, "\n\u{000C}\nNext");
    let l = lay(&d);
    let page_of =
        |block: usize| l.pages.iter().position(|p| p.items.iter().any(|i| matches!(i, Placed::Lines { path, .. } if path.0 == [block as u32])));
    assert_eq!(page_of(full - 1), Some(0), "the first page is full");
    assert_eq!(page_of(full), Some(0), "the break stays at the foot of the full page");
    assert_eq!(page_of(full + 1), Some(1), "the next paragraph starts the second page: {} pages", l.pages.len());
}

#[test]
fn center_and_right_alignment() {
    let mut d = Document::from_text("abc\nabc\nabc");
    d.format_paragraphs(&Pos::body(1, 0), &Pos::body(1, 0), &|p| p.align = Some(Align::Center)).unwrap();
    d.format_paragraphs(&Pos::body(2, 0), &Pos::body(2, 0), &|p| p.align = Some(Align::Right)).unwrap();
    let l = lay(&d);
    let x0 = l.caret(&Pos::body(0, 0)).unwrap().x;
    let x1 = l.caret(&Pos::body(1, 0)).unwrap().x;
    let x2 = l.caret(&Pos::body(2, 3)).unwrap().x;
    assert!((x0 - 72.0).abs() < 0.5);
    assert!(x1 > 250.0 && x1 < 320.0, "{x1}");
    assert!((x2 - 540.0).abs() < 1.0, "{x2}");
}

#[test]
fn tabs_land_on_default_stops() {
    let d = Document::from_text("a\tb");
    let l = lay(&d);
    let xb = l.caret(&Pos::body(0, 2)).unwrap().x;
    assert!((xb - (72.0 + 36.0)).abs() < 0.5, "{xb}");
}

#[test]
fn right_tab_aligns_text_end() {
    let mut d = Document::from_text("left\tright");
    d.format_paragraphs(&Pos::body(0, 0), &Pos::body(0, 0), &|p| {
        p.tabs = Some(vec![kition_doc_doc::props::TabStop { pos: 400.0, align: kition_doc_doc::props::TabAlign::Right, leader: Default::default() }])
    })
    .unwrap();
    let l = lay(&d);
    let end = l.caret(&Pos::body(0, 10)).unwrap().x;
    assert!((end - 472.0).abs() < 1.0, "{end}");
}

#[test]
fn lists_get_labels_and_indent() {
    let mut d = Document::from_text("one\ntwo\nthree");
    let id = d.numbering.add_list(kition_doc_doc::ListKind::Numbered);
    d.format_paragraphs(&Pos::body(0, 0), &Pos::body(2, 0), &|p| p.numbering = Some(kition_doc_doc::props::NumRef { num: id, level: 0 })).unwrap();
    let l = lay(&d);
    let labels: Vec<String> = l.pages[0]
        .items
        .iter()
        .filter_map(|it| if let Placed::Lines { para, .. } = it { para.label.as_ref().map(|x| x.text.clone()) } else { None })
        .collect();
    assert_eq!(labels, vec!["1.", "2.", "3."]);
    let x = l.caret(&Pos::body(0, 0)).unwrap().x;
    assert!((x - 108.0).abs() < 0.5, "{x}");
}

#[test]
fn symbol_label_text_exports_unicode_without_changing_glyphs_or_source() {
    let mut doc = Document::from_text("Item");
    let number = doc.numbering.add_list(kition_doc_doc::ListKind::BulletChar('\u{F0B7}'));
    for definition in &mut doc.numbering.abstracts {
        definition.levels[0].chr.font = Some("Symbol".into());
    }
    doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props.numbering = Some(kition_doc_doc::props::NumRef { num: number, level: 0 });
    let mut layout = lay(&doc);
    let Placed::Lines { para, .. } = &mut layout.pages[0].items[0] else { panic!() };
    let para = Arc::make_mut(para);
    // An installed Symbol face can retain its private-use input after shaping. Preserve the
    // positioned glyphs and exercise that path independently of the machine's font inventory.
    let label = para.label.as_mut().unwrap();
    label.text = "\u{F0B7}".into();
    let glyph_ids: Vec<_> = label.glyphs.iter().map(|glyph| glyph.gid).collect();
    let draws = display::page_display(&doc, &layout.pages[0], &Default::default());
    let (text, glyphs) = draws
        .iter()
        .find_map(|draw| match draw {
            display::Draw::Glyphs { text, glyphs, .. } => Some((text, glyphs)),
            _ => None,
        })
        .unwrap();
    assert_eq!(text, "\u{2022}");
    assert_eq!(glyphs.iter().map(|glyph| glyph.0).collect::<Vec<_>>(), glyph_ids);
    assert_eq!(doc.numbering.abstracts[0].levels[0].text, "\u{F0B7}");
}

#[test]
fn symbol_font_bullet_without_the_font_draws_a_bullet() {
    // Word's default bullet: U+F0B7 in Symbol.
    let mut d = Document::from_text("one");
    let id = d.numbering.add_list(kition_doc_doc::ListKind::BulletChar('\u{F0B7}'));
    for a in &mut d.numbering.abstracts {
        if let Some(l) = a.levels.first_mut() {
            l.chr.font = Some("Symbol".into());
        }
    }
    d.format_paragraphs(&Pos::body(0, 0), &Pos::body(0, 0), &|p| p.numbering = Some(kition_doc_doc::props::NumRef { num: id, level: 0 })).unwrap();
    let l = lay(&d);
    let label = l.pages[0].items.iter().find_map(|it| if let Placed::Lines { para, .. } = it { para.label.clone() } else { None }).unwrap();
    // Where a real Symbol font with the private-use code is installed (Windows), the code stays.
    let symbol = kition_doc_fonts::word::resolve("Symbol", false, false);
    let expected = if symbol.substituted || !symbol.face.covers('\u{F0B7}') { "\u{2022}" } else { "\u{F0B7}" };
    assert_eq!(label.text, expected);
    assert_eq!(label.glyphs.len(), 1);
}

#[test]
fn tables_lay_out_cells() {
    let mut d = Document::from_text("before\nafter");
    let mut t = Table::new(2, 3, 468.0);
    t.rows[0].cells[1].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("cell text", Default::default()))];
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
    let l = lay(&d);
    let pos = Pos { story: StoryRef::Body, path: Path(vec![1, 0, 1, 0]), off: 0 };
    let c = l.caret(&pos).unwrap();
    assert!(c.x > 72.0 + 150.0 && c.x < 72.0 + 170.0, "{c:?}");
    let after = l.caret(&Pos::body(2, 0)).unwrap();
    assert!(after.top > c.top + 20.0);
    let rules = l.pages[0].items.iter().filter(|i| matches!(i, Placed::Rule { .. })).count();
    assert!(rules >= 12, "rules {rules}");
    assert!(l.cell_at(0, c.x, c.top + 2.0).is_some());
}

/// A 1×3 table after "before" whose middle cell holds `text` running `dir`.
fn turned_table(dir: TextDirection, text: &str, row: Option<f32>) -> Document {
    let mut d = Document::from_text("before\nafter");
    let mut t = Table::new(1, 3, 468.0);
    t.rows[0].cells[0].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("plain", Default::default()))];
    t.rows[0].cells[1].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(text, Default::default()))];
    t.rows[0].cells[1].props.text_direction = dir;
    if let Some(h) = row {
        t.rows[0].props.height = Some(h);
        t.rows[0].props.height_rule = kition_doc_doc::props::HeightRule::Exact;
    }
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
    d
}

fn turned_lines(l: &DocLayout) -> &Placed {
    l.pages[0].items.iter().find(|it| matches!(it, Placed::Lines { path, .. } if path.0 == [1, 0, 1, 0])).unwrap()
}

/// Table Layout › Text Direction: turned cell text runs along the cell's height, the row
/// grows to fit it, and caret, clicks and drawing follow the turn.
#[test]
fn turned_cell_text_runs_down_the_cell() {
    let text = "Turned cell text";
    let d = turned_table(TextDirection::Down, text, None);
    let l = lay(&d);
    let it = turned_lines(&l);
    assert!(matches!(it, Placed::Lines { turn: TextDirection::Down, l0: 0, l1: 1, .. }), "one unwrapped line: {it:?}");
    let b = it.turned_bounds().unwrap();
    // Tall and narrow, against the cell's right edge (the table's edge a cell margin left of the
    // margin, cell 2 spanning x 222.6..378.6, 5.4pt margins).
    let edge = 72.0 - 5.4;
    assert!(b.h > 60.0 && b.w < 20.0, "{b:?}");
    assert!((b.right() - (edge + 312.0 - 5.4)).abs() < 1.0 && b.x > edge + 156.0, "{b:?}");
    // The row grew to hold the text: the next paragraph starts below it.
    let after = l.caret(&Pos::body(2, 0)).unwrap();
    assert!(after.top > b.bottom(), "{after:?} vs {b:?}");
    let cell = l.pages[0].items.iter().find_map(|i| if let Placed::Cell { rect, cell: 1, .. } = i { Some(*rect) } else { None }).unwrap();
    assert!(cell.h >= b.h, "{cell:?}");
    // The caret lies across the page and moves down as the text goes on.
    let pos = |off| Pos { story: StoryRef::Body, path: Path(vec![1, 0, 1, 0]), off };
    let c0 = l.caret(&pos(0)).unwrap();
    let c1 = l.caret(&pos(text.len())).unwrap();
    assert!(c0.width > 5.0 && c0.height == 0.0, "{c0:?}");
    assert!((c0.top - b.y).abs() < 1.0 && c1.top > c0.top + 50.0, "{c0:?} {c1:?}");
    assert!(c0.x >= b.x - 0.5 && c0.x + c0.width <= b.right() + 0.5, "{c0:?} {b:?}");
    // A click on the turned text lands in it, by how far down the click is.
    let hit = l.hit(0, b.x + b.w / 2.0, b.y + b.h * 0.6, StoryRef::Body).unwrap();
    assert_eq!(hit.path, Path(vec![1, 0, 1, 0]));
    assert!(hit.off > 3 && hit.off < text.len(), "{hit:?}");
    // Selection highlights are turned too: tall, inside the text's area.
    let sel = l.selection_rects(&d, &pos(0), &pos(text.len()), 0);
    assert!(sel.iter().all(|(_, r)| r.h > r.w && r.x >= b.x - 1.0 && r.right() <= b.right() + 1.0), "{sel:?}");
    // Drawn in a turned frame.
    let draws = display::page_display(&d, &l.pages[0], &Default::default());
    let turned = draws.iter().find_map(|x| if let display::Draw::Turned { turn, items, .. } = x { Some((*turn, items)) } else { None });
    let (turn, items) = turned.expect("turned draw");
    assert_eq!(turn, TextDirection::Down);
    assert!(items.iter().any(|x| matches!(x, display::Draw::Glyphs { text, .. } if text.contains("Turned"))), "{items:?}");
}

#[test]
fn turned_up_cell_text_reads_bottom_to_top() {
    let text = "Bottom to top";
    let d = turned_table(TextDirection::Up, text, None);
    let l = lay(&d);
    let b = turned_lines(&l).turned_bounds().unwrap();
    // Against the cell's left edge (the table's edge a cell margin left of the margin); the text
    // starts at the bottom and climbs.
    assert!((b.x - (72.0 - 5.4 + 156.0 + 5.4)).abs() < 1.0, "{b:?}");
    let pos = |off| Pos { story: StoryRef::Body, path: Path(vec![1, 0, 1, 0]), off };
    let (c0, c1) = (l.caret(&pos(0)).unwrap(), l.caret(&pos(text.len())).unwrap());
    assert!((c0.top - b.bottom()).abs() < 1.0 && c1.top < c0.top - 50.0, "{c0:?} {c1:?} {b:?}");
    let hit = l.hit(0, b.x + b.w / 2.0, b.bottom() - 2.0, StoryRef::Body).unwrap();
    assert_eq!((hit.path, hit.off), (Path(vec![1, 0, 1, 0]), 0));
}

#[test]
fn turned_text_wraps_in_an_exact_row() {
    let d = turned_table(TextDirection::Down, &"word ".repeat(30), Some(72.0));
    let l = lay(&d);
    let it = turned_lines(&l);
    let lines = if let Placed::Lines { l0, l1, .. } = it { l1 - l0 } else { 0 };
    assert!(lines > 2, "the text wraps at the row height");
    let b = it.turned_bounds().unwrap();
    assert!(b.h <= 72.0 && b.w > 30.0, "{b:?}");
    let after = l.caret(&Pos::body(2, 0)).unwrap();
    assert!(after.top < b.y + 72.0 + 30.0, "the row keeps its exact height: {after:?}");
}

#[test]
fn hostile_turned_cells_never_panic() {
    // Empty, huge and nested turned cells, in exact rows too small for anything.
    for dir in [TextDirection::Down, TextDirection::Up] {
        for row in [None, Some(0.5), Some(1e9)] {
            let mut d = turned_table(dir, "", row);
            let l = lay(&d);
            let _ = l.caret(&Pos { story: StoryRef::Body, path: Path(vec![1, 0, 1, 0]), off: 0 });
            let mut inner = Table::new(1, 1, 20.0);
            inner.rows[0].cells[0].props.text_direction = dir;
            if let Some(Block::Table(t)) = d.body.get_mut(1).map(Arc::make_mut) {
                t.rows[0].cells[1].blocks.insert(0, Arc::new(Block::Table(inner)));
            }
            let l = lay(&d);
            for p in &l.pages {
                let _ = display::page_display(&d, p, &Default::default());
            }
        }
    }
    let l = lay(&turned_table(TextDirection::Down, &"long ".repeat(5000), None));
    assert!(!l.pages.is_empty());
}

fn rules_on_page0(l: &DocLayout) -> Vec<f32> {
    l.pages[0].items.iter().filter_map(|i| if let Placed::Rule { border, .. } = i { Some(border.width) } else { None }).collect()
}

#[test]
fn table_style_borders_survive_empty_tbl_borders() {
    let mut d = Document::from_text("before\nafter");
    let mut t = Table::new(2, 2, 468.0); // defaults to the TableGrid style
    t.props.borders = Some(Borders::default()); // what an empty <w:tblBorders/> parses to
    for r in &mut t.rows {
        for c in &mut r.cells {
            c.props.borders = Some(Borders::default());
        }
    }
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
    let l = lay(&d);
    assert!(rules_on_page0(&l).len() >= 8, "rules {:?}", rules_on_page0(&l));
}

#[test]
fn table_own_border_overrides_only_that_side() {
    let mut d = Document::from_text("before\nafter");
    let mut t = Table::new(2, 2, 468.0);
    t.props.borders = Some(Borders { top: Some(Border::single(2.0)), ..Default::default() });
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
    let l = lay(&d);
    let w = rules_on_page0(&l);
    assert!(w.contains(&2.0), "own top missing: {w:?}");
    assert!(w.iter().filter(|x| (**x - 0.5).abs() < 1e-6).count() >= 6, "style sides missing: {w:?}");
}

#[test]
fn table_nil_border_hides_style_side() {
    let mut d = Document::from_text("before\nafter");
    let mut t = Table::new(1, 1, 468.0); // 1×1: no inside edges
    t.props.borders = Some(Borders::all(Border { style: BorderStyle::None, width: 0.0, color: None, space: 0.0, theme: None }));
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
    assert!(rules_on_page0(&lay(&d)).is_empty());
}

#[test]
fn table_without_style_stays_borderless() {
    let mut d = Document::from_text("before\nafter");
    let mut t = Table::new(2, 2, 468.0);
    t.props.style = None;
    t.props.borders = Some(Borders::default());
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
    assert!(rules_on_page0(&lay(&d)).is_empty());
}

#[test]
fn cell_spacing_opens_above_every_row_and_below_the_last() {
    // WPS for Mac: w:tblCellSpacing holds half the gap; two rows with 7.8 pt stored (15.6 pt
    // between cells) are three gaps of 15.6 pt - above each row and below the last - and a border
    // width taller per gap.
    let height = |spacing: Option<f32>| {
        let mut d = Document::from_text("before\nafter");
        let mut t = Table::new(2, 2, 468.0);
        t.props.borders = Some(Borders::all(Border::single(0.5)));
        t.props.cell_spacing = spacing;
        d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
        let l = lay(&d);
        let ys: Vec<f32> = l.pages[0]
            .items
            .iter()
            .filter_map(|i| if let Placed::Lines { y, path, .. } = i { (path.0.len() == 1).then_some(*y) } else { None })
            .collect();
        ys[1] - ys[0]
    };
    let plain = height(None);
    let spaced = height(Some(7.8));
    assert!((spaced - plain - 3.0 * (15.6 + 0.5)).abs() < 0.01, "{plain} {spaced}");
}

#[test]
fn headers_and_page_fields() {
    let mut d = Document::from_text(&"para\n".repeat(120));
    let mut hp = kition_doc_doc::Paragraph::new();
    hp.insert_object(
        0,
        InlineObject::Field { instr: "PAGE".into(), result: String::new(), locked: false, code_runs: Vec::new(), code_fields: Vec::new() },
        &Default::default(),
    )
    .unwrap();
    let id = d.add_part(kition_doc_doc::PartKind::Footer, vec![kition_doc_doc::para_block(hp)]);
    d.last_section.footers.default = Some(id);
    let l = lay(&d);
    assert!(l.pages.len() >= 2);
    for (i, p) in l.pages.iter().enumerate() {
        assert_eq!(p.footer_story, Some(id));
        let label = p.footer.iter().find_map(|it| if let Placed::Lines { para, .. } = it { Some(para.clusters.len()) } else { None });
        assert!(label.is_some(), "page {i}");
    }
    let c = l.caret_on(&Pos { story: StoryRef::Part(id), path: Path::top(0), off: 0 }, 1).unwrap();
    assert_eq!(c.page, 1);
    assert!(c.top > 700.0);
}

#[test]
fn tall_first_page_header_pushes_body_down() {
    let mut d = Document::from_text(&"para\n".repeat(120));
    let tall: Vec<_> = (0..12).map(|_| kition_doc_doc::para_block(kition_doc_doc::Paragraph::new())).collect();
    let id = d.add_part(kition_doc_doc::PartKind::Header, tall);
    d.last_section.headers.first = Some(id);
    d.last_section.title_page = true;
    let l = lay(&d);
    let first = l.caret(&Pos::body(0, 0)).unwrap();
    assert!(first.top > d.last_section.margin_top + 50.0, "{first:?}");
    // The first paragraph on page 2 (which has no header) sits higher than page 1's first line,
    // whatever the installed fonts do to pagination.
    let later = (0..120).filter_map(|i| l.caret(&Pos::body(i, 0))).find(|c| c.page == 1).unwrap();
    assert!(later.top < first.top, "{later:?}");
}

#[test]
fn cache_reuses_unchanged_paragraphs() {
    let d = Document::from_text(&"some text here\n".repeat(50));
    let mut c = LayoutCache::new();
    layout(&d, &mut c, &LayoutOptions::default());
    let misses = c.misses;
    let mut d2 = d.clone();
    d2.insert_text(&Pos::body(10, 0), "x", &Default::default()).unwrap();
    layout(&d2, &mut c, &LayoutOptions::default());
    assert_eq!(c.misses, misses + 1);
}

#[test]
fn selection_rects_cover_range() {
    let d = Document::from_text("Hello world\nSecond line");
    let l = lay(&d);
    let r = l.selection_rects(&d, &Pos::body(0, 6), &Pos::body(1, 6), 0);
    assert_eq!(r.len(), 2);
    assert!(r[0].1.w > 20.0);
}

#[test]
fn column_segments_take_the_same_x_range_on_every_line() {
    let d = Document::from_text("abcdef\nabcdef\nab");
    let l = lay(&d);
    let left = l.caret(&Pos::body(0, 2)).unwrap().x;
    let right = l.caret(&Pos::body(0, 4)).unwrap().x;
    let segs = l.column_segments(&d, &Pos::body(2, 2), &Pos::body(0, 2), right, left, 0, 100);
    let offs: Vec<(u32, usize, usize)> = segs.iter().map(|(a, b)| (a.path.0[0], a.off, b.off)).collect();
    assert_eq!(offs, vec![(0, 2, 4), (1, 2, 4), (2, 2, 2)]);
    // Capped, and junk x gives nothing.
    assert_eq!(l.column_segments(&d, &Pos::body(0, 2), &Pos::body(2, 2), left, right, 0, 2).len(), 2);
    assert!(l.column_segments(&d, &Pos::body(0, 2), &Pos::body(2, 2), f32::NAN, right, 0, 100).is_empty());
}

#[test]
fn display_has_glyphs_and_marks() {
    let mut d = Document::from_text("Hello\tworld");
    d.format_range(&Pos::body(0, 0), &Pos::body(0, 5), &|c| c.underline = Some(kition_doc_doc::props::Underline::Single)).unwrap();
    let l = lay(&d);
    let items = display::page_display(&d, &l.pages[0], &display::DisplayOptions { marks: true, ..Default::default() });
    assert!(items.iter().any(|i| matches!(i, display::Draw::Glyphs { .. })));
    assert!(items.iter().any(|i| matches!(i, display::Draw::Line { .. })));
    assert!(items.iter().any(|i| matches!(i, display::Draw::Mark { ch: '¶', .. })));
    assert!(items.iter().any(|i| matches!(i, display::Draw::Mark { ch: '→', .. })));
}

#[test]
fn a_tracked_paragraph_mark_shows_in_its_authors_colour() {
    let mut d = Document::from_text("Split\nhere");
    d.revisions.push(kition_doc_doc::Revision { kind: kition_doc_doc::RevisionKind::Insert, author: "Ana".into(), date: String::new() });
    d.para_mut(kition_doc_doc::StoryRef::Body, &kition_doc_doc::Path::top(0)).unwrap().mark.ins = Some(0);
    let l = lay(&d);
    let marks = |markup: bool| -> Vec<Option<kition_doc_doc::props::Rgb>> {
        display::page_display(&d, &l.pages[0], &display::DisplayOptions { marks: true, markup, ..Default::default() })
            .into_iter()
            .filter_map(|i| if let display::Draw::Mark { ch: '¶', color, .. } = i { Some(color) } else { None })
            .collect()
    };
    assert_eq!(marks(true), [Some(display::revision_color(0)), None]);
    assert_eq!(marks(false), [None, None]);
}

#[test]
fn section_breaks_show_with_formatting_marks() {
    use kition_doc_doc::section::{SectionProps, SectionStart};
    // Two sections: "One" ends the first; the second starts as `start`.
    let doc = |start: SectionStart| {
        let mut d = Document::from_text("One\nTwo");
        d.para_mut(kition_doc_doc::StoryRef::Body, &kition_doc_doc::Path::top(0)).unwrap().section = Some(Box::new(SectionProps::default()));
        d.last_section.start = start;
        d
    };
    let labels = |d: &Document, marks: bool| -> (Vec<String>, usize) {
        let l = lay(d);
        let items = display::page_display(d, &l.pages[0], &display::DisplayOptions { marks, ..Default::default() });
        let texts = items.iter().filter_map(|i| if let display::Draw::MarkText { text, .. } = i { Some(text.clone()) } else { None }).collect();
        let pilcrows = items.iter().filter(|i| matches!(i, display::Draw::Mark { ch: '¶', .. })).count();
        (texts, pilcrows)
    };
    let d = doc(SectionStart::Continuous);
    // The break replaces the first paragraph's ¶; the last paragraph keeps its own.
    assert_eq!(labels(&d, true), (vec!["Section Break (Continuous)".to_string()], 1));
    assert_eq!(labels(&d, false), (vec![], 0));
    assert_eq!(labels(&doc(SectionStart::NextPage), true).0, ["Section Break (Next Page)"]);
    assert_eq!(labels(&doc(SectionStart::EvenPage), true).0, ["Section Break (Even Page)"]);
    assert_eq!(labels(&doc(SectionStart::OddPage), true).0, ["Section Break (Odd Page)"]);
    // A document with one section has no break.
    assert_eq!(labels(&Document::from_text("One\nTwo"), true), (vec![], 2));
}

#[test]
fn paragraph_marks_sit_past_text_running_the_other_way() {
    // A left-to-right paragraph ending in Hebrew, and a right-to-left one ending in Latin: the
    // logically last letters are at the wrong edge, but the ¶ still goes past every glyph.
    let mut d = Document::from_text("Hello \u{5e9}\u{5dc}\u{5d5}\u{5dd}\n\u{5e9}\u{5dc}\u{5d5}\u{5dd} abc");
    let at = Pos::body(1, 0);
    d.format_paragraphs(&at, &at, &|p: &mut ParaProps| p.bidi = Some(true)).unwrap();
    let l = lay(&d);
    // Leftmost and rightmost glyph edges (page x) of each paragraph's single line.
    let extents: Vec<(f32, f32)> = l.pages[0]
        .items
        .iter()
        .filter_map(|it| if let Placed::Lines { para, x, .. } = it { Some((para, *x)) } else { None })
        .map(|(pl, x)| {
            let ln = &pl.lines[0];
            let edges = (ln.c0..ln.c1).filter_map(|k| Some((ln.cl_left(k)?, ln.cl_right(k)?)));
            edges.fold((f32::MAX, f32::MIN), |(a, b), (l, r)| (a.min(x + l), b.max(x + r)))
        })
        .collect();
    let marks: Vec<(f32, f32)> = display::page_display(&d, &l.pages[0], &display::DisplayOptions { marks: true, ..Default::default() })
        .into_iter()
        .filter_map(|i| if let display::Draw::Mark { ch: '¶', x, size, .. } = i { Some((x, size)) } else { None })
        .collect();
    assert_eq!((extents.len(), marks.len()), (2, 2));
    let ((ltr_x, _), (rtl_x, size)) = (marks[0], marks[1]);
    assert!(ltr_x >= extents[0].1, "LTR ¶ at {ltr_x} should be right of all glyphs {:?}", extents[0]);
    assert!(rtl_x + size * 0.5 <= extents[1].0, "RTL ¶ at {rtl_x} should be left of all glyphs {:?}", extents[1]);
}

fn border_lines(d: &Document) -> Vec<(f32, f32, f32, f32)> {
    border_lines_in(d, &lay(d))
}

fn border_lines_in(d: &Document, l: &DocLayout) -> Vec<(f32, f32, f32, f32)> {
    display::page_display(d, &l.pages[0], &display::DisplayOptions::default())
        .into_iter()
        .filter_map(|i| if let display::Draw::Line { x0, y0, x1, y1, .. } = i { Some((x0, y0, x1, y1)) } else { None })
        .collect()
}

#[test]
fn character_border_draws_one_box_around_run() {
    use kition_doc_doc::props::Border;
    let mut d = Document::from_text("Hello world");
    assert!(border_lines(&d).is_empty());
    d.format_range(&Pos::body(0, 0), &Pos::body(0, 5), &|c| c.border = Some(Border::single(0.5))).unwrap();
    // One layout for both the draw list and the expected geometry: the background font scan can change metrics between layouts.
    let l = lay(&d);
    let lines = border_lines_in(&d, &l);
    assert_eq!(lines.len(), 4, "{lines:?}");
    let Some(Placed::Lines { para, x, y, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
    let (gx0, gx1) = (x + para.x_of(0, 0).unwrap(), x + para.x_of(0, 5).unwrap());
    let (top, bottom) = (*y, y + para.lines[0].height);
    let horiz: Vec<_> = lines.iter().filter(|l| l.1 == l.3).collect();
    let vert: Vec<_> = lines.iter().filter(|l| l.0 == l.2).collect();
    assert_eq!(horiz.len(), 2);
    assert_eq!(vert.len(), 2);
    assert!(horiz.iter().any(|h| (h.1 - top).abs() < 0.01) && horiz.iter().any(|h| (h.1 - bottom).abs() < 0.01), "{lines:?}");
    assert!(horiz.iter().all(|h| (h.0 - gx0).abs() < 0.01 && (h.2 - gx1).abs() < 0.01), "{lines:?} {gx0} {gx1}");
}

#[test]
fn equal_adjacent_borders_share_a_box() {
    use kition_doc_doc::props::Border;
    let mut d = Document::from_text("ab");
    d.format_range(&Pos::body(0, 0), &Pos::body(0, 2), &|c| c.border = Some(Border::single(0.5))).unwrap();
    d.format_range(&Pos::body(0, 1), &Pos::body(0, 2), &|c| c.bold = Some(true)).unwrap();
    assert_eq!(border_lines(&d).len(), 4);
    d.format_range(&Pos::body(0, 1), &Pos::body(0, 2), &|c| c.border = Some(Border::single(1.0))).unwrap();
    assert_eq!(border_lines(&d).len(), 8);
}

#[test]
fn wrapped_border_boxes_each_line() {
    use kition_doc_doc::props::Border;
    let text = "boxed ".repeat(20);
    let mut d = Document::from_text(text.trim_end());
    let n = d.para_at(&Pos::body(0, 0)).unwrap().len();
    d.format_range(&Pos::body(0, 0), &Pos::body(0, n), &|c| c.border = Some(Border::single(0.5))).unwrap();
    assert_eq!(lines_of(&lay(&d)), 2);
    assert_eq!(border_lines(&d).len(), 8);
}

#[test]
fn runs_differing_only_in_link_or_decoration_keep_their_own_style() {
    let mut d = Document::from_text("Alpha Beta Gamma Delta");
    let set = |d: &mut Document, a: usize, b: usize, f: &dyn Fn(&mut kition_doc_doc::props::CharProps)| {
        d.format_range(&Pos::body(0, a), &Pos::body(0, b), f).unwrap();
    };
    set(&mut d, 0, 5, &|c| c.link = Some("https://a.example/".into()));
    set(&mut d, 6, 10, &|c| c.link = Some("https://b.example/".into()));
    set(&mut d, 11, 16, &|c| c.strike = Some(true));
    set(&mut d, 17, 22, &|c| c.double_strike = Some(true));
    let l = lay(&d);
    let items = display::page_display(&d, &l.pages[0], &display::DisplayOptions::default());
    let link_of = |word: &str| {
        items.iter().find_map(|i| if let display::Draw::Glyphs { text, link, .. } = i { text.contains(word).then(|| link.clone()) } else { None })
    };
    assert_eq!(link_of("Alpha"), Some(Some("https://a.example/".into())));
    assert_eq!(link_of("Beta"), Some(Some("https://b.example/".into())));
    let strokes: Vec<_> = items.iter().filter_map(|i| if let display::Draw::Line { stroke, .. } = i { Some(*stroke) } else { None }).collect();
    assert!(strokes.contains(&display::Stroke::Solid) && strokes.contains(&display::Stroke::Double), "{strokes:?}");
    // Underline colour is per run too.
    let mut d = Document::from_text("Red Blue");
    let red = kition_doc_doc::props::Rgb(0xFF, 0, 0);
    let blue = kition_doc_doc::props::Rgb(0, 0, 0xFF);
    for (a, b, c) in [(0, 3, red), (4, 8, blue)] {
        d.format_range(&Pos::body(0, a), &Pos::body(0, b), &|p| {
            p.underline = Some(kition_doc_doc::props::Underline::Single);
            p.underline_color = Some(c);
        })
        .unwrap();
    }
    let l = lay(&d);
    let items = display::page_display(&d, &l.pages[0], &display::DisplayOptions::default());
    let colors: Vec<_> = items.iter().filter_map(|i| if let display::Draw::Line { color, .. } = i { Some(*color) } else { None }).collect();
    assert!(colors.contains(&red) && colors.contains(&blue), "{colors:?}");
}

#[test]
fn web_view_is_one_page() {
    let d = Document::from_text(&"text ".repeat(3000));
    let mut c = LayoutCache::new();
    let l = layout(&d, &mut c, &LayoutOptions { view: ViewMode::Web, web_width: 800.0, show_hidden: false, hide_deleted: false, proofing: false });
    assert_eq!(l.pages.len(), 1);
    assert!(l.pages[0].h > 800.0);
}

#[test]
fn hostile_props_do_not_panic() {
    let mut d = Document::from_text("x\ny");
    d.format_paragraphs(&Pos::body(0, 0), &Pos::body(1, 0), &|p| {
        *p = ParaProps { indent_left: Some(1e9), indent_right: Some(1e9), indent_first: Some(-1e9), ..Default::default() }
    })
    .unwrap();
    d.last_section.page_w = 1.0;
    d.last_section.margin_left = 500.0;
    let l = lay(&d);
    assert!(!l.pages.is_empty());
}

#[test]
fn layout_is_fast() {
    let d =
        Document::from_text(&"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore.\n".repeat(3000));
    let mut c = LayoutCache::new();
    let t0 = now_ms();
    let l = layout(&d, &mut c, &LayoutOptions::default());
    let cold = now_ms() - t0;
    let mut d2 = d.clone();
    d2.insert_text(&Pos::body(1500, 0), "x", &Default::default()).unwrap();
    let t1 = now_ms();
    let _ = layout(&d2, &mut c, &LayoutOptions::default());
    let warm = now_ms() - t1;
    eprintln!("3000 paragraphs, {} pages: cold {cold:.1} ms, warm {warm:.1} ms", l.pages.len());
    assert!(warm < cold);
}

#[test]
fn empty_center_tab_then_right_tab() {
    let mut d = Document::from_text("left\t\tright");
    d.format_paragraphs(&Pos::body(0, 0), &Pos::body(0, 0), &|p| p.style = Some("Header".into())).unwrap();
    let l = lay(&d);
    let end = l.caret(&Pos::body(0, 11)).unwrap().x;
    assert!((end - (72.0 + 468.0)).abs() < 1.0, "{end}");
}

#[test]
fn footnotes_sit_at_page_bottom() {
    let mut d = Document::from_text(&"Body text line.\n".repeat(30));
    let mut note = kition_doc_doc::Paragraph::with_text("The note text.", Default::default()).styled("FootnoteText");
    note.props.space_after = Some(0.0);
    let id = d.add_part(kition_doc_doc::PartKind::Footnote, vec![kition_doc_doc::para_block(note)]);
    d.insert_object(
        &Pos::body(3, 4),
        InlineObject::NoteRef { kind: kition_doc_doc::para::NoteKind::Footnote, id, custom: String::new() },
        &Default::default(),
    )
    .unwrap();
    let eid = d.add_part(
        kition_doc_doc::PartKind::Endnote,
        vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("An endnote.", Default::default()))],
    );
    d.insert_object(
        &Pos::body(5, 2),
        InlineObject::NoteRef { kind: kition_doc_doc::para::NoteKind::Endnote, id: eid, custom: String::new() },
        &Default::default(),
    )
    .unwrap();
    let l = lay(&d);
    let c = l.caret_on(&Pos { story: StoryRef::Part(id), path: Path::top(0), off: 0 }, 0).unwrap();
    assert_eq!(c.page, 0);
    assert!(c.top > 650.0 && c.top < 720.0, "{c:?}");
    let body_bottom = l.pages[0]
        .items
        .iter()
        .filter_map(|it| if let Placed::Lines { story: StoryRef::Body, y, para, l0, l1, .. } = it { item_bottom(*y, para, *l0, *l1) } else { None })
        .fold(0.0f32, f32::max);
    assert!(body_bottom < c.top, "{body_bottom} {}", c.top);
    assert!(l.caret_on(&Pos { story: StoryRef::Part(eid), path: Path::top(0), off: 0 }, 0).is_some());
}

#[test]
fn deleted_note_anchors_take_no_number_and_print_no_note() {
    use kition_doc_doc::para::NoteKind;
    let mut d = Document::from_text("Alpha Beta Gamma");
    let mut ids = Vec::new();
    // Footnotes after "Alpha" and "Beta", an endnote after "Gamma" (inserted back to front).
    for (off, kind, part) in [
        (16, NoteKind::Endnote, kition_doc_doc::PartKind::Endnote),
        (10, NoteKind::Footnote, kition_doc_doc::PartKind::Footnote),
        (5, NoteKind::Footnote, kition_doc_doc::PartKind::Footnote),
    ] {
        let id = d.add_part(part, vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("A note.", Default::default()))]);
        d.insert_object(&Pos::body(0, off), InlineObject::NoteRef { kind, id, custom: String::new() }, &Default::default()).unwrap();
        ids.push(id);
    }
    let [end_id, second, first] = ids[..] else { panic!() };
    // Track-delete the first footnote's anchor and the endnote's anchor.
    let obj = kition_doc_doc::para::OBJ.len_utf8();
    d.format_range(&Pos::body(0, 5), &Pos::body(0, 5 + obj), &|c| c.del = Some(0)).unwrap();
    let end = d.para(StoryRef::Body, &Path::top(0)).unwrap().len();
    d.format_range(&Pos::body(0, end - obj), &Pos::body(0, end), &|c| c.del = Some(0)).unwrap();
    // The glyphs of each note mark in the body, and whether the endnote's text is placed.
    let marks = |hide_deleted: bool| {
        let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions { hide_deleted, ..Default::default() });
        let items: Vec<&Placed> = l.pages.iter().flat_map(|p| p.items.iter()).collect();
        let mut marks = HashMap::new();
        for it in &items {
            if let Placed::Lines { story: StoryRef::Body, para, .. } = it {
                for (ci, id) in &para.notes {
                    let c = &para.clusters[*ci];
                    marks.insert(*id, para.glyphs[c.g0 as usize..c.g1 as usize].iter().map(|g| g.gid).collect::<Vec<_>>());
                }
            }
        }
        let endnote = items.iter().any(|i| matches!(i, Placed::Lines { story: StoryRef::Part(p), .. } if *p == end_id));
        (marks, endnote)
    };
    // Markup: footnotes 1 and 2, and the endnote prints.
    let (full, endnote) = marks(false);
    assert!(full[&first] != full[&second] && endnote, "{full:?}");
    // Final text: the surviving footnote is number 1, and the deleted endnote anchor prints no note.
    let (fin, endnote) = marks(true);
    assert_eq!(fin.get(&second), full.get(&first), "the surviving footnote keeps number 2");
    assert!(!endnote, "the endnote of a deleted anchor is printed");
}

#[test]
fn hidden_note_anchors_keep_their_number_and_their_note() {
    // Measured in Word (hidden text not printed): a hidden footnote or endnote reference mark still
    // takes its number, so the visible footnote after it is 2, and both notes are still printed.
    use kition_doc_doc::para::NoteKind;
    let mut d = Document::from_text("Alpha Beta Gamma");
    let mut ids = Vec::new();
    for (off, kind, part) in [
        (16, NoteKind::Endnote, kition_doc_doc::PartKind::Endnote),
        (10, NoteKind::Footnote, kition_doc_doc::PartKind::Footnote),
        (5, NoteKind::Footnote, kition_doc_doc::PartKind::Footnote),
    ] {
        let id = d.add_part(part, vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("A note.", Default::default()))]);
        d.insert_object(&Pos::body(0, off), InlineObject::NoteRef { kind, id, custom: String::new() }, &Default::default()).unwrap();
        ids.push(id);
    }
    let [end_id, second, first] = ids[..] else { panic!() };
    // Hide the first footnote's anchor and the endnote's anchor.
    let obj = kition_doc_doc::para::OBJ.len_utf8();
    d.format_range(&Pos::body(0, 5), &Pos::body(0, 5 + obj), &|c| c.hidden = Some(true)).unwrap();
    let end = d.para(StoryRef::Body, &Path::top(0)).unwrap().len();
    d.format_range(&Pos::body(0, end - obj), &Pos::body(0, end), &|c| c.hidden = Some(true)).unwrap();
    // The glyphs of each visible note mark in the body, and which notes' text is placed.
    let marks = |show_hidden: bool| {
        let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions { show_hidden, ..Default::default() });
        let items: Vec<&Placed> = l.pages.iter().flat_map(|p| p.items.iter()).collect();
        let mut marks = HashMap::new();
        for it in &items {
            if let Placed::Lines { story: StoryRef::Body, para, .. } = it {
                for (ci, id) in &para.notes {
                    let c = &para.clusters[*ci];
                    marks.insert(*id, para.glyphs[c.g0 as usize..c.g1 as usize].iter().map(|g| g.gid).collect::<Vec<_>>());
                }
            }
        }
        let printed = |id: u32| items.iter().any(|i| matches!(i, Placed::Lines { story: StoryRef::Part(p), .. } if *p == id));
        (marks, [first, second, end_id].map(printed))
    };
    let (shown, printed) = marks(true);
    assert!(shown[&first] != shown[&second] && printed == [true; 3], "{shown:?} {printed:?}");
    // Not shown: the visible footnote is still number 2, and every note is printed.
    let (fin, printed) = marks(false);
    assert_eq!(fin.get(&second), shown.get(&second), "the visible footnote is renumbered");
    assert_eq!(printed, [true; 3], "a note whose reference is hidden is not printed");
}

#[test]
fn text_wraps_around_square_float() {
    let mut d = Document::from_text(&"Words flow around the picture here. ".repeat(30));
    let float = kition_doc_doc::para::Float {
        wrap: kition_doc_doc::para::Wrap::Square,
        h_rel: kition_doc_doc::para::Anchor::Column,
        v_rel: kition_doc_doc::para::Anchor::Paragraph,
        x: 0.0,
        y: 0.0,
        dist: 9.0,
        ..Default::default()
    };
    let shape = InlineObject::Shape {
        vml: None,
        kind: kition_doc_doc::para::ShapeKind::Rectangle,
        w: 144.0,
        h: 100.0,
        fill: None,
        stroke: None,
        stroke_width: 1.0,
        float,
        story: None,
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    d.insert_object(&Pos::body(0, 0), shape, &Default::default()).unwrap();
    let l = lay(&d);
    let Placed::Lines { para, x, .. } = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })).unwrap() else { panic!() };
    let first = &para.lines[0];
    assert!(first.left >= 144.0, "first line starts beside the float: {}", first.left);
    let later = para.lines.iter().find(|ln| ln.top > 120.0).unwrap();
    assert!(later.left < 1.0, "lines below the float use the full width: {}", later.left);
    assert!(x + first.left > 72.0 + 144.0);
    assert!(l.pages[0].items.iter().any(|i| matches!(i, Placed::Shape { .. })));
}

#[test]
fn deleted_float_leaves_no_wrap_area_without_markup() {
    // A square-wrapped shape anchored at the start of the paragraph, its anchor track-deleted.
    let text = "Words flow around the picture here. ".repeat(30);
    let mut d = Document::from_text(&text);
    let float = kition_doc_doc::para::Float {
        wrap: kition_doc_doc::para::Wrap::Square,
        h_rel: kition_doc_doc::para::Anchor::Column,
        v_rel: kition_doc_doc::para::Anchor::Paragraph,
        x: 0.0,
        y: 0.0,
        dist: 9.0,
        dist_top: 9.0,
        dist_bottom: 9.0,
        ..Default::default()
    };
    let shape = InlineObject::Shape {
        vml: None,
        kind: kition_doc_doc::para::ShapeKind::Rectangle,
        w: 144.0,
        h: 100.0,
        fill: None,
        stroke: None,
        stroke_width: 1.0,
        float,
        story: None,
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    d.insert_object(&Pos::body(0, 0), shape, &Default::default()).unwrap();
    let obj = kition_doc_doc::para::OBJ.len_utf8();
    d.format_range(&Pos::body(0, 0), &Pos::body(0, obj), &|c| c.del = Some(0)).unwrap();
    // Where the first line starts, and whether the shape is placed.
    let first = |hide_deleted: bool| {
        let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions { hide_deleted, ..Default::default() });
        let p = &l.pages[0];
        let Some(Placed::Lines { para, .. }) = p.items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
        (para.lines[0].left, p.items.iter().any(|i| matches!(i, Placed::Shape { .. })))
    };
    // With markup the deleted shape is shown and the text flows beside it.
    let (left, shown) = first(false);
    assert!(left >= 144.0 && shown, "{left} {shown}");
    // Final text: no shape, and no gap where it was.
    assert_eq!(first(true), (0.0, false));
}

#[test]
fn hidden_float_leaves_no_wrap_area_when_hidden_text_is_not_shown() {
    // A square-wrapped shape whose anchor is hidden text, through a character style (so only the
    // resolved formatting says so). Measured in Word: not printed, and the text runs full width.
    let text = "Words flow around the picture here. ".repeat(30);
    let mut d = Document::from_text(&text);
    d.styles.upsert(kition_doc_doc::Style {
        id: "Secret".into(),
        name: "Secret".into(),
        kind: kition_doc_doc::StyleKind::Character,
        chr: kition_doc_doc::CharProps { hidden: Some(true), ..Default::default() },
        ..Default::default()
    });
    let float = kition_doc_doc::para::Float {
        wrap: kition_doc_doc::para::Wrap::Square,
        h_rel: kition_doc_doc::para::Anchor::Column,
        v_rel: kition_doc_doc::para::Anchor::Paragraph,
        x: 0.0,
        y: 0.0,
        dist: 9.0,
        dist_top: 9.0,
        dist_bottom: 9.0,
        ..Default::default()
    };
    let shape = InlineObject::Shape {
        vml: None,
        kind: kition_doc_doc::para::ShapeKind::Rectangle,
        w: 144.0,
        h: 100.0,
        fill: None,
        stroke: None,
        stroke_width: 1.0,
        float,
        story: None,
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    d.insert_object(&Pos::body(0, 0), shape, &Default::default()).unwrap();
    let obj = kition_doc_doc::para::OBJ.len_utf8();
    d.format_range(&Pos::body(0, 0), &Pos::body(0, obj), &|c| c.style = Some("Secret".into())).unwrap();
    // Where the first line starts, and whether the shape is placed.
    let first = |show_hidden: bool| {
        let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions { show_hidden, ..Default::default() });
        let p = &l.pages[0];
        let Some(Placed::Lines { para, .. }) = p.items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
        (para.lines[0].left, p.items.iter().any(|i| matches!(i, Placed::Shape { .. })))
    };
    // Hidden text shown: the shape is placed and the text flows beside it.
    let (left, shown) = first(true);
    assert!(left >= 144.0 && shown, "{left} {shown}");
    // Not shown: no shape, and no gap where it was.
    assert_eq!(first(false), (0.0, false));
}

/// A page-number start or a line-number start near `u32::MAX` (accepted from files and commands)
/// overflowed when the next page or line was counted: a panic with overflow checks on.
#[test]
fn extreme_page_and_line_number_starts_do_not_overflow() {
    let mut d = Document::from_text("one\ntwo\nthree");
    for b in d.body.iter_mut() {
        if let kition_doc_doc::Block::Para(p) = std::sync::Arc::make_mut(b) {
            p.props.page_break_before = Some(true);
        }
    }
    d.last_section.page_num_start = Some(u32::MAX);
    d.last_section.line_numbers = Some(kition_doc_doc::section::LineNumbering { start: u32::MAX, ..Default::default() });
    let l = lay(&d);
    assert!(l.pages.len() >= 3, "{} pages", l.pages.len());
}

#[test]
fn page_numbers_restart_at_zero() {
    // `<w:pgNumType w:start="0"/>` on a cover section and on the next one: WPS (desktop and the
    // kdocs web editor) numbers the cover 0 and the first page of the next section 0 again.
    let mut d = Document::from_text("Cover\nContents\nBody\nMore");
    for (i, b) in d.body.iter_mut().enumerate() {
        if let kition_doc_doc::Block::Para(p) = std::sync::Arc::make_mut(b) {
            p.props.page_break_before = Some(i == 1 || i == 3);
        }
    }
    let mut first = d.last_section.clone();
    first.page_num_start = Some(0);
    d.last_section.page_num_start = Some(0);
    if let Some(Block::Para(p)) = d.body.get_mut(1).map(std::sync::Arc::make_mut) {
        p.section = Some(Box::new(first));
    }
    let l = lay(&d);
    let numbers: Vec<u32> = l.pages.iter().map(|p| p.number).collect();
    assert_eq!(numbers, [0, 1, 0, 1]);
}

#[test]
fn a_formula_in_a_header_shows_each_pages_number() {
    use kition_doc_doc::para::CodeField;
    // WPS's even-page header `第{ = { PAGE } }页`: WPS computes the formula on every page.
    let mut d = Document::from_text("One\nTwo");
    if let Some(Block::Para(p)) = d.body.get_mut(1).map(std::sync::Arc::make_mut) {
        p.props.page_break_before = Some(true);
    }
    let mut header = kition_doc_doc::Paragraph::with_text("第页", Default::default());
    let formula = InlineObject::Field {
        instr: "=".into(),
        result: "6".into(),
        locked: false,
        code_runs: Vec::new(),
        code_fields: vec![CodeField { start: 1, len: 0, instr: "PAGE".into() }],
    };
    header.insert_object("第".len(), formula, &Default::default()).unwrap();
    let id = d.add_part(kition_doc_doc::PartKind::Header, vec![kition_doc_doc::para_block(header)]);
    d.last_section.headers.default = Some(id);
    let l = lay(&d);
    let text = |i: usize| -> String {
        flat(display::page_display(&d, &l.pages[i], &Default::default()))
            .iter()
            .filter_map(|x| if let display::Draw::Glyphs { text, .. } = x { Some(text.to_string()) } else { None })
            .collect()
    };
    assert!(text(0).contains("第1页"), "{}", text(0));
    assert!(text(1).contains("第2页"), "{}", text(1));
}

#[test]
fn a_missing_fangsong_line_is_as_tall_as_wps_draws_it() {
    // 仿宋 is not installed here; WPS for Mac draws a 12 pt single line of it 17.4 pt tall.
    if kition_doc_fonts::FontDb::global().has_family("仿宋") {
        return;
    }
    let mut d = Document::from_text(&"仿宋行高校准文字。".repeat(40));
    d.styles.default_chr.font_ea = Some("仿宋".into());
    d.styles.default_chr.size = Some(12.0);
    let props = &mut d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props;
    props.line_spacing = Some(kition_doc_doc::props::LineSpacing::Multiple(1.0));
    let l = lay(&d);
    let Placed::Lines { para, .. } = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })).unwrap() else { panic!() };
    let step = para.lines[1].baseline - para.lines[0].baseline;
    assert!((step - 17.4).abs() < 0.2, "line step {step}");
}

#[test]
fn line_numbers_borders_text_boxes() {
    let mut d = Document::from_text("one\ntwo\nthree");
    d.last_section.line_numbers = Some(Default::default());
    d.last_section.page_borders = Some(kition_doc_doc::props::Borders::box_(kition_doc_doc::props::Border::single(1.0)));
    let id = d.add_part(
        kition_doc_doc::PartKind::TextBox,
        vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("inside the box", Default::default()))],
    );
    let tb = InlineObject::Shape {
        vml: None,
        kind: kition_doc_doc::para::ShapeKind::TextBox,
        w: 144.0,
        h: 72.0,
        fill: None,
        stroke: None,
        stroke_width: 1.0,
        float: Default::default(),
        story: Some(id),
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    d.insert_object(&Pos::body(2, 5), tb, &Default::default()).unwrap();
    let l = lay(&d);
    let p = &l.pages[0];
    let numbers = p.items.iter().filter(|i| matches!(i, Placed::Lines { story: StoryRef::Part(u32::MAX), .. })).count();
    assert_eq!(numbers, 3);
    assert!(p.items.iter().filter(|i| matches!(i, Placed::Rule { .. })).count() >= 4);
    assert!(l.caret(&Pos { story: StoryRef::Part(id), path: Path::top(0), off: 0 }).is_some());
}

#[test]
fn tall_rows_split_across_pages() {
    let mut d = Document::from_text("before\nafter");
    let mut t = Table::new(2, 2, 468.0);
    let long = "Row text that keeps going and going so the cell grows taller than a page. ".repeat(120);
    t.rows[0].cells[0].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(&long, Default::default()))];
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t.clone())).unwrap();
    let l = lay(&d);
    assert!(l.pages.len() >= 2, "pages {}", l.pages.len());
    // The cell's lines start on page 1 (row split, not moved) and continue on page 2.
    let cell_lines = |p: &Page| p.items.iter().filter(|i| matches!(i, Placed::Lines { path, .. } if path.0 == vec![1, 0, 0, 0])).count();
    assert!(cell_lines(&l.pages[0]) > 0);
    assert!(cell_lines(&l.pages[1]) > 0);
    // Nothing runs past the bottom margin.
    for p in &l.pages[..l.pages.len() - 1] {
        for it in &p.items {
            if let Placed::Lines { y, para, l0, l1, .. } = it {
                let b = item_bottom(*y, para, *l0, *l1).unwrap_or(0.0);
                assert!(b <= 792.0 - 72.0 + 0.5, "line bottom {b}");
            }
        }
    }
    // Every line of the long paragraph is placed exactly once.
    let total: usize = l
        .pages
        .iter()
        .flat_map(|p| p.items.iter())
        .filter_map(|i| if let Placed::Lines { path, l0, l1, .. } = i { (path.0 == vec![1, 0, 0, 0]).then_some(l1 - l0) } else { None })
        .sum();
    let n = l
        .pages
        .iter()
        .flat_map(|p| p.items.iter())
        .find_map(|i| if let Placed::Lines { path, para, .. } = i { (path.0 == vec![1, 0, 0, 0]).then_some(para.lines.len()) } else { None })
        .unwrap_or(0);
    assert_eq!(total, n);
    // "Can't split" rows move whole instead.
    let mut d2 = Document::from_text("before\nafter");
    t.rows[0].props.cant_split = true;
    d2.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
    let l2 = lay(&d2);
    assert!(l2.pages.len() >= 2);
}

#[test]
fn a_row_moves_whole_when_a_cell_would_keep_no_line() {
    // A row of a 30 pt cell and an 8 pt cell near a page's foot: where only the small line fits,
    // WPS moves the whole row on rather than leave the big cell's text alone on the next page.
    let small = kition_doc_doc::props::CharProps { size: Some(8.0), ..Default::default() };
    let big = kition_doc_doc::props::CharProps { size: Some(30.0), ..Default::default() };
    for n in 14..34 {
        let mut d = Document::from_text(&vec!["Line"; n].join("\n"));
        let mut t = Table::new(1, 2, 468.0);
        t.rows[0].cells[0].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("Big", big.clone()))];
        t.rows[0].cells[1].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("small", small.clone()))];
        d.insert_block(StoryRef::Body, &Path::top(n), kition_doc_doc::Block::Table(t)).unwrap();
        let l = lay(&d);
        let page_of = |cell: u32| {
            l.pages.iter().position(|p| p.items.iter().any(|i| matches!(i, Placed::Lines { path, .. } if path.0 == vec![n as u32, 0, cell, 0])))
        };
        assert!(page_of(0).is_some() && page_of(1).is_some(), "{n}: both cells placed");
        assert_eq!(page_of(0), page_of(1), "{n} lines before: the row's cells start on the same page");
    }
}

#[test]
fn repeated_header_is_followed_by_the_next_row_even_when_it_overflows() {
    // A one-column table at the top of a letter page (body 648 pt), then an empty paragraph. Returns
    // the table rows on each page, in order.
    let rows_per_page = |t: Table| -> Vec<Vec<usize>> {
        let mut d = Document::new();
        d.insert_block(StoryRef::Body, &Path::top(0), kition_doc_doc::Block::Table(t)).unwrap();
        let l = lay(&d);
        l.pages
            .iter()
            .map(|p| p.items.iter().filter_map(|i| if let Placed::Cell { row, cell: 0, .. } = i { Some(*row) } else { None }).collect())
            .collect()
    };
    // A header row of `header` points and body rows of `rows` points (at least), one line of text each.
    let table = |header: f32, rows: &[f32]| {
        let mut t = Table::new(rows.len() + 1, 1, 468.0);
        t.rows[0].props.header = true;
        for (row, h) in t.rows.iter_mut().zip(std::iter::once(&header).chain(rows)) {
            row.props.height = Some(*h);
            row.props.height_rule = kition_doc_doc::props::HeightRule::AtLeast;
        }
        t
    };
    // Header and row fit together: one row per page, under the header.
    assert_eq!(rows_per_page(table(100.0, &[400.0; 3])), vec![vec![0, 1], vec![0, 2], vec![0, 3]]);
    // A 300 pt header and 400 pt rows never fit together. As in Word, the first row moves to a new
    // page, goes under the repeated header and runs into the bottom margin; so does every later row.
    assert_eq!(rows_per_page(table(300.0, &[400.0; 4])), vec![vec![0], vec![0, 1], vec![0, 2], vec![0, 3], vec![0, 4], vec![]]);
    // A header that almost fills the page still repeats, with one row under it on each page.
    assert_eq!(rows_per_page(table(640.0, &[100.0; 3])), vec![vec![0], vec![0, 1], vec![0, 2], vec![0, 3], vec![]]);
    // A 20-row table under a 640 pt header: one page per row, never a runaway.
    assert_eq!(rows_per_page(table(640.0, &[12.0; 19])).len(), 21);
    // A row of text that is taller than the room under the header still splits across pages.
    let mut t = table(100.0, &[12.0]);
    let long = "Row text that keeps going and going so the cell grows taller than a page. ".repeat(120);
    t.rows[1].cells[0].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(&long, Default::default()))];
    let pages = rows_per_page(t);
    assert!(pages.len() >= 3, "{pages:?}");
    assert!(pages.iter().all(|p| p.first() == Some(&0) && p.contains(&1)), "{pages:?}");
    // An ordinary header still repeats on every page the table reaches.
    let mut t = Table::new(80, 2, 468.0);
    t.rows[0].props.header = true;
    let pages = rows_per_page(t);
    assert!(pages.len() >= 2, "{pages:?}");
    assert!(pages.iter().all(|p| p.first() == Some(&0)), "{pages:?}");
}

/// A drop-cap paragraph in a font larger than 1000 pt used to panic in layout (`clamp` with min > max).
#[test]
fn drop_cap_with_a_huge_font_lays_out() {
    for size in [999.0, 1000.0, 1001.0, 1638.0] {
        let cp = kition_doc_doc::CharProps { size: Some(size), ..Default::default() };
        let mut p = kition_doc_doc::Paragraph::with_text("Hello drop cap world", cp);
        p.props.drop_cap = Some(3);
        let mut d = Document::from_text("x");
        d.body = vec![kition_doc_doc::para_block(p)];
        let l = lay(&d);
        let Some(pl) = l.pages[0].items.iter().find_map(|i| if let Placed::Lines { para, .. } = i { Some(para.clone()) } else { None }) else {
            panic!("no lines at size {size}")
        };
        let (nc, lines, _) = pl.drop_cap.unwrap_or((0, 0, 0.0));
        assert_eq!((nc, lines), (1, 3), "size {size}");
    }
}

#[test]
fn drop_cap_indents_its_lines() {
    let mut p =
        kition_doc_doc::Paragraph::with_text(&"Every line of this paragraph wraps around a large first letter. ".repeat(6), Default::default());
    p.props.drop_cap = Some(3);
    let mut d = Document::from_text("x");
    d.body = vec![kition_doc_doc::para_block(p)];
    let l = lay(&d);
    let Some(pl) = l.pages[0].items.iter().find_map(|i| if let Placed::Lines { para, .. } = i { Some(para.clone()) } else { None }) else {
        panic!("no lines")
    };
    let (nc, lines, w) = pl.drop_cap.unwrap_or((0, 0, 0.0));
    assert_eq!((nc, lines), (1, 3));
    assert!(w > 20.0, "drop cap width {w}");
    assert!(pl.lines.len() > 4);
    for (k, line) in pl.lines.iter().enumerate() {
        let x = line.xs.first().copied().unwrap_or(0.0);
        if k < 3 {
            assert!(x >= w - 0.5, "line {k} at {x}");
        } else {
            assert!(x < 1.0, "line {k} at {x}");
        }
    }
    // The letter's glyphs are lowered toward the third line's baseline.
    let g = pl.glyphs.first().map(|g| g.dy).unwrap_or(0.0);
    assert!(g < -20.0, "dy {g}");
}

#[test]
fn a_documents_drop_cap_keeps_its_size_and_distance_from_the_text() {
    // Word stores the drop cap's size in its run (60 pt for these lines) and its distance from the
    // text in the frame (w:hSpace): WPS for Mac lays the text beside it at exactly that indent.
    let mut p = kition_doc_doc::Paragraph::with_text("D", kition_doc_doc::CharProps { size: Some(60.0), ..Default::default() });
    p.append(kition_doc_doc::Paragraph::with_text(&"Every line wraps beside the large letter. ".repeat(6), Default::default()));
    p.props.drop_cap = Some(3);
    p.props.drop_cap_space = Some(20.0);
    let mut d = Document::from_text("x");
    d.body = vec![kition_doc_doc::para_block(p)];
    let l = lay(&d);
    let Some(pl) = l.pages[0].items.iter().find_map(|i| if let Placed::Lines { para, .. } = i { Some(para.clone()) } else { None }) else {
        panic!("no lines")
    };
    let (_, lines, indent) = pl.drop_cap.unwrap_or((0, 0, 0.0));
    assert_eq!(lines, 3);
    assert!(pl.drop_cap_w > 20.0 && pl.drop_cap_w < 60.0, "a 60 pt letter, not one sized from its own run: {}", pl.drop_cap_w);
    assert!((indent - pl.drop_cap_w - 20.0).abs() < 0.01, "{indent}");
}

#[test]
fn the_automatic_hyphenation_setting_does_not_break_words() {
    // WPS for Mac has no automatic hyphenation: the setting is kept but words don't break at
    // dictionary points.
    let text = "Internationalization considerations notwithstanding, administrators systematically reconsidered extraordinarily uncharacteristic responsibilities. ".repeat(4);
    let mut d = Document::from_text(&text);
    let hyphens = |l: &DocLayout| {
        l.pages[0]
            .items
            .iter()
            .filter_map(|i| if let Placed::Lines { para, .. } = i { Some(para.lines.iter().filter(|x| x.hyphen.is_some()).count()) } else { None })
            .sum::<usize>()
    };
    let off = lay(&d);
    d.settings.auto_hyphenation = true;
    let on = lay(&d);
    assert_eq!((hyphens(&off), hyphens(&on)), (0, 0));
    // A soft hyphen still breaks its word.
    let mut soft = Document::from_text(&text.replace("Internationalization", "Inter\u{ad}na\u{ad}tion\u{ad}al\u{ad}iza\u{ad}tion"));
    soft.settings.auto_hyphenation = true;
    let narrow = |d: &mut Document| d.format_paragraphs(&Pos::body(0, 0), &Pos::body(0, 0), &|p| p.indent_right = Some(300.0)).unwrap();
    narrow(&mut soft);
    let points = |l: &DocLayout| {
        l.pages[0].items.iter().filter_map(|i| if let Placed::Lines { para, .. } = i { Some(para.hyph_after.len()) } else { None }).sum::<usize>()
    };
    assert_eq!(points(&lay(&soft)), 5 * 4, "the soft hyphens are the only hyphenation points");
}

/// A table of `rows` x 2 with text in every cell, after a "before" paragraph.
fn styled_table_doc(style: Option<&str>, rows: usize) -> Document {
    let mut d = Document::from_text("before\nafter");
    let mut t = Table::new(rows, 2, 468.0);
    t.props.style = style.map(str::to_string);
    for (r, row) in t.rows.iter_mut().enumerate() {
        for (c, cell) in row.cells.iter_mut().enumerate() {
            cell.blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(&format!("R{r} C{c}"), Default::default()))];
        }
    }
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
    d
}

fn row_height(l: &DocLayout, row: usize) -> f32 {
    l.pages[0].items.iter().find_map(|i| if let Placed::Cell { rect, row: r, .. } = i { (*r == row).then_some(rect.h) } else { None }).unwrap()
}

/// The laid-out paragraph in cell (row, col) of the table at body index 1.
fn cell_para(l: &DocLayout, row: u32, col: u32) -> Arc<ParaLayout> {
    l.pages[0]
        .items
        .iter()
        .find_map(|i| if let Placed::Lines { path, para, .. } = i { (path.0 == vec![1, row, col, 0]).then(|| para.clone()) } else { None })
        .unwrap()
}

/// Text in a table cell takes its table style's paragraph and run formatting over the document
/// defaults (ECMA-376 §17.7.2): Table Grid's single spacing and no space after, not the defaults'
/// 8 pt after and 1.15 lines.
#[test]
fn table_style_paragraph_props_beat_document_defaults() {
    let d = styled_table_doc(Some("TableGrid"), 3);
    let plain = styled_table_doc(None, 3);
    let (lg, lp) = (lay(&d), lay(&plain));
    let p = cell_para(&lg, 1, 0);
    assert_eq!((p.rp.space_after, p.rp.line_spacing), (0.0, kition_doc_doc::props::LineSpacing::Multiple(1.0)));
    let q = cell_para(&lp, 1, 0);
    assert_eq!((q.rp.space_after, q.rp.line_spacing), (8.0, kition_doc_doc::props::LineSpacing::Multiple(1.15)));
    let (g, n) = (row_height(&lg, 1), row_height(&lp, 1));
    assert!(g + 8.0 < n, "Table Grid row {g}, unstyled row {n}");
    assert!(g < 18.0, "one 12 pt line, single spaced: {g}");
}

/// The paragraph's own style (Normal included) and direct formatting beat the table style; the
/// table style only fills in what they leave unset.
#[test]
fn paragraph_style_and_direct_formatting_beat_table_style() {
    let mut d = styled_table_doc(Some("TableGrid"), 3);
    d.styles.upsert(kition_doc_doc::Style {
        id: "Spaced".into(),
        name: "Spaced".into(),
        based_on: Some("Normal".into()),
        para: ParaProps { space_after: Some(12.0), ..Default::default() },
        ..Default::default()
    });
    d.format_paragraphs(
        &Pos { story: StoryRef::Body, path: Path(vec![1, 0, 0, 0]), off: 0 },
        &Pos { story: StoryRef::Body, path: Path(vec![1, 0, 0, 0]), off: 0 },
        &|p| p.style = Some("Spaced".into()),
    )
    .unwrap();
    d.format_paragraphs(
        &Pos { story: StoryRef::Body, path: Path(vec![1, 1, 0, 0]), off: 0 },
        &Pos { story: StoryRef::Body, path: Path(vec![1, 1, 0, 0]), off: 0 },
        &|p| p.space_after = Some(3.0),
    )
    .unwrap();
    let l = lay(&d);
    let styled = cell_para(&l, 0, 0);
    assert_eq!(styled.rp.space_after, 12.0, "paragraph style");
    assert_eq!(styled.rp.line_spacing, kition_doc_doc::props::LineSpacing::Multiple(1.0), "unset by the style: from the table style");
    assert_eq!(cell_para(&l, 1, 0).rp.space_after, 3.0, "direct formatting");
    assert_eq!(cell_para(&l, 2, 0).rp.space_after, 0.0, "table style");
    // Normal setting its own spacing wins too, even where it equals nothing in the defaults.
    if let Some(n) = d.styles.get_mut("Normal") {
        n.para.space_after = Some(10.0);
    }
    assert_eq!(cell_para(&lay(&d), 2, 0).rp.space_after, 10.0, "Normal");
}

/// A table style based on another keeps the base style's borders and cell text formatting, and
/// adds its own run formatting and conditional formats.
#[test]
fn derived_table_style_merges_its_base() {
    use kition_doc_doc::styles::TableStyleParts;
    let red = kition_doc_doc::Rgb(0xC0, 0, 0);
    let mut d = styled_table_doc(Some("RedGrid"), 3);
    d.styles.upsert(kition_doc_doc::Style {
        id: "RedGrid".into(),
        name: "Red Grid".into(),
        kind: kition_doc_doc::StyleKind::Table,
        based_on: Some("TableGrid".into()),
        chr: CharProps { bold: Some(true), color: Some(kition_doc_doc::TextColor::Rgb(red)), ..Default::default() },
        table: Some(TableStyleParts { header_chr: CharProps { italic: Some(true), ..Default::default() }, ..Default::default() }),
        ..Default::default()
    });
    let l = lay(&d);
    let rules = l.pages[0].items.iter().filter(|i| matches!(i, Placed::Rule { .. })).count();
    assert!(rules >= 18, "Table Grid's borders: {rules}");
    let body = cell_para(&l, 1, 1);
    let rc = &body.styles[0].rc;
    assert!(rc.bold && !rc.italic, "{rc:?}");
    assert_eq!(rc.color, kition_doc_doc::TextColor::Rgb(red));
    assert_eq!(body.rp.space_after, 0.0, "Table Grid's paragraph formatting");
    let head = &cell_para(&l, 0, 0).styles[0].rc;
    assert!(head.bold && head.italic, "{head:?}");
    // Direct formatting still wins over the table style.
    let mut d2 = d.clone();
    let path = Path(vec![1, 2, 0, 0]);
    d2.format_range(&Pos { story: StoryRef::Body, path: path.clone(), off: 0 }, &Pos { story: StoryRef::Body, path, off: 5 }, &|c| {
        c.bold = Some(false)
    })
    .unwrap();
    assert!(!cell_para(&lay(&d2), 2, 0).styles[0].rc.bold);
}

fn text_box(d: &mut Document, at: usize, text: &str, w: f32, h: f32, float: kition_doc_doc::para::Float) -> u32 {
    text_box_at(d, &Pos::body(0, at), text, w, h, float)
}

fn text_box_at(d: &mut Document, pos: &Pos, text: &str, w: f32, h: f32, float: kition_doc_doc::para::Float) -> u32 {
    let id = d.add_part(
        kition_doc_doc::PartKind::TextBox,
        vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(text, Default::default()))],
    );
    let shape = InlineObject::Shape {
        vml: None,
        kind: kition_doc_doc::para::ShapeKind::TextBox,
        w,
        h,
        fill: None,
        stroke: None,
        stroke_width: 0.75,
        float,
        story: Some(id),
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    d.insert_object(pos, shape, &Default::default()).unwrap();
    id
}

#[test]
fn plain_word_art_keeps_its_letters_curves_and_turns_with_its_shape() {
    use kition_doc_doc::graphic::{PathSeg, PresetGeom};
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // A watermark: plain art text stretched over its turned shape. Its letters keep their curves,
    // and they turn with the shape (a quarter here) about its centre.
    let mut d = Document::from_text("Body");
    let float = Float { wrap: Wrap::BehindText, h_rel: Anchor::Margin, v_rel: Anchor::Margin, rot: 90.0, ..Default::default() };
    let id = text_box_at(&mut d, &Pos::body(0, 0), "OS", 200.0, 100.0, float);
    if let Some(Block::Para(p)) = d.body.get_mut(0).map(std::sync::Arc::make_mut)
        && let Some(InlineObject::Shape { warp, body, stroke_width, .. }) = p.objects.get_mut(0)
    {
        *warp = Some(PresetGeom { name: "textPlain".into(), adj: Vec::new() });
        *body = Some(kition_doc_doc::graphic::TextFrame {
            anchor: kition_doc_doc::graphic::TextAnchor::Middle,
            insets: [0.0; 4],
            fit: false,
            no_wrap: false,
            fit_height: false,
        });
        *stroke_width = 0.0;
    }
    assert!(d.parts.contains_key(&id));
    let l = lay(&d);
    let draws = flat(display::page_display(&d, &l.pages[0], &Default::default()));
    let turned = draws.iter().find_map(|x| match x {
        display::Draw::Rotated { spin, items, .. }
            if (spin.deg - 90.0).abs() < 0.01 && items.iter().any(|i| matches!(i, display::Draw::Path { .. })) =>
        {
            Some(items)
        }
        _ => None,
    });
    let items = turned.unwrap_or_else(|| panic!("the letters turn with the shape: {draws:?}"));
    let curves = items.iter().any(|i| matches!(i, display::Draw::Path { segs, .. } if segs.iter().any(|s| matches!(s, PathSeg::Cubic(..)))));
    assert!(curves, "the round letters keep their curves: {items:?}");
}

#[test]
fn bent_word_art_keeps_its_outlined_letters_outline() {
    use kition_doc_doc::graphic::PresetGeom;
    use kition_doc_doc::props::{Rgb, TextOutline, TextPaint};
    // Art text bent by a warp whose letters are outlined (red, 1 pt, hollow): the bent letters
    // keep their outline.
    let mut d = Document::from_text("Body");
    let id = text_box_at(&mut d, &Pos::body(0, 0), "Arch", 200.0, 100.0, Default::default());
    if let Some(Block::Para(p)) = d.parts.get_mut(&id).and_then(|part| part.blocks.get_mut(0)).map(std::sync::Arc::make_mut) {
        let outline = TextOutline { stroke: TextPaint::Rgb(Rgb(255, 0, 0)), width: 1.0, fill: Some(None) };
        p.format(0, 4, &|c| c.text_outline = Some(outline)).unwrap();
    }
    if let Some(Block::Para(p)) = d.body.get_mut(0).map(std::sync::Arc::make_mut)
        && let Some(InlineObject::Shape { warp, .. }) = p.objects.get_mut(0)
    {
        *warp = Some(PresetGeom { name: "textArchUp".into(), adj: Vec::new() });
    }
    let l = lay(&d);
    let draws = flat(display::page_display(&d, &l.pages[0], &Default::default()));
    let bent = draws.iter().any(|x| matches!(x, display::Draw::Path { stroke: Some(Rgb(255, 0, 0)), fill: None, segs, .. } if segs.len() > 20));
    assert!(bent, "the bent letters are outlined: {draws:?}");
}

#[test]
fn a_blank_page_before_an_odd_page_section_has_no_header() {
    use kition_doc_doc::section::SectionStart;
    // WPS references: the blank page inserted so a section starts on an odd page shows nothing,
    // not even the header (a watermark) the pages around it show.
    let mut d = Document::from_text("First\nSecond");
    let header = d.add_part(
        kition_doc_doc::PartKind::Header,
        vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("Header", Default::default()))],
    );
    let mut first = d.last_section.clone();
    first.headers.default = Some(header);
    d.last_section.start = SectionStart::OddPage;
    if let Some(Block::Para(p)) = d.body.get_mut(0).map(std::sync::Arc::make_mut) {
        p.section = Some(Box::new(first));
    }
    let l = lay(&d);
    assert_eq!(l.pages.len(), 3, "page 2 is the blank one");
    assert!(!l.pages[0].header.is_empty() && !l.pages[2].header.is_empty(), "the header shows on pages 1 and 3");
    assert!(l.pages[1].header.is_empty() && l.pages[1].footer.is_empty() && l.pages[1].items.is_empty(), "{:?}", l.pages[1].header);
}

fn box_rect(l: &DocLayout, id: u32) -> kition_doc_geom::Rect {
    l.pages[0]
        .items
        .iter()
        .find_map(|i| if let Placed::Object { rect, text_box, .. } = i { (*text_box == Some(id)).then_some(*rect) } else { None })
        .unwrap()
}

#[test]
fn a_text_box_that_fits_its_text_takes_its_width_for_its_placement() {
    use kition_doc_doc::graphic::{TextAnchor, TextFrame};
    use kition_doc_doc::para::{Anchor, Float, FloatAlign, Wrap};
    // A page number in a 144 pt box set to the right margin, the box fitting its text
    // (`wrap="none"` with `a:spAutoFit`): WPS sets the number against the margin.
    let mut d = Document::from_text("Body");
    let float =
        Float { wrap: Wrap::InFrontOfText, h_rel: Anchor::Margin, h_align: Some(FloatAlign::End), v_rel: Anchor::Paragraph, ..Default::default() };
    let id = text_box(&mut d, 0, "11", 144.0, 144.0, float);
    let InlineObject::Shape { body, .. } = &mut d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().objects[0] else { panic!() };
    *body = Some(TextFrame { anchor: TextAnchor::Top, insets: [0.0; 4], fit: true, no_wrap: false, fit_height: false });
    let l = lay(&d);
    let r = box_rect(&l, id);
    let right = l.pages[0].body.right();
    assert!((r.right() - right).abs() < 0.5, "the box ends at the margin: {r:?}");
    assert!(r.w > 5.0 && r.w < 30.0, "as wide as \"11\": {r:?}");
    let caret = l.caret(&Pos { story: StoryRef::Part(id), path: Path::top(0), off: 0 }).unwrap();
    assert!((caret.x - r.x).abs() < 0.5, "its text starts at the box's left: {} in {r:?}", caret.x);
}

#[test]
fn a_text_box_that_fits_its_text_keeps_an_indented_line_whole() {
    use kition_doc_doc::graphic::{TextAnchor, TextFrame};
    // "Page 10 of" with a first-line indent in a narrow box that fits its text: one line, the
    // indent counted in the box's width.
    let mut d = Document::from_text("Body");
    let id = text_box(&mut d, 0, "Page 10 of", 20.0, 20.0, Default::default());
    if let Some(Block::Para(p)) = d.parts.get_mut(&id).and_then(|part| part.blocks.get_mut(0)).map(std::sync::Arc::make_mut) {
        p.props.indent_first = Some(23.2);
    }
    let InlineObject::Shape { body, .. } = &mut d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().objects[0] else { panic!() };
    *body = Some(TextFrame { anchor: TextAnchor::Top, insets: [0.0; 4], fit: true, no_wrap: false, fit_height: false });
    let l = lay(&d);
    let lines: Vec<_> = l.pages[0]
        .items
        .iter()
        .filter_map(|i| if let Placed::Lines { story: StoryRef::Part(s), para, .. } = i { (*s == id).then(|| para.lines.len()) } else { None })
        .collect();
    assert_eq!(lines, [1], "one line");
}

#[test]
fn a_text_box_that_fits_its_wrapped_text_takes_its_height() {
    use kition_doc_doc::graphic::{TextAnchor, TextFrame};
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // A 100 x 40 pt box set top and bottom, fitting its wrapped text (`a:spAutoFit`): WPS keeps
    // its width, makes it as tall as its text and sets the next lines below it.
    let mut d = Document::from_text("Anchor\nAfter the box");
    let float = Float { wrap: Wrap::TopAndBottom, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, ..Default::default() };
    let id = text_box(&mut d, 0, &"Text that needs several lines in a narrow box. ".repeat(4), 100.0, 40.0, float);
    let InlineObject::Shape { body, .. } = &mut d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().objects[0] else { panic!() };
    *body = Some(TextFrame { anchor: TextAnchor::Top, insets: [0.0; 4], fit: false, no_wrap: false, fit_height: true });
    let l = lay(&d);
    let r = box_rect(&l, id);
    assert!((r.w - 100.0).abs() < 0.5, "the width is kept: {r:?}");
    assert!(r.h > 80.0, "as tall as its lines: {r:?}");
    let text_bottom = l.pages[0]
        .items
        .iter()
        .filter_map(|i| if let Placed::Lines { story: StoryRef::Part(s), para, y, .. } = i { (*s == id).then(|| y + para.height) } else { None })
        .fold(0.0f32, f32::max);
    let space_after = 8.0;
    assert!(
        r.bottom() >= text_bottom - 0.5 && r.bottom() <= text_bottom + space_after + 0.5,
        "the box ends with its text and its space after: {r:?} {text_bottom}"
    );
    let after = l.caret(&Pos::body(1, 0)).unwrap();
    assert!(after.top >= r.bottom() - 0.5, "the next paragraph is set below the box: {} {r:?}", after.top);
}

#[test]
fn a_float_that_would_run_past_the_page_is_moved_up_to_its_foot() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // WPS for Mac (synthetic references): an object positioned from its paragraph that would end
    // below the page is moved up to end at the page's bottom edge; its paragraph stays put.
    let mut d = Document::from_text(&format!("{}\nAnchor\nAfter", "Filler text for the page. ".repeat(100)));
    let float = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, y: 200.0, ..Default::default() };
    let id = text_box_at(&mut d, &Pos::body(1, 0), "Box", 200.0, 250.0, float);
    let l = lay(&d);
    assert_eq!(l.caret(&Pos::body(1, 0)).unwrap().page, 0, "the paragraph stays on its page");
    let page_h = l.pages[0].h;
    let r = box_rect(&l, id);
    assert!((r.bottom() - page_h).abs() < 0.5, "the box ends at the page's foot: {r:?} {page_h}");
    assert!((r.h - 250.0).abs() < 0.5, "and keeps its size: {r:?}");
}

#[test]
fn a_float_text_doesnt_wrap_around_may_run_past_the_page() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // WPS for Mac (synthetic references): an object in front of or behind the text stays where its
    // offset puts it, past the page's foot if so; only wrapped objects move up.
    let mut d = Document::from_text(&format!("{}\nAnchor\nAfter", "Filler text for the page. ".repeat(100)));
    let float = Float { wrap: Wrap::InFrontOfText, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, y: 200.0, ..Default::default() };
    let id = text_box_at(&mut d, &Pos::body(1, 0), "Box", 200.0, 250.0, float);
    let l = lay(&d);
    let r = box_rect(&l, id);
    assert!(r.bottom() > l.pages[0].h + 1.0, "the box runs past the page: {r:?}");
}

#[test]
fn a_paragraph_offset_counts_from_above_the_paragraphs_space_before() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // WPS for Mac (synthetic references, 0 to 40 pt before): a box 20 pt below its paragraph stays
    // put however much space the paragraph has before it; only the text moves down.
    let rect = |before: f32| {
        let mut d = Document::from_text("Top\nAnchor");
        d.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.space_before = Some(before);
        let float = Float { wrap: Wrap::InFrontOfText, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, y: 20.0, ..Default::default() };
        let id = text_box_at(&mut d, &Pos::body(1, 0), "Box", 100.0, 40.0, float);
        box_rect(&lay(&d), id)
    };
    let (plain, spaced) = (rect(0.0), rect(40.0));
    assert!((plain.y - spaced.y).abs() < 0.01, "{plain:?} {spaced:?}");
}

#[test]
fn a_paragraph_kept_with_the_next_moves_on_when_its_float_pushes_the_next_off_the_page() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // WPS for Mac (synthetic references): a heading whose full-width text box pushes the next
    // paragraph onto the next page goes there with it; one whose box leaves room for the next
    // paragraph's first line below it on the page stays.
    let (mut stayed, mut moved) = (false, false);
    for fillers in 10..40 {
        let mut d = Document::from_text(&format!("{}Heading\nAfter", "Filler\n".repeat(fillers)));
        d.para_mut(StoryRef::Body, &Path::top(fillers)).unwrap().props.keep_next = Some(true);
        let float = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, y: 30.0, ..Default::default() };
        let width = d.last_section.text_width();
        let id = text_box_at(&mut d, &Pos::body(fillers, 0), "Box", width, 300.0, float);
        let l = lay(&d);
        let find = |index: usize| {
            l.pages.iter().enumerate().find_map(|(page, p)| {
                p.items.iter().find_map(|i| match i {
                    Placed::Lines { path, y, para, l0, .. } if path.0 == [index as u32] => Some((page, *y + para.lines[*l0].top)),
                    _ => None,
                })
            })
        };
        let (heading, after) = (find(fillers).unwrap(), find(fillers + 1).unwrap());
        let rect = l.pages[heading.0]
            .items
            .iter()
            .find_map(|i| if let Placed::Object { rect, text_box, .. } = i { (*text_box == Some(id)).then_some(*rect) } else { None })
            .unwrap();
        assert_eq!(heading.0, after.0, "{fillers} fillers: the heading stays with the paragraph after it");
        assert!(after.1 >= rect.bottom() - 0.01, "{fillers} fillers: the next paragraph goes below the box ({after:?}, {rect:?})");
        stayed |= heading.0 == 0;
        moved |= heading.0 == 1;
    }
    assert!(stayed && moved, "some headings stay, some move with their box");
}

#[test]
fn text_above_an_anchor_wraps_around_a_float_reaching_up_to_it() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // WPS for Mac (luan's turned text boxes): a box reaching above its paragraph's top pushes the
    // lines it meets there aside, as it does those after its anchor.
    let mut d = Document::from_text(&format!("Top\n{}\nAnchor", "Text above the anchor. ".repeat(20)));
    let float = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, y: -10.0, ..Default::default() };
    d.insert_object(&Pos::body(2, 0), rect_shape(200.0, 60.0, float), &Default::default()).unwrap();
    let l = lay(&d);
    let last = l.pages[0]
        .items
        .iter()
        .find_map(|i| match i {
            Placed::Lines { para, path, .. } if path.0 == [1] => para.lines.last().map(|line| line.left),
            _ => None,
        })
        .unwrap();
    assert!(last >= 200.0, "the line beside the box starts after it: {last}");
}

#[test]
fn a_turned_section_sets_its_lines_down_the_page_from_the_right() {
    // WPS for Mac (luan's tbRl sections): the body's lines run top to bottom from the top margin,
    // the first at the right margin and the next to its left; the page keeps its size.
    let mut d = Document::from_text(&format!("{}\nSecond", "Turned text runs down the page. ".repeat(12)));
    d.last_section.text_direction = kition_doc_doc::props::TextDirection::Down;
    let s = d.last_section.clone();
    let l = lay(&d);
    let page = &l.pages[0];
    assert_eq!((page.w, page.h), (s.page_w, s.page_h));
    let lines: Vec<(f32, f32, kition_doc_doc::props::TextDirection, f32)> = page
        .items
        .iter()
        .filter_map(|i| match i {
            Placed::Lines { x, y, turn, para, .. } => Some((*x, *y, *turn, para.lines.iter().map(|l| l.right).fold(0.0, f32::max))),
            _ => None,
        })
        .collect();
    let (x, y, turn, length) = lines[0];
    assert_eq!(turn, kition_doc_doc::props::TextDirection::Down);
    assert!((x - (s.page_w - s.margin_right)).abs() < 0.01 && (y - s.margin_top).abs() < 0.01, "first line at the top right: {x}, {y}");
    assert!(length > s.text_width() + 1.0 && length <= s.text_height() + 0.01, "lines run the body's height: {length}");
    assert!(lines[1].0 < x, "the next paragraph stands to the left: {lines:?}");
}

#[test]
fn text_moved_to_a_new_page_wraps_around_a_float_reaching_up_there() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // A paragraph kept together that moves on to the next page still wraps around a box there
    // that reaches up from the paragraph after it.
    let lines = |fillers: usize| {
        let mut d = Document::from_text(&format!("{}{}\nAnchor", "Filler\n".repeat(fillers), "Text above the anchor. ".repeat(20)));
        d.para_mut(StoryRef::Body, &Path::top(fillers)).unwrap().props.keep_lines = Some(true);
        let float = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, y: -10.0, ..Default::default() };
        d.insert_object(&Pos::body(fillers + 1, 0), rect_shape(200.0, 60.0, float), &Default::default()).unwrap();
        let l = lay(&d);
        l.pages.iter().enumerate().find_map(|(page, p)| {
            p.items.iter().find_map(|i| match i {
                Placed::Lines { para, path, l0, .. } if path.0 == [fillers as u32] && *l0 == 0 => {
                    Some((page, para.lines.last().map(|line| line.left)))
                }
                _ => None,
            })
        })
    };
    let moved = (20..40).filter_map(|n| lines(n).filter(|(page, _)| *page == 1)).next();
    let (_, last) = moved.expect("the paragraph moves on to the next page");
    assert!(last.is_some_and(|left| left >= 200.0), "its last line starts after the box: {last:?}");
}

#[test]
fn a_run_set_across_vertical_text_takes_one_em() {
    // WPS for Mac (luan's red years, `w:eastAsianLayout w:vert="1"` in a tbRl section): the run
    // takes one em along the column, its letters drawn across it.
    let mut d = Document::from_text("汉2009字");
    d.styles.default_chr.size = Some(10.5);
    d.last_section.text_direction = kition_doc_doc::props::TextDirection::Down;
    let across = kition_doc_doc::props::CharProps {
        east_asian_layout: Some(kition_doc_doc::props::EastAsianLayout { vertical: Some(true), vertical_compress: Some(true), ..Default::default() }),
        ..Default::default()
    };
    let p = d.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    let len = "汉".len();
    p.runs = vec![
        kition_doc_doc::Run { len, props: Default::default() },
        kition_doc_doc::Run { len: 4, props: across },
        kition_doc_doc::Run { len: "字".len(), props: Default::default() },
    ];
    let l = lay(&d);
    let Placed::Lines { para, .. } = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })).unwrap() else { panic!() };
    let advances: Vec<f32> = para.clusters.iter().map(|c| c.adv).collect();
    assert!((advances[1] - 10.5).abs() < 0.01 && advances[2..5].iter().all(|a| *a == 0.0), "{advances:?}");
    assert_eq!(para.across, vec![1..5]);
}

#[test]
fn a_floating_objects_anchor_does_not_make_its_line_taller() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // WPS: the run holding a floating picture (here 32 pt) adds nothing to the height of the line
    // its anchor ends; the line stays as tall as its 10.5 pt text.
    let mut d = Document::from_text("Text before the anchor");
    d.styles.default_chr.size = Some(10.5);
    let float = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, x: 300.0, ..Default::default() };
    let end = d.para(StoryRef::Body, &Path::top(0)).unwrap().text.len();
    let big = kition_doc_doc::props::CharProps { size: Some(32.0), ..Default::default() };
    d.insert_object(&Pos::body(0, end), rect_shape(40.0, 40.0, float), &big).unwrap();
    let plain = {
        let mut d = Document::from_text("Text before the anchor");
        d.styles.default_chr.size = Some(10.5);
        lay(&d)
    };
    let height = |l: &DocLayout| match l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) {
        Some(Placed::Lines { para, .. }) => para.lines[0].height,
        _ => panic!(),
    };
    let l = lay(&d);
    assert!((height(&l) - height(&plain)).abs() < 0.01, "{} vs {}", height(&l), height(&plain));
}

#[test]
fn a_text_boxs_lines_follow_the_document_grid() {
    // WPS for Mac spaces a text box's lines on the section's line grid, as it does the
    // body's: 10.5 pt single lines take one 15.6 pt row (synthetic references).
    let mut d = Document::from_text("Anchor");
    d.styles.default_chr.size = Some(10.5);
    d.last_section.grid = Some(kition_doc_doc::section::DocumentGrid { kind: Some("lines".into()), line_pitch: Some(312), char_space: None });
    let id = text_box(&mut d, 0, &"Text in a grid. ".repeat(20), 150.0, 300.0, Default::default());
    if let Some(Block::Para(p)) = d.parts.get_mut(&id).and_then(|part| part.blocks.get_mut(0)).map(std::sync::Arc::make_mut) {
        p.props.line_spacing = Some(kition_doc_doc::props::LineSpacing::Multiple(1.0));
        p.props.space_after = Some(0.0);
    }
    let l = lay(&d);
    let para = l.pages[0]
        .items
        .iter()
        .find_map(|i| if let Placed::Lines { story: StoryRef::Part(s), para, .. } = i { (*s == id).then(|| para.clone()) } else { None })
        .expect("the box's lines");
    let step = para.lines[1].baseline - para.lines[0].baseline;
    assert!((step - 15.6).abs() < 0.01, "line step {step}");
}

#[test]
fn a_text_box_that_does_not_wrap_keeps_a_long_line_whole() {
    use kition_doc_doc::graphic::{TextAnchor, TextFrame};
    // `wrap="none"` without `a:spAutoFit`: the box keeps its size and its text runs past it on
    // one line, as WPS shows a label wider than its box.
    let mut d = Document::from_text("Body");
    let id = text_box(&mut d, 0, "LEOPARD LEOPARD", 30.0, 30.0, Default::default());
    let InlineObject::Shape { body, .. } = &mut d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().objects[0] else { panic!() };
    *body = Some(TextFrame { anchor: TextAnchor::Top, insets: [0.0; 4], fit: false, no_wrap: true, fit_height: false });
    let l = lay(&d);
    let r = box_rect(&l, id);
    assert!((r.w - 30.0).abs() < 0.5 && (r.h - 30.0).abs() < 0.5, "the box keeps its size: {r:?}");
    let lines: Vec<_> = l.pages[0]
        .items
        .iter()
        .filter_map(|i| if let Placed::Lines { story: StoryRef::Part(s), para, .. } = i { (*s == id).then(|| para.lines.len()) } else { None })
        .collect();
    assert_eq!(lines, [1], "one line");
    let caret = l.caret(&Pos { story: StoryRef::Part(id), path: Path::top(0), off: 0 }).unwrap();
    assert!((caret.x - r.x).abs() < 0.5, "its text starts at the box's left: {} in {r:?}", caret.x);
}

#[test]
fn clicks_inside_a_text_box_hit_its_story() {
    let mut d = Document::from_text("Before after");
    let id = text_box(&mut d, 7, "Inside the box", 144.0, 72.0, Default::default());
    let l = lay(&d);
    let r = box_rect(&l, id);
    assert!((r.w - 144.0).abs() < 0.5 && (r.h - 72.0).abs() < 0.5, "{r:?}");
    let box_story = StoryRef::Part(id);
    // Anywhere inside the box, even below its text, is the box.
    for (x, y) in [(r.x + 20.0, r.y + 10.0), (r.x + r.w - 4.0, r.y + r.h - 4.0)] {
        assert_eq!(l.story_at(0, x, y), Some(box_story), "({x}, {y}) in {r:?}");
        assert_eq!(l.hit(0, x, y, box_story).map(|p| p.story), Some(box_story));
    }
    // The body text beside it is the body; so is the margin just right of the box, although the
    // box's own lines are within reach there.
    let c = l.caret(&Pos::body(0, 2)).unwrap();
    assert_eq!(l.story_at(0, c.x, c.top + c.height / 2.0), Some(StoryRef::Body));
    assert_ne!(l.story_at(0, r.right() + 8.0, r.y + 10.0), Some(box_story));
    // The caret in the box sits inside it.
    let bc = l.caret(&Pos { story: box_story, path: Path::top(0), off: 0 }).unwrap();
    assert!(r.contains(kition_doc_geom::Point::new(bc.x, bc.top)), "{bc:?} outside {r:?}");
}

#[test]
fn body_text_wins_over_a_text_box_behind_it() {
    let mut d = Document::from_text("Hi");
    let float = kition_doc_doc::para::Float {
        wrap: kition_doc_doc::para::Wrap::BehindText,
        h_rel: kition_doc_doc::para::Anchor::Column,
        v_rel: kition_doc_doc::para::Anchor::Paragraph,
        x: 0.0,
        y: 0.0,
        dist: 0.0,
        ..Default::default()
    };
    let id = text_box(&mut d, 0, "Behind", 200.0, 150.0, float);
    let l = lay(&d);
    let r = box_rect(&l, id);
    let c = l.caret(&Pos::body(0, 0)).unwrap();
    assert!(r.contains(kition_doc_geom::Point::new(c.x + 2.0, c.top + 2.0)), "box covers the text: {r:?} {c:?}");
    assert_eq!(l.story_at(0, c.x + 2.0, c.top + c.height / 2.0), Some(StoryRef::Body));
    assert_eq!(l.story_at(0, r.right() - 10.0, r.bottom() - 10.0), Some(StoryRef::Part(id)));
}

#[test]
fn presses_grab_pictures_anywhere_and_text_boxes_by_their_border() {
    let mut d = Document::from_text("Words before the objects.");
    let id = text_box(&mut d, 0, "Inside", 144.0, 72.0, Default::default());
    let shape = InlineObject::Shape {
        vml: None,
        kind: kition_doc_doc::para::ShapeKind::Rectangle,
        w: 60.0,
        h: 40.0,
        fill: None,
        stroke: None,
        stroke_width: 1.0,
        float: Default::default(),
        story: None,
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    d.insert_object(&Pos::body(0, 0), shape, &Default::default()).unwrap();
    let l = lay(&d);
    let b = box_rect(&l, id);
    // The text box: its border grabs it, inside is its text.
    let edge = l.object_at(0, b.x + 1.0, b.y + b.h / 2.0, 4.0).unwrap();
    assert_eq!(edge.text_box, Some(id));
    assert_eq!(l.object(&edge.pos(), 0).map(|o| o.rect), Some(b));
    assert_eq!(l.text_box(id, 5).map(|o| o.rect), Some(b), "a bad hint still finds it");
    assert!(l.object_at(0, b.x + b.w / 2.0, b.y + b.h / 2.0, 4.0).is_none());
    // The shape: anywhere on it.
    let s = l.find_object(0, |o| o.text_box.is_none()).unwrap();
    let hit = l.object_at(0, s.rect.x + s.rect.w / 2.0, s.rect.y + s.rect.h / 2.0, 4.0).unwrap();
    assert_eq!((hit.off, hit.text_box, hit.floating()), (0, None, false));
    // Plain text: nothing.
    let c = l.caret(&Pos::body(0, 10)).unwrap();
    assert!(l.object_at(0, c.x, c.top + 2.0, 4.0).is_none());
}

/// (lines placed, lines laid out) of a text box's story, and its area.
fn box_lines(l: &DocLayout, id: u32) -> (usize, usize, kition_doc_geom::Rect) {
    let mut shown = 0;
    let mut total = 0;
    for it in &l.pages[0].items {
        if let Placed::Lines { story: StoryRef::Part(p), para, l0, l1, y, .. } = it
            && *p == id
        {
            shown += l1 - l0;
            total += para.lines.len();
            let first = &para.lines[*l0];
            let last = &para.lines[l1 - 1];
            // Every shown line starts inside the box; the last may run past its edge, where it is
            // clipped (as WPS draws it).
            let top = y + last.top - first.top;
            let r = box_rect(l, id);
            assert!(top < r.bottom() || shown == 1, "a placed line starts below the box: {top} >= {}", r.bottom());
        }
    }
    (shown, total, box_rect(l, id))
}

#[test]
fn a_line_that_starts_inside_a_text_box_is_drawn_clipped_at_its_edge() {
    // WPS for Mac (synthetic reference): a 40 pt box with insets of 3.6 pt top and bottom shows
    // its lines down to the third, which starts inside it and is cut off at the box's edge.
    let long = "Calibration text that overflows. ".repeat(12);
    let mut d = Document::from_text("Body");
    let id = text_box(&mut d, 0, &long, 150.0, 40.0, Default::default());
    if let Some(Block::Para(p)) = d.parts.get_mut(&id).and_then(|part| part.blocks.get_mut(0)).map(std::sync::Arc::make_mut) {
        p.props.line_spacing = Some(kition_doc_doc::props::LineSpacing::Exactly(15.6));
        p.props.space_after = Some(0.0);
    }
    let l = lay(&d);
    let (shown, total, r) = box_lines(&l, id);
    assert_eq!(shown, 3, "{shown} of {total} lines shown in {r:?}");
    let draws = display::page_display(&d, &l.pages[0], &Default::default());
    let clipped = draws.iter().find_map(|x| if let display::Draw::Clipped { rect, items } = x { Some((*rect, items.len())) } else { None });
    assert!(clipped.is_some_and(|(rect, n)| (rect.bottom() - r.bottom()).abs() < 0.01 && n > 0), "the text is clipped to the box: {clipped:?} {r:?}");
}

#[test]
fn text_box_hides_text_that_does_not_fit() {
    let long = "The quick brown fox jumps over the lazy dog. ".repeat(12);
    let mut d = Document::from_text("Body");
    let id = text_box(&mut d, 0, &long, 144.0, 72.0, Default::default());
    let (shown, total, r) = box_lines(&lay(&d), id);
    assert!(shown >= 2 && shown < total, "{shown} of {total} lines shown in {r:?}");
    // Nothing below the box belongs to it.
    let l = lay(&d);
    assert_ne!(l.story_at(0, r.x + 20.0, r.bottom() + 6.0), Some(StoryRef::Part(id)));
    // A taller box shows more; a tiny one still shows its first line.
    let mut d2 = Document::from_text("Body");
    let id2 = text_box(&mut d2, 0, &long, 144.0, 1500.0, Default::default());
    let (shown2, total2, _) = box_lines(&lay(&d2), id2);
    assert_eq!(shown2, total2);
    let mut d3 = Document::from_text("Body");
    let id3 = text_box(&mut d3, 0, &long, 144.0, 18.0, Default::default());
    assert_eq!(box_lines(&lay(&d3), id3).0, 1);
}

#[test]
fn square_wrap_flows_text_on_both_sides() {
    let float = |x: f32| kition_doc_doc::para::Float {
        wrap: kition_doc_doc::para::Wrap::Square,
        h_rel: kition_doc_doc::para::Anchor::Column,
        v_rel: kition_doc_doc::para::Anchor::Paragraph,
        x,
        y: 0.0,
        dist: 9.0,
        ..Default::default()
    };
    let text = "Words flow on both sides of the box in the middle here. ".repeat(20);
    let mut d = Document::from_text(&text);
    let id = text_box(&mut d, 0, "Middle", 144.0, 100.0, float(160.0));
    let l = lay(&d);
    let r = box_rect(&l, id);
    let Placed::Lines { para, x, y, .. } = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { story: StoryRef::Body, .. })).unwrap() else {
        panic!()
    };
    let beside: Vec<&para::Line> = para.lines.iter().filter(|ln| ln.beside).collect();
    assert!(beside.len() >= 4, "{} rows have text on the far side", beside.len());
    for (k, ln) in para.lines.iter().enumerate().filter(|(_, ln)| ln.beside) {
        let prev = &para.lines[k - 1];
        assert_eq!((prev.top, prev.height), (ln.top, ln.height), "one row");
        assert!(x + ink_end(para, prev) <= r.x + 0.01, "left part stops before the box");
        assert!(x + ln.xs[0] >= r.right(), "right part starts after it");
        assert_eq!(prev.stop, ln.start, "text runs left part, then right part");
    }
    // Below the box: full-width rows again.
    let below = para.lines.iter().find(|ln| y + ln.top > r.bottom() + 10.0).unwrap();
    assert!(!below.beside && below.right - below.left > 400.0);
    // The right-hand part is hittable and has a caret.
    let ln = beside[0];
    let pos = l.hit(0, x + ln.xs[1], y + ln.top + 2.0, StoryRef::Body).unwrap();
    assert!(pos.off >= ln.start && pos.off <= ln.stop, "{pos:?}");
    assert!(l.caret(&pos).unwrap().x >= r.right());
    // A gap too narrow for text stays empty.
    let mut d2 = Document::from_text(&text);
    let id2 = text_box(&mut d2, 0, "Edge", 144.0, 100.0, float(468.0 - 144.0 - 20.0));
    let l2 = lay(&d2);
    let r2 = box_rect(&l2, id2);
    let Placed::Lines { para, x, .. } = l2.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { story: StoryRef::Body, .. })).unwrap() else {
        panic!()
    };
    assert!(para.lines.iter().all(|ln| !ln.beside && ink_end(para, ln) <= (r2.x - x).max(ln.right) + 0.01));
}

/// Where a line's text ends (its last non-space cluster's right edge; trailing spaces may hang
/// past the margin), relative to the column.
fn ink_end(pl: &ParaLayout, ln: &para::Line) -> f32 {
    (ln.c0..ln.c1)
        .filter_map(|k| {
            let c = pl.clusters.get(k)?;
            (c.kind != para::ClKind::Space).then_some(ln.xs.get(k - ln.c0)? + c.adv)
        })
        .fold(0.0, f32::max)
}

fn footnote(d: &mut Document, pos: &Pos, text: &str) -> u32 {
    let id = d.add_part(
        kition_doc_doc::PartKind::Footnote,
        vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(text, Default::default()))],
    );
    d.insert_object(pos, InlineObject::NoteRef { kind: kition_doc_doc::para::NoteKind::Footnote, id, custom: String::new() }, &Default::default())
        .unwrap();
    id
}

fn page_float(x: f32, y: f32) -> kition_doc_doc::para::Float {
    kition_doc_doc::para::Float {
        wrap: kition_doc_doc::para::Wrap::InFrontOfText,
        h_rel: kition_doc_doc::para::Anchor::Page,
        v_rel: kition_doc_doc::para::Anchor::Page,
        x,
        y,
        dist: 0.0,
        ..Default::default()
    }
}

#[test]
fn text_boxes_show_in_headers_footers_cells_and_notes() {
    let mut d = Document::from_text(&"Body paragraph.\n".repeat(4));
    // Header: an inline box and a box placed on the page.
    let hid = d.add_part(
        kition_doc_doc::PartKind::Header,
        vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("Head", Default::default()))],
    );
    d.last_section.headers.default = Some(hid);
    let e = d.end_of(StoryRef::Part(hid));
    let inline = text_box_at(&mut d, &e, "Inline in header", 100.0, 30.0, Default::default());
    let e = d.end_of(StoryRef::Part(hid));
    let placed = text_box_at(&mut d, &e, "On the page", 100.0, 40.0, page_float(400.0, 20.0));
    // Footer: a box placed on the page (the footer is laid out twice to find it).
    let fid = d.add_part(
        kition_doc_doc::PartKind::Footer,
        vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("Foot", Default::default()))],
    );
    d.last_section.footers.default = Some(fid);
    let e = d.end_of(StoryRef::Part(fid));
    let in_footer = text_box_at(&mut d, &e, "Footer box", 100.0, 30.0, page_float(300.0, 740.0));
    // A table cell and a footnote.
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(kition_doc_doc::Table::new(1, 1, 300.0))).unwrap();
    let cell = Pos { story: StoryRef::Body, path: Path(vec![1, 0, 0, 0]), off: 0 };
    let in_cell = text_box_at(&mut d, &cell, "In a cell", 120.0, 30.0, Default::default());
    let note = footnote(&mut d, &Pos::body(0, 4), "Note: ");
    let e = d.end_of(StoryRef::Part(note));
    let in_note = text_box_at(&mut d, &e, "In a note", 120.0, 30.0, Default::default());
    let l = lay(&d);
    let p = &l.pages[0];
    let shows =
        |id: u32| p.header.iter().chain(&p.footer).chain(&p.items).any(|i| matches!(i, Placed::Lines { story: StoryRef::Part(s), .. } if *s == id));
    for (name, id) in
        [("inline header box", inline), ("page header box", placed), ("footer box", in_footer), ("cell box", in_cell), ("note box", in_note)]
    {
        assert!(shows(id), "{name}: no text");
        assert!(l.caret(&d.start_of(StoryRef::Part(id))).is_some(), "{name}: no caret");
    }
    // Page-anchored boxes land where they say, header and footer alike; their shapes are drawn.
    assert_eq!(l.text_box(placed, 0).map(|o| (o.rect.x, o.rect.y)), Some((400.0, 20.0)));
    assert_eq!(l.text_box(in_footer, 0).map(|o| (o.rect.x, o.rect.y)), Some((300.0, 740.0)));
    assert!(p.header.iter().any(|i| matches!(i, Placed::Shape { rect, .. } if rect.x == 400.0)));
    // Header boxes are found for clicks while editing the header.
    let r = l.text_box(inline, 0).unwrap().rect;
    assert_eq!(l.header_footer_text_box_at(0, r.x + 5.0, r.y + 5.0), Some(inline));
}

#[test]
fn footnotes_inside_text_boxes_are_numbered_and_placed() {
    let mut d = Document::from_text(&"Body text line.\n".repeat(30));
    let id = text_box_at(&mut d, &Pos::body(2, 0), "Box text", 144.0, 40.0, Default::default());
    let e = d.end_of(StoryRef::Part(id));
    let in_box = footnote(&mut d, &e, "Note from the box.");
    let after = footnote(&mut d, &Pos::body(5, 4), "Note from the body.");
    let l = lay(&d);
    // Both notes are at the bottom of page 1, the box's first (reading order).
    let a = l.caret(&d.start_of(StoryRef::Part(in_box))).expect("the box's footnote is placed");
    let b = l.caret(&d.start_of(StoryRef::Part(after))).unwrap();
    assert_eq!((a.page, b.page), (0, 0));
    assert!(a.top > 600.0 && a.top < b.top, "{a:?} {b:?}");
    let nums = note_numbers(&d, false);
    assert_eq!((nums.get(&in_box), nums.get(&after)), (Some(&1), Some(&2)));
}

/// A document whose text box `ids[k]` holds `fan` shapes showing `ids[k + 1]` (the last one shows
/// itself, `fan` times): hand-built, since the app never builds these.
fn box_fan_out(levels: usize, fan: usize) -> Document {
    let (w, h) = (40.0, 20.0);
    let mut d = Document::from_text("Body");
    let ids: Vec<u32> = (0..levels)
        .map(|k| {
            d.add_part(
                kition_doc_doc::PartKind::TextBox,
                vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(&format!("Level {k}"), Default::default()))],
            )
        })
        .collect();
    let shape = |story| InlineObject::Shape {
        vml: None,
        kind: kition_doc_doc::para::ShapeKind::TextBox,
        w,
        h,
        fill: None,
        stroke: None,
        stroke_width: 0.0,
        float: Default::default(),
        story: Some(story),
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    for (k, id) in ids.iter().enumerate() {
        let next = *ids.get(k + 1).unwrap_or(id);
        for _ in 0..fan {
            d.insert_object(&Pos { story: StoryRef::Part(*id), path: Path::top(0), off: 0 }, shape(next), &Default::default()).unwrap();
        }
    }
    d.insert_object(&Pos::body(0, 0), shape(ids[0]), &Default::default()).unwrap();
    d
}

#[test]
fn self_showing_and_fanned_out_text_boxes_stay_bounded() {
    // A box whose 30 shapes all show itself: laid out once.
    assert_eq!(lay(&box_fan_out(1, 30)).text_boxes, 1);
    // Chains of boxes each showing the next 30 times (30^4 expansions unbounded).
    let d = box_fan_out(5, 30);
    let l = lay(&d);
    assert!(l.text_boxes <= 1 + kition_doc_doc::BoxBudget::MAX_NESTED, "{} boxes laid out", l.text_boxes);
    // Footnote numbering's walk is bounded the same way.
    let mut visits = 0usize;
    d.objects_in_reading_order(&mut |_| visits += 1);
    assert!(visits < 5_000, "{visits} visits");
}

#[test]
fn many_text_boxes_and_a_header_box_all_show() {
    // Outermost boxes aren't budgeted: a long document's boxes, and its header's on every page,
    // all get their text.
    let mut d = Document::from_text("Paragraph with a box.");
    for _ in 1..400 {
        let end = d.end_of(StoryRef::Body);
        d.split_paragraph(&end).unwrap();
    }
    let ids: Vec<u32> = (0..400).map(|k| text_box_at(&mut d, &Pos::body(k, 0), &format!("Box {k}"), 100.0, 30.0, Default::default())).collect();
    let hid = d.add_part(
        kition_doc_doc::PartKind::Header,
        vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("Head", Default::default()))],
    );
    d.last_section.headers.default = Some(hid);
    let e = d.end_of(StoryRef::Part(hid));
    let in_header = text_box_at(&mut d, &e, "Header box", 100.0, 30.0, Default::default());
    let l = lay(&d);
    assert!(l.pages.len() >= 10, "{} pages", l.pages.len());
    for id in ids {
        assert!(l.caret(&d.start_of(StoryRef::Part(id))).is_some(), "box {id} has no text");
    }
    for (i, p) in l.pages.iter().enumerate() {
        let shown = p.header.iter().any(|it| matches!(it, Placed::Lines { story: StoryRef::Part(s), .. } if *s == in_header));
        assert!(shown, "page {i}: the header box has no text");
    }
}

#[test]
fn a_text_box_inside_a_text_box_still_shows_its_text() {
    let mut d = Document::from_text("Body");
    let outer = text_box(&mut d, 0, "Outer ", 300.0, 200.0, Default::default());
    let e = d.end_of(StoryRef::Part(outer));
    let inner = text_box_at(&mut d, &e, "Inner", 120.0, 40.0, Default::default());
    let l = lay(&d);
    assert_eq!(l.text_boxes, 2);
    assert!(l.caret(&d.start_of(StoryRef::Part(inner))).is_some(), "the inner box's text is laid out");
}

fn picture(w: f32, h: f32, float: kition_doc_doc::para::Float) -> InlineObject {
    InlineObject::Shape {
        vml: None,
        kind: kition_doc_doc::para::ShapeKind::Rectangle,
        w,
        h,
        fill: None,
        stroke: None,
        stroke_width: 1.0,
        float,
        story: None,
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    }
}

#[test]
fn document_grid_rounds_up_inline_picture_lines_to_whole_rows() {
    // WPS rounds a line holding an inline picture or diagram up to whole grid rows like a text
    // line (a 303.5 pt diagram line takes 312 pt, a 542.85 pt picture line 546 pt on a 15.6 pt grid).
    let mut d = Document::from_text("");
    d.last_section.grid = Some(kition_doc_doc::section::DocumentGrid { kind: Some("lines".into()), line_pitch: Some(312), char_space: None });
    if let Some(Block::Para(p)) = d.body.get_mut(0).map(std::sync::Arc::make_mut) {
        p.insert_object(0, picture(144.0, 144.0, Default::default()), &Default::default()).unwrap();
    }
    let l = lay(&d);
    let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|item| matches!(item, Placed::Lines { .. })) else { panic!() };
    let natural = 144.0 + para.styles[0].descent;
    let rows = (natural / 15.6 - 0.00001).ceil() * 15.6;
    assert!((para.lines[0].height - rows).abs() < 0.01, "{} vs {rows}", para.lines[0].height);
}

#[test]
fn document_grid_leaves_out_the_descent_of_a_line_of_only_pictures() {
    use kition_doc_doc::graphic::{Graphic, GraphicKind};
    // WPS gives a line holding only a 108 pt diagram or picture on a 15.6 pt grid 7 rows
    // (109.2 pt), though the object and the font's descent below it are taller than that: the
    // descent adds no row, and the object keeps to the line's top. With text beside it, the line
    // counts the text's descent (8 rows).
    let diagram = InlineObject::Graphic {
        w: 216.0,
        h: 108.0,
        alt: String::new(),
        float: Default::default(),
        graphic: Arc::new(Graphic { kind: GraphicKind::Diagram, ..Default::default() }),
    };
    for (object, text, rows) in
        [(diagram, "", 7.0), (picture(216.0, 108.0, Default::default()), "", 7.0), (picture(216.0, 108.0, Default::default()), " Caption", 8.0)]
    {
        let mut d = Document::from_text(text);
        d.last_section.grid = Some(kition_doc_doc::section::DocumentGrid { kind: Some("lines".into()), line_pitch: Some(312), char_space: None });
        if let Some(Block::Para(p)) = d.body.get_mut(0).map(std::sync::Arc::make_mut) {
            p.insert_object(0, object, &Default::default()).unwrap();
        }
        let l = lay(&d);
        let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|item| matches!(item, Placed::Lines { .. })) else { panic!() };
        let line = &para.lines[0];
        assert!(108.0 + para.styles[0].descent > 109.2, "the case needs a descent past the seventh row");
        assert!((line.height - rows * 15.6).abs() < 0.01, "{text:?}: {} vs {rows} rows", line.height);
        assert!(line.baseline - line.top >= 108.0 - 0.01, "the object starts below the line's top: baseline {} top {}", line.baseline, line.top);
    }
}

#[test]
fn line_spacing_does_not_scale_pictures() {
    use kition_doc_doc::props::LineSpacing;
    let mut d = Document::from_text("");
    if let Some(kition_doc_doc::Block::Para(p)) = d.body.get_mut(0).map(std::sync::Arc::make_mut) {
        p.props.line_spacing = Some(LineSpacing::Multiple(1.15));
        p.insert_object(0, picture(144.0, 144.0, Default::default()), &Default::default()).unwrap();
    }
    let l = lay(&d);
    let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
    let line = &para.lines[0];
    let desc = para.styles[0].descent;
    // Word: the picture plus the font's descent, not (picture + descent) x 1.15.
    assert!((line.height - (144.0 + desc)).abs() < 0.01, "line {} for a 144pt picture (descent {desc})", line.height);
    assert!((line.baseline - line.top - 144.0).abs() < 0.01, "the picture stands on the baseline");
}

#[test]
fn body_starts_right_below_a_tall_header() {
    let mut d = Document::from_text("Body");
    let mut hp = kition_doc_doc::Paragraph::with_text("", Default::default());
    hp.insert_object(0, picture(100.0, 80.0, Default::default()), &Default::default()).unwrap();
    hp.props.space_after = Some(0.0);
    let id = d.add_part(kition_doc_doc::PartKind::Header, vec![kition_doc_doc::para_block(hp)]);
    d.last_section.headers.default = Some(id);
    let l = lay(&d);
    let Some(Placed::Lines { para, y, .. }) = l.pages[0].header.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
    let header_bottom = y + para.height;
    assert!(header_bottom > d.last_section.margin_top, "the header reaches past the top margin");
    // No gap: the body's first paragraph starts where the header ends (Word).
    let body = l.pages[0].items.iter().find_map(|i| if let Placed::Lines { y, story: StoryRef::Body, .. } = i { Some(*y) } else { None }).unwrap();
    assert!((body - header_bottom).abs() < 0.01, "body at {body}, header ends at {header_bottom}");
}

#[test]
fn body_ends_right_above_a_tall_footer() {
    // A footer reaching past the bottom margin pushes the body's end up, as a tall header pushes
    // its start down (WPS for Mac: luan.docx's two-line footer moves a heading to the next page).
    let mut d = Document::from_text(&"Body line\n".repeat(80));
    let mut fp = kition_doc_doc::Paragraph::with_text("", Default::default());
    fp.insert_object(0, picture(100.0, 80.0, Default::default()), &Default::default()).unwrap();
    fp.props.space_after = Some(0.0);
    let id = d.add_part(kition_doc_doc::PartKind::Footer, vec![kition_doc_doc::para_block(fp)]);
    d.last_section.footers.default = Some(id);
    let l = lay(&d);
    let s = &d.last_section;
    let Some(Placed::Lines { y: footer_top, .. }) = l.pages[0].footer.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
    assert!(*footer_top < s.page_h - s.margin_bottom, "the footer reaches past the bottom margin");
    // Where the body's text ends (the room line spacing adds below it may reach further).
    let body_end = l.pages[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Placed::Lines { y, para, l0, l1, story: StoryRef::Body, .. } => {
                let (first, last) = (&para.lines[*l0], &para.lines[*l1 - 1]);
                Some(y + last.baseline + last.descent - first.top)
            }
            _ => None,
        })
        .fold(0.0f32, f32::max);
    assert!(body_end <= footer_top + 0.01, "body text ends at {body_end}, the footer starts at {footer_top}");
    assert!(body_end > footer_top - 30.0, "the body still fills the page down to the footer: {body_end}");
}

#[test]
fn inline_picture_keeps_room_for_its_effects() {
    let mut d = Document::from_text("");
    let shadow = kition_doc_doc::para::Float { effect: [6.0, 12.0, 18.0, 27.0], ..Default::default() };
    if let Some(kition_doc_doc::Block::Para(p)) = d.body.get_mut(0).map(std::sync::Arc::make_mut) {
        p.insert_object(0, picture(100.0, 50.0, shadow), &Default::default()).unwrap();
    }
    let l = lay(&d);
    let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
    let c = para.clusters.iter().find(|c| matches!(c.kind, para::ClKind::Object(_))).unwrap();
    assert_eq!((c.adv, c.obj_h), (124.0, 89.0), "picture plus effect extents");
    // Drawn inside that room.
    let obj = picture(100.0, 50.0, shadow);
    let r = display::inline_rect(Some(&obj), 10.0, 200.0, c.adv, c.obj_h);
    assert_eq!((r.x, r.y, r.w, r.h), (16.0, 123.0, 100.0, 50.0));
}

/// Tops of the body text lines drawn on page one, in order.
fn line_tops(l: &DocLayout) -> Vec<f32> {
    l.pages[0].items.iter().filter_map(|i| if let Placed::Lines { y, story: StoryRef::Body, .. } = i { Some(*y) } else { None }).collect()
}

#[test]
fn at_least_rows_add_cell_margins_and_border_bands() {
    use kition_doc_doc::props::{Border, BorderStyle, Borders, HeightRule};
    let mut d = Document::from_text("");
    let line = Some(Border { style: BorderStyle::Single, width: 0.5, color: None, space: 0.0, theme: None });
    let mut t = Table::new(2, 1, 200.0);
    t.props.borders = Some(Borders { top: line, left: line, bottom: line, right: line, between: line, inside_v: line });
    t.props.cell_margins = Some([5.0, 5.4, 5.0, 5.4]);
    for (r, text) in t.rows.iter_mut().zip(["one", "two"]) {
        r.props.height = Some(30.0);
        r.props.height_rule = HeightRule::AtLeast;
        r.cells[0].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(text, Default::default()))];
    }
    d.body = vec![std::sync::Arc::new(kition_doc_doc::Block::Table(t))];
    let tops = line_tops(&lay(&d));
    // Word: a 30pt at-least row with 5pt margins and 0.5pt borders steps 40.5pt, and the text
    // starts below the top border and the top margin.
    assert!((tops[1] - tops[0] - 40.5).abs() < 0.01, "row pitch {}", tops[1] - tops[0]);
    assert!((tops[0] - (72.0 + 0.5 + 5.0)).abs() < 0.01, "first cell text at {}", tops[0]);
}

/// Line count and, per line, how far the text (trailing spaces excluded) reaches.
fn justified_lines(text: &str, compat_mode: u32, align: Align) -> (usize, Vec<f32>) {
    let mut d = Document::from_text(text);
    d.settings.compat_mode = compat_mode;
    d.format_paragraphs(&Pos::body(0, 0), &Pos::body(0, 0), &|p| p.align = Some(align)).unwrap();
    let l = lay(&d);
    let mut ends = Vec::new();
    for p in &l.pages {
        for it in &p.items {
            if let Placed::Lines { para, l0, l1, x, .. } = it {
                for li in *l0..*l1 {
                    let line = &para.lines[li];
                    let content = text[line.start..line.stop].trim_end().len();
                    ends.push(para.x_of(li, line.start + content).unwrap() + x);
                }
            }
        }
    }
    (lines_of(&l), ends)
}

#[test]
fn justified_lines_shrink_spaces_in_word_2013_mode() {
    let text = "We tie the spend to a pipe and to the books, so it is in an ad hoc view of all of it. ".repeat(40);
    let (modern, ends) = justified_lines(&text, 15, Align::Justify);
    let (legacy, legacy_ends) = justified_lines(&text, 14, Align::Justify);
    assert!(modern < legacy, "mode 15 fits more per line: {modern} vs {legacy} lines");
    // Shrinking only ever pulls a line back inside the margin.
    for e in ends.iter().chain(&legacy_ends) {
        assert!(*e <= 540.0 + 0.05, "line ends past the right margin: {e}");
    }
    // Justified lines still reach the margin; the last line is the only short one.
    for e in &ends[..ends.len() - 1] {
        assert!((*e - 540.0).abs() < 0.05, "justified line ends at {e}");
    }
}

#[test]
fn space_shrinking_is_only_for_justified_text() {
    let text = "We tie the spend to a pipe and to the books, so it is in an ad hoc view of all of it. ".repeat(40);
    for align in [Align::Left, Align::Center, Align::Right] {
        assert_eq!(justified_lines(&text, 15, align).0, justified_lines(&text, 14, align).0, "{align:?}");
    }
}

#[test]
fn lines_with_tabs_never_shrink() {
    let text = "Item\tWe tie the spend to a pipe and to the books, so it is in an ad hoc view of all of it. ".repeat(30);
    let (modern, _) = justified_lines(&text, 15, Align::Justify);
    let (legacy, _) = justified_lines(&text, 14, Align::Justify);
    assert!(modern <= legacy);
    let (_, ends) = justified_lines(&text, 15, Align::Justify);
    for e in &ends {
        assert!(*e <= 540.0 + 0.05, "{e}");
    }
}

fn rect_shape(w: f32, h: f32, float: kition_doc_doc::para::Float) -> InlineObject {
    InlineObject::Shape {
        vml: None,
        kind: kition_doc_doc::para::ShapeKind::Rectangle,
        w,
        h,
        fill: None,
        stroke: None,
        stroke_width: 1.0,
        float,
        story: None,
        freeform: None,
        effects: Default::default(),
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    }
}

fn shapes(items: &[Placed]) -> Vec<Rect> {
    items.iter().filter_map(|i| if let Placed::Shape { rect, .. } = i { Some(*rect) } else { None }).collect()
}

/// Top of the first body line drawn on a page.
fn first_line_top(items: &[Placed]) -> f32 {
    items.iter().find_map(|i| if let Placed::Lines { y, story: StoryRef::Body, .. } = i { Some(*y) } else { None }).unwrap()
}

#[test]
fn floats_align_within_their_reference_area() {
    use kition_doc_doc::para::{Anchor, Float, FloatAlign, Wrap};
    let mut d = Document::from_text("Some text beside the pictures.");
    let centred = Float { wrap: Wrap::InFrontOfText, h_rel: Anchor::Margin, h_align: Some(FloatAlign::Center), x: 999.0, ..Default::default() };
    let right = Float { wrap: Wrap::InFrontOfText, h_rel: Anchor::Page, h_align: Some(FloatAlign::End), ..Default::default() };
    let in_left_margin = Float { wrap: Wrap::InFrontOfText, h_rel: Anchor::LeftMargin, x: 10.0, ..Default::default() };
    let bottom = Float { wrap: Wrap::InFrontOfText, v_rel: Anchor::BottomMargin, v_align: Some(FloatAlign::Start), ..Default::default() };
    for f in [centred, right, in_left_margin, bottom] {
        d.insert_object(&Pos::body(0, 0), rect_shape(100.0, 50.0, f), &Default::default()).unwrap();
    }
    let l = lay(&d);
    // Objects are inserted at the start, so they come out in reverse order.
    let r = shapes(&l.pages[0].items);
    assert_eq!(r.len(), 4, "{r:?}");
    assert!((r[3].x - (72.0 + (468.0 - 100.0) / 2.0)).abs() < 0.01, "centred in the margins: {:?}", r[3]);
    assert!((r[2].right() - 612.0).abs() < 0.01, "right edge of the page: {:?}", r[2]);
    assert!((r[1].x - 10.0).abs() < 0.01, "measured from the page's left edge: {:?}", r[1]);
    assert!((r[0].y - (792.0 - 72.0)).abs() < 0.01, "top of the bottom margin: {:?}", r[0]);
}

#[test]
fn top_and_bottom_float_pushes_its_own_first_line_down() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    let mut d = Document::from_text(&"Text that starts below the picture. ".repeat(10));
    let f = Float { wrap: Wrap::TopAndBottom, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, dist_bottom: 6.0, ..Default::default() };
    d.insert_object(&Pos::body(0, 0), rect_shape(144.0, 100.0, f), &Default::default()).unwrap();
    let l = lay(&d);
    let r = shapes(&l.pages[0].items)[0];
    let top = first_line_top(&l.pages[0].items);
    assert!(top >= r.bottom() + 6.0 - 0.01, "first line {top} drawn under the picture ending at {}", r.bottom());
    // The caret agrees with what is drawn.
    let c = l.caret(&Pos::body(0, 4)).unwrap();
    assert!((c.top - top).abs() < 2.0, "{c:?} vs {top}");
}

#[test]
fn float_follows_its_paragraph_to_the_next_page() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    let mut d = Document::from_text(&format!("First page.\n{}", "Text beside the picture on page two. ".repeat(20)));
    let f = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, dist: 9.0, ..Default::default() };
    d.insert_object(&Pos::body(1, 0), rect_shape(144.0, 100.0, f), &Default::default()).unwrap();
    if let Some(kition_doc_doc::Block::Para(p)) = d.body.get_mut(1).map(std::sync::Arc::make_mut) {
        p.props.page_break_before = Some(true);
    }
    let l = lay(&d);
    assert_eq!(l.pages.len(), 2);
    assert!(shapes(&l.pages[0].items).is_empty());
    let r = shapes(&l.pages[1].items)[0];
    assert!((r.y - 72.0).abs() < 0.5, "at the top of page two: {r:?}");
    let Placed::Lines { para, .. } = l.pages[1].items.iter().find(|i| matches!(i, Placed::Lines { .. })).unwrap() else { panic!() };
    assert!(para.lines[0].left >= 144.0, "text wraps beside it: {}", para.lines[0].left);
}

#[test]
fn tight_wrap_follows_a_diagrams_polygon_and_a_pictures_frame() {
    use kition_doc_doc::graphic::{Graphic, GraphicKind};
    use kition_doc_doc::para::{Anchor, Float, Wrap, WrapPolygon};
    // A 216 x 108 pt object whose wrap polygon is its top-left tenth. Beside a diagram, the lines
    // past the polygon run the column's full width. Older WPS references kept text clear of a
    // picture's or shape's whole frame; today's WPS for Mac follows their polygon too (synthetic
    // references, maintainer-approved change).
    let f = Float {
        wrap: Wrap::Tight,
        h_rel: Anchor::Column,
        v_rel: Anchor::Paragraph,
        wrap_polygon: Some(WrapPolygon::rect(0.0, 0.0, 0.1, 0.1)),
        ..Default::default()
    };
    let diagram = InlineObject::Graphic {
        w: 216.0,
        h: 108.0,
        alt: String::new(),
        float: f,
        graphic: Arc::new(Graphic { kind: GraphicKind::Diagram, ..Default::default() }),
    };
    for object in [diagram, rect_shape(216.0, 108.0, f)] {
        let mut d = Document::from_text(&"Text that wraps around the object's polygon. ".repeat(30));
        d.insert_object(&Pos::body(0, 0), object, &Default::default()).unwrap();
        let l = lay(&d);
        let Placed::Lines { para, .. } = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })).unwrap() else { panic!() };
        assert!(para.lines[0].left > 21.5, "the first line keeps clear of the polygon: {}", para.lines[0].left);
        let beside: Vec<f32> = para.lines.iter().filter(|line| line.top >= 20.0 && line.top + line.height <= 108.0).map(|line| line.left).collect();
        assert!(!beside.is_empty(), "lines beside the frame");
        assert!(beside.iter().all(|left| *left < 1.0), "beside the frame past its polygon, lines are full width: {beside:?}");
    }
}

#[test]
fn a_tight_wrap_follows_its_polygon_where_it_meets_a_lines_text() {
    use kition_doc_doc::para::{Anchor, Float, Wrap, WrapPolygon};
    // WPS for Mac (synthetic references with luan's picture): each line keeps clear of the
    // polygon's span over the line's text, its distance from text added; a line whose rows but
    // not text reach the polygon runs past it. A triangle widening downwards: lines lower down
    // start further right.
    let triangle = WrapPolygon::new(&[(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (0.0, 0.0)]).unwrap();
    let mut d = Document::from_text(&"Text that wraps around the picture's outline. ".repeat(30));
    d.styles.default_chr.size = Some(10.5);
    d.last_section.grid = Some(kition_doc_doc::section::DocumentGrid { kind: Some("lines".into()), line_pitch: Some(312), char_space: None });
    let float = Float {
        wrap: Wrap::Tight,
        h_rel: Anchor::Column,
        v_rel: Anchor::Paragraph,
        y: 15.6,
        dist: 9.0,
        wrap_polygon: Some(triangle),
        ..Default::default()
    };
    d.insert_object(&Pos::body(0, 0), rect_shape(200.0, 100.0, float), &Default::default()).unwrap();
    let props = &mut d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props;
    props.space_before = Some(0.0);
    props.line_spacing = Some(kition_doc_doc::props::LineSpacing::Multiple(1.0));
    let l = lay(&d);
    let Placed::Lines { para, .. } = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })).unwrap() else { panic!() };
    let lefts: Vec<f32> = para.lines.iter().map(|line| line.left).collect();
    assert!(lefts[0] < 1.0, "the line above the picture is full width: {lefts:?}");
    // The second line's text (rows 15.6 to 31.2) meets the triangle down to about 13.5 pt below its
    // top, where it is 27 pt wide.
    assert!(lefts[1] > 20.0 && lefts[1] < 60.0, "beside the triangle's narrow top: {lefts:?}");
    assert!(lefts[2] > lefts[1] + 20.0 && lefts[3] > lefts[2] + 20.0, "further right lower down: {lefts:?}");
}

#[test]
fn wrap_areas_take_effects_but_outlines_only_their_side_distance() {
    use kition_doc_doc::para::{Float, Wrap, WrapPolygon};
    // WPS for Mac (luan's text boxes): a top-and-bottom box keeps text clear of its effects
    // (`wp:effectExtent`) too; a tight outline keeps its distance from text at the sides only.
    let r = Rect::new(100.0, 100.0, 50.0, 40.0);
    let below = Float { wrap: Wrap::TopAndBottom, dist_top: 3.6, dist_bottom: 3.6, effect: [0.0, 0.0, 2.25, 2.25], ..Default::default() };
    let (area, _) = wrap_areas(r, &below)[0];
    assert!((area.bottom() - (140.0 + 3.6 + 2.25)).abs() < 0.01 && (area.right() - 152.25).abs() < 0.01, "{area:?}");
    let tight = Float {
        wrap: Wrap::Tight,
        dist: 9.0,
        dist_top: 3.6,
        dist_bottom: 3.6,
        wrap_polygon: Some(WrapPolygon::rect(0.0, 0.0, 1.0, 0.5)),
        ..Default::default()
    };
    let areas = wrap_areas(r, &tight);
    let (top, bottom) = areas.iter().fold((f32::MAX, f32::MIN), |(a, b), (r, _)| (a.min(r.y), b.max(r.bottom())));
    assert!((top - 100.0).abs() < 0.01 && (bottom - 120.0).abs() < 0.01, "{top} {bottom}");
    assert!(areas.iter().all(|(r, _)| (r.x - 91.0).abs() < 0.01 && (r.right() - 159.0).abs() < 0.01), "{areas:?}");
}

#[test]
fn a_relative_width_sizes_a_floating_box() {
    use kition_doc_doc::para::{Anchor, Float, RelSize, Wrap};
    // WPS for Mac (luan's quote text boxes, `wp14:pctWidth` 40% of the margins): the box is 40%
    // of the text width wide, whatever its stored width, its text wrapped in it.
    let mut d = Document::from_text("Anchor");
    let rel = Some(RelSize { from: Anchor::Margin, share: 0.4 });
    let float = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, rel_width: rel, ..Default::default() };
    let id = text_box_at(&mut d, &Pos::body(0, 0), "Box", 300.0, 60.0, float);
    let r = box_rect(&lay(&d), id);
    assert!((r.w - 0.4 * d.last_section.text_width()).abs() < 0.01, "{r:?}");
}

#[test]
fn objects_that_may_not_overlap_stand_one_below_the_other() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // Word and WPS (allowOverlap="0"): an object that would overlap another that may not be
    // overlapped moves down below it; one that allows overlap stays where its offset puts it.
    let rects = |apart: bool| {
        let mut d = Document::from_text("First\nSecond");
        let float = Float { wrap: Wrap::InFrontOfText, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, keep_apart: apart, ..Default::default() };
        let a = text_box_at(&mut d, &Pos::body(0, 0), "A", 100.0, 60.0, float);
        let b = text_box_at(&mut d, &Pos::body(1, 0), "B", 100.0, 60.0, Float { y: -20.0, ..float });
        let l = lay(&d);
        (box_rect(&l, a), box_rect(&l, b))
    };
    let (a, b) = rects(true);
    assert!(b.y >= a.bottom() - 0.01, "kept apart: {a:?} {b:?}");
    let (a, b) = rects(false);
    assert!(b.y < a.bottom() - 1.0, "allowed to overlap: {a:?} {b:?}");
}

#[test]
fn a_character_grid_gives_chinese_characters_whole_cells() {
    // WPS for Mac (luan's 出师表, `linesAndChars` with charSpace 13735): each Chinese character
    // takes a cell of the normal size plus charSpace/4096 pt (13.85 pt at 10.5 pt); Latin text
    // keeps its own widths.
    let mut d = Document::from_text("汉字Ab");
    d.styles.default_chr.size = Some(10.5);
    d.last_section.grid =
        Some(kition_doc_doc::section::DocumentGrid { kind: Some("linesAndChars".into()), line_pitch: Some(407), char_space: Some(13735) });
    let l = lay(&d);
    let Placed::Lines { para, .. } = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })).unwrap() else { panic!() };
    let pitch = 10.5 + 13735.0 / 4096.0;
    assert!((para.clusters[0].adv - pitch).abs() < 0.01 && (para.clusters[1].adv - pitch).abs() < 0.01, "{:?}", para.clusters);
    assert!(para.clusters[2].adv < 10.0, "Latin keeps its width: {:?}", para.clusters[2]);
}

#[test]
fn a_word_too_wide_for_the_room_beside_an_object_moves_down_whole() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // WPS for Mac (synthetic references, a box leaving 31 pt beside it): a word that doesn't fit
    // there isn't broken; its line moves down a row at a time until it fits, below the box.
    let mut d = Document::from_text("Top\nANCHORWORD and more text after it");
    d.styles.default_chr.size = Some(10.5);
    d.last_section.grid = Some(kition_doc_doc::section::DocumentGrid { kind: Some("lines".into()), line_pitch: Some(312), char_space: None });
    let width = d.last_section.text_width();
    let float = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, x: 40.0, ..Default::default() };
    let end = d.para(StoryRef::Body, &Path::top(1)).unwrap().text.len();
    let id = text_box_at(&mut d, &Pos::body(1, end), "Box", width - 40.0, 50.0, float);
    let l = lay(&d);
    let rect = box_rect(&l, id);
    let Some(Placed::Lines { para, y, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { path, .. } if path.0 == [1])) else {
        panic!()
    };
    let first = &para.lines[0];
    assert!(first.stop - first.start >= "ANCHORWORD".len(), "the word stays whole: {first:?}");
    assert!(y + first.top - para.lines[0].top + first.top >= rect.bottom() - 0.01, "below the box: {y} {first:?} {rect:?}");
}

#[test]
fn a_tall_gridded_line_at_a_multiple_takes_the_multiple_of_the_pitch() {
    // A 15.6 pt grid and a 16 pt heading spaced at 2.4 lines (`w:line="578"`): its text is
    // taller than one row, and WPS lays the line out 2.4 rows tall (37.6 pt), not rounded up to
    // three whole rows.
    let mut d = Document::from_text("Heading");
    d.styles.default_chr.size = Some(16.0);
    d.last_section.grid = Some(kition_doc_doc::section::DocumentGrid { kind: Some("lines".into()), line_pitch: Some(312), char_space: None });
    let props = &mut d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props;
    props.line_spacing = Some(kition_doc_doc::props::LineSpacing::Multiple(578.0 / 240.0));
    let l = lay(&d);
    let Placed::Lines { para, .. } = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })).unwrap() else { panic!() };
    let want = 578.0 / 240.0 * 15.6;
    assert!((para.lines[0].height - want).abs() < 0.05, "{} rather than {want}", para.lines[0].height);
}

#[test]
fn a_gridded_line_wraps_only_where_its_text_meets_an_object() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // A 15.6 pt grid: a 15 pt line takes two rows (31.2 pt), its text centred in them. An
    // object starting 29.65 pt below the paragraph's top overlaps the line's rows but not its
    // text. WPS for Mac wraps it beside the object already, as the line's rows meet it
    // (synthetic references).
    let mut d = Document::from_text(&"Text beside the picture. ".repeat(40));
    d.styles.default_chr.size = Some(15.0);
    d.last_section.grid = Some(kition_doc_doc::section::DocumentGrid { kind: Some("lines".into()), line_pitch: Some(312), char_space: None });
    let f = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, y: 29.65, ..Default::default() };
    d.insert_object(&Pos::body(0, 0), rect_shape(200.0, 100.0, f), &Default::default()).unwrap();
    let props = &mut d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props;
    props.space_before = Some(0.0);
    props.space_after = Some(0.0);
    let l = lay(&d);
    let Placed::Lines { para, .. } = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })).unwrap() else { panic!() };
    assert!((para.lines[0].height - 31.2).abs() < 0.01, "two rows: {}", para.lines[0].height);
    assert!(para.lines[0].left >= 200.0, "the first line wraps beside the object: {}", para.lines[0].left);
    assert!(para.lines[1].left >= 200.0, "the second wraps beside the object: {}", para.lines[1].left);
}

#[test]
fn floats_in_headers_and_table_cells_are_drawn() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    let mut d = Document::from_text("Body text.");
    let logo = Float { wrap: Wrap::BehindText, h_rel: Anchor::Page, v_rel: Anchor::Page, x: 400.0, y: 20.0, ..Default::default() };
    let mut hp = kition_doc_doc::Paragraph::with_text("Header", Default::default());
    hp.insert_object(0, rect_shape(120.0, 60.0, logo), &Default::default()).unwrap();
    let id = d.add_part(kition_doc_doc::PartKind::Header, vec![kition_doc_doc::para_block(hp)]);
    d.last_section.headers.default = Some(id);
    let mut t = Table::new(1, 2, 468.0);
    let mut cp = kition_doc_doc::Paragraph::with_text("cell", Default::default());
    let in_cell = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, x: 5.0, ..Default::default() };
    cp.insert_object(0, rect_shape(40.0, 20.0, in_cell), &Default::default()).unwrap();
    t.rows[0].cells[0].blocks = vec![kition_doc_doc::para_block(cp)];
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
    let l = lay(&d);
    let h = shapes(&l.pages[0].header);
    assert_eq!(h.len(), 1, "header float drawn");
    assert!((h[0].x - 400.0).abs() < 0.01 && (h[0].y - 20.0).abs() < 0.01, "page coordinates: {:?}", h[0]);
    assert!(matches!(l.pages[0].header.first(), Some(Placed::Shape { .. })), "behind the header text");
    let c = shapes(&l.pages[0].items);
    assert_eq!(c.len(), 1, "cell float drawn");
    assert!(c[0].x > 72.0 && c[0].x < 72.0 + 234.0, "inside the first cell: {:?}", c[0]);
}

#[test]
fn wrap_area_uses_each_distance() {
    use kition_doc_doc::para::{Float, Wrap};
    let f = Float { wrap: Wrap::Square, dist: 9.0, dist_top: 0.0, dist_bottom: 4.0, ..Default::default() };
    let (r, tb) = wrap_areas(Rect::new(10.0, 20.0, 100.0, 50.0), &f)[0];
    assert_eq!((r.x, r.y, r.w, r.h, tb), (1.0, 20.0, 118.0, 54.0, para::Clear::Beside));
    let hostile = Float { wrap: Wrap::TopAndBottom, dist: f32::NAN, dist_top: -5.0, dist_bottom: f32::INFINITY, ..Default::default() };
    let (r, tb) = wrap_areas(Rect::new(0.0, 0.0, 10.0, 10.0), &hostile)[0];
    assert!(r.w.is_finite() && r.h.is_finite() && tb == para::Clear::Below, "{r:?}");
    assert!(wrap_areas(Rect::new(0.0, 0.0, 10.0, 10.0), &Float { wrap: Wrap::BehindText, ..Default::default() }).is_empty());
}

#[test]
fn float_moved_to_a_new_page_sits_at_its_paragraph_top() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    let mut d = Document::from_text(&format!("First page.\n{}", "Text beside the picture on page two. ".repeat(20)));
    let f = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, dist: 9.0, ..Default::default() };
    d.insert_object(&Pos::body(1, 0), rect_shape(144.0, 100.0, f), &Default::default()).unwrap();
    if let Some(kition_doc_doc::Block::Para(p)) = d.body.get_mut(1).map(std::sync::Arc::make_mut) {
        p.props.page_break_before = Some(true);
        p.props.space_before = Some(24.0);
    }
    let l = lay(&d);
    let r = shapes(&l.pages[1].items)[0];
    let top = first_line_top(&l.pages[1].items);
    // WPS for Mac (synthetic references): the picture sits at the paragraph's top, above its space
    // before, and the text starts below that space.
    assert!((r.y - d.last_section.margin_top).abs() < 0.5, "picture at {} on a page starting at {}", r.y, d.last_section.margin_top);
    assert!((top - r.y - 24.0).abs() < 0.5, "picture at {} but its paragraph's text starts at {top}", r.y);
}

#[test]
fn page_relative_header_float_wraps_where_it_is_drawn() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    let body_top = |wrap| {
        let mut d = Document::from_text("Body text.");
        let logo = Float { wrap, h_rel: Anchor::Page, v_rel: Anchor::Page, x: 72.0, y: 0.0, ..Default::default() };
        let mut hp = kition_doc_doc::Paragraph::with_text("Header", Default::default());
        hp.insert_object(0, rect_shape(400.0, 20.0, logo), &Default::default()).unwrap();
        let id = d.add_part(kition_doc_doc::PartKind::Header, vec![kition_doc_doc::para_block(hp)]);
        d.last_section.headers.default = Some(id);
        first_line_top(&lay(&d).pages[0].items)
    };
    // The logo sits at the top edge of the page, above the header text, so wrapping text around
    // it must not make the header taller (and push the body down) compared with no wrapping.
    let (wrapped, in_front) = (body_top(Wrap::TopAndBottom), body_top(Wrap::InFrontOfText));
    assert!((wrapped - in_front).abs() < 0.5, "body starts at {wrapped} with wrapping, {in_front} without");
}

/// A tall text box floating behind the text in the header (placed relative to the
/// page, no wrapping) neither makes the header taller nor pushes the body down: the body starts
/// at the top margin and paginates exactly as without the box.
#[test]
fn floating_header_text_box_leaves_the_body_at_the_top_margin() {
    use kition_doc_doc::para::{Anchor, Float, ShapeKind, Wrap};
    let body: Vec<String> = (1..=40).map(|i| format!("Body paragraph {i}: synthetic public test content.")).collect();
    let make = |float: Option<Float>| {
        let mut d = Document::from_text(&body.join("\n"));
        // The header starts 18pt from the top, so its two lines end well above the 72pt top
        // margin whatever font the default text gets (the line height differs between the
        // installed fonts and the bundled fallback: a header at 36pt overflows the margin there).
        d.last_section.header = 18.0;
        // A label, then the paragraph the box is anchored to (empty in the control).
        let tight = ParaProps { space_before: Some(0.0), space_after: Some(0.0), ..Default::default() };
        let mut label = kition_doc_doc::Paragraph::with_text("Header label", Default::default());
        label.props = tight.clone();
        let mut anchor = kition_doc_doc::Paragraph { props: tight, ..Default::default() };
        if let Some(float) = float {
            let text = kition_doc_doc::Paragraph::with_text("WATERMARK", Default::default());
            let story = d.add_part(kition_doc_doc::PartKind::TextBox, vec![kition_doc_doc::para_block(text)]);
            let tb = InlineObject::Shape {
                vml: None,
                kind: ShapeKind::TextBox,
                w: 405.0,
                h: 491.4,
                fill: None,
                stroke: None,
                stroke_width: 0.75,
                float,
                story: Some(story),
                freeform: None,
                effects: Default::default(),
                geom: None,
                uid: 0,
                gradient: None,
                connection: None,
                warp: None,
                body: None,
                dash: None,
            };
            anchor.insert_object(0, tb, &Default::default()).unwrap();
        }
        let id = d.add_part(kition_doc_doc::PartKind::Header, vec![kition_doc_doc::para_block(label), kition_doc_doc::para_block(anchor)]);
        d.last_section.headers.default = Some(id);
        let l = lay(&d);
        (l.pages.len(), first_line_top(&l.pages[0].items))
    };
    let (control_pages, control_top) = make(None);
    let behind = Float { wrap: Wrap::BehindText, h_rel: Anchor::Page, v_rel: Anchor::Page, x: 0.0, y: 80.0, ..Default::default() };
    let (pages, top) = make(Some(behind));
    assert!(control_pages > 1, "the control needs more than one page: {control_pages}");
    assert!((control_top - 72.0).abs() < 0.5, "control body starts at the top margin: {control_top}");
    assert!((top - control_top).abs() < 0.5, "body starts at {top} with the floating box, {control_top} without");
    assert_eq!(pages, control_pages);
    // The same box inline does take room in the header, which is what the reader used to make of it.
    let (inline_pages, inline_top) = make(Some(Float::default()));
    assert!(inline_top > 400.0 && inline_pages > control_pages, "inline box: body at {inline_top}, {inline_pages} pages");
}

/// List labels drawn, in page order (each paragraph's first line).
fn labels(l: &DocLayout) -> Vec<String> {
    l.pages
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| if let Placed::Lines { para, l0: 0, .. } = i { para.label.as_ref().map(|lb| lb.text.clone()) } else { None })
        .collect()
}

fn numbered(d: &mut Document, texts: &[&str], props: ParaProps) -> Vec<kition_doc_doc::Paragraph> {
    let num = d.numbering.add_list(kition_doc_doc::ListKind::Numbered);
    texts
        .iter()
        .map(|t| {
            let mut p = kition_doc_doc::Paragraph::with_text(t, Default::default());
            p.props = ParaProps { numbering: Some(kition_doc_doc::props::NumRef { num, level: 0 }), ..props.clone() };
            p
        })
        .collect()
}

#[test]
fn list_items_are_counted_once_when_laid_out_again() {
    // Contextual spacing drops the space before, so each item's top differs from the first guess.
    let mut d = Document::new();
    let props = ParaProps { space_before: Some(6.0), contextual_spacing: Some(true), ..Default::default() };
    let mut items = numbered(&mut d, &["one", "two", "three"], props);
    // A page break before moves the last item after its first layout.
    items[2].props.page_break_before = Some(true);
    d.body = items.into_iter().map(kition_doc_doc::para_block).collect();
    let l = lay(&d);
    assert_eq!(l.pages.len(), 2);
    assert_eq!(labels(&l), ["1.", "2.", "3."]);
}

#[test]
fn list_items_beside_a_float_in_a_cell_are_counted_once() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    let mut d = Document::from_text("Body.");
    let mut items = numbered(&mut d, &["first item", "second item", "third item"], ParaProps::default());
    // The float makes the cell's paragraphs wrap, so they are laid out a second time.
    let f = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, ..Default::default() };
    items[0].insert_object(0, rect_shape(40.0, 60.0, f), &Default::default()).unwrap();
    let mut t = Table::new(1, 1, 468.0);
    t.rows[0].cells[0].blocks = items.into_iter().map(kition_doc_doc::para_block).collect();
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
    assert_eq!(labels(&lay(&d)), ["1.", "2.", "3."]);
}

#[test]
fn a_tables_first_cell_text_lines_up_with_the_margin_in_every_mode() {
    use kition_doc_doc::props::{Border, BorderStyle, Borders};
    let cell_text_x = |mode: u32| {
        let mut d = Document::from_text("");
        d.settings.compat_mode = mode;
        let line = Some(Border { style: BorderStyle::Single, width: 0.5, color: None, space: 0.0, theme: None });
        let mut t = Table::new(1, 1, 200.0);
        t.props.borders = Some(Borders { top: line, left: line, bottom: line, right: line, between: line, inside_v: line });
        t.rows[0].cells[0].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("cell", Default::default()))];
        d.body = vec![std::sync::Arc::new(kition_doc_doc::Block::Table(t))];
        let l = lay(&d);
        l.pages[0].items.iter().find_map(|i| if let Placed::Lines { x, .. } = i { Some(*x) } else { None }).unwrap()
    };
    // WPS for Mac (synthetic references in modes 12, 14 and 15): the text at the margin, the border
    // a cell margin outside it, whatever the compatibility mode (Word 2013+ would put the border
    // at the margin in mode 15; maintainer decision: follow WPS).
    assert!((cell_text_x(15) - 72.0).abs() < 0.01, "{}", cell_text_x(15));
    assert!((cell_text_x(12) - 72.0).abs() < 0.01, "{}", cell_text_x(12));
}

#[test]
fn floating_tables_take_no_room_and_text_wraps_beside_them() {
    use kition_doc_doc::para::Anchor;
    use kition_doc_doc::props::TableFloat;
    let mut d = Document::from_text("Body text beside the narrow table.");
    let cell = |t: &mut Table, text: &str| {
        t.rows[0].cells[0].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(text, Default::default()))]
    };
    let mut wide = Table::new(1, 1, 468.0);
    cell(&mut wide, "Wide");
    wide.props.float = Some(TableFloat { h_rel: Anchor::Margin, v_rel: Anchor::Paragraph, overlap: false, ..Default::default() });
    let mut narrow = Table::new(1, 1, 200.0);
    cell(&mut narrow, "Narrow");
    narrow.props.float =
        Some(TableFloat { h_rel: Anchor::Column, v_rel: Anchor::Paragraph, dist: [9.0, 0.0, 9.0, 0.0], overlap: false, ..Default::default() });
    d.insert_block(StoryRef::Body, &Path::top(0), kition_doc_doc::Block::Table(wide)).unwrap();
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(narrow)).unwrap();
    let l = lay(&d);
    let lines: Vec<(f32, f32, f32)> = l.pages[0]
        .items
        .iter()
        .filter_map(|i| if let Placed::Lines { x, y, para, .. } = i { Some((*x, *y, para.lines[0].left)) } else { None })
        .collect();
    let [(wx, wy, _), (nx, ny, _), (bx, by, bleft)] = lines.as_slice() else { panic!("{lines:?}") };
    // The wide table stands where the text is; the narrow one may not overlap it, so it goes
    // right below, keeping its 9pt from the text's left edge.
    assert!(*wy >= 72.0 && *wy < 80.0, "wide at {wy}");
    assert!(*ny > *wy + 10.0, "narrow at {ny}, below the wide table at {wy}");
    assert!((nx - wx - 9.0).abs() < 0.01, "narrow text at {nx}, wide text at {wx}");
    // The paragraph doesn't wait below them: it runs beside the narrow table, level with it.
    assert!((by - ny).abs() < 2.0, "body at {by}, narrow table text at {ny}");
    assert!(bx + bleft > nx + 200.0, "body text at {}, right of the narrow table", bx + bleft);
}

#[test]
fn a_floating_table_taller_than_a_page_runs_across_pages() {
    use kition_doc_doc::para::Anchor;
    use kition_doc_doc::props::TableFloat;
    let mut d = Document::from_text("After the table.");
    let mut t = Table::new(80, 1, 300.0);
    for (i, r) in t.rows.iter_mut().enumerate() {
        r.cells[0].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(&format!("Row {i}"), Default::default()))];
    }
    t.props.float = Some(TableFloat { h_rel: Anchor::Margin, v_rel: Anchor::Paragraph, x: -30.0, overlap: false, ..Default::default() });
    d.insert_block(StoryRef::Body, &Path::top(0), kition_doc_doc::Block::Table(t)).unwrap();
    let l = lay(&d);
    assert!(l.pages.len() >= 2, "{} page(s)", l.pages.len());
    // Every row is drawn on a page, none past a page's bottom margin, and all from the table's
    // own left edge.
    let mut rows = 0;
    for p in &l.pages {
        for it in &p.items {
            if let Placed::Lines { x, y, story: StoryRef::Body, path, .. } = it
                && path.0.len() > 1
            {
                rows += 1;
                assert!(*y < 792.0 - 72.0, "row drawn at {y}, below the bottom margin");
                assert!(*x < 72.0, "row text at {x}, not from the table's edge 30pt left of the margin");
            }
        }
    }
    assert_eq!(rows, 80);
}

#[test]
fn no_line_break_right_after_a_slash() {
    // Word keeps "and/or" and web addresses whole on a line when they fit.
    let text = "word and/or https://example.org/keepers/log/winter ".repeat(40);
    let d = Document::from_text(&text);
    let l = lay(&d);
    let mut lines = 0;
    for p in &l.pages {
        for it in &p.items {
            if let Placed::Lines { para, l0, l1, .. } = it {
                for line in &para.lines[*l0..*l1] {
                    lines += 1;
                    assert!(!text[..line.start].ends_with('/'), "line starts after a slash: {:?}", &text[line.start..line.stop]);
                }
            }
        }
    }
    assert!(lines > 10, "{lines} lines");
}

#[test]
fn no_line_break_inside_a_word_after_a_question_mark_exclamation_or_hyphen() {
    // WPS for Mac keeps "a?b", "a!b" and "a-b" whole on a line when they fit (synthetic
    // calibration of its exports), as a web address's query does.
    let text = "word list.aspx?sid=75 hello!world well-known ".repeat(40);
    let d = Document::from_text(&text);
    let l = lay(&d);
    let mut lines = 0;
    for p in &l.pages {
        for it in &p.items {
            if let Placed::Lines { para, l0, l1, .. } = it {
                for line in &para.lines[*l0..*l1] {
                    lines += 1;
                    let before = &text[..line.start];
                    assert!(!before.ends_with(['?', '!', '-']), "line starts inside a word: {:?}", &text[line.start..line.stop]);
                }
            }
        }
    }
    assert!(lines > 10, "{lines} lines");
}

#[test]
fn balanced_widths_make_a_space_beside_chinese_half_an_em() {
    // WPS for Mac with w:balanceSingleByteDoubleByteWidth: a space next to a Chinese character is
    // half an em wide (its own font still sets the line's height); between Latin letters, and
    // without the option, it keeps its font's width.
    let space_advances = |balanced: bool| {
        let mut d = Document::from_text("测试 abc def 测试");
        if balanced {
            d.settings.compat_flags.push("balanceSingleByteDoubleByteWidth".into());
        }
        let l = lay(&d);
        let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
        let size = para.styles[para.clusters[0].style as usize].size;
        let spaces: Vec<f32> = para.clusters.iter().filter(|c| c.kind == para::ClKind::Space).map(|c| c.adv / size).collect();
        spaces
    };
    let plain = space_advances(false);
    let balanced = space_advances(true);
    assert_eq!(plain.len(), 3);
    assert!(plain.iter().all(|w| *w < 0.45), "{plain:?}");
    assert!((balanced[0] - 0.5).abs() < 1e-4 && (balanced[2] - 0.5).abs() < 1e-4, "{balanced:?}");
    assert_eq!(balanced[1], plain[1], "a space between Latin words keeps its width");
}

#[test]
fn a_character_border_stands_above_and_below_its_text() {
    // WPS for Mac: a run's border width adds above the text and below it, so a 1 pt border makes a
    // 10.5 pt Song line two 15.6 pt grid rows; its distance from the text doesn't count.
    let line = |border: Option<kition_doc_doc::props::Border>| {
        let mut d = Document::from_text("Boxed text");
        let p = d.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
        p.runs = vec![kition_doc_doc::Run { len: p.text.len(), props: kition_doc_doc::CharProps { border, ..Default::default() } }];
        p.props.line_spacing = Some(kition_doc_doc::props::LineSpacing::Multiple(1.0));
        let l = lay(&d);
        let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
        (para.lines[0].height, para.lines[0].baseline - para.lines[0].top)
    };
    let rule = |width: f32, space: f32| {
        Some(kition_doc_doc::props::Border { style: kition_doc_doc::props::BorderStyle::Single, width, color: None, space, theme: None })
    };
    let plain = line(None);
    let bordered = line(rule(1.5, 0.0));
    assert!((bordered.0 - plain.0 - 3.0).abs() < 0.01 && (bordered.1 - plain.1 - 1.5).abs() < 0.01, "{plain:?} {bordered:?}");
    assert_eq!(line(rule(1.5, 4.0)), bordered, "the border's distance from the text doesn't count");
}

#[test]
fn unequal_formats_lay_out_in_linear_time() {
    // Runs whose formats never compare equal (NaN spacing from a hostile file) each get a style;
    // finding a run's style must not compare it against every earlier one.
    let doc = |n: usize| {
        let mut p = kition_doc_doc::Paragraph::with_text("", Default::default());
        p.text = "ab ".repeat(n);
        p.runs = (0..n)
            .map(|i| kition_doc_doc::Run {
                len: 3,
                props: kition_doc_doc::CharProps { spacing: Some(f32::NAN), bold: Some(i % 2 == 0), ..Default::default() },
            })
            .collect();
        let mut d = Document::from_text("");
        d.body = vec![kition_doc_doc::para_block(p)];
        d
    };
    let (t1, tk, ratio) = layout_time_ratio(doc, 5_000, &LayoutOptions::default());
    eprintln!("t(n) {t1:.4} s, t(8n) {tk:.4} s, ratio {ratio:.1}");
    assert!(ratio < 24.0, "8x the runs took {ratio:.1}x the time ({t1:.4} s → {tk:.4} s): not linear");
}

#[test]
fn hidden_float_in_a_table_cell_takes_no_room() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // A square-wrapped shape anchored in hidden text inside a table cell, beside wrapping text.
    let mut d = Document::from_text("Body text.");
    let mut t = Table::new(1, 2, 468.0);
    let mut cp = kition_doc_doc::Paragraph::with_text(&"Cell words wrap around it. ".repeat(8), Default::default());
    let f = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, dist: 9.0, ..Default::default() };
    let hidden = kition_doc_doc::CharProps { hidden: Some(true), ..Default::default() };
    cp.insert_object(0, rect_shape(60.0, 40.0, f), &hidden).unwrap();
    t.rows[0].cells[0].blocks = vec![kition_doc_doc::para_block(cp)];
    d.insert_block(StoryRef::Body, &Path::top(1), kition_doc_doc::Block::Table(t)).unwrap();
    // (shapes placed, the cell text's widest first-line indent)
    let run = |show_hidden: bool| {
        let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions { show_hidden, ..Default::default() });
        let items = &l.pages[0].items;
        let left = items
            .iter()
            .filter_map(|i| if let Placed::Lines { para, .. } = i { para.lines.first().map(|l| l.left) } else { None })
            .fold(0.0f32, f32::max);
        (shapes(items).len(), left)
    };
    let (n, left) = run(true);
    assert!(n == 1 && left >= 60.0, "shown: placed and wrapped around ({n}, {left})");
    assert_eq!(run(false), (0, 0.0), "hidden: not placed, no wrap area");
}

#[test]
fn objects_left_out_through_the_table_style_formatting() {
    // The table style's character formatting sits under the runs' own, as the cell is laid out.
    let mut p = kition_doc_doc::Paragraph::with_text("ab", Default::default());
    p.insert_object(1, rect_shape(10.0, 10.0, Default::default()), &Default::default()).unwrap();
    let d = Document::from_text("");
    let hidden = kition_doc_doc::CharProps { hidden: Some(true), ..Default::default() };
    assert!(!left_out_objects(&d, &p, None, false, false)(0));
    assert!(left_out_objects(&d, &p, Some(&hidden), false, false)(0));
    assert!(!left_out_objects(&d, &p, Some(&hidden), true, false)(0), "hidden text shown");
}

#[test]
fn hidden_float_in_a_text_box_is_not_placed() {
    use kition_doc_doc::para::{Anchor, Float, Wrap};
    // A shape anchored in hidden text inside a text box: no drawing, no hit area.
    let mut d = Document::from_text("Body text.");
    let id = text_box(&mut d, 0, "Box words.", 200.0, 120.0, Default::default());
    let f = Float { wrap: Wrap::Square, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, ..Default::default() };
    let hidden = kition_doc_doc::CharProps { hidden: Some(true), ..Default::default() };
    let at = Pos { story: StoryRef::Part(id), path: Path::top(0), off: 0 };
    d.insert_object(&at, rect_shape(30.0, 20.0, f), &hidden).unwrap();
    let count = |show_hidden: bool| {
        let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions { show_hidden, ..Default::default() });
        let items = &l.pages[0].items;
        let areas = items.iter().filter(|i| matches!(i, Placed::Object { story: StoryRef::Part(_), .. })).count();
        (shapes(items).len(), areas)
    };
    let (shown, shown_areas) = count(true);
    let (hidden_n, hidden_areas) = count(false);
    assert_eq!((shown - hidden_n, shown_areas - hidden_areas), (1, 1), "shown {shown}/{shown_areas}, hidden {hidden_n}/{hidden_areas}");
}

#[test]
fn inline_graphic_reserves_its_height() {
    let mut d = Document::from_text("Chart\nAfter");
    let obj = InlineObject::Graphic { w: 216.0, h: 144.0, alt: String::new(), float: Default::default(), graphic: Arc::new(Default::default()) };
    d.insert_object(&Pos::body(0, 0), obj, &Default::default()).unwrap();
    let l = lay(&d);
    let tops: Vec<f32> = l.pages[0].items.iter().filter_map(|i| if let Placed::Lines { y, .. } = i { Some(*y) } else { None }).collect();
    assert_eq!(tops.len(), 2);
    assert!(tops[1] - tops[0] >= 144.0, "the next paragraph starts below the chart: {tops:?}");
}

#[test]
fn graphic_items_draw_inside_the_object() {
    use kition_doc_doc::graphic::{Graphic, GraphicItem, GraphicKind, PathSeg, TextAlign};
    use kition_doc_doc::props::Rgb;
    let items = vec![
        GraphicItem::Shape {
            rect: [10.0, 5.0, 20.0, 20.0],
            kind: kition_doc_doc::para::ShapeKind::Rectangle,
            fill: Some(Rgb::BLACK),
            stroke: None,
            stroke_width: 0.0,
        },
        GraphicItem::Path {
            segs: vec![PathSeg::Move(1.0, 2.0), PathSeg::Line(30.0, 40.0), PathSeg::Close],
            fill: None,
            stroke: Some(Rgb::BLACK),
            stroke_width: 0.0,
        },
        GraphicItem::Text {
            rect: [0.0, 100.0, 216.0, 20.0],
            text: "Q1".into(),
            size: 10.0,
            color: Rgb::BLACK,
            bold: false,
            align: TextAlign::Center,
            font: None,
            prompt: false,
        },
    ];
    let mut d = Document::from_text("Chart");
    let graphic = Graphic { kind: GraphicKind::Chart, items, ..Default::default() };
    let obj = InlineObject::Graphic { w: 216.0, h: 144.0, alt: String::new(), float: Default::default(), graphic: Arc::new(graphic) };
    d.insert_object(&Pos::body(0, 0), obj, &Default::default()).unwrap();
    let l = lay(&d);
    // The chart's draws sit inside its figure.
    let draws = flat(display::page_display(&d, &l.pages[0], &Default::default()));
    let shape = draws.iter().find_map(|x| if let display::Draw::Shape { rect, .. } = x { Some(*rect) } else { None }).expect("a shape");
    let first = draws.iter().find_map(|x| if let display::Draw::Path { segs, .. } = x { segs.first().copied() } else { None }).expect("a path");
    let PathSeg::Move(mx, my) = first else { panic!("{first:?}") };
    // Path points are page coordinates, moved with the object like the shape's rectangle.
    assert_eq!((mx - shape.x, my - shape.y), (1.0 - 10.0, 2.0 - 5.0));
    assert!(draws.iter().any(|x| matches!(x, display::Draw::Glyphs { text, .. } if text == "Q1")));
}

#[test]
fn graphic_prompts_are_drawn_as_outlines_not_text() {
    use kition_doc_doc::graphic::{Graphic, GraphicItem, GraphicKind, TextAlign};
    use kition_doc_doc::props::Rgb;
    let text = |text: &str, prompt: bool| GraphicItem::Text {
        rect: [0.0, 10.0, 216.0, 20.0],
        text: text.into(),
        size: 10.0,
        color: Rgb::BLACK,
        bold: false,
        align: TextAlign::Center,
        font: None,
        prompt,
    };
    let items = vec![text("Node", false), text("[Text]", true)];
    let mut d = Document::from_text("Diagram");
    let graphic = Graphic { kind: GraphicKind::Diagram, items, ..Default::default() };
    let obj = InlineObject::Graphic { w: 216.0, h: 144.0, alt: String::new(), float: Default::default(), graphic: Arc::new(graphic) };
    d.insert_object(&Pos::body(0, 0), obj, &Default::default()).unwrap();
    let l = lay(&d);
    let draws = flat(display::page_display(&d, &l.pages[0], &Default::default()));
    // The node's own text is text; the placeholder's prompt shows, but only as ink.
    assert!(draws.iter().any(|x| matches!(x, display::Draw::Glyphs { text, .. } if text == "Node")));
    assert!(!draws.iter().any(|x| matches!(x, display::Draw::Glyphs { text, .. } if text.contains("Text"))), "the prompt is not text");
    let ink = draws.iter().filter(|x| matches!(x, display::Draw::Path { fill: Some(_), segs, .. } if segs.len() > 8)).count();
    assert!(ink > 0, "the prompt's letters are outlines: {draws:?}");
}

#[test]
fn outlined_text_is_drawn_as_letters_not_text() {
    use kition_doc_doc::graphic::PathSeg;
    use kition_doc_doc::props::{Rgb, TextOutline, TextPaint};
    // A run with a visible text outline (red, 1 pt) filled green: drawn as outlined letters,
    // as WPS prints it, not as text.
    let mut d = Document::from_text("Plain Outlined");
    if let Some(Block::Para(p)) = d.body.get_mut(0).map(std::sync::Arc::make_mut) {
        let outline = TextOutline { stroke: TextPaint::Rgb(Rgb(255, 0, 0)), width: 1.0, fill: Some(Some(TextPaint::Rgb(Rgb(0, 255, 0)))) };
        p.format(6, 14, &|c| c.text_outline = Some(outline)).unwrap();
    }
    let l = lay(&d);
    let draws = flat(display::page_display(&d, &l.pages[0], &Default::default()));
    assert!(draws.iter().any(|x| matches!(x, display::Draw::Glyphs { text, .. } if text.contains("Plain"))));
    assert!(!draws.iter().any(|x| matches!(x, display::Draw::Glyphs { text, .. } if text.contains("Outlined"))), "not text: {draws:?}");
    let letters = draws.iter().find_map(|x| match x {
        display::Draw::Path { segs, fill, stroke, stroke_width, .. } if *stroke == Some(Rgb(255, 0, 0)) => Some((segs.len(), *fill, *stroke_width)),
        _ => None,
    });
    let (n, fill, width) = letters.unwrap_or_else(|| panic!("outlined letters: {draws:?}"));
    assert!(n > 20 && fill == Some(Rgb(0, 255, 0)) && (width - 1.0).abs() < 0.01, "{n} {fill:?} {width}");
    let _ = PathSeg::Close;
}

#[test]
fn text_with_a_blurred_effect_is_drawn_as_letters_not_text() {
    // A run with a blurred shadow, glow or reflection (w14 text effects): WPS prints its letters
    // as a picture, so they are ink, filled in the text's colour, not text.
    let mut d = Document::from_text("Plain Shadowed");
    if let Some(Block::Para(p)) = d.body.get_mut(0).map(std::sync::Arc::make_mut) {
        p.format(6, 14, &|c| c.text_blur = Some(true)).unwrap();
    }
    let l = lay(&d);
    let draws = flat(display::page_display(&d, &l.pages[0], &Default::default()));
    assert!(draws.iter().any(|x| matches!(x, display::Draw::Glyphs { text, .. } if text.contains("Plain"))));
    assert!(!draws.iter().any(|x| matches!(x, display::Draw::Glyphs { text, .. } if text.contains("Shadowed"))), "not text: {draws:?}");
    assert!(draws.iter().any(|x| matches!(x, display::Draw::Path { segs, fill: Some(_), .. } if segs.len() > 20)), "letters: {draws:?}");
}

#[test]
fn graphic_text_in_a_degenerate_rectangle_is_not_drawn() {
    use kition_doc_doc::graphic::{Graphic, GraphicItem, GraphicKind, TextAlign};
    use kition_doc_doc::props::Rgb;
    let text = |rect: [f32; 4], size: f32| GraphicItem::Text {
        rect,
        text: "Q1".into(),
        size,
        color: Rgb::BLACK,
        bold: false,
        align: TextAlign::Center,
        font: None,
        prompt: false,
    };
    let items = vec![text([f32::NAN, 0.0, 10.0, 10.0], 10.0), text([0.0, 0.0, f32::INFINITY, 10.0], 10.0), text([0.0, 0.0, 10.0, 10.0], f32::NAN)];
    let graphic = Graphic { kind: GraphicKind::Diagram, items, ..Default::default() };
    let obj = InlineObject::Graphic { w: 216.0, h: 144.0, alt: String::new(), float: Default::default(), graphic: Arc::new(graphic) };
    let mut d = Document::from_text("Chart");
    d.insert_object(&Pos::body(0, 0), obj, &Default::default()).unwrap();
    let draws = flat(display::page_display(&d, &lay(&d).pages[0], &Default::default()));
    assert!(!draws.iter().any(|x| matches!(x, display::Draw::Glyphs { text, .. } if text == "Q1")));
}

#[test]
fn group_with_a_blurred_effect_prints_its_text_as_letters() {
    use kition_doc_doc::effects::{Shadow, ShapeEffects};
    use kition_doc_doc::para::{GroupChild, ShapeKind};
    // WPS prints a group holding a blurred shadow, glow or soft edge as one picture: its text
    // boxes' letters are ink, not text. A sharp shadow (no blur) leaves the text as text.
    let group = |d: &mut Document, words: &str, blur: f32| {
        let id = d.add_part(
            kition_doc_doc::PartKind::TextBox,
            vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(words, Default::default()))],
        );
        let shape = |story: Option<u32>, effects: ShapeEffects| InlineObject::Shape {
            vml: None,
            kind: if story.is_some() { ShapeKind::TextBox } else { ShapeKind::Rectangle },
            w: 120.0,
            h: 40.0,
            fill: None,
            stroke: None,
            stroke_width: 0.0,
            float: Default::default(),
            story,
            freeform: None,
            effects,
            geom: None,
            uid: 0,
            gradient: None,
            connection: None,
            warp: None,
            body: None,
            dash: None,
        };
        let shadow = ShapeEffects { shadow: Some(Shadow { blur, ..Default::default() }), ..Default::default() };
        InlineObject::Group {
            uid: 0,
            w: 120.0,
            h: 80.0,
            float: Default::default(),
            ch_w: 120.0,
            ch_h: 80.0,
            children: vec![
                GroupChild { x: 0.0, y: 0.0, obj: shape(None, shadow) },
                GroupChild { x: 0.0, y: 40.0, obj: shape(Some(id), Default::default()) },
            ],
        }
    };
    let mut d = Document::from_text("Body");
    let blurred = group(&mut d, "Blurred", 4.0);
    d.insert_object(&Pos::body(0, 0), blurred, &Default::default()).unwrap();
    let sharp = group(&mut d, "Sharp", 0.0);
    d.insert_object(&Pos::body(0, 0), sharp, &Default::default()).unwrap();
    let draws = flat(display::page_display(&d, &lay(&d).pages[0], &Default::default()));
    let text = |w: &str| draws.iter().any(|x| matches!(x, display::Draw::Glyphs { text, .. } if text.contains(w)));
    assert!(text("Sharp") && text("Body"), "{draws:?}");
    assert!(!text("Blurred"), "{draws:?}");
    assert!(draws.iter().any(|x| matches!(x, display::Draw::Path { segs, fill: Some(_), .. } if segs.len() > 20)), "letters: {draws:?}");
}

/// The draws with figures opened up.
fn flat(draws: Vec<display::Draw>) -> Vec<display::Draw> {
    draws
        .into_iter()
        .flat_map(|x| match x {
            display::Draw::Figure { draws, .. } | display::Draw::Clipped { items: draws, .. } => draws,
            x => vec![x],
        })
        .collect()
}

#[test]
fn inline_chart_is_a_figure_with_its_alt_text() {
    use kition_doc_doc::graphic::{Graphic, GraphicItem, GraphicKind, PathSeg};
    use kition_doc_doc::props::Rgb;
    let items = vec![GraphicItem::Path {
        segs: vec![PathSeg::Move(1.0, 1.0), PathSeg::Line(2.0, 2.0)],
        fill: None,
        stroke: Some(Rgb::BLACK),
        stroke_width: 1.0,
    }];
    let graphic = Graphic { kind: GraphicKind::Chart, items, ..Default::default() };
    let obj = InlineObject::Graphic { w: 216.0, h: 144.0, alt: "Sales by quarter".into(), float: Default::default(), graphic: Arc::new(graphic) };
    let mut d = Document::from_text("Chart");
    d.insert_object(&Pos::body(0, 0), obj, &Default::default()).unwrap();
    let draws = display::page_display(&d, &lay(&d).pages[0], &Default::default());
    let figure = draws.iter().find_map(|x| if let display::Draw::Figure { alt, draws, .. } = x { Some((alt.clone(), draws.len())) } else { None });
    assert_eq!(figure, Some(("Sales by quarter".to_string(), 1)));
}

#[test]
fn chart_text_takes_letters_its_font_lacks_from_a_fallback_face() {
    use kition_doc_doc::graphic::{Graphic, GraphicItem, GraphicKind, TextAlign};
    use kition_doc_doc::props::Rgb;
    // A Chinese chart title in a Latin font: the Han letters come from a face that has them, so
    // none of them is drawn as a missing glyph.
    let title = |bold: bool| {
        let item = GraphicItem::Text {
            rect: [0.0, 0.0, 216.0, 24.0],
            text: "Sales 销售".into(),
            size: 14.0,
            color: Rgb::BLACK,
            bold,
            align: TextAlign::Center,
            font: Some("Calibri".into()),
            prompt: false,
        };
        let graphic = Graphic { kind: GraphicKind::Chart, items: vec![item], ..Default::default() };
        let obj = InlineObject::Graphic { w: 216.0, h: 144.0, alt: String::new(), float: Default::default(), graphic: Arc::new(graphic) };
        let mut d = Document::from_text("Chart");
        d.insert_object(&Pos::body(0, 0), obj, &Default::default()).unwrap();
        flat(display::page_display(&d, &lay(&d).pages[0], &Default::default()))
    };
    let draws = title(false);
    let runs: Vec<(String, u32, Vec<u32>)> = draws
        .iter()
        .filter_map(|x| match x {
            display::Draw::Glyphs { face, glyphs, text, .. } if text.contains("Sales") || text.contains('销') => {
                Some((text.clone(), face.id(), glyphs.iter().map(|g| g.0).collect()))
            }
            _ => None,
        })
        .collect();
    let han = runs.iter().find(|(t, _, _)| t.contains('销')).expect("the Han letters are drawn");
    assert!(han.2.iter().all(|g| *g != 0), "no missing glyph: {runs:?}");
    let latin = runs.iter().find(|(t, _, _)| t.contains("Sales")).expect("the Latin letters are drawn");
    assert_ne!(latin.1, han.1, "the Han letters come from another face");
    // One line: the Han run starts where the Latin run ends.
    let x = |t: &str| {
        draws.iter().find_map(|d| {
            if let display::Draw::Glyphs { glyphs, text, .. } = d {
                (text.as_str() == t).then(|| glyphs.first().map(|g| g.1)).flatten()
            } else {
                None
            }
        })
    };
    assert!(x(&han.0).unwrap() > x(&latin.0).unwrap());
    // A bold title is bold in both faces: a fallback face without a bold of its own is emboldened.
    let han_bold = title(true).iter().find_map(|x| match x {
        display::Draw::Glyphs { face, synth_bold, text, .. } if text.contains('销') => Some(*synth_bold || face.get().weight >= 600.0),
        _ => None,
    });
    assert_eq!(han_bold, Some(true), "the Han letters of a bold title are bold");
}

#[test]
fn graphic_paths_drop_segments_that_cannot_be_drawn() {
    use kition_doc_doc::graphic::{Graphic, GraphicItem, GraphicKind, PathSeg};
    use kition_doc_doc::props::Rgb;
    // The line before the first move, the move with a NaN, the infinite line and the line after the
    // close are all dropped.
    let segs = vec![
        PathSeg::Line(5.0, 5.0),
        PathSeg::Move(f32::NAN, 1.0),
        PathSeg::Move(1.0, 2.0),
        PathSeg::Line(3.0, 4.0),
        PathSeg::Line(f32::INFINITY, 0.0),
        PathSeg::Close,
        PathSeg::Line(9.0, 9.0),
    ];
    let items = vec![GraphicItem::Path { segs, fill: None, stroke: Some(Rgb::BLACK), stroke_width: 1.0 }];
    let graphic = Graphic { kind: GraphicKind::Chart, items, ..Default::default() };
    let obj = InlineObject::Graphic { w: 216.0, h: 144.0, alt: String::new(), float: Default::default(), graphic: Arc::new(graphic) };
    let mut d = Document::from_text("Chart");
    d.insert_object(&Pos::body(0, 0), obj, &Default::default()).unwrap();
    let draws = flat(display::page_display(&d, &lay(&d).pages[0], &Default::default()));
    let segs: Vec<PathSeg> =
        draws.iter().find_map(|x| if let display::Draw::Path { segs, .. } = x { Some(segs.clone()) } else { None }).expect("a path");
    assert!(matches!(segs.as_slice(), [PathSeg::Move(x, y), PathSeg::Line(..), PathSeg::Close] if x.is_finite() && y.is_finite()), "{segs:?}");
}

#[test]
fn oversized_graphic_draws_shrunk_with_its_items() {
    use kition_doc_doc::graphic::{Graphic, GraphicItem, GraphicKind};
    use kition_doc_doc::props::Rgb;
    // A 1000 x 500 pt chart is wider than the text: layout shrinks it, and its items with it.
    let items = vec![GraphicItem::Shape {
        rect: [0.0, 0.0, 1000.0, 500.0],
        kind: kition_doc_doc::para::ShapeKind::Rectangle,
        fill: None,
        stroke: Some(Rgb::BLACK),
        stroke_width: 4.0,
    }];
    let graphic = Graphic { kind: GraphicKind::Chart, items, w: 1000.0, h: 500.0, ..Default::default() };
    let obj = InlineObject::Graphic { w: 1000.0, h: 500.0, alt: String::new(), float: Default::default(), graphic: Arc::new(graphic) };
    let mut d = Document::from_text("Chart");
    d.insert_object(&Pos::body(0, 0), obj, &Default::default()).unwrap();
    let draws = flat(display::page_display(&d, &lay(&d).pages[0], &Default::default()));
    let (r, stroke) = draws
        .iter()
        .find_map(|x| if let display::Draw::Shape { rect, stroke_width, .. } = x { Some((*rect, *stroke_width)) } else { None })
        .expect("a shape");
    assert!(r.w < 1000.0, "{r:?}");
    assert!((r.h / r.w - 0.5).abs() < 1e-3, "{r:?}");
    assert!((stroke / r.w - 4.0 / 1000.0).abs() < 1e-5, "stroke {stroke} at width {}", r.w);
}

/// The paragraph at `path` on page 0: its first line's x, its y and its line count.
fn para_at(l: &DocLayout, path: &[u32]) -> Vec<(f32, f32, usize)> {
    l.pages[0]
        .items
        .iter()
        .filter_map(|it| match it {
            Placed::Lines { x, y, para, path: p, .. } if p.0 == path => Some((*x + para.lines.first().map_or(0.0, |l| l.left), *y, para.lines.len())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_floating_table_in_a_cell_takes_no_room_and_the_cell_holds_it() {
    use kition_doc_doc::para::Anchor;
    use kition_doc_doc::props::TableFloat;
    let text = |s: &str| kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(s, Default::default()));
    let mut inner = Table::new(3, 1, 200.0);
    for (i, r) in inner.rows.iter_mut().enumerate() {
        r.cells[0].blocks = vec![text(&format!("Inner {i}"))];
    }
    inner.props.float = Some(TableFloat { h_rel: Anchor::Margin, v_rel: Anchor::Paragraph, dist: [9.0, 0.0, 9.0, 0.0], ..Default::default() });
    let mut outer = Table::new(1, 1, 468.0);
    outer.rows[0].cells[0].blocks = vec![std::sync::Arc::new(kition_doc_doc::Block::Table(inner)), text("Beside")];
    let mut d = Document::from_text("Below");
    d.insert_block(StoryRef::Body, &Path::top(0), kition_doc_doc::Block::Table(outer)).unwrap();
    let l = lay(&d);
    let (&[(ix, iy, _)], &[(_, last_y, _)], &[(bx, by, _)], &[(_, below_y, _)]) =
        (&para_at(&l, &[0, 0, 0, 0, 0, 0, 0])[..], &para_at(&l, &[0, 0, 0, 0, 2, 0, 0])[..], &para_at(&l, &[0, 0, 0, 1])[..], &para_at(&l, &[1])[..])
    else {
        panic!("lines missing")
    };
    // The paragraph after the floating table runs beside it, level with its top.
    assert!((by - iy).abs() < 2.0, "beside at {by}, inner table at {iy}");
    assert!(bx > ix + 200.0, "beside text at {bx}, inner table text at {ix}");
    // The cell grows to hold the whole floating table.
    assert!(below_y > last_y + 10.0, "text below the outer table at {below_y}, the inner table's last row at {last_y}");
}

#[test]
fn autofit_sizes_columns_by_their_content() {
    let text = |s: &str| kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(s, Default::default()));
    // No width anywhere: the columns come from their content.
    let mut t = Table::new(1, 2, 468.0);
    t.rows[0].cells[0].blocks = vec![text("Short")];
    t.rows[0].cells[1].blocks = vec![text(&"A long cell that needs most of the width. ".repeat(4))];
    t.rows[0].cells.iter_mut().for_each(|c| c.props.width = None);
    let mut d = Document::from_text("After");
    d.insert_block(StoryRef::Body, &Path::top(0), kition_doc_doc::Block::Table(t)).unwrap();
    let l = lay(&d);
    let (&[(sx, _, _)], &[(lx, _, _)]) = (&para_at(&l, &[0, 0, 0, 0])[..], &para_at(&l, &[0, 0, 1, 0])[..]) else { panic!("lines missing") };
    assert!(lx - sx < 80.0, "the short column is {}pt wide", lx - sx);
    // A grid column narrower than its longest word grows; the others give up the difference.
    let mut t = Table::new(1, 3, 468.0);
    t.grid = vec![400.0, 20.0, 48.0];
    t.rows[0].cells[1].blocks = vec![text("Unbreakable")];
    let mut d = Document::from_text("After");
    d.insert_block(StoryRef::Body, &Path::top(0), kition_doc_doc::Block::Table(t)).unwrap();
    let l = lay(&d);
    let [(_, _, n)] = para_at(&l, &[0, 0, 1, 0])[..] else { panic!("line missing") };
    assert_eq!(n, 1, "the word is broken over {n} lines");
}

#[test]
fn autofit_grows_a_column_for_its_word_only_while_the_table_has_room() {
    let text = |s: &str| kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(s, Default::default()));
    // WPS for Mac (synthetic references): a narrow column grows to its longest word and the table
    // widens, the other columns keeping their widths.
    let columns = |grid: Vec<f32>, word: &str| {
        let mut t = Table::new(1, grid.len(), 468.0);
        t.grid = grid;
        t.rows[0].cells[1].blocks = vec![text(word)];
        t.rows[0].cells[2].blocks = vec![text("x")];
        let mut d = Document::from_text("After");
        d.insert_block(StoryRef::Body, &Path::top(0), kition_doc_doc::Block::Table(t)).unwrap();
        let l = lay(&d);
        let x = |c: u32| para_at(&l, &[0, 0, c, 0])[0].0;
        let lines = para_at(&l, &[0, 0, 1, 0])[0].2;
        (x(1) - x(0), x(2) - x(1), lines)
    };
    let (first, second, lines) = columns(vec![300.0, 20.0, 50.0], "Unbreakable");
    assert!((first - 300.0).abs() < 0.5 && second > 40.0 && lines == 1, "{first} {second} {lines}");
    // A grid wider than its room (to the borders, 478.8 pt) narrows to it, every column in
    // proportion, and the long word breaks (WPS references of 12 tables).
    let (first, _, lines) = columns(vec![100.0, 20.0, 400.0], "Unbreakable");
    assert!((first - 100.0 * 478.8 / 520.0).abs() < 0.5 && lines > 1, "{first} {lines}");
}

#[test]
fn a_tables_double_border_takes_its_whole_extent() {
    use kition_doc_doc::props::{Border, BorderStyle, Borders};
    // WPS for Mac (luan's double-bordered tables): a double outer border 0.5 pt wide takes 1.5 pt
    // above the first row and below the last, so the text after the table stands 2 pt lower than
    // after a single border.
    let after = |style: BorderStyle| {
        let line = Some(Border { style, width: 0.5, color: None, space: 0.0, theme: None });
        let mut t = Table::new(2, 1, 200.0);
        t.props.borders = Some(Borders { top: line, left: line, bottom: line, right: line, ..Default::default() });
        let mut d = Document::from_text("After");
        d.insert_block(StoryRef::Body, &Path::top(0), kition_doc_doc::Block::Table(t)).unwrap();
        para_at(&lay(&d), &[1])[0].1
    };
    let (single, double) = (after(BorderStyle::Single), after(BorderStyle::Double));
    assert!((double - single - 2.0).abs() < 0.01, "{single} {double}");
}

#[test]
fn autofit_handles_spanning_cells_long_words_and_empty_tables() {
    let text = |s: &str| kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(s, Default::default()));
    let table_x = |t: Table, path: &[u32]| {
        let mut d = Document::from_text("After");
        d.insert_block(StoryRef::Body, &Path::top(0), kition_doc_doc::Block::Table(t)).unwrap();
        para_at(&lay(&d), path)
    };
    let automatic = |rows: usize, cols: usize| {
        let mut t = Table::new(rows, cols, 468.0);
        t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| c.props.width = None);
        t
    };
    // A title cell spanning both columns needs its width shared between them: it stays on one line.
    let mut t = automatic(2, 2);
    t.rows[0].cells.truncate(1);
    t.rows[0].cells[0].props.span = 2;
    t.rows[0].cells[0].blocks = vec![text("A title that spans both columns")];
    t.rows[1].cells[0].blocks = vec![text("A")];
    t.rows[1].cells[1].blocks = vec![text("B")];
    let [(_, _, n)] = table_x(t, &[0, 0, 0, 0])[..] else { panic!("title missing") };
    assert_eq!(n, 1, "the title wraps over {n} lines");
    // An unbreakable string longer than the page doesn't widen the table past it.
    let mut t = automatic(1, 2);
    t.rows[0].cells[0].blocks = vec![text(&"x".repeat(300))];
    t.rows[0].cells[1].blocks = vec![text("Second")];
    let [(x, _, _)] = table_x(t, &[0, 0, 1, 0])[..] else { panic!("second cell missing") };
    assert!(x < 72.0 + 468.0, "the second column starts at {x}, past the margin");
    // A table of empty cells keeps its grid.
    let [(x, _, _)] = table_x(automatic(1, 2), &[0, 0, 1, 0])[..] else { panic!("second cell missing") };
    assert!(x > 72.0 + 200.0, "the second empty column starts at {x}");
    // Cells asking for a share of the table's width keep their grid too.
    let mut t = automatic(1, 2);
    for (c, s) in t.rows[0].cells.iter_mut().zip(["A", "B"]) {
        c.props.width_pct = Some(50.0);
        c.blocks = vec![text(s)];
    }
    let [(x, _, _)] = table_x(t, &[0, 0, 1, 0])[..] else { panic!("second cell missing") };
    assert!(x > 72.0 + 200.0, "the second 50% column starts at {x}");
}

#[test]
fn a_floating_table_running_past_the_page_runs_on_from_where_its_offset_puts_it() {
    use kition_doc_doc::para::Anchor;
    use kition_doc_doc::props::TableFloat;
    // WPS for Mac (synthetic references with a six-row table 19.8 pt below its anchor, and
    // word-ab5cc's narrow tables): a floating table that would run past the page's foot starts
    // where its offset puts it, the rows that don't fit carry on at the next page's top, still
    // floating, and the text after it starts there: below a column-wide table, beside a narrow one.
    for width in [468.0, 300.0] {
        let mut d = Document::from_text(&format!("{}After", "Filler\n".repeat(25)));
        let mut t = Table::new(6, 1, width);
        for (i, r) in t.rows.iter_mut().enumerate() {
            r.cells[0].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(&format!("Row {i}"), Default::default()))];
        }
        t.props.float = Some(TableFloat { h_rel: Anchor::Margin, v_rel: Anchor::Paragraph, y: 19.8, overlap: false, ..Default::default() });
        d.insert_block(StoryRef::Body, &Path::top(25), kition_doc_doc::Block::Table(t)).unwrap();
        let l = lay(&d);
        let place = |path: &[u32]| {
            l.pages.iter().enumerate().find_map(|(page, p)| {
                p.items.iter().find_map(|i| match i {
                    Placed::Lines { y, path: at, para, .. } if at.0 == path => Some((page, *y, para.lines[0].left)),
                    _ => None,
                })
            })
        };
        let filler = place(&[24]).unwrap();
        let first = place(&[25, 0, 0, 0]).unwrap();
        let last = place(&[25, 5, 0, 0]).unwrap();
        let after = place(&[26]).unwrap();
        assert_eq!(first.0, 0, "{width}: the first rows stay on the anchor's page");
        assert!(first.1 >= filler.1 + 19.8, "{width}: below its offset: {first:?} after {filler:?}");
        assert_eq!(last.0, 1, "{width}: the last rows run on to the next page");
        assert_eq!(after.0, 1, "{width}: the text after it goes on to the next page");
        if width > 400.0 {
            assert!(after.1 > last.1, "below a column-wide table's last row: {after:?} {last:?}");
        } else {
            assert!(after.1 < last.1 && after.2 >= width, "beside a narrow table's rows: {after:?} {last:?}");
        }
    }
}

#[test]
fn a_floating_table_in_the_footer_leaves_the_footer_where_it_is() {
    use kition_doc_doc::para::Anchor;
    use kition_doc_doc::props::TableFloat;
    let footer_y = |float: bool| {
        let mut d = Document::from_text("Body");
        let mut t = Table::new(1, 1, 100.0);
        t.rows[0].cells[0].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("Float", Default::default()))];
        if float {
            t.props.float = Some(TableFloat { h_rel: Anchor::Page, v_rel: Anchor::Page, y: 720.0, ..Default::default() });
        }
        let blocks = vec![
            kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text("Footer", Default::default())),
            std::sync::Arc::new(kition_doc_doc::Block::Table(t)),
        ];
        let id = d.add_part(kition_doc_doc::PartKind::Footer, blocks);
        d.last_section.footers.default = Some(id);
        let l = lay(&d);
        l.pages[0].footer.iter().find_map(|it| if let Placed::Lines { y, path, .. } = it { (path.0 == [0]).then_some(*y) } else { None }).unwrap()
    };
    // A floating table in the footer doesn't push the footer's text up.
    assert!(footer_y(true) >= footer_y(false) - 0.01, "footer text at {} with the float, {} without", footer_y(true), footer_y(false));
}

#[test]
fn a_split_row_moves_shading_below_the_cut_with_it() {
    use crate::table::{RowLayout, split_row};
    let line_row = |y: f32| {
        let d = Document::from_text("Line");
        let l = lay(&d);
        let Some(Placed::Lines { story, path, para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })).cloned() else {
            panic!("no line")
        };
        Placed::Lines { story, path, para, l0: 0, l1: 1, x: 0.0, y, turn: TextDirection::Horizontal }
    };
    let fill = |y: f32, h: f32| Placed::Fill { rect: kition_doc_geom::Rect::new(0.0, y, 100.0, h), color: kition_doc_doc::props::Rgb(200, 0, 0) };
    // The row's own shading (from its top) and a nested cell's shading starting below the cut.
    let row = RowLayout { height: 200.0, items: vec![line_row(0.0), line_row(150.0), fill(0.0, 200.0), fill(150.0, 50.0)] };
    let (a, b) = split_row(&row, 100.0).expect("splits");
    let fills = |r: &RowLayout| {
        r.items.iter().filter_map(|i| if let Placed::Fill { rect, .. } = i { Some((rect.y, rect.h)) } else { None }).collect::<Vec<_>>()
    };
    assert_eq!(fills(&a), vec![(0.0, 100.0)], "only the row's shading above the cut stays");
    let moved = fills(&b);
    assert_eq!(moved.first().map(|f| f.0), Some(0.0), "the row's shading starts the continuation: {moved:?}");
    let nested = moved.get(1).copied().unwrap_or_default();
    assert!(nested.0 > 0.0 && (nested.1 - 50.0).abs() < 0.01, "the nested shading keeps its place and size: {moved:?}");
}

#[test]
fn kinsoku_keeps_a_manual_line_break_before_a_closing_bracket() {
    // Kinsoku forbids a line starting with 」, but a manual line break (Shift+Enter) right
    // before one still ends the line there.
    let text = "日本語\n」テスト";
    let mut d = Document::from_text("");
    d.body = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(text, Default::default()))];
    let l = lay(&d);
    let mut lines = Vec::new();
    for p in &l.pages {
        for it in &p.items {
            if let Placed::Lines { para, l0, l1, .. } = it {
                lines.extend(para.lines[*l0..*l1].iter().map(|line| (line.start, line.end)));
            }
        }
    }
    let bracket = text.find('」').unwrap();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0].1, LineEnd::LineBreak, "{lines:?}");
    assert_eq!(lines[1].0, bracket, "the second line starts at the bracket: {lines:?}");
}

/// A rotated floating shape keeps text clear of its rotated bounds (it reaches further at
/// the side), its frame stays put (rotation is about the centre) and it is drawn turned; hit
/// testing follows the turned shape, not its frame.
#[test]
fn rotated_float_wraps_around_its_rotated_bounds() {
    let text = "Words flow around the turned shape here. ".repeat(30);
    let place = |rot: f32| {
        let mut d = Document::from_text(&text);
        let float = kition_doc_doc::para::Float {
            wrap: kition_doc_doc::para::Wrap::Square,
            h_rel: kition_doc_doc::para::Anchor::Column,
            v_rel: kition_doc_doc::para::Anchor::Paragraph,
            x: 0.0,
            y: 40.0,
            rot,
            ..Default::default()
        };
        let shape = InlineObject::Shape {
            vml: None,
            kind: kition_doc_doc::para::ShapeKind::Rectangle,
            w: 144.0,
            h: 40.0,
            fill: None,
            stroke: None,
            stroke_width: 1.0,
            float,
            story: None,
            freeform: None,
            effects: Default::default(),
            geom: None,
            uid: 0,
            gradient: None,
            connection: None,
            warp: None,
            body: None,
            dash: None,
        };
        d.insert_object(&Pos::body(0, 0), shape, &Default::default()).unwrap();
        let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions::default());
        let p = &l.pages[0];
        let Some(Placed::Lines { para, .. }) = p.items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
        // The widest indent of a line beside the shape.
        let left = para.lines.iter().map(|l| l.left).fold(0.0f32, f32::max);
        let hit = l.object(&Pos::body(0, 0), 0).unwrap();
        let drawn = crate::display::page_display(&d, p, &Default::default()).iter().any(|d| matches!(d, crate::display::Draw::Rotated { .. }));
        (left, hit, drawn, l)
    };
    let (flat, frame, flat_drawn, _) = place(0.0);
    let (turned, hit, drawn, l) = place(30.0);
    // 144 × 40 turned 30° is 144.7 wide: 0.36 pt further out on each side, the centre kept.
    let bounds = hit.bounds();
    assert!((bounds.w - (144.0 * 30f32.to_radians().cos() + 40.0 * 0.5)).abs() < 0.01, "{bounds:?}");
    assert_eq!(hit.rect, frame.rect);
    assert!(turned > flat && turned >= bounds.right() - hit.origin.x, "{flat} → {turned} ({bounds:?})");
    assert!(drawn && !flat_drawn);
    // Its frame's top-right corner turns away from where it was: no longer on the shape.
    let r = hit.rect;
    assert!(l.object_at(0, r.right() - 2.0, r.y + 2.0, 0.0).is_none());
    assert!(l.object_at(0, r.x + r.w / 2.0, r.y + r.h / 2.0, 0.0).is_some());
}

/// A group turns as a whole: each member's centre is carried round the group's centre and
/// the member drawn with the group's turn on top of its own; hit testing follows the turned
/// group. A shadow that doesn't rotate with its shape (`rotWithShape="0"`, Word's presets) keeps
/// its page direction inside the turn; one that does turns with the shape.
#[test]
fn rotated_group_turns_its_members_and_their_shadows() {
    use kition_doc_doc::effects::{Shadow, ShapeEffects};
    use kition_doc_doc::para::{Anchor, Float, GroupChild, ShapeKind, Wrap};
    let member = |rot_with_shape: bool, rot: f32| InlineObject::Shape {
        vml: None,
        kind: ShapeKind::Rectangle,
        w: 60.0,
        h: 40.0,
        fill: Some(kition_doc_doc::props::Rgb(1, 2, 3)),
        stroke: None,
        stroke_width: 0.0,
        float: Float { rot, ..Default::default() },
        story: None,
        freeform: None,
        effects: ShapeEffects { shadow: Some(Shadow { angle: 0.0, rot_with_shape, ..Default::default() }), ..Default::default() },
        geom: None,
        uid: 0,
        gradient: None,
        connection: None,
        warp: None,
        body: None,
        dash: None,
    };
    let float =
        Float { wrap: Wrap::InFrontOfText, h_rel: Anchor::Column, v_rel: Anchor::Paragraph, x: 100.0, y: 100.0, rot: 90.0, ..Default::default() };
    let group = InlineObject::Group {
        uid: 0,
        w: 120.0,
        h: 40.0,
        float,
        ch_w: 120.0,
        ch_h: 40.0,
        children: vec![GroupChild { x: 0.0, y: 0.0, obj: member(false, 0.0) }, GroupChild { x: 60.0, y: 0.0, obj: member(true, 10.0) }],
    };
    let mut d = Document::from_text("Text under the group.");
    d.insert_object(&Pos::body(0, 0), group, &Default::default()).unwrap();
    let l = lay(&d);
    let p = &l.pages[0];
    let hit = l.object(&Pos::body(0, 0), 0).unwrap();
    let g = hit.rect;
    let (cx, cy) = (g.x + g.w / 2.0, g.y + g.h / 2.0);
    assert_eq!(hit.spin.deg, 90.0);
    // Turned a quarter clockwise, the left member is now above the centre, the right one below.
    let shapes: Vec<(Rect, Spin)> =
        p.items.iter().filter_map(|i| if let Placed::Shape { rect, spin, .. } = i { Some((*rect, *spin)) } else { None }).collect();
    assert_eq!(shapes.len(), 2, "{shapes:?}");
    let centre = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01;
    assert!(near(centre(shapes[0].0), (cx, cy - 30.0)), "{shapes:?} about ({cx}, {cy})");
    assert!(near(centre(shapes[1].0), (cx, cy + 30.0)), "{shapes:?}");
    assert_eq!((shapes[0].1.deg, shapes[1].1.deg), (90.0, 100.0));
    // Drawn turned: the page-fixed shadow still falls to the right on the page (270° in the
    // quarter-turned frame); the other turns with its shape (still 0° in its own frame).
    let shadows: Vec<f32> = crate::display::page_display(&d, p, &Default::default())
        .iter()
        .filter_map(|d| match d {
            crate::display::Draw::Rotated { items, .. } => items.iter().find_map(|i| match i {
                crate::display::Draw::Shape { effects, .. } => effects.shadow.map(|s| s.angle),
                _ => None,
            }),
            _ => None,
        })
        .collect();
    assert_eq!(shadows.len(), 2, "{shadows:?}");
    assert!((shadows[0] - 270.0).abs() < 0.01 && shadows[1].abs() < 0.01, "{shadows:?}");
    // Grabbed on the turned group: its frame's left end is empty now, above the centre isn't.
    assert!(l.object_at(0, g.x + 5.0, cy, 0.0).is_none());
    assert!(l.object_at(0, cx, cy - 50.0, 0.0).is_some());
}

/// The pieces of footnote `id` as (page, first line, end line, top).
fn note_pieces(l: &DocLayout, id: u32) -> Vec<(usize, usize, usize, f32)> {
    let mut v = Vec::new();
    for (pi, p) in l.pages.iter().enumerate() {
        for it in &p.items {
            if let Placed::Lines { story: StoryRef::Part(s), l0, l1, y, .. } = it
                && *s == id
            {
                v.push((pi, *l0, *l1, *y));
            }
        }
    }
    v
}

/// Lengths of the horizontal rules on page `pi` below `y`.
fn rules_below(l: &DocLayout, pi: usize, y: f32) -> Vec<f32> {
    l.pages[pi]
        .items
        .iter()
        .filter_map(|it| if let Placed::Rule { x0, y0, x1, y1, .. } = it { (*y0 == *y1 && *y0 > y).then_some(x1 - x0) } else { None })
        .collect()
}

#[test]
fn long_footnotes_continue_on_the_next_page() {
    let mut d = Document::from_text(&"Body text line.\n".repeat(16));
    let long = footnote(&mut d, &Pos::body(4, 4), &"A long footnote that runs on and on. ".repeat(100));
    let own = footnote(&mut d, &Pos::body(14, 4), "The second page's own note.");
    let l = lay(&d);
    let refs = l.caret(&Pos::body(4, 4)).unwrap();
    assert_eq!(refs.page, 0, "the reference stays where it was");
    let pieces = note_pieces(&l, long);
    assert!(pieces.len() >= 2, "{pieces:?}");
    let (p0, p1) = (&pieces[0], &pieces[1]);
    assert_eq!((p0.0, p1.0), (0, 1), "split over pages 1 and 2: {pieces:?}");
    assert_eq!(p0.1, 0);
    assert_eq!(p0.2, p1.1, "continues at the next line");
    // Every line of the note is placed once.
    let lines = pieces.iter().map(|p| p.2 - p.1).sum::<usize>();
    let para = l.pages[0]
        .items
        .iter()
        .find_map(|it| if let Placed::Lines { story: StoryRef::Part(s), para, .. } = it { (*s == long).then(|| para.lines.len()) } else { None })
        .unwrap();
    assert_eq!(lines, para);
    // Page 1: the note fills the space under the text, which ends above it.
    let body_bottom = |pi: usize| {
        l.pages[pi]
            .items
            .iter()
            .filter_map(
                |it| if let Placed::Lines { story: StoryRef::Body, y, para, l0, l1, .. } = it { item_bottom(*y, para, *l0, *l1) } else { None },
            )
            .fold(0.0f32, f32::max)
    };
    assert!(body_bottom(0) <= p0.3, "{} {}", body_bottom(0), p0.3);
    assert_eq!(rules_below(&l, 0, body_bottom(0)), vec![144.0], "a normal separator on page 1");
    // Page 2: the continuation separator spans the text width, the rest of the long note comes
    // first and the page's own note follows it.
    assert!(body_bottom(1) <= p1.3, "{} {}", body_bottom(1), p1.3);
    assert_eq!(rules_below(&l, 1, body_bottom(1)), vec![468.0], "continuation separator");
    let own_ref = l.caret(&Pos::body(14, 4)).unwrap();
    let own_note = l.caret(&d.start_of(StoryRef::Part(own))).unwrap();
    assert_eq!((own_ref.page, own_note.page), (1, 1));
    assert!(own_note.top > p1.3, "own note after the continued one: {own_note:?} {p1:?}");
}

#[test]
fn short_footnotes_stay_whole() {
    let mut d = Document::from_text(&"Body text line.\n".repeat(60));
    let id = footnote(&mut d, &Pos::body(10, 4), "A short note, two lines long. ".repeat(4).as_str());
    let l = lay(&d);
    let pieces = note_pieces(&l, id);
    assert_eq!(pieces.len(), 1, "{pieces:?}");
    assert_eq!(pieces[0].0, l.caret(&Pos::body(10, 4)).unwrap().page);
}

#[test]
fn footnote_longer_than_pages_ends() {
    // A note several pages long after a one-line body: it continues on pages of its own.
    let mut d = Document::from_text("Body");
    let id = footnote(&mut d, &Pos::body(0, 4), &"Words in a very long note. ".repeat(1500));
    let l = lay(&d);
    let pieces = note_pieces(&l, id);
    assert!(pieces.len() >= 3 && l.pages.len() == pieces.len(), "{} pages, {pieces:?}", l.pages.len());
    assert!(pieces.windows(2).all(|w| w[0].2 == w[1].1 && w[1].0 == w[0].0 + 1), "{pieces:?}");
}

/// The caret at the start of text box `id`'s story, and the box.
fn box_caret(d: &Document, id: u32) -> (kition_doc_geom::Rect, f32, f32) {
    let l = lay(d);
    let c = l.caret(&Pos { story: StoryRef::Part(id), path: Path::top(0), off: 0 }).unwrap();
    (box_rect(&l, id), c.x, c.top)
}

#[test]
fn a_shape_sets_its_text_by_its_frame_anchor_and_margins() {
    use kition_doc_doc::graphic::{TextAnchor, TextFrame};
    let framed = |frame: Option<TextFrame>| {
        let mut d = Document::from_text("Body");
        let id = text_box(&mut d, 0, "One line", 144.0, 96.0, Default::default());
        let InlineObject::Shape { body, .. } = &mut d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().objects[0] else { panic!() };
        *body = frame;
        box_caret(&d, id)
    };
    let (r, x, top) = framed(None);
    assert!((x - (r.x + 7.2)).abs() < 0.5 && (top - (r.y + 3.6)).abs() < 2.0, "({x}, {top}) in {r:?}");
    let (r, _, middle) = framed(Some(TextFrame { anchor: TextAnchor::Middle, ..Default::default() }));
    // The line's top is half a line above the middle.
    assert!(middle < r.y + r.h / 2.0 && middle > r.y + r.h / 2.0 - 16.0, "{middle} in {r:?}");
    let (r, _, bottom) = framed(Some(TextFrame { anchor: TextAnchor::Bottom, ..Default::default() }));
    assert!(bottom > middle && bottom < r.bottom() - 3.6, "{bottom} in {r:?}");
    let (r, x, top) =
        framed(Some(TextFrame { anchor: TextAnchor::Top, insets: [20.0, 10.0, 0.0, 0.0], fit: false, no_wrap: false, fit_height: false }));
    assert!((x - (r.x + 20.0)).abs() < 0.5 && (top - (r.y + 10.0)).abs() < 2.0, "({x}, {top}) in {r:?}");
}

#[test]
fn a_table_fits_its_room_to_the_borders_unless_it_has_a_width_of_its_own() {
    use kition_doc_doc::props::Align;
    // WPS for Mac (synthetic references, 12 tables): a table's room reaches its borders, a cell
    // margin outside the text column on each side. A table with no width of its own whose grid
    // is wider narrows to the room, every column in proportion, and to the room from its indent
    // when it has one; a percentage width is a share of the room, past 100 % too; a width in
    // points stands, however wide.
    let widths = |width: Option<f32>, pct: Option<f32>, indent: Option<f32>, grid: Vec<f32>| {
        let mut t = Table::new(1, grid.len(), 468.0);
        t.grid = grid;
        t.props.width = width;
        t.props.width_pct = pct;
        t.props.indent = indent;
        t.props.align = Some(Align::Left);
        let mut d = Document::from_text("After");
        d.insert_block(StoryRef::Body, &Path::top(0), kition_doc_doc::Block::Table(t)).unwrap();
        let l = lay(&d);
        let x = |c: u32| para_at(&l, &[0, 0, c, 0])[0].0;
        (x(0), x(1) - x(0), x(2) - x(1))
    };
    let room = 468.0 + 2.0 * 5.4;
    let (_, first, second) = widths(None, None, None, vec![170.0, 170.0, 170.0]);
    assert!((first - room / 3.0).abs() < 0.1 && (second - room / 3.0).abs() < 0.1, "{first} {second}");
    let (x, first, _) = widths(None, None, Some(-50.0), vec![190.0, 190.0, 190.0]);
    assert!((first - (room + 50.0) / 3.0).abs() < 0.1, "{x} {first}");
    let (_, first, _) = widths(None, Some(120.0), None, vec![170.0, 170.0, 170.0]);
    assert!((first - room * 1.2 / 3.0).abs() < 0.1, "{first}");
    let (_, first, _) = widths(Some(550.0), None, None, vec![170.0, 170.0, 170.0]);
    assert!((first - 550.0 / 3.0).abs() < 0.1, "{first}");
}

#[test]
fn balanced_widths_make_every_space_of_a_run_half_an_em() {
    // WPS for Mac with w:balanceSingleByteDoubleByteWidth (synthetic references in Helvetica Neue,
    // Times New Roman and Arial): every space of a run of two or more is half an em, between
    // Latin letters too and all along a run beside Chinese; a single space between Latin letters
    // keeps its font's width.
    let mut d = Document::from_text("A  B C\u{6570}   \u{6570}");
    d.settings.compat_flags.push("balanceSingleByteDoubleByteWidth".into());
    let l = lay(&d);
    let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
    let size = para.styles[para.clusters[0].style as usize].size;
    let spaces: Vec<f32> = para.clusters.iter().filter(|c| c.kind == para::ClKind::Space).map(|c| c.adv / size).collect();
    assert_eq!(spaces.len(), 6);
    for k in [0, 1, 3, 4, 5] {
        assert!((spaces[k] - 0.5).abs() < 1e-4, "{spaces:?}");
    }
    assert!(spaces[2] < 0.45, "{spaces:?}");
}

#[test]
fn justified_spaces_give_up_a_tenth_and_balanced_ones_nothing() {
    // WPS for Mac (synthetic references in compatibility mode 15): a justified line's spaces take
    // back up to a tenth of their width to fit (7 spaces: 9.8 % fits, 10.8 % doesn't); spaces made
    // half an em by balanced widths take back nothing.
    let lines = |text: &str, squeeze: f32, balanced: bool| {
        let fits = |width: f32| {
            let mut d = Document::from_text(text);
            d.settings.compat_mode = 15;
            if balanced {
                d.settings.compat_flags.push("balanceSingleByteDoubleByteWidth".into());
            }
            d.last_section.page_w = width + 144.0;
            d.format_paragraphs(&Pos::body(0, 0), &Pos::body(0, 0), &|p| p.align = Some(Align::Justify)).unwrap();
            lines_of(&lay(&d))
        };
        let natural = {
            let mut d = Document::from_text(text);
            if balanced {
                d.settings.compat_flags.push("balanceSingleByteDoubleByteWidth".into());
            }
            let l = lay(&d);
            let Some(Placed::Lines { para, .. }) = l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { .. })) else { panic!() };
            let spaces: f32 = para.clusters.iter().filter(|c| c.kind == para::ClKind::Space).map(|c| c.adv).sum();
            (para.lines[0].end_x() - para.lines[0].xs[0], spaces)
        };
        fits(natural.0 - natural.1 * squeeze)
    };
    let single = "ab cd ef gh ij kl mn op";
    assert_eq!(lines(single, 0.098, false), 1);
    assert_eq!(lines(single, 0.108, false), 2);
    assert_eq!(lines("ab  cd  ef  gh  ij  kl", 0.02, true), 2);
}

#[test]
fn a_list_in_a_text_box_that_fits_its_text_counts_from_one() {
    use kition_doc_doc::graphic::{TextAnchor, TextFrame};
    // WPS for Mac (luan's rich text box): the box is measured before it is laid out, and its list
    // still numbers 1, 2, not on from the measuring.
    let mut d = Document::from_text("Body");
    let id = text_box(&mut d, 0, "", 200.0, 100.0, Default::default());
    let items = numbered(&mut d, &["one", "two"], ParaProps::default());
    if let Some(part) = d.parts.get_mut(&id) {
        part.blocks = items.into_iter().map(kition_doc_doc::para_block).collect();
    }
    let InlineObject::Shape { body, .. } = &mut d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().objects[0] else { panic!() };
    *body = Some(TextFrame { anchor: TextAnchor::Top, insets: [0.0; 4], fit: true, no_wrap: true, fit_height: false });
    assert_eq!(labels(&lay(&d)), ["1.", "2."]);
}

#[test]
fn a_footnote_in_a_table_row_sits_at_the_foot_of_the_rows_page() {
    // WPS for Mac (luan's small table): a footnote referenced in a table cell goes to the foot of
    // the page its row starts on, and the rows there get that much less room.
    let rows_on_first_page = |note: bool| {
        let mut d = Document::from_text(&"Body text line.\n".repeat(20));
        let mut t = Table::new(30, 1, 468.0);
        for (k, row) in t.rows.iter_mut().enumerate() {
            row.cells[0].blocks = vec![kition_doc_doc::para_block(kition_doc_doc::Paragraph::with_text(&format!("Row {k}"), Default::default()))];
        }
        d.insert_block(StoryRef::Body, &Path::top(20), kition_doc_doc::Block::Table(t)).unwrap();
        let id = note.then(|| {
            let lines = "A long footnote that takes a few lines at the foot of the page. ".repeat(6);
            footnote(&mut d, &Pos { story: StoryRef::Body, path: Path(vec![20, 0, 0, 0]), off: 3 }, &lines)
        });
        let l = lay(&d);
        let rows = l.pages[0].items.iter().filter(|it| matches!(it, Placed::Lines { story: StoryRef::Body, path, .. } if path.0.len() == 4)).count();
        let note_page = id.and_then(|id| l.caret_on(&Pos { story: StoryRef::Part(id), path: Path::top(0), off: 0 }, 0)).map(|c| c.page);
        (rows, note_page)
    };
    let (plain, _) = rows_on_first_page(false);
    let (with_note, page) = rows_on_first_page(true);
    assert_eq!(page, Some(0), "the note is at the foot of the first page");
    assert!(with_note < plain, "the note leaves fewer rows on the page: {with_note} vs {plain}");
}

#[test]
fn turned_text_wraps_at_an_at_least_row_height() {
    // WPS for Mac (luan's 90 degree cell): an "at least" row height sets the length of turned
    // lines too, longer text wrapping onto more lines across the cell rather than the row growing.
    let mut d = turned_table(TextDirection::Down, &"word ".repeat(30), None);
    if let Some(Block::Table(t)) = d.body.get_mut(1).map(std::sync::Arc::make_mut) {
        t.rows[0].props.height = Some(72.0);
        t.rows[0].props.height_rule = kition_doc_doc::props::HeightRule::AtLeast;
    }
    let l = lay(&d);
    let it = turned_lines(&l);
    let lines = if let Placed::Lines { l0, l1, .. } = it { l1 - l0 } else { 0 };
    assert!(lines > 2, "the text wraps at the row height: {lines} lines");
    let b = it.turned_bounds().unwrap();
    assert!(b.h <= 72.0, "{b:?}");
}
