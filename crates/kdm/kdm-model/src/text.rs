//! A paragraph's text as a KDM `text` field with marks, so
//! concurrent typing in one paragraph merges by character.
//!
//! - The field holds the paragraph's text, U+FFFC object anchors included. Positions are Unicode
//!   scalar values; the engine's runs ([`Run::len`]) count bytes, so every conversion walks
//!   characters, never byte offsets.
//! - Each run property ([`CharProps`] field, by its serde name) is a mark of its own
//!   ([`RUN_MARKS`]): bold set on one range and italic on an overlapping one both apply. A
//!   property this build does not name goes into one `extra` JSON mark. References in them
//!   (`ins`/`del` revisions, a `fmtChange`'s revision) are written as object ids, as everywhere
//!   (`refs`).
//! - The k-th U+FFFC carries an `object` mark naming the `Inline` object that holds the k-th
//!   inline object (`value`, references as object ids). An anchor whose object is missing reads
//!   as nothing: the view drops the character, and the next write of the paragraph deletes it.
//! - A field's runs as last read or written ([`ParaText`], kept in [`crate::BlockIds`]) are what
//!   the next write diffs from (`kition_kit::session::write_rich_text`): the replica's truth, not
//!   the view's, so a view that dropped something still writes correct positions.

use std::collections::BTreeMap;
use std::sync::Arc;

use kition_doc_doc::para::{OBJ, Run};
use kition_doc_doc::props::CharProps;
use kition_doc_doc::{InlineObject, Paragraph};
use kition_kit::kdm::{ObjectId, TextRun};
use serde_json::{Map, Value};

use crate::refs::{Locals, Objects};
use crate::{KdmResult, kdm_err};

/// The `Block` field holding a paragraph's text.
pub(crate) const TEXT: &str = "text";
/// The mark naming an anchor's `Inline` object.
pub(crate) const OBJECT_MARK: &str = "object";
/// The mark holding the run properties without a mark of their own.
pub(crate) const EXTRA_MARK: &str = "extra";
/// The type of an inline object's KDM object.
pub(crate) const INLINE: &str = "Inline";

/// The run properties with a mark of their own: [`CharProps`]'s fields by serde name, as
/// `model/schema.json` declares them (a test keeps the two equal).
pub(crate) const RUN_MARKS: &[&str] = &[
    "style",
    "font",
    "size",
    "bold",
    "italic",
    "underline",
    "underlineColor",
    "strike",
    "doubleStrike",
    "color",
    "highlight",
    "shading",
    "border",
    "vertAlign",
    "caps",
    "smallCaps",
    "hidden",
    "spacing",
    "scale",
    "fitText",
    "eastAsianLayout",
    "position",
    "kern",
    "outline",
    "shadow",
    "emboss",
    "engrave",
    "lang",
    "noProof",
    "rtl",
    "cs",
    "fontCs",
    "fontEa",
    "fontHint",
    "sizeCs",
    "boldCs",
    "italicCs",
    "langBidi",
    "langEa",
    "textEffects",
    "colorTheme",
    "shadingTheme",
    "fontTheme",
    "link",
    "ins",
    "del",
    "fmtChange",
    "control",
];

type Marks = BTreeMap<String, Value>;

/// A paragraph text field's runs, as the replica holds them (see the module docs). Cheap to clone.
#[derive(Clone, Debug, Default)]
pub struct ParaText(Arc<Vec<TextRun>>);

impl ParaText {
    pub(crate) fn new(runs: Vec<TextRun>) -> Self {
        Self(Arc::new(runs))
    }

    /// The runs, merged (adjacent runs differ in their marks), none empty.
    pub fn runs(&self) -> &[TextRun] {
        &self.0
    }

    /// The `Inline` objects the field's anchors name.
    pub(crate) fn inlines(&self) -> Vec<ObjectId> {
        self.0.iter().filter_map(|run| run.attributes.get(OBJECT_MARK)?.as_str().map(ObjectId::from_string)).collect()
    }
}

impl PartialEq for ParaText {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}

impl Eq for ParaText {}

