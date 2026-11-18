//! Writing back charts, smart graphics and OLE objects read from a file (see `read::embed`).
//!
//! Each written object gets its kept markup with fresh relationship ids, pointing at copies of the
//! parts it needs. The first object to use a part writes it under its original name (unless that
//! name is one this writer uses for something else: then it's renamed). An object written again
//! (copied and pasted in Kition Doc) gets its own renamed copies of every part, so the copies stay
//! independent in Word; past [`MAX_COPY_BYTES`] of such copies, later ones share the first's parts.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use kition_doc_doc::graphic::Embedded;
use kition_doc_doc::para::Float;

use super::PartRels;
use crate::package::{EMBEDDED_PARTS, EmbeddedManifest, MAX_EMBEDDED_PARTS, OWN_MEDIA, VBA_PROJECT_PART, VBA_RELATED, relative};
use crate::units::emu;
use crate::xml::{self, El, MAX_DEPTH};

/// Most bytes written for copies of parts (objects pasted more than once).
const MAX_COPY_BYTES: u64 = 256 * 1024 * 1024;
/// Most bytes of kept markup written for all objects (objects can share one kept markup).
const MAX_MARKUP_BYTES: u64 = 128 * 1024 * 1024;
/// A chart edited or made in Kition Doc, as it is written: its part, and a workbook of its data
/// when the part takes them from a new one (a chart whose data were edited, or a new chart).
#[derive(Clone, Debug)]
pub(super) struct EditedChart {
    pub xml: Arc<String>,
    pub workbook: Option<Vec<u8>>,
}

/// The relationship id an edited chart's part names its new workbook by.
pub(super) const WORKBOOK_RID: &str = "rIdKitionData";

/// A chart part's content type.
const CT_CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
/// What one part copy costs at least against [`MAX_COPY_BYTES`] (its relationships, entry…).
const PART_COST: u64 = 4096;

/// Where the stories' parts live: every story this writer makes is a part in `word/`.
const STORY_PART: &str = "word/document.xml";

/// The embedded-object parts of a package being written.
pub(crate) struct EmbedWriter<'d> {
    manifest: EmbeddedManifest,
    passthrough: &'d BTreeMap<String, Arc<Vec<u8>>>,
    /// Part names in use (lower case): this writer's own, and parts written so far.
    taken: HashSet<String>,
    /// Next suffix to try when renaming a part.
    next: HashMap<String, u32>,
    /// Original part name → name written, for parts written by an object's first copy.
    first: HashMap<String, String>,
    /// Budget left for copies.
    copy_bytes: u64,
    /// Budget left for kept markup.
    markup_bytes: u64,
    /// Parts to write: (name, bytes, content type, relationships).
    pub parts: Vec<(String, Vec<u8>, String, PartRels)>,
    /// A part written with other bytes than the kept ones (an edited chart's part), by original name.
    replace: Option<(String, EditedChart)>,
}

