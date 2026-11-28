//! Review tab: comments, track changes, word count, spelling (via the proofing hook).

use kition_doc_doc::para::InlineObject;
use kition_doc_doc::{Comment, Paragraph, PartKind, Pos, StoryRef, para_block};
use serde_json::{Value, json};

use super::{fmt_revisions, now_iso, pos_json, sel_result};
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

/// The comment `id` names: its number in this session (comments are numbered in 32 bits, so a
/// larger number names none: C8-doc-3, it was cut to its low bits and named another comment), or
/// its tool id (`comment:<durable id>`, C8-doc-6).
fn comment_id(s: &Session, v: &Value) -> Result<Option<u32>, CmdError> {
    match v.get("id") {
        None | Some(Value::Null) => Ok(None),
        Some(_) => comment_param(s, v, "id").map(Some),
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("comment.new", "New Comment", "Review › Comments", new_comment).key("Mod+Alt+A").params(r#"{"text": string}"#),
        CommandSpec::new("comment.replyCurrent", "Reply", "Review › Comments", reply).params(r#"{"id": n, "text": string}"#),
        CommandSpec::new("comment.edit", "Edit Comment", "Review › Comments", edit_comment).params(r#"{"id": n, "text": string}"#),
        CommandSpec::new("comment.deleteCurrent", "Delete", "Review › Comments", delete_comment).params(r#"{"id"?: n, "all"?: bool}"#),
        CommandSpec::new("comment.resolveCurrent", "Resolve", "Review › Comments", |s, v| {
            let id = comment_id(s, v)?.or_else(|| comment_at_caret(s)).ok_or_else(|| CmdError::Params("`id` required".into()))?;
            // A reply's thread is resolved: the flag is the thread's first comment's (C8-doc-4).
            let mut id = id;
            for _ in 0..64 {
                match s.doc.comments.get(&id).and_then(|c| c.parent).filter(|parent| s.doc.comments.contains_key(parent)) {
                    Some(parent) => id = parent,
                    None => break,
                }
            }
            let c = s.doc.comments.get_mut(&id).ok_or_else(|| CmdError::Params("no such comment".into()))?;
            c.resolved = p::bool(v, "value").unwrap_or(!c.resolved);
            sel_result(s)
        }),
        CommandSpec::new("comment.next", "Next", "Review › Comments", |s, _| nav_comment(s, 1)).pure(),
        CommandSpec::new("comment.previous", "Previous", "Review › Comments", |s, _| nav_comment(s, -1)).pure(),
        CommandSpec::new("comment.pane", "Show Comments", "Review › Comments", list_comments).pure(),
        // Comment threads (Kition platform rule M23), as kit-editor's comments panel shows them.
        CommandSpec::new("comment.threads", "Comment Threads", "Comments", threads)
            .params(r#"{} → [{id, anchor: {object, field?, range?, quote?, place?: {page, x, y, height}}, resolved, comments: [{id, author, name, body, created}]}]"#)
            .pure(),
        CommandSpec::new("comment.add", "Add Comment", "Comments", add_thread)
            .params(r#"{"text": string, "author"?: string (principal id), "name"?: string, "created"?: string (ISO 8601 UTC)} → {thread, threadId} (on the selection, or the word at the caret)"#),
        CommandSpec::new("comment.reply", "Reply to Comment", "Comments", reply_thread)
            .params(r#"{"thread": string (`c<n>`, or a tool id `thread:<id>`/`comment:<id>`), "text": string, "author"?: string, "name"?: string, "created"?: string} → {thread, comment, threadId, commentId}"#),
        CommandSpec::new("comment.resolve", "Resolve Comment Thread", "Comments", resolve_thread).params(r#"{"thread": string, "resolved"?: bool}"#),
        CommandSpec::new("comment.delete", "Delete Comment", "Comments", delete_from_thread)
            .params(r#"{"thread": string, "comment"?: string (a reply; omitted or the first comment: the whole thread)}"#),
        CommandSpec::new("comment.goto", "Go to Comment", "Comments", goto_thread).params(r#"{"thread": string}"#).pure(),
        CommandSpec::new("change.track", "Track Changes", "Review › Tracking", |s, v| {
            s.doc.settings.track_changes = p::bool(v, "value").unwrap_or(!s.doc.settings.track_changes);
            Ok(json!({"value": s.doc.settings.track_changes}))
        })
        .params(r#"{"value"?: bool (omit to toggle)}"#)
        .key("Mod+Shift+E"),
        CommandSpec::new("change.acceptAll", "Accept All Changes", "Review › Changes", |s, _| resolve_all(s, true)),
        CommandSpec::new("change.rejectAll", "Reject All Changes", "Review › Changes", |s, _| resolve_all(s, false)),
        CommandSpec::new("change.accept", "Accept", "Review › Changes", |s, _| resolve_sel(s, true)),
        CommandSpec::new("change.reject", "Reject", "Review › Changes", |s, _| resolve_sel(s, false)),
        CommandSpec::new("change.next", "Next Change", "Review › Changes", |s, _| nav_change(s, 1)).pure(),
        CommandSpec::new("change.previous", "Previous Change", "Review › Changes", |s, _| nav_change(s, -1)).pure(),
        CommandSpec::new("change.markup", "Display for Review", "Review › Tracking", |s, v| {
            s.view.show_markup = p::str(v, "value").map(|m| m != "noMarkup" && m != "original").unwrap_or(!s.view.show_markup);
            // Without markup, deletions leave the layout.
            s.relayout();
            Ok(json!({"showMarkup": s.view.show_markup}))
        })
        .pure(),
        CommandSpec::new("change.options", "Track Changes Options", "Review › Tracking", tracking_options)
            .params(
                r#"{"comments"?: bool, "ink"?: bool, "insertionsDeletions"?: bool, "formatting"?: bool, "balloons"?: "revisions|inline|commentsAndFormatting", "insertMark"?: "underline|doubleUnderline|bold|italic|strikethrough|colorOnly|none", "insertColor"?: "byAuthor" | "RRGGBB", "deleteMark"?: "strikethrough|doubleStrikethrough|hidden|caret|hash|underline|colorOnly", "deleteColor"?: "byAuthor" | "RRGGBB", "changedLines"?: "outside|left|right|none", "changedLinesColor"?: "auto" | "RRGGBB", "trackFormatting"?: bool} → the options (omit all to read them)"#,
            )
            .pure(),
        CommandSpec::new("file.wordCount", "Word Count", "Review › Proofing", word_count).params(r#"{"includeTextBoxes"?: bool}"#).pure(),
        CommandSpec::new("change.list", "Reviewing Pane", "Review › Tracking", list_changes).pure(),
        CommandSpec::new("spelling.check", "Spelling & Grammar", "Review › Proofing", next_issue).key("F7").pure(),
        CommandSpec::new("spelling.issues", "Proofing Issues", "Review › Proofing", all_issues).pure(),
        CommandSpec::new("spelling.suggestions", "Spelling Suggestions", "Review › Proofing", suggestions).params(r#"{"pos"?: Pos}"#).pure(),
        CommandSpec::new("spelling.addToDictionary", "Add to Dictionary", "Review › Proofing", |s, v| {
            let w = match p::str(v, "word") {
                Some(w) => w.to_string(),
                None => issue_at(s, &s.sel.focus.clone())
                    .map(|(a, b, _)| s.doc.para_at(&a).and_then(|p| p.text.get(a.off..b.off).map(str::to_string)).unwrap_or_default())
                    .unwrap_or_default(),
            };
            if w.is_empty() {
                return Err(CmdError::Params("no word".into()));
            }
            kition_doc_proof::add_word(&w);
            s.relayout();
            Ok(json!({"added": w}))
        })
        .params(r#"{"word"?: string}"#)
        .pure(),
        CommandSpec::new("spelling.ignoreAll", "Ignore All", "Review › Proofing", |s, v| {
            let w = match p::str(v, "word") {
                Some(w) => w.to_string(),
                None => issue_at(s, &s.sel.focus.clone())
                    .map(|(a, b, _)| s.doc.para_at(&a).and_then(|p| p.text.get(a.off..b.off).map(str::to_string)).unwrap_or_default())
                    .unwrap_or_default(),
            };
            kition_doc_proof::add_word(&w);
            s.relayout();
            Ok(json!({"ignored": w}))
        })
        .pure(),
        CommandSpec::new("spelling.applySuggestion", "Change", "Review › Proofing", |s, v| {
            let text = p::req_str(v, "text")?.to_string();
            let at = s.sel.focus.clone();
            if let Some((a, b, _)) = issue_at(s, &at) {
                s.sel = Selection { anchor: a, focus: b };
            }
            super::type_text(s, &text)?;
            sel_result(s)
        })
        .params(r#"{"text": string}"#),
        CommandSpec::new("spelling.proofing", "Check Spelling as You Type", "File › Options › Proofing", |s, v| {
            s.view.proofing = p::bool(v, "value").unwrap_or(!s.view.proofing);
            Ok(json!({"value": s.view.proofing}))
        })
        .pure(),
        CommandSpec::new("spelling.thesaurus", "Thesaurus", "Review › Proofing", thesaurus).key("Shift+F7").pure(),
    ]
}

fn new_comment(s: &mut Session, v: &Value) -> CmdResult {
    let text = p::str(v, "text").unwrap_or("").to_string();
    let (a, b) = s.sel.ordered();
    let (a, b) = if a == b {
        // Comment on the word at the caret.
        match s.doc.para_at(&a) {
            Some(para) if !para.is_empty() => {
                let (x, y) = para.word_at(a.off);
                let y = para.text.get(x..y).map(|w| x + w.trim_end().len()).unwrap_or(y);
                (Pos { off: x, ..a.clone() }, Pos { off: y, ..a })
            }
            _ => (a.clone(), a),
        }
    } else {
        (a, b)
    };
    let part = s.doc.add_part(PartKind::Comment, vec![para_block(Paragraph::with_text(&text, Default::default()))]);
    let id = s.doc.comments.keys().next_back().map(|k| k + 1).unwrap_or(0);
    let initials: String = s.author.split_whitespace().filter_map(|w| w.chars().next()).collect();
    s.doc
        .comments
        .insert(id, Comment { author: s.author.clone(), initials, date: now_iso(), parent: None, resolved: false, part, ..Default::default() });
    let props = Default::default();
    s.doc.insert_object(&b, InlineObject::CommentEnd { id }, &props)?;
    s.doc.insert_object(&a, InlineObject::CommentStart { id }, &props)?;
    s.view.comments_pane = true;
    Ok(json!({"id": id, "story": part}))
}

fn reply(s: &mut Session, v: &Value) -> CmdResult {
    let parent = comment_id(s, v)?.ok_or_else(|| CmdError::Params("`id` required".into()))?;
    if !s.doc.comments.contains_key(&parent) {
        return Err(CmdError::Params("no such comment".into()));
    }
    let text = p::req_str(v, "text")?;
    let part = s.doc.add_part(PartKind::Comment, vec![para_block(Paragraph::with_text(text, Default::default()))]);
    let id = s.doc.comments.keys().next_back().map(|k| k + 1).unwrap_or(0);
    let initials: String = s.author.split_whitespace().filter_map(|w| w.chars().next()).collect();
    s.doc.comments.insert(
        id,
        Comment { author: s.author.clone(), initials, date: now_iso(), parent: Some(parent), resolved: false, part, ..Default::default() },
    );
    Ok(json!({"id": id}))
}

/// Replace a comment's (or reply's) text; each line becomes a paragraph.
fn edit_comment(s: &mut Session, v: &Value) -> CmdResult {
    let id = comment_id(s, v)?.ok_or_else(|| CmdError::Params("`id` required".into()))?;
    let text = p::req_str(v, "text")?;
    let part = s.doc.comments.get(&id).map(|c| c.part).ok_or_else(|| CmdError::Params("no such comment".into()))?;
    let blocks = text.split('\n').take(10_000).map(|l| para_block(Paragraph::with_text(l.trim_end_matches('\r'), Default::default()))).collect();
    s.doc.set_story(StoryRef::Part(part), blocks)?;
    Ok(json!({"id": id}))
}

/// The comment the selection is in: the innermost one whose range overlaps the selection's part
/// in the focus paragraph (a caret at the range's end is in it, one before its first character is
/// not). None when the selection is in no comment (C8-doc-4: it fell back to the paragraph's
/// first comment, wherever the caret was).
/// The comment the selection is in (Delete and Resolve with no `id`).
pub fn comment_at_caret(s: &Session) -> Option<u32> {
    let (a, b) = s.sel.ordered();
    let p = s.doc.para_at(&b)?;
    let from = if a.story == b.story && a.path == b.path { a.off } else { 0 };
    let to = b.off;
    // Each comment's range in this paragraph: from its start marker (or the start), to its end
    // marker (or the end).
    let mut ranges: Vec<(u32, usize, usize)> = Vec::new();
    for off in p.object_offsets() {
        match p.object_at(off) {
            Some(InlineObject::CommentStart { id }) => ranges.push((*id, off, p.len())),
            Some(InlineObject::CommentEnd { id }) => match ranges.iter_mut().rev().find(|(r, _, end)| r == id && *end == p.len()) {
                Some(range) => range.2 = off,
                None => ranges.push((*id, 0, off)),
            },
            _ => {}
        }
    }
    ranges.into_iter().filter(|(_, start, end)| *start < to && from <= *end).max_by_key(|(_, start, _)| *start).map(|(id, _, _)| id)
}

fn remove_anchors(s: &mut Session, ids: &[u32]) -> Result<(), CmdError> {
    for story in [StoryRef::Body] {
        for path in s.doc.para_paths(story) {
            let offs: Vec<usize> = s
                .doc
                .para(story, &path)
                .map(|p| {
                    p.object_offsets()
                        .into_iter()
                        .filter(|o| matches!(p.object_at(*o), Some(InlineObject::CommentStart { id }) | Some(InlineObject::CommentEnd { id }) if ids.contains(id)))
                        .collect()
                })
                .unwrap_or_default();
            if offs.is_empty() {
                continue;
            }
            let para = s.doc.para_mut(story, &path)?;
            for o in offs.into_iter().rev() {
                para.delete(o, o + kition_doc_doc::para::OBJ.len_utf8())?;
            }
        }
    }
    Ok(())
}

fn delete_comment(s: &mut Session, v: &Value) -> CmdResult {
    let ids: Vec<u32> = if p::bool(v, "all").unwrap_or(false) {
        s.doc.comments.keys().copied().collect()
    } else {
        let id = comment_id(s, v)?.or_else(|| comment_at_caret(s)).ok_or_else(|| CmdError::Params("no comment here".into()))?;
        // The comment and its replies.
        let mut v = vec![id];
        v.extend(s.doc.comments.iter().filter(|(_, c)| c.parent == Some(id)).map(|(k, _)| *k));
        v
    };
    remove_anchors(s, &ids)?;
    for id in &ids {
        if let Some(c) = s.doc.comments.remove(id) {
            s.doc.parts.remove(&c.part);
        }
    }
    s.clamp_selection();
    Ok(json!({"deleted": ids}))
}

/// Comments in document order with their anchors.
pub fn comment_list(s: &Session) -> Vec<(u32, Option<Pos>)> {
    let mut found: Vec<(u32, Option<Pos>)> = Vec::new();
    for path in s.doc.para_paths(StoryRef::Body) {
        let Some(p) = s.doc.para(StoryRef::Body, &path) else { continue };
        for o in p.object_offsets() {
            if let Some(InlineObject::CommentStart { id }) = p.object_at(o) {
                found.push((*id, Some(Pos { story: StoryRef::Body, path: path.clone(), off: o })));
            }
        }
    }
    for id in s.doc.comments.keys() {
        if !found.iter().any(|(x, _)| x == id) {
            found.push((*id, None));
        }
    }
    found
}

fn list_comments(s: &mut Session, _: &Value) -> CmdResult {
    let list = comment_list(s);
    Ok(Value::Array(
        list.iter()
            .filter_map(|(id, pos)| {
                let c = s.doc.comments.get(id)?;
                Some(json!({
                    "id": id, "author": c.author, "date": c.date, "resolved": c.resolved, "parent": c.parent,
                    "text": s.doc.plain_text(StoryRef::Part(c.part)),
                    "anchor": pos.as_ref().map(pos_json),
                }))
            })
            .collect(),
    ))
}

fn nav_comment(s: &mut Session, dir: i32) -> CmdResult {
    let list: Vec<Pos> = comment_list(s).into_iter().filter_map(|(_, p)| p).collect();
    let caret = s.sel.focus.clone();
    let t = if dir > 0 { list.iter().find(|p| **p > caret).or(list.first()) } else { list.iter().rev().find(|p| **p < caret).or(list.last()) };
    if let Some(p) = t {
        s.sel = Selection::caret(p.clone());
    }
    sel_result(s)
}

/// Accept or reject a revision range in one paragraph; `mark` includes its paragraph mark (and
/// the section break on it), `props` the paragraph's own formatting change.
#[allow(clippy::too_many_arguments)]
fn resolve_para(
    s: &mut Session,
    story: StoryRef,
    path: &kition_doc_doc::Path,
    from: usize,
    to: usize,
    mark: bool,
    props: bool,
    accept: bool,
) -> Result<(), CmdError> {
    let para = s.doc.para_mut(story, path)?;
    if props && (para.props.fmt_change.is_some() || para.props.num_change.is_some()) {
        fmt_revisions::resolve_para(&mut para.props, accept);
        para.touch();
    }
    let ranges: Vec<(usize, usize, bool, bool, bool)> = para
        .run_ranges()
        .filter(|(r, _)| r.end > from && r.start < to)
        .map(|(r, c)| (r.start.max(from), r.end.min(to), c.ins.is_some(), c.del.is_some(), c.fmt_change.is_some()))
        .collect();
    for (a, b, ins, del, fmt) in ranges.into_iter().rev() {
        if (del && accept) || (ins && !accept) {
            para.delete(a, b)?;
        } else if ins || del || fmt {
            para.format(a, b, &|c| {
                c.ins = None;
                c.del = None;
                fmt_revisions::resolve_char(c, accept);
            })?;
        }
    }
    if !mark {
        return Ok(());
    }
    if para.mark.fmt_change.is_some() || para.section.as_ref().is_some_and(|x| x.fmt_change.is_some()) {
        fmt_revisions::resolve_char(&mut para.mark, accept);
        if let Some(sec) = para.section.as_deref_mut() {
            fmt_revisions::resolve_section(sec, accept);
        }
        para.touch();
    }
    let (ins, del) = (para.mark.ins, para.mark.del);
    if ins.is_none() && del.is_none() {
        return Ok(());
    }
    // An inserted paragraph mark that is rejected, or a deleted one that is accepted, goes away:
    // the next paragraph joins this one (the split is undone). The mark at the end of a story
    // or cell can't go, so it just stops being a revision.
    if ((del.is_some() && accept) || (ins.is_some() && !accept)) && super::join_next_para(s, story, path)? {
        return Ok(());
    }
    let para = s.doc.para_mut(story, path)?;
    para.mark.ins = None;
    para.mark.del = None;
    para.touch();
    Ok(())
}

/// Accept or reject the changes of the table cell `path` is in: the table's, its row's and the
/// cell's own. Returns whether there were any.
fn resolve_cell(s: &mut Session, story: StoryRef, path: &kition_doc_doc::Path, accept: bool) -> Result<bool, CmdError> {
    let Some((tp, r, c)) = path.cell() else { return Ok(false) };
    let has = s.doc.table(story, &tp).is_some_and(|t| {
        let row = t.rows.get(r);
        t.props.fmt_change.is_some()
            || row.is_some_and(|x| x.props.fmt_change.is_some())
            || row.and_then(|x| x.cells.get(c)).is_some_and(|x| x.props.fmt_change.is_some())
    });
    if !has {
        return Ok(false);
    }
    let t = s.doc.table_mut(story, &tp)?;
    fmt_revisions::resolve_table(t, accept);
    if let Some(row) = t.rows.get_mut(r) {
        fmt_revisions::resolve_row(row, accept);
        if let Some(cell) = row.cells.get_mut(c) {
            fmt_revisions::resolve_cell(cell, accept);
        }
    }
    Ok(true)
}

/// A tracked change, for navigation and the Reviewing Pane: its range, kind (`insert`,
/// `delete`, `format`), revision, and for a formatting change the properties it set.
struct Change {
    a: Pos,
    b: Pos,
    kind: &'static str,
    rev: Option<u32>,
    props: Vec<Value>,
}

impl Change {
    fn format(a: Pos, b: Pos, rev: u32, props: Vec<Value>) -> Change {
        Change { a, b, kind: "format", rev: Some(rev), props }
    }
}

/// The tracked paragraph mark at the end of the paragraph at `path`, as a change range: from
/// the paragraph's end to the start of the next paragraph (or the end itself when none follows).
fn mark_change(s: &Session, story: StoryRef, path: &kition_doc_doc::Path) -> Option<Change> {
    let p = s.doc.para(story, path)?;
    let kind = if p.mark.ins.is_some() {
        "insert"
    } else if p.mark.del.is_some() {
        "delete"
    } else {
        return None;
    };
    let a = Pos { story, path: path.clone(), off: p.len() };
    let b = if super::has_next_para(s, story, path) { Pos { story, path: path.with_last(path.last().saturating_add(1)), off: 0 } } else { a.clone() };
    Some(Change { a, b, kind, rev: p.mark.ins.or(p.mark.del), props: Vec::new() })
}

fn resolve_all(s: &mut Session, accept: bool) -> CmdResult {
    let stories: Vec<StoryRef> = std::iter::once(StoryRef::Body).chain(s.doc.parts.keys().map(|k| StoryRef::Part(*k))).collect();
    for st in stories {
        for path in s.doc.para_paths(st).into_iter().rev() {
            let len = s.doc.para(st, &path).map(|p| p.len()).unwrap_or(0);
            resolve_para(s, st, &path, 0, len, true, true, accept)?;
        }
    }
    // Tables, rows, cells and sections.
    fmt_revisions::resolve_containers(&mut s.doc.body, accept, 0);
    for part in s.doc.parts.values_mut() {
        fmt_revisions::resolve_containers(&mut part.blocks, accept, 0);
    }
    fmt_revisions::resolve_section(&mut s.doc.last_section, accept);
    s.doc.revisions.clear();
    s.clamp_selection();
    sel_result(s)
}

/// Whether Accept/Reject at this caret falls through to body-wide change navigation.
pub fn resolve_navigates(s: &Session, a: &Pos, b: &Pos) -> bool {
    if a != b {
        return false;
    }
    if let Some(p) = s.doc.para_at(a)
        && (p.run_ranges().any(|(r, c)| r.start <= a.off && a.off <= r.end && (c.ins.is_some() || c.del.is_some() || c.fmt_change.is_some()))
            || p.props.fmt_change.is_some()
            || p.props.num_change.is_some()
            || (a.off == p.len() && (p.mark.fmt_change.is_some() || mark_change(s, a.story, &a.path).is_some())))
    {
        return false;
    }
    !a.path.cell().and_then(|(tp, r, c)| s.doc.table(a.story, &tp).map(|t| (t, r, c))).is_some_and(|(t, r, c)| {
        t.props.fmt_change.is_some()
            || t.rows.get(r).is_some_and(|row| row.props.fmt_change.is_some() || row.cells.get(c).is_some_and(|cell| cell.props.fmt_change.is_some()))
    })
}

fn resolve_sel(s: &mut Session, accept: bool) -> CmdResult {
    let (a, b) = s.sel.ordered();
    if resolve_navigates(s, &a, &b) {
        return nav_change(s, 1);
    }
    // A caret right before a tracked paragraph mark (and no tracked text around it): that mark.
    let mut caret_mark = None;
    // A caret in a paragraph whose own formatting changed (and no tracked text around it).
    let mut caret_props = None;
    let ranged = a != b;
    let (a, b) = if a == b {
        // The change at the caret: the run around it, else the paragraph mark right after it,
        // else the paragraph's formatting, else the table cell's.
        let found = s.doc.para_at(&a).and_then(|p| {
            p.run_ranges()
                .find(|(r, c)| r.start <= a.off && a.off <= r.end && (c.ins.is_some() || c.del.is_some() || c.fmt_change.is_some()))
                .map(|(r, _)| r)
        });
        let para_fmt = s.doc.para_at(&a).is_some_and(|p| p.props.fmt_change.is_some() || p.props.num_change.is_some());
        let mark_fmt = s.doc.para_at(&a).is_some_and(|p| a.off == p.len() && p.mark.fmt_change.is_some());
        match found {
            Some(r) => (Pos { off: r.start, ..a.clone() }, Pos { off: r.end, ..a }),
            None if mark_fmt || mark_change(s, a.story, &a.path).is_some_and(|m| m.a == a) => {
                caret_mark = Some(a.path.clone());
                (a.clone(), a)
            }
            None if para_fmt => {
                caret_props = Some(a.path.clone());
                (a.clone(), a)
            }
            None => {
                if resolve_cell(s, a.story, &a.path, accept)? {
                    return sel_result(s);
                }
                return nav_change(s, 1);
            }
        }
    } else {
        (a, b)
    };
    for path in s.doc.paths_between(&a, &b).into_iter().rev() {
        let len = s.doc.para(a.story, &path).map(|p| p.len()).unwrap_or(0);
        let from = if path == a.path { a.off } else { 0 };
        let to = if path == b.path { b.off } else { len };
        // A paragraph's mark is in the selection when the selection goes on past it.
        let mark = path != b.path || caret_mark.as_ref() == Some(&path);
        let props = (ranged && (path != b.path || to > from)) || caret_props.as_ref() == Some(&path);
        resolve_para(s, a.story, &path, from, to, mark, props, accept)?;
        if ranged {
            resolve_cell(s, a.story, &path, accept)?;
        }
    }
    s.sel = Selection::caret(a);
    s.clamp_selection();
    sel_result(s)
}

fn changes(s: &Session) -> Vec<Change> {
    let mut out: Vec<Change> = Vec::new();
    let mk = |path: &kition_doc_doc::Path, off| Pos { story: StoryRef::Body, path: path.clone(), off };
    for path in s.doc.para_paths(StoryRef::Body) {
        let Some(p) = s.doc.para(StoryRef::Body, &path) else { continue };
        // A table's, row's and cell's changes sit at the start of the cell's first paragraph.
        if path.last() == 0
            && let Some((tp, r, c)) = path.cell()
            && let Some(t) = s.doc.table(StoryRef::Body, &tp)
        {
            let row = t.rows.get(r);
            let cell = row.and_then(|x| x.cells.get(c));
            let found = [
                (r == 0 && c == 0).then(|| t.props.fmt_change.as_ref().map(|ch| (ch.rev, fmt_revisions::table_diff(&t.props)))).flatten(),
                (c == 0).then(|| row.and_then(|x| x.props.fmt_change.as_ref().map(|ch| (ch.rev, fmt_revisions::row_diff(&x.props))))).flatten(),
                cell.and_then(|x| x.props.fmt_change.as_ref().map(|ch| (ch.rev, fmt_revisions::cell_diff(&x.props)))),
            ];
            for (rev, props) in found.into_iter().flatten() {
                out.push(Change::format(mk(&path, 0), mk(&path, 0), rev, props));
            }
        }
        if let Some(ch) = &p.props.fmt_change {
            out.push(Change::format(mk(&path, 0), mk(&path, p.len()), ch.rev, fmt_revisions::para_diff(&p.props)));
        }
        for (r, c) in p.run_ranges() {
            let kind = if c.ins.is_some() {
                "insert"
            } else if c.del.is_some() {
                "delete"
            } else {
                ""
            };
            if !kind.is_empty() {
                out.push(Change { a: mk(&path, r.start), b: mk(&path, r.end), kind, rev: c.ins.or(c.del), props: Vec::new() });
            }
            let Some(ch) = &c.fmt_change else { continue };
            let props = fmt_revisions::char_diff(c);
            // Neighbouring runs changed the same way by the same revision are one change.
            if let Some(last) =
                out.last_mut().filter(|l| l.kind == "format" && l.rev == Some(ch.rev) && l.b == mk(&path, r.start) && l.props == props)
            {
                last.b = mk(&path, r.end);
            } else {
                out.push(Change::format(mk(&path, r.start), mk(&path, r.end), ch.rev, props));
            }
        }
        if let Some(ch) = &p.mark.fmt_change {
            let end = mk(&path, p.len());
            out.push(Change::format(end.clone(), end, ch.rev, fmt_revisions::char_diff(&p.mark)));
        }
        out.extend(mark_change(s, StoryRef::Body, &path));
        if let Some(ch) = p.section.as_ref().and_then(|x| x.fmt_change.as_ref().map(|ch| (ch.rev, x))) {
            let end = mk(&path, p.len());
            out.push(Change::format(end.clone(), end, ch.0, fmt_revisions::section_diff(ch.1)));
        }
    }
    if let Some(ch) = &s.doc.last_section.fmt_change {
        let end = s.doc.end_of(StoryRef::Body);
        out.push(Change::format(end.clone(), end, ch.rev, fmt_revisions::section_diff(&s.doc.last_section)));
    }
    out
}

/// The formatting changes in the body, for balloons: (position, author, properties set).
pub fn format_changes(s: &Session) -> Vec<(Pos, String, Vec<Value>)> {
    changes(s)
        .into_iter()
        .filter(|c| c.kind == "format")
        .map(|c| {
            let author = c.rev.and_then(|r| s.doc.revisions.get(r as usize)).map(|r| r.author.clone()).unwrap_or_default();
            (c.a, author, c.props)
        })
        .collect()
}

/// Whether the document has any tracked formatting change (cheap: stops at the first).
pub fn has_format_changes(doc: &kition_doc_doc::Document) -> bool {
    if doc.last_section.fmt_change.is_some() {
        return true;
    }
    doc.para_paths(StoryRef::Body).iter().any(|path| {
        doc.para(StoryRef::Body, path).is_some_and(|p| {
            p.props.fmt_change.is_some()
                || p.mark.fmt_change.is_some()
                || p.runs.iter().any(|r| r.props.fmt_change.is_some())
                || p.section.as_ref().is_some_and(|x| x.fmt_change.is_some())
                || path.cell().and_then(|(tp, r, c)| doc.table(StoryRef::Body, &tp).map(|t| (t, r, c))).is_some_and(|(t, r, c)| {
                    t.props.fmt_change.is_some()
                        || t.rows.get(r).is_some_and(|x| x.props.fmt_change.is_some() || x.cells.get(c).is_some_and(|x| x.props.fmt_change.is_some()))
                })
        })
    })
}

fn nav_change(s: &mut Session, dir: i32) -> CmdResult {
    let list = changes(s);
    let caret = s.sel.ordered();
    let t = if dir > 0 {
        list.iter().find(|c| c.a >= caret.1 && (c.a > caret.0 || c.b > caret.1)).or(list.first())
    } else {
        list.iter().rev().find(|c| c.b <= caret.0 && (c.b < caret.1 || c.a < caret.0)).or(list.last())
    };
    if let Some(c) = t {
        s.sel = Selection { anchor: c.a.clone(), focus: c.b.clone() };
    }
    sel_result(s)
}

fn list_changes(s: &mut Session, _: &Value) -> CmdResult {
    let list = changes(s);
    Ok(Value::Array(
        list.iter()
            .map(|c| {
                let rev = c.rev.and_then(|r| s.doc.revisions.get(r as usize));
                let mut v = json!({
                    "kind": c.kind, "start": pos_json(&c.a), "end": pos_json(&c.b),
                    "text": s.doc.copy_range(&c.a, &c.b).plain_text(),
                    "author": rev.map(|r| r.author.clone()), "date": rev.map(|r| r.date.clone()),
                });
                if c.kind == "format" {
                    v["description"] = json!(fmt_revisions::describe(&c.props));
                    v["props"] = json!(c.props);
                }
                v
            })
            .collect(),
    ))
}

fn word_count(s: &mut Session, v: &Value) -> CmdResult {
    if let Some(b) = p::bool(v, "includeTextBoxes") {
        s.prefs.count_notes = b;
    }
    let stories = if s.prefs.count_notes { s.doc.counted_stories() } else { vec![StoryRef::Body] };
    // A selected picture, shape or text box isn't a text selection: count the document.
    let text = if s.sel.is_collapsed() || super::objects::object_selection(s).is_some() {
        stories.iter().map(|st| s.doc.plain_text(*st)).collect::<Vec<_>>().join("\n")
    } else {
        s.selected_text()
    };
    let words = kition_doc_doc::count_words(&text);
    let chars = text.chars().filter(|c| *c != '\n').count();
    let chars_no_spaces = text.chars().filter(|c| !c.is_whitespace()).count();
    let paragraphs = text.split('\n').filter(|l| !l.trim().is_empty()).count();
    let lines: usize = {
        let l = s.layout();
        l.pages
            .iter()
            .flat_map(|p| p.items.iter())
            .map(|it| {
                if let kition_doc_layout::Placed::Lines { l0, l1, story, .. } = it
                    && stories.contains(story)
                {
                    l1 - l0
                } else {
                    0
                }
            })
            .sum()
    };
    let pages = s.layout().pages.len();
    Ok(json!({
        "pages": pages, "words": words, "characters": chars_no_spaces, "charactersWithSpaces": chars, "paragraphs": paragraphs, "lines": lines,
        "includeTextBoxes": s.prefs.count_notes,
    }))
}

/// Issues (spelling + grammar) in one paragraph as positions.
fn para_issues(s: &Session, story: StoryRef, path: &kition_doc_doc::Path) -> Vec<(Pos, Pos, kition_doc_proof::Issue)> {
    let Some(p) = s.doc.para(story, path) else { return Vec::new() };
    let text = kition_doc_layout::para::proof_text(p);
    let mut v: Vec<kition_doc_proof::Issue> = kition_doc_proof::check_spelling(&text);
    v.extend(kition_doc_proof::check_grammar(&text));
    v.sort_by_key(|i| i.start);
    v.into_iter()
        .filter(|i| !p.run_ranges().any(|(r, c)| r.start < i.end && i.start < r.end && (c.no_proof == Some(true) || c.link.is_some())))
        .map(|i| (Pos { story, path: path.clone(), off: i.start }, Pos { story, path: path.clone(), off: i.end }, i))
        .collect()
}

/// The issue under a position.
fn issue_at(s: &Session, at: &Pos) -> Option<(Pos, Pos, kition_doc_proof::Issue)> {
    para_issues(s, at.story, &at.path).into_iter().find(|(a, b, _)| a.off <= at.off && at.off <= b.off)
}

fn issue_json(s: &Session, a: &Pos, b: &Pos, i: &kition_doc_proof::Issue) -> Value {
    let word = s.doc.para_at(a).and_then(|p| p.text.get(a.off..b.off)).unwrap_or("").to_string();
    let sugg = if i.kind == kition_doc_proof::IssueKind::Spelling { kition_doc_proof::suggest(&word, 6) } else { i.suggestions.clone() };
    json!({"start": pos_json(a), "end": pos_json(b), "text": word, "kind": if i.kind == kition_doc_proof::IssueKind::Spelling { "spelling" } else { "grammar" }, "message": i.message, "suggestions": sugg})
}

/// F7: select the next issue after the caret and return it with suggestions.
fn next_issue(s: &mut Session, _: &Value) -> CmdResult {
    let caret = s.sel.ordered().1;
    let story = caret.story;
    let paths = s.doc.para_paths(story);
    let start = paths.iter().position(|p| *p == caret.path).unwrap_or(0);
    for k in 0..paths.len() {
        let Some(path) = paths.get((start + k) % paths.len().max(1)) else { break };
        for (a, b, i) in para_issues(s, story, path) {
            if k == 0 && a.off < caret.off && start + k < paths.len() && paths.len() > 1 {
                continue;
            }
            let r = issue_json(s, &a, &b, &i);
            s.sel = Selection { anchor: a, focus: b };
            return Ok(r);
        }
    }
    s.status = "The spelling and grammar check is complete.".into();
    Ok(json!({"done": true}))
}

fn all_issues(s: &mut Session, _: &Value) -> CmdResult {
    let mut out = Vec::new();
    for path in s.doc.para_paths(StoryRef::Body) {
        for (a, b, i) in para_issues(s, StoryRef::Body, &path) {
            out.push(issue_json(s, &a, &b, &i));
            if out.len() >= 500 {
                return Ok(Value::Array(out));
            }
        }
    }
    Ok(Value::Array(out))
}

fn suggestions(s: &mut Session, v: &Value) -> CmdResult {
    let at = v.get("pos").and_then(super::parse_pos).unwrap_or_else(|| s.sel.focus.clone());
    match issue_at(s, &at) {
        Some((a, b, i)) => Ok(issue_json(s, &a, &b, &i)),
        None => Ok(Value::Null),
    }
}

/// A tiny built-in thesaurus of common words (our own list).
fn thesaurus(s: &mut Session, v: &Value) -> CmdResult {
    const T: &[(&str, &[&str])] = &[
        ("good", &["fine", "excellent", "decent", "solid", "worthy"]),
        ("bad", &["poor", "inferior", "faulty", "unpleasant"]),
        ("big", &["large", "huge", "sizable", "vast", "major"]),
        ("small", &["little", "tiny", "compact", "minor", "modest"]),
        ("fast", &["quick", "rapid", "swift", "speedy"]),
        ("slow", &["unhurried", "gradual", "leisurely", "sluggish"]),
        ("happy", &["glad", "cheerful", "content", "joyful", "pleased"]),
        ("sad", &["unhappy", "sorrowful", "downcast", "gloomy"]),
        ("important", &["significant", "key", "vital", "essential", "major"]),
        ("show", &["display", "reveal", "present", "demonstrate"]),
        ("make", &["create", "build", "produce", "craft", "form"]),
        ("use", &["employ", "apply", "utilize", "operate"]),
        ("help", &["assist", "aid", "support", "serve"]),
        ("start", &["begin", "launch", "open", "initiate"]),
        ("end", &["finish", "close", "conclude", "stop"]),
        ("say", &["state", "tell", "remark", "mention"]),
        ("think", &["believe", "consider", "reckon", "suppose"]),
        ("new", &["fresh", "novel", "recent", "modern"]),
        ("old", &["aged", "former", "ancient", "vintage"]),
        ("beautiful", &["lovely", "attractive", "gorgeous", "stunning"]),
        ("easy", &["simple", "effortless", "straightforward"]),
        ("hard", &["difficult", "tough", "demanding", "firm"]),
        ("work", &["labor", "effort", "job", "task", "operate"]),
        ("idea", &["concept", "notion", "thought", "plan"]),
        ("change", &["alter", "modify", "adjust", "shift"]),
        ("great", &["excellent", "superb", "terrific", "considerable"]),
        ("very", &["extremely", "highly", "truly", "really"]),
        ("shared", &["common", "joint", "communal", "mutual"]),
        ("community", &["group", "society", "collective", "circle"]),
    ];
    let word = match p::str(v, "word") {
        Some(w) => w.to_lowercase(),
        None => {
            let f = s.sel.focus.clone();
            let p = s.doc.para_at(&f).ok_or_else(|| CmdError::Failed("no word".into()))?;
            let (a, b) = p.word_at(f.off);
            p.text.get(a..b).unwrap_or("").trim().to_lowercase()
        }
    };
    let syn = T.iter().find(|(w, _)| *w == word).map(|(_, l)| l.to_vec()).unwrap_or_default();
    Ok(json!({"word": word, "synonyms": syn}))
}

/// Track Changes Options: change the fields given, return them all. A per-user preference.
fn tracking_options(s: &mut Session, v: &Value) -> CmdResult {
    use kition_doc_doc::Rgb;
    use kition_doc_layout::display::{BalloonMode, ChangeBar, DeleteMark, InsertMark};
    let mut m = s.prefs.markup.clone();
    let color = |k: &str, auto: &str, cur: Option<Rgb>| -> Result<Option<Rgb>, CmdError> {
        match p::str(v, k) {
            None => Ok(cur),
            Some(c) if c == auto => Ok(None),
            Some(c) => Rgb::parse(c).map(Some).ok_or_else(|| CmdError::Params(format!("`{k}`: \"{auto}\" or RRGGBB"))),
        }
    };
    let pick = |k: &str| p::str(v, k);
    for (k, f) in [
        ("comments", &mut m.comments),
        ("ink", &mut m.ink),
        ("insertionsDeletions", &mut m.insertions_deletions),
        ("formatting", &mut m.formatting),
        ("trackFormatting", &mut m.track_formatting),
    ] {
        if let Some(b) = p::bool(v, k) {
            *f = b;
        }
    }
    if let Some(x) = pick("balloons") {
        m.balloons = BalloonMode::parse(x).ok_or_else(|| CmdError::Params("`balloons`: revisions|inline|commentsAndFormatting".into()))?;
    }
    if let Some(x) = pick("insertMark") {
        m.insert_mark = InsertMark::parse(x).ok_or_else(|| CmdError::Params("unknown `insertMark`".into()))?;
    }
    if let Some(x) = pick("deleteMark") {
        m.delete_mark = DeleteMark::parse(x).ok_or_else(|| CmdError::Params("unknown `deleteMark`".into()))?;
    }
    if let Some(x) = pick("changedLines") {
        m.changed_lines = ChangeBar::parse(x).ok_or_else(|| CmdError::Params("unknown `changedLines`".into()))?;
    }
    m.insert_color = color("insertColor", "byAuthor", m.insert_color)?;
    m.delete_color = color("deleteColor", "byAuthor", m.delete_color)?;
    m.changed_lines_color = color("changedLinesColor", "auto", m.changed_lines_color)?;
    if m != s.prefs.markup {
        s.prefs.markup = m;
        // Hidden deletions leave the layout.
        s.relayout();
    }
    Ok(tracking_json(&s.prefs.markup))
}

/// The Track Changes Options as `change.options` takes them.
pub fn tracking_json(m: &kition_doc_layout::display::MarkupOptions) -> Value {
    let hex = |c: Option<kition_doc_doc::Rgb>, auto: &str| c.map(|c| c.hex()).unwrap_or_else(|| auto.to_string());
    json!({
        "comments": m.comments,
        "ink": m.ink,
        "insertionsDeletions": m.insertions_deletions,
        "formatting": m.formatting,
        "balloons": m.balloons.name(),
        "insertMark": m.insert_mark.name(),
        "insertColor": hex(m.insert_color, "byAuthor"),
        "deleteMark": m.delete_mark.name(),
        "deleteColor": hex(m.delete_color, "byAuthor"),
        "changedLines": m.changed_lines.name(),
        "changedLinesColor": hex(m.changed_lines_color, "auto"),
        "trackFormatting": m.track_formatting,
    })
}

// --- comment threads (Kition platform rule M23) ------------------------------------------------

/// The id the comment commands name comment `id` by (`c<id>`): stable while the document is open.
fn thread_key(id: u32) -> String {
    format!("c{id}")
}

/// A comment's tool id (C8-doc-6): `comment:` and its durable id (`w16cid:durableId`), which the
/// file keeps; `None` before the session gave it one ([`kition_doc_doc::Document::ensure_comment_ids`]).
pub fn comment_tool_id(c: &Comment) -> Option<String> {
    (!c.durable_id.is_empty()).then(|| format!("comment:{}", c.durable_id))
}

/// A thread's tool id: `thread:` and its first comment's durable id.
pub fn thread_tool_id(c: &Comment) -> Option<String> {
    (!c.durable_id.is_empty()).then(|| format!("thread:{}", c.durable_id))
}

/// The comment a parameter names: its number in this session (`c<n>` or `n`), or its tool id
/// (`comment:<durable id>`), or a thread's (`thread:<durable id>`, its first comment).
pub fn comment_param(s: &Session, v: &Value, key: &str) -> Result<u32, CmdError> {
    let raw = v.get(key).ok_or_else(|| CmdError::Params(format!("`{key}` required")))?;
    let id = match raw {
        Value::String(text) => match text.strip_prefix("comment:").or_else(|| text.strip_prefix("thread:")) {
            Some(durable) => {
                s.doc.comments.iter().find(|(_, c)| !c.durable_id.is_empty() && c.durable_id.eq_ignore_ascii_case(durable)).map(|(id, _)| *id)
            }
            None => text.strip_prefix('c').unwrap_or(text).parse::<u32>().ok(),
        },
        Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()),
        _ => None,
    };
    id.ok_or_else(|| CmdError::Params(format!("`{key}`: no such comment")))
}

/// The comment that opens `id`'s thread.
pub fn thread_root(s: &Session, id: u32) -> u32 {
    let mut at = id;
    for _ in 0..64 {
        match s.doc.comments.get(&at).and_then(|c| c.parent) {
            Some(parent) if parent != at && s.doc.comments.contains_key(&parent) => at = parent,
            _ => break,
        }
    }
    at
}

/// Every comment's range ends in the content (body, notes, headers, text boxes).
fn comment_ends(s: &Session) -> std::collections::HashMap<u32, (Option<Pos>, Option<Pos>)> {
    let mut out: std::collections::HashMap<u32, (Option<Pos>, Option<Pos>)> = std::collections::HashMap::new();
    let mut stories = vec![StoryRef::Body];
    stories.extend(s.doc.parts.iter().filter(|(_, p)| p.kind != PartKind::Comment).map(|(id, _)| StoryRef::Part(*id)));
    for story in stories {
        for path in s.doc.para_paths(story) {
            let Some(p) = s.doc.para(story, &path) else { continue };
            for off in p.object_offsets() {
                let pos = || Some(Pos { story, path: path.clone(), off });
                match p.object_at(off) {
                    Some(InlineObject::CommentStart { id }) => {
                        out.entry(*id).or_default().0.get_or_insert_with(|| Pos { story, path: path.clone(), off });
                    }
                    Some(InlineObject::CommentEnd { id }) => {
                        let entry = out.entry(*id).or_default();
                        if entry.1.is_none() {
                            entry.1 = pos();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

/// The text of a paragraph between two places, inline objects left out.
fn quote(s: &Session, a: &Pos, b: &Pos) -> String {
    if a.story != b.story || a.path != b.path || b.off <= a.off {
        return String::new();
    }
    s.doc
        .para_at(a)
        .and_then(|p| p.text.get(a.off..b.off))
        .map(|t| t.chars().filter(|c| *c != kition_doc_doc::para::OBJ).collect())
        .unwrap_or_default()
}

/// `comment.threads`: each thread (in the order of its range in the document, then the ones
/// without a range) with its comments, as `@kition/kdm`'s `CommentThread`; the anchor's `place`
/// is where the range starts on its page.
fn threads(s: &mut Session, _: &Value) -> CmdResult {
    let ends = comment_ends(s);
    let layout = s.layout();
    let mut roots: Vec<(Option<Pos>, u32)> = s
        .doc
        .comments
        .keys()
        .filter(|id| thread_root(s, **id) == **id)
        .map(|id| (ends.get(id).and_then(|e| e.0.clone().or(e.1.clone())), *id))
        .collect();
    roots.sort_by(|a, b| match (&a.0, &b.0) {
        (Some(x), Some(y)) => x.cmp(y).then(a.1.cmp(&b.1)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.1.cmp(&b.1),
    });
    let comment_json = |id: u32| -> Option<Value> {
        let c = s.doc.comments.get(&id)?;
        let author = if c.author_id.is_empty() { c.author.clone() } else { c.author_id.clone() };
        let created = if c.date_utc.is_empty() { c.date.clone() } else { c.date_utc.clone() };
        let mut out =
            json!({ "id": thread_key(id), "author": author, "name": c.author, "body": s.doc.plain_text(StoryRef::Part(c.part)), "created": created });
        if let Some(tool) = comment_tool_id(c) {
            out["toolId"] = json!(tool);
        }
        Some(out)
    };
    let mut out = Vec::new();
    for (start, root) in roots {
        let Some(c) = s.doc.comments.get(&root) else { continue };
        let mut members: Vec<u32> = vec![root];
        members.extend(s.doc.comments.keys().filter(|id| **id != root && thread_root(s, **id) == root));
        let end = ends.get(&root).and_then(|e| e.1.clone());
        let anchor = match &start {
            Some(a) => {
                let mut anchor = json!({ "object": format!("{}", pos_json(a)), "field": "text" });
                let q = end.as_ref().map(|b| quote(s, a, b)).unwrap_or_default();
                if !q.is_empty() {
                    anchor["quote"] = json!(q);
                }
                if let Some(caret) = layout.caret_on(a, s.page_hint) {
                    anchor["place"] = json!({ "page": caret.page, "x": caret.x, "y": caret.top, "height": caret.height });
                }
                anchor
            }
            None => json!({ "object": "document" }),
        };
        let mut thread = json!({
            "id": thread_key(root),
            "anchor": anchor,
            "resolved": c.resolved,
            "comments": members.into_iter().filter_map(comment_json).collect::<Vec<_>>(),
        });
        if let Some(tool) = thread_tool_id(c) {
            thread["toolId"] = json!(tool);
        }
        out.push(thread);
    }
    Ok(Value::Array(out))
}

/// Who writes a comment: `name` (shown) and the principal id `author` (when it is not the name),
/// from the params, else the session's author.
fn writer(s: &Session, v: &Value) -> (String, String, String) {
    let name = p::str(v, "name").filter(|n| !n.is_empty()).map(str::to_owned).unwrap_or_else(|| s.author.clone());
    let author = p::str(v, "author").filter(|a| *a != name).unwrap_or("").to_owned();
    let created = p::str(v, "created").filter(|c| !c.is_empty()).map(str::to_owned).unwrap_or_else(now_iso);
    (name, author, created)
}

/// `comment.add`: a thread on the selection (or the word at the caret).
fn add_thread(s: &mut Session, v: &Value) -> CmdResult {
    let text = p::req_str(v, "text")?.to_owned();
    let (name, author, created) = writer(s, v);
    let made = new_comment(s, &json!({ "text": text }))?;
    let id = made.get("id").and_then(Value::as_u64).and_then(|x| u32::try_from(x).ok()).ok_or_else(|| CmdError::Failed("no comment made".into()))?;
    if let Some(c) = s.doc.comments.get_mut(&id) {
        c.initials = name.split_whitespace().filter_map(|w| w.chars().next()).collect();
        c.author = name;
        c.author_id = author;
        c.date = created;
    }
    Ok(with_tool_ids(s, json!({ "thread": thread_key(id) }), id, None))
}

/// `answer` with the tool ids (C8-doc-6) of thread `root` (`threadId`) and of comment `comment`
/// (`commentId`): a session that keeps tool ids gives a new comment its durable id now, as it
/// would after the command.
fn with_tool_ids(s: &mut Session, mut answer: Value, root: u32, comment: Option<u32>) -> Value {
    if s.stable_ids {
        s.doc.ensure_comment_ids();
    }
    if let Some(tool) = s.doc.comments.get(&root).and_then(thread_tool_id) {
        answer["threadId"] = json!(tool);
    }
    if let Some(tool) = comment.and_then(|id| s.doc.comments.get(&id)).and_then(comment_tool_id) {
        answer["commentId"] = json!(tool);
    }
    answer
}

fn reply_thread(s: &mut Session, v: &Value) -> CmdResult {
    let thread = comment_param(s, v, "thread")?;
    if !s.doc.comments.contains_key(&thread) {
        return Err(CmdError::Params("`thread`: no such comment".into()));
    }
    let root = thread_root(s, thread);
    let text = p::req_str(v, "text")?.to_owned();
    let (name, author, created) = writer(s, v);
    let made = reply(s, &json!({ "id": root, "text": text }))?;
    let id = made.get("id").and_then(Value::as_u64).and_then(|x| u32::try_from(x).ok()).ok_or_else(|| CmdError::Failed("no reply made".into()))?;
    if let Some(c) = s.doc.comments.get_mut(&id) {
        c.initials = name.split_whitespace().filter_map(|w| w.chars().next()).collect();
        c.author = name;
        c.author_id = author;
        c.date = created;
    }
    Ok(with_tool_ids(s, json!({ "thread": thread_key(root), "comment": thread_key(id) }), root, Some(id)))
}

fn resolve_thread(s: &mut Session, v: &Value) -> CmdResult {
    let root = thread_root(s, comment_param(s, v, "thread")?);
    let c = s.doc.comments.get_mut(&root).ok_or_else(|| CmdError::Params("`thread`: no such comment".into()))?;
    c.resolved = p::bool(v, "resolved").unwrap_or(true);
    Ok(json!({ "thread": thread_key(root), "resolved": c.resolved }))
}

/// `comment.delete`: a reply, or the whole thread (its first comment, with its replies and range).
fn delete_from_thread(s: &mut Session, v: &Value) -> CmdResult {
    let root = thread_root(s, comment_param(s, v, "thread")?);
    if !s.doc.comments.contains_key(&root) {
        return Err(CmdError::Params("`thread`: no such comment".into()));
    }
    let comment = if v.get("comment").is_some() { comment_param(s, v, "comment")? } else { root };
    if comment == root {
        let mut ids: Vec<u32> = s.doc.comments.keys().copied().filter(|id| thread_root(s, *id) == root).collect();
        ids.sort_unstable();
        remove_anchors(s, &ids)?;
        for id in &ids {
            if let Some(c) = s.doc.comments.remove(id) {
                s.doc.parts.remove(&c.part);
            }
        }
        s.clamp_selection();
        return Ok(json!({ "deleted": ids.iter().map(|id| thread_key(*id)).collect::<Vec<_>>() }));
    }
    if thread_root(s, comment) != root {
        return Err(CmdError::Params("`comment` is not in this thread".into()));
    }
    // Its own replies answer the thread from now on.
    let parent = s.doc.comments.get(&comment).and_then(|c| c.parent);
    for c in s.doc.comments.values_mut() {
        if c.parent == Some(comment) {
            c.parent = parent;
        }
    }
    remove_anchors(s, &[comment])?;
    if let Some(c) = s.doc.comments.remove(&comment) {
        s.doc.parts.remove(&c.part);
    }
    s.clamp_selection();
    Ok(json!({ "deleted": [thread_key(comment)] }))
}

/// `comment.goto`: selects the thread's range.
fn goto_thread(s: &mut Session, v: &Value) -> CmdResult {
    let root = thread_root(s, comment_param(s, v, "thread")?);
    let ends = comment_ends(s);
    let Some((start, end)) = ends.get(&root).cloned() else { return sel_result(s) };
    match (start, end) {
        (Some(a), Some(b)) if a.story == b.story && a <= b => s.sel = Selection { anchor: a, focus: b },
        (Some(a), _) | (None, Some(a)) => s.sel = Selection::caret(a),
        (None, None) => {}
    }
    sel_result(s)
}

#[cfg(test)]
mod tests {
    use kition_doc_doc::Document;
    use kition_doc_layout::display::{DisplayOptions, Draw, page_display};
    use serde_json::json;

    use crate::Session;

    /// What the first page draws with the session's Track Changes Options.
    fn draws(s: &mut Session) -> (Vec<Draw>, f32) {
        let l = s.layout();
        let page = &l.pages[0];
        let opts = DisplayOptions { revisions: s.prefs.markup.clone(), ..Default::default() };
        (page_display(&s.doc, page, &opts), page.body.x)
    }

    fn glyph_colors(d: &[Draw]) -> Vec<(kition_doc_doc::Rgb, bool)> {
        d.iter().filter_map(|x| if let Draw::Glyphs { color, synth_bold, .. } = x { Some((*color, *synth_bold)) } else { None }).collect()
    }

    fn lines(d: &[Draw]) -> Vec<(f32, f32, f32, f32)> {
        d.iter().filter_map(|x| if let Draw::Line { x0, y0, x1, y1, .. } = x { Some((*x0, *y0, *x1, *y1)) } else { None }).collect()
    }

    /// C8-doc-3: a comment number past the 32 bits comments are numbered in names no comment; it
    /// was cut to its low bits, so `4294967296` deleted, replied to or resolved comment 0.
    #[test]
    fn a_comment_number_past_32_bits_names_no_comment() {
        let mut s = Session::new(Document::new());
        s.run("file.setText", &json!({"text": "words to talk about"})).unwrap();
        s.run("select.range", &json!({"anchor": {"block": 0, "off": 0}, "focus": {"block": 0, "off": 5}})).unwrap();
        s.run("comment.new", &json!({"text": "first"})).unwrap();
        let first = *s.doc.comments.keys().next().unwrap();
        let past = u64::from(first) + (1u64 << 32);
        assert!(s.run("comment.deleteCurrent", &json!({"id": past})).is_err());
        assert!(s.run("comment.replyCurrent", &json!({"id": past, "text": "x"})).is_err());
        assert!(s.run("comment.resolveCurrent", &json!({"id": past})).is_err());
        assert_eq!(s.doc.comments.len(), 1);
        assert!(!s.doc.comments[&first].resolved);
    }

    /// C8-doc-4: Delete and Resolve without an id act on the comment the selection is in; with the
    /// caret outside every comment's range they name no comment (they took the paragraph's first
    /// comment, wherever the caret was).
    #[test]
    fn a_comment_by_the_caret_is_the_one_the_caret_is_in() {
        let mut s = Session::new(Document::new());
        s.run("file.setText", &json!({"text": "words to talk about"})).unwrap();
        s.run("select.range", &json!({"anchor": {"block": 0, "off": 0}, "focus": {"block": 0, "off": 5}})).unwrap();
        s.run("comment.new", &json!({"text": "first"})).unwrap();
        // The caret in "about", outside the comment on "words".
        s.run("select.range", &json!({"anchor": {"block": 0, "off": 18}, "focus": {"block": 0, "off": 18}})).unwrap();
        assert!(s.run("comment.deleteCurrent", &json!({})).is_err());
        assert!(s.run("comment.resolveCurrent", &json!({})).is_err());
        assert_eq!(s.doc.comments.len(), 1);
        assert!(!s.doc.comments.values().any(|c| c.resolved));
        // The commented word selected whole (the focus at its end), and the caret inside it.
        let focus = s.doc.para_at(&s.sel.focus).map(|p| p.text.find("words").map_or(0, |i| i + 5)).unwrap_or(0);
        s.run("select.range", &json!({"anchor": {"block": 0, "off": 0}, "focus": {"block": 0, "off": focus}})).unwrap();
        s.run("comment.resolveCurrent", &json!({})).unwrap();
        assert!(s.doc.comments.values().all(|c| c.resolved));
        s.run("select.range", &json!({"anchor": {"block": 0, "off": 3}, "focus": {"block": 0, "off": 3}})).unwrap();
        s.run("comment.deleteCurrent", &json!({})).unwrap();
        assert!(s.doc.comments.is_empty());
    }

    /// C8-doc-4: resolving a reply resolves its thread: the flag is the thread's (Word keeps it on
    /// the first comment); it was set on the reply.
    #[test]
    fn resolving_a_reply_resolves_its_thread() {
        let mut s = Session::new(Document::new());
        s.run("file.setText", &json!({"text": "words to talk about"})).unwrap();
        s.run("select.range", &json!({"anchor": {"block": 0, "off": 0}, "focus": {"block": 0, "off": 5}})).unwrap();
        s.run("comment.new", &json!({"text": "first"})).unwrap();
        let first = *s.doc.comments.keys().next().unwrap();
        s.run("comment.replyCurrent", &json!({"id": first, "text": "reply"})).unwrap();
        let reply = *s.doc.comments.iter().find(|(_, c)| c.parent == Some(first)).unwrap().0;
        s.run("comment.resolveCurrent", &json!({"id": reply})).unwrap();
        assert!(s.doc.comments[&first].resolved);
        assert!(!s.doc.comments[&reply].resolved);
    }

    /// Track Changes Options change how an insertion and a deletion are drawn, the bar
    /// beside changed lines, and whether deletions take room.
    #[test]
    fn tracking_options_change_how_revisions_are_drawn() {
        let mut s = Session::new(Document::new());
        // No proofing squiggles among the lines.
        s.view.proofing = false;
        s.run("file.setText", &json!({"text": "keep gone"})).unwrap();
        s.run("change.track", &json!({"value": true})).unwrap();
        s.run("caret.docStart", &json!({})).unwrap();
        s.run("text.insert", &json!({"text": "new "})).unwrap();
        s.run("select.range", &json!({"anchor": {"block": 0, "off": 9}, "focus": {"block": 0, "off": 13}})).unwrap();
        s.run("text.delete", &json!({})).unwrap();

        // Defaults: underlined insertion and struck deletion in the author's colour, a bar left of
        // the text.
        let (d, body_x) = draws(&mut s);
        let bars: Vec<_> = lines(&d).into_iter().filter(|(x0, _, x1, _)| x0 == x1 && *x0 < body_x).collect();
        assert_eq!(bars.len(), 1, "one changed line, one bar in the left margin");
        let horizontal = lines(&d).into_iter().filter(|(_, y0, _, y1)| y0 == y1).count();
        assert_eq!(horizontal, 2, "an underline and a strikethrough: {:?}", lines(&d));
        let author = glyph_colors(&d).iter().filter(|(c, _)| *c != kition_doc_doc::Rgb::BLACK).count();
        assert!(author >= 2, "insertion and deletion in the author's colour");

        // Bold, fixed-colour insertions; hidden deletions; no bars.
        let r = s
            .run(
                "change.options",
                &json!({"insertMark": "bold", "insertColor": "00AA00", "deleteMark": "hidden", "changedLines": "none", "balloons": "inline"}),
            )
            .unwrap();
        assert_eq!((r["insertMark"].as_str(), r["deleteColor"].as_str(), r["balloons"].as_str()), (Some("bold"), Some("byAuthor"), Some("inline")));
        let (d, _) = draws(&mut s);
        assert!(lines(&d).is_empty(), "no underline, no strikethrough, no bar: {:?}", lines(&d));
        assert!(glyph_colors(&d).contains(&(kition_doc_doc::Rgb(0, 0xAA, 0), true)), "a bold green insertion");
        let text: String = d.iter().filter_map(|x| if let Draw::Glyphs { text, .. } = x { Some(text.as_str()) } else { None }).collect();
        assert!(!text.contains("gone"), "the deletion takes no room: {text:?}");

        // Unknown choices are refused; reading changes nothing.
        assert!(s.run("change.options", &json!({"deleteMark": "sparkles"})).is_err());
        assert_eq!(s.run("change.options", &json!({})).unwrap()["insertColor"], "00AA00");
    }

    fn body_para(s: &Session) -> &kition_doc_doc::Paragraph {
        s.doc.para(kition_doc_doc::StoryRef::Body, &kition_doc_doc::Path::top(0)).unwrap()
    }

    /// "plain words here" with "words" made bold, then 14 pt, and the paragraph centred, under
    /// Track Changes by Ana.
    fn formatted_while_tracking() -> Session {
        let mut s = Session::new(Document::new());
        s.view.proofing = false;
        s.author = "Ana".into();
        s.run("file.setText", &json!({"text": "plain words here"})).unwrap();
        // Untracked formatting is no revision.
        s.run("select.range", &json!({"anchor": {"block": 0, "off": 12}, "focus": {"block": 0, "off": 16}})).unwrap();
        s.run("format.italic", &json!({})).unwrap();
        assert!(body_para(&s).runs.iter().all(|r| r.props.fmt_change.is_none()));
        s.run("change.track", &json!({"value": true})).unwrap();
        s.run("select.range", &json!({"anchor": {"block": 0, "off": 6}, "focus": {"block": 0, "off": 11}})).unwrap();
        s.run("format.bold", &json!({})).unwrap();
        s.run("format.fontSize", &json!({"size": 14})).unwrap();
        s.run("format.alignCenter", &json!({})).unwrap();
        s
    }

    /// Formatting with Track Changes on records the formatting before the author's first
    /// change; it's listed in the Reviewing Pane and marked by a bar beside the line.
    #[test]
    fn formatting_is_tracked_while_tracking_changes() {
        let mut s = formatted_while_tracking();
        let p = body_para(&s);
        let ch = p.props_of_char(6).fmt_change.as_deref().expect("a formatting change on \"words\"");
        assert_eq!((ch.old.bold, ch.old.size), (None, None), "the formatting before the first change is kept");
        let rev = &s.doc.revisions[ch.rev as usize];
        assert_eq!((rev.kind, rev.author.as_str()), (kition_doc_doc::RevisionKind::Format, "Ana"));
        assert!(p.props_of_char(0).fmt_change.is_none() && p.props_of_char(13).fmt_change.is_none());
        assert_eq!(p.props.fmt_change.as_deref().map(|c| c.old.align), Some(None));

        let list = s.run("change.list", &json!({})).unwrap();
        let desc: Vec<&str> = list.as_array().unwrap().iter().filter(|c| c["kind"] == "format").filter_map(|c| c["description"].as_str()).collect();
        assert_eq!(desc, ["Formatted: Align: center", "Formatted: Bold, Size: 14"], "{list:#}");

        let (d, body_x) = draws(&mut s);
        let bars = lines(&d).into_iter().filter(|(x0, _, x1, _)| x0 == x1 && *x0 < body_x).count();
        assert_eq!(bars, 1, "one changed line, one bar");

        // Formatting changed back is no change any more.
        s.run("select.range", &json!({"anchor": {"block": 0, "off": 6}, "focus": {"block": 0, "off": 11}})).unwrap();
        s.run("format.set", &json!({"props": {"bold": false}})).unwrap();
        s.run("format.fontSize", &json!({"size": 11})).unwrap();
        assert!(body_para(&s).props_of_char(6).fmt_change.is_some(), "explicit Not Bold and 11 pt differ from inherited");
    }

    /// Rejecting restores the formatting before the change; accepting keeps the new
    /// formatting and drops the record. Both find formatting changes at the caret too.
    #[test]
    fn formatting_changes_are_accepted_and_rejected() {
        let mut s = formatted_while_tracking();
        s.run("change.rejectAll", &json!({})).unwrap();
        let p = body_para(&s);
        let c = p.props_of_char(6);
        assert_eq!((c.bold, c.size, c.fmt_change.is_none()), (None, None, true));
        assert_eq!((p.props.align, p.props.fmt_change.is_none()), (None, true));
        assert_eq!(p.props_of_char(13).italic, Some(true), "untracked formatting stays");

        s.run("edit.undo", &json!({})).unwrap();
        // Accept at the caret: just the run's change.
        s.run("select.range", &json!({"anchor": {"block": 0, "off": 8}, "focus": {"block": 0, "off": 8}})).unwrap();
        s.run("change.accept", &json!({})).unwrap();
        let p = body_para(&s);
        assert_eq!((p.props_of_char(6).bold, p.props_of_char(6).fmt_change.is_none()), (Some(true), true));
        assert!(p.props.fmt_change.is_some(), "the paragraph's change is still there");
        s.run("change.acceptAll", &json!({})).unwrap();
        let p = body_para(&s);
        assert_eq!((p.props.align, p.props.fmt_change.is_none()), (Some(kition_doc_doc::props::Align::Center), true));
        assert!(s.run("change.list", &json!({})).unwrap().as_array().unwrap().is_empty());
    }
}