/// `value` as the replica stores it: whole floats as integers, object keys sorted. Marks are
/// compared with what the replica holds, so they are written in this form.
pub(crate) fn canonical(value: Value) -> Value {
    const SAFE: f64 = 9_007_199_254_740_992.0;
    match value {
        Value::Number(number) if !number.is_i64() && !number.is_u64() => match number.as_f64() {
            #[allow(clippy::cast_possible_truncation)]
            Some(float) if float.fract() == 0.0 && float.abs() <= SAFE => Value::from(float as i64),
            _ => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        Value::Object(fields) => {
            let mut sorted: Vec<(String, Value)> = fields.into_iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(sorted.into_iter().map(|(key, value)| (key, canonical(value))).collect())
        }
        other => other,
    }
}

/// A run's properties as marks, references written as object ids.
pub(crate) fn char_marks(props: &CharProps, objects: &Objects) -> KdmResult<Marks> {
    let mut out = Marks::new();
    if props.is_empty() {
        return Ok(out);
    }
    let mut v = serde_json::to_value(props).map_err(kdm_err)?;
    objects.translate(&mut v);
    let Value::Object(map) = canonical(v) else { return Ok(out) };
    let mut extra = Map::new();
    for (key, value) in map {
        if value.is_null() {
            continue;
        }
        if RUN_MARKS.contains(&key.as_str()) {
            out.insert(key, value);
        } else {
            extra.insert(key, value);
        }
    }
    if !extra.is_empty() {
        out.insert(EXTRA_MARK.to_owned(), Value::Object(extra));
    }
    Ok(out)
}

/// [`char_marks`] back, references read as local ids (one naming nothing is cleared). A value
/// that does not read (another build's, or a merge's) is dropped, the rest kept.
pub(crate) fn char_props(marks: &Marks, locals: &Locals) -> CharProps {
    let mut map = Map::new();
    if let Some(Value::Object(extra)) = marks.get(EXTRA_MARK) {
        for (key, value) in extra {
            if !RUN_MARKS.contains(&key.as_str()) {
                map.insert(key.clone(), value.clone());
            }
        }
    }
    for (key, value) in marks {
        if key != OBJECT_MARK && key != EXTRA_MARK {
            map.insert(key.clone(), value.clone());
        }
    }
    if map.is_empty() {
        return CharProps::default();
    }
    let mut v = Value::Object(map);
    locals.translate(&mut v);
    if let Ok(props) = serde_json::from_value::<CharProps>(v.clone()) {
        return props;
    }
    let mut kept = Map::new();
    if let Value::Object(map) = v {
        for (key, value) in map {
            kept.insert(key.clone(), value);
            if serde_json::from_value::<CharProps>(Value::Object(kept.clone())).is_err() {
                kept.remove(&key);
            }
        }
    }
    serde_json::from_value(Value::Object(kept)).unwrap_or_default()
}

/// Adjacent runs with equal marks merged, empty runs dropped: the form the replica reads back.
pub(crate) fn merged(runs: Vec<TextRun>) -> Vec<TextRun> {
    let mut out: Vec<TextRun> = Vec::with_capacity(runs.len());
    for run in runs {
        if run.insert.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.attributes == run.attributes => last.insert.push_str(&run.insert),
            _ => out.push(run),
        }
    }
    out
}

/// A paragraph's text field: its text, each character marked with its run's properties, the k-th
/// U+FFFC also with `inlines[k]` (the object of `p.objects[k]`). Text past the runs (never
/// expected) has no marks.
pub(crate) fn para_runs(p: &Paragraph, inlines: &[ObjectId], objects: &Objects) -> KdmResult<Vec<TextRun>> {
    let marks: Vec<Marks> = p.runs.iter().map(|r| char_marks(&r.props, objects)).collect::<KdmResult<_>>()?;
    let none = Marks::new();
    let mut out: Vec<TextRun> = Vec::new();
    let mut run = 0usize;
    let mut run_end = p.runs.first().map_or(0, |r| r.len);
    let mut k = 0usize;
    let mut current: Option<usize> = None;
    for (at, c) in p.text.char_indices() {
        while run < p.runs.len() && at >= run_end {
            run += 1;
            run_end = run_end.saturating_add(p.runs.get(run).map_or(0, |r| r.len));
        }
        if c == OBJ {
            let mut attributes = marks.get(run).unwrap_or(&none).clone();
            if let Some(id) = inlines.get(k) {
                attributes.insert(OBJECT_MARK.to_owned(), Value::String(id.as_str().to_owned()));
            }
            k += 1;
            out.push(TextRun { insert: c.to_string(), attributes });
            current = None;
            continue;
        }
        if current == Some(run)
            && let Some(last) = out.last_mut()
        {
            last.insert.push(c);
            continue;
        }
        out.push(TextRun { insert: c.to_string(), attributes: marks.get(run).unwrap_or(&none).clone() });
        current = Some(run);
    }
    Ok(merged(out))
}

