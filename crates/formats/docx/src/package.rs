//! OPC package access: bounded zip reading, relationships, part names.

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read, Write};

use crate::DocxError;
use crate::xml::{self, El};

/// Largest single part we decompress.
pub const MAX_PART: u64 = 256 * 1024 * 1024;
/// Largest total of all decompressed parts.
pub const MAX_TOTAL: u64 = 512 * 1024 * 1024;
/// Most zip entries we look at.
pub const MAX_ENTRIES: usize = 20_000;

/// Relationship type suffixes (after `.../relationships/`).
pub mod rt {
    pub const OFFICE_DOC: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
    pub const CORE: &str = "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
    pub const APP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties";
    pub const CUSTOM: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties";
    pub const STYLES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
    pub const NUMBERING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
    pub const SETTINGS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    pub const THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
    pub const FONT_TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable";
    pub const FOOTNOTES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes";
    pub const ENDNOTES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes";
    pub const COMMENTS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
    pub const COMMENTS_EX: &str = "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";
    pub const COMMENTS_IDS: &str = "http://schemas.microsoft.com/office/2016/09/relationships/commentsIds";
    pub const COMMENTS_EXTENSIBLE: &str = "http://schemas.microsoft.com/office/2018/08/relationships/commentsExtensible";
    pub const PEOPLE: &str = "http://schemas.microsoft.com/office/2011/relationships/people";
    pub const HEADER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
    pub const FOOTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer";
    pub const IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
    pub const HYPERLINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
    pub const VBA_PROJECT: &str = "http://schemas.microsoft.com/office/2006/relationships/vbaProject";
    pub const CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
    pub const DIAGRAM_DATA: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagramData";
    pub const DIAGRAM_DRAWING: &str = "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing";
}

/// `Document::passthrough` key for the font table (`w:fonts`: the panose numbers, character sets,
/// pitches and alternate names Word substitutes missing fonts by), kept verbatim.
pub const FONT_TABLE_PART: &str = "word/fontTable.xml";

/// `Document::passthrough` key for the settings Kition Doc does not model (document variables,
/// revision save ids, the colour scheme mapping, compatibility settings, view options…), kept as
/// their XML and written back in schema order around the modelled ones. Not a package part.
pub const SETTINGS_KEPT: &str = "kition-doc:settings.kept";

/// `Document::passthrough` key for the people part (`w15:people`: the comment and revision
/// authors' sign-in identities), kept verbatim.
pub const PEOPLE_PART: &str = "word/people.xml";

/// `Document::passthrough` key for the theme part a file had, written back verbatim while the
/// document's theme (name, colours, fonts) is still what it says; otherwise a theme is written from
/// the document's settings.
pub const THEME_PART: &str = "word/theme/theme1.xml";

/// `Document::passthrough` key for the styles part's `w:latentStyles` (the built-in styles Word
/// lists, hides or locks before they are used), kept verbatim. Not a package part.
pub const LATENT_STYLES: &str = "kition-doc:styles.latentStyles";

/// `Document::passthrough` key for the styles a file had (`w:style` elements), kept as their XML
/// with a digest of what the writer writes for the style as read: a style is written back as the
/// file had it (table style regions, `w14` effects, flags the model does not hold) while the writer
/// would still write it the same; an edited style is written from the model. [`kept_records`]
/// format. Not a package part.
pub const STYLES_KEPT: &str = "kition-doc:styles.kept";

/// Most bytes of kept markup records under one passthrough key.
pub const MAX_KEPT_RECORDS: usize = 32 * 1024 * 1024;

