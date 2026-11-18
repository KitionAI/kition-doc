//! [`normalize`]: an id-blind form of a [`Document`], for comparing views. Part, comment and list
//! ids are local to each view (minted by every read, never replicated), so two views of the same
//! content — a round trip, or two replicas that converged — need not share them.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use kition_doc_doc::numbering::{AbstractNum, Numbering};
use kition_doc_doc::table::{Cell, Row};
use kition_doc_doc::{Block, Blocks, Document, InlineObject, Paragraph, Part, SectionProps, Table};
use serde_json::Value;

use crate::KdmResult;
use crate::refs::{Fix, Ref, local_number, para_from, para_value, walk};

const MAX_DEPTH: usize = 64;

/// An id-blind copy of `doc`: parts and comments renumbered from 1 in reading order (a part or
/// comment where it is first referenced from the body — sections, note references, text boxes,
/// comment anchors — and, depth first, from what it references; each comment's replies after it;
/// then what nothing references, ordered by content), lists from 1 in their order, abstract lists
/// from 1 in the order the lists use them, and every
/// reference rewritten to match. A reference to nothing keeps its number. Two documents with the
/// same content normalize to the same value whatever ids either holds.
pub fn normalize(doc: &Document) -> Document {
    renumbered(doc).unwrap_or_else(|_| doc.clone())
}

struct Order<'d> {
    doc: &'d Document,
    parts: Vec<u32>,
    comments: Vec<u32>,
    seen_parts: HashSet<u32>,
    seen_comments: HashSet<u32>,
}

impl Order<'_> {
    fn blocks(&mut self, blocks: &Blocks, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        for b in blocks {
            match &**b {
                Block::Para(p) => self.para(p, depth),
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            self.blocks(&cell.blocks, depth + 1);
                        }
                    }
                }
            }
        }
    }

    fn para(&mut self, p: &Paragraph, depth: usize) {
        if let Some(s) = &p.section {
            self.section(s, depth);
        }
        for o in &p.objects {
            self.object(o, depth);
        }
    }

    fn section(&mut self, s: &SectionProps, depth: usize) {
        for set in [&s.headers, &s.footers] {
            for id in [set.default, set.first, set.even].into_iter().flatten() {
                self.part(id, depth);
            }
        }
    }

    fn object(&mut self, o: &InlineObject, depth: usize) {
        match o {
            InlineObject::NoteRef { id, .. } => self.part(*id, depth),
            InlineObject::CommentStart { id } | InlineObject::CommentEnd { id } => self.comment(*id, depth),
            InlineObject::Group { children, .. } => {
                for c in children {
                    self.object(&c.obj, depth + 1);
                }
            }
            o => {
                for id in o.text_boxes() {
                    self.part(id, depth);
                }
            }
        }
    }

    fn part(&mut self, id: u32, depth: usize) {
        let doc = self.doc;
        if depth > MAX_DEPTH || !self.seen_parts.insert(id) {
            return;
        }
        if let Some(part) = doc.parts.get(&id) {
            self.parts.push(id);
            self.blocks(&part.blocks, depth + 1);
        }
    }

    fn comment(&mut self, id: u32, depth: usize) {
        let doc = self.doc;
        if depth > MAX_DEPTH || !self.seen_comments.insert(id) {
            return;
        }
        let Some(c) = doc.comments.get(&id) else { return };
        self.comments.push(id);
        self.part(c.part, depth + 1);
        let mut replies: Vec<(String, u32)> =
            doc.comments.iter().filter(|(_, r)| r.parent == Some(id)).map(|(rid, _)| (comment_key(doc, *rid), *rid)).collect();
        replies.sort();
        for (_, reply) in replies {
            self.comment(reply, depth + 1);
        }
    }
}

/// What orders comments nothing anchors: their fields and their text.
fn comment_key(doc: &Document, id: u32) -> String {
    let Some(c) = doc.comments.get(&id) else { return String::new() };
    let text = doc.parts.get(&c.part).map(|p| plain(&p.blocks)).unwrap_or_default();
    format!("{}\u{1}{}\u{1}{}\u{1}{}\u{1}{text}", c.author, c.initials, c.date, c.resolved)
}

fn plain(blocks: &Blocks) -> String {
    let mut d = Document::new();
    d.body = blocks.clone();
    d.plain_text(kition_doc_doc::StoryRef::Body)
}

fn part_key(part: &Part) -> String {
    format!("{:?}\u{1}{}", part.kind, serde_json::to_string(&part.blocks).unwrap_or_default())
}