impl<'d> EmbedWriter<'d> {
    pub fn new(doc: &'d kition_doc_doc::Document) -> EmbedWriter<'d> {
        let manifest = doc.passthrough.get(EMBEDDED_PARTS).map(|m| EmbeddedManifest::parse(m)).unwrap_or_default();
        let mut taken: HashSet<String> = [
            "word/document.xml",
            "word/styles.xml",
            "word/numbering.xml",
            "word/settings.xml",
            "word/theme/theme1.xml",
            "word/footnotes.xml",
            "word/endnotes.xml",
            "word/comments.xml",
            "word/commentsextended.xml",
            "[content_types].xml",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        taken.insert(VBA_PROJECT_PART.to_ascii_lowercase());
        // The macro project's parts keep their names (see `write_as`).
        if let Some(m) = doc.passthrough.get(VBA_RELATED) {
            for line in String::from_utf8_lossy(m).lines() {
                if let Some(path) = line.split('\t').nth(1) {
                    taken.insert(path.to_ascii_lowercase());
                }
            }
        }
        EmbedWriter {
            manifest,
            passthrough: &doc.passthrough,
            taken,
            next: HashMap::new(),
            first: HashMap::new(),
            copy_bytes: MAX_COPY_BYTES,
            markup_bytes: MAX_MARKUP_BYTES,
            parts: Vec::new(),
            replace: None,
        }
    }

    /// Reserve the names of the media files this writer writes.
    pub fn reserve_media<'a>(&mut self, files: impl Iterator<Item = &'a String>) {
        for f in files {
            self.taken.insert(format!("word/media/{f}").to_ascii_lowercase());
        }
    }

    /// Names this writer may give its own parts later (headers, footers, package parts).
    fn reserved(lower: &str) -> bool {
        lower.ends_with(".rels")
            || lower.starts_with("docprops/")
            || lower.starts_with("_rels/")
            || ((lower.starts_with("word/header") || lower.starts_with("word/footer")) && lower.ends_with(".xml"))
    }

    /// A free part name for (a copy of) `orig`: `orig` itself if free, else `stem_N.ext`.
    fn alloc(&mut self, orig: &str) -> String {
        let lower = orig.to_ascii_lowercase();
        if !Self::reserved(&lower) && self.taken.insert(lower) {
            return orig.to_string();
        }
        let file_at = orig.rfind('/').map_or(0, |i| i + 1);
        let (stem, ext) = match orig.rfind('.').filter(|i| *i > file_at) {
            Some(i) => (orig.get(..i).unwrap_or(orig), orig.get(i..).unwrap_or("")),
            None => (orig, ""),
        };
        loop {
            let n = self.next.entry(orig.to_string()).or_insert(1);
            *n = n.saturating_add(1);
            let name = format!("{stem}_{n}{ext}");
            let lower = name.to_ascii_lowercase();
            if !Self::reserved(&lower) && self.taken.insert(lower) {
                return name;
            }
        }
    }

    /// Can part `name` be written (bytes and manifest entry both kept)?
    fn available(&self, name: &str) -> bool {
        self.manifest.parts.contains_key(name) && self.passthrough.contains_key(name)
    }

    /// What copying the parts reached from `roots` costs against the copy budget.
    fn tree_cost(&self, roots: &[&str]) -> u64 {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut todo: Vec<&str> = roots.to_vec();
        let mut cost = 0u64;
        while let Some(p) = todo.pop() {
            if !seen.insert(p) || seen.len() > MAX_EMBEDDED_PARTS {
                continue;
            }
            cost = cost.saturating_add(self.passthrough.get(p).map_or(0, |b| b.len() as u64)).saturating_add(PART_COST);
            if let Some(info) = self.manifest.parts.get(p) {
                todo.extend(info.rels.iter().filter(|r| !r.external).map(|r| r.target.as_str()));
            }
        }
        cost
    }

    /// Write part `root` and the parts it reaches, naming them through `memo` (original → written
    /// name; parts already in it aren't written again). Returns `root`'s written name and adds
    /// the indices of the parts written to `written`.
    fn copy_tree(&mut self, root: &str, memo: &mut HashMap<String, String>, written: &mut Vec<usize>) -> Option<String> {
        if let Some(n) = memo.get(root) {
            return Some(n.clone());
        }
        if !self.available(root) {
            return None;
        }
        let name = self.alloc(root);
        memo.insert(root.to_string(), name.clone());
        let mut todo = vec![root.to_string()];
        while let Some(orig) = todo.pop() {
            let (Some(info), Some(bytes)) = (self.manifest.parts.get(&orig).cloned(), self.passthrough.get(&orig)) else { continue };
            let Some(new) = memo.get(&orig).cloned() else { continue };
            let mut rels = PartRels::default();
            let replaced = self.replace.as_ref().filter(|(part, _)| *part == orig).map(|(_, e)| e.clone());
            let new_workbook = replaced.as_ref().is_some_and(|e| e.workbook.is_some());
            for r in &info.rels {
                // The workbook the part read its data from gives way to the new one.
                if new_workbook && r.kind == kition_office_core::chart::workbook::PACKAGE_REL {
                    continue;
                }
                if r.external {
                    rels.add_with_id(&r.id, &r.kind, &r.target, true);
                    continue;
                }
                let target = match memo.get(&r.target) {
                    Some(t) => t.clone(),
                    None if self.available(&r.target) => {
                        let t = self.alloc(&r.target);
                        memo.insert(r.target.clone(), t.clone());
                        todo.push(r.target.clone());
                        t
                    }
                    // A part that wasn't kept: leave the relationship out.
                    None => continue,
                };
                rels.add_with_id(&r.id, &r.kind, &relative(&new, &target), false);
            }
            let ct = if info.content_type.is_empty() { "application/octet-stream".to_string() } else { info.content_type };
            let bytes = match &replaced {
                Some(edited) => {
                    if let Some(book) = &edited.workbook {
                        let name = self.alloc("word/embeddings/Microsoft_Excel_Worksheet.xlsx");
                        rels.add_with_id(WORKBOOK_RID, kition_office_core::chart::workbook::PACKAGE_REL, &relative(&new, &name), false);
                        self.parts.push((
                            name,
                            book.clone(),
                            kition_office_core::chart::workbook::XLSX_CONTENT_TYPE.to_string(),
                            PartRels::default(),
                        ));
                    }
                    edited.xml.as_bytes().to_vec()
                }
                None => bytes.to_vec(),
            };
            written.push(self.parts.len());
            self.parts.push((new, bytes, ct, rels));
        }
        Some(name)
    }
}

impl super::Writer<'_> {
    /// The markup for an object kept from a file, with its relationships added to `rels` (the
    /// story part's) and the parts it needs queued for writing. `resize`, for an OLE object, writes
    /// the object's current size and position into its own markup (a chart or diagram's frame is
    /// written by the caller). `None` when it can't be written back (its parts are gone).
    /// `own_media`: the media key of the picture the object shows, for a relationship naming
    /// [`OWN_MEDIA`]. `chart`: an edited chart's part, written in place of the kept one (always in
    /// parts of the object's own, so a copy of it that was not edited keeps the kept part).
    pub(super) fn embedded_xml(
        &mut self,
        src: &Embedded,
        rels: &mut PartRels,
        resize: Option<(f32, f32, &Float)>,
        own_media: Option<&str>,
        chart: Option<&EditedChart>,
    ) -> Option<String> {
        let Some(left) = self.embeds.markup_bytes.checked_sub(src.xml.len() as u64) else {
            log::warn!("docx: too much embedded-object markup; the rest is left out");
            return None;
        };
        self.embeds.markup_bytes = left;
        let mut root = xml::parse(src.xml.as_bytes()).ok()?;
        let mut used = Vec::new();
        rel_attr_values(&root, &mut used, 0);
        // Every relationship the markup uses must be writable, or the object would be broken.
        let own_file = own_media.and_then(|m| self.media_files.get(m).cloned());
        let writable = |id: &String| {
            src.refs.iter().any(|r| r.id == *id && (r.external || self.embeds.available(&r.target) || (r.target == OWN_MEDIA && own_file.is_some())))
        };
        if !used.iter().all(writable) {
            log::warn!("docx: an embedded object's parts are missing; it is left out");
            return None;
        }
        let roots: Vec<&str> = src.refs.iter().filter(|r| !r.external && r.target != OWN_MEDIA).map(|r| r.target.as_str()).collect();
        let edited =
            chart.and_then(|c| src.refs.iter().find(|r| !r.external && r.kind == crate::package::rt::CHART).map(|r| (r.target.clone(), c.clone())));
        // Written before (a copy of this object): give this one its own parts while the budget lasts.
        let again = roots.iter().any(|r| self.embeds.first.contains_key(*r));
        let cost = if again { self.embeds.tree_cost(&roots) } else { 0 };
        let fresh = edited.is_some() || (again && cost <= self.embeds.copy_bytes);
        if fresh && edited.is_none() {
            self.embeds.copy_bytes -= cost;
        }
        self.embeds.replace = edited;
        let mut memo = if fresh { HashMap::new() } else { std::mem::take(&mut self.embeds.first) };
        let mut written = Vec::new();
        let mut ids: HashMap<String, String> = HashMap::new();
        for r in &src.refs {
            let new_id = if r.external {
                Some(rels.add(&r.kind, &r.target, true))
            } else if r.target == OWN_MEDIA {
                own_file.as_ref().map(|file| {
                    if let Some(m) = own_media {
                        self.used_media.insert(m.to_string());
                    }
                    rels.add(&r.kind, &format!("media/{file}"), false)
                })
            } else {
                self.embeds.copy_tree(&r.target, &mut memo, &mut written).map(|name| rels.add(&r.kind, &relative(STORY_PART, &name), false))
            };
            if let Some(id) = new_id {
                ids.insert(r.id.clone(), id);
            }
        }
        self.embeds.replace = None;
        if !fresh {
            self.embeds.first = memo;
        }
        if used.iter().any(|u| !ids.contains_key(u)) {
            log::warn!("docx: an embedded object lost a part; it is left out");
            return None;
        }
        // A smart graphic's data part names its drawing's relationship (of the story part) by id.
        for i in written {
            if let Some((_, bytes, ct, _)) = self.embeds.parts.get_mut(i)
                && ct.contains("diagramData")
            {
                *bytes = patch_rel_ids(bytes, &ids);
            }
        }
        set_rel_ids(&mut root, &ids, 0);
        match resize {
            Some((w, h, float)) if root.name == "w:pict" => resize_vml(&mut root, w, h, float),
            Some((w, h, float)) if root.name == "mc:AlternateContent" => self.resize_alternate(&mut root, w, h, float),
            Some((w, h, float)) => self.resize_object(&mut root, w, h, float, 0),
            None => {}
        }
        Some(root.to_xml())
    }