/// FNV-1a of `s`: tells whether what the writer would write for an object is still what it wrote
/// for the object as read.
pub fn digest(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// Records of kept markup with digests, as stored: `{digest:016x} {len}\n{xml}` each.
pub fn kept_records_to_bytes(records: &[(u64, String)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (d, xml) in records {
        out.extend_from_slice(format!("{d:016x} {}\n", xml.len()).as_bytes());
        out.extend_from_slice(xml.as_bytes());
    }
    out
}

/// The records of [`kept_records_to_bytes`]; stops at the first malformed one.
pub fn kept_records(bytes: &[u8]) -> Vec<(u64, &str)> {
    let mut out = Vec::new();
    let mut rest = bytes.get(..bytes.len().min(MAX_KEPT_RECORDS)).unwrap_or_default();
    while !rest.is_empty() {
        let Some(nl) = rest.iter().position(|b| *b == b'\n') else { break };
        let Some(head) = rest.get(..nl).and_then(|h| std::str::from_utf8(h).ok()) else { break };
        let Some((d, len)) = head.split_once(' ') else { break };
        let (Ok(d), Ok(len)) = (u64::from_str_radix(d, 16), len.parse::<usize>()) else { break };
        let Some(xml) = nl.checked_add(1).and_then(|start| Some((start, start.checked_add(len)?))).and_then(|(a, b)| rest.get(a..b)) else { break };
        let Ok(xml) = std::str::from_utf8(xml) else { break };
        out.push((d, xml));
        rest = rest.get(nl + 1 + len..).unwrap_or_default();
    }
    out
}

/// The target of a kept object's relationship that names the object's own picture (a VML
/// picture's `v:imagedata`): written as a relationship to the media file the writer writes for that
/// picture, so the image is not carried twice. Not a package part.
pub const OWN_MEDIA: &str = "kition-doc:media";

/// `Document::passthrough` key for a macro project, kept as opaque bytes (never parsed or run).
pub const VBA_PROJECT_PART: &str = "word/vbaProject.bin";
/// `Document::passthrough` key listing the parts the project relates to (VBA data, signatures…),
/// one `relationship type \t part name \t content type` line each; each part's bytes are kept
/// under its part name. Not a package part: Kition Doc's own bookkeeping.
pub const VBA_RELATED: &str = "kition-doc:vbaProject.related";
/// Most parts we carry along with a macro project.
pub const MAX_VBA_RELATED: usize = 64;

/// `Document::passthrough` key listing the parts that objects read from a file refer to (charts,
/// smart graphics, OLE objects: see `kition_doc_doc::graphic::Embedded`), with their content
/// types and own relationships, as [`EmbeddedManifest`] lines. Each part's bytes are kept under its
/// part name. Not a package part: Kition Doc's own bookkeeping.
pub const EMBEDDED_PARTS: &str = "kition-doc:embedded.parts";
/// Most parts we carry along for a file's embedded objects.
pub const MAX_EMBEDDED_PARTS: usize = 4096;
/// Most relationships we keep for one such part.
pub const MAX_EMBEDDED_RELS: usize = 1024;
/// Longest markup we keep for one embedded object.
pub const MAX_EMBEDDED_XML: usize = 1024 * 1024;

/// One part an embedded object needs: its content type and its own relationships (internal
/// targets are part names).
#[derive(Clone, Debug, Default)]
pub struct EmbeddedPart {
    pub content_type: String,
    pub rels: Vec<Rel>,
}

/// The parts kept for embedded objects, by part name (see [`EMBEDDED_PARTS`]). Stored as text:
/// `P\tpart\tcontent type` per part, then `R\tpart\tid\ttype\ttarget\tI|E` per relationship of
/// that part (`E`: `target` is an external URL).
#[derive(Clone, Debug, Default)]
pub struct EmbeddedManifest {
    pub parts: BTreeMap<String, EmbeddedPart>,
}

/// Can `s` be a field of a manifest line?
pub fn manifest_field_ok(s: &str) -> bool {
    !s.is_empty() && !s.contains(['\t', '\r', '\n'])
}

impl EmbeddedManifest {
    pub fn parse(bytes: &[u8]) -> EmbeddedManifest {
        let mut m = EmbeddedManifest::default();
        for line in String::from_utf8_lossy(bytes).lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match f.as_slice() {
                ["P", part, ct] if m.parts.len() < MAX_EMBEDDED_PARTS => {
                    m.parts.entry((*part).to_string()).or_insert_with(|| EmbeddedPart { content_type: (*ct).to_string(), rels: Vec::new() });
                }
                ["R", part, id, kind, target, mode] => {
                    if let Some(p) = m.parts.get_mut(*part)
                        && p.rels.len() < MAX_EMBEDDED_RELS
                        && !p.rels.iter().any(|r| r.id == *id)
                    {
                        p.rels.push(Rel { id: (*id).to_string(), kind: (*kind).to_string(), target: (*target).to_string(), external: *mode == "E" });
                    }
                }
                _ => {}
            }
        }
        m
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut s = String::new();
        for (part, p) in &self.parts {
            if !manifest_field_ok(part) || !manifest_field_ok(&p.content_type) {
                continue;
            }
            s.push_str(&format!("P\t{part}\t{}\n", p.content_type));
        }
        for (part, p) in &self.parts {
            for r in p.rels.iter().filter(|r| [&r.id, &r.kind, &r.target].iter().all(|f| manifest_field_ok(f))) {
                s.push_str(&format!("R\t{part}\t{}\t{}\t{}\t{}\n", r.id, r.kind, r.target, if r.external { "E" } else { "I" }));
            }
        }
        s.into_bytes()
    }
}

/// Does relationship type `t` end with `suffix` (ignoring the transitional/strict prefix)?
pub fn rel_is(t: &str, full: &str) -> bool {
    let tail = |s: &str| s.rsplit('/').next().unwrap_or(s).to_string();
    t == full || tail(t) == tail(full)
}

/// The decompressed parts of a package (bounded).
pub struct Package {
    files: BTreeMap<String, Vec<u8>>,
}

impl Package {
    pub fn open(bytes: &[u8]) -> Result<Package, DocxError> {
        Package::open_limited(bytes, MAX_PART, MAX_TOTAL)
    }

    pub fn open_limited(bytes: &[u8], max_part: u64, max_total: u64) -> Result<Package, DocxError> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| DocxError::Zip(e.to_string()))?;
        if zip.len() > MAX_ENTRIES {
            return Err(DocxError::Limit(format!("{} zip entries", zip.len())));
        }
        let mut files = BTreeMap::new();
        let mut total: u64 = 0;
        for i in 0..zip.len() {
            let mut f = match zip.by_index(i) {
                Ok(f) => f,
                Err(e) => {
                    log::warn!("docx: skipping unreadable zip entry {i}: {e}");
                    continue;
                }
            };
            if f.is_dir() {
                continue;
            }
            let name = f.name().trim_start_matches('/').to_string();
            if f.size() > max_part {
                return Err(DocxError::Limit(format!("part {name} declares {} bytes", f.size())));
            }
            let room = max_part.min(max_total.saturating_sub(total));
            let mut buf = Vec::with_capacity(f.size().min(16 * 1024 * 1024) as usize);
            let read = (&mut f).take(room + 1).read_to_end(&mut buf);
            if let Err(e) = read {
                // A corrupt entry: keep going unless it's a part we can't do without.
                log::warn!("docx: zip entry {name} unreadable: {e}");
                continue;
            }
            if buf.len() as u64 > room {
                return Err(DocxError::Limit(format!("part {name} decompresses past the size limit")));
            }
            total = total.saturating_add(buf.len() as u64);
            files.insert(name, buf);
        }
        Ok(Package { files })
    }

    /// Part bytes by name (leading `/` ignored; case-insensitive fallback).
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        let n = name.trim_start_matches('/');
        if let Some(v) = self.files.get(n) {
            return Some(v);
        }
        self.files.iter().find(|(k, _)| k.eq_ignore_ascii_case(n)).map(|(_, v)| v.as_slice())
    }

    pub fn xml(&self, name: &str) -> Result<Option<El>, DocxError> {
        match self.get(name) {
            Some(b) => xml::parse(b).map(Some),
            None => Ok(None),
        }
    }

    /// Relationships of `part` (from `dir/_rels/name.rels`).
    pub fn rels(&self, part: &str) -> Rels {
        let (dir, file) = split_dir(part);
        let path = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
        let mut rels = Rels::default();
        let root = match self.xml(&path) {
            Ok(Some(root)) => root,
            Ok(None) => return rels,
            Err(e) => {
                log::warn!("docx: ignoring unreadable relationships {path}: {e}");
                return rels;
            }
        };
        for r in root.els().filter(|e| e.local() == "Relationship") {
            let (Some(id), Some(target)) = (r.attr("Id"), r.attr("Target")) else { continue };
            let external = r.attr("TargetMode").is_some_and(|m| m.eq_ignore_ascii_case("External"));
            let resolved = if external { target.to_string() } else { resolve(dir, target) };
            rels.list.push(Rel { id: id.to_string(), kind: r.attr("Type").unwrap_or("").to_string(), target: resolved, external });
        }
        rels
    }
}

