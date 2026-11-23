//! `Document` -> replica: a full write (a new replica, an import) and an incremental diff (the
//! engine command bridge) that writes only what changed. A full write is a diff from nothing.
//!
//! Parts, comments, lists and media are the same objects as long as their local key (part id,
//! comment id, list id, media key) is; revisions, which are only indexes, are aligned by content.
//! Blocks have no ids: each block list is aligned (`pair_blocks`) first by `Arc` identity (an
//! untouched block is the same `Arc`, whatever moved around it), then by equal content (a block
//! moved by copying it), then, between blocks so aligned, a changed block takes the place of the
//! one it replaced (an edit; a paragraph split keeps the first half's object); what is left is
//! inserted or deleted. Table rows and cells are aligned by content, then by place.
//!
//! An object's register is set only when its new value differs from what the replica holds. A
//! paragraph's text field is written as the characters and marks that changed (`text`), diffed
//! from what the field holds; a split's first half keeps its object, so the moved text is deleted
//! from its field and inserted into the new paragraph's, in the same transaction, after a split
//! anchor naming the new paragraph is inserted at the split point (`pieces`). A merge of two
//! paragraphs keeps both objects: the second stays listed, with its text, after a merge anchor
//! naming the first at the start of its field, and is read as part of the first (`pieces`); its
//! `BlockIds` move into the first's `merged`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use kition_doc_doc::StyleSheet;
use kition_doc_doc::numbering::{Num, Numbering};
use kition_doc_doc::para::OBJ;
use kition_doc_doc::table::{Cell, Row};
use kition_doc_doc::{Block, Document, Paragraph, PartKind, Table};
use kition_kit::kdm::{ObjectId, TextRun, Tx};
use kition_kit::session::{BlobSink, diff_text, write_rich_text};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::comments::{self, Stripped};
use crate::ids::{BlockIds, CellIds, Ids, PartIds, RowIds, TableIds};
use crate::list::{align, longest_increasing, sync_list, sync_set};
use crate::pieces::{Composition, Mark, Piece, anchor, assign, char_len, compose, has_anchor, layout, slice, text_of};
use crate::refs::{Objects, inline_value, para_meta};
use crate::text::{INLINE, OBJECT_MARK, ParaText, TEXT, kept_from, merged, para_runs};
use crate::{KdmResult, kdm_err, root_id};

/// Tables in table cells, at most this deep.
pub(crate) const MAX_DEPTH: usize = 64;

fn value(v: &impl Serialize) -> KdmResult<Value> {
    serde_json::to_value(v).map_err(kdm_err)
}

fn fields(pairs: Vec<(&str, Value)>) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

fn object_value(object: &ObjectId) -> Value {
    Value::String(object.as_str().to_owned())
}

/// A part's `kind`. A comment's text is no part of the object model (it is its `Comment`'s
/// `body`, rule M23): the writer strips those parts first (`comments::Stripped`).
pub(crate) fn kind_name(kind: PartKind) -> KdmResult<&'static str> {
    Ok(match kind {
        PartKind::Header => "header",
        PartKind::Footer => "footer",
        PartKind::Footnote => "footnote",
        PartKind::Endnote => "endnote",
        PartKind::TextBox => "textbox",
        PartKind::Comment => return Err(kdm_err("a comment's text is no part")),
    })
}

/// The MIME type of a media key, by its extension.
pub(crate) fn mime_of(key: &str) -> &'static str {
    match key.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("tif" | "tiff") => "image/tiff",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("emf") => "image/emf",
        Some("wmf") => "image/wmf",
        _ => "application/octet-stream",
    }
}

/// Writes a whole document into a fresh replica's creating transaction. Answers the `Document`
/// object's id.
pub fn write_document(tx: &mut Tx<'_>, doc: &Document, ids: &mut Ids, blobs: &mut dyn BlobSink) -> KdmResult<ObjectId> {
    let root = root_id();
    tx.create_with_id("Document", &root, Map::new())?;
    *ids = Ids::default();
    let content = Stripped::default().doc(doc);
    let mut writer = Writer::new(tx, blobs);
    writer.write(None, &content, ids)?;
    let objects = std::mem::take(&mut writer.objects);
    comments::write(tx, None, doc, ids, &objects)?;
    Ok(root)
}

/// Writes only what differs between `before` (the document `ids` describe) and `after`, and
/// leaves `ids` describing `after`. Answers whether it wrote anything.
///
/// The paragraphs are written without their comment anchors and the comment texts not as parts
/// (rule M23): the comments are written as `CommentThread` and `Comment` objects beside the
/// content (`comments`), so a change that only comments touches only those.
pub fn write_changes(tx: &mut Tx<'_>, before: &Document, after: &Document, ids: &mut Ids, blobs: &mut dyn BlobSink) -> KdmResult<bool> {
    let mut stripped = Stripped::default();
    let (content_before, content_after) = (stripped.doc(before), stripped.doc(after));
    let mut writer = Writer::new(tx, blobs);
    writer.write(Some(&content_before), &content_after, ids)?;
    let mut wrote = writer.wrote;
    let objects = std::mem::take(&mut writer.objects);
    wrote |= comments::write(tx, Some(before), after, ids, &objects)?;
    Ok(wrote)
}

struct Writer<'w, 'tx> {
    tx: &'w mut Tx<'tx>,
    blobs: &'w mut dyn BlobSink,
    /// `after`'s local ids to object ids.
    objects: Objects,
    /// Some local id names another object than it did before (a revision index after a removal):
    /// every reference must be compared, even in blocks the command did not touch.
    remap: bool,
    wrote: bool,
    /// `Inline` objects whose anchors this write removed, deleted at its end unless claimed: a
    /// field may keep an anchor another field gave up (`pieces`).
    doomed: Vec<ObjectId>,
    /// `Inline` objects a field this write wrote names.
    claimed: HashSet<ObjectId>,
}

impl<'w, 'tx> Writer<'w, 'tx> {
    fn new(tx: &'w mut Tx<'tx>, blobs: &'w mut dyn BlobSink) -> Self {
        Self { tx, blobs, objects: Objects::default(), remap: false, wrote: false, doomed: Vec::new(), claimed: HashSet::new() }
    }
}

impl Writer<'_, '_> {
    fn set(&mut self, object: &ObjectId, field: &str, v: Value) -> KdmResult<()> {
        if !same_json(&self.tx.get(object, field)?, &v) {
            self.tx.set(object, field, v)?;
            self.wrote = true;
        }
        Ok(())
    }

    fn create(&mut self, ty: &str, pairs: Vec<(&str, Value)>) -> KdmResult<ObjectId> {
        self.wrote = true;
        let pairs = pairs.into_iter().filter(|(_, v)| !v.is_null()).collect();
        self.tx.create(ty, fields(pairs))
    }