    /// The `a:graphic` of a chart made in Kition Doc, with its part `chart` (a `c:chartSpace`)
    /// queued for writing and related from the story part.
    pub(super) fn new_chart_xml(&mut self, chart: &EditedChart, rels: &mut PartRels) -> String {
        let name = self.embeds.alloc("word/charts/chart1.xml");
        let mut chart_rels = PartRels::default();
        if let Some(book) = &chart.workbook {
            let wb = self.embeds.alloc("word/embeddings/Microsoft_Excel_Worksheet.xlsx");
            chart_rels.add_with_id(WORKBOOK_RID, kition_office_core::chart::workbook::PACKAGE_REL, &relative(&name, &wb), false);
            self.embeds.parts.push((wb, book.clone(), kition_office_core::chart::workbook::XLSX_CONTENT_TYPE.to_string(), PartRels::default()));
        }
        self.embeds.parts.push((name.clone(), chart.xml.as_bytes().to_vec(), CT_CHART.to_string(), chart_rels));
        let id = rels.add(crate::package::rt::CHART, &relative(STORY_PART, &name), false);
        format!(
            r#"<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="{id}"/></a:graphicData></a:graphic>"#
        )
    }

    /// Alternate content kept whole (ink shown as its VML fallback picture): written as it was
    /// while the picture keeps its size and place (with fresh drawing ids); once moved or resized,
    /// only the fallback picture at its new frame (the other choice would show it where it was).
    fn resize_alternate(&mut self, root: &mut El, w: f32, h: f32, float: &Float) {
        let fallback = root.child("mc:Fallback").and_then(|f| f.child("w:pict")).cloned();
        let moved = fallback.as_ref().and_then(crate::read::story::vml_shape).is_none_or(|shape| {
            let (rw, rh, rf) = crate::read::story::vml_frame(shape);
            (rw - w).abs() >= 0.005 || (rh - h).abs() >= 0.005 || rf != *float
        });
        match fallback {
            Some(mut pict) if moved => {
                resize_vml(&mut pict, w, h, float);
                *root = pict;
            }
            _ => self.fresh_drawing_ids(root, 0),
        }
    }