/// `[Content_Types].xml`: Override by part name, then Default by extension (both case-insensitive).
#[derive(Default)]
pub struct ContentTypes {
    overrides: HashMap<String, String>,
    defaults: HashMap<String, String>,
}

impl ContentTypes {
    pub fn read(pkg: &Package) -> ContentTypes {
        let mut ct = ContentTypes::default();
        let root = match pkg.xml("[Content_Types].xml") {
            Ok(Some(root)) => root,
            Ok(None) => return ct,
            Err(e) => {
                log::warn!("docx: ignoring unreadable [Content_Types].xml: {e}");
                return ct;
            }
        };
        for e in root.els() {
            let (Some(key), Some(t)) = (e.attr("PartName").or_else(|| e.attr("Extension")), e.attr("ContentType")) else { continue };
            let map = if e.local() == "Override" { &mut ct.overrides } else { &mut ct.defaults };
            map.entry(key.trim_start_matches('/').to_ascii_lowercase()).or_insert_with(|| t.to_string());
        }
        ct
    }

    /// Content type of `part` (a package path without the leading `/`).
    pub fn of(&self, part: &str) -> Option<&str> {
        let part = part.trim_start_matches('/').to_ascii_lowercase();
        if let Some(t) = self.overrides.get(&part) {
            return Some(t);
        }
        let ext = part.rsplit_once('.').map(|(_, e)| e)?;
        self.defaults.get(ext).map(String::as_str)
    }
}