    fn write(&mut self, before: Option<&Document>, after: &Document, ids: &mut Ids) -> KdmResult<()> {
        let root = root_id();

        // Revisions: aligned by content (they are only indexes).
        let old_revs = std::mem::take(&mut ids.revisions);
        let before_revs = before.map(|b| b.revisions.as_slice()).unwrap_or(&[]);
        let partner: Vec<Option<usize>> = if before_revs.len() != old_revs.len() {
            vec![None; after.revisions.len()]
        } else if before_revs == after.revisions.as_slice() {
            (0..after.revisions.len()).map(Some).collect()
        } else {
            align(&keyless(before_revs), &keyless(&after.revisions))
        };
        let mut kept = vec![false; old_revs.len()];
        let mut revisions = Vec::with_capacity(after.revisions.len());
        for (j, rev) in after.revisions.iter().enumerate() {
            let old = partner.get(j).copied().flatten().and_then(|i| Some((i, old_revs.get(i)?, before_revs.get(i)?)));
            match old {
                Some((i, object, was)) => {
                    kept[i] = true;
                    if was != rev {
                        self.set(object, "props", value(rev)?)?;
                    }
                    revisions.push(object.clone());
                }
                None => revisions.push(self.create("Revision", vec![("props", value(rev)?)])?),
            }
        }
        for (i, object) in old_revs.iter().enumerate() {
            if !kept[i] && self.tx.exists(object) {
                self.tx.delete(object)?;
                self.wrote = true;
            }
        }
        self.remap |= !revisions.starts_with(&old_revs);

        // Parts, comments and media: the same object while the key is the same.
        let mut old_parts = std::mem::take(&mut ids.parts);
        let mut parts: BTreeMap<u32, PartIds> = BTreeMap::new();
        // Each part's tool id (C8-doc-6): what a save and a reread keep of it.
        let names = after.part_names();
        let name = |id: &u32| names.get(id).map_or(Value::Null, |name| json!(name));
        for (id, part) in &after.parts {
            let was = before.and_then(|b| b.parts.get(id));
            match (was, old_parts.remove(id)) {
                (Some(was), Some(existing)) => {
                    if was.kind != part.kind {
                        self.set(&existing.object, "kind", json!(kind_name(part.kind)?))?;
                    }
                    self.set(&existing.object, "id", name(id))?;
                    parts.insert(*id, existing);
                }
                (_, stale) => {
                    if let Some(stale) = stale {
                        self.drop_inlines(&stale.blocks, 0)?;
                        self.delete_tree(&stale.object, 0)?;
                    }
                    let mut fields = vec![("kind", json!(kind_name(part.kind)?))];
                    if names.contains_key(id) {
                        fields.push(("id", name(id)));
                    }
                    let object = self.create("Part", fields)?;
                    parts.insert(*id, PartIds { object, blocks: Vec::new() });
                }
            }
        }
        for stale in old_parts.into_values() {
            self.drop_inlines(&stale.blocks, 0)?;
            self.delete_tree(&stale.object, 0)?;
        }

        let mut old_media = std::mem::take(&mut ids.media);
        let mut media: BTreeMap<String, ObjectId> = BTreeMap::new();
        for (key, bytes) in &after.media {
            let was = before.and_then(|b| b.media.get(key));
            match (was, old_media.remove(key)) {
                (Some(was), Some(object)) => {
                    if !Arc::ptr_eq(was, bytes) && was != bytes {
                        let blob = value(&self.blobs.put(bytes, mime_of(key)))?;
                        self.set(&object, "blob", blob)?;
                    }
                    media.insert(key.clone(), object);
                }
                (_, stale) => {
                    if let Some(stale) = stale
                        && self.tx.exists(&stale)
                    {
                        self.tx.delete(&stale)?;
                    }
                    let blob = value(&self.blobs.put(bytes, mime_of(key)))?;
                    media.insert(key.clone(), self.create("Media", vec![("key", json!(key)), ("blob", blob)])?);
                }
            }
        }
        for stale in old_media.into_values() {
            if self.tx.exists(&stale) {
                self.tx.delete(&stale)?;
                self.wrote = true;
            }
        }

        // Lists (`Num`s) and abstract lists: a UUID per local id.
        let mut nums: BTreeMap<u32, String> = BTreeMap::new();
        for num in &after.numbering.nums {
            if num.id != 0 && !nums.contains_key(&num.id) {
                let uuid = ids.nums.get(&num.id).cloned().unwrap_or_else(|| ObjectId::new().as_str().to_owned());
                nums.insert(num.id, uuid);
            }
        }
        let mut abstracts: BTreeMap<u32, String> = BTreeMap::new();
        for a in &after.numbering.abstracts {
            abstracts.entry(a.id).or_insert_with(|| ids.abstracts.get(&a.id).cloned().unwrap_or_else(|| ObjectId::new().as_str().to_owned()));
        }
        let lists_changed = nums != ids.nums || abstracts != ids.abstracts;

        self.objects = Objects {
            parts: parts.iter().map(|(id, p)| (*id, p.object.as_str().to_owned())).collect(),
            // Comment anchors are not in the content (`comments`).
            comments: Default::default(),
            revisions: revisions.iter().map(|o| o.as_str().to_owned()).collect(),
            nums: nums.iter().map(|(id, uuid)| (*id, uuid.clone())).collect(),
            media: media.iter().map(|(key, o)| (key.clone(), o.as_str().to_owned())).collect(),
        };

        // The document's own registers.
        self.root_field("settings", before.map(|b| &b.settings), &after.settings, false)?;
        self.root_field("core", before.map(|b| &b.core), &after.core, false)?;
        self.root_field("sources", before.map(|b| &b.sources), &after.sources, false)?;
        self.root_field("customProps", before.map(|b| &b.custom_props), &after.custom_props, false)?;
        self.root_field("lastSection", before.map(|b| &b.last_section), &after.last_section, true)?;
        if self.remap || before.is_none_or(|b| b.styles != after.styles) {
            self.write_styles(&after.styles)?;
        }
        if self.remap || lists_changed || before.is_none_or(|b| b.numbering != after.numbering) {
            self.write_lists(&after.numbering, &nums, &abstracts, &ids.nums)?;
        }
        let passthrough_same = before.is_some_and(|b| {
            b.passthrough.len() == after.passthrough.len()
                && b.passthrough.iter().zip(&after.passthrough).all(|((k1, v1), (k2, v2))| k1 == k2 && (Arc::ptr_eq(v1, v2) || v1 == v2))
        });
        if !passthrough_same {
            // A part the write leaves as it was keeps its blob reference: its bytes may not have
            // arrived yet (read as empty), and writing them would replace the real part.
            let stored = self.tx.get(&root, "passthrough")?;
            let mut map = Map::new();
            for (key, bytes) in &after.passthrough {
                let unchanged = before.and_then(|b| b.passthrough.get(key)).is_some_and(|was| Arc::ptr_eq(was, bytes) || was == bytes);
                let kept = stored.get(key).filter(|_| unchanged).cloned();
                let blob = match kept {
                    Some(blob) => blob,
                    None => value(&self.blobs.put(bytes, "application/octet-stream"))?,
                };
                map.insert(key.clone(), blob);
            }
            self.set(&root, "passthrough", Value::Object(map))?;
        }

        // Blocks.
        let old_body = std::mem::take(&mut ids.body);
        let before_body = before.map(|b| b.body.as_slice()).unwrap_or(&[]);
        let body = self.write_blocks(&root, "body", before_body, &old_body, &after.body, 0)?;
        for (id, part) in &after.parts {
            let Some(ids) = parts.get_mut(id) else { continue };
            let old = std::mem::take(&mut ids.blocks);
            let was: &[Arc<Block>] = match before.and_then(|b| b.parts.get(id)) {
                Some(p) if !old.is_empty() || p.blocks.is_empty() => &p.blocks,
                _ => &[],
            };
            let object = ids.object.clone();
            ids.blocks = self.write_blocks(&object, "blocks", was, &old, &part.blocks, 0)?;
        }

        // The document's lists.
        let part_order: Vec<ObjectId> = parts.values().map(|p| p.object.clone()).collect();
        let media_order: Vec<ObjectId> = media.values().cloned().collect();
        self.wrote |= sync_set(self.tx, &root, "parts", &part_order)? > 0;
        self.wrote |= sync_list(self.tx, &root, "revisions", &revisions)? > 0;
        self.wrote |= sync_set(self.tx, &root, "media", &media_order)? > 0;

        self.flush_inlines()?;
        // The comments' objects are `comments::write`'s.
        ids.body = body;
        ids.parts = parts;
        ids.revisions = revisions;
        ids.nums = nums;
        ids.abstracts = abstracts;
        ids.media = media;
        Ok(())
    }