    /// Give every `wp:docPr` in `e` a fresh id.
    fn fresh_drawing_ids(&mut self, e: &mut El, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        if e.name == "wp:docPr" {
            let id = self.next_docpr();
            if let Some((_, v)) = e.attrs.iter_mut().find(|(k, _)| k == "id") {
                *v = id;
            }
        }
        for k in e.kids.iter_mut() {
            if let xml::Node::El(c) = k {
                self.fresh_drawing_ids(c, depth + 1);
            }
        }
    }

    /// Write an OLE object's size (and position, when floating), rotation and flips into its VML
    /// shape and DrawingML picture, with the effect extent covering the rotated bounds as for
    /// other drawings, and give its drawing a fresh `wp:docPr` id.
    fn resize_object(&mut self, e: &mut El, w: f32, h: f32, float: &Float, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        match e.name.as_str() {
            "v:shape" => {
                if let Some((_, style)) = e.attrs.iter_mut().find(|(k, _)| k == "style") {
                    *style = vml_style_with(style, w, h, float);
                }
            }
            "wp:extent" => set_size_attrs(e, w, h),
            // A turned object needs an effect extent (filled in below) right after its extent.
            "wp:inline" | "wp:anchor" if !e.els().any(|c| c.name == "wp:effectExtent") && float.spin().deg != 0.0 => {
                if let Some(at) = e.kids.iter().position(|k| matches!(k, xml::Node::El(c) if c.name == "wp:extent")) {
                    e.kids.insert(at + 1, xml::Node::El(El { name: "wp:effectExtent".to_string(), attrs: Vec::new(), kids: Vec::new() }));
                }
            }
            "wp:effectExtent" => {
                // Word's effect extent also covers a rotated object's overhang (its rotated bounds).
                let (px, py) = float.spin_pad(w, h);
                let [el, et, er, eb] = float.effect_extent();
                let ext = [el + px, et + py, er + px, eb + py].map(emu);
                e.attrs.retain(|(k, _)| !matches!(k.as_str(), "l" | "t" | "r" | "b"));
                for (k, v) in ["l", "t", "r", "b"].into_iter().zip(ext) {
                    e.attrs.push((k.to_string(), v));
                }
            }
            // The picture's own frame (`pic:spPr`): its size and the object's rotation and flips.
            "a:xfrm" => {
                e.attrs.retain(|(k, _)| !matches!(k.as_str(), "rot" | "flipH" | "flipV"));
                for (k, v) in super::story::spin_attrs(float.spin()) {
                    e.attrs.push((k.to_string(), v));
                }
                for k in e.kids.iter_mut() {
                    if let xml::Node::El(c) = k
                        && c.name == "a:ext"
                    {
                        set_size_attrs(c, w, h);
                    }
                }
                return;
            }
            "wp:docPr" => {
                let id = self.next_docpr();
                if let Some((_, v)) = e.attrs.iter_mut().find(|(k, _)| k == "id") {
                    *v = id;
                }
            }
            _ => {}
        }
        for k in e.kids.iter_mut() {
            if let xml::Node::El(c) = k {
                self.resize_object(c, w, h, float, depth + 1);
            }
        }
    }
}