/// A paragraph's text, runs and inline objects from its text field's runs. `inline` reads an
/// anchor's object (by id); an anchor without one is dropped (see the module docs).
pub(crate) fn para_parts(
    runs: &[TextRun],
    locals: &Locals,
    inline: &mut dyn FnMut(&str) -> Option<InlineObject>,
) -> (String, Vec<Run>, Vec<InlineObject>) {
    let mut text = String::new();
    let mut out: Vec<Run> = Vec::with_capacity(runs.len());
    let mut objects = Vec::new();
    for run in runs {
        let props = char_props(&run.attributes, locals);
        let object = run.attributes.get(OBJECT_MARK).and_then(Value::as_str);
        let start = text.len();
        for c in run.insert.chars() {
            if c == OBJ {
                match object.and_then(&mut *inline) {
                    Some(o) => {
                        objects.push(o);
                        text.push(OBJ);
                    }
                    None => continue,
                }
            } else {
                text.push(c);
            }
        }
        let len = text.len() - start;
        if len == 0 {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.props == props => last.len += len,
            _ => out.push(Run { len, props }),
        }
    }
    (text, out, objects)
}

/// For each character of `new`, the character of `old` it keeps after `edits` (`diff_text`'s,
/// from `old` to `new`), if any.
pub(crate) fn kept_from(old_len: usize, new_len: usize, edits: &[kition_kit::session::TextEdit]) -> Vec<Option<usize>> {
    let mut out = Vec::with_capacity(new_len);
    let mut old_at = 0usize;
    for edit in edits {
        while old_at < edit.index && old_at < old_len {
            out.push(Some(old_at));
            old_at += 1;
        }
        out.extend(std::iter::repeat_n(None, edit.insert.chars().count()));
        old_at = edit.index.saturating_add(edit.delete);
    }
    while out.len() < new_len {
        out.push((old_at < old_len).then_some(old_at));
        old_at += 1;
    }
    out.truncate(new_len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_kit::session::{KitModel, diff_text};

    #[test]
    fn the_schema_declares_every_run_mark() {
        let schema = <crate::DocModel as KitModel>::schema();
        let def = schema.field("Block", TEXT).unwrap_or_else(|e| panic!("{e}"));
        let mut declared: Vec<&str> = def.marks.keys().map(String::as_str).collect();
        declared.sort_unstable();
        let mut expected: Vec<&str> = RUN_MARKS.iter().copied().chain([OBJECT_MARK, EXTRA_MARK]).collect();
        expected.sort_unstable();
        assert_eq!(declared, expected);
    }

    #[test]
    fn every_char_props_field_has_a_mark() {
        // Every field set: what serde names is exactly the mark list.
        let props = CharProps {
            style: Some("s".into()),
            font: Some("f".into()),
            size: Some(1.5),
            bold: Some(true),
            italic: Some(true),
            underline: Some(Default::default()),
            underline_color: Some(Default::default()),
            strike: Some(true),
            double_strike: Some(true),
            color: Some(Default::default()),
            highlight: Some(Default::default()),
            shading: Some(Default::default()),
            border: Some(Default::default()),
            vert_align: Some(Default::default()),
            caps: Some(true),
            small_caps: Some(true),
            hidden: Some(true),
            spacing: Some(1.5),
            scale: Some(1.5),
            fit_text: Some(kition_doc_doc::props::FitText { width: 90.0, id: Some(42) }),
            east_asian_layout: Some(kition_doc_doc::props::EastAsianLayout { id: Some(42), combine: Some(true), ..Default::default() }),
            position: Some(1.5),
            kern: Some(1.5),
            outline: Some(true),
            shadow: Some(true),
            emboss: Some(true),
            engrave: Some(true),
            lang: Some("en".into()),
            no_proof: Some(true),
            rtl: Some(true),
            cs: Some(true),
            font_cs: Some("f".into()),
            font_ea: Some("f".into()),
            font_hint: Some("eastAsia".into()),
            size_cs: Some(1.5),
            bold_cs: Some(true),
            italic_cs: Some(true),
            lang_bidi: Some("fa".into()),
            lang_ea: Some("zh-CN".into()),
            text_effects: Some("<w14:ligatures w14:val=\"standard\"/>".into()),
            text_outline: None,
            text_blur: None,
            color_theme: Some(kition_doc_doc::props::ThemeColor {
                slot: kition_doc_doc::props::ThemeSlot::Accent1,
                tint: Some(0x99),
                shade: None,
                rgb: Default::default(),
            }),
            shading_theme: Some(kition_doc_doc::props::ThemeColor {
                slot: kition_doc_doc::props::ThemeSlot::Background1,
                tint: None,
                shade: Some(0xD9),
                rgb: Default::default(),
            }),
            font_theme: Some(Box::new(kition_doc_doc::props::ThemeFonts { ascii: Some("minorHAnsi".into()), ..Default::default() })),
            link: Some("https://example.com".into()),
            ins: Some(0),
            del: Some(0),
            fmt_change: kition_doc_doc::props::PropChange::boxed(0, CharProps::default()),
            control: Some(kition_doc_doc::props::ControlMarks { start: Some(0.0), end: Some(20.0) }),
        };
        let marks = char_marks(&props, &Objects::default()).unwrap_or_else(|e| panic!("{e}"));
        assert!(!marks.contains_key(EXTRA_MARK), "a CharProps field has no mark of its own: {:?}", marks.get(EXTRA_MARK));
        assert_eq!(marks.len(), RUN_MARKS.len());
        assert_eq!(char_props(&marks, &Locals::default()), props);
    }

    #[test]
    fn runs_count_bytes_and_marks_count_characters() {
        let mut p = Paragraph::new();
        p.text = "héllo wörld".into();
        let bold = CharProps { bold: Some(true), ..Default::default() };
        p.runs = vec![Run { len: "héllo".len(), props: bold.clone() }, Run { len: " wörld".len(), props: CharProps::default() }];
        let runs = para_runs(&p, &[], &Objects::default()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].insert, "héllo");
        assert_eq!(runs[0].attributes.get("bold"), Some(&Value::Bool(true)));
        assert_eq!(runs[1].insert, " wörld");
        let (text, back, objects) = para_parts(&runs, &Locals::default(), &mut |_| None);
        assert_eq!(text, p.text);
        assert_eq!(back, p.runs);
        assert!(objects.is_empty());
    }

    #[test]
    fn anchors_carry_their_object_and_orphans_drop() {
        let mut p = Paragraph::new();
        p.text = format!("a{OBJ}b{OBJ}");
        p.runs = vec![Run { len: p.text.len(), props: CharProps::default() }];
        let ids = [ObjectId::from_string("i1"), ObjectId::from_string("i2")];
        let runs = para_runs(&p, &ids, &Objects::default()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(runs.iter().filter(|r| r.attributes.contains_key(OBJECT_MARK)).count(), 2);
        let (text, back, objects) = para_parts(&runs, &Locals::default(), &mut |id| (id == "i1").then_some(InlineObject::CommentEnd { id: 1 }));
        assert_eq!(text, format!("a{OBJ}b"), "the anchor without an object is dropped");
        assert_eq!(back.iter().map(|r| r.len).sum::<usize>(), text.len());
        assert_eq!(objects.len(), 1);
    }

    #[test]
    fn kept_characters_follow_the_diff() {
        let edits = diff_text("abcdef", "abXdf");
        let kept = kept_from(6, 5, &edits);
        assert_eq!(kept[0], Some(0));
        assert_eq!(kept[1], Some(1));
        assert_eq!(kept[2], None);
        assert_eq!(kept[3], Some(3));
        assert_eq!(kept[4], Some(5));
    }
}
