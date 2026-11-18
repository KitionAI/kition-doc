//! Format fidelity scorecard (roadmap phase 7).
//!
//! `KITION_DOC_CORPUS=<dir>`: every Word package in that directory is read, written and read
//! again, and the test reports
//!
//! - per feature class (text, run and paragraph properties, tables, pictures, …), which files keep
//!   it through import → export → import (the two documents' leaves of that class are compared as
//!   multisets of `path=value`, so a moved paragraph is not a loss but a dropped property is), and
//! - what the written package lost against the original package's XML: elements and attributes
//!   counted per part kind (part names with digits folded), relationship types, and binary parts.
//!
//! `KITION_FIDELITY_OUT=<dir>` writes the full report (`word-fidelity.txt`, per file and
//! aggregated) and the aggregate scorecard (`word-fidelity.json`, no file names) there.
//! `KITION_FIDELITY_BASELINE=<file>` fails when the corpus scores below such a scorecard: fewer
//! files keeping a class, fewer leaves kept, or less XML retained (the non-regression gate).
//! Skipped when the corpus variable is unset. The KDM leg of the product's path (import into a
//! replica, read back) is covered by `kdm-model`'s `tests/roundtrip.rs`.
//!
//! The scorer below the kit-specific part is the same in Kition Doc, Slide and Sheet, except that
//! `features` first passes each part through the kit's [`canonical`] spelling (equivalent
//! rewrites a reader cannot tell apart are not counted as losses).

use std::collections::BTreeMap;
use std::io::Read;

use serde_json::{Value, json};

use kition_doc_doc::Document;

// ---- Kit-specific -------------------------------------------------------------------------------

const KIT: &str = "word";
const CORPUS_VAR: &str = "KITION_DOC_CORPUS";
const EXTENSIONS: &[&str] = &["docx", "docm", "dotx", "dotm"];
/// A class the self-test's document must have.
const PRIMARY_CLASS: &str = "text";

type Model = Document;

fn import(bytes: &[u8]) -> Result<Model, String> {
    kition_doc_docx::read(bytes).map_err(|error| error.to_string())
}

fn export(doc: &Model) -> Result<Vec<u8>, String> {
    kition_doc_docx::write(doc).map_err(|error| error.to_string())
}