    /// Writes the lists: each `Num` that has a list id of its own, with its abstract list, as the
    /// `lists` entry of its UUID (`{order, num, abstract}`: its place among the lists, the `Num`
    /// without its id, the abstract list), so lists made concurrently are all kept; an abstract
    /// list several `Num`s use is in each of their entries, and the reader keeps the first. The
    /// rest (abstract lists no list uses, `Num`s without an id of their own) is the `numbering`
    /// register. Abstract list ids are written as their UUIDs. Entries are compared with what the
    /// replica holds, so a copy a merge left stale is written again.
    fn write_lists(
        &mut self,
        numbering: &Numbering,
        nums: &BTreeMap<u32, String>,
        abstracts: &BTreeMap<u32, String>,
        old: &BTreeMap<u32, String>,
    ) -> KdmResult<()> {
        let root = root_id();
        let mut first: HashMap<u32, usize> = HashMap::new();
        for (k, a) in numbering.abstracts.iter().enumerate() {
            first.entry(a.id).or_insert(k);
        }
        let abstract_value = |k: usize| -> KdmResult<Value> {
            let Some(a) = numbering.abstracts.get(k) else { return Ok(Value::Null) };
            let mut v = value(a)?;
            let key = if first.get(&a.id) == Some(&k) { abstracts.get(&a.id).map_or(Value::Null, |u| Value::String(u.clone())) } else { Value::Null };
            if let Some(map) = v.as_object_mut() {
                map.insert("id".into(), key);
            }
            Ok(v)
        };
        let abstract_ref = |num: &Num, v: &mut Value| {
            if let (Some(uuid), Some(map)) = (abstracts.get(&num.abstract_id), v.as_object_mut()) {
                map.insert("abstractId".into(), Value::String(uuid.clone()));
            }
        };
        let mut used: HashSet<usize> = HashSet::new();
        let mut keyed: HashSet<u32> = HashSet::new();
        let mut rest_nums = Vec::new();
        for (i, num) in numbering.nums.iter().enumerate() {
            let mut v = value(num)?;
            abstract_ref(num, &mut v);
            let key = nums.get(&num.id).filter(|_| num.id != 0 && keyed.insert(num.id));
            let Some(key) = key else {
                rest_nums.push(v);
                continue;
            };
            if let Some(map) = v.as_object_mut() {
                map.remove("id");
            }
            let own = first.get(&num.abstract_id).copied();
            used.extend(own);
            let mut entry = json!({ "order": i, "num": v, "abstract": own.map_or(Ok(Value::Null), abstract_value)? });
            self.objects.translate(&mut entry);
            if !same_json(&self.tx.map_get(&root, "lists", key)?, &entry) {
                self.tx.map_set(&root, "lists", key, entry)?;
                self.wrote = true;
            }
        }
        let live: HashSet<&String> = nums.values().collect();
        for key in old.values() {
            if !live.contains(key) && !self.tx.map_get(&root, "lists", key)?.is_null() {
                self.tx.map_delete(&root, "lists", key)?;
                self.wrote = true;
            }
        }
        let rest_abstracts = (0..numbering.abstracts.len()).filter(|k| !used.contains(k)).map(abstract_value).collect::<KdmResult<Vec<_>>>()?;
        let mut rest = json!({ "abstracts": rest_abstracts, "nums": rest_nums });
        self.objects.translate(&mut rest);
        self.set(&root, "numbering", rest)
    }

    /// Writes the styles: the document defaults as the `styleDefaults` register, each style as the
    /// `styles` entry of its style id (`{order, style}`: its place among the styles, the style), so
    /// styles made or changed concurrently are all kept; a style whose id an earlier one already
    /// has is keyed `<id>#<n>`. An entry keeps the order it holds while that still sorts it after
    /// the one before it, so adding or removing a style rewrites no other entry. References are
    /// written as object ids; entries are compared with what the replica holds, so a copy a merge
    /// left stale is written again.
    fn write_styles(&mut self, sheet: &StyleSheet) -> KdmResult<()> {
        let root = root_id();
        let mut defaults = json!({ "defaultChr": value(&sheet.default_chr)?, "defaultPara": value(&sheet.default_para)? });
        self.objects.translate(&mut defaults);
        self.set(&root, "styleDefaults", defaults)?;

        let stored: HashMap<String, Value> = self.tx.map_entries(&root, "styles")?.into_iter().collect();
        let mut used: HashSet<String> = HashSet::new();
        let mut keys: Vec<String> = Vec::with_capacity(sheet.styles.len());
        for style in &sheet.styles {
            let mut key = style.id.clone();
            let mut n = 1u32;
            while !used.insert(key.clone()) {
                n = n.saturating_add(1);
                key = format!("{}#{n}", style.id);
            }
            keys.push(key);
        }
        let orders = style_orders(&keys, |key| stored.get(key).and_then(|v| v.get("order")).and_then(Value::as_f64).filter(|o| o.is_finite()));
        for ((style, key), order) in sheet.styles.iter().zip(&keys).zip(orders) {
            let mut entry = json!({ "order": order, "style": value(style)? });
            self.objects.translate(&mut entry);
            if !stored.get(key).is_some_and(|had| same_json(had, &entry)) {
                self.tx.map_set(&root, "styles", key, entry)?;
                self.wrote = true;
            }
        }
        for key in stored.keys().filter(|key| !used.contains(*key)) {
            self.tx.map_delete(&root, "styles", key)?;
            self.wrote = true;
        }
        Ok(())
    }

    fn root_field<T: Serialize + PartialEq>(&mut self, field: &str, before: Option<&T>, after: &T, refs: bool) -> KdmResult<()> {
        if before == Some(after) && !(refs && self.remap) {
            return Ok(());
        }
        let mut v = value(after)?;
        if refs {
            self.objects.translate(&mut v);
        }
        self.set(&root_id(), field, v)
    }

    /// Deletes `object` and everything under it.
    fn delete_tree(&mut self, object: &ObjectId, depth: usize) -> KdmResult<()> {
        if depth > MAX_DEPTH * 3 + 4 || !self.tx.exists(object) {
            return Ok(());
        }
        let ty = self.tx.object_type(object)?;
        let lists: &[&str] = match ty.as_str() {
            "Table" => &["cols", "rows"],
            "Row" => &["cells"],
            "Cell" | "Part" => &["blocks"],
            _ => &[],
        };
        for field in lists {
            for item in self.tx.list_items(object, field)? {
                self.delete_tree(&item, depth + 1)?;
            }
        }
        if ty == "Block"
            && let Value::String(table) = self.tx.get(object, "table")?
        {
            self.delete_tree(&ObjectId::from_string(table), depth + 1)?;
        }
        self.tx.delete(object)?;
        self.wrote = true;
        Ok(())
    }

