//! Replica -> `Document`. First the objects every reference can name get their local ids — the
//! one [`Known`] remembers for each, else a fresh one — then the fields are read, translating every
//! reference stored as data. A list entry whose object is gone, and every object after its first
//! listing anywhere in the document, is skipped.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use kition_doc_doc::numbering::{AbstractNum, Num};
use kition_doc_doc::styles::{Style, StyleSheet};
use kition_doc_doc::table::{Cell, Row};
use kition_doc_doc::{Block, Blocks, Document, InlineObject, Numbering, Paragraph, Part, PartKind, Revision, Table};
use kition_kit::kdm::{KdmError, ObjectId, Replica, TextRun};
use kition_kit::session::{BlobRef, BlobSource};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::ids::{BlockIds, CellIds, Ids, Known, Minter, PartIds, RowIds, TableIds};
use crate::pieces::{Composition, compose, has_anchor, layout};
use crate::refs::{Locals, inline_from, para_from_meta};
use crate::text::{INLINE, ParaText, TEXT, merged, para_parts};
use crate::write::MAX_DEPTH;
use crate::{KdmResult, MediaCache, kdm_err, root_id};

/// Reads the whole document with fresh local ids. A blob `blobs` lacks (it has not arrived yet)
/// reads as empty bytes.
pub fn read_document(replica: &Replica, blobs: &dyn BlobSource) -> KdmResult<(Document, Ids)> {
    let (doc, ids, _, _) = read_with(replica, blobs, &Known::default(), &MediaCache::new())?;
    Ok((doc, ids))
}

/// Reads the whole document, giving every object `known` remembers the local id it had.
pub fn read_document_known(replica: &Replica, blobs: &dyn BlobSource, known: &Known) -> KdmResult<(Document, Ids)> {
    let (doc, ids, _, _) = read_with(replica, blobs, known, &MediaCache::new())?;
    Ok((doc, ids))
}

/// `null` (a field never written) reads as `T`'s default.
fn from<T: DeserializeOwned + Default>(value: Value) -> KdmResult<T> {
    if value.is_null() { Ok(T::default()) } else { serde_json::from_value(value).map_err(kdm_err) }
}

pub(crate) fn part_kind(name: &str) -> PartKind {
    match name {
        "footer" => PartKind::Footer,
        "footnote" => PartKind::Footnote,
        "endnote" => PartKind::Endnote,
        "textbox" => PartKind::TextBox,
        _ => PartKind::Header,
    }
}

/// `key`, or `stem_2.ext`, `stem_3.ext`… if `used` has it.
fn unique_key(key: &str, used: &HashSet<String>) -> String {
    if !used.contains(key) {
        return key.to_owned();
    }
    let (stem, ext) = key.rsplit_once('.').map_or((key, None), |(s, e)| (s, Some(e)));
    (2u32..)
        .map(|n| match ext {
            Some(ext) => format!("{stem}_{n}.{ext}"),
            None => format!("{stem}_{n}"),
        })
        .find(|k| !used.contains(k))
        .unwrap_or_else(|| format!("{key}_dup"))
}

/// A listed block, before the paragraphs' pieces are known.
enum Pending {
    Table(ObjectId),
    /// A table whose table object is gone.
    Empty,
    /// A paragraph: its register and its field's runs.
    Text(Value, Vec<TextRun>),
}

struct Reader<'r> {
    replica: &'r Replica,
    blobs: &'r dyn BlobSource,
    cache: &'r MediaCache,
    new_cache: MediaCache,
    /// Hashes of blobs the read found missing: they read as empty bytes until they arrive.
    missing: BTreeSet<String>,
    seen: HashSet<ObjectId>,
    locals: Locals,
}

