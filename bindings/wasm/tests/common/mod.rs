//! The adapter of the Kition platform kit suite's performance mode (`kition_convergence::perf`)
//! for Kition Doc: replicas are [`DocDocument`]s driven the way a host drives the engine
//! (contract 1 commands, the contract 2 change exchange, `openWithReplica` to join), and the
//! canonical state is read from each replica's snapshot.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use kdm_model::DocModel;
use kition_convergence::{Blobs, KitAdapter, Rng, Script};
use kition_doc_doc::{Block, Document, PartKind};
use kition_doc_kit::DocDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, Replica};
use kition_kit::session::{KitModel, MemoryBlobs};
use serde_json::{Value, json};

pub const NAME: &str = "Document.docx";
const SCHEMA: &str = include_str!("../../../../crates/kdm/kdm-model/model/schema.json");

/// Two CJK ideographs and an emoji, by code point, for typing.
const CJK_WORD: [char; 2] = ['\u{6587}', '\u{5B57}'];
const EMOJI: char = '\u{1F389}';

pub struct DocKit;

/// A replica's document read back from its own snapshot, with its blobs.
fn replica_of(doc: &mut DocDocument) -> Result<(Replica, MemoryBlobs), String> {
    let snapshot = doc.snapshot()?;
    let replica = Replica::load(DocModel::schema(), &snapshot, ActorId::random()).map_err(|error| error.to_string())?;
    let blobs = decode_blobs(&doc.blobs()?)?;
    Ok((replica, memory(&blobs)))
}

fn memory(blobs: &Blobs) -> MemoryBlobs {
    MemoryBlobs(blobs.iter().map(|(hash, bytes)| (hash.clone(), Arc::from(bytes.as_slice()))).collect())
}

/// The document as its replica holds it (the view matches it: [`DocKit::check_view`]).
pub fn document(doc: &mut DocDocument) -> Result<Document, String> {
    let (replica, blobs) = replica_of(doc)?;
    kdm_model::read_document(&replica, &blobs).map(|(document, _)| document).map_err(|error| error.to_string())
}

/// The body's top-level paragraphs: block index and text.
fn paragraphs(document: &Document) -> Vec<(usize, String)> {
    document.body.iter().enumerate().filter_map(|(index, block)| block.as_para().map(|para| (index, para.text.clone()))).collect()
}

/// Character `at` of the paragraphs joined by `\n`, as a body position `{block, off}` (bytes).
fn position(document: &Document, at: usize) -> Value {
    let paras = paragraphs(document);
    let mut rest = at;
    for (block, text) in &paras {
        let count = text.chars().count();
        if rest <= count {
            let off = text.char_indices().nth(rest).map_or(text.len(), |(off, _)| off);
            return json!({ "block": block, "off": off });
        }
        rest -= count + 1;
    }
    let (block, text) = paras.last().cloned().unwrap_or((0, String::new()));
    json!({ "block": block, "off": text.len() })
}

/// Blobs as `DocDocument::blobs` encodes them: `(64-character hex hash, u64 LE length, bytes) * N`.
pub fn decode_blobs(bytes: &[u8]) -> Result<Blobs, String> {
    let mut out = BTreeMap::new();
    let mut at = 0usize;
    while at < bytes.len() {
        let hash = bytes.get(at..at + 64).ok_or("truncated blob hash")?;
        let len_bytes: [u8; 8] = bytes.get(at + 64..at + 72).ok_or("truncated blob length")?.try_into().map_err(|_| "blob length")?;
        let len = usize::try_from(u64::from_le_bytes(len_bytes)).map_err(|_| "blob length")?;
        let data = bytes.get(at + 72..at + 72 + len).ok_or("truncated blob")?;
        out.insert(String::from_utf8_lossy(hash).into_owned(), data.to_vec());
        at += 72 + len;
    }
    Ok(out)
}