    /// Writes `after` (one block list: the body, a part's, a cell's) over `before` (whose objects
    /// are `old`) and makes `owner.field` list it. Answers `after`'s objects.
    ///
    /// A text paragraph's view may show more than its own field's text: the tails of split
    /// anchors (`pieces`). Its edit is written onto the fields its pieces live in, and a split
    /// that keeps the first half's object leaves a split anchor naming the second half.
    fn write_blocks(
        &mut self,
        owner: &ObjectId,
        field: &str,
        before: &[Arc<Block>],
        old: &[BlockIds],
        after: &[Arc<Block>],
        depth: usize,
    ) -> KdmResult<Vec<BlockIds>> {
        if depth > MAX_DEPTH {
            return Err(kdm_err("tables nest too deep to write"));
        }
        // A view that does not match the objects it was read with is written anew.
        let consistent = before.len() == old.len();
        let partner = if consistent { pair_blocks(before, after) } else { vec![None; after.len()] };
        let mut paired_to: Vec<Option<usize>> = vec![None; old.len()];
        for (j, i) in partner.iter().enumerate() {
            if let Some(slot) = i.and_then(|i| paired_to.get_mut(i)) {
                *slot = Some(j);
            }
        }
        // The blocks `after` adds get their objects first: a split anchor names one.
        let mut out: Vec<Option<BlockIds>> = Vec::with_capacity(after.len());
        for (j, block) in after.iter().enumerate() {
            out.push(match partner.get(j).copied().flatten() {
                Some(_) => None,
                None => Some(self.create_block(block, depth)?),
            });
        }

        let mut fields = ListFields::new(old);
        // Backspace at a paragraph's start (or Delete at the end of the one before it): the second
        // paragraph is kept, merged into the first (`pieces`), when the first's new text is both
        // texts. Keyed by the first's index.
        let mut merges: BTreeMap<usize, usize> = BTreeMap::new();
        for i in 1..old.len() {
            let (Some(Some(j)), Some(None)) = (paired_to.get(i - 1), paired_to.get(i)) else { continue };
            let (Some(Block::Para(first)), Some(Block::Para(second)), Some(Block::Para(now))) =
                (before.get(i - 1).map(|b| &**b), before.get(i).map(|b| &**b), after.get(*j).map(|b| &**b))
            else {
                continue;
            };
            if fields.is_text(fields.flat(i - 1))
                && fields.is_text(fields.flat(i))
                && !second.text.is_empty()
                && now.text.len() == first.text.len() + second.text.len()
                && now.text.starts_with(&first.text)
                && now.text.ends_with(&second.text)
            {
                merges.insert(i - 1, i);
            }
        }
        let merging: HashSet<usize> = merges.values().copied().collect();
        // A text paragraph's field goes when its block does, or becomes a table, and with it the
        // paragraphs merged into it.
        let goes = |i: usize| !merging.contains(&i) && paired_to[i].is_none_or(|j| !matches!(after.get(j).map(|b| &**b), Some(Block::Para(_))));
        let mut gone: HashSet<ObjectId> = HashSet::new();
        for (i, ids) in old.iter().enumerate() {
            if goes(i) && fields.is_text(fields.flat(i)) {
                gone.insert(ids.object.clone());
                gone.extend(ids.merged.iter().map(|m| m.object.clone()));
            }
        }
        // A deleted paragraph's view took the tails it showed with it.
        for (i, ids) in old.iter().enumerate() {
            if !gone.contains(&ids.object) {
                continue;
            }
            let own = fields.flat(i);
            for (k, tail) in fields.pieces(i) {
                if k == own || gone.contains(fields.object(k)) {
                    continue;
                }
                let Some(t) = tail else { continue };
                if let Some(slot) = fields.now(k).tails.get_mut(t) {
                    let cleared = std::mem::take(&mut slot.1);
                    self.doom_runs(&cleared);
                }
            }
        }

        // Paired blocks: tables (and a block whose table is gone) as they are; a text paragraph's
        // new runs onto its pieces (and those of the paragraph merged into it).
        let mut splits: HashMap<usize, (usize, ObjectId)> = HashMap::new();
        let mut merge_marks: HashMap<usize, ObjectId> = HashMap::new();
        for (j, block) in after.iter().enumerate() {
            let Some(i) = partner.get(j).copied().flatten() else { continue };
            let (Some(was), Some(ids)) = (before.get(i), old.get(i)) else { continue };
            let unchanged = Arc::ptr_eq(was, block) && !self.remap;
            let Block::Para(p) = &**block else {
                out[j] = Some(if unchanged { ids.clone() } else { self.update_block(ids, was, block, depth)? });
                continue;
            };
            let fi = fields.flat(i);
            if !fields.is_text(fi) {
                out[j] = Some(if unchanged { ids.clone() } else { self.update_block(ids, was, block, depth)? });
                continue;
            }
            let mut parts = fields.pieces(i);
            if let Some(&second) = merges.get(&i) {
                parts.extend(fields.pieces(second));
                let fs = fields.flat(second);
                fields.now(fs);
                merge_marks.insert(fs, ids.object.clone());
            }
            let losing: Vec<bool> = parts.iter().map(|(k, _)| gone.contains(fields.object(*k))).collect();
            let lost = losing.contains(&true);
            out[j] = Some(ids.clone());
            if unchanged && !lost {
                continue;
            }
            let object = ids.object.clone();
            let meta = self.objects.translated(para_meta(p)?);
            self.set(&object, "para", meta)?;
            self.set(&object, "kind", json!("para"))?;
            self.set(&object, "id", block_tool_id(block))?;
            let olds: Vec<Vec<TextRun>> = parts.iter().map(|(k, t)| fields.was(*k).piece(*t).to_vec()).collect();
            let old_view = merged(olds.iter().flatten().cloned().collect());
            let new_view = self.view_runs(&old_view, p)?;
            if new_view == old_view && !lost {
                continue;
            }
            let old_text = text_of(&old_view);
            let lens: Vec<usize> = olds.iter().map(|runs| char_len(runs)).collect();
            for ((k, t), runs) in parts.iter().zip(assign(&old_text, &lens, &losing, &new_view)) {
                if gone.contains(fields.object(*k)) {
                    continue;
                }
                let c = fields.now(*k);
                match t {
                    None => c.visible = runs,
                    Some(t) => {
                        if let Some(slot) = c.tails.get_mut(*t) {
                            slot.1 = runs;
                        }
                    }
                }
            }
            // Enter inside this paragraph's own text, its object kept for the first half: a split
            // anchor at the split point, so text typed concurrently after it follows the second
            // half (`pieces`).
            let target = out.get(j + 1).and_then(Option::as_ref).filter(|b| b.text.is_some()).map(|b| b.object.clone());
            if let (Some(next), Some(target), Some(None)) = (after.get(j + 1), target, partner.get(j + 1))
                && let Block::Para(next) = &**next
            {
                let at = p.text.chars().count();
                let own = char_len(&fields.was(fi).visible);
                if at > 0 && !next.text.is_empty() && at < own && old_text == format!("{}{}", p.text, next.text) {
                    splits.insert(fi, (at, target));
                }
            }
        }

        for (i, ids) in old.iter().enumerate() {
            if paired_to[i].is_none() && !merging.contains(&i) {
                self.delete_block(ids, depth)?;
            } else if ids.merged.first().is_some_and(|m| gone.contains(&m.object)) {
                // Became a table: the paragraphs merged into it go.
                for merged in &ids.merged {
                    self.delete_block(merged, depth)?;
                }
            }
        }
        // Each changed field: written where it differs from what it held.
        let mut texts: HashMap<usize, ParaText> = HashMap::new();
        for (f, mut c) in fields.take_changed() {
            let Some(ids) = fields.ids(f).filter(|ids| !gone.contains(&ids.object)) else { continue };
            let object = &ids.object;
            let tx = &*self.tx;
            c.tails.retain(|(mark, tail)| !tail.is_empty() || !gone.contains(&mark.target) && tx.exists(&mark.target));
            let truth = ids.text.as_ref().map(ParaText::runs).unwrap_or_default();
            let mark = match (splits.get(&f), merge_marks.get(&f)) {
                (Some((at, target)), _) => Some((*at, Mark::split(target))),
                (None, Some(target)) => Some((0, Mark::merge(target))),
                (None, None) => None,
            };
            let new_truth = match mark {
                Some((at, mark)) => {
                    // The anchor first, between two characters still there (a merge anchor at the
                    // start), then the rest.
                    let mut with = slice(truth, 0, at);
                    with.push(anchor(&mark));
                    with.extend(slice(truth, at, char_len(truth)));
                    let with = merged(with);
                    write_rich_text(self.tx, object, TEXT, truth, &with).map_err(kdm_err)?;
                    // A split's tail starts empty (the moved text goes); a merge's is everything
                    // the merged paragraph showed of its own.
                    let tail = if mark.merge { std::mem::take(&mut c.visible) } else { Vec::new() };
                    c.tails.insert(0, (mark, tail));
                    let new_truth = c.runs();
                    write_rich_text(self.tx, object, TEXT, &with, &new_truth).map_err(kdm_err)?;
                    self.wrote = true;
                    new_truth
                }
                None => {
                    let new_truth = c.runs();
                    if new_truth.as_slice() != truth {
                        write_rich_text(self.tx, object, TEXT, truth, &new_truth).map_err(kdm_err)?;
                        self.wrote = true;
                    }
                    new_truth
                }
            };
            texts.insert(f, ParaText::new(new_truth));
        }
        // Each paired paragraph's merged paragraphs: those it had, then the one merged into it now
        // and those that one had.
        let mut result: Vec<BlockIds> = Vec::with_capacity(after.len());
        for (ids, i) in out.into_iter().zip(&partner) {
            let Some(mut ids) = ids else { continue };
            if let Some(i) = *i {
                let fi = fields.flat(i);
                if let Some(text) = texts.remove(&fi) {
                    ids.text = Some(text);
                }
                let mut flats: Vec<usize> = fields.merged_of(i).collect();
                if let Some(&second) = merges.get(&i) {
                    flats.push(fields.flat(second));
                    flats.extend(fields.merged_of(second));
                }
                ids.merged = Vec::with_capacity(flats.len());
                for f in flats {
                    let Some(m) = fields.ids(f).filter(|m| !gone.contains(&m.object)) else { continue };
                    let mut m = BlockIds::new(m.object.clone(), None, m.text.clone());
                    if let Some(text) = texts.remove(&f) {
                        m.text = Some(text);
                    }
                    ids.merged.push(m);
                }
            }
            result.push(ids);
        }
        let order: Vec<ObjectId> =
            result.iter().flat_map(|b| std::iter::once(&b.object).chain(b.merged.iter().map(|m| &m.object))).cloned().collect();
        let same_order = consistent && fields.objects().eq(order.iter());
        if !same_order {
            self.wrote |= sync_list(self.tx, owner, field, &order)? > 0;
        }
        Ok(result)
    }