/// A VML drawing (`w:pict`) at its object's size and placement: its shape's `style`, rewritten
/// only when they changed (shapes inside it, such as a text box's, stay as they are).
fn resize_vml(pict: &mut El, w: f32, h: f32, float: &Float) {
    let Some(shape) = pict.kids.iter_mut().find_map(|k| match k {
        xml::Node::El(e) if e.name.starts_with("v:") && e.name != "v:shapetype" => Some(e),
        _ => None,
    }) else {
        return;
    };
    let (rw, rh, rf) = crate::read::story::vml_frame(shape);
    if (rw - w).abs() < 0.005 && (rh - h).abs() < 0.005 && rf == *float {
        return;
    }
    match shape.attrs.iter_mut().find(|(k, _)| k == "style") {
        Some((_, style)) => *style = vml_style_with(style, w, h, float),
        None => shape.attrs.push(("style".into(), vml_style_with("", w, h, float))),
    }
    // The DrawingML copy Office keeps of the shape no longer matches it.
    shape.attrs.retain(|(k, _)| k != "o:gfxdata");
}

/// Set an extent's `cx` and `cy` (those it has) to `w` × `h` points.
fn set_size_attrs(e: &mut El, w: f32, h: f32) {
    for (k, v) in e.attrs.iter_mut() {
        match k.as_str() {
            "cx" => *v = emu(w.max(0.0)),
            "cy" => *v = emu(h.max(0.0)),
            _ => {}
        }
    }
}

/// Is attribute `k` a relationship id (see `read::embed::rel_ids`)?
fn is_rel_attr(k: &str) -> bool {
    k.starts_with("r:") || k == "o:relid"
}

fn rel_attr_values(e: &El, out: &mut Vec<String>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for (k, v) in &e.attrs {
        if is_rel_attr(k) && !out.contains(v) {
            out.push(v.clone());
        }
    }
    for c in e.els() {
        rel_attr_values(c, out, depth + 1);
    }
}

fn set_rel_ids(e: &mut El, ids: &HashMap<String, String>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for (k, v) in e.attrs.iter_mut() {
        if is_rel_attr(k)
            && let Some(n) = ids.get(v.as_str())
        {
            *v = n.clone();
        }
    }
    for k in e.kids.iter_mut() {
        if let xml::Node::El(c) = k {
            set_rel_ids(c, ids, depth + 1);
        }
    }
}