/// Also answers the media bytes read (a cache for the next read) and the hashes of the blobs
/// `blobs` lacks, read as empty bytes.
pub(crate) fn read_with(
    replica: &Replica,
    blobs: &dyn BlobSource,
    known: &Known,
    cache: &MediaCache,
) -> KdmResult<(Document, Ids, MediaCache, BTreeSet<String>)> {
    let root = root_id();
    if !replica.exists(&root) {
        return Err(KdmError::NotFound(root.to_string()));
    }
    let mut r =
        Reader { replica, blobs, cache, new_cache: MediaCache::new(), missing: BTreeSet::new(), seen: HashSet::new(), locals: Locals::default() };
    let mut ids = Ids::default();

    // Local ids of everything a reference can name.
    for (i, object) in r.live(&root, "revisions")?.into_iter().enumerate() {
        r.locals.revisions.insert(object.as_str().to_owned(), u32::try_from(i).unwrap_or(u32::MAX));
        ids.revisions.push(object);
    }
    let mut part_minter = Minter::new(known.max_part);
    let mut parts = Vec::new();
    for object in r.live(&root, "parts")? {
        let id = part_minter.mint(known.parts.get(&object).copied());
        r.locals.parts.insert(object.as_str().to_owned(), id);
        parts.push((id, object));
    }
    let comment_minter = Minter::with_zero(known.max_comment);
    let mut media = Vec::new();
    let mut keys = HashSet::new();
    for object in r.live(&root, "media")? {
        let key = unique_key(replica.get(&object, "key")?.as_str().unwrap_or("image"), &keys);
        keys.insert(key.clone());
        r.locals.media.insert(object.as_str().to_owned(), key.clone());
        media.push((key, object));
    }
    let numbering = r.lists(&root, known, &mut ids)?;

    // The document's own registers.
    let mut doc = Document {
        body: Vec::new(),
        last_section: from(r.translated(&root, "lastSection")?)?,
        parts: BTreeMap::new(),
        styles: r.styles(&root)?,
        numbering,
        comments: BTreeMap::new(),
        revisions: Vec::new(),
        settings: from(replica.get(&root, "settings")?)?,
        core: from(replica.get(&root, "core")?)?,
        sources: from(replica.get(&root, "sources")?)?,
        custom_props: from(replica.get(&root, "customProps")?)?,
        media: BTreeMap::new(),
        passthrough: BTreeMap::new(),
    };
    if let Value::Object(map) = replica.get(&root, "passthrough")? {
        for (key, blob) in map {
            let bytes = r.bytes(blob, &key)?;
            doc.passthrough.insert(key, bytes);
        }
    }
    for (key, object) in media {
        let bytes = r.bytes(replica.get(&object, "blob")?, &key)?;
        doc.media.insert(key.clone(), bytes);
        ids.media.insert(key, object);
    }
    for object in &ids.revisions {
        doc.revisions.push(from::<Revision>(replica.get(object, "props")?)?);
    }

    // Blocks.
    let (body, body_ids) = r.blocks(&root, "body", 0)?;
    doc.body = body;
    ids.body = body_ids;
    for (id, object) in parts {
        let kind = part_kind(replica.get(&object, "kind")?.as_str().unwrap_or("header"));
        let (blocks, block_ids) = r.blocks(&object, "blocks", 0)?;
        doc.parts.insert(id, Part { kind, blocks });
        ids.parts.insert(id, PartIds { object, blocks: block_ids });
    }

    // Comment threads (rule M23), and their anchors placed in the paragraphs.
    let mut minters = crate::comments::Minters { comments: comment_minter, parts: part_minter };
    crate::comments::read(replica, &mut doc, &mut ids, &mut minters, known, &mut r.locals, &mut r.seen)?;
    crate::charts::redraw(&mut doc);
    Ok((doc, ids, r.new_cache, r.missing))
}