    /// Marks for deletion the `Inline` objects `runs`' anchors name (`flush_inlines`).
    fn doom_runs(&mut self, runs: &[TextRun]) {
        for run in runs {
            if let Some(id) = run.attributes.get(OBJECT_MARK).and_then(Value::as_str) {
                self.doomed.push(ObjectId::from_string(id));
            }
        }
    }

    /// Deletes the `Inline` objects this write let go and no field kept.
    fn flush_inlines(&mut self) -> KdmResult<()> {
        for id in std::mem::take(&mut self.doomed) {
            if !self.claimed.contains(&id) && self.is_inline(&id) {
                crate::charts::delete(self.tx, &id)?;
                self.tx.delete(&id)?;
                self.wrote = true;
            }
        }
        Ok(())
    }

    fn create_block(&mut self, block: &Block, depth: usize) -> KdmResult<BlockIds> {
        match block {
            Block::Para(p) => {
                let meta = self.objects.translated(para_meta(p)?);
                let object = self.create("Block", vec![("id", block_tool_id(block)), ("kind", json!("para")), ("para", meta)])?;
                let text = self.write_text_field(&object, &[], p)?;
                Ok(BlockIds::new(object, None, Some(text)))
            }
            Block::Table(t) => {
                let table = self.create_table(t, depth)?;
                let object =
                    self.create("Block", vec![("id", block_tool_id(block)), ("kind", json!("table")), ("table", object_value(&table.object))])?;
                Ok(BlockIds::new(object, Some(table), None))
            }
        }
    }

    fn update_block(&mut self, ids: &BlockIds, before: &Block, after: &Block, depth: usize) -> KdmResult<BlockIds> {
        let object = ids.object.clone();
        match after {
            Block::Para(p) => {
                let (None, Some(truth)) = (&ids.table, &ids.text) else {
                    // A table becoming a paragraph (or a block whose table is gone): its object's
                    // text field holds a table's leftovers, so the paragraph is a new object in
                    // its place, with an id of its own.
                    let fresh = self.create_block(after, depth)?;
                    self.delete_block(ids, depth)?;
                    return Ok(fresh);
                };
                let text = self.write_text_field(&object, truth.runs(), p)?;
                let meta = self.objects.translated(para_meta(p)?);
                self.set(&object, "para", meta)?;
                self.set(&object, "kind", json!("para"))?;
                self.set(&object, "id", block_tool_id(after))?;
                Ok(BlockIds { object, table: None, text: Some(text), merged: ids.merged.clone() })
            }
            Block::Table(t) => {
                let table = match (before, &ids.table) {
                    (Block::Table(was), Some(table)) => self.update_table(table, was, t, depth)?,
                    _ => {
                        let table = self.create_table(t, depth)?;
                        if let Some(text) = &ids.text {
                            // The paragraph's text and objects go; the field stays, unread.
                            self.delete_inlines(text)?;
                            let len = self.tx.text_len(&object, TEXT)?;
                            if len > 0 {
                                self.tx.text_delete(&object, TEXT, 0, len)?;
                            }
                        }
                        self.set(&object, "table", object_value(&table.object))?;
                        self.set(&object, "para", Value::Null)?;
                        self.set(&object, "kind", json!("table"))?;
                        table
                    }
                };
                self.set(&object, "id", block_tool_id(after))?;
                Ok(BlockIds::new(object, Some(table), None))
            }
        }
    }

    /// Writes paragraph `p`'s text and run properties over its text field, which holds `truth`
    /// (see `text`, [`Writer::view_runs`]). Answers what the field holds now.
    fn write_text_field(&mut self, object: &ObjectId, truth: &[TextRun], p: &Paragraph) -> KdmResult<ParaText> {
        let runs = self.view_runs(truth, p)?;
        if runs.as_slice() != truth {
            write_rich_text(self.tx, object, TEXT, truth, &runs).map_err(kdm_err)?;
            self.wrote = true;
        }
        Ok(ParaText::new(runs))
    }

    /// Paragraph `p`'s text and run properties as runs, over `truth` (the runs its view showed):
    /// inline objects kept with the anchors the diff keeps (their `value` updated), made for new
    /// anchors, and those of removed anchors let go (deleted at the end of the write unless a
    /// field keeps them).
    fn view_runs(&mut self, truth: &[TextRun], p: &Paragraph) -> KdmResult<Vec<TextRun>> {
        let old_text: String = truth.iter().map(|r| r.insert.as_str()).collect();
        let mut old_inline: Vec<Option<&str>> = Vec::with_capacity(old_text.len());
        for run in truth {
            let id = run.attributes.get(OBJECT_MARK).and_then(Value::as_str);
            old_inline.extend(run.insert.chars().map(|c| if c == OBJ { id } else { None }));
        }
        let new_len = p.text.chars().count();
        let edits = diff_text(&old_text, &p.text);
        let kept = kept_from(old_inline.len(), new_len, &edits);
        let mut used: HashSet<String> = HashSet::new();
        let mut inlines: Vec<ObjectId> = Vec::with_capacity(p.objects.len());
        let mut k = 0usize;
        for (j, c) in p.text.chars().enumerate() {
            if c != OBJ {
                continue;
            }
            let Some(o) = p.objects.get(k) else { break };
            k += 1;
            let value = self.objects.translated(inline_value(o)?);
            let reuse = kept
                .get(j)
                .copied()
                .flatten()
                .and_then(|i| old_inline.get(i).copied().flatten())
                .filter(|id| !used.contains(*id))
                .map(ObjectId::from_string)
                .filter(|id| self.is_inline(id));
            // A shape's frame is its own register, so a move and a format both stay; an edited
            // chart's description its own object.
            let (value, chart) = crate::charts::split(value);
            let (value, frame) = crate::frames::split(value);
            // A shape's other members are entries of its `look` map, so two people changing two
            // of them both keep theirs.
            let shape = crate::frames::is_plain_shape(&value);
            let id = match reuse {
                Some(id) => {
                    if shape {
                        self.wrote |= crate::frames::write_look(self.tx, &id, &value)?;
                    } else {
                        self.wrote |= crate::frames::clear_look(self.tx, &id)?;
                        self.set(&id, "value", value)?;
                    }
                    if let Some(frame) = frame {
                        self.set(&id, "frame", frame)?;
                    }
                    id
                }
                None => {
                    let mut fields = vec![("value", if shape { serde_json::json!({ "type": "shape" }) } else { value.clone() })];
                    if let Some(frame) = frame {
                        fields.push(("frame", frame));
                    }
                    let id = self.create_inline(o.tool_uid().filter(|uid| *uid > 0), fields)?;
                    if shape {
                        crate::frames::write_look(self.tx, &id, &value)?;
                    }
                    id
                }
            };
            // A drawing object's, picture's or equation's number is its tool id (C8-land-14-doc, C8-doc-6):
            // `drawing.*` take a drawing object's.
            self.set(&id, "uid", o.tool_uid().filter(|uid| *uid > 0).map_or(Value::Null, |uid| serde_json::json!(uid)))?;
            if crate::charts::write(self.tx, &id, chart.as_ref())? {
                self.wrote = true;
            }
            used.insert(id.as_str().to_owned());
            self.claimed.insert(id.clone());
            inlines.push(id);
        }
        for id in old_inline.iter().flatten() {
            if !used.contains(*id) {
                self.doomed.push(ObjectId::from_string(*id));
            }
        }
        para_runs(p, &inlines, &self.objects)
    }