#[derive(Clone, Debug)]
pub struct Rel {
    pub id: String,
    pub kind: String,
    /// Package path (internal) or URL (external).
    pub target: String,
    pub external: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Rels {
    pub list: Vec<Rel>,
}

impl Rels {
    pub fn by_id(&self, id: &str) -> Option<&Rel> {
        self.list.iter().find(|r| r.id == id)
    }
    pub fn by_type(&self, full: &str) -> Option<&Rel> {
        self.list.iter().find(|r| rel_is(&r.kind, full))
    }
}

fn split_dir(part: &str) -> (&str, &str) {
    let p = part.trim_start_matches('/');
    match p.rfind('/') {
        Some(i) => (p.get(..i).unwrap_or(""), p.get(i + 1..).unwrap_or(p)),
        None => ("", p),
    }
}

/// Resolve a relative target against a source directory into a package path.
pub fn resolve(dir: &str, target: &str) -> String {
    let target = target.split('#').next().unwrap_or(target);
    let mut parts: Vec<&str> = if target.starts_with('/') { Vec::new() } else { dir.split('/').filter(|s| !s.is_empty()).collect() };
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// The relative reference from part `from` to part `to` (both package paths), as a
/// relationship target: `word/document.xml` → `word/charts/chart1.xml` is `charts/chart1.xml`.
pub fn relative(from: &str, to: &str) -> String {
    let from_dir: Vec<&str> = from.split('/').filter(|s| !s.is_empty()).collect();
    let from_dir = from_dir.get(..from_dir.len().saturating_sub(1)).unwrap_or(&[]);
    let to: Vec<&str> = to.split('/').filter(|s| !s.is_empty()).collect();
    let to_dir = to.get(..to.len().saturating_sub(1)).unwrap_or(&[]);
    let common = from_dir.iter().zip(to_dir).take_while(|(a, b)| a == b).count();
    let mut out: Vec<&str> = vec![".."; from_dir.len() - common];
    out.extend(to.get(common..).unwrap_or(&[]));
    let s = out.join("/");
    // A relative reference whose first segment has a colon would read as a URI scheme.
    if s.split('/').next().is_some_and(|seg| seg.contains(':')) { format!("./{s}") } else { s }
}

/// Build a zip from (name, bytes) entries.
pub fn zip_entries(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, DocxError> {
    let mut zw = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).compression_level(Some(6));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in entries {
        // Already-compressed media gain nothing from deflate.
        let lower = name.to_ascii_lowercase();
        let o = if [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|e| lower.ends_with(e)) { stored } else { opts };
        zw.start_file(name.as_str(), o).map_err(|e| DocxError::Zip(e.to_string()))?;
        zw.write_all(bytes).map_err(|e| DocxError::Zip(e.to_string()))?;
    }
    let c = zw.finish().map_err(|e| DocxError::Zip(e.to_string()))?;
    Ok(c.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zip_of(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let e: Vec<(String, Vec<u8>)> = entries.iter().map(|(n, b)| (n.to_string(), b.clone())).collect();
        zip_entries(&e).unwrap()
    }

    #[test]
    fn decompression_bomb_is_rejected() {
        let z = zip_of(&[("word/document.xml", vec![b' '; 2_000_000])]);
        assert!(z.len() < 100_000);
        assert!(matches!(Package::open_limited(&z, 1_000_000, 10_000_000), Err(DocxError::Limit(_))));
        assert!(matches!(Package::open_limited(&z, 10_000_000, 1_000_000), Err(DocxError::Limit(_))));
        assert!(Package::open_limited(&z, 10_000_000, 10_000_000).is_ok());
    }

    #[test]
    fn lying_declared_size_is_rejected_or_bounded() {
        let mut z = zip_of(&[("a.xml", b"<a/>".to_vec())]);
        // Patch the uncompressed size in the central directory and local header to ~4 GB.
        let mut patched = 0;
        for sig in [[0x50u8, 0x4B, 0x01, 0x02], [0x50, 0x4B, 0x03, 0x04]] {
            if let Some(pos) = z.windows(4).position(|w| w == sig) {
                let off = if sig[2] == 1 { pos + 24 } else { pos + 22 };
                z[off..off + 4].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
                patched += 1;
            }
        }
        assert_eq!(patched, 2);
        match Package::open(&z) {
            Err(_) => {}
            Ok(p) => assert!(p.get("a.xml").is_none_or(|b| b.len() < 1024)),
        }
    }

    #[test]
    fn resolves_targets() {
        assert_eq!(resolve("word", "media/image1.png"), "word/media/image1.png");
        assert_eq!(resolve("word", "../customXml/item1.xml"), "customXml/item1.xml");
        assert_eq!(resolve("word", "/word/styles.xml"), "word/styles.xml");
        assert_eq!(resolve("", "word/document.xml"), "word/document.xml");
        assert_eq!(resolve("word", "../../../x"), "x");
    }

    #[test]
    fn relative_targets_resolve_back() {
        for (from, to, want) in [
            ("word/document.xml", "word/charts/chart1.xml", "charts/chart1.xml"),
            ("word/charts/chart1.xml", "word/embeddings/book.xlsx", "../embeddings/book.xlsx"),
            ("word/charts/chart1.xml", "word/charts/colors1.xml", "colors1.xml"),
            ("word/document.xml", "customXml/item1.xml", "../customXml/item1.xml"),
            ("word/document.xml", "word/a:b.bin", "./a:b.bin"),
        ] {
            let r = relative(from, to);
            assert_eq!(r, want);
            assert_eq!(resolve(from.rsplit_once('/').map_or("", |(d, _)| d), &r), to);
        }
    }

    #[test]
    fn embedded_manifest_round_trips_and_ignores_junk() {
        let mut m = EmbeddedManifest::default();
        let rel = Rel { id: "rId1".into(), kind: "k".into(), target: "word/embeddings/x.xlsx".into(), external: false };
        let url = Rel { id: "rId2".into(), kind: "k".into(), target: "https://example.com/x".into(), external: true };
        m.parts.insert("word/charts/chart1.xml".into(), EmbeddedPart { content_type: "ct".into(), rels: vec![rel, url] });
        let back = EmbeddedManifest::parse(&m.to_bytes());
        let p = &back.parts["word/charts/chart1.xml"];
        assert_eq!((p.content_type.as_str(), p.rels.len(), p.rels[1].external), ("ct", 2, true));
        let junk = EmbeddedManifest::parse(b"R\tnope\trId1\tk\tt\tI\nP\tonly-two\n\t\t\n\xff\xfe");
        assert!(junk.parts.is_empty());
    }
}
