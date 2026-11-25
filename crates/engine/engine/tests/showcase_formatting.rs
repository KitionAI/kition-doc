//! Formatting problems met while building the Kition Showcase samples.

use kition_doc_engine::{
    Session,
    doc::{
        Document, Path, StoryRef,
        props::{Rgb, TextColor},
    },
    layout::Placed,
};
use serde_json::{Value, json};

fn session() -> Session {
    Session::new(Document::new())
}

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// Character formatting of the body paragraph `i` at byte `off`.
fn body_char(s: &Session, i: usize, off: usize) -> kition_doc_engine::doc::CharProps {
    s.doc.para(StoryRef::Body, &Path::top(i)).map(|p| p.props_of_char(off).clone()).unwrap_or_default()
}

/// Colours are `RRGGBB` everywhere, as `format.state` and `format.color` give them: in
/// `format.set`, `style.set` and `paragraph.set`, borders included.
#[test]
fn colours_take_rrggbb() {
    let mut s = session();
    run(&mut s, "text.insert", json!({"text": "Red words here"}));
    run(&mut s, "select.text", json!({"text": "Red"}));
    run(&mut s, "format.set", json!({"props": {"color": "C00000", "shading": "#FFFF00", "underlineColor": "0000ff"}}));
    let c = body_char(&s, 0, 0);
    assert_eq!(c.color, Some(TextColor::Rgb(Rgb(0xC0, 0, 0))));
    assert_eq!(c.shading, Some(Rgb(0xFF, 0xFF, 0)));
    assert_eq!(c.underline_color, Some(Rgb(0, 0, 0xFF)));
    run(&mut s, "format.set", json!({"props": {"color": "auto"}}));
    assert_eq!(body_char(&s, 0, 0).color, Some(TextColor::Auto));
    // The form the document reports (`file.paragraph`) is taken back as it is.
    let reported = run(&mut s, "file.paragraph", json!({"path": [0]}));
    run(&mut s, "select.text", json!({"text": "words"}));
    run(&mut s, "format.set", json!({"props": {"color": {"Rgb": [1, 2, 3]}}}));
    assert_eq!(body_char(&s, 0, 4).color, Some(TextColor::Rgb(Rgb(1, 2, 3))), "{reported}");

    run(
        &mut s,
        "style.set",
        json!({"style": "Normal", "chr": {"color": "112233"}, "para": {"shading": "EEEEEE", "borders": {"bottom": {"style": "single", "width": 1, "color": "FF0000"}}}}),
    );
    let normal = s.doc.styles.get("Normal").expect("Normal");
    assert_eq!(normal.chr.color, Some(TextColor::Rgb(Rgb(0x11, 0x22, 0x33))));
    assert_eq!(normal.para.shading, Some(Rgb(0xEE, 0xEE, 0xEE)));
    assert_eq!(normal.para.borders.and_then(|b| b.bottom).and_then(|b| b.color), Some(Rgb(0xFF, 0, 0)));

    run(&mut s, "paragraph.set", json!({"props": {"shading": "00FF00"}}));
    assert_eq!(s.doc.para(StoryRef::Body, &Path::top(0)).and_then(|p| p.props.shading), Some(Rgb(0, 0xFF, 0)));

    // A colour that isn't one is refused with a message that says what a colour is.
    let err = s.run("format.set", &json!({"props": {"color": "red"}})).expect_err("not a colour").to_string();
    assert!(err.contains("RRGGBB"), "{err}");
}

/// Formatting chosen at the caret belongs to the place it was chosen: it doesn't follow the caret
/// from the footer into the body, or from the body into the header.
#[test]
fn caret_formatting_stays_in_its_story() {
    let mut s = session();
    run(&mut s, "footer.insert", json!({"text": "Footer words"}));
    run(&mut s, "format.fontSize", json!({"size": 9}));
    run(&mut s, "header.close", json!({}));
    run(&mut s, "text.insert", json!({"text": "Body"}));
    assert_eq!(body_char(&s, 0, 0).size, None, "the footer's size leaked into the body");

    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "format.italic", json!({"value": true}));
    run(&mut s, "header.insert", json!({"text": ""}));
    run(&mut s, "text.insert", json!({"text": "Header"}));
    let story = s.sel.focus.story;
    assert_ne!(story, StoryRef::Body);
    let header = s.doc.para(story, &Path::top(0)).map(|p| p.props_of_char(0).clone()).unwrap_or_default();
    assert_ne!(header.italic, Some(true), "the body's italics leaked into the header");
}

/// Word's Clear All Formatting makes the paragraph Normal (it is documented so); Reset
/// Character Formatting (Ctrl+Space) removes the character formatting and keeps the style, also
/// in an empty paragraph.
#[test]
fn reset_character_formatting_keeps_the_paragraph_style() {
    let mut s = session();
    run(&mut s, "paragraph.heading1", json!({}));
    run(&mut s, "format.bold", json!({"value": false}));
    run(&mut s, "format.resetChar", json!({}));
    let p = s.doc.para(StoryRef::Body, &Path::top(0)).expect("paragraph");
    assert_eq!(p.props.style.as_deref(), Some("Heading1"));
    assert_eq!(s.typing_props().bold, None, "the caret's own bold is gone");
    run(&mut s, "text.insert", json!({"text": "Title words"}));
    run(&mut s, "select.text", json!({"text": "Title"}));
    run(&mut s, "format.italic", json!({"value": true}));
    run(&mut s, "format.resetChar", json!({}));
    assert_eq!(body_char(&s, 0, 0).italic, None);
    assert_eq!(s.doc.para(StoryRef::Body, &Path::top(0)).and_then(|p| p.props.style.clone()).as_deref(), Some("Heading1"));
    // Clear All Formatting resets the paragraph to Normal, as in Word.
    run(&mut s, "format.clear", json!({}));
    assert_eq!(s.doc.para(StoryRef::Body, &Path::top(0)).and_then(|p| p.props.style.clone()), None);
}

