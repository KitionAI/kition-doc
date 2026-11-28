//! What an input method composes at the caret (plan "Doc usable by hand", stage 3; walkthrough
//! D16; WPS W-V1-W-V7): the pinyin (or kana, or hangul) not yet chosen shows in the text at the
//! caret, underlined, in the font and size the text typed there would have, and the text after it
//! moves on to make room, as WPS shows it. With text selected the composition shows in its place;
//! the selection comes back when the composition is taken away.
//!
//! It is session state, never the document: the pages are drawn from a copy of the document with
//! the composition typed in, laid out with the session's paragraph cache; the document, its
//! revision, its replica, its undo history and whether it needs saving stay as they were. The
//! composition shows while the document and the selection are as they were when it was set: a
//! click or a key elsewhere, an undo, or another person's or an agent's edit takes it away, and
//! what the input method commits after that goes in at the caret as it is then.
//!
//! The editor sends the composition from the composition events of kit-editor's text input
//! (`setPreedit`, in the editor's input order) and puts that input where the composition starts
//! (`inspect().inputAt`), so the candidate window opens under the line. The chosen text comes as
//! typed text ([`DocDocument::commit_preedit`]): one undo step of its own (WPS W-V5).

use std::sync::Arc;

use kition_doc_engine::Selection;
use kition_doc_engine::doc::{Document, Pos};
use kition_doc_engine::layout::DocLayout;
use kition_kit::tools::ToolOverlay;
use serde_json::{Value, json};

use crate::{CARET, DocDocument, PageArea};

/// The longest composition shown, in characters (an input method composes a few words at most).
pub(crate) const LONGEST: usize = 1000;
/// The line under a composition: dark, apart from the caret's colour.
pub(crate) const UNDERLINE: [u8; 4] = [64, 64, 64, 255];

/// A composition showing.
pub(crate) struct Preedit {
    /// What is composed (no line breaks or other control characters).
    pub(crate) text: String,
    /// The input method's cursor in it, in characters.
    cursor: usize,
    /// The document (its id and revision) and the selection it was typed into: it shows while
    /// they are the session's.
    document: u64,
    rev: u64,
    sel: Selection,
    /// The document with the composition typed in, and its layout: what the pages are drawn from.
    pub(crate) doc: Document,
    pub(crate) layout: Arc<DocLayout>,
    /// Where the composition is in `doc`, and the input method's cursor.
    start: Pos,
    end: Pos,
    at: Pos,
}

impl DocDocument {
    /// Shows `text` composing at the caret (over the selected text), the input method's cursor
    /// `cursor` characters into it; empty text takes the composition away. Answers whether a
    /// composition shows now: none while the document is open for commenting only, in an equation
    /// (whose own editing takes the typing) or with a column selection.
    pub fn set_preedit(&mut self, text: &str, cursor: usize) -> bool {
        let text: String = text.chars().filter(|c| !c.is_control()).take(LONGEST).collect();
        let had = self.preedit.take().is_some();
        let shown = if text.is_empty() { None } else { self.composed(text, cursor) };
        let showing = shown.is_some();
        self.preedit = shown;
        if had || showing {
            self.drawn = self.drawn.saturating_add(1);
        }
        showing
    }

    /// The document with `text` typed in at the selection, laid out.
    fn composed(&mut self, text: String, cursor: usize) -> Option<Preedit> {
        let mut session = self.session.borrow_mut();
        if session.comment_only() || session.math.is_some() || session.column_segments().is_some() {
            return None;
        }
        // Formatted as typing there would be: the caret's pending formatting, else the text's.
        let mut props = session.typing_props();
        props.ins = None;
        props.del = None;
        props.fmt_change = None;
        let mut doc = session.doc.clone();
        let (a, b) = session.sel.ordered();
        let start = if a == b { a } else { doc.delete_range(&a, &b).ok()? };
        let end = doc.insert_text(&start, &text, &props).ok()?;
        let length = text.chars().count();
        let cursor = cursor.min(length);
        let bytes: usize = text.chars().take(cursor).map(char::len_utf8).sum();
        let at = Pos { off: start.off.saturating_add(bytes), ..start.clone() };
        let layout = session.layout_of(&doc);
        Some(Preedit { text, cursor, document: session.document_id(), rev: session.rev(), sel: session.sel.clone(), doc, layout, start, end, at })
    }