/// Blobs as `DocDocument::add_blobs` takes them: `(64-character hex hash, u64 LE length, bytes) * N`.
pub fn encode_blobs(blobs: &Blobs) -> Vec<u8> {
    let mut out = Vec::new();
    for (hash, bytes) in blobs {
        out.extend_from_slice(hash.as_bytes());
        out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

/// A small 24-bit BMP of one colour from `seed`, base64-encoded for `insert.picture`.
fn picture(seed: u32) -> String {
    let (w, h) = (4u32, 4u32);
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut bmp = Vec::new();
    bmp.extend_from_slice(b"BM");
    for value in [size, 0, 54, 40, w, h] {
        bmp.extend_from_slice(&value.to_le_bytes());
    }
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    for value in [0u32, row * h, 2835, 2835, 0, 0] {
        bmp.extend_from_slice(&value.to_le_bytes());
    }
    let color = seed.to_le_bytes();
    for _ in 0..h {
        for _ in 0..w {
            bmp.extend_from_slice(&[color[0], color[1], color[2] | 1]);
        }
        bmp.extend(std::iter::repeat_n(0, (row - w * 3) as usize));
    }
    base64(&bmp)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

impl DocKit {
    fn run(&self, doc: &mut DocDocument, command: &str, params: Value) -> Result<Value, String> {
        doc.execute(command, params).map_err(|error| format!("{command}: {error}"))
    }

    fn caret(&self, doc: &mut DocDocument, pos: Value) -> Result<(), String> {
        self.run(doc, "caret.set", json!({ "pos": pos })).map(|_| ())
    }

    fn select(&self, doc: &mut DocDocument, anchor: Value, focus: Value) -> Result<(), String> {
        self.run(doc, "select.range", json!({ "anchor": anchor, "focus": focus })).map(|_| ())
    }

    /// The document must never leave its replica: that is a bug the host would have to resync.
    fn no_reset(&self, doc: &DocDocument, what: &str) -> Result<(), String> {
        match doc.kdm_resets() {
            0 => Ok(()),
            resets => Err(format!("{what} made the document leave its replica ({resets} resets)")),
        }
    }

    /// One random edit, before the reset check.
    fn edit(&self, doc: &mut DocDocument, rng: &mut Rng) -> Result<Option<String>, String> {
        let document = document(doc)?;
        let text: String = paragraphs(&document).into_iter().map(|(_, text)| text).collect::<Vec<_>>().join("\n");
        let len = text.chars().count();
        let at = rng.below(len + 1);
        let pos = position(&document, at);
        let span = |rng: &mut Rng| {
            let end = (at + 1 + rng.below(6)).min(len);
            position(&document, end)
        };
        let roll = rng.below(100);
        let (label, result) = match roll {
            0..30 => {
                let word = match rng.below(3) {
                    0 => format!("w{} ", rng.below(100)),
                    1 => CJK_WORD.iter().collect(),
                    _ => EMOJI.to_string(),
                };
                self.caret(doc, pos)?;
                ("type", self.run(doc, "text.insert", json!({ "text": word })))
            }
            30..40 => {
                let focus = span(rng);
                self.select(doc, pos, focus)?;
                ("delete", self.run(doc, "text.delete", json!({})))
            }
            40..50 => {
                self.caret(doc, pos)?;
                ("enter", self.run(doc, "text.newParagraph", json!({})))
            }
            50..57 => {
                // Backspace at a paragraph's start merges it into the one before.
                let starts: Vec<usize> = text.char_indices().filter(|(_, c)| *c == '\n').map(|(i, _)| text[..i].chars().count() + 1).collect();
                let Some(&start) = rng.pick(&starts) else { return Ok(None) };
                self.caret(doc, position(&document, start))?;
                ("merge", self.run(doc, "text.backspace", json!({})))
            }
            57..65 => {
                let focus = span(rng);
                self.select(doc, pos, focus)?;
                let command = *rng.pick(&["format.bold", "format.italic", "format.underline"]).unwrap_or(&"format.bold");
                ("format", self.run(doc, command, json!({})))
            }
            65..72 => {
                self.caret(doc, pos)?;
                let value = *rng.pick(&["left", "center", "right", "justify"]).unwrap_or(&"left");
                ("align", self.run(doc, "para.align", json!({ "value": value })))
            }
            72..77 => {
                self.caret(doc, pos)?;
                let command = *rng.pick(&["para.heading1", "para.normal", "para.bullets", "para.numbering"]).unwrap_or(&"para.normal");
                ("paragraph style", self.run(doc, command, json!({})))
            }
            77..81 => {
                self.caret(doc, pos)?;
                ("picture", self.run(doc, "insert.picture", json!({ "data": picture(rng.below(1 << 24) as u32), "width": 40 })))
            }
            81..85 => {
                self.caret(doc, pos)?;
                ("table", self.run(doc, "insert.table", json!({ "rows": 2, "cols": 2 })))
            }
            85..90 => {
                let focus = span(rng);
                self.select(doc, pos, focus)?;
                ("comment", self.run(doc, "review.newComment", json!({ "text": format!("note {}", rng.below(100)) })))
            }
            90..95 => ("title", self.run(doc, "file.properties", json!({ "title": format!("Title {}", rng.below(1000)) }))),
            _ => {
                let color = format!("{:06X}", rng.below(1 << 24));
                ("page color", self.run(doc, "design.pageColor", json!({ "color": color })))
            }
        };
        // A command the engine refuses for this state (a table where none fits) is no edit.
        Ok(result.ok().map(|_| label.to_owned()))
    }
}

/// One replica as a host keeps it: the engine's document and the host's store of the blobs it was
/// given, which a host hands the engine before every import (`addBlobs`, then `importChanges`).
pub struct DocReplica {
    pub doc: DocDocument,
    store: Blobs,
}

impl KitAdapter for DocKit {
    type Doc = DocReplica;

    fn name(&self) -> &str {
        "kit.doc"
    }

    fn schema(&self) -> &str {
        SCHEMA
    }

    fn create(&self) -> Result<DocReplica, String> {
        let mut doc = DocDocument::create(NAME, 0, 0)?;
        for (index, text) in ["Hello world, a shared document.", "Second paragraph here.", "Third one closes it."].into_iter().enumerate() {
            if index > 0 {
                self.run(&mut doc, "text.newParagraph", json!({}))?;
            }
            self.run(&mut doc, "text.insert", json!({ "text": text }))?;
        }
        // Make the replica now, holding the content above.
        doc.version()?;
        Ok(DocReplica { doc, store: Blobs::new() })
    }

    fn join(&self, snapshot: &[u8], blobs: &Blobs) -> Result<DocReplica, String> {
        let doc = DocDocument::open_with_replica(NAME, &[], snapshot, &encode_blobs(blobs))?;
        if !doc.has_replica() {
            return Err("the snapshot did not open as a replica".into());
        }
        Ok(DocReplica { doc, store: blobs.clone() })
    }

    fn version(&self, replica: &mut DocReplica) -> Result<Vec<u8>, String> {
        replica.doc.version()
    }

    fn export_changes(&self, replica: &mut DocReplica, since: &[u8]) -> Result<Vec<u8>, String> {
        replica.doc.export_changes(since)
    }

    fn import_changes(&self, replica: &mut DocReplica, bytes: &[u8]) -> Result<(), String> {
        if !replica.store.is_empty() {
            replica.doc.add_blobs(&encode_blobs(&replica.store))?;
        }
        replica.doc.import_changes(bytes)?;
        self.no_reset(&replica.doc, "an import")
    }

    fn snapshot(&self, replica: &mut DocReplica) -> Result<Vec<u8>, String> {
        replica.doc.snapshot()
    }

    fn state(&self, replica: &mut DocReplica) -> Result<Value, String> {
        let (loaded, _) = replica_of(&mut replica.doc)?;
        loaded.state_json().map_err(|error| error.to_string())
    }

    fn undo(&self, replica: &mut DocReplica) -> Result<bool, String> {
        let (done, _) = replica.doc.undo()?;
        self.no_reset(&replica.doc, "an undo")?;
        Ok(done)
    }

    fn redo(&self, replica: &mut DocReplica) -> Result<bool, String> {
        let (done, _) = replica.doc.redo()?;
        self.no_reset(&replica.doc, "a redo")?;
        Ok(done)
    }

    fn random_edit(&self, replica: &mut DocReplica, rng: &mut Rng) -> Result<Option<String>, String> {
        let label = self.edit(&mut replica.doc, rng)?;
        self.no_reset(&replica.doc, label.as_deref().unwrap_or("a refused command"))?;
        Ok(label)
    }

    fn script(&self, replica: &mut DocReplica, step: &Script) -> Result<bool, String> {
        let doc = &mut replica.doc;
        let document = document(doc)?;
        let first = paragraphs(&document).first().map(|(block, _)| *block).unwrap_or(0);
        match step {
            // Two registers of the document object: its properties (`core`) and its settings.
            // (A paragraph's properties are one register, `Block.para`: see the report.)
            Script::SetField { slot: 0, value } => {
                self.run(doc, "file.properties", json!({ "title": format!("Title {value}") }))?;
            }
            Script::SetField { value, .. } => {
                self.run(doc, "design.pageColor", json!({ "color": format!("{:06X}", value.wrapping_mul(2_654_435_761) & 0xFF_FFFF) }))?;
            }
            Script::Create => {
                self.run(doc, "caret.docEnd", json!({}))?;
                self.run(doc, "text.newParagraph", json!({}))?;
                self.run(doc, "text.insert", json!({ "text": "A new paragraph." }))?;
            }
            Script::Delete => {
                // The first paragraph, with its mark: from its start to the next paragraph's.
                let next =
                    document.body.iter().enumerate().skip(first + 1).find(|(_, block)| matches!(***block, Block::Para(_))).map(|(index, _)| index);
                let Some(next) = next else { return Ok(false) };
                self.select(doc, json!({ "block": first, "off": 0 }), json!({ "block": next, "off": 0 }))?;
                self.run(doc, "text.delete", json!({}))?;
            }
            Script::Reorder { which } => {
                // Sorting paragraphs moves them (the blocks keep their identity): all of them for
                // 0, the last two for 1; descending, or ascending when that changes nothing.
                let paras = paragraphs(&document);
                let from = if *which == 0 { 0 } else { paras.len().saturating_sub(2) };
                let (Some((start, _)), Some((end, _))) = (paras.get(from), paras.last()) else { return Ok(false) };
                if start == end {
                    return Ok(false);
                }
                let (start, end) = (*start, *end);
                for descending in [true, false] {
                    let before = self.text(replica)?;
                    self.select(&mut replica.doc, json!({ "block": start, "off": 0 }), json!({ "block": end, "off": 0 }))?;
                    self.run(&mut replica.doc, "para.sort", json!({ "descending": descending }))?;
                    if self.text(replica)? != before {
                        break;
                    }
                }
                self.no_reset(&replica.doc, "Reorder")?;
                return Ok(true);
            }
            Script::Duplicate => {
                self.caret(doc, json!({ "block": first, "off": 0 }))?;
                self.run(doc, "select.paragraph", json!({}))?;
                self.run(doc, "edit.copy", json!({}))?;
                self.run(doc, "caret.docEnd", json!({}))?;
                self.run(doc, "text.newParagraph", json!({}))?;
                self.run(doc, "edit.paste", json!({}))?;
            }
            Script::InsertText { at, text } => {
                self.caret(doc, position(&document, *at))?;
                self.run(doc, "text.insert", json!({ "text": text }))?;
            }
            Script::DeleteText { at, len } => {
                self.select(doc, position(&document, *at), position(&document, at + len))?;
                self.run(doc, "text.delete", json!({}))?;
            }
            Script::SplitParagraph { at } => {
                self.caret(doc, position(&document, *at))?;
                self.run(doc, "text.newParagraph", json!({}))?;
            }
            Script::MergeParagraph { at } => {
                self.caret(doc, position(&document, at + 1))?;
                self.run(doc, "text.backspace", json!({}))?;
            }
            Script::InsertBlob { seed } => {
                let end = paragraphs(&document).first().map_or(0, |(_, text)| text.len());
                self.caret(doc, json!({ "block": first, "off": end }))?;
                self.run(doc, "insert.picture", json!({ "data": picture(*seed), "width": 40 }))?;
            }
            _ => return Ok(false),
        }
        self.no_reset(doc, &format!("{step:?}"))?;
        Ok(true)
    }

    fn text(&self, replica: &mut DocReplica) -> Result<Option<String>, String> {
        let document = document(&mut replica.doc)?;
        Ok(Some(paragraphs(&document).into_iter().map(|(_, text)| text).collect::<Vec<_>>().join("\n")))
    }

    /// The engine's document and a fresh replica's must save the same document.
    fn check_view(&self, replica: &mut DocReplica) -> Result<(), String> {
        let doc = &mut replica.doc;
        self.no_reset(doc, "the session")?;
        let snapshot = doc.snapshot()?;
        let blobs = decode_blobs(&doc.blobs()?)?;
        let fresh = self.join(&snapshot, &blobs)?;
        let read = |bytes: &[u8]| -> Result<Value, String> {
            let mut document = kdm_model::decode(NAME, bytes).map_err(|error| error.to_string())?;
            // A save stamps the file (modified time, revision): not part of the document. Fresh
            // comment paragraph ids follow the engine's local comment numbering, which differs
            // between replicas of one document.
            document.core = Default::default();
            for comment in document.comments.values_mut() {
                comment.para_id.clear();
            }
            for part in document.parts.values_mut().filter(|part| part.kind == PartKind::Comment) {
                for block in &mut part.blocks {
                    if let Block::Para(para) = Arc::make_mut(block) {
                        para.props.para_id = None;
                    }
                }
            }
            serde_json::to_value(kdm_model::normalize(&document)).map_err(|error| error.to_string())
        };
        let (mine, theirs) = (read(&doc.save(None)?)?, read(&fresh.doc.save(None)?)?);
        if mine == theirs { Ok(()) } else { Err(format!("the view saves differently from its replica:\n  view    {mine}\n  replica {theirs}")) }
    }

    fn blobs(&self, replica: &mut DocReplica) -> Result<Blobs, String> {
        decode_blobs(&replica.doc.blobs()?)
    }

    fn add_blobs(&self, replica: &mut DocReplica, blobs: &Blobs) -> Result<(), String> {
        replica.store.extend(blobs.iter().map(|(hash, bytes)| (hash.clone(), bytes.clone())));
        replica.doc.add_blobs(&encode_blobs(blobs))
    }

    fn export_file(&self, replica: &mut DocReplica) -> Result<Option<Vec<u8>>, String> {
        replica.doc.save(None).map(Some)
    }

    /// Typing one character, and painting the first page's top.
    fn keystroke(&self, replica: &mut DocReplica, rng: &mut Rng) -> Result<(), String> {
        let letter = char::from(b'a' + rng.below(26) as u8);
        self.run(&mut replica.doc, "text.insert", json!({ "text": letter.to_string() }))?;
        replica.doc.render_area(0, 0, 612, 400, 612, 400).map(|_| ())
    }
}