impl Reader<'_> {
    /// The styles (see `Writer::write_styles`): the `styleDefaults` register, then the `styles`
    /// entries by their order (then key). A replica whose styles were never written (a script's)
    /// reads with the built-in styles.
    fn styles(&self, root: &ObjectId) -> KdmResult<StyleSheet> {
        let mut defaults = self.translated(root, "styleDefaults")?;
        let unwritten = defaults.is_null() && self.replica.map_entries(root, "styles")?.is_empty();
        if unwritten {
            return Ok(StyleSheet::default());
        }
        let mut sheet = StyleSheet::empty();
        if let Some(map) = defaults.as_object_mut() {
            sheet.default_chr = from(map.remove("defaultChr").unwrap_or(Value::Null))?;
            sheet.default_para = from(map.remove("defaultPara").unwrap_or(Value::Null))?;
        }
        let mut entries: Vec<(f64, String, Value)> = self
            .replica
            .map_entries(root, "styles")?
            .into_iter()
            .filter_map(|(key, v)| {
                Some((v.get("order").and_then(Value::as_f64).unwrap_or(f64::MAX), key, v.get("style").filter(|s| s.is_object())?.clone()))
            })
            .collect();
        entries.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        for (_, _, mut style) in entries {
            self.locals.translate(&mut style);
            sheet.styles.push(from::<Style>(style)?);
        }
        Ok(sheet)
    }

    /// The lists (see `Writer::write_lists`): the `lists` entries by their order (then key), then
    /// the `numbering` register's rest. An abstract list is read once, from the first entry that
    /// holds it; every list and abstract list gets its local id ([`Known`]), and the lists' ids go
    /// to `self.locals` for the paragraphs that name them.
    fn lists(&mut self, root: &ObjectId, known: &Known, ids: &mut Ids) -> KdmResult<Numbering> {
        let mut entries: Vec<(f64, String, Value)> = self
            .replica
            .map_entries(root, "lists")?
            .into_iter()
            .filter(|(_, v)| v.is_object())
            .map(|(key, v)| (v.get("order").and_then(Value::as_f64).unwrap_or(f64::MAX), key, v))
            .collect();
        entries.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut rest = self.replica.get(root, "numbering")?;
        self.locals.translate(&mut rest);

        // Abstract lists first, so every list finds its own.
        let mut minter = Minter::with_zero(known.max_abstract);
        let mut keys: HashMap<String, u32> = HashMap::new();
        let mut abstracts = Vec::new();
        let mut take = |mut v: Value, abstracts: &mut Vec<AbstractNum>| -> KdmResult<()> {
            let key = v.get("id").and_then(Value::as_str).map(str::to_owned);
            let id = match &key {
                Some(key) if keys.contains_key(key) => return Ok(()),
                Some(key) => {
                    let id = minter.mint(known.abstracts.get(key).copied());
                    keys.insert(key.clone(), id);
                    ids.abstracts.insert(id, key.clone());
                    id
                }
                None => minter.mint(None),
            };
            if let Some(map) = v.as_object_mut() {
                map.insert("id".into(), Value::from(id));
            }
            abstracts.push(from::<AbstractNum>(v)?);
            Ok(())
        };
        for (_, _, entry) in &mut entries {
            self.locals.translate(entry);
            if let Some(v) = entry.get("abstract").filter(|v| v.is_object()) {
                take(v.clone(), &mut abstracts)?;
            }
        }
        if let Some(items) = rest.get("abstracts").and_then(Value::as_array) {
            for v in items.iter().filter(|v| v.is_object()) {
                take(v.clone(), &mut abstracts)?;
            }
        }

        let mut minter = Minter::new(known.max_num);
        let abstract_id = |v: &mut Value| {
            if let Some(slot) = v.get_mut("abstractId")
                && let Some(key) = slot.as_str()
            {
                // An abstract list no entry holds names nothing.
                *slot = Value::from(keys.get(key).copied().unwrap_or(u32::MAX));
            }
        };
        let mut nums = Vec::new();
        for (_, key, entry) in entries {
            let Some(mut v) = entry.get("num").filter(|v| v.is_object()).cloned() else { continue };
            let id = match self.locals.nums.get(&key) {
                Some(id) => *id,
                None => {
                    let id = minter.mint(known.nums.get(&key).copied());
                    self.locals.nums.insert(key.clone(), id);
                    ids.nums.insert(id, key);
                    id
                }
            };
            if let Some(map) = v.as_object_mut() {
                map.insert("id".into(), Value::from(id));
            }
            abstract_id(&mut v);
            nums.push(from::<Num>(v)?);
        }
        if let Some(items) = rest.get("nums").and_then(Value::as_array) {
            for v in items.iter().filter(|v| v.is_object()) {
                let mut v = v.clone();
                abstract_id(&mut v);
                nums.push(from::<Num>(v)?);
            }
        }
        Ok(Numbering { abstracts, nums })
    }

    /// The list's objects that exist and were not listed before (anywhere in this read).
    fn live(&mut self, owner: &ObjectId, field: &str) -> KdmResult<Vec<ObjectId>> {
        let items = self.replica.list(owner, field)?;
        Ok(items.into_iter().filter(|o| self.replica.exists(o) && self.seen.insert(o.clone())).collect())
    }

    fn translated(&self, object: &ObjectId, field: &str) -> KdmResult<Value> {
        let mut v = self.replica.get(object, field)?;
        self.locals.translate(&mut v);
        Ok(v)
    }

    fn bytes(&mut self, blob: Value, what: &str) -> KdmResult<Arc<Vec<u8>>> {
        if blob.is_null() {
            return Ok(Arc::new(Vec::new()));
        }
        let blob: BlobRef = serde_json::from_value(blob).map_err(kdm_err)?;
        let data = match self.cache.get(&blob.hash).or_else(|| self.new_cache.get(&blob.hash)) {
            Some(data) => data.clone(),
            None => {
                // A blob that has not arrived yet (its changes came first) reads as empty bytes,
                // not cached, so the view loads and is read again once it arrives.
                let Some(bytes) = self.blobs.get(&blob.hash) else {
                    let _ = what;
                    self.missing.insert(blob.hash);
                    return Ok(Arc::new(Vec::new()));
                };
                Arc::new(bytes.to_vec())
            }
        };
        self.new_cache.insert(blob.hash, data.clone());
        Ok(data)
    }

    fn blocks(&mut self, owner: &ObjectId, field: &str, depth: usize) -> KdmResult<(Blocks, Vec<BlockIds>)> {
        if depth > MAX_DEPTH {
            return Ok((Vec::new(), Vec::new()));
        }
        let objects = self.live(owner, field)?;
        // First every block's kind, and each text paragraph's field: a split anchor's tail shows
        // in another paragraph of the list, and a merged paragraph in the one before it (`pieces`).
        let mut pending = Vec::with_capacity(objects.len());
        let mut list = Vec::new();
        let mut fields: HashMap<ObjectId, Composition> = HashMap::new();
        for object in objects {
            let is_table = self.replica.get(&object, "kind")?.as_str() == Some("table");
            let table = if is_table {
                match self.replica.get(&object, "table")? {
                    Value::String(t) if self.replica.exists(&ObjectId::from_string(t.as_str())) => Some(ObjectId::from_string(t)),
                    _ => None,
                }
            } else {
                None
            };
            let entry = match table {
                Some(table) => Pending::Table(table),
                // A table whose table object is gone reads as an empty paragraph (written anew
                // when next written).
                None if is_table => Pending::Empty,
                None => {
                    // A register never written (a script's or another writer's paragraph made
                    // with only its text) reads as the default properties, and the text is kept.
                    let v = self.translated(&object, "para")?;
                    let runs = self.replica.text_delta(&object, TEXT)?;
                    if has_anchor(&runs) {
                        fields.insert(object.clone(), compose(&runs));
                    }
                    list.push((object.clone(), true));
                    Pending::Text(v, runs)
                }
            };
            if !matches!(entry, Pending::Text(..)) {
                list.push((object.clone(), false));
            }
            pending.push((object, entry));
        }
        let layout = layout(&list, &fields);
        if !layout.is_empty() {
            for (object, entry) in &pending {
                if let Pending::Text(_, runs) = entry {
                    fields.entry(object.clone()).or_insert_with(|| compose(runs));
                }
            }
        }

        let mut blocks = Vec::with_capacity(pending.len());
        let mut ids = Vec::with_capacity(pending.len());
        for (object, entry) in pending {
            match entry {
                Pending::Table(table) => {
                    let (t, table_ids) = self.table(table, depth)?;
                    blocks.push(Arc::new(Block::Table(t)));
                    ids.push(BlockIds::new(object, Some(table_ids), None));
                }
                Pending::Empty => {
                    blocks.push(Arc::new(Block::Para(Paragraph::new())));
                    ids.push(BlockIds::new(object, None, None));
                }
                Pending::Text(v, runs) => {
                    if layout.holder.contains_key(&object) {
                        // Merged: shown in its holder, the block before it.
                        let merged = BlockIds::new(object, None, Some(ParaText::new(runs)));
                        match ids.last_mut() {
                            Some(holder) => holder.merged.push(merged),
                            None => return Err(kdm_err("a merged paragraph without a holder")),
                        }
                        continue;
                    }
                    let p = match (layout.pieces.get(&object), fields.get(&object)) {
                        (Some(pieces), _) => {
                            let shown: Vec<TextRun> = pieces
                                .iter()
                                .flat_map(|piece| fields.get(&piece.object).map(|c| c.piece(piece.tail).to_vec()).unwrap_or_default())
                                .collect();
                            self.paragraph(&v, &merged(shown))?
                        }
                        (None, Some(c)) => self.paragraph(&v, &c.visible)?,
                        (None, None) => self.paragraph(&v, &runs)?,
                    };
                    blocks.push(Arc::new(Block::Para(p)));
                    ids.push(BlockIds::new(object, None, Some(ParaText::new(runs))));
                }
            }
        }
        Ok((blocks, ids))
    }

    /// A paragraph: its register (`meta`, translated) and its text field's runs.
    fn paragraph(&self, meta: &Value, runs: &[TextRun]) -> KdmResult<Paragraph> {
        let mut p = para_from_meta(meta)?;
        let (replica, locals) = (self.replica, &self.locals);
        let mut inline = |id: &str| -> Option<InlineObject> {
            let object = ObjectId::from_string(id);
            if !replica.exists(&object) || replica.object_type(&object).ok()?.as_str() != INLINE {
                return None;
            }
            let v = replica.get(&object, "value").ok()?;
            let v = crate::frames::join_look(v, replica.map_entries(&object, "look").unwrap_or_default());
            let v = crate::charts::join(replica, &object, v);
            let mut v = crate::frames::join(v, &replica.get(&object, "frame").unwrap_or(serde_json::Value::Null));
            locals.translate(&mut v);
            inline_from(&v).ok()
        };
        let (text, runs, objects) = para_parts(runs, locals, &mut inline);
        p.text = text;
        p.runs = runs;
        p.objects = objects;
        Ok(p)
    }

    fn table(&mut self, object: ObjectId, depth: usize) -> KdmResult<(Table, TableIds)> {
        let mut t = Table { props: from(self.translated(&object, "props")?)?, grid: Vec::new(), rows: Vec::new() };
        let cols = self.live(&object, "cols")?;
        for col in &cols {
            // A width a merge left unreadable reads as 0 (the layout sizes such a column).
            t.grid.push(self.replica.get(col, "width")?.as_f64().map_or(0.0, |w| w as f32));
        }
        let mut row_ids = Vec::new();
        for row in self.live(&object, "rows")? {
            let mut r = Row { props: from(self.translated(&row, "props")?)?, cells: Vec::new() };
            let mut cell_ids = Vec::new();
            for cell in self.live(&row, "cells")? {
                let props = from(self.translated(&cell, "props")?)?;
                let (blocks, block_ids) = self.blocks(&cell, "blocks", depth + 1)?;
                r.cells.push(Cell { props, blocks });
                cell_ids.push(CellIds { object: cell, blocks: block_ids });
            }
            t.rows.push(r);
            row_ids.push(RowIds { object: row, cells: cell_ids });
        }
        Ok((t, TableIds { object, cols, rows: row_ids }))
    }
}