/// The model as JSON plus what serde skips (media and verbatim parts, by content). A reference to
/// a media item names its content, since the writer may rename media parts.
fn model_value(doc: &Model) -> Value {
    fn rename(value: &mut Value, names: &BTreeMap<String, String>) {
        match value {
            Value::String(s) => {
                if let Some(content) = names.get(s.as_str()) {
                    *s = content.clone();
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|v| rename(v, names)),
            Value::Object(map) => map.values_mut().for_each(|v| rename(v, names)),
            _ => {}
        }
    }
    let mut value = serde_json::to_value(doc).unwrap_or(Value::Null);
    let names: BTreeMap<String, String> = doc.media.iter().map(|(key, bytes)| (key.clone(), format!("media:{}", digest(bytes)))).collect();
    rename(&mut value, &names);
    renumber_parts(&mut value);
    if let Value::Object(map) = &mut value {
        let mut media: Vec<Value> = doc.media.values().map(|bytes| json!(digest(bytes))).collect();
        media.sort_by_key(|v| v.to_string());
        map.insert("media".into(), Value::Array(media));
        let passthrough: Vec<Value> = doc.passthrough.iter().map(|(name, bytes)| json!(format!("{}:{}", fold_digits(name), digest(bytes)))).collect();
        map.insert("passthrough".into(), Value::Array(passthrough));
    }
    value
}

/// A story part's number is a handle (`Document::parts` key): the parts are numbered again in the
/// order their first reference is met (the body and sections first, then each part's own
/// references in that order, unreferenced parts last), and every reference with them: notes'
/// `id`, text boxes' `story`, sections' header and footer sets, comments' `part`. A file whose
/// notes are numbered out of reference order (or with gaps) reads back the same.
fn renumber_parts(value: &mut Value) {
    /// Calls `f` with each part reference in `v`, in document order.
    fn refs(v: &mut Value, key: &str, f: &mut dyn FnMut(&mut Value)) {
        match v {
            Value::Object(map) => {
                let note = map.get("type").and_then(Value::as_str) == Some("noteRef");
                let set = matches!(key, "headers" | "footers");
                for (k, item) in map.iter_mut() {
                    let reference = (note && k == "id") || k == "story" || (set && matches!(k.as_str(), "default" | "first" | "even"));
                    if reference && item.is_u64() {
                        f(item);
                    } else if k != "parts" {
                        refs(item, k, f);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| refs(item, key, f)),
            _ => {}
        }
    }
    let Some(Value::Object(mut parts)) = value.as_object_mut().and_then(|m| m.remove("parts")) else { return };
    let mut order: Vec<u64> = Vec::new();
    let note = |v: &mut Value, order: &mut Vec<u64>| {
        if let Some(n) = v.as_u64().filter(|n| !order.contains(n)) {
            order.push(n);
        }
    };
    refs(value, "", &mut |v| note(v, &mut order));
    // Comments name their story parts.
    if let Some(Value::Object(comments)) = value.get_mut("comments") {
        for c in comments.values_mut() {
            if let Some(p) = c.get_mut("part") {
                note(p, &mut order);
            }
        }
    }
    let mut i = 0;
    while let Some(id) = order.get(i).copied() {
        i += 1;
        if let Some(part) = parts.get_mut(&id.to_string()) {
            refs(part, "", &mut |v| note(v, &mut order));
        }
    }
    for key in parts.keys() {
        if let Ok(n) = key.parse::<u64>()
            && !order.contains(&n)
        {
            order.push(n);
        }
    }
    let rank = |v: &mut Value| {
        if let Some(i) = v.as_u64().and_then(|n| order.iter().position(|o| *o == n)) {
            *v = json!(i + 1);
        }
    };
    refs(value, "", &mut |v| rank(v));
    if let Some(Value::Object(comments)) = value.get_mut("comments") {
        for c in comments.values_mut() {
            if let Some(p) = c.get_mut("part") {
                rank(p);
            }
        }
    }
    let mut renumbered = serde_json::Map::new();
    for (key, mut part) in std::mem::take(&mut parts) {
        refs(&mut part, "", &mut |v| rank(v));
        let at = key.parse::<u64>().ok().and_then(|n| order.iter().position(|o| *o == n)).map_or(key, |i| (i + 1).to_string());
        renumbered.insert(at, part);
    }
    if let Some(map) = value.as_object_mut() {
        map.insert("parts".into(), Value::Object(renumbered));
    }
}

/// Keys whose values are ids or caches a re-import may renumber without losing anything.
fn ignored(key: &str) -> bool {
    matches!(key, "rev" | "rsid" | "paraId" | "textId")
}

/// Whether two numbers of `class` are the same after a round trip: unit conversions (twips, EMU,
/// points through `f32`) may move a length by a hundredth.
fn close(_class: &str, a: f64, b: f64) -> bool {
    (a - b).abs() <= 0.01_f64.max(a.abs() * 1e-4)
}

/// The feature class of a leaf, by its path (`.key` segments, `[]` for array items, `#` for
/// numeric map keys).
fn classify(path: &str) -> &'static str {
    const RULES: &[(&str, &str)] = &[
        (".media", "pictures"),
        (".passthrough", "verbatim parts"),
        (".styles", "styles"),
        (".numbering", "numbering"),
        (".comments", "comments"),
        (".revisions", "revisions"),
        (".settings", "settings"),
        (".core", "properties"),
        (".customProps", "properties"),
        (".sources", "citations"),
        (".lastSection", "sections"),
        ("sectPr", "sections"),
        ("section", "sections"),
        ("chart", "charts"),
        ("math", "equations"),
        ("shape", "shapes"),
        ("diagram", "shapes"),
        ("image", "pictures"),
        ("picture", "pictures"),
        ("objects", "inline objects"),
        ("rows", "tables"),
        ("cells", "tables"),
        ("table", "tables"),
        ("runs", "run props"),
        ("chars", "run props"),
        (".text", "text"),
        ("props", "paragraph props"),
        (".parts", "headers, footers, notes"),
    ];
    let lower = path.to_ascii_lowercase();
    for (needle, class) in RULES {
        if lower.contains(&needle.to_ascii_lowercase()) {
            return class;
        }
    }
    "other"
}

/// The package part `name`'s XML with Word's equivalent spellings made one, so the package
/// comparison counts what a reader would see, not how it was spelled. Applied to the original and
/// the written package alike; `None` leaves the part as it is. The rules (each equivalent by
/// ECMA-376; tested by `canonical_rules_are_equivalences`):
///
/// - `<w:r/>`, `<w:rPr/>` and `<w:pPr/>` with no attributes and no content say nothing: dropped;
/// - adjacent runs holding only text (`w:t`) with identical run properties are one run: counted
///   as one (the writer joins runs the model holds as one);
/// - attributes holding their schema default: `a:xfrm@rot="0"` (no rotation) and
///   `w:u@w:color="auto"` (the underline takes the text's colour): dropped.
fn canonical(name: &str, xml: &[u8]) -> Option<Vec<u8>> {
    use quick_xml::events::{BytesStart, Event};

    enum Node {
        El(BytesStart<'static>, Vec<Node>),
        Other(Event<'static>),
    }
    fn local(e: &BytesStart) -> String {
        String::from_utf8_lossy(e.name().as_ref()).into_owned()
    }
    fn attrs(e: &BytesStart) -> Vec<(String, String)> {
        e.attributes()
            .flatten()
            .map(|a| (String::from_utf8_lossy(a.key.as_ref()).into_owned(), String::from_utf8_lossy(&a.value).into_owned()))
            .collect()
    }
    fn key(n: &Node, out: &mut String) {
        match n {
            Node::El(e, kids) => {
                let mut a = attrs(e);
                a.sort();
                out.push_str(&format!("<{}{a:?}>", local(e)));
                kids.iter().for_each(|k| key(k, out));
                out.push_str("</>");
            }
            Node::Other(Event::Text(t)) => out.push_str(&String::from_utf8_lossy(t.as_ref())),
            Node::Other(_) => {}
        }
    }
    fn blank(n: &Node) -> bool {
        matches!(n, Node::Other(Event::Text(t)) if t.iter().all(u8::is_ascii_whitespace))
    }
    /// A run holding only text: its run properties' key.
    fn text_run(n: &Node) -> Option<String> {
        let Node::El(e, kids) = n else { return None };
        if local(e) != "w:r" || e.attributes().next().is_some() {
            return None;
        }
        let mut props = String::new();
        let mut text = false;
        for k in kids.iter().filter(|k| !blank(k)) {
            match k {
                Node::El(c, _) if local(c) == "w:rPr" && props.is_empty() => key(k, &mut props),
                Node::El(c, _) if local(c) == "w:t" => text = true,
                _ => return None,
            }
        }
        text.then_some(props)
    }
    fn fix(n: &mut Node) {
        let Node::El(e, kids) = n else { return };
        let name = local(e);
        let defaults: &[(&str, &str)] = match name.as_str() {
            "a:xfrm" => &[("rot", "0")],
            "w:u" => &[("w:color", "auto")],
            _ => &[],
        };
        let default = |(k, v): &(String, String)| defaults.iter().any(|(dk, dv)| k == dk && v == dv);
        if attrs(e).iter().any(default) {
            let mut new = BytesStart::new(name.clone());
            for (k, v) in attrs(e).iter().filter(|a| !default(a)) {
                new.push_attribute((k.as_str(), v.as_str()));
            }
            *e = new;
        }
        kids.iter_mut().for_each(fix);
        kids.retain(|k| match k {
            Node::El(c, inner) => {
                !(matches!(local(c).as_str(), "w:r" | "w:rPr" | "w:pPr") && c.attributes().next().is_none() && inner.iter().all(blank))
            }
            _ => true,
        });
        let mut last: Option<String> = None;
        kids.retain(|k| {
            if blank(k) {
                return true;
            }
            let run = text_run(k);
            let joined = run.is_some() && run == last;
            last = run;
            !joined
        });
    }
    fn write(n: &Node, w: &mut quick_xml::Writer<Vec<u8>>) -> Option<()> {
        match n {
            Node::El(e, kids) if kids.is_empty() => w.write_event(Event::Empty(e.borrow())).ok(),
            Node::El(e, kids) => {
                w.write_event(Event::Start(e.borrow())).ok()?;
                kids.iter().try_for_each(|k| write(k, w))?;
                w.write_event(Event::End(e.to_end())).ok()
            }
            Node::Other(ev) => w.write_event(ev.borrow()).ok(),
        }
    }

    let lower = name.to_ascii_lowercase();
    if !lower.starts_with("word/") || !lower.ends_with(".xml") {
        return None;
    }
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut stack: Vec<(BytesStart<'static>, Vec<Node>)> = Vec::new();
    let mut top: Vec<Node> = Vec::new();
    let mut buf = Vec::new();
    loop {
        let node = match reader.read_event_into(&mut buf).ok()?.into_owned() {
            Event::Eof => break,
            Event::Start(e) => {
                stack.push((e, Vec::new()));
                buf.clear();
                continue;
            }
            Event::End(_) => {
                let (e, kids) = stack.pop()?;
                Node::El(e, kids)
            }
            Event::Empty(e) => Node::El(e, Vec::new()),
            other => Node::Other(other),
        };
        match stack.last_mut() {
            Some((_, kids)) => kids.push(node),
            None => top.push(node),
        }
        buf.clear();
    }
    top.iter_mut().for_each(fix);
    let mut w = quick_xml::Writer::new(Vec::new());
    top.iter().try_for_each(|n| write(n, &mut w))?;
    Some(w.into_inner())
}

/// A small document exercising most classes, for the self-test.
fn synthetic() -> Model {
    use kition_doc_doc::props::CharProps;
    use kition_doc_doc::{Paragraph, para_block};
    let mut doc = Document::new();
    let mut p = Paragraph::new();
    let _ = p.insert_text(0, "Bold ", &CharProps { bold: Some(true), ..Default::default() });
    let _ = p.insert_text(5, "plain", &CharProps::default());
    doc.body = vec![para_block(p), para_block(Paragraph::with_text("Second", CharProps::default()))];
    doc
}

// ---- Shared scorer (identical in kition-doc, kition-slide, kition-sheet) ------------------------

/// Leaves of a model by class: `path=value` → count.
type Leaves = BTreeMap<&'static str, BTreeMap<String, u64>>;
/// Features of a package by part kind: feature → count.
type Features = BTreeMap<String, BTreeMap<String, u64>>;

fn digest(bytes: &[u8]) -> String {
    // FNV-1a: enough to tell two media items apart in a report.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:016x}/{}", hash, bytes.len())
}

fn fold_digits(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_digits = false;
    for c in name.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            out.push(c);
            in_digits = false;
        }
    }
    out
}

fn leaves(value: &Value) -> Leaves {
    fn walk(value: &Value, path: &mut String, out: &mut Leaves) {
        match value {
            Value::Null => {}
            Value::Object(map) => {
                for (key, item) in map {
                    if ignored(key) {
                        continue;
                    }
                    let len = path.len();
                    path.push('.');
                    path.push_str(if key.chars().all(|c| c.is_ascii_digit() || c == '-') { "#" } else { key });
                    walk(item, path, out);
                    path.truncate(len);
                }
            }
            Value::Array(items) if items.iter().any(|v| v.is_object() || v.is_array()) => {
                let len = path.len();
                path.push_str("[]");
                for item in items {
                    walk(item, path, out);
                }
                path.truncate(len);
            }
            scalar => {
                *out.entry(classify(path)).or_default().entry(format!("{path}={scalar}")).or_default() += 1;
            }
        }
    }
    let mut out = Leaves::new();
    walk(value, &mut String::new(), &mut out);
    out
}

fn features(package: &[u8]) -> Result<Features, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(package)).map_err(|e| e.to_string())?;
    let mut out = Features::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let kind = fold_digits(&name);
        *out.entry("(parts)".into()).or_default().entry(kind.clone()).or_default() += 1;
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with(".xml") || lower.ends_with(".rels") || lower.ends_with(".vml")) {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if let Some(canonical) = canonical(&name, &bytes) {
            bytes = canonical;
        }
        let counts = out.entry(kind).or_default();
        let mut reader = quick_xml::Reader::from_reader(bytes.as_slice());
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e)) => {
                    let element = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                        if key == "xmlns" || key.starts_with("xmlns:") {
                            continue;
                        }
                        if element == "Relationship" && key == "Type" {
                            let value = String::from_utf8_lossy(&attr.value).into_owned();
                            let tail = value.rsplit('/').next().unwrap_or_default().to_owned();
                            *counts.entry(format!("rel:{tail}")).or_default() += 1;
                        }
                        *counts.entry(format!("{element}@{key}")).or_default() += 1;
                    }
                    *counts.entry(element).or_default() += 1;
                }
                Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                Ok(_) => {}
            }
            buf.clear();
        }
    }
    Ok(out)
}

