use super::*;
use kition_doc_doc::Pos;

const AMBIGUOUS: &str = "A\u{2014}\u{03b1}\u{0101}\u{00d7}\u{00e9}\u{20ac}";

fn families(hint: &str, language: &str, direct_override: bool) -> Vec<String> {
    families_for(AMBIGUOUS, hint, language, direct_override)
}

fn families_for(text: &str, hint: &str, language: &str, direct_override: bool) -> Vec<String> {
    let mut doc = Document::from_text(text);
    doc.styles.default_chr.font = Some("Source Serif 4".into());
    doc.styles.default_chr.font_ea = Some("Source Sans 3".into());
    doc.styles.default_chr.font_hint = Some(hint.into());
    doc.styles.default_chr.lang_ea = Some(language.into());
    if direct_override {
        doc.format_range(&Pos::body(0, 1), &Pos::body(0, 4), &|props| props.font_hint = Some("default".into())).unwrap();
    }
    let layout = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    let Placed::Lines { para, .. } = &layout.pages[0].items[0] else { panic!() };
    para.clusters.iter().map(|cluster| para.styles[cluster.style as usize].face.family.clone()).collect()
}

#[test]
fn font_hint_routes_ambiguous_characters_and_inherits_language() {
    let latin = "Source Serif 4";
    let east_asian = "Source Sans 3";
    assert_eq!(families("eastAsia", "zh-CN", false), [latin, east_asian, east_asian, east_asian, east_asian, east_asian, east_asian]);
    assert_eq!(families("eastAsia", "ja-JP", false), [latin, east_asian, east_asian, latin, east_asian, latin, east_asian]);
    assert_eq!(families("default", "zh-CN", false), [latin; 7]);
    assert_eq!(families("cs", "zh-CN", false), [latin; 7]);
}

#[test]
fn font_hint_direct_formatting_overrides_inherited_hint() {
    let latin = "Source Serif 4";
    let east_asian = "Source Sans 3";
    assert_eq!(families("eastAsia", "zh-TW", true), [latin, latin, east_asian, east_asian, east_asian, east_asian, east_asian]);
}

#[test]
fn font_hint_keeps_covered_punctuation_in_the_requested_east_asian_face() {
    let mut doc = Document::from_text("\u{4e2d}\u{2014}");
    doc.styles.default_chr.font = Some("Source Serif 4".into());
    doc.styles.default_chr.font_ea = Some("Source Sans 3".into());
    doc.styles.default_chr.font_hint = Some("eastAsia".into());
    doc.styles.default_chr.lang_ea = Some("zh-CN".into());
    let layout = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
    let Placed::Lines { para, .. } = &layout.pages[0].items[0] else { panic!() };
    let punctuation = para.clusters.iter().find(|cluster| cluster.start == 3).unwrap();
    assert_eq!(para.styles[punctuation.style as usize].face.family, "Source Sans 3");
}

#[test]
fn font_hint_keeps_combining_marks_with_the_base_character() {
    assert_eq!(families_for("A\u{0301}\u{2014}", "eastAsia", "zh-CN", false), ["Source Serif 4", "Source Sans 3"]);
}

#[test]
fn font_hint_private_symbol_codes_keep_the_explicit_symbol_font() {
    for family in ["Symbol", "Wingdings", "Webdings"] {
        let mut doc = Document::from_text("\u{f0b7}");
        doc.styles.default_chr.font = Some(family.into());
        doc.styles.default_chr.font_ea = Some("Source Sans 3".into());
        doc.styles.default_chr.font_hint = Some("eastAsia".into());
        let layout = layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
        let Placed::Lines { para, .. } = &layout.pages[0].items[0] else { panic!() };
        assert_eq!(para.clusters.len(), 1);
        assert_eq!(para.styles[para.clusters[0].style as usize].rc.font, family);
    }
}