fn renumbered(doc: &Document) -> KdmResult<Document> {
    let mut order = Order { doc, parts: Vec::new(), comments: Vec::new(), seen_parts: HashSet::new(), seen_comments: HashSet::new() };
    order.blocks(&doc.body, 0);
    order.section(&doc.last_section, 0);
    let mut rest: Vec<(String, u32)> =
        doc.comments.keys().filter(|id| !order.seen_comments.contains(id)).map(|id| (comment_key(doc, *id), *id)).collect();
    rest.sort();
    for (_, id) in rest {
        // Roots first: a reply is reached from its parent.
        let root = doc.comments.get(&id).and_then(|c| c.parent).is_none_or(|p| !doc.comments.contains_key(&p));
        if root {
            order.comment(id, 0);
        }
    }
    let mut rest: Vec<(String, u32)> =
        doc.comments.keys().filter(|id| !order.seen_comments.contains(id)).map(|id| (comment_key(doc, *id), *id)).collect();
    rest.sort();
    for (_, id) in rest {
        order.comment(id, 0);
    }
    let mut rest: Vec<(String, u32)> = doc.parts.iter().filter(|(id, _)| !order.seen_parts.contains(id)).map(|(id, p)| (part_key(p), *id)).collect();
    rest.sort();
    for (_, id) in rest {
        order.part(id, 0);
    }

    let number =
        |list: &[u32]| -> HashMap<u32, u32> { list.iter().enumerate().map(|(i, id)| (*id, u32::try_from(i + 1).unwrap_or(u32::MAX))).collect() };
    let parts = number(&order.parts);
    let comments = number(&order.comments);
    let mut nums: HashMap<u32, u32> = HashMap::new();
    for (i, num) in doc.numbering.nums.iter().enumerate() {
        if num.id != 0 {
            nums.entry(num.id).or_insert(u32::try_from(i + 1).unwrap_or(u32::MAX));
        }
    }
    let remap = |v: &mut Value| {
        walk(v, &mut |kind, value| {
            let Some(n) = local_number(value) else { return Fix::Keep };
            let map = match kind {
                Ref::Part => &parts,
                Ref::Comment => &comments,
                Ref::Num => &nums,
                Ref::Revision | Ref::Media => return Fix::Keep,
            };
            map.get(&n).map_or(Fix::Keep, |k| Fix::Set(Value::from(*k)))
        });
    };
    let mapped = Mapper { remap: &remap };

    let mut out = doc.clone();
    out.body = mapped.blocks(&doc.body, 0)?;
    out.last_section = mapped.json(&doc.last_section)?;
    out.styles = mapped.json(&doc.styles)?;
    let mut numbering = doc.numbering.clone();
    for (i, num) in numbering.nums.iter_mut().enumerate() {
        if num.id != 0 {
            num.id = u32::try_from(i + 1).unwrap_or(u32::MAX);
        }
    }
    renumber_abstracts(&mut numbering);
    out.numbering = mapped.json(&numbering)?;
    out.parts = BTreeMap::new();
    for (id, part) in &doc.parts {
        let key = parts.get(id).copied().unwrap_or(*id);
        out.parts.insert(key, Part { kind: part.kind, blocks: mapped.blocks(&part.blocks, 0)? });
    }
    out.comments = BTreeMap::new();
    for (id, c) in &doc.comments {
        let mut c = c.clone();
        c.part = parts.get(&c.part).copied().unwrap_or(c.part);
        c.parent = c.parent.map(|p| comments.get(&p).copied().unwrap_or(p));
        out.comments.insert(comments.get(id).copied().unwrap_or(*id), c);
    }
    Ok(out)
}

/// Abstract lists from 1 in the order the lists first use them, then the others in their order;
/// each list's `abstract_id` follows (one naming nothing keeps its number).
fn renumber_abstracts(numbering: &mut Numbering) {
    let mut first: HashMap<u32, usize> = HashMap::new();
    for (k, a) in numbering.abstracts.iter().enumerate() {
        first.entry(a.id).or_insert(k);
    }
    let mut rank: Vec<Option<u32>> = vec![None; numbering.abstracts.len()];
    let mut next = 0u32;
    let mut place = |slot: Option<&mut Option<u32>>| {
        if let Some(slot) = slot
            && slot.is_none()
        {
            next = next.saturating_add(1);
            *slot = Some(next);
        }
    };
    for num in &numbering.nums {
        if let Some(&k) = first.get(&num.abstract_id) {
            place(rank.get_mut(k));
        }
    }
    for k in 0..rank.len() {
        place(rank.get_mut(k));
    }
    let new_id: HashMap<u32, u32> = first.iter().filter_map(|(id, k)| Some((*id, rank.get(*k).copied().flatten()?))).collect();
    let mut ranked: Vec<(u32, AbstractNum)> = numbering
        .abstracts
        .drain(..)
        .zip(rank)
        .map(|(mut a, r)| {
            let r = r.unwrap_or(u32::MAX);
            a.id = r;
            (r, a)
        })
        .collect();
    ranked.sort_by_key(|(r, _)| *r);
    numbering.abstracts = ranked.into_iter().map(|(_, a)| a).collect();
    for num in &mut numbering.nums {
        num.abstract_id = new_id.get(&num.abstract_id).copied().unwrap_or(num.abstract_id);
    }
}

struct Mapper<'f> {
    remap: &'f dyn Fn(&mut Value),
}

impl Mapper<'_> {
    fn json<T: serde::Serialize + serde::de::DeserializeOwned>(&self, v: &T) -> KdmResult<T> {
        let mut value = serde_json::to_value(v).map_err(crate::kdm_err)?;
        (self.remap)(&mut value);
        serde_json::from_value(value).map_err(crate::kdm_err)
    }

    fn blocks(&self, blocks: &Blocks, depth: usize) -> KdmResult<Blocks> {
        if depth > MAX_DEPTH {
            return Ok(blocks.clone());
        }
        blocks
            .iter()
            .map(|b| {
                Ok(Arc::new(match &**b {
                    Block::Para(p) => {
                        let mut v = para_value(p)?;
                        (self.remap)(&mut v);
                        // Runs merged: the text form keeps no boundary between equal runs.
                        let mut p = para_from(&v)?;
                        p.normalize();
                        Block::Para(p)
                    }
                    Block::Table(t) => Block::Table(Table {
                        props: self.json(&t.props)?,
                        grid: t.grid.clone(),
                        rows: t
                            .rows
                            .iter()
                            .map(|r| {
                                Ok(Row {
                                    props: self.json(&r.props)?,
                                    cells: r
                                        .cells
                                        .iter()
                                        .map(|c| Ok(Cell { props: self.json(&c.props)?, blocks: self.blocks(&c.blocks, depth + 1)? }))
                                        .collect::<KdmResult<_>>()?,
                                })
                            })
                            .collect::<KdmResult<_>>()?,
                    }),
                }))
            })
            .collect()
    }
}