#[derive(Default)]
struct ClassScore {
    files: u64,
    files_kept: u64,
    leaves: u64,
    kept: u64,
    added: u64,
}

#[derive(Default)]
struct Scorecard {
    files: u64,
    unreadable: u64,
    unwritable: u64,
    unrereadable: u64,
    classes: BTreeMap<&'static str, ClassScore>,
    /// (class, path with digits folded) → (files, leaves lost, an example: lost leaf → a leaf the
    /// second import has instead on that path, if any)
    lost_leaves: BTreeMap<(&'static str, String), (u64, u64, String)>,
    /// (class, path with digits folded) → (files, leaves added, an example)
    added_leaves: BTreeMap<(&'static str, String), (u64, u64, String)>,
    xml_original: u64,
    xml_kept: u64,
    /// (part kind, feature) → (files, count lost)
    lost_features: BTreeMap<(String, String), (u64, u64)>,
    part_kinds: BTreeMap<String, (u64, u64)>,
    lines: Vec<String>,
}

/// The number a leaf holds, if it holds one.
fn number_of(leaf: &str) -> Option<f64> {
    leaf.split_once('=').and_then(|(_, v)| if v.starts_with('"') { None } else { v.parse().ok() })
}

/// Folded path → (count, an example).
type Paths = BTreeMap<String, (u64, String)>;

/// How many of `left`'s leaves `right` keeps (equal, or a number [`close`] to a spare one on the
/// same path), how many it adds, what it lost and what it added.
fn compare(class: &str, left: &BTreeMap<String, u64>, right: &BTreeMap<String, u64>) -> (u64, u64, Paths, Paths) {
    let mut spare: BTreeMap<String, Vec<(f64, &str)>> = BTreeMap::new();
    let mut added = 0;
    for (leaf, m) in right {
        let extra = m.saturating_sub(left.get(leaf).copied().unwrap_or(0));
        added += extra;
        if let Some(v) = number_of(leaf) {
            for _ in 0..extra {
                spare.entry(path_of(leaf)).or_default().push((v, leaf));
            }
        }
    }
    let mut kept = 0;
    let mut lost = Paths::new();
    for (leaf, n) in left {
        let m = right.get(leaf).copied().unwrap_or(0);
        kept += m.min(*n);
        let path = path_of(leaf);
        for _ in m..*n {
            if let (Some(v), Some(list)) = (number_of(leaf), spare.get_mut(&path))
                && let Some(i) = list.iter().position(|(w, _)| close(class, v, *w))
            {
                list.swap_remove(i);
                kept += 1;
                added -= 1;
                continue;
            }
            let entry = lost.entry(fold_digits(&path)).or_default();
            entry.0 += 1;
            if entry.1.is_empty() {
                let instead = right.keys().find(|r| path_of(r) == path && !left.contains_key(*r)).map(String::as_str).unwrap_or("nothing");
                entry.1 = format!("{} → {}", clip(leaf), clip(instead));
            }
        }
    }
    let mut gained = Paths::new();
    let mut note = |leaf: &str, n: u64| {
        let entry = gained.entry(fold_digits(&path_of(leaf))).or_default();
        entry.0 += n;
        if entry.1.is_empty() {
            entry.1 = clip(leaf);
        }
    };
    for (leaf, m) in right {
        let extra = m.saturating_sub(left.get(leaf).copied().unwrap_or(0));
        if extra > 0 && number_of(leaf).is_none() {
            note(leaf, extra);
        }
    }
    for (_, leaf) in spare.values().flatten() {
        note(leaf, 1);
    }
    (kept, added, lost, gained)
}

/// `leaf`, cut to a readable length.
fn clip(leaf: &str) -> String {
    let mut out: String = leaf.chars().take(160).collect();
    if out.len() < leaf.len() {
        out.push('…');
    }
    out
}

fn path_of(leaf: &str) -> String {
    leaf.split_once('=').map(|(p, _)| p.to_owned()).unwrap_or_default()
}

impl Scorecard {
    fn file(&mut self, name: &str, bytes: &[u8]) {
        self.files += 1;
        let Ok(first) = import(bytes) else {
            self.unreadable += 1;
            self.lines.push(format!("{name}: not importable"));
            return;
        };
        let Ok(written) = export(&first) else {
            self.unwritable += 1;
            self.lines.push(format!("{name}: export failed"));
            return;
        };
        let Ok(second) = import(&written) else {
            self.unrereadable += 1;
            self.lines.push(format!("{name}: export not re-importable"));
            return;
        };
        let (a, b) = (leaves(&model_value(&first)), leaves(&model_value(&second)));
        let mut lost_classes = Vec::new();
        let empty = BTreeMap::new();
        for (class, left) in &a {
            let right = b.get(class).unwrap_or(&empty);
            let score = self.classes.entry(class).or_default();
            let total: u64 = left.values().sum();
            let (kept, added, lost_paths, added_paths) = compare(class, left, right);
            score.files += 1;
            score.leaves += total;
            score.kept += kept;
            score.added += added;
            if kept == total && added == 0 {
                score.files_kept += 1;
            } else {
                lost_classes.push(format!("{class} {kept}/{total}+{added}"));
            }
            for (paths, into) in [(lost_paths, &mut self.lost_leaves), (added_paths, &mut self.added_leaves)] {
                for (path, (n, example)) in paths {
                    let entry = into.entry((class, path)).or_default();
                    entry.0 += 1;
                    entry.1 += n;
                    if entry.2.is_empty() {
                        entry.2 = example;
                    }
                }
            }
        }
        for (class, right) in &b {
            if !a.contains_key(class) {
                let score = self.classes.entry(class).or_default();
                score.files += 1;
                score.added += right.values().sum::<u64>();
                lost_classes.push(format!("{class} 0/0+{}", right.values().sum::<u64>()));
            }
        }
        let (Ok(original), Ok(exported)) = (features(bytes), features(&written)) else {
            self.lines.push(format!("{name}: classes changed: {lost_classes:?}; package not comparable"));
            return;
        };
        let (mut total, mut kept) = (0u64, 0u64);
        for (kind, counts) in &original {
            let theirs = exported.get(kind);
            let kind_total: u64 = counts.values().sum();
            let mut kind_kept = 0;
            for (feature, n) in counts {
                let m = theirs.and_then(|t| t.get(feature)).copied().unwrap_or(0);
                kind_kept += m.min(*n);
                if m < *n {
                    let entry = self.lost_features.entry((kind.clone(), feature.clone())).or_default();
                    entry.0 += 1;
                    entry.1 += n - m;
                }
            }
            let entry = self.part_kinds.entry(kind.clone()).or_default();
            entry.0 += kind_total;
            entry.1 += kind_kept;
            total += kind_total;
            kept += kind_kept;
        }
        self.xml_original += total;
        self.xml_kept += kept;
        self.lines.push(format!("{name}: xml {:.1}%; classes changed: {lost_classes:?}", percent(kept, total)));
    }

    fn json(&self) -> Value {
        let classes: serde_json::Map<String, Value> = self
            .classes
            .iter()
            .map(|(class, s)| {
                ((*class).to_owned(), json!({ "files": s.files, "filesKept": s.files_kept, "leaves": s.leaves, "kept": s.kept, "added": s.added }))
            })
            .collect();
        json!({
            "kit": KIT,
            "files": self.files,
            "unreadable": self.unreadable,
            "unwritable": self.unwritable,
            "unrereadable": self.unrereadable,
            "classes": classes,
            "xml": { "original": self.xml_original, "kept": self.xml_kept },
        })
    }

    fn report(&self) -> String {
        let mut out = String::new();
        let ok = self.files - self.unreadable - self.unwritable - self.unrereadable;
        out.push_str(&format!(
            "{KIT} fidelity: {} files, {ok} round-tripped ({} unreadable, {} export failed, {} export unreadable)\n\n",
            self.files, self.unreadable, self.unwritable, self.unrereadable
        ));
        out.push_str("class                        files kept   leaves kept      added\n");
        for (class, s) in &self.classes {
            out.push_str(&format!(
                "{class:<28} {:>4}/{:<4}  {:>6.2}% of {:<9} {}\n",
                s.files_kept,
                s.files,
                percent(s.kept, s.leaves),
                s.leaves,
                s.added
            ));
        }
        out.push_str(&format!(
            "\nXML retained: {:.2}% of {} elements and attributes\n",
            percent(self.xml_kept, self.xml_original),
            self.xml_original
        ));
        let mut kinds: Vec<_> = self.part_kinds.iter().collect();
        kinds.sort_by_key(|(_, (total, kept))| std::cmp::Reverse(total - kept));
        out.push_str("\npart kinds by features lost:\n");
        for (kind, (total, kept)) in kinds.iter().take(25) {
            out.push_str(&format!("  {kind:<50} {:>9} lost of {:<9} ({:.1}% kept)\n", total - kept, total, percent(*kept, *total)));
        }
        let mut lost: Vec<_> = self.lost_features.iter().collect();
        lost.sort_by_key(|(_, (files, count))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop lost XML features by files (files, count):\n");
        for ((kind, feature), (files, count)) in lost.iter().take(150) {
            out.push_str(&format!("  {files:>3} {count:>8}  {kind} {feature}\n"));
        }
        lost.sort_by_key(|(_, (files, count))| std::cmp::Reverse((*count, *files)));
        out.push_str("\ntop lost XML features by count (files, count):\n");
        for ((kind, feature), (files, count)) in lost.iter().take(150) {
            out.push_str(&format!("  {files:>3} {count:>8}  {kind} {feature}\n"));
        }
        let mut lost: Vec<_> = self.lost_leaves.iter().collect();
        lost.sort_by_key(|(_, (files, count, _))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop lost model leaves (files, count, example):\n");
        for ((class, path), (files, count, example)) in lost.iter().take(300) {
            out.push_str(&format!("  {files:>3} {count:>8}  [{class}] {path}\n               e.g. {example}\n"));
        }
        let mut added: Vec<_> = self.added_leaves.iter().collect();
        added.sort_by_key(|(_, (files, count, _))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop added model leaves (files, count, example):\n");
        for ((class, path), (files, count, example)) in added.iter().take(100) {
            out.push_str(&format!("  {files:>3} {count:>8}  [{class}] {path}\n               e.g. {example}\n"));
        }
        out.push_str("\nper file:\n");
        for line in &self.lines {
            out.push_str(&format!("  {line}\n"));
        }
        out
    }
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 { 100.0 } else { part as f64 * 100.0 / whole as f64 }
}

/// The regressions of `now` against `baseline`, both [`Scorecard::json`] values.
fn regressions(now: &Value, baseline: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let number = |v: &Value, path: &[&str]| path.iter().fold(v, |v, key| &v[*key]).as_u64().unwrap_or(0);
    for key in ["unreadable", "unwritable", "unrereadable"] {
        if number(now, &[key]) > number(baseline, &[key]) {
            out.push(format!("{key}: {} > {}", number(now, &[key]), number(baseline, &[key])));
        }
    }
    let ratio = |kept: u64, total: u64| if total == 0 { 1.0 } else { kept as f64 / total as f64 };
    if let Some(classes) = baseline["classes"].as_object() {
        for class in classes.keys() {
            let (b, n) = (&baseline["classes"][class], &now["classes"][class]);
            if number(n, &["filesKept"]) < number(b, &["filesKept"]) {
                out.push(format!("{class}: {} files keep it, baseline {}", number(n, &["filesKept"]), number(b, &["filesKept"])));
            }
            // More leaves lost than the class grew by: a model that learns a property may also
            // count it on objects that were lost already, which is not a regression.
            let lost = |v: &Value| number(v, &["leaves"]).saturating_sub(number(v, &["kept"]));
            let grown = number(n, &["leaves"]).saturating_sub(number(b, &["leaves"]));
            if lost(n) > lost(b) + grown {
                out.push(format!("{class}: {} leaves lost, baseline {} (+{grown} leaves)", lost(n), lost(b)));
            }
        }
    }
    let (rb, rn) = (
        ratio(number(baseline, &["xml", "kept"]), number(baseline, &["xml", "original"])),
        ratio(number(now, &["xml", "kept"]), number(now, &["xml", "original"])),
    );
    if rn + 1e-9 < rb {
        out.push(format!("XML retained {:.4}%, baseline {:.4}%", rn * 100.0, rb * 100.0));
    }
    out
}

#[test]
fn corpus_fidelity() {
    let Ok(dir) = std::env::var(CORPUS_VAR) else { return };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e))))
        .collect();
    files.sort();
    let mut card = Scorecard::default();
    for path in &files {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{name}: {error}"));
        card.file(&name, &bytes);
    }
    let report = card.report();
    let summary: String = report.split("\npart kinds").next().unwrap_or_default().to_owned();
    eprintln!("{summary}");
    let now = card.json();
    if let Ok(out) = std::env::var("KITION_FIDELITY_OUT") {
        let _ = std::fs::create_dir_all(&out);
        std::fs::write(format!("{out}/{KIT}-fidelity.txt"), &report).unwrap_or_else(|error| panic!("{out}: {error}"));
        let pretty = serde_json::to_string_pretty(&now).unwrap_or_default();
        std::fs::write(format!("{out}/{KIT}-fidelity.json"), pretty).unwrap_or_else(|error| panic!("{out}: {error}"));
    }
    if let Ok(path) = std::env::var("KITION_FIDELITY_BASELINE") {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
        let baseline: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"));
        let regressed = regressions(&now, &baseline);
        assert!(regressed.is_empty(), "fidelity regressed against {path}:\n{}", regressed.join("\n"));
    }
}

#[test]
fn scorer_passes_a_lossless_round_trip_and_flags_a_regression() {
    let doc = synthetic();
    let bytes = export(&doc).unwrap_or_else(|error| panic!("export: {error}"));
    let mut card = Scorecard::default();
    card.file("synthetic", &bytes);
    assert_eq!(card.unreadable + card.unwritable + card.unrereadable, 0, "{}", card.report());
    assert!(card.classes.values().all(|s| s.files_kept == s.files), "{}", card.report());
    assert!(card.classes.contains_key(PRIMARY_CLASS), "{}", card.report());
    assert!(card.xml_original > 0 && card.xml_kept == card.xml_original, "{}", card.report());
    // A baseline of this run accepts it; one that kept more flags it.
    let now = card.json();
    assert!(regressions(&now, &now).is_empty());
    let mut better = now.clone();
    better["xml"]["kept"] = json!(card.xml_kept + 1);
    better["xml"]["original"] = json!(card.xml_original);
    assert_eq!(regressions(&now, &better).len(), 1);
}

/// Each [`canonical`] rule makes one of two spellings that Word reads alike: the reader reads
/// both into the same model, and the package comparison counts them alike.
#[test]
fn canonical_rules_are_equivalences() {
    const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
    fn package(body: &str) -> Vec<u8> {
        use std::io::Write;
        let doc = format!(r#"<?xml version="1.0" encoding="UTF-8"?><w:document {NS}><w:body>{body}</w:body></w:document>"#);
        let root = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in [("_rels/.rels", root), ("word/document.xml", doc.as_str())] {
            zw.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zw.write_all(bytes.as_bytes()).unwrap();
        }
        zw.finish().unwrap().into_inner()
    }
    let same = |a: &str, b: &str| {
        let (pa, pb) = (package(a), package(b));
        let (ma, mb) = (import(&pa).unwrap(), import(&pb).unwrap());
        assert_eq!(model_value(&ma), model_value(&mb), "{a}\n{b}");
        assert_eq!(features(&pa).unwrap(), features(&pb).unwrap(), "{a}\n{b}");
    };
    let bold = "<w:rPr><w:b/></w:rPr>";
    // Empty runs and property elements.
    same("<w:p><w:pPr/><w:r/><w:r><w:rPr/><w:t>a</w:t></w:r></w:p>", "<w:p><w:r><w:t>a</w:t></w:r></w:p>");
    // Adjacent text runs with the same properties.
    same(&format!("<w:p><w:r>{bold}<w:t>ab</w:t></w:r></w:p>"), &format!("<w:p><w:r>{bold}<w:t>a</w:t></w:r><w:r>{bold}<w:t>b</w:t></w:r></w:p>"));
    // A default underline colour.
    same(
        r#"<w:p><w:r><w:rPr><w:u w:val="single" w:color="auto"/></w:rPr><w:t>a</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>a</w:t></w:r></w:p>"#,
    );
    // Runs with different properties, a run holding more than text, and a non-default colour stay.
    let differ = |a: &str, b: &str| assert_ne!(features(&package(a)).unwrap(), features(&package(b)).unwrap(), "{a}\n{b}");
    differ(&format!("<w:p><w:r>{bold}<w:t>ab</w:t></w:r></w:p>"), &format!("<w:p><w:r>{bold}<w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>"));
    differ("<w:p><w:r><w:t>a</w:t><w:tab/></w:r></w:p>", "<w:p><w:r><w:t>a</w:t></w:r><w:r><w:tab/></w:r></w:p>");
    differ(
        r#"<w:p><w:r><w:rPr><w:u w:val="single" w:color="FF0000"/></w:rPr><w:t>a</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>a</w:t></w:r></w:p>"#,
    );
    // A rotation of zero is no rotation (DrawingML markup, counted alike).
    let xfrm = |rot: &str| canonical("word/document.xml", format!(r#"<a:xfrm xmlns:a="a"{rot}><a:off x="0" y="0"/></a:xfrm>"#).as_bytes());
    assert_eq!(xfrm(r#" rot="0""#), xfrm(""));
    assert_ne!(xfrm(r#" rot="60000""#), xfrm(""));
}

/// Part numbers are handles: two documents that differ only in how their notes are numbered
/// score alike, and a note whose text differs does not.
#[test]
fn part_numbers_are_handles() {
    use kition_doc_doc::para::NoteKind;
    use kition_doc_doc::props::CharProps;
    use kition_doc_doc::{InlineObject, Paragraph, PartKind, para_block};
    let doc = |first: u32, second: u32, texts: [&str; 2]| {
        let mut d = Document::new();
        let mut p = Paragraph::with_text("a", CharProps::default());
        for (at, id) in [(1, first), (2, second)] {
            let _ = p.insert_object(at, InlineObject::NoteRef { kind: NoteKind::Footnote, id, custom: String::new() }, &CharProps::default());
        }
        d.body = vec![para_block(p)];
        for (id, text) in [(first, texts[0]), (second, texts[1])] {
            d.parts.insert(
                id,
                kition_doc_doc::Part { kind: PartKind::Footnote, blocks: vec![para_block(Paragraph::with_text(text, CharProps::default()))] },
            );
        }
        leaves(&model_value(&d))
    };
    assert_eq!(doc(1, 2, ["one", "two"]), doc(7, 3, ["one", "two"]));
    assert_ne!(doc(1, 2, ["one", "two"]), doc(7, 3, ["one", "three"]));
}