    /// A new `Inline`: a drawing object with a number gets the object id that number names
    /// ([`inline_id`]), so two reads of one file give it the same object id and a reread pairs it
    /// (C8-doc-4: nothing owning holds an `Inline`, its paragraph's text names it in a mark, so a
    /// minted id paired it by equal content alone, and another program's rotation of it read as
    /// nothing). An id used before (and deleted) or held by another object: any id will do.
    fn create_inline(&mut self, uid: Option<u32>, pairs: Vec<(&str, Value)>) -> KdmResult<ObjectId> {
        let Some(uid) = uid else { return self.create(INLINE, pairs) };
        self.wrote = true;
        let pairs: Vec<(&str, Value)> = pairs.into_iter().filter(|(_, v)| !v.is_null()).collect();
        let id = inline_id(uid);
        if !self.tx.exists(&id)
            && let Ok(object) = self.tx.create_with_id(INLINE, &id, fields(pairs.clone()))
        {
            return Ok(object);
        }
        self.tx.create(INLINE, fields(pairs))
    }

    fn is_inline(&self, id: &ObjectId) -> bool {
        self.tx.exists(id) && self.tx.object_type(id).is_ok_and(|ty| ty == INLINE)
    }

    /// Lets go of the `Inline` objects a text field's anchors name (deleted at the end of the
    /// write unless a field keeps them).
    fn delete_inlines(&mut self, text: &ParaText) -> KdmResult<()> {
        self.doomed.extend(text.inlines());
        Ok(())
    }

    /// Deletes the `Inline` objects of every paragraph in `blocks`, tables descended.
    fn drop_inlines(&mut self, blocks: &[BlockIds], depth: usize) -> KdmResult<()> {
        if depth > MAX_DEPTH {
            return Ok(());
        }
        for block in blocks {
            if let Some(text) = &block.text {
                self.delete_inlines(text)?;
            }
            self.drop_inlines(&block.merged, depth)?;
            if let Some(table) = &block.table {
                for row in &table.rows {
                    self.drop_row_inlines(row, depth)?;
                }
            }
        }
        Ok(())
    }

    fn drop_row_inlines(&mut self, row: &RowIds, depth: usize) -> KdmResult<()> {
        for cell in &row.cells {
            self.drop_inlines(&cell.blocks, depth + 1)?;
        }
        Ok(())
    }

    /// Deletes a block, everything under it, the paragraphs merged into it, and their inline
    /// objects.
    fn delete_block(&mut self, ids: &BlockIds, depth: usize) -> KdmResult<()> {
        self.drop_inlines(std::slice::from_ref(ids), depth)?;
        for merged in &ids.merged {
            self.delete_tree(&merged.object, 0)?;
        }
        self.delete_tree(&ids.object, 0)
    }

    fn create_table(&mut self, t: &Table, depth: usize) -> KdmResult<TableIds> {
        let props = self.objects.translated(value(&t.props)?);
        let object = self.create("Table", vec![("props", props)])?;
        let cols = self.write_cols(&object, &[], &[], &t.grid)?;
        let mut rows = Vec::with_capacity(t.rows.len());
        for (r, row) in t.rows.iter().enumerate() {
            let ids = self.create_row(row, depth)?;
            self.tx.list_insert(&object, "rows", r, &ids.object)?;
            rows.push(ids);
        }
        Ok(TableIds { object, cols, rows })
    }

    /// Writes a table's grid (`after`, its column widths) as its `cols`, over `before` (whose
    /// `Col` objects are `old`): a column keeps its object while it is the same column (all of
    /// them when the count is the same, else aligned by width, then in order), so two people
    /// resizing two columns at once both keep theirs. Answers the columns' objects.
    fn write_cols(&mut self, table: &ObjectId, old: &[ObjectId], before: &[f32], after: &[f32]) -> KdmResult<Vec<ObjectId>> {
        let partner: Vec<Option<usize>> = if before.len() != old.len() {
            vec![None; after.len()]
        } else if before.len() == after.len() {
            (0..after.len()).map(Some).collect()
        } else {
            align(&keyless(before), &keyless(after))
        };
        let mut kept = vec![false; old.len()];
        let mut cols = Vec::with_capacity(after.len());
        for (j, w) in after.iter().enumerate() {
            let width = col_width(*w);
            match partner.get(j).copied().flatten().and_then(|i| Some((i, old.get(i)?))).filter(|(_, o)| self.tx.exists(o)) {
                Some((i, object)) => {
                    kept[i] = true;
                    self.set(object, "width", width)?;
                    cols.push(object.clone());
                }
                None => cols.push(self.create("Col", vec![("width", width)])?),
            }
        }
        for (i, object) in old.iter().enumerate() {
            if !kept[i] && self.tx.exists(object) {
                self.tx.delete(object)?;
                self.wrote = true;
            }
        }
        if cols.as_slice() != old {
            self.wrote |= sync_list(self.tx, table, "cols", &cols)? > 0;
        }
        Ok(cols)
    }

    fn create_row(&mut self, row: &Row, depth: usize) -> KdmResult<RowIds> {
        let props = self.objects.translated(value(&row.props)?);
        let object = self.create("Row", vec![("id", row_tool_id(row)), ("props", props)])?;
        let mut cells = Vec::with_capacity(row.cells.len());
        for (c, cell) in row.cells.iter().enumerate() {
            let ids = self.create_cell(cell, depth)?;
            self.tx.list_insert(&object, "cells", c, &ids.object)?;
            cells.push(ids);
        }
        Ok(RowIds { object, cells })
    }

    fn create_cell(&mut self, cell: &Cell, depth: usize) -> KdmResult<CellIds> {
        let props = self.objects.translated(value(&cell.props)?);
        let object = self.create("Cell", vec![("props", props)])?;
        let blocks = self.write_blocks(&object, "blocks", &[], &[], &cell.blocks, depth + 1)?;
        Ok(CellIds { object, blocks })
    }