/// A drop cap belongs to its paragraph: the paragraphs typed after it have none.
#[test]
fn a_drop_cap_stays_on_its_paragraph() {
    let mut s = session();
    run(&mut s, "text.insert", json!({"text": "Once upon a time there was a long paragraph."}));
    run(&mut s, "dropCap.set", json!({"lines": 3}));
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "The next one."}));
    assert_eq!(s.doc.para(StoryRef::Body, &Path::top(0)).and_then(|p| p.props.drop_cap), Some(3));
    assert_eq!(s.doc.para(StoryRef::Body, &Path::top(1)).and_then(|p| p.props.drop_cap), None);
}

/// A drop cap spans its lines at any line spacing: the letter's capital reaches from the first
/// line's capitals to the last dropped line's baseline.
#[test]
fn a_drop_cap_spans_its_lines_at_any_spacing() {
    for spacing in [json!({"rule": "multiple", "value": 1.0}), json!({"rule": "multiple", "value": 1.5}), json!({"rule": "exactly", "value": 24})] {
        let mut s = session();
        run(&mut s, "text.insert", json!({"text": "Every line of this paragraph wraps around a large first letter. ".repeat(8)}));
        run(&mut s, "paragraph.set", json!({"props": {"lineSpacing": spacing.clone(), "spaceAfter": 0, "spaceBefore": 0}}));
        run(&mut s, "dropCap.set", json!({"lines": 4}));
        let layout = s.layout();
        let pl =
            layout.pages[0].items.iter().find_map(|i| if let Placed::Lines { para, .. } = i { Some(para.clone()) } else { None }).expect("lines");
        let (nc, lines, _) = pl.drop_cap.expect("a drop cap");
        assert_eq!((nc, lines), (1, 4));
        let size = |c: usize| pl.clusters.get(c).and_then(|c| pl.styles.get(c.style as usize)).map(|st| st.size).unwrap_or(0.0);
        let (big, body) = (size(0), size(nc + 1));
        let reach = pl.lines[3].baseline - pl.lines[0].baseline + 0.7 * body;
        let cap = 0.7 * big;
        assert!((cap - reach).abs() < reach * 0.12, "{spacing}: the capital is {cap} pt tall, the four lines {reach} pt");
    }
}

/// The Paragraph dialog's answer is the paragraph's formatting, for the caret's paragraph or the
/// one at `path`, in the form `paragraph.set` takes.
#[test]
fn the_paragraph_dialog_answers_the_formatting() {
    let mut s = session();
    run(&mut s, "text.insert", json!({"text": "First"}));
    run(&mut s, "paragraph.set", json!({"props": {"align": "center", "spaceAfter": 12}}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "paragraph.set", json!({"props": {"align": "right"}}));
    let here = run(&mut s, "paragraph.dialog", json!({}));
    assert_eq!(here["props"]["align"], "right", "{here}");
    let first = run(&mut s, "paragraph.dialog", json!({"path": [0]}));
    assert_eq!(first["props"]["align"], "center", "{first}");
    assert_eq!(first["props"]["spaceAfter"], 12.0, "{first}");
    // What it answers can be given back.
    run(&mut s, "paragraph.set", json!({"props": first["props"].clone()}));
    assert_eq!(run(&mut s, "paragraph.dialog", json!({}))["props"]["align"], "center");
}

/// A list number is formatted as its paragraph mark: Bold turned off before Enter leaves the next
/// item's number plain, though the text before Enter ended bold; Bold
/// turned on in an empty item makes its number bold, as in Word.
#[test]
fn a_list_number_does_not_take_bold_turned_off_before_enter() {
    let mut s = session();
    run(&mut s, "paragraph.numbering", json!({"kind": "numbered"}));
    run(&mut s, "text.insert", json!({"text": "Hello "}));
    run(&mut s, "format.bold", json!({"value": true}));
    run(&mut s, "text.insert", json!({"text": "world"}));
    run(&mut s, "format.bold", json!({"value": false}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "Next"}));
    let marks: Vec<Option<bool>> = s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| p.mark.bold.filter(|b| *b)).collect();
    assert_eq!(marks, [None, None], "neither number is bold");
    assert_ne!(body_char(&s, 1, 0).bold, Some(true), "nor is the text typed after Enter");

    let mut s = session();
    run(&mut s, "paragraph.numbering", json!({"kind": "numbered"}));
    run(&mut s, "format.bold", json!({"value": true}));
    run(&mut s, "text.insert", json!({"text": "Heading"}));
    assert_eq!(s.doc.body.iter().find_map(|b| b.as_para()).and_then(|p| p.mark.bold), Some(true));
}