/// `bytes` with each `relId="old"` naming a remapped relationship rewritten to its new id.
fn patch_rel_ids(bytes: &[u8], ids: &HashMap<String, String>) -> Vec<u8> {
    const KEY: &[u8] = b"relId=\"";
    let mut out = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while let Some(at) = rest.windows(KEY.len()).position(|w| w == KEY) {
        let (head, tail) = rest.split_at(at + KEY.len());
        out.extend_from_slice(head);
        let end = tail.iter().position(|b| *b == b'"').unwrap_or(tail.len());
        let (old, after) = tail.split_at(end);
        match std::str::from_utf8(old).ok().and_then(|o| ids.get(o)) {
            Some(new) => out.extend_from_slice(new.as_bytes()),
            None => out.extend_from_slice(old),
        }
        rest = after;
    }
    out.extend_from_slice(rest);
    out
}

/// A VML `style` with the object's size, rotation and flips, and its offset when it floats
/// (`position:absolute`).
fn vml_style_with(style: &str, w: f32, h: f32, float: &Float) -> String {
    let absolute = style
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .any(|(k, v)| k.trim().eq_ignore_ascii_case("position") && v.trim().eq_ignore_ascii_case("absolute"));
    let mut out: Vec<String> = style
        .split(';')
        .filter(|d| {
            let key = d.split_once(':').map_or("", |(k, _)| k).trim().to_ascii_lowercase();
            !(d.trim().is_empty()
                || matches!(key.as_str(), "width" | "height" | "rotation" | "flip")
                || absolute && matches!(key.as_str(), "left" | "top" | "margin-left" | "margin-top"))
        })
        .map(|d| d.trim().to_string())
        .collect();
    if absolute {
        out.push(format!("margin-left:{}pt", pt(float.x)));
        out.push(format!("margin-top:{}pt", pt(float.y)));
    }
    out.push(format!("width:{}pt", pt(w.max(0.0))));
    out.push(format!("height:{}pt", pt(h.max(0.0))));
    // VML `rotation`: degrees clockwise; `flip`: `x` and/or `y` (as `read::story::vml_float` reads them).
    let spin = float.spin();
    if spin.deg != 0.0 {
        out.push(format!("rotation:{}", pt(spin.deg)));
    }
    let flip: Vec<&str> = [(spin.flip_h, "x"), (spin.flip_v, "y")].into_iter().filter(|(on, _)| *on).map(|(_, f)| f).collect();
    if !flip.is_empty() {
        out.push(format!("flip:{}", flip.join(" ")));
    }
    out.join(";")
}

/// Points for a VML style: finite, bounded, at most two decimals.
fn pt(v: f32) -> String {
    let v = kition_doc_geom::finite(v).clamp(-crate::units::MAX_LEN_PT, crate::units::MAX_LEN_PT);
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_part_drawing_ids_are_rewritten_once() {
        let ids: HashMap<String, String> = [("rId9".to_string(), "rId2".to_string()), ("rId2".to_string(), "rId3".to_string())].into_iter().collect();
        let out = patch_rel_ids(br#"<x relId="rId9"/><y relId="rId2"/><z relId="rId7"/><w relId="unterminated"#, &ids);
        assert_eq!(String::from_utf8(out).unwrap(), r#"<x relId="rId2"/><y relId="rId3"/><z relId="rId7"/><w relId="unterminated"#);
    }

    #[test]
    fn vml_style_takes_the_new_size_and_offset() {
        let inline = vml_style_with("width:100pt;height:50pt;mso-wrap-style:square", 120.5, 60.0, &Float::default());
        assert_eq!(inline, "mso-wrap-style:square;width:120.5pt;height:60pt");
        let float = Float { x: 10.0, y: 20.25, ..Float::default() };
        let abs = vml_style_with("position:absolute;left:5pt;margin-left:3pt;top:1pt;width:1pt;height:1pt", 2.0, 3.0, &float);
        assert_eq!(abs, "position:absolute;margin-left:10pt;margin-top:20.25pt;width:2pt;height:3pt");
        assert_eq!(pt(f32::NAN), "0");
    }

    #[test]
    fn vml_style_takes_the_rotation_and_flips() {
        let mut float = Float::default();
        float.set_spin(kition_doc_geom::Spin::new(-30.5, true, true));
        let s = vml_style_with("width:1pt;rotation:10;flip:x;height:1pt;z-index:3", 2.0, 3.0, &float);
        assert_eq!(s, "z-index:3;width:2pt;height:3pt;rotation:329.5;flip:x y");
        // Unturned: no rotation or flip left behind.
        assert_eq!(vml_style_with("rotation:90;flip:y;width:1pt", 2.0, 3.0, &Float::default()), "width:2pt;height:3pt");
    }
}