    fn update_table(&mut self, ids: &TableIds, before: &Table, after: &Table, depth: usize) -> KdmResult<TableIds> {
        let object = ids.object.clone();
        if self.remap || before.props != after.props {
            let props = self.objects.translated(value(&after.props)?);
            self.set(&object, "props", props)?;
        }
        let cols = if before.grid == after.grid && before.grid.len() == ids.cols.len() {
            ids.cols.clone()
        } else {
            self.write_cols(&object, &ids.cols, &before.grid, &after.grid)?
        };
        let partner: Vec<Option<usize>> = if before.rows.len() != ids.rows.len() {
            vec![None; after.rows.len()]
        } else if before.rows == after.rows {
            (0..after.rows.len()).map(Some).collect()
        } else {
            align(&keyless(&before.rows), &keyless(&after.rows))
        };
        let mut kept = vec![false; ids.rows.len()];
        let mut rows = Vec::with_capacity(after.rows.len());
        for (j, row) in after.rows.iter().enumerate() {
            let old = partner.get(j).copied().flatten().and_then(|i| Some((i, before.rows.get(i)?, ids.rows.get(i)?)));
            rows.push(match old {
                Some((i, was, row_ids)) => {
                    kept[i] = true;
                    if !self.remap && was == row { row_ids.clone() } else { self.update_row(row_ids, was, row, depth)? }
                }
                None => self.create_row(row, depth)?,
            });
        }
        for (i, row_ids) in ids.rows.iter().enumerate() {
            if !kept[i] {
                self.drop_row_inlines(row_ids, depth)?;
                self.delete_tree(&row_ids.object, 0)?;
            }
        }
        let order: Vec<ObjectId> = rows.iter().map(|r| r.object.clone()).collect();
        if order.len() != ids.rows.len() || order.iter().zip(&ids.rows).any(|(a, b)| *a != b.object) {
            self.wrote |= sync_list(self.tx, &object, "rows", &order)? > 0;
        }
        Ok(TableIds { object, cols, rows })
    }

    fn update_row(&mut self, ids: &RowIds, before: &Row, after: &Row, depth: usize) -> KdmResult<RowIds> {
        let object = ids.object.clone();
        if self.remap || before.props != after.props {
            let props = self.objects.translated(value(&after.props)?);
            self.set(&object, "props", props)?;
            self.set(&object, "id", row_tool_id(after))?;
        }
        let partner: Vec<Option<usize>> = if before.cells.len() != ids.cells.len() {
            vec![None; after.cells.len()]
        } else if before.cells.len() == after.cells.len() {
            // Same count: cells keep their places (an edit, not a column insert and delete).
            (0..after.cells.len()).map(Some).collect()
        } else {
            align(&keyless(&before.cells), &keyless(&after.cells))
        };
        let mut kept = vec![false; ids.cells.len()];
        let mut cells = Vec::with_capacity(after.cells.len());
        for (j, cell) in after.cells.iter().enumerate() {
            let old = partner.get(j).copied().flatten().and_then(|i| Some((i, before.cells.get(i)?, ids.cells.get(i)?)));
            cells.push(match old {
                Some((i, was, cell_ids)) => {
                    kept[i] = true;
                    if !self.remap && was == cell {
                        cell_ids.clone()
                    } else {
                        let object = cell_ids.object.clone();
                        if self.remap || was.props != cell.props {
                            let props = self.objects.translated(value(&cell.props)?);
                            self.set(&object, "props", props)?;
                        }
                        let blocks = self.write_blocks(&object, "blocks", &was.blocks, &cell_ids.blocks, &cell.blocks, depth + 1)?;
                        CellIds { object, blocks }
                    }
                }
                None => self.create_cell(cell, depth)?,
            });
        }
        for (i, cell_ids) in ids.cells.iter().enumerate() {
            if !kept[i] {
                self.drop_inlines(&cell_ids.blocks, depth + 1)?;
                self.delete_tree(&cell_ids.object, 0)?;
            }
        }
        let order: Vec<ObjectId> = cells.iter().map(|c| c.object.clone()).collect();
        if order.len() != ids.cells.len() || order.iter().zip(&ids.cells).any(|(a, b)| *a != b.object) {
            self.wrote |= sync_list(self.tx, &object, "cells", &order)? > 0;
        }
        Ok(RowIds { object, cells })
    }
}

/// A column's `width`: its grid width, a width JSON cannot hold (not finite) as 0.
fn col_width(w: f32) -> Value {
    json!(if w.is_finite() { w } else { 0.0 })
}

/// Each style's `order` (see `Writer::write_styles`), by its key: the order an entry holds
/// (`stored`) while it sorts the entry (order, then key) after the one before it, else one between
/// its neighbours' that are kept.
fn style_orders(keys: &[String], stored: impl Fn(&str) -> Option<f64>) -> Vec<f64> {
    let mut kept: Vec<Option<f64>> = Vec::with_capacity(keys.len());
    let mut last: Option<(f64, &str)> = None;
    for key in keys {
        let order = stored(key).filter(|o| last.is_none_or(|(lo, lk)| (*o, key.as_str()) > (lo, lk)));
        if let Some(o) = order {
            last = Some((o, key.as_str()));
        }
        kept.push(order);
    }
    let mut out = Vec::with_capacity(keys.len());
    let mut i = 0;
    while i < kept.len() {
        if let Some(Some(o)) = kept.get(i) {
            out.push(*o);
            i += 1;
            continue;
        }
        // A run of entries to place, between the kept ones around it.
        let end = (i..kept.len()).find(|k| kept.get(*k).is_some_and(Option::is_some)).unwrap_or(kept.len());
        let lo = out.last().copied();
        let hi = kept.get(end).copied().flatten();
        let count = end - i;
        for k in 0..count {
            let n = k as f64;
            out.push(match (lo, hi) {
                (None, None) => n,
                (Some(lo), None) => lo + 1.0 + n,
                (None, Some(hi)) => hi - (count - k) as f64,
                (Some(lo), Some(hi)) => lo + (hi - lo) * (n + 1.0) / (count as f64 + 1.0),
            });
        }
        i = end;
    }
    out
}

/// Whether two JSON values are equal, numbers by value: the replica keeps a whole float as an
/// integer (`36.0` reads back as `36`).
/// A block's tool id (C8-land-14-doc): `p` and its paragraph's `w14:paraId`, `t` and its first
/// row's; `null` while the engine has not numbered it.
pub(crate) fn block_tool_id(block: &Block) -> Value {
    match block {
        Block::Para(p) => p.props.para_id.map_or(Value::Null, |v| json!(format!("p{v:08X}"))),
        Block::Table(t) => t.rows.first().and_then(|r| r.props.para_id).map_or(Value::Null, |v| json!(format!("t{v:08X}"))),
    }
}

/// A table row's tool id: `r` and its `w14:paraId`.
pub(crate) fn row_tool_id(row: &Row) -> Value {
    row.props.para_id.map_or(Value::Null, |v| json!(format!("r{v:08X}")))
}

pub(crate) fn same_json(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y || x.as_f64().zip(y.as_f64()).is_some_and(|(x, y)| x == y),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_json(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, p)| y.get(k).is_some_and(|q| same_json(p, q))),
        _ => a == b,
    }
}

/// `items` as [`align`]'s (value, key) pairs, with no key.
fn keyless<T>(items: &[T]) -> Vec<(&T, ())> {
    items.iter().map(|item| (item, ())).collect()
}

fn same_kind(a: &Block, b: &Block) -> bool {
    matches!((a, b), (Block::Para(_), Block::Para(_)) | (Block::Table(_), Block::Table(_)))
}