    /// The composition the pages show now, if any (see the module's notes).
    pub(crate) fn shown_preedit(&self) -> Option<&Preedit> {
        let preedit = self.preedit.as_ref()?;
        let session = self.session.borrow();
        (session.document_id() == preedit.document && session.rev() == preedit.rev && session.sel == preedit.sel).then_some(preedit)
    }

    /// Takes away a composition the document or the selection has moved on from (a click, a key,
    /// an undo, another's edit), so the pages are drawn again without it.
    pub(crate) fn settle_preedit(&mut self) {
        if self.preedit.is_some() && self.shown_preedit().is_none() {
            self.preedit = None;
            self.drawn = self.drawn.saturating_add(1);
        }
    }

    /// Typed text ends the composition under way: what the input method chose, one undo step of
    /// its own (WPS W-V5), the typing before and after it steps of their own. Answers whether
    /// there was one.
    pub(crate) fn commit_preedit(&mut self) -> bool {
        if self.preedit.take().is_none() {
            return false;
        }
        self.drawn = self.drawn.saturating_add(1);
        self.session().close_typing();
        true
    }

    /// The composition for `inspect()`: its text and the input method's cursor (characters).
    pub(crate) fn preedit_info(&self) -> Value {
        self.shown_preedit().map_or(Value::Null, |preedit| json!({ "text": preedit.text, "cursor": preedit.cursor }))
    }

    /// Where a composition starts on the canvas (points): where it shows, else at the caret, or at
    /// the start of the selected text it would replace. The editor puts the input method's text
    /// input there, so the candidate window opens under the line, lined up with the composition.
    pub(crate) fn input_at(&self, layout: &DocLayout, areas: &[PageArea]) -> Value {
        let session = self.session.borrow();
        let caret = match self.shown_preedit() {
            Some(preedit) => preedit.layout.caret_on(&preedit.start, session.page_hint),
            None => layout.caret_on(&session.sel.ordered().0, session.page_hint),
        };
        caret
            .and_then(|caret| {
                let area = areas.get(caret.page)?;
                Some(json!({ "page": caret.page, "x": area.x + f64::from(caret.x), "top": area.y + f64::from(caret.top), "height": f64::from(caret.height) }))
            })
            .unwrap_or(Value::Null)
    }

    /// The caret at the input method's cursor and the line under the composition, as the pages
    /// show it; `None` with no composition showing.
    pub(crate) fn preedit_overlays(&self) -> Option<Vec<ToolOverlay>> {
        let preedit = self.shown_preedit()?;
        let page_hint = self.session.borrow().page_hint;
        let areas = Self::areas(&preedit.layout);
        let at = |page: usize, x: f32, y: f32| areas.get(page).map(|area| (area.x + f64::from(x), area.y + f64::from(y)));
        let mut out = Vec::new();
        for (page, rect) in preedit.layout.selection_rects(&preedit.doc, &preedit.start, &preedit.end, page_hint) {
            let under = rect.y + rect.h - 0.75;
            if let (Some((ax, ay)), Some((bx, by))) = (at(page, rect.x, under), at(page, rect.x + rect.w, under)) {
                out.push(ToolOverlay::Line { ax, ay, bx, by, color: UNDERLINE, dashed: false });
            }
        }
        if let Some(caret) = preedit.layout.caret_on(&preedit.at, page_hint)
            && let (Some((ax, ay)), Some((bx, by))) = (at(caret.page, caret.x, caret.top), at(caret.page, caret.x, caret.top + caret.height))
        {
            out.push(ToolOverlay::Line { ax, ay, bx, by, color: CARET, dashed: false });
        }
        Some(out)
    }
}
