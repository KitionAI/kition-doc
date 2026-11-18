//! What [`KitDocument::check`](kition_kit::KitDocument::check) reports on a document (Kit
//! Standard 2.0 A21): pictures whose image the document lacks, internal links and field
//! references to bookmarks that do not exist, fields whose result is an error, note references
//! without their note, headings with no text, table rows that do not fill the table's grid,
//! pictures without alternative text, fonts drawn with a substitute, and tracked changes left in
//! the document. Targets name the story and paragraph path (`body:3`, `body:2/0/1/0` inside a
//! table cell, `part:7:0` in a header, footer, note, or text box) and, in the body, the page.
//!
//! Codes: `doc.missing-media`, `doc.broken-link`, `doc.broken-reference`, `doc.field-error`,
//! `doc.missing-note`, `doc.empty-heading`, `doc.ragged-table`, `doc.missing-alt-text`,
//! `doc.font-substituted`, `doc.tracked-changes`, and `doc.truncated` when a code is found more
//! often than [`PER_CODE`] times.

use std::collections::{BTreeMap, BTreeSet};

use kition_doc_doc::para::{InlineObject, OBJ, Paragraph};
use kition_doc_doc::table::Table;
use kition_doc_doc::{Block, Blocks, Document, Path, Pos, StoryRef};
use kition_doc_engine::layout::DocLayout;
use kition_kit::diagnostics::{Diagnostic, Severity, Target};

/// At most this many diagnostics of one code; the rest are counted in one `doc.truncated`.
pub const PER_CODE: usize = 100;

/// Everything worth fixing in the document, errors first. `layout` (when given) adds the page of
/// findings in the body.
pub fn check(doc: &Document, layout: Option<&DocLayout>) -> Vec<Diagnostic> {
    let bookmarks = bookmarks(doc);
    let mut walk = Walk { doc, layout, bookmarks: &bookmarks, found: Found::default(), fonts: BTreeMap::new(), tracked: 0 };
    walk.blocks(StoryRef::Body, &doc.body, &[]);
    for (id, part) in &doc.parts {
        walk.blocks(StoryRef::Part(*id), &part.blocks, &[]);
    }
    let Walk { mut found, fonts, tracked, .. } = walk;
    for (family, target) in fonts {
        found.push(
            Diagnostic::new(
                "doc.font-substituted",
                Severity::Info,
                format!("The font \"{family}\" is not available here: its text is drawn with a substitute, so lines may break differently."),
            )
            .on(target),
        );
    }
    if tracked > 0 {
        found.push(
            Diagnostic::new(
                "doc.tracked-changes",
                Severity::Info,
                format!("{tracked} paragraphs hold tracked insertions or deletions that are not accepted or rejected yet."),
            )
            .fix("review.acceptAll", None),
        );
    }
    found.finish()
}