/// For each `after` block, its `before` partner (see the module docs).
pub(crate) fn pair_blocks(before: &[Arc<Block>], after: &[Arc<Block>]) -> Vec<Option<usize>> {
    if before.len() == after.len() && before.iter().zip(after).all(|(b, a)| Arc::ptr_eq(b, a)) {
        return (0..after.len()).map(Some).collect();
    }
    let mut partner: Vec<Option<usize>> = vec![None; after.len()];
    let mut used = vec![false; before.len()];

    // 0. The same paragraph id (C8-doc-2): the id is the paragraph's identity, and it follows
    // the paragraph's text across a split or a join. Only an id one block holds on each side.
    let unique = |blocks: &[Arc<Block>]| {
        let mut count: HashMap<u32, (usize, usize)> = HashMap::new();
        for (i, b) in blocks.iter().enumerate() {
            if let Block::Para(p) = &**b
                && let Some(v) = p.props.para_id
            {
                let e = count.entry(v).or_insert((0, i));
                e.0 += 1;
            }
        }
        count.into_iter().filter(|(_, (n, _))| *n == 1).map(|(v, (_, i))| (v, i)).collect::<HashMap<u32, usize>>()
    };
    let was = unique(before);
    for (v, j) in unique(after) {
        if let Some(&i) = was.get(&v) {
            partner[j] = Some(i);
            used[i] = true;
        }
    }

    // 1. The same `Arc`: untouched (maybe moved).
    let mut by_ptr: HashMap<*const Block, Vec<usize>> = HashMap::new();
    for (i, b) in before.iter().enumerate().rev() {
        by_ptr.entry(Arc::as_ptr(b)).or_default().push(i);
    }
    for (j, a) in after.iter().enumerate() {
        if partner[j].is_some() {
            continue;
        }
        if let Some(i) = by_ptr.get_mut(&Arc::as_ptr(a)).and_then(Vec::pop).filter(|i| !used[*i]) {
            partner[j] = Some(i);
            used[i] = true;
        }
    }

    // 2. Equal content: copied or rebuilt, but not changed.
    let mut by_text: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut tables: Vec<usize> = Vec::new();
    for (i, b) in before.iter().enumerate() {
        if used[i] {
            continue;
        }
        match &**b {
            Block::Para(p) => by_text.entry(p.text.as_str()).or_default().push(i),
            Block::Table(_) => tables.push(i),
        }
    }
    for (j, a) in after.iter().enumerate() {
        if partner[j].is_some() {
            continue;
        }
        let candidates: &[usize] = match &**a {
            Block::Para(p) => by_text.get(p.text.as_str()).map(Vec::as_slice).unwrap_or(&[]),
            Block::Table(_) => &tables,
        };
        if let Some(&i) = candidates.iter().find(|&&i| !used[i] && before.get(i).is_some_and(|b| **b == **a)) {
            partner[j] = Some(i);
            used[i] = true;
        }
    }

    // 3. Between blocks paired in order, a changed block takes the place of one of its kind.
    let matched: Vec<(usize, usize)> = partner.iter().enumerate().filter_map(|(j, i)| i.map(|i| (j, i))).collect();
    let order: Vec<usize> = matched.iter().map(|(_, i)| *i).collect();
    let stays: HashSet<usize> = longest_increasing(&order).into_iter().collect();
    let mut anchors: Vec<(usize, usize)> = matched.into_iter().filter(|(_, i)| stays.contains(i)).collect();
    anchors.push((after.len(), before.len()));
    let mut start = (0usize, 0usize);
    for (j_end, i_end) in anchors {
        let mut free: Vec<usize> = (start.1..i_end).filter(|&i| !used[i]).collect();
        for (slot, a) in partner.iter_mut().zip(after).take(j_end).skip(start.0) {
            if slot.is_some() {
                continue;
            }
            if let Some(k) = free.iter().position(|&i| before.get(i).is_some_and(|b| same_kind(b, a))) {
                let i = free.remove(k);
                *slot = Some(i);
                used[i] = true;
            }
        }
        start = (j_end + 1, i_end + 1);
    }
    partner
}

/// The fields of one block list's text paragraphs (by their index in the list as read: each
/// view block, then the paragraphs merged into it), composed at their anchors when first needed
/// (`pieces`), and the ones a write changes.
struct ListFields<'a> {
    flat: Vec<&'a BlockIds>,
    /// Each view block's index in `flat`.
    at: Vec<usize>,
    /// Each shown text paragraph's pieces, when some field of the list has anchors (else each is
    /// its own field's visible runs).
    pieces: HashMap<ObjectId, Vec<Piece>>,
    /// The index of each text paragraph's object, when `pieces` is not empty.
    index: HashMap<ObjectId, usize>,
    was: HashMap<usize, Composition>,
    now: BTreeMap<usize, Composition>,
}

impl<'a> ListFields<'a> {
    fn new(old: &'a [BlockIds]) -> Self {
        let mut flat = Vec::with_capacity(old.len());
        let mut at = Vec::with_capacity(old.len());
        for b in old {
            at.push(flat.len());
            flat.push(b);
            flat.extend(b.merged.iter());
        }
        let mut fields = Self { flat, at, pieces: HashMap::new(), index: HashMap::new(), was: HashMap::new(), now: BTreeMap::new() };
        if !fields.flat.iter().any(|b| b.text.as_ref().is_some_and(|t| has_anchor(t.runs()))) {
            return fields;
        }
        let mut list = Vec::with_capacity(fields.flat.len());
        for (i, b) in fields.flat.iter().enumerate() {
            let text = match (&b.table, &b.text) {
                (None, Some(text)) => Some(text),
                _ => None,
            };
            list.push((b.object.clone(), text.is_some()));
            if let Some(text) = text {
                fields.index.insert(b.object.clone(), i);
                if has_anchor(text.runs()) {
                    fields.was.insert(i, compose(text.runs()));
                }
            }
        }
        let composed: HashMap<ObjectId, Composition> = fields.was.iter().map(|(i, c)| (fields.flat[*i].object.clone(), c.clone())).collect();
        fields.pieces = layout(&list, &composed).pieces;
        fields
    }

    /// View block `i`'s index in the list as read.
    fn flat(&self, i: usize) -> usize {
        self.at.get(i).copied().unwrap_or(usize::MAX)
    }

    /// The indexes of the paragraphs merged into view block `i`.
    fn merged_of(&self, i: usize) -> std::ops::Range<usize> {
        let start = self.flat(i).saturating_add(1);
        start..start + self.flat.get(self.flat(i)).map_or(0, |b| b.merged.len())
    }

    fn ids(&self, f: usize) -> Option<&'a BlockIds> {
        self.flat.get(f).copied()
    }

    fn object(&self, f: usize) -> &'a ObjectId {
        &self.flat[f].object
    }

    /// The objects of the list as read, in order.
    fn objects(&self) -> impl Iterator<Item = &'a ObjectId> + '_ {
        self.flat.iter().map(|b| &b.object)
    }

    /// Whether list entry `f` is a paragraph in the text form.
    fn is_text(&self, f: usize) -> bool {
        self.flat.get(f).is_some_and(|b| b.table.is_none() && b.text.is_some())
    }

    /// View block `i`'s pieces, as (list index, tail).
    fn pieces(&self, i: usize) -> Vec<(usize, Option<usize>)> {
        let f = self.flat(i);
        let own = vec![(f, None)];
        let Some(object) = self.flat.get(f).map(|b| &b.object) else { return own };
        match self.pieces.get(object) {
            Some(pieces) => pieces.iter().filter_map(|piece| Some((*self.index.get(&piece.object)?, piece.tail))).collect(),
            None => own,
        }
    }

    /// Field `f` as read.
    fn was(&mut self, f: usize) -> &Composition {
        let flat = &self.flat;
        self.was.entry(f).or_insert_with(|| compose(flat.get(f).and_then(|b| b.text.as_ref()).map(ParaText::runs).unwrap_or_default()))
    }

    /// Field `f` as this write leaves it.
    fn now(&mut self, f: usize) -> &mut Composition {
        if !self.now.contains_key(&f) {
            let was = self.was(f).clone();
            self.now.insert(f, was);
        }
        self.now.entry(f).or_default()
    }

    /// The changed fields, in list order.
    fn take_changed(&mut self) -> BTreeMap<usize, Composition> {
        std::mem::take(&mut self.now)
    }
}

/// The object id of the drawing object numbered `uid` (its tool id): the same on every read.
pub(crate) fn inline_id(uid: u32) -> ObjectId {
    ObjectId::from_string(format!("inline-{uid}"))
}