/// Every bookmark name in the document.
fn bookmarks(doc: &Document) -> BTreeSet<String> {
    fn collect(blocks: &Blocks, out: &mut BTreeSet<String>) {
        for block in blocks {
            match &**block {
                Block::Para(p) => {
                    for object in &p.objects {
                        if let InlineObject::BookmarkStart { name } = object {
                            out.insert(name.clone());
                        }
                    }
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            collect(&cell.blocks, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = BTreeSet::new();
    collect(&doc.body, &mut out);
    for part in doc.parts.values() {
        collect(&part.blocks, &mut out);
    }
    out
}

#[derive(Default)]
struct Found {
    list: Vec<Diagnostic>,
    counts: BTreeMap<String, usize>,
}

impl Found {
    fn push(&mut self, diagnostic: Diagnostic) {
        let count = self.counts.entry(diagnostic.code.clone()).or_default();
        *count += 1;
        if *count <= PER_CODE {
            self.list.push(diagnostic);
        }
    }

    fn finish(mut self) -> Vec<Diagnostic> {
        for (code, count) in &self.counts {
            if *count > PER_CODE {
                self.list.push(Diagnostic::new(
                    "doc.truncated",
                    Severity::Info,
                    format!("{} more `{code}` diagnostics are not listed.", count - PER_CODE),
                ));
            }
        }
        self.list.sort_by_key(|diagnostic| diagnostic.severity);
        self.list
    }
}

struct Walk<'a> {
    doc: &'a Document,
    layout: Option<&'a DocLayout>,
    bookmarks: &'a BTreeSet<String>,
    found: Found,
    /// Substituted font families, with where each is first used.
    fonts: BTreeMap<String, Target>,
    /// Paragraphs with tracked insertions or deletions.
    tracked: usize,
}

impl Walk<'_> {
    fn blocks(&mut self, story: StoryRef, blocks: &Blocks, prefix: &[u32]) {
        for (i, block) in blocks.iter().enumerate() {
            let mut path = prefix.to_vec();
            path.push(i as u32);
            match &**block {
                Block::Para(p) => self.paragraph(story, Path(path), p),
                Block::Table(t) => self.table(story, &path, t),
            }
        }
    }

    fn table(&mut self, story: StoryRef, path: &[u32], table: &Table) {
        let cols = table.grid.len();
        for (r, row) in table.rows.iter().enumerate() {
            let filled: usize = row.cells.iter().map(|cell| cell.span()).sum();
            if cols > 0 && filled != cols {
                let target = self.target(story, &Path(path.to_vec()));
                self.found.push(
                    Diagnostic::new(
                        "doc.ragged-table",
                        Severity::Warning,
                        format!(
                            "Row {} of a table spans {filled} of the table's {cols} columns: the row ends short of or past the table's edge.",
                            r + 1
                        ),
                    )
                    .on(target),
                );
            }
            for (c, cell) in row.cells.iter().enumerate() {
                let mut inner = path.to_vec();
                inner.extend([r as u32, c as u32]);
                self.blocks(story, &cell.blocks, &inner);
            }
        }
    }

    fn paragraph(&mut self, story: StoryRef, path: Path, p: &Paragraph) {
        let resolved = self.doc.styles.resolve_para(&p.props);
        if resolved.outline_level.is_some() && story == StoryRef::Body && !has_content(p) {
            let target = self.target(story, &path);
            self.found.push(
                Diagnostic::new(
                    "doc.empty-heading",
                    Severity::Warning,
                    "A heading has no text: it shows as an empty entry in the navigation pane and tables of contents.",
                )
                .on(target),
            );
        }

        let mut tracked = false;
        let mut start = 0;
        for run in &p.runs {
            let range = start..start + run.len;
            start += run.len;
            tracked |= run.props.ins.is_some() || run.props.del.is_some();
            if let Some(link) = &run.props.link
                && let Some(name) = link.strip_prefix('#')
                && !self.bookmarks.contains(name)
            {
                let target = self.target_at(story, &path, range.start);
                self.found.push(
                    Diagnostic::new(
                        "doc.broken-link",
                        Severity::Error,
                        format!("A link points to the bookmark \"{name}\", which the document does not have."),
                    )
                    .on(target)
                    .fix("insert.removeLink", None),
                );
            }
            let visible = p.text.get(range).is_some_and(|text| text.chars().any(|c| c != OBJ && !c.is_whitespace()));
            if visible {
                let font = self.doc.styles.resolve_char(p.props.style.as_deref(), &run.props).font;
                if !font.is_empty() && !self.fonts.contains_key(&font) && kition_doc_fonts::word::resolve(&font, false, false).substituted {
                    let target = self.target(story, &path);
                    self.fonts.insert(font, target);
                }
            }
        }
        if tracked {
            self.tracked += 1;
        }

        for (k, off) in p.object_offsets().into_iter().enumerate() {
            let Some(object) = p.objects.get(k) else { break };
            self.object(story, &path, off, object);
        }
    }

    fn object(&mut self, story: StoryRef, path: &Path, off: usize, object: &InlineObject) {
        match object {
            InlineObject::Image { media, alt, .. } => {
                let held = self.doc.media.get(media).is_some_and(|bytes| !bytes.is_empty());
                if !held {
                    let target = self.target_at(story, path, off);
                    self.found.push(
                        Diagnostic::new(
                            "doc.missing-media",
                            Severity::Error,
                            format!("A picture's image \"{media}\" is not in the document: it shows as an empty frame."),
                        )
                        .on(target),
                    );
                } else if alt.trim().is_empty() {
                    self.missing_alt(story, path, off);
                }
            }
            InlineObject::Graphic { alt, .. } if alt.trim().is_empty() => self.missing_alt(story, path, off),
            InlineObject::Field { instr, result, .. } => {
                self.field_instruction(story, path, off, instr);
                if result.trim_start().starts_with("Error!") {
                    let target = self.target_at(story, path, off);
                    self.found.push(
                        Diagnostic::new(
                            "doc.field-error",
                            Severity::Error,
                            format!("A field shows an error: \"{}\" ({}).", result.trim(), instr.trim()),
                        )
                        .on(target)
                        .fix("references.updateFields", None),
                    );
                }
            }
            InlineObject::FieldStart { instr, .. } => self.field_instruction(story, path, off, instr),
            InlineObject::NoteRef { kind, id, .. } if !self.doc.parts.contains_key(id) => {
                let noun = match kind {
                    kition_doc_doc::para::NoteKind::Footnote => "footnote",
                    kition_doc_doc::para::NoteKind::Endnote => "endnote",
                };
                let target = self.target_at(story, path, off);
                self.found.push(
                    Diagnostic::new("doc.missing-note", Severity::Error, format!("A {noun} reference has no {noun} text in the document."))
                        .on(target),
                );
            }
            _ => {}
        }
    }

    /// REF, PAGEREF, NOTEREF, and HYPERLINK \l fields name a bookmark the document must have.
    fn field_instruction(&mut self, story: StoryRef, path: &Path, off: usize, instr: &str) {
        let Some(name) = referenced_bookmark(instr) else { return };
        if self.bookmarks.contains(&name) {
            return;
        }
        let target = self.target_at(story, path, off);
        self.found.push(
            Diagnostic::new(
                "doc.broken-reference",
                Severity::Error,
                format!("A field refers to the bookmark \"{name}\", which the document does not have: {}", instr.trim()),
            )
            .on(target)
            .fix("references.updateFields", None),
        );
    }

    fn missing_alt(&mut self, story: StoryRef, path: &Path, off: usize) {
        let target = self.target_at(story, path, off);
        self.found.push(
            Diagnostic::new("doc.missing-alt-text", Severity::Info, "A picture or chart has no alternative text: screen readers cannot describe it.")
                .on(target)
                .fix("picture.altText", None),
        );
    }

    fn target(&self, story: StoryRef, path: &Path) -> Target {
        self.target_at(story, path, 0)
    }

    fn target_at(&self, story: StoryRef, path: &Path, off: usize) -> Target {
        let named = match story {
            StoryRef::Body => format!("body:{path}"),
            StoryRef::Part(id) => format!("part:{id}:{path}"),
        };
        let page = match (story, self.layout) {
            (StoryRef::Body, Some(layout)) => layout.page_of(&Pos { story, path: path.clone(), off }).map(|page| page as u32 + 1),
            _ => None,
        };
        Target { path: Some(named), page, ..Target::default() }
    }
}

/// Text or a drawing besides the paragraph's anchors (bookmarks, comment marks, fields' ends).
fn has_content(p: &Paragraph) -> bool {
    if p.text.chars().any(|c| c != OBJ && !c.is_whitespace()) {
        return true;
    }
    p.objects.iter().any(|object| object.is_drawing() || matches!(object, InlineObject::Equation { .. } | InlineObject::Field { .. }))
}

/// The bookmark a field instruction refers to: `REF name`, `PAGEREF name`, `NOTEREF name`, or
/// `HYPERLINK \l "name"`; `None` for other fields.
fn referenced_bookmark(instr: &str) -> Option<String> {
    let words = split_words(instr);
    let (first, rest) = words.split_first()?;
    match first.to_ascii_uppercase().as_str() {
        "REF" | "PAGEREF" | "NOTEREF" => rest.first().filter(|name| !name.starts_with('\\')).cloned(),
        "HYPERLINK" => rest.iter().position(|word| word.eq_ignore_ascii_case("\\l")).and_then(|i| rest.get(i + 1)).cloned(),
        _ => None,
    }
}

/// Field instruction words; a quoted word keeps its spaces.
fn split_words(instr: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut chars = instr.trim().chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            chars.next();
            let word: String = chars.by_ref().take_while(|c| *c != '"').collect();
            words.push(word);
        } else {
            let mut word = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                word.push(c);
                chars.next();
            }
            words.push(word);
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kition_doc_doc::para::{InlineObject, OBJ, Paragraph, Run};
    use kition_doc_doc::props::{CharProps, ParaProps};
    use kition_doc_doc::table::Table;
    use kition_doc_doc::{Block, Document, Part, PartKind};
    use kition_kit::diagnostics::check_diagnostics;

    use super::*;

    fn para(text: &str) -> Arc<Block> {
        Arc::new(Block::Para(Paragraph::with_text(text, CharProps::default())))
    }

    /// A paragraph whose U+FFFC characters are `objects` (`Paragraph::with_text` drops them).
    fn with_objects(text: &str, objects: Vec<InlineObject>) -> Arc<Block> {
        let mut p = Paragraph::new();
        p.text = text.into();
        p.runs = vec![Run { len: text.len(), props: CharProps::default() }];
        p.objects = objects;
        Arc::new(Block::Para(p))
    }

    /// What `check` finds besides fonts this machine lacks (the default body font may be one).
    fn found(doc: &Document) -> Vec<Diagnostic> {
        let found = check(doc, None);
        check_diagnostics(&found).expect("valid diagnostics");
        found.into_iter().filter(|d| d.code != "doc.font-substituted").collect()
    }

    fn codes(doc: &Document) -> Vec<String> {
        found(doc).into_iter().map(|d| d.code).collect()
    }

    fn image(media: &str, alt: &str) -> InlineObject {
        InlineObject::Image {
            edge: Default::default(),
            geom: None,
            media: media.into(),
            w: 10.0,
            h: 10.0,
            alt: alt.into(),
            float: Default::default(),
            crop: [0.0; 4],
            ole: None,
            adjust: Default::default(),
            uid: 0,
        }
    }

    fn field(instr: &str, result: &str) -> InlineObject {
        InlineObject::Field { instr: instr.into(), result: result.into(), locked: false, code_runs: Vec::new(), code_fields: Vec::new() }
    }

    #[test]
    fn a_plain_document_has_nothing_to_report() {
        let mut doc = Document::new();
        doc.body = vec![para("Hello world.")];
        assert!(found(&doc).is_empty());
    }

    #[test]
    fn pictures_without_their_image_or_alt_text() {
        let mut doc = Document::new();
        doc.media.insert("img1".into(), Arc::new(vec![1, 2, 3]));
        let text = format!("{OBJ}{OBJ}");
        doc.body = vec![with_objects(&text, vec![image("gone", "A chart"), image("img1", "")])];
        let found = found(&doc);
        assert_eq!(found.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(), ["doc.missing-media", "doc.missing-alt-text"]);
        assert_eq!(found[0].target.path.as_deref(), Some("body:0"));
        assert_eq!(found[1].fix.as_ref().map(|f| f.command.as_str()), Some("picture.altText"));
    }

    #[test]
    fn links_and_references_to_missing_bookmarks() {
        let mut doc = Document::new();
        let mut linked = Paragraph::with_text("see there", CharProps::default());
        linked.runs[0].props.link = Some("#nowhere".into());
        let mut kept = Paragraph::with_text("ok", CharProps::default());
        kept.runs[0].props.link = Some("#intro".into());
        let marks = format!("{OBJ}Intro");
        let fields = format!("{OBJ}{OBJ}{OBJ}");
        doc.body = vec![
            with_objects(&marks, vec![InlineObject::BookmarkStart { name: "intro".into() }]),
            Arc::new(Block::Para(linked)),
            Arc::new(Block::Para(kept)),
            with_objects(
                &fields,
                vec![field("REF intro \\h", "Intro"), field(" PAGEREF _Toc123 \\h ", "4"), field("HYPERLINK \\l \"lost one\"", "here")],
            ),
        ];
        let found = found(&doc);
        let codes: Vec<_> = found.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(codes, ["doc.broken-link", "doc.broken-reference", "doc.broken-reference"]);
        assert!(found[1].message.contains("_Toc123"));
        assert!(found[2].message.contains("lost one"));
    }

    #[test]
    fn fields_that_show_an_error() {
        let mut doc = Document::new();
        let text = format!("{OBJ}");
        doc.body = vec![with_objects(&text, vec![field("SEQ Figure", "Error! No sequence specified.")])];
        let found = found(&doc);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "doc.field-error");
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].fix.as_ref().map(|f| f.command.as_str()), Some("references.updateFields"));
    }

    #[test]
    fn note_references_without_their_note() {
        let mut doc = Document::new();
        doc.parts.insert(5, Part { kind: PartKind::Footnote, blocks: vec![para("The note.")] });
        let text = format!("A{OBJ}B{OBJ}");
        let note = |id| InlineObject::NoteRef { kind: kition_doc_doc::para::NoteKind::Footnote, id, custom: String::new() };
        doc.body = vec![with_objects(&text, vec![note(5), note(9)])];
        assert_eq!(codes(&doc), ["doc.missing-note"]);
    }

    #[test]
    fn headings_with_no_text() {
        let mut doc = Document::new();
        let mut empty = Paragraph::new();
        empty.props = ParaProps { style: Some("Heading1".into()), ..ParaProps::default() };
        let mut titled = Paragraph::with_text("Results", CharProps::default());
        titled.props = ParaProps { style: Some("Heading1".into()), ..ParaProps::default() };
        doc.body = vec![Arc::new(Block::Para(empty)), Arc::new(Block::Para(titled)), para("")];
        assert_eq!(codes(&doc), ["doc.empty-heading"]);
    }

    #[test]
    fn table_rows_that_do_not_fill_the_grid() {
        let mut doc = Document::new();
        let mut table = Table::new(2, 3, 300.0);
        table.rows[1].cells.pop();
        doc.body = vec![Arc::new(Block::Table(table)), para("")];
        let found = found(&doc);
        assert_eq!(found.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(), ["doc.ragged-table"]);
        assert!(found[0].message.contains("Row 2"));
    }

    #[test]
    fn fonts_drawn_with_a_substitute() {
        let mut doc = Document::new();
        let props = CharProps { font: Some("No Such Font Family 42".into()), ..CharProps::default() };
        doc.body =
            vec![Arc::new(Block::Para(Paragraph::with_text("Body", props.clone()))), Arc::new(Block::Para(Paragraph::with_text("More", props)))];
        let found = check(&doc, None);
        assert_eq!(found.iter().filter(|d| d.code == "doc.font-substituted").count(), 1);
        assert!(found.iter().any(|d| d.code == "doc.font-substituted" && d.message.contains("No Such Font Family 42")));
    }

    #[test]
    fn tracked_changes_left_in_the_document() {
        let mut doc = Document::new();
        let mut p = Paragraph::with_text("added", CharProps::default());
        p.runs[0].props.ins = Some(0);
        doc.revisions.push(Default::default());
        doc.body = vec![Arc::new(Block::Para(p))];
        let found = found(&doc);
        assert_eq!(found.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(), ["doc.tracked-changes"]);
        assert_eq!(found[0].fix.as_ref().map(|f| f.command.as_str()), Some("review.acceptAll"));
    }

    #[test]
    fn a_code_found_too_often_is_truncated() {
        let mut doc = Document::new();
        doc.body = (0..PER_CODE + 5)
            .map(|_| {
                let mut p = Paragraph::with_text("x", CharProps::default());
                p.runs[0].props.link = Some("#missing".into());
                Arc::new(Block::Para(p))
            })
            .collect();
        let found = found(&doc);
        assert_eq!(found.iter().filter(|d| d.code == "doc.broken-link").count(), PER_CODE);
        let last = found.last().expect("truncated");
        assert_eq!(last.code, "doc.truncated");
        assert!(last.message.starts_with("5 more"));
    }

    #[test]
    fn field_words_keep_quoted_spaces() {
        assert_eq!(referenced_bookmark("REF  name \\h"), Some("name".into()));
        assert_eq!(referenced_bookmark("HYPERLINK \"https://x\" \\l \"a b\""), Some("a b".into()));
        assert_eq!(referenced_bookmark("HYPERLINK \"https://x\""), None);
        assert_eq!(referenced_bookmark("PAGE"), None);
    }
}
