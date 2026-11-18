//! What each family of write commands rests on beyond the objects a step names (C8-doc-2 item 7),
//! read from the handlers in `crates/engine/engine/src/cmd/*.rs`. The SDK default holds every
//! field and position of each object a step names (`on`, a drawing's `ids`) and the selection's
//! state; [`crate::place`] holds positions, ranges and the call's selection. What the default
//! misses, by family (the table is in `agent/guide.md`):
//!
//! - **Every command that changes the document**: whether changes are tracked
//!   (`settings.trackChanges`: typing, deleting and formatting become revisions).
//! - **Character and paragraph formatting** (`format.*`, `para.*`): a toggle (bold, italic,
//!   hidden, …) and a relative change (grow font, distribute) read the formatting the selected
//!   text has with its paragraph style resolved: the selected paragraphs' styles, the styles they
//!   are based on, the document defaults, and each paragraph's style (a paragraph command its
//!   whole properties).
//! - **Lists and numbering** (`para.*Numbering`, `para.bullets`, `para.numbering`, `list.*`):
//!   the list definitions (`numbering`, `lists`) and the paragraphs around (where a list
//!   continues or restarts): the definitions and the body's order.
//! - **Tables** (`table.*`): the table the selection is in, whole (its grid and properties), and
//!   each of its rows' properties and cells' properties (merge, split, distribute, insert by row
//!   or column index); a table style without a name reads the styles.
//! - **Sections and pages** (`layout.*`): the section the selection is in (the last section's
//!   properties, a section break's on its paragraph): `lastSection` and the selected paragraphs.
//! - **Reading the whole document** (cross-references, tables of contents, captions, indexes,
//!   citations and bibliographies, field updates, mail merge, replace all, equation conversion of
//!   every equation): every paragraph's text and properties, and the root.
//! - **Revisions and comments by index** (`review.*`, `comment.*`): the root's revisions and
//!   comments; a comment command the comment and thread it names, by their tool ids.
//! - **Themes and document-wide design** (`design.*`), **styles without a name**: the root.
//! - **Pictures** (`picture.*`): the selected picture by its number; bitmap adjustments also
//!   read its media (`Media` has no tool id, so a bitmap change is the root's `media`).
//! - **Equations** (`equation.*`): the equation at the caret or being edited, by its number.
//! - **Headers and footers** (`hf.*`, `insert.header`, `insert.footer`): the parts; the header or
//!   footer a command replaces by its tool id (`part:header-1-default`), and every part's
//!   paragraph list where a command reads every story.
use kition_doc_engine::doc::{Block, Document, StoryRef};
use kition_kit::{Premise, Under};
use serde_json::Value;

use crate::DocDocument;

/// The object model's root, as change sets name it.
const ROOT: &str = "document";

/// The root's fields that are the document's content: all but the save stamp's `core` (any
/// save of another program changes it) and what the kit keeps of the file unread (`passthrough`).
const ROOT_CONTENT: [&str; 12] =
    ["settings", "styles", "numbering", "lists", "body", "parts", "comments", "revisions", "media", "lastSection", "sources", "customProps"];

/// Above this many paragraphs a command reading the whole document holds the root only (its
/// lists), not each paragraph.
const MAX_HELD: usize = 5000;

/// Commands that read the whole document's text.
fn reads_document(command: &str) -> bool {
    // These reference commands format one paragraph, insert a note or mark,
    // or copy/insert a named fragment. None scans the document.
    if matches!(
        command,
        "references.addText"
            | "references.footnote"
            | "references.endnote"
            | "references.markEntry"
            | "references.markCitation"
            | "insert.quickParts"
    ) {
        return false;
    }
    (command != "references.noteOptions" && matches!(command.split_once('.').map(|(family, _)| family), Some("references" | "mailings")))
        || matches!(
            command,
            "insert.crossReference"
                | "insert.caption"
                | "insert.toc"
                | "insert.index"
                | "insert.tableOfFigures"
                | "insert.quickParts"
                | "insert.field"
                | "edit.replaceAll"
                | "edit.replace"
                | "equation.convert"
                | "review.acceptAll"
                | "review.rejectAll"
                | "text.updateFields"
        )
}

/// Commands outside `layout.*` that act on the section the selection is in (`page::sect`,
/// `with_sect`: the section ending at the first section break at or after the selection's block),
/// from the reading (C8-doc-3).
const SECTION_READERS: &[&str] = &[
    "design.pageBorders",
    "arrange.align",
    "arrange.position",
    "table.autofit",
    "table.fromText",
    "table.quick",
    "table.draw",
    "insert.removeHeader",
    "insert.header",
    "insert.footer",
    "hf.position",
];

/// Commands on the selected drawing objects (`objects::selected_all`) and the fields of each they
/// read (a rotation turns from the angle the object has; its place and wrap are its `frame`, a
/// shape's fill and outline its `look`), from the reading (C8-doc-3).
fn on_selected_objects(command: &str) -> Option<&'static [&'static str]> {
    match command.split_once('.').map(|(family, _)| family) {
        Some("arrange") => Some(&["frame"]),
        // The fill, outline and kind are written as given; the effects read the look and frame.
        Some("shape") => Some(if command == "shape.effects" { &["look", "frame"] } else { &[] }),
        // A picture's own fields are its `value` (and its `frame`); a shape's look.
        Some("picture") => Some(match command {
            "picture.size" => &["frame", "value"],
            _ => &["value", "look", "frame"],
        }),
        _ => None,
    }
}

/// What a command of `design.*` reads of the root, where it is less than the whole (C8-doc-3):
/// the page colour and the watermark are settings written as given, the paragraph spacing the
/// styles' defaults; the themes, style sets and effects re-resolve the whole document.
fn design_reads(command: &str) -> Option<&'static [&'static str]> {
    match command {
        "design.effects" | "design.pageColor" | "design.watermark" => Some(&["settings"]),
        "design.paragraphSpacing" => Some(&["styles"]),
        "design.pageBorders" => Some(&["lastSection"]),
        "design.styleSet" | "design.theme" | "design.themeColors" | "design.themeFonts" => Some(&["settings", "styles", "lastSection"]),
        _ => None,
    }
}

/// Direct character assignments use `format::apply`, without resolving a paragraph
/// or character style. Their text/mark premises still hold every assigned property.
fn direct_character_assignment(command: &str, params: &Value) -> bool {
    matches!(
        command,
        "format.font"
            | "format.size"
            | "format.highlight"
            | "format.spacing"
            | "format.scale"
            | "format.position"
            | "format.set"
            | "format.color"
            | "format.shading"
            | "format.resetChar"
            | "format.clear"
            | "format.charStyle"
            | "format.changeCase"
            | "format.doubleUnderline"
            | "format.wordUnderline"
    ) || (command == "format.underline"
        && (params.get("style").and_then(Value::as_str).is_some() || params.get("value").and_then(Value::as_bool).is_some()))
        || (params.get("value").and_then(Value::as_bool).is_some()
            && matches!(
                command,
                "format.bold"
                    | "format.italic"
                    | "format.strikethrough"
                    | "format.doubleStrikethrough"
                    | "format.allCaps"
                    | "format.smallCaps"
                    | "format.hidden"
                    | "format.outline"
                    | "format.shadow"
                    | "format.emboss"
                    | "format.engrave"
                    | "format.subscript"
                    | "format.superscript"
                    | "format.border"
            ))
}

/// These operations consume direct paragraph properties. They never resolve
/// style definitions to decide the values they write (including their toggles).
fn direct_paragraph_properties(command: &str) -> bool {
    matches!(
        command,
        "para.set"
            | "para.indents"
            | "para.hangingIndent"
            | "para.removeHanging"
            | "para.rtl"
            | "para.ltr"
            | "para.lineSpacing"
            | "para.single"
            | "para.double"
            | "para.oneAndHalf"
            | "para.keepNext"
            | "para.keepLines"
            | "para.pageBreakBefore"
            | "para.widowControl"
            | "para.outlineLevel"
            | "para.shading"
            | "para.borders"
            | "para.sort"
    )
}

/// Longer plain replacement text cannot trigger single-character AutoFormat
/// or split a paragraph. It uses direct typing marks, held by place.
fn plain_paragraph_replacement(command: &str, params: &Value) -> bool {
    command == "para.text"
        && params.get("text").and_then(Value::as_str).is_some_and(|text| text.chars().count() > 1 && !text.contains(['\r', '\n', '\u{2029}']))
}

/// The section breaks the section of the blocks `from..=to` of the body depends on: every
/// top-level paragraph from `from` to the first one at or after `to` ending a section (the last
/// block when none does), by its tool id. Another writer's break between them makes the
/// selection's section another one.
fn section_breaks(doc: &Document, from: usize, to: usize) -> Vec<String> {
    let mut ids = Vec::new();
    for (i, block) in doc.body.iter().enumerate().skip(from) {
        let Block::Para(p) = &**block else { continue };
        if let Some(v) = p.props.para_id {
            ids.push(format!("p{v:08X}"));
        }
        if i >= to && p.section.is_some() {
            break;
        }
    }
    ids
}

/// The root's fields a command reads, per command, from the independent reading (C8-doc-4): the
/// section a page-wide object is sized or placed on (`lastSection`: a table, a picture, a
/// header), the styles a toggle or a built paragraph resolves, the list definitions a list
/// command continues, the parts notes, comments and text boxes live in, the media a picture
/// shows, the sources a citation names. A command whose reading names a field on one rare path
/// only (an AutoFormat while typing) is not listed: it would hold the field on every path.
fn root_reads(command: &str) -> &'static [&'static str] {
    match command {
        // Note Options echoes both formats; Hyphenation assigns its setting even when
        // an absolute value was given. Neither command consumes a page section.
        "references.noteOptions" => &["settings.footnoteFormat", "settings.endnoteFormat"],
        "layout.hyphenation" => &["settings.autoHyphenation"],
        "layout.differentOddEven" => &["settings.evenOddHeaders"],
        "comment.add" | "comment.delete" | "comment.reply" | "review.deleteComment" | "review.newComment" | "review.reply" => &["comments", "parts"],
        "file.properties" => &["customProps"],
        // C8-doc-6: a chart keeps its parts as read; Set Text replaces the body.
        "drawing.chart" => &["passthrough"],
        "document.setText" => &["body"],
        "table.deleteStyle" => &["body", "parts", "styles"],
        "edit.pasteAllClipboard" | "edit.pasteClipboardItem" | "edit.pasteMerge" => &["revisions"],
        "design.pageBorders"
        | "hf.position"
        | "insert.coverPage"
        | "insert.removeFooter"
        | "insert.removeHeader"
        | "layout.columns"
        | "layout.differentFirstPage"
        | "layout.lineNumbers"
        | "layout.linkToPrevious"
        | "layout.margins"
        | "layout.orientation"
        | "layout.pageNumberFormat"
        | "layout.pageSetup"
        | "layout.size"
        | "layout.verticalAlign"
        | "review.acceptAll"
        | "review.rejectAll"
        | "table.draw"
        | "table.fromText"
        | "table.quick" => &["lastSection"],
        "references.updateFields" => &["lastSection", "lists", "numbering", "sources", "styles"],
        "references.toc" | "references.updateToc" => &["lastSection", "lists", "numbering", "styles"],
        "insert.picture" => &["lastSection", "media"],
        "insert.editFooter" | "insert.editHeader" | "insert.footer" | "insert.header" | "insert.pageNumber" => &["lastSection", "parts"],
        "references.bibliography" | "references.citationStyle" | "references.updateFigures" | "references.updateIndex" => {
            &["lastSection", "sources", "styles"]
        }
        "design.styleSet"
        | "design.theme"
        | "design.themeColors"
        | "design.themeFonts"
        | "insert.spreadsheet"
        | "insert.table"
        | "references.index"
        | "references.tableOfAuthorities"
        | "references.tableOfFigures"
        | "table.autofit" => &["lastSection", "styles"],
        "para.bullets"
        | "para.defineBullet"
        | "para.defineNumber"
        | "para.multilevel"
        | "para.numbering"
        | "para.restartNumbering"
        | "para.setNumberingValue" => &["lists", "numbering"],
        "insert.field" => &["lists", "numbering", "sources", "styles"],
        "list.define" => &["lists", "numbering", "styles"],
        "picture.border"
        | "picture.change"
        | "picture.color"
        | "picture.compress"
        | "picture.corrections"
        | "picture.effects"
        | "picture.removeBackground"
        | "picture.style"
        | "picture.transparency" => &["media"],
        "drawing.delete"
        | "drawing.text"
        | "insert.textBox"
        | "insert.textFromFile"
        | "insert.artText"
        | "references.endnote"
        | "references.footnote" => &["parts"],
        "references.citation" | "references.sources" => &["sources"],
        "design.paragraphSpacing"
        | "format.allCaps"
        | "format.bold"
        | "format.border"
        | "format.charStyle"
        | "format.doubleStrikethrough"
        | "format.emboss"
        | "format.engrave"
        | "format.growFont"
        | "format.growFont1"
        | "format.hidden"
        | "format.italic"
        | "format.outline"
        | "format.shadow"
        | "format.shrinkFont"
        | "format.shrinkFont1"
        | "format.smallCaps"
        | "format.strikethrough"
        | "format.subscript"
        | "format.superscript"
        | "format.underline"
        | "insert.blankPage"
        | "insert.crossReference"
        | "para.addSpaceBefore"
        | "para.align"
        | "para.alignCenter"
        | "para.alignLeft"
        | "para.alignRight"
        | "para.asianTypography"
        | "para.distribute"
        | "para.heading1"
        | "para.heading2"
        | "para.heading3"
        | "para.indent"
        | "para.insert"
        | "para.justify"
        | "para.normal"
        | "para.outdent"
        | "para.removeSpaceAfter"
        | "para.spacing"
        | "para.style"
        | "para.tabs"
        | "styles.addToGallery"
        | "styles.create"
        | "styles.delete"
        | "styles.modify"
        | "styles.setVisibility"
        | "styles.updateToMatch"
        | "table.modifyStyle"
        | "table.newStyle"
        | "table.style" => &["styles"],
        _ => &[],
    }
}

/// The document properties (`core`) a command reads, per command from the reading (C8-doc-5):
/// never all of `core`, whose save stamp (`modified`, `revision`, `lastModifiedBy`) any save of
/// any program changes (`session::stamp_core`), but the members its code reads: a Title or Author
/// field's text (`references.rs` `TITLE`/`AUTHOR`, `tools.rs` `insert.docProperty`), a cover
/// page's title, subtitle and author (`insert.rs` `cover_page`), a header's title preset. A save
/// stamps the revision it reads; File › Info reads who made and last saved the document.
fn core_reads(command: &str, params: &Value) -> Vec<&'static str> {
    let member = |name: &str| -> Option<&'static str> {
        Some(match name.to_lowercase().as_str() {
            "title" => "core.title",
            "author" | "creator" => "core.creator",
            "subject" => "core.subject",
            "keywords" => "core.keywords",
            "comments" | "description" => "core.description",
            "category" => "core.category",
            _ => return None,
        })
    };
    match command {
        "insert.docProperty" => member(params.get("name").and_then(Value::as_str).unwrap_or("Title")).into_iter().collect(),
        "insert.coverPage" => vec!["core.title", "core.subject", "core.creator"],
        "insert.header" => vec!["core.title"],
        "insert.field" | "references.updateFields" | "text.updateFields" => vec!["core.title", "core.creator"],
        "file.inspect" => vec!["core.creator", "core.lastModifiedBy"],
        "file.save" | "file.saveAs" => vec!["core.revision"],
        "file.properties" => params.as_object().into_iter().flatten().filter_map(|(key, _)| member(key)).collect(),
        _ => Vec::new(),
    }
}

/// The premises on table `t` whole: the table (its grid, its properties, its rows) and its rows
/// (their heights, their cells' widths and spans: objects of their own), each its properties and
/// its cells' properties.
fn table_premises(t: &kition_doc_engine::doc::table::Table, under: &mut Under) {
    let Some(v) = t.rows.first().and_then(|r| r.props.para_id) else { return };
    under.premises.push(Premise::on(format!("t{v:08X}").as_str()));
    for row in &t.rows {
        if let Some(r) = row.props.para_id {
            let cells: Vec<String> = (0..row.cells.len()).map(|k| format!("cells.{k}.props")).collect();
            under.premises.push(
                Premise::on(format!("r{r:08X}").as_str()).fields([vec!["props".to_owned(), "cells".to_owned()], cells].concat()).positions(["rows"]),
            );
        }
    }
}

/// Table-wide property writes that neither consume the cells nor depend on
/// which row/cell contains the caret. All assigned fields remain premises.
pub(crate) fn table_property_fields(command: &str, params: &Value) -> Option<Vec<&'static str>> {
    match command {
        "table.style" => Some(vec!["table.props.style"]),
        "table.look" => Some(vec!["table.props.look"]),
        "table.cellMargins" => Some(vec!["table.props.cellMargins"]),
        "table.properties" => {
            if ["rowHeight", "rowHeightRule", "allowBreak", "headerRow", "columnWidth", "cellWidth", "valign"]
                .iter()
                .any(|key| params.get(*key).is_some())
            {
                return None;
            }
            let mut fields = Vec::new();
            if params.get("align").is_some() {
                fields.push("table.props.align");
            }
            if params.get("indent").is_some() {
                fields.push("table.props.indent");
            }
            if params.get("width").is_some() || params.get("widthPct").is_some() {
                fields.extend(["table.props.width", "table.props.widthPct"]);
            }
            if fields.is_empty() { None } else { Some(fields) }
        }
        _ => None,
    }
}

/// Creating a style without applying it, or modifying a named style without
/// Apply, does not consult the table at the caret. The styles remain held.
pub(crate) fn table_definition_only(command: &str, params: &Value) -> bool {
    (command == "table.newStyle" && params.get("apply").and_then(Value::as_bool) == Some(false))
        || (command == "table.modifyStyle"
            && params.get("style").and_then(Value::as_str).is_some()
            && params.get("apply").and_then(Value::as_bool) != Some(true))
}

/// Commands outside `table.*` that read the tables their selection touches whole (C8-doc-5, from
/// the reading): a cut copies a table it spans, Accept and Reject read a cell's, row's and table's
/// tracked changes, Tab and Shift+Tab move between cells (and add a row at the last), Backspace
/// at a paragraph after a table goes into its last cell.
const TABLE_READERS: &[&str] = &["edit.cut", "review.accept", "review.reject", "text.tab", "text.backTab", "text.backspace"];

/// The top-level tables of the body from block `a` to block `b` (and the block before `a` for
/// Backspace, which goes into a table before its paragraph).
fn touched_tables<'d>(
    doc: &'d Document,
    a: &kition_doc_engine::doc::Pos,
    b: &kition_doc_engine::doc::Pos,
    before: bool,
) -> Vec<&'d kition_doc_engine::doc::table::Table> {
    if a.story != StoryRef::Body {
        return Vec::new();
    }
    let (from, to) = (a.path.0.first().copied().unwrap_or(0) as usize, b.path.0.first().copied().unwrap_or(0) as usize);
    let from = if before { from.saturating_sub(1) } else { from };
    doc.body
        .iter()
        .skip(from)
        .take(to.saturating_sub(from) + 1)
        .filter_map(|block| if let Block::Table(t) = &**block { Some(t) } else { None })
        .collect()
}

fn lists(command: &str) -> bool {
    command != "para.outlineLevel"
        && (command.starts_with("list.")
            || matches!(command, "para.defineBullet" | "para.defineNumber" | "para.multilevel")
            || (command.starts_with("para.")
                && ["umbering", "bullets", "Bullets", "List", "list", "Level"].iter().any(|word| command.contains(word))))
}

fn disabling_list(command: &str, params: &Value) -> bool {
    matches!(command, "para.bullets" | "para.numbering") && params.get("off").and_then(Value::as_bool) == Some(true)
}

/// New default levels have no style links. Without `apply`, this definition
/// neither reads nor assigns any paragraph or style.
pub(crate) fn new_unlinked_list(command: &str, params: &Value) -> bool {
    command == "list.define"
        && params.get("num").is_none()
        && params
            .get("levels")
            .and_then(Value::as_array)
            .is_some_and(|levels| levels.iter().all(|level| level.get("style").is_none_or(Value::is_null)))
}

/// The tool ids of every paragraph in every story.
fn every_paragraph(doc: &Document) -> Vec<String> {
    let stories = std::iter::once(StoryRef::Body)
        .chain(doc.parts.iter().filter(|(_, p)| p.kind != kition_doc_engine::doc::PartKind::Comment).map(|(k, _)| StoryRef::Part(*k)));
    let mut ids = Vec::new();
    for story in stories {
        for path in doc.para_paths(story) {
            let pos = kition_doc_engine::doc::Pos::new(story, path, 0);
            if let Some(v) = doc.para_at(&pos).and_then(|p| p.props.para_id) {
                ids.push(format!("p{v:08X}"));
            }
        }
    }
    ids
}

/// The section breaks of the section the selection `a..b` is in (see [`section_breaks`]): the
/// engine counts the selection's block in the body, or, in a header or note, the body place the
/// caret came from (`body_return`, `page::body_block`); none with neither.
/// Which sections a section command acts on (C8-doc-6): the selection's (by default), every one
/// from the selection's to the end (`layout.columns` `apply: "forward"`), or every one
/// (`apply`/`applyTo: "document"`, Remove Header, which scans every body paragraph).
#[derive(Clone, Copy, PartialEq, Eq)]
enum SectionScope {
    Selection,
    Forward,
    Document,
}

fn section_scope(command: &str, params: &Value) -> SectionScope {
    let apply = params.get("apply").or_else(|| params.get("applyTo")).and_then(Value::as_str);
    match (command, apply) {
        ("insert.removeHeader", _) | (_, Some("document")) => SectionScope::Document,
        ("layout.columns", Some("forward")) => SectionScope::Forward,
        _ => SectionScope::Selection,
    }
}

fn section_premises(
    doc: &Document,
    body_return: Option<&kition_doc_engine::doc::Pos>,
    a: &kition_doc_engine::doc::Pos,
    b: &kition_doc_engine::doc::Pos,
    scope: SectionScope,
    under: &mut Under,
) {
    if scope != SectionScope::Selection {
        // Every section break from the selection's block (or the first) to the end.
        let from = match scope {
            SectionScope::Forward => (if a.story == StoryRef::Body { Some(a) } else { body_return.filter(|p| p.story == StoryRef::Body) })
                .map_or(0, |p| p.path.0.first().copied().unwrap_or(0) as usize),
            _ => 0,
        };
        for id in section_breaks(doc, from, usize::MAX) {
            under.premises.push(Premise::on(id.as_str()).fields(["para.section"]).positions(Vec::<String>::new()));
        }
        return;
    }
    let first = |pos: &kition_doc_engine::doc::Pos| {
        let pos = if pos.story == StoryRef::Body { Some(pos) } else { body_return.filter(|p| p.story == StoryRef::Body) };
        pos.map(|p| p.path.0.first().copied().unwrap_or(0) as usize)
    };
    let (Some(from), Some(to)) = (first(a), first(b)) else { return };
    for id in section_breaks(doc, from, to) {
        under.premises.push(Premise::on(id.as_str()).fields(["para.section"]).positions(Vec::<String>::new()));
    }
}

/// The drawing objects and pictures in the selection `a..b`, or the one at or just before a
/// caret, as the engine's `objects::selected_all` picks them (the selection a step acts on may be
/// one an earlier step of the run sets, so it is passed here).
fn selected_drawings<'d>(
    doc: &'d Document,
    a: &kition_doc_engine::doc::Pos,
    b: &kition_doc_engine::doc::Pos,
) -> Vec<&'d kition_doc_engine::doc::InlineObject> {
    let len = kition_doc_engine::doc::para::OBJ.len_utf8();
    let paths = if a == b { vec![a.path.clone()] } else { doc.paths_between(a, b) };
    let mut out = Vec::new();
    for path in paths {
        let Some(p) = doc.para(a.story, &path) else { continue };
        for off in p.object_offsets() {
            let inside =
                if a == b { off + len == a.off || off == a.off } else { (path != a.path || off >= a.off) && (path != b.path || off < b.off) };
            if let Some(o) = p.object_at(off).filter(|o| inside && o.is_drawing()) {
                out.push(o);
                if a == b {
                    return out;
                }
            }
        }
    }
    out
}

/// The comment a comment command acts on (C8-doc-6), as its handler resolves it
/// (`review.rs` `comment_param`, `comment_at_caret`): the one its `thread` (and `comment`) or `id`
/// names, else, for Delete and Resolve, the one the selection is in.
fn named_comment(session: &kition_doc_engine::Session, command: &str, params: &Value) -> Option<u32> {
    use kition_doc_engine::cmd::review;
    match command {
        "comment.reply" | "comment.resolve" => review::comment_param(session, params, "thread").ok(),
        "comment.delete" => match params.get("comment") {
            Some(_) => review::comment_param(session, params, "comment").ok(),
            None => review::comment_param(session, params, "thread").ok(),
        },
        "review.reply" | "review.editComment" => review::comment_param(session, params, "id").ok(),
        "review.deleteComment" | "review.resolveComment" if params.get("all").and_then(Value::as_bool) != Some(true) => {
            match params.get("id").filter(|id| !id.is_null()) {
                Some(_) => review::comment_param(session, params, "id").ok(),
                None => review::comment_at_caret(session),
            }
        }
        _ => None,
    }
}

/// The paragraph id of the paragraph comment `id`'s range starts in.
fn anchored_in(doc: &Document, id: u32) -> Option<u32> {
    let stories = std::iter::once(StoryRef::Body).chain(doc.parts.keys().map(|k| StoryRef::Part(*k)));
    for story in stories {
        for path in doc.para_paths(story) {
            let Some(p) = doc.para_at(&kition_doc_engine::doc::Pos::new(story, path, 0)) else { continue };
            if p.objects.iter().any(|o| matches!(o, kition_doc_engine::doc::InlineObject::CommentStart { id: c } if *c == id)) {
                return p.props.para_id;
            }
        }
    }
    None
}

/// The premises a comment command rests on (C8-doc-6), by the tool ids comments and threads have
/// (`comment:<durable id>`, `thread:<its first comment's>`): a comment's `props` hold its parent
/// (`replyTo`, and its thread's list) and a reply's resolved flag, its `body` its text; a thread
/// holds its comments (`comments`) and its first comment's resolved flag (`resolved`).
fn comment_premises(session: &kition_doc_engine::Session, command: &str, params: &Value, under: &mut Under) {
    use kition_doc_engine::cmd::review;
    // Document Inspector's Remove Comments deletes every thread and every comment's text.
    if (command == "file.inspect" && params.get("remove").and_then(Value::as_array).is_some_and(|r| r.iter().any(|v| v == "comments")))
        || (command == "review.deleteComment" && params.get("all") == Some(&Value::Bool(true)))
    {
        under.premises.push(Premise::on(ROOT).fields(["~commentCount"]).positions(Vec::<String>::new()));
        for (id, c) in &session.doc.comments {
            if let Some(tool) = review::comment_tool_id(c) {
                under.premises.push(Premise::on(tool.as_str()).positions(Vec::<String>::new()));
            }
            if let Some(tool) = (review::thread_root(session, *id) == *id).then(|| review::thread_tool_id(c)).flatten() {
                under.premises.push(Premise::on(tool.as_str()).fields(["comments"]).positions(Vec::<String>::new()));
                if let Some(v) = anchored_in(&session.doc, *id) {
                    under.premises.push(Premise::on(format!("p{v:08X}").as_str()).fields(["~comments"]).positions(Vec::<String>::new()));
                }
            }
        }
        return;
    }
    let Some(id) = named_comment(session, command, params) else { return };
    let comments = &session.doc.comments;
    if !comments.contains_key(&id) {
        return;
    }
    let root = review::thread_root(session, id);
    // `comment.delete` without a `comment` deletes the whole thread its `thread` is in.
    let id = if command == "comment.delete" && params.get("comment").is_none() { root } else { id };
    let members: Vec<u32> = comments.keys().copied().filter(|c| review::thread_root(session, *c) == root).collect();
    let comment = |id: u32, fields: &[&str], under: &mut Under| {
        if let Some(tool) = comments.get(&id).and_then(review::comment_tool_id) {
            under.premises.push(Premise::on(tool.as_str()).fields(fields.iter().copied()).positions(Vec::<String>::new()));
        }
    };
    let thread = |fields: &[&str], under: &mut Under| {
        if let Some(tool) = comments.get(&root).and_then(review::thread_tool_id) {
            under.premises.push(Premise::on(tool.as_str()).fields(fields.iter().copied()).positions(Vec::<String>::new()));
        }
    };
    // A reread of the file tells another program's change of a thread's comments only through
    // the `~comments` of the paragraph its range starts in ([`DocDocument::derived_comments`]).
    if let Some(v) = anchored_in(&session.doc, root) {
        under.premises.push(Premise::on(format!("p{v:08X}").as_str()).fields(["~comments"]).positions(Vec::<String>::new()));
    }
    // The comments up the chain from the named one to its thread's first: a reply goes to the
    // thread's first comment, a resolve to the flag on it.
    let mut chain = vec![id];
    while let Some(parent) =
        chain.last().and_then(|c| comments.get(c)).and_then(|c| c.parent).filter(|p| comments.contains_key(p) && !chain.contains(p))
    {
        chain.push(parent);
    }
    match command {
        "comment.reply" => chain.iter().for_each(|c| comment(*c, &["props"], under)),
        "review.reply" => comment(id, &[], under),
        "review.editComment" => comment(id, &["body", "props.blocks"], under),
        // The comments up to the first, whose flag (the thread's) it writes.
        "comment.resolve" | "review.resolveComment" if id == root || command == "comment.resolve" => {
            chain.iter().filter(|c| **c != root || id == root).for_each(|c| comment(*c, &["props"], under));
            thread(&["resolved"], under);
        }
        // A reply's own flag (Resolve by `id`; with the caret in a reply, its thread's).
        "review.resolveComment" => {
            comment(id, &["props"], under);
            if params.get("id").is_none_or(Value::is_null) {
                thread(&["resolved"], under);
            }
        }
        // The whole thread: its comments, each with its text and parent.
        "comment.delete" if id == root => {
            thread(&["comments"], under);
            for id in &members {
                if let Some(tool) = comments.get(id).and_then(review::comment_tool_id) {
                    under.premises.push(Premise::on(tool.as_str()).positions(Vec::<String>::new()));
                }
            }
        }
        // A reply, and the replies that answer it (they answer its parent from now on); Delete
        // reads its replies' parents, not its own.
        "comment.delete" | "review.deleteComment" => {
            if let Some(tool) = comments.get(&id).and_then(review::comment_tool_id) {
                under.premises.push(Premise::on(tool.as_str()).positions(Vec::<String>::new()));
            }
            for (c, _) in comments.iter().filter(|(_, c)| c.parent == Some(id)) {
                if command == "review.deleteComment" {
                    if let Some(tool) = comments.get(c).and_then(review::comment_tool_id) {
                        under.premises.push(Premise::on(tool.as_str()).positions(Vec::<String>::new()));
                    }
                } else {
                    comment(*c, &["props"], under);
                }
            }
            if id == root {
                thread(&["comments"], under);
            }
        }
        _ => {}
    }
}

/// The premise on each part's paragraph list (`blocks`) by its tool id (`part:<name>`,
/// [`Document::part_names`], C8-doc-6): a part has its own tool id, so a paragraph another writer
/// adds to or takes from it is its change, not the root's `parts`. Comment texts are no parts of
/// the object model (a comment's `body`).
fn part_premises(doc: &Document, only: Option<u32>, under: &mut Under) {
    for (id, name) in doc.part_names() {
        if only.is_some_and(|only| only != id) || doc.parts.get(&id).is_some_and(|part| part.kind == kition_doc_engine::doc::PartKind::Comment) {
            continue;
        }
        under.premises.push(Premise::on(format!("part:{name}").as_str()).fields(["blocks"]).positions(Vec::<String>::new()));
    }
    if only.is_none() {
        // Comment paragraphs live in Comment.body / props.blocks, not Block objects.
        for c in doc.comments.values() {
            if let Some(tool) = kition_doc_engine::cmd::review::comment_tool_id(c) {
                under.premises.push(Premise::on(tool.as_str()).fields(["body", "props"]).positions(Vec::<String>::new()));
            }
        }
    }
}

/// The tool ids of the paragraphs of story `story` (a part's).
fn story_paragraphs(doc: &Document, story: StoryRef) -> Vec<String> {
    doc.para_paths(story)
        .into_iter()
        .filter_map(|path| doc.para_at(&kition_doc_engine::doc::Pos::new(story, path, 0)).and_then(|p| p.props.para_id))
        .map(|v| format!("p{v:08X}"))
        .collect()
}

/// The header (or footer) `insert.header`, `insert.footer` and `insert.pageNumber` replace: the
/// default one of the section the body block `block` is in, as `page::with_sect` finds it.
fn section_part(doc: &Document, block: usize, footer: bool) -> Option<u32> {
    let section =
        doc.body.iter().skip(block).find_map(|b| if let Block::Para(p) = &**b { p.section.as_deref() } else { None }).unwrap_or(&doc.last_section);
    if footer { section.footers.default } else { section.headers.default }
}

/// The equation an `equation.*` command edits (`equation.rs` `equation_at_caret`): the one being
/// edited (`Session::math`), else the one at or just before the caret `focus`, or the only one a
/// selection `a..b` holds.
fn equation_at<'d>(
    doc: &'d Document,
    math: Option<&kition_doc_engine::MathEdit>,
    a: &kition_doc_engine::doc::Pos,
    focus: &kition_doc_engine::doc::Pos,
) -> Option<&'d kition_doc_engine::doc::InlineObject> {
    let equation = |pos: &kition_doc_engine::doc::Pos, off: usize| {
        doc.para_at(pos)?.object_at(off).filter(|o| matches!(o, kition_doc_engine::doc::InlineObject::Equation { .. }))
    };
    if let Some(m) = math {
        return equation(&m.at, m.at.off);
    }
    let len = kition_doc_engine::doc::para::OBJ.len_utf8();
    equation(focus, focus.off)
        .or_else(|| focus.off.checked_sub(len).and_then(|off| equation(focus, off)))
        .or_else(|| (a.story == focus.story && a.path == focus.path && focus.off == a.off + len).then(|| equation(a, a.off)).flatten())
}

/// The numbers of the drawing objects and pictures [`selected_drawings`] finds (C8-doc-6: a
/// picture has its number, `wp:docPr/@id`, as a shape has).
fn selected_objects(doc: &Document, a: &kition_doc_engine::doc::Pos, b: &kition_doc_engine::doc::Pos) -> Vec<u32> {
    selected_drawings(doc, a, b).into_iter().filter_map(|o| o.tool_uid()).filter(|uid| *uid > 0).collect()
}

/// What of the layout a command reads (C8-doc-6, from the reading's `facts=layout` rows): page
/// numbers of every paragraph (tables of contents, indexes, fields, cross-references), what lies
/// on a page it names by number (`page`, `placement.page`: a point of that page), or where the
/// drawing objects it names or has selected, or the caret, are laid out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LayoutRead {
    Every,
    Page(usize),
    Objects,
    FrameValidity,
    Caret,
}

fn layout_read(command: &str, params: &Value) -> Option<LayoutRead> {
    if command == "arrange.bounds" && !(params.get("x").is_some() && params.get("y").is_some()) {
        return None;
    }
    let page = params
        .get("page")
        .or_else(|| params.get("placement").and_then(|p| p.get("page")))
        .and_then(Value::as_u64)
        .and_then(|p| usize::try_from(p).ok());
    match command {
        "references.toc"
        | "references.updateToc"
        | "references.index"
        | "references.tableOfAuthorities"
        | "references.tableOfFigures"
        | "references.updateFigures"
        | "references.updateIndex"
        | "references.updateFields"
        | "insert.field"
        | "insert.crossReference" => Some(LayoutRead::Every),
        "table.draw" | "table.eraser" | "draw.erase" | "draw.stroke" | "drawing.ink" | "drawing.erase" | "arrange.bounds" | "drawing.insert"
            if page.is_some() =>
        {
            page.map(LayoutRead::Page)
        }
        "drawing.format" | "drawing.text" => Some(LayoutRead::FrameValidity),
        "drawing.transform" | "drawing.group" | "drawing.ungroup" | "drawing.connect" | "drawing.reconnect" | "drawing.chart" | "arrange.group"
        | "arrange.ungroup" | "arrange.nudge" | "arrange.bounds" => Some(LayoutRead::Objects),
        "drawing.insert" | "insert.chart" | "table.autofit" | "drawing.ink" | "drawing.erase" => Some(LayoutRead::Caret),
        _ => None,
    }
}

/// The paragraph ids whose layout [`LayoutRead`] names.
fn layout_paragraphs(
    doc: &Document,
    layout: &kition_doc_engine::layout::DocLayout,
    read: LayoutRead,
    params: &Value,
    command: &str,
    a: &kition_doc_engine::doc::Pos,
    b: &kition_doc_engine::doc::Pos,
) -> Vec<u32> {
    let id = |story: StoryRef, path: &kition_doc_engine::doc::Path| doc.para(story, path).and_then(|p| p.props.para_id);
    let mut out: Vec<u32> = match read {
        LayoutRead::Every => layout.index.keys().filter_map(|(story, path)| id(*story, path)).collect(),
        LayoutRead::Page(page) => layout
            .index
            .iter()
            .filter(|(_, pieces)| pieces.iter().any(|(p, _)| *p == page))
            .filter_map(|((story, path), _)| id(*story, path))
            .collect(),
        LayoutRead::Objects | LayoutRead::FrameValidity => {
            // The drawing objects the step names by number, else the selected ones.
            let mut uids: Vec<u32> = ["ids", "id", "from", "to"]
                .iter()
                .filter_map(|name| params.get(*name))
                .flat_map(|v| match v {
                    Value::Array(items) => items.clone(),
                    other => vec![other.clone()],
                })
                .filter_map(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
                .filter_map(|v| u32::try_from(v).ok())
                .collect();
            if uids.is_empty() {
                uids = selected_objects(doc, a, b);
            }
            doc.numbered()
                .into_iter()
                .filter(|(_, o)| o.tool_uid().is_some_and(|u| uids.contains(&u)))
                .filter_map(|(pos, _)| id(pos.story, &pos.path))
                .collect()
        }
        LayoutRead::Caret => id(b.story, &b.path).into_iter().collect(),
    };
    let moving = matches!(command, "drawing.connect" | "drawing.group" | "arrange.group")
        || (matches!(command, "drawing.transform" | "arrange.bounds") && ["dx", "dy", "x", "y"].iter().any(|key| params.get(*key).is_some()))
        || (matches!(command, "arrange.ungroup" | "drawing.ungroup")
            && doc.numbered().iter().any(|(pos, o)| id(pos.story, &pos.path).is_some_and(|id| out.contains(&id)) && !o.is_floating()));
    if read == LayoutRead::Objects && moving {
        // anchor_paragraph/nearest_line inspect the destination page, including paragraphs
        // whose geometry changes without moving the source object's frame.
        let mut pages: Vec<usize> = doc
            .numbered()
            .iter()
            .filter(|(pos, _)| id(pos.story, &pos.path).is_some_and(|id| out.contains(&id)))
            .filter_map(|(pos, _)| layout.object(pos, 0).map(|hit| hit.page))
            .collect();
        if let Some(page) = params.get("page").and_then(Value::as_u64).and_then(|p| usize::try_from(p).ok()) {
            pages.push(page);
        }
        out.extend(
            layout
                .index
                .iter()
                .filter(|(_, pieces)| pieces.iter().any(|(page, _)| pages.contains(page)))
                .filter_map(|((story, path), _)| id(*story, path)),
        );
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The paragraph ids of the paragraphs `a..b` and of the one before and the one after them in
/// their story (in story order, table cells included).
fn list_neighbourhood(doc: &Document, a: &kition_doc_engine::doc::Pos, b: &kition_doc_engine::doc::Pos) -> Vec<(kition_doc_engine::doc::Path, u32)> {
    let paths = doc.para_paths(a.story);
    let (Some(first), Some(last)) = (paths.iter().position(|p| *p == a.path), paths.iter().position(|p| *p == b.path)) else { return Vec::new() };
    let (first, last) = (first.min(last), first.max(last));
    paths
        .iter()
        .skip(first.saturating_sub(1))
        .take(last + 2 - first.saturating_sub(1))
        .filter_map(|path| doc.para(a.story, path).and_then(|p| p.props.para_id).map(|id| (path.clone(), id)))
        .collect()
}

fn hold_paragraph(doc: &Document, pos: &kition_doc_engine::doc::Pos, fields: &[&str], under: &mut Under) {
    if let Some(v) = doc.para_at(pos).and_then(|p| p.props.para_id) {
        under.premises.push(Premise::on(format!("p{v:08X}").as_str()).fields(fields.iter().copied()).positions(Vec::<String>::new()));
    }
}

/// Movement resolves its destination after inserting/resizing the drawing. Project only these
/// document-local commands so the premise names that resulting anchor, including reflow.
fn drawing_destinations(
    session: &kition_doc_engine::Session,
    command: &str,
    params: &Value,
    a: &kition_doc_engine::doc::Pos,
    b: &kition_doc_engine::doc::Pos,
) -> Vec<kition_doc_engine::doc::Pos> {
    let moves = match command {
        "drawing.insert" => params.get("frame").is_some_and(|frame| frame.get("x").is_some() || frame.get("y").is_some()),
        "drawing.transform" => ["dx", "dy", "x", "y"].iter().any(|key| params.get(*key).is_some()),
        "drawing.connect" => true,
        "draw.stroke" | "drawing.ink" => true,
        _ => false,
    };
    if !moves {
        return Vec::new();
    }
    let mut projected = kition_doc_engine::Session::new(session.doc.clone());
    projected.sel = kition_doc_engine::Selection { anchor: a.clone(), focus: b.clone() };
    projected.view = session.view.clone();
    projected.pending = session.pending.clone();
    projected.page_hint = session.page_hint;
    let Ok(result) = projected.run(command, params) else { return Vec::new() };
    if command == "draw.stroke" {
        return result
            .get("pos")
            .cloned()
            .and_then(|pos| serde_json::from_value(pos).ok())
            .and_then(|pos| projected.doc.para_at(&pos).and_then(|p| p.props.para_id))
            .and_then(|id| crate::context::paragraph(&session.doc, &format!("p{id:08X}")))
            .into_iter()
            .collect();
    }
    let values: Vec<&Value> = if matches!(command, "drawing.insert" | "drawing.connect") {
        result.get("id").into_iter().collect()
    } else if command == "drawing.ink" {
        result.get("ids").and_then(Value::as_array).map(|ids| ids.iter().collect()).unwrap_or_default()
    } else {
        params.get("ids").and_then(Value::as_array).map(|ids| ids.iter().collect()).unwrap_or_else(|| params.get("id").into_iter().collect())
    };
    let ids: Vec<u32> = values
        .into_iter()
        .filter_map(|id| id.as_u64().or_else(|| id.as_str().and_then(|id| id.parse().ok())))
        .filter_map(|id| u32::try_from(id).ok())
        .collect();
    projected
        .doc
        .numbered()
        .into_iter()
        .filter(|(_, object)| object.tool_uid().is_some_and(|uid| ids.contains(&uid)))
        .filter_map(|(pos, _)| projected.doc.para_at(&pos).and_then(|p| p.props.para_id))
        .filter_map(|id| crate::context::paragraph(&session.doc, &format!("p{id:08X}")))
        .collect()
}

fn table_point_reads(
    session: &kition_doc_engine::Session,
    layout: &kition_doc_engine::layout::DocLayout,
    command: &str,
    params: &Value,
    under: &mut Under,
) {
    use kition_doc_engine::cmd::table_draw;
    let Some(page) = params.get("page").and_then(Value::as_u64).and_then(|p| usize::try_from(p).ok()) else { return };
    let number = |key| params.get(key).and_then(Value::as_f64).map(|v| v as f32).filter(|v| v.is_finite());
    let table = if command == "table.eraser" {
        match (number("x"), number("y")) {
            (Some(x), Some(y)) => table_draw::erase_target(layout, page, x, y).map(|e| {
                let (story, path) = e.table();
                (story, path.clone())
            }),
            _ => None,
        }
    } else if command == "table.draw" {
        let (Some(x0), Some(y0), Some(x1), Some(y1)) = (number("x0"), number("y0"), number("x1"), number("y1")) else { return };
        match table_draw::stroke(layout, page, x0, y0, x1, y1) {
            Some(table_draw::Stroke::Rect(_)) => {
                if let Some(pos) = layout.hit(page, x0, y0, kition_doc_engine::cmd::story_param(session, params)) {
                    hold_paragraph(&session.doc, &pos, &["text", "para"], under);
                    crate::place::hit_premises(&session.doc, &pos, under);
                }
                None
            }
            Some(stroke) => table_draw::stroke_table(layout, page, stroke),
            None => None,
        }
    } else {
        None
    };
    if let Some(t) = table.and_then(|(story, path)| session.doc.table(story, &path)) {
        table_premises(t, under);
    }
}

/// Paragraph values copied by typing, splitting, joining and fragment insertion. These are
/// separate from the character range a place holds: a split copies paragraph properties, and
/// a join consumes text outside the call's original range.
fn pastes_lines(command: &str, params: &Value) -> bool {
    let plain = matches!(command, "edit.pasteText" | "edit.pasteMerge")
        || (command == "edit.pasteSpecial" && matches!(params.get("as").and_then(Value::as_str), Some("text" | "plain" | "unformatted")));
    // A preceding Copy in this call can supply multiline text after declarations are collected.
    plain && params.get("text").and_then(Value::as_str).is_none_or(|t| t.contains(['\n', '\r', '\u{2029}']))
}

fn paragraph_reads(
    session: &kition_doc_engine::Session,
    command: &str,
    params: &Value,
    a: &kition_doc_engine::doc::Pos,
    b: &kition_doc_engine::doc::Pos,
    under: &mut Under,
) {
    use kition_doc_engine::doc::Pos;
    let doc = &session.doc;
    let collapsed = a == b;
    if matches!(command, "references.addText" | "format.clear") {
        for path in doc.paths_between(a, b) {
            hold_paragraph(doc, &Pos::new(a.story, path, 0), &["para"], under);
        }
    }
    if command == "para.sort" {
        for path in doc.paths_between(a, b) {
            hold_paragraph(doc, &Pos::new(a.story, path, 0), &["text"], under);
        }
    }
    let fragment = matches!(
        command,
        "edit.paste"
            | "edit.pasteAllClipboard"
            | "edit.pasteClipboardItem"
            | "insert.object"
            | "insert.signatureLine"
            | "insert.spreadsheet"
            | "insert.textFromFile"
    ) || (command == "edit.pasteSpecial" && matches!(params.get("as").and_then(Value::as_str), Some("formatted" | "html")))
        || (matches!(command, "insert.autoText" | "insert.quickParts") && params.get("insert").is_some());
    let copies = fragment
        || matches!(
            command,
            "text.newParagraph" | "insert.blankPage" | "insert.dropCap" | "insert.horizontalLine" | "styles.create" | "styles.updateToMatch"
        )
        || pastes_lines(command, params)
        || (command == "edit.pasteFormat" && (collapsed || a.path != b.path || a.off == 0))
        || (command == "styles.inspectorClear" && params.get("level").and_then(Value::as_str).is_some_and(|s| s.starts_with("paragraph")))
        || (command == "insert.pageNumber" && params.get("position").and_then(Value::as_str) == Some("current"))
        || (command == "insert.removeLink" && collapsed && doc.para_at(a).is_some_and(|p| p.is_empty()))
        || (command == "text.tab" && b.path.cell().is_none() && ((collapsed && b.off == 0) || a.path != b.path))
        || (command == "text.backTab" && b.path.cell().is_none());
    if copies {
        for path in doc.paths_between(a, b) {
            hold_paragraph(doc, &Pos::new(a.story, path, 0), &["para"], under);
        }
    }
    // Restart scans all following paragraphs until its handler's stopping condition, including
    // the paragraph that stops it (para.rs:477-491; tools.rs:310-319).
    if matches!(command, "para.restartNumbering" | "para.setNumberingValue")
        && let Some(num) = doc.para_at(b).and_then(|p| p.props.numbering).filter(|n| n.num != 0)
    {
        for path in doc.para_paths(b.story).into_iter().filter(|p| *p >= b.path) {
            let at = Pos::new(b.story, path, 0);
            let Some(p) = doc.para_at(&at) else { continue };
            let fields: &[&str] = if command == "para.restartNumbering" && p.props.numbering.is_none() { &["para", "text"] } else { &["para"] };
            hold_paragraph(doc, &at, fields, under);
            crate::place::hit_premises(doc, &at, under);
            if command == "para.setNumberingValue" {
                if !p.props.numbering.is_some_and(|n| n.num == num.num) {
                    break;
                }
            } else if p.props.numbering.is_none() && !p.is_empty() {
                break;
            }
        }
    }
    if !collapsed {
        return;
    }
    let Some(p) = doc.para_at(b) else { return };
    let backward = matches!(command, "text.backspace" | "text.deleteWordBack") && b.off == 0;
    let forward = matches!(command, "text.delete" | "text.deleteWordForward") && b.off >= p.len();
    if backward {
        hold_paragraph(doc, b, &["para"], under);
        if p.props.numbering.is_some_and(|n| n.num != 0)
            || p.props.style.as_deref() == Some("ListParagraph")
            || p.props.indent_first.is_some_and(|x| x > 0.0)
        {
            return;
        }
    }
    if backward || forward {
        let neighbor = if backward { b.path.last().checked_sub(1) } else { b.path.last().checked_add(1) };
        if let Some(index) = neighbor {
            let at = Pos::new(b.story, b.path.with_last(index), 0);
            match doc.block(at.story, &at.path) {
                Some(Block::Para(_)) => {
                    hold_paragraph(doc, b, &["para"], under);
                    // The right paragraph contributes text and its stable id, not formatting.
                    hold_paragraph(doc, &at, if backward { &["text", "para"] } else { &["text"] }, under);
                }
                Some(Block::Table(t)) if forward => {
                    if let Some(id) = t.rows.first().and_then(|r| r.props.para_id) {
                        under.premises.push(Premise::on(format!("t{id:08X}").as_str()).fields(["kind"]).positions(Vec::<String>::new()));
                    }
                }
                _ => {}
            }
        }
    }
}

impl DocDocument {
    /// The premises `command` adds by its family (see the module docs). Answers whether it added
    /// any.
    pub(crate) fn family_premises(&self, command: &str, params: &Value, under: &mut Under) -> bool {
        // The layout a command reads (C8-doc-6), laid out before the session is borrowed.
        let mut reads_layout = layout_read(command, params);
        let layout = reads_layout.map(|_| self.session.borrow_mut().layout());
        let ink = matches!(command, "drawing.ink" | "drawing.erase" | "draw.stroke" | "draw.erase");
        let ink_hits = if command == "drawing.erase" {
            let mut session = self.session.borrow_mut();
            kition_doc_engine::cmd::ink::erase_hits(&mut session, params)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|uid| session.doc.find_shape(uid))
                .collect::<Vec<_>>()
        } else if command == "draw.erase" {
            let mut session = self.session.borrow_mut();
            params
                .get("page")
                .and_then(Value::as_u64)
                .and_then(|page| usize::try_from(page).ok())
                .zip(params.get("x").and_then(Value::as_f64))
                .zip(params.get("y").and_then(Value::as_f64))
                .and_then(|((page, x), y)| kition_doc_engine::cmd::draw::ink_at(&mut session, page, x as f32, y as f32))
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        let session = self.session.borrow();
        if !session.registry.get(command).is_some_and(crate::changes_document) {
            return false;
        }
        let doc = &session.doc;
        let consumes_ink = matches!(command, "drawing.ink" | "draw.stroke") || !session.view.hide_ink;
        if ink
            && reads_layout == Some(LayoutRead::Caret)
            && consumes_ink
            && let Some(layout) = layout.as_deref()
        {
            reads_layout =
                Some(LayoutRead::Page(layout.caret_on(&session.sel.focus, session.page_hint).map_or(session.page_hint, |caret| caret.page)));
        }
        let family = command.split_once('.').map_or(command, |(family, _)| family);
        let before = under.premises.len();
        if family == "drawing" {
            let ids: Vec<u32> = ["id", "ids", "from", "to"]
                .iter()
                .filter_map(|name| params.get(*name))
                .flat_map(|v| match v {
                    Value::Array(ids) => ids.clone(),
                    id => vec![id.clone()],
                })
                .filter_map(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
                .filter_map(|id| u32::try_from(id).ok())
                .collect();
            for (pos, object) in doc.numbered().into_iter().filter(|(_, o)| o.tool_uid().is_some_and(|id| ids.contains(&id))) {
                let moves = command == "drawing.transform" && ["dx", "dy", "x", "y", "w", "h"].iter().any(|key| params.get(*key).is_some());
                let fields: &[&str] = if moves
                    || matches!(
                        command,
                        "drawing.delete" | "drawing.format" | "drawing.text" | "drawing.group" | "drawing.ungroup" | "drawing.connect"
                    ) {
                    &["text", "~objects"]
                } else {
                    &["~objects"]
                };
                hold_paragraph(doc, &pos, fields, under);
                if moves || matches!(command, "drawing.ungroup" | "drawing.connect") {
                    crate::place::hit_premises(doc, &pos, under);
                }
                if let Some(uid) = object.tool_uid() {
                    under.premises.push(Premise::on(serde_json::json!(uid)).positions(Vec::<String>::new()));
                }
            }
        }
        if matches!(
            command,
            "drawing.insert"
                | "drawing.delete"
                | "drawing.text"
                | "drawing.chart"
                | "drawing.group"
                | "drawing.ungroup"
                | "drawing.connect"
                | "insert.chart"
                | "insert.artText"
                | "drawing.ink"
        ) {
            under.premises.push(Premise::on(ROOT).fields(["~drawingIds"]).positions(Vec::<String>::new()));
        }
        if matches!(command, "drawing.arrange" | "drawing.connect" | "drawing.reconnect" | "drawing.transform" | "drawing.format")
            || (command == "drawing.insert"
                && params
                    .get("frame")
                    .is_some_and(|f| params.get("kind").and_then(Value::as_str) != Some("chart") || f.get("x").is_some() || f.get("y").is_some()))
        {
            under.premises.push(Premise::on(ROOT).fields(["~drawingFrames"]).positions(Vec::<String>::new()));
        }
        under.premises.push(Premise::on(ROOT).fields(["settings.trackChanges"]).positions(Vec::<String>::new()));
        // Changes tracked: what the command types, deletes or formats becomes a revision, numbered
        // in the root's `revisions` and merged with the author's revision next to it (C8-doc-4).
        if doc.settings.track_changes {
            under.premises.push(Premise::on(ROOT).fields(["revisions"]).positions(Vec::<String>::new()));
        }
        // The paragraphs the step acts on: the selection a step's `on` set, or the range an
        // earlier step of the run selected, or the selection the call started with.
        let (a, b) = match (self.context_on.is_empty(), self.pass.last_range()) {
            (true, Some(range)) => range,
            _ => session.sel.ordered(),
        };
        let focus = b.clone();
        let selected: Vec<&kition_doc_engine::doc::Paragraph> =
            doc.paths_between(&a, &b).into_iter().filter_map(|path| doc.para_at(&kition_doc_engine::doc::Pos::new(a.story, path, 0))).collect();
        paragraph_reads(&session, command, params, &a, &b, under);
        if command == "para.text" && params.get("text").and_then(Value::as_str).is_some_and(|text| text.contains(['\r', '\u{2029}'])) {
            // A split copies the named paragraph's properties and uses its
            // style's next style. The call's old caret may be elsewhere.
            if let Some(index) = params.get("index").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()) {
                let at = kition_doc_engine::doc::Pos::body(index, 0);
                hold_paragraph(doc, &at, &["para"], under);
                if let Some(style) = doc.para_at(&at).and_then(|p| p.props.style.as_deref()) {
                    under.premises.push(crate::place::style_premise(style));
                }
            }
        }
        if matches!(command, "edit.replace" | "edit.replaceAll")
            && params.get("with").and_then(Value::as_str).unwrap_or(&session.find.replace).contains(['\r', '\u{2029}'])
        {
            // type_text splits paragraphs on this path. At a paragraph end,
            // split_para resolves the current style's next style.
            under.premises.push(Premise::on(ROOT).fields(["styles"]).positions(Vec::<String>::new()));
        }
        if a != b && matches!(command, "edit.replace" | "mailings.addressBlock" | "mailings.greetingLine" | "mailings.insertField" | "mailings.rules")
        {
            // Replacing selected content resolves its revision marks even on
            // paths where the command is not creating a new tracked revision.
            under.premises.push(Premise::on(ROOT).fields(["revisions"]).positions(Vec::<String>::new()));
        }
        if command == "table.fromText" && a.story == b.story && a.path.parent() == b.path.parent() {
            for index in a.path.last()..=b.path.last() {
                let path = a.path.with_last(index);
                let id = match doc.block(a.story, &path) {
                    Some(Block::Para(p)) => p.props.para_id.map(|id| format!("p{id:08X}")),
                    Some(Block::Table(t)) => t.rows.first().and_then(|row| row.props.para_id).map(|id| format!("t{id:08X}")),
                    None => None,
                };
                if let Some(id) = id {
                    // The whole block is removed, including properties and nested contents.
                    under.premises.push(Premise::on(id.as_str()).positions(Vec::<String>::new()));
                }
                if let Some(Block::Para(_)) = doc.block(a.story, &path) {
                    hold_paragraph(doc, &kition_doc_engine::doc::Pos::new(a.story, path, 0), &["~objects"], under);
                }
            }
        }
        if command == "insert.removeHeader" {
            for path in doc.para_paths(StoryRef::Body) {
                hold_paragraph(doc, &kition_doc_engine::doc::Pos::new(StoryRef::Body, path, 0), &["para.section"], under);
            }
        }
        if command == "drawing.ink" {
            under.premises.push(Premise::on(ROOT).fields(["~inkIds"]).positions(Vec::<String>::new()));
        }
        for at in ink_hits {
            hold_paragraph(doc, &at, &["text", "~objects"], under);
            crate::place::hit_premises(doc, &at, under);
        }
        for at in drawing_destinations(&session, command, params, &a, &b) {
            hold_paragraph(doc, &at, &["text"], under);
            crate::place::hit_premises(doc, &at, under);
        }
        if matches!(command, "review.compare" | "review.combine") && (params.get("text").is_some() || params.get("path").is_some()) {
            // Compare assigns the entire body, including the tables it discards.
            under.premises.push(Premise::on(ROOT).fields(["body"]).positions(Vec::<String>::new()));
            for path in doc.para_paths(StoryRef::Body) {
                hold_paragraph(doc, &kition_doc_engine::doc::Pos::new(StoryRef::Body, path, 0), &["~objects"], under);
            }
            under.within.push(kition_kit::Within { parent: serde_json::json!(ROOT), field: "body".into(), from: 0, to: doc.body.len(), time: None });
            for (i, block) in doc.body.iter().enumerate() {
                match &**block {
                    Block::Para(_) => hold_paragraph(doc, &kition_doc_engine::doc::Pos::body(i, 0), &["text", "para", "~objects"], under),
                    Block::Table(t) => {
                        if let Some(id) = t.rows.first().and_then(|r| r.props.para_id) {
                            under.premises.push(Premise::on(format!("t{id:08X}").as_str()).fields(["kind"]).positions(Vec::<String>::new()));
                        }
                    }
                }
            }
        }
        if command == "text.insert"
            && params.get("raw") != Some(&Value::Bool(true))
            && params.get("text").and_then(Value::as_str) == Some(" ")
            && let Some(p) = doc.para_at(&a).filter(|p| a.path == b.path && b.off == p.len())
            && let Some(prefix) = p.text.get(..a.off)
            && matches!(prefix, "*" | ">" | "-" | "1." | "1)" | "a." | "a)" | "A." | "i." | "I.")
        {
            hold_paragraph(doc, &a, &["para"], under);
            if !p.props.numbering.is_some_and(|n| n.num != 0) {
                under.premises.push(Premise::on(ROOT).fields(["lists", "numbering"]).positions(Vec::<String>::new()));
            }
        }
        if command == "text.newParagraph" {
            let projected = self.pass.last_insert().and_then(|(from, to, text)| {
                let p = doc.para_at(&from)?;
                if from.story != a.story || from.path != a.path {
                    return None;
                }
                Some((
                    kition_doc_engine::doc::Pos { off: from.off.checked_add(text.len())?, ..from.clone() },
                    format!("{}{}{}", p.text.get(..from.off)?, text, p.text.get(to.off..)?),
                ))
            });
            let source = projected.or_else(|| doc.para_at(&a).map(|p| (a.clone(), p.text.clone())));
            if let Some((at, text)) = source
                && kition_doc_engine::cmd::text::auto_border(&session, &at, &text, self.pass.last_insert().is_some() || session.last_typed_one_char())
                    .is_some()
                && let Some(previous) =
                    at.path.last().checked_sub(1).map(|i| kition_doc_engine::doc::Pos { path: at.path.with_last(i), off: 0, ..at })
            {
                hold_paragraph(doc, &previous, &["para"], under);
            }
        }
        if matches!(command, "review.accept" | "review.reject") && kition_doc_engine::cmd::review::resolve_navigates(&session, &a, &b) {
            under.premises.push(Premise::on(ROOT).fields(["body", "lastSection"]).positions(Vec::<String>::new()));
        }
        if command == "equation.enter"
            && let Some(at) = kition_doc_engine::cmd::equation::enter_split_position(&session, self.pass.equation_edit().as_ref())
        {
            hold_paragraph(doc, &at, &["para"], under);
            if let Some(p) = doc.para_at(&at).filter(|p| at.off >= p.len())
                && let Some(style) = &p.props.style
            {
                under.premises.push(crate::place::style_premise(style));
            }
        }
        if command == "insert.bookmark" {
            under.premises.push(Premise::on(ROOT).fields(["~bookmarks"]).positions(Vec::<String>::new()));
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default().trim();
            for (_, pos) in doc.bookmarks().into_iter().filter(|(n, _)| n == name) {
                hold_paragraph(doc, &pos, &["text"], under);
            }
        }
        if command == "styles.delete"
            && let Some(style) = params
                .get("style")
                .and_then(Value::as_str)
                .and_then(|name| doc.styles.styles.iter().find(|s| s.id == name || s.name.eq_ignore_ascii_case(name)))
                .filter(|style| !style.builtin)
        {
            let table = style.kind == kition_doc_engine::doc::styles::StyleKind::Table;
            let fields: &[&str] = if table { &[] } else { &["text", "para"] };
            for id in every_paragraph(doc) {
                under.premises.push(Premise::on(id.as_str()).fields(fields.iter().copied()).positions(Vec::<String>::new()));
            }
            for c in doc.comments.values() {
                if let Some(tool) = kition_doc_engine::cmd::review::comment_tool_id(c) {
                    let fields: &[&str] = if table { &[] } else { &["body", "props"] };
                    under.premises.push(Premise::on(tool.as_str()).fields(fields.iter().copied()).positions(Vec::<String>::new()));
                }
            }
            if table {
                fn tables(blocks: &kition_doc_engine::doc::Blocks, depth: usize, under: &mut Under) {
                    if depth > 32 {
                        return;
                    }
                    for block in blocks {
                        if let Block::Table(t) = &**block {
                            if let Some(id) = t.rows.first().and_then(|r| r.props.para_id) {
                                under
                                    .premises
                                    .push(Premise::on(format!("t{id:08X}").as_str()).fields(["table.props"]).positions(Vec::<String>::new()));
                            }
                            for row in &t.rows {
                                for cell in &row.cells {
                                    tables(&cell.blocks, depth + 1, under);
                                }
                            }
                        }
                    }
                }
                tables(&doc.body, 0, under);
                for part in doc.parts.values() {
                    tables(&part.blocks, 0, under);
                }
            }
        }
        // The selected paragraphs' styles with the styles they are based on (the formatting a
        // toggle or a relative change resolves), each once.
        let paragraph_styles = || {
            let mut styles: Vec<String> = Vec::new();
            // The paragraphs' styles, and the character styles their runs take.
            let mut starts: Vec<String> = Vec::new();
            for p in &selected {
                starts.push(p.props.style.clone().unwrap_or_else(|| "Normal".to_owned()));
                starts.extend(p.runs.iter().filter_map(|run| run.props.style.clone()));
            }
            for start in starts {
                let mut next = Some(start);
                while let Some(id) = next.take() {
                    if styles.contains(&id) || styles.len() > 64 {
                        break;
                    }
                    next = doc.styles.styles.iter().find(|s| s.id == id).and_then(|s| s.based_on.clone());
                    styles.push(id);
                }
            }
            styles
        };
        // The styles are held whole only where nothing narrower holds the styles the command
        // resolves (a style by name, a paragraph's style chain: `place::style_premise`).
        let fields = if direct_character_assignment(command, params)
            || disabling_list(command, params)
            || plain_paragraph_replacement(command, params)
            || matches!(command, "para.sort" | "styles.inspectorClear")
        {
            &[]
        } else if new_unlinked_list(command, params) {
            &["lists", "numbering"]
        } else {
            root_reads(command)
        };
        let others: Vec<&str> = fields
            .iter()
            .copied()
            .filter(|f| *f != "styles" && !(command == "file.properties" && *f == "customProps" && params.get("custom").is_none()))
            .collect();
        if !others.is_empty() {
            under.premises.push(Premise::on(ROOT).fields(others).positions(Vec::<String>::new()));
        }
        if command == "file.inspect" {
            comment_premises(&session, command, params, under);
            under.premises.push(Premise::on(ROOT).fields(["~commentCount"]).positions(Vec::<String>::new()));
            // Every inspector path reports hidden text and tracked runs throughout the body.
            // Accept Revisions also resolves paragraph/table/row changes in every story.
            under.premises.push(Premise::on(ROOT).fields(["body"]).positions(Vec::<String>::new()));
            // Remove Revisions accepts every story's changes: every part's paragraphs.
            if params.get("remove").and_then(Value::as_array).is_some_and(|r| r.iter().any(|v| v == "revisions")) {
                part_premises(doc, None, under);
            }
        }
        // The mail merge list a merge command reads (C8-doc-6): the kit's external input.
        if crate::acts::reads_merge_source(command, params) {
            under.inputs.push(crate::acts::MERGE_SOURCE.to_owned());
        }
        if let (Some(read), Some(layout)) = (reads_layout, layout.as_deref()) {
            table_point_reads(&session, layout, command, params, under);
            let (a, b) = match (self.context_on.is_empty(), self.pass.last_range()) {
                (true, Some(range)) => range,
                _ => session.sel.ordered(),
            };
            for v in layout_paragraphs(doc, layout, read, params, command, &a, &b) {
                let field = if read == LayoutRead::FrameValidity { "~frameValidity" } else { "~layout" };
                under.premises.push(Premise::on(format!("p{v:08X}").as_str()).fields([field]).positions(Vec::<String>::new()));
                if ink && consumes_ink {
                    let field = if matches!(command, "draw.stroke" | "drawing.ink") { "~inkCount" } else { "~inkGeometry" };
                    under.premises.push(Premise::on(format!("p{v:08X}").as_str()).fields([field]).positions(Vec::<String>::new()));
                }
            }
            // A floating ink anchor may have its text on a different page from the stroke.
            if ink
                && consumes_ink
                && let LayoutRead::Page(page) = read
                && let Some(page) = layout.pages.get(page)
            {
                for item in &page.items {
                    if let kition_doc_engine::layout::Placed::Object { story, path, off, .. } = item {
                        let at = kition_doc_engine::doc::Pos::new(*story, path.clone(), *off);
                        if doc.para_at(&at).and_then(|p| p.object_at(*off)).is_some_and(|object| object.ink().is_some()) {
                            let field = if matches!(command, "draw.stroke" | "drawing.ink") { "~inkCount" } else { "~inkGeometry" };
                            hold_paragraph(doc, &at, &[field], under);
                        }
                    }
                }
            }
            if matches!(command, "arrange.bounds" | "arrange.group" | "arrange.ungroup") {
                let picks = kition_doc_engine::cmd::objects::picked_objects(&session);
                if command == "arrange.group" && picks.len() >= 2 {
                    for pos in &picks {
                        hold_paragraph(doc, pos, &["text", "~objects", "~layout"], under);
                        crate::place::hit_premises(doc, pos, under);
                    }
                }
                let destination = match command {
                    "arrange.bounds" => params.get("y").and_then(Value::as_f64).and_then(|y| {
                        let pos = picks.first()?;
                        if pos.story != StoryRef::Body || pos.path.depth() > 0 {
                            return None;
                        }
                        let hit = layout.object(pos, session.page_hint)?;
                        let page = params.get("page").and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).unwrap_or(hit.page);
                        let pg = layout.pages.get(page)?;
                        Some((page, (y as f32).max(12.0 - hit.rect.h).min(pg.h.min(4000.0) - 12.0)))
                    }),
                    "arrange.group" if picks.len() >= 2 => {
                        let hits: Option<Vec<_>> = picks.iter().map(|pos| layout.object(pos, session.page_hint)).collect();
                        hits.and_then(|hits| {
                            let first = hits.first()?;
                            let floating = picks.iter().all(|pos| doc.para_at(pos).and_then(|p| p.object_at(pos.off)).is_some_and(|o| o.is_floating()));
                            (floating && hits.iter().all(|hit| hit.page == first.page))
                                .then(|| (first.page, hits.iter().map(|hit| hit.rect.y).fold(f32::MAX, f32::min)))
                        })
                    }
                    "arrange.ungroup" => picks.first().and_then(|pos| {
                        let object = doc.para_at(pos)?.object_at(pos.off)?;
                        if !matches!(object, kition_doc_engine::doc::InlineObject::Group { float, .. } if float.wrap == kition_doc_engine::doc::para::Wrap::Inline) {
                            return None;
                        }
                        layout.object(pos, session.page_hint).map(|hit| (hit.page, hit.rect.y))
                    }),
                    _ => None,
                };
                if let Some((page, y)) = destination {
                    let at = kition_doc_engine::cmd::objects::anchor_paragraph(layout, page, y)
                        .map(|path| kition_doc_engine::doc::Pos::new(StoryRef::Body, path, 0))
                        .or_else(|| kition_doc_engine::cmd::objects::nearest_line(layout, page, y));
                    if let Some(at) = at {
                        hold_paragraph(doc, &at, &["text"], under);
                        crate::place::hit_premises(doc, &at, under);
                    }
                }
            }
        }
        // What a command reads of the root only on some of its paths (C8-doc-6, from the reading).
        let mut more: Vec<&str> = Vec::new();
        let text = params.get("text").and_then(Value::as_str);
        // A paste that replaces the selection with given text or a formatted fragment deletes
        // through the revisions (a tracked insertion of one's own is removed, not marked).
        let html = params.get("html").is_some_and(|h| !h.is_null());
        if matches!(command, "edit.paste" | "edit.pasteAllClipboard" | "edit.pasteClipboardItem" | "insert.textFromFile")
            || (command == "edit.pasteSpecial" && !matches!(params.get("as").and_then(Value::as_str), None | Some("text" | "plain" | "unformatted")))
        {
            under.premises.push(Premise::on(ROOT).fields(["~paraIds"]).positions(Vec::<String>::new()));
        }
        match command {
            "edit.pasteSpecial" if html => more.extend(["lists", "media", "numbering", "revisions"]),
            "edit.pasteSpecial" if params.get("as").and_then(Value::as_str) == Some("formatted") => more.push("revisions"),
            "edit.paste" | "edit.pasteText" if text.is_some() => more.push("revisions"),
            "drawing.insert" if params.get("kind").and_then(Value::as_str) == Some("textBox") => more.push("parts"),
            "file.inspect" => {
                let remove: Vec<&str> = params.get("remove").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
                // The report counts both even when a removal was requested. Unanchored
                // comments have no paragraph fingerprint that could hold their membership.
                more.push("parts");
                if remove.contains(&"revisions") {
                    more.extend(["lastSection", "revisions"]);
                }
                if remove.iter().any(|r| matches!(*r, "properties" | "hidden" | "headers")) {
                    more.push("lastSection");
                }
            }
            _ => {}
        }
        if !more.is_empty() {
            under.premises.push(Premise::on(ROOT).fields(more).positions(Vec::<String>::new()));
        }
        // The styles a new paragraph, a tab or a pasted line resolves (the paragraph's style, its
        // `next`, the styles it is based on): those of the selected paragraphs.
        let resolves_styles = pastes_lines(command, params) || (command == "edit.pasteSpecial" && html);
        // split_para reads only the current style's `next`, at the end of a paragraph. It
        // assigns that id without resolving the destination style (cmd/mod.rs:239-267).
        if matches!(command, "text.newParagraph" | "equation.enter")
            && let Some(p) = doc.para_at(&a)
            && b.off >= doc.para_at(&b).map_or(0, |p| p.len())
            && !p.props.numbering.is_some_and(|n| n.num != 0 && p.is_empty())
            && let Some(style) = &p.props.style
        {
            under.premises.push(crate::place::style_premise(style));
        }
        if matches!(command, "text.tab" | "text.backTab") && b.path.cell().is_none() {
            let resolves_indent = if command == "text.tab" { a.path != b.path } else { true };
            if resolves_indent {
                for p in &selected {
                    if !p.props.numbering.is_some_and(|n| n.num != 0) {
                        let mut style = p.props.style.as_deref().unwrap_or("Normal");
                        let mut seen = Vec::new();
                        while !seen.contains(&style) && seen.len() < 64 {
                            seen.push(style);
                            under.premises.push(crate::place::style_premise(style));
                            let Some(parent) = doc.styles.get(style).and_then(|s| s.based_on.as_deref()) else { break };
                            style = parent;
                        }
                        under.premises.push(Premise::on(ROOT).fields(["styles.defaultPara"]).positions(Vec::<String>::new()));
                    }
                }
            }
        }
        if resolves_styles && !matches!(family, "format" | "para") {
            for style in paragraph_styles() {
                under.premises.push(crate::place::style_premise(&style));
                if let Some(next) = doc.styles.styles.iter().find(|s| s.id == style).and_then(|s| s.next.clone()) {
                    under.premises.push(crate::place::style_premise(&next));
                }
            }
        }
        let core = core_reads(command, params);
        if !core.is_empty() {
            under.premises.push(Premise::on(ROOT).fields(core).positions(Vec::<String>::new()));
        }
        // An equation command edits the equation at the caret or the one being edited, and
        // reads it (a toggle, a conversion, a structure), by its number (C8-doc-6).
        if family == "equation"
            && !(command == "equation.convert" && params.get("all").and_then(Value::as_bool) == Some(true))
            && let Some(uid) = equation_at(doc, session.math.as_ref(), &a, &focus).and_then(|o| o.tool_uid()).filter(|uid| *uid > 0)
        {
            under.premises.push(Premise::on(serde_json::json!(uid)).fields(["value"]).positions(Vec::<String>::new()));
        }
        // Replace in a header, footer or note searches that story only (`edit.rs` `search(s,
        // story)`, the focus's): its paragraphs and the part's paragraph list (C8-doc-6).
        let part_story = match focus.story {
            StoryRef::Part(part) if matches!(command, "edit.replace" | "edit.replaceAll") => Some(part),
            _ => None,
        };
        if let Some(part) = part_story {
            for id in story_paragraphs(doc, StoryRef::Part(part)) {
                under.premises.push(Premise::on(id.as_str()).fields(["text", "para"]).positions(Vec::<String>::new()));
            }
            part_premises(doc, Some(part), under);
        } else if reads_document(command) {
            part_premises(doc, None, under);
            let ids = every_paragraph(doc);
            if ids.len() <= MAX_HELD {
                for id in ids {
                    under.premises.push(Premise::on(id.as_str()).fields(["text", "para"]).positions(Vec::<String>::new()));
                }
            }
            if matches!(command, "mailings.envelopes" | "mailings.labels" | "mailings.finish") {
                // Replacing the document or building a full merged document keeps
                // all discarded/copied document fields protected.
                under.premises.push(Premise::on(ROOT).fields(ROOT_CONTENT).positions(Vec::<String>::new()));
            } else {
                // These commands visit stories. Keep their membership and complete
                // content (including deleted nested paragraphs), without unrelated
                // metadata, media, sources or definitions absent from root_reads.
                under.premises.push(Premise::on(ROOT).fields(["body", "parts"]).positions(Vec::<String>::new()));
            }
            // Accepting or rejecting every change reads every table's, row's and cell's.
            if matches!(command, "review.acceptAll" | "review.rejectAll") {
                under.premises.push(Premise::on(ROOT).fields(["revisions"]).positions(Vec::<String>::new()));
                for block in &doc.body {
                    if let Block::Table(t) = &**block {
                        table_premises(t, under);
                    }
                }
            }
        } else if lists(command)
            && !disabling_list(command, params)
            && !(new_unlinked_list(command, params) && params.get("apply").and_then(Value::as_bool) == Some(false))
        {
            // The list definitions; the body's order is the step's window (not every paragraph:
            // C8-doc-6, a list command does not rest on another paragraph's text).
            under.premises.push(Premise::on(ROOT).fields(["numbering", "lists"]).positions(Vec::<String>::new()));
            // The selected paragraphs' properties (their numbering, style and indents) and the
            // paragraphs before and after them in their story, where a list continues or
            // restarts (C8-doc-6, from the reading).
            for (path, v) in list_neighbourhood(doc, &a, &b) {
                under.premises.push(Premise::on(format!("p{v:08X}").as_str()).fields(["para"]).positions(Vec::<String>::new()));
                crate::place::hit_premises(doc, &kition_doc_engine::doc::Pos::new(a.story, path, 0), under);
            }
        } else {
            match family {
                "format" if direct_character_assignment(command, params) => {}
                "format" | "para" => {
                    if family != "para"
                        || !(direct_paragraph_properties(command) || disabling_list(command, params) || plain_paragraph_replacement(command, params))
                    {
                        for style in paragraph_styles() {
                            under.premises.push(crate::place::style_premise(&style));
                        }
                        under.premises.push(Premise::on(ROOT).fields(["styles.defaultChr", "styles.defaultPara"]).positions(Vec::<String>::new()));
                    }
                    // The selected paragraphs' own: a paragraph command reads (and replaces) their
                    // properties; a character command's toggle reads their style.
                    let field = if family == "para" { "para" } else { "para.props.style" };
                    for p in &selected {
                        if let Some(v) = p.props.para_id {
                            under.premises.push(Premise::on(format!("p{v:08X}").as_str()).fields([field]).positions(Vec::<String>::new()));
                        }
                    }
                }
                "table" if table_definition_only(command, params) => {}
                "table" => {
                    // Structural operations consume the table whole. Table-wide
                    // property writes consume only their property fields.
                    if let Some((table, _, _)) = focus.path.cell()
                        && let Some(Block::Table(t)) = doc.block(focus.story, &table)
                    {
                        if let Some(fields) = table_property_fields(command, params) {
                            if let Some(id) = t.rows.first().and_then(|r| r.props.para_id) {
                                under.premises.push(Premise::on(format!("t{id:08X}").as_str()).fields(fields).positions(Vec::<String>::new()));
                            }
                        } else {
                            table_premises(t, under);
                        }
                    }
                    if matches!(command, "table.newStyle" | "table.modifyStyle" | "table.style") && params.get("style").is_none() {
                        under.premises.push(Premise::on(ROOT).fields(["styles"]).positions(Vec::<String>::new()));
                    }
                }
                "layout" if !matches!(command, "layout.hyphenation" | "layout.differentOddEven") => {
                    under.premises.push(Premise::on(ROOT).fields(["lastSection"]).positions(Vec::<String>::new()));
                    for p in &selected {
                        if let Some(v) = p.props.para_id {
                            under.premises.push(Premise::on(format!("p{v:08X}").as_str()).fields(["para"]).positions(["body"]));
                        }
                    }
                    if command == "layout.linkToPrevious" {
                        let at = kition_doc_engine::doc::Pos::body(focus.path.0.first().copied().unwrap_or(0) as usize, 0);
                        crate::place::hit_premises(doc, &at, under);
                        section_premises(doc, None, &at, &at, SectionScope::Selection, under);
                    } else {
                        section_premises(doc, session.body_return.as_ref(), &a, &b, section_scope(command, params), under);
                    }
                }
                "review" | "comment" => {
                    // Comments read the comments, not the tracked changes; the tracked changes'
                    // commands read both (C8-doc-3: a reply was refused when another writer
                    // accepted a tracked change). Track Changes and protection read the settings.
                    comment_premises(&session, command, params, under);
                    let fields: &[&str] = match command {
                        "review.trackChanges" | "review.restrict" => &[],
                        // A command on one comment holds that comment by its tool id, not every
                        // comment (C8-doc-6).
                        "comment.resolve" | "review.resolveComment" | "review.editComment" => &[],
                        _ if family == "comment"
                            || matches!(
                                command,
                                "review.newComment" | "review.reply" | "review.editComment" | "review.deleteComment" | "review.resolveComment"
                            ) =>
                        {
                            &["comments"]
                        }
                        _ => &["revisions", "comments"],
                    };
                    if !fields.is_empty() {
                        under.premises.push(Premise::on(ROOT).fields(fields.iter().copied()).positions(Vec::<String>::new()));
                    }
                }
                "design" => match design_reads(command) {
                    Some(fields) => {
                        under.premises.push(Premise::on(ROOT).fields(fields.iter().copied()).positions(Vec::<String>::new()));
                        if matches!(command, "design.styleSet" | "design.theme" | "design.themeColors" | "design.themeFonts") {
                            // Retheme visits formatting in every story; unrelated metadata,
                            // media and revisions are not inputs to that traversal.
                            part_premises(doc, None, under);
                            for id in every_paragraph(doc) {
                                under.premises.push(Premise::on(id.as_str()).fields(["text", "para"]).positions(Vec::<String>::new()));
                            }
                        }
                    }
                    None => {
                        under.premises.push(Premise::on(ROOT).fields(ROOT_CONTENT).positions(Vec::<String>::new()));
                        part_premises(doc, None, under);
                    }
                },
                "styles" if command != "styles.inspectorClear" && params.get("style").is_none() => {
                    under.premises.push(Premise::on(ROOT).fields(["styles"]).positions(Vec::<String>::new()));
                }
                "picture" if !matches!(command, "picture.size" | "picture.crop" | "picture.altText" | "picture.reset") => {
                    // Bitmap adjustments decode media and scan the collection for deduplication.
                    under.premises.push(Premise::on(ROOT).fields(["media"]).positions(Vec::<String>::new()));
                }
                "hf" => under.premises.push(Premise::on(ROOT).fields(["parts"]).positions(Vec::<String>::new())),
                _ if matches!(command, "insert.header" | "insert.footer") => {
                    under.premises.push(Premise::on(ROOT).fields(["parts"]).positions(Vec::<String>::new()));
                }
                _ => {}
            }
            // The header or footer a header, footer or page number replaces whole: that it is
            // there (its paragraphs are written blind), by its tool id (C8-doc-6).
            let replaced = match command {
                "insert.header" | "insert.footer" if params.get("text").is_some() || params.get("preset").is_some() => {
                    Some(command == "insert.footer")
                }
                "insert.pageNumber" => match params.get("position").and_then(Value::as_str) {
                    Some("current") => None,
                    Some("top") => Some(false),
                    _ => Some(true),
                },
                _ => None,
            };
            if let Some(footer) = replaced {
                let block = if a.story == StoryRef::Body { a.path.0.first().copied().unwrap_or(0) as usize } else { 0 };
                let names = doc.part_names();
                if let Some(name) = section_part(doc, block, footer).and_then(|part| names.get(&part)) {
                    under.premises.push(Premise::on(format!("part:{name}").as_str()).fields(Vec::<String>::new()).positions(Vec::<String>::new()));
                }
            }
            // `arrange.align` and `arrange.position` read the section only for a selected object.
            let objects = selected_objects(doc, &a, &b);
            if family != "layout" && SECTION_READERS.contains(&command) && (family != "arrange" || !objects.is_empty()) {
                under.premises.push(Premise::on(ROOT).fields(["lastSection"]).positions(Vec::<String>::new()));
                if command == "hf.position" {
                    let at = kition_doc_engine::doc::Pos::body(focus.path.0.first().copied().unwrap_or(0) as usize, 0);
                    crate::place::hit_premises(doc, &at, under);
                    section_premises(doc, None, &at, &at, SectionScope::Selection, under);
                } else {
                    section_premises(doc, session.body_return.as_ref(), &a, &b, section_scope(command, params), under);
                }
            }
            if TABLE_READERS.contains(&command) {
                for t in touched_tables(doc, &a, &b, command == "text.backspace") {
                    table_premises(t, under);
                }
                // The selected paragraphs' properties: their tracked changes (Accept); a cell's
                // paragraph a cut, Tab or Backspace reads.
                let always = matches!(command, "review.accept" | "review.reject");
                for p in &selected {
                    if let Some(v) = p.props.para_id.filter(|_| always || a.path.cell().is_some() || b.path.cell().is_some()) {
                        under.premises.push(Premise::on(format!("p{v:08X}").as_str()).fields(["para"]).positions(Vec::<String>::new()));
                    }
                }
            }
            if let Some(fields) = on_selected_objects(command) {
                // The drawing objects and pictures the command acts on, as `selected_all` finds
                // them, by their numbers.
                for uid in objects {
                    under.premises.push(Premise::on(serde_json::json!(uid)).fields(fields.iter().copied()).positions(Vec::<String>::new()));
                }

                if family == "arrange" {
                    for object in selected_drawings(doc, &a, &b) {
                        let Some(uid) = object.tool_uid().filter(|id| *id > 0) else { continue };
                        let extra = match object {
                            kition_doc_engine::doc::InlineObject::Image { .. } | kition_doc_engine::doc::InlineObject::Group { .. } => Some("value"),
                            kition_doc_engine::doc::InlineObject::Shape { .. } if command == "arrange.bounds" => Some("look"),
                            _ => None,
                        };
                        if let Some(field) = extra {
                            under.premises.push(Premise::on(serde_json::json!(uid)).fields([field]).positions(Vec::<String>::new()));
                        }
                    }
                }

                // Inline inventories now compare frames directly across file rereads.
                // Shape and picture operations still hold their complete assigned values.
                if family != "arrange" {
                    let paths = if a == b { vec![a.path.clone()] } else { doc.paths_between(&a, &b) };
                    for path in paths {
                        if let Some(p) = doc.para(a.story, &path).filter(|p| !p.object_offsets().is_empty())
                            && let Some(v) = p.props.para_id
                        {
                            under.premises.push(Premise::on(format!("p{v:08X}").as_str()).fields(["~objects"]).positions(Vec::<String>::new()));
                        }
                    }
                }
            }
        }
        let holds_styles = |under: &Under| {
            under
                .premises
                .iter()
                .any(|p| p.id == serde_json::json!(ROOT) && p.fields.as_ref().is_none_or(|fields| fields.iter().any(|f| f.starts_with("styles"))))
        };
        if fields.contains(&"styles") && !holds_styles(under) {
            under.premises.push(Premise::on(ROOT).fields(["styles"]).positions(Vec::<String>::new()));
        }
        under.premises.len() > before
    }
}

/// The fingerprint of a JSON value ([`fingerprint`] of its bytes).
pub(crate) fn fingerprint_of(value: &Value) -> String {
    fingerprint(&serde_json::to_vec(value).unwrap_or_default())
}

/// FNV-1a, 64 bits: the same on every build and machine.
fn fingerprint(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

impl DocDocument {
    /// Paragraph facts for inline objects: `~objects` fingerprints complete objects;
    /// `~shapeFrames` fingerprints only shape/graphic uid, size and stored float.
    /// These eager paragraph facts are retained for commands that consume complete
    /// collections of anchored objects. Empty paragraphs return null facts.
    pub(crate) fn derived_objects(&self, ids: &[Value], names: &[String]) -> Vec<kition_kit::DerivedValues> {
        let mut out = self.derived_comments(ids, names);
        self.derived_layout(ids, names, &mut out);
        if names.is_empty() || names.iter().any(|name| name == "frameValidity") {
            // Formatting preserves coordinates; only write_back's validation result matters.
            let mut session = self.session.borrow_mut();
            let objects: Vec<_> = session.doc.numbered().into_iter().map(|(pos, object)| (pos, object.clone())).collect();
            let mut validity: std::collections::BTreeMap<u32, Vec<Value>> = std::collections::BTreeMap::new();
            for (pos, object) in objects {
                let Some(holder) = session.doc.para_at(&pos).and_then(|p| p.props.para_id) else { continue };
                let id = serde_json::json!(format!("p{holder:08X}"));
                if !ids.is_empty() && !ids.contains(&id) {
                    continue;
                }
                let drawing = kition_doc_engine::cmd::drawing::to_drawing(&mut session, &pos, &object);
                validity
                    .entry(holder)
                    .or_default()
                    .push(serde_json::json!([object.tool_uid(), kition_office_core::ops::check_frame(&drawing.frame).is_ok()]));
            }
            for (holder, value) in validity {
                let id = serde_json::json!(format!("p{holder:08X}"));
                let value = fingerprint_of(&serde_json::json!(value)).into();
                if let Some(entry) = out.iter_mut().find(|entry| entry.id == id) {
                    entry.values.insert("frameValidity".into(), value);
                } else {
                    out.push(kition_kit::DerivedValues { id, values: [("frameValidity".into(), value)].into_iter().collect() });
                }
            }
        }
        if (ids.is_empty() || ids.iter().any(|id| id == ROOT))
            && (names.is_empty() || names.iter().any(|name| matches!(name.as_str(), "drawingIds" | "drawingFrames" | "inkIds")))
        {
            let session = self.session.borrow();
            let objects = session.doc.numbered();
            let mut numbers: Vec<u32> = objects.iter().filter_map(|(_, o)| o.tool_uid()).collect();
            for (_, object) in &objects {
                if let kition_doc_engine::doc::InlineObject::Group { children, .. } = object {
                    numbers.extend(children.iter().filter_map(|child| child.obj.tool_uid()));
                }
            }
            numbers.sort_unstable();
            numbers.dedup();
            let frames: Vec<Value> = objects.iter().map(|(_, o)| serde_json::json!([o.tool_uid(), o.frame()])).collect();
            let mut ink_ids: Vec<u32> = session.doc.shapes().iter().filter(|(_, o)| o.ink().is_some()).filter_map(|(_, o)| o.object_uid()).collect();
            ink_ids.sort_unstable();
            ink_ids.dedup();
            for (name, value) in
                [("drawingIds", serde_json::json!(numbers)), ("drawingFrames", serde_json::json!(frames)), ("inkIds", serde_json::json!(ink_ids))]
            {
                if !names.is_empty() && !names.iter().any(|n| n == name) {
                    continue;
                }
                let value = Value::String(fingerprint_of(&value));
                match out.iter_mut().find(|entry| entry.id == ROOT) {
                    Some(entry) => {
                        entry.values.insert(name.to_owned(), value);
                    }
                    None => {
                        out.push(kition_kit::DerivedValues { id: serde_json::json!(ROOT), values: [(name.to_owned(), value)].into_iter().collect() })
                    }
                }
            }
        }
        if names.is_empty() || names.iter().any(|name| name == "tableOrder") {
            fn tables(blocks: &kition_doc_engine::doc::Blocks, ids: &[Value], depth: usize, out: &mut Vec<kition_kit::DerivedValues>) {
                if depth > 32 {
                    return;
                }
                for block in blocks {
                    let Block::Table(table) = &**block else { continue };
                    let rows: Vec<_> = table.rows.iter().map(|row| row.props.para_id).collect();
                    for row in &table.rows {
                        let Some(uid) = row.props.para_id else { continue };
                        let id = serde_json::json!(format!("r{uid:08X}"));
                        if ids.is_empty() || ids.contains(&id) {
                            out.push(kition_kit::DerivedValues {
                                id,
                                values: [("tableOrder".to_owned(), fingerprint_of(&serde_json::json!(rows)).into())].into_iter().collect(),
                            });
                        }
                    }
                    for row in &table.rows {
                        for cell in &row.cells {
                            tables(&cell.blocks, ids, depth + 1, out);
                        }
                    }
                }
            }
            let session = self.session.borrow();
            tables(&session.doc.body, ids, 0, &mut out);
            for part in session.doc.parts.values() {
                tables(&part.blocks, ids, 0, &mut out);
            }
        }
        if (ids.is_empty() || ids.iter().any(|id| id == ROOT)) && (names.is_empty() || names.iter().any(|name| name == "bookmarks")) {
            let session = self.session.borrow();
            let bookmarks: Vec<Value> = session
                .doc
                .bookmarks()
                .into_iter()
                .map(|(name, pos)| serde_json::json!([name, session.doc.para_at(&pos).and_then(|p| p.props.para_id), pos.off]))
                .collect();
            let value = fingerprint_of(&serde_json::json!(bookmarks)).into();
            match out.iter_mut().find(|entry| entry.id == ROOT) {
                Some(entry) => {
                    entry.values.insert("bookmarks".to_owned(), value);
                }
                None => out
                    .push(kition_kit::DerivedValues { id: serde_json::json!(ROOT), values: [("bookmarks".to_owned(), value)].into_iter().collect() }),
            }
        }
        if (ids.is_empty() || ids.iter().any(|id| id == ROOT)) && (names.is_empty() || names.iter().any(|name| name == "paraIds")) {
            let mut ids: Vec<u32> = self.session.borrow().doc.para_ids_in_use().into_iter().collect();
            ids.sort_unstable();
            let value = fingerprint_of(&serde_json::json!(ids)).into();
            match out.iter_mut().find(|entry| entry.id == ROOT) {
                Some(entry) => {
                    entry.values.insert("paraIds".to_owned(), value);
                }
                None => {
                    out.push(kition_kit::DerivedValues { id: serde_json::json!(ROOT), values: [("paraIds".to_owned(), value)].into_iter().collect() })
                }
            }
        }
        let mut at: std::collections::HashMap<Value, usize> = out.iter().enumerate().map(|(i, d)| (d.id.clone(), i)).collect();
        let objects_wanted = names.is_empty() || names.iter().any(|name| name == "objects");
        let frames_wanted = names.is_empty() || names.iter().any(|name| name == "shapeFrames");
        if !objects_wanted && !frames_wanted {
            return out;
        }
        let wanted: Vec<&str> = ids.iter().filter_map(Value::as_str).collect();
        let session = self.session.borrow();
        let doc = &session.doc;
        let stories = std::iter::once(StoryRef::Body).chain(doc.parts.keys().map(|k| StoryRef::Part(*k)));
        for story in stories {
            for path in doc.para_paths(story) {
                let pos = kition_doc_engine::doc::Pos::new(story, path, 0);
                let Some(p) = doc.para_at(&pos) else { continue };
                let Some(v) = p.props.para_id else { continue };
                let id = format!("p{v:08X}");
                if !wanted.is_empty() && !wanted.contains(&id.as_str()) {
                    continue;
                }
                let objects: Vec<&kition_doc_engine::doc::InlineObject> =
                    p.object_offsets().into_iter().filter_map(|off| p.object_at(off)).filter(|o| o.is_drawing() || o.tool_uid().is_some()).collect();
                let mut values = serde_json::Map::new();
                if objects_wanted {
                    let value =
                        if objects.is_empty() { Value::Null } else { Value::String(fingerprint(&serde_json::to_vec(&objects).unwrap_or_default())) };
                    values.insert("objects".to_owned(), value);
                }
                if frames_wanted {
                    use kition_doc_engine::doc::InlineObject;
                    let mut frames: Vec<Value> = objects
                        .iter()
                        .filter_map(|object| match object {
                            InlineObject::Shape { w, h, float, .. } | InlineObject::Graphic { w, h, float, .. } => {
                                Some(serde_json::json!([object.tool_uid(), w, h, float]))
                            }
                            _ => None,
                        })
                        .collect();
                    frames.sort_by_key(|frame| frame.get(0).and_then(Value::as_u64));
                    let value = if frames.is_empty() { Value::Null } else { Value::String(fingerprint_of(&serde_json::json!(frames))) };
                    values.insert("shapeFrames".to_owned(), value);
                }
                // One entry per paragraph: its `~comments` too ([`Self::derived_comments`]).
                let id = Value::String(id);
                match at.get(&id).and_then(|i| out.get_mut(*i)) {
                    Some(entry) => {
                        entry.values.extend(values);
                    }
                    None => {
                        at.insert(id.clone(), out.len());
                        out.push(kition_kit::DerivedValues { id, values });
                    }
                }
            }
        }
        out
    }
}

impl DocDocument {
    /// The derived value `~comments` of paragraphs (`Block`, schema `derived`, C8-doc-6): a
    /// fingerprint of the comment threads whose range starts in each (each comment's durable id,
    /// author, date, text, parent and resolved flag), `null` for a paragraph with none. A thread
    /// is held by no field of the document, so a reread of the file never reads it; the guard
    /// compares this value of the paragraph it is anchored in instead.
    pub(crate) fn derived_comments(&self, ids: &[Value], names: &[String]) -> Vec<kition_kit::DerivedValues> {
        use kition_doc_engine::cmd::review;
        if !names.is_empty() && !names.iter().any(|name| matches!(name.as_str(), "comments" | "commentCount")) {
            return Vec::new();
        }
        let wanted: Vec<&str> = ids.iter().filter_map(Value::as_str).collect();
        let session = self.session.borrow();
        let doc = &session.doc;
        let mut body_anchors: std::collections::BTreeMap<u32, Vec<Value>> = std::collections::BTreeMap::new();
        for path in doc.para_paths(StoryRef::Body) {
            let Some(p) = doc.para(StoryRef::Body, &path) else { continue };
            for off in p.object_offsets() {
                let marker = match p.object_at(off) {
                    Some(kition_doc_engine::doc::InlineObject::CommentStart { id }) => Some((*id, true)),
                    Some(kition_doc_engine::doc::InlineObject::CommentEnd { id }) => Some((*id, false)),
                    _ => None,
                };
                if let Some((id, start)) = marker {
                    body_anchors.entry(review::thread_root(&session, id)).or_default().push(serde_json::json!([
                        doc.comments.get(&id).map(|c| &c.durable_id),
                        p.props.para_id,
                        off,
                        start
                    ]));
                }
            }
        }
        let thread = |root: u32| -> Value {
            let members: Vec<Value> = doc
                .comments
                .iter()
                .filter(|(m, _)| review::thread_root(&session, **m) == root)
                .map(|(_, m)| {
                    let parent = m.parent.and_then(|p| doc.comments.get(&p)).map(|p| p.durable_id.clone());
                    serde_json::json!([
                        m.durable_id,
                        m.author,
                        m.initials,
                        m.para_id,
                        m.date,
                        doc.plain_text(StoryRef::Part(m.part)),
                        doc.parts.get(&m.part).map(|p| &p.blocks),
                        parent,
                        m.resolved
                    ])
                })
                .collect();
            serde_json::json!([members, body_anchors.get(&root)])
        };
        let stories = std::iter::once(StoryRef::Body).chain(doc.parts.keys().map(|k| StoryRef::Part(*k)));
        let mut out = Vec::new();
        if (wanted.is_empty() || wanted.contains(&ROOT)) && (names.is_empty() || names.iter().any(|name| name == "commentCount")) {
            out.push(kition_kit::DerivedValues {
                id: serde_json::json!(ROOT),
                values: [("commentCount".into(), serde_json::json!(doc.comments.len()))].into_iter().collect(),
            });
        }
        if !names.is_empty() && !names.iter().any(|name| name == "comments") {
            return out;
        }
        for story in stories {
            for path in doc.para_paths(story) {
                let Some(p) = doc.para_at(&kition_doc_engine::doc::Pos::new(story, path, 0)) else { continue };
                let Some(v) = p.props.para_id else { continue };
                let id = format!("p{v:08X}");
                if !wanted.is_empty() && !wanted.contains(&id.as_str()) {
                    continue;
                }
                let threads: Vec<Value> = p
                    .objects
                    .iter()
                    .filter_map(|o| match o {
                        kition_doc_engine::doc::InlineObject::CommentStart { id } if review::thread_root(&session, *id) == *id => Some(thread(*id)),
                        _ => None,
                    })
                    .collect();
                let value =
                    if threads.is_empty() { Value::Null } else { Value::String(fingerprint(&serde_json::to_vec(&threads).unwrap_or_default())) };
                out.push(kition_kit::DerivedValues { id: Value::String(id), values: [("comments".to_owned(), value)].into_iter().collect() });
            }
        }
        out
    }
}

impl DocDocument {
    /// Private geometry, ink-count and ink-geometry facts of paragraphs (C8-doc-6). `~layout` is a
    /// fingerprint of where each is laid out (each piece's page, the page's number, its lines and
    /// its place), `null` for one not laid out. A command reading the layout (a page number, what
    /// lies at a point of a page, the page a drawing is on) rests on it. Cost: the guard reads
    /// derived values after every write, so a document is laid out after every write (the
    /// session keeps the layout until the next change). Ink adds a linear scan of laid-out
    /// objects and deterministic fingerprints on their paragraph holders.
    pub(crate) fn derived_layout(&self, ids: &[Value], names: &[String], out: &mut Vec<kition_kit::DerivedValues>) {
        if !names.is_empty() && !names.iter().any(|name| matches!(name.as_str(), "layout" | "inkCount" | "inkGeometry")) {
            return;
        }
        let wanted: Vec<&str> = ids.iter().filter_map(Value::as_str).collect();
        let layout = self.session.borrow_mut().layout();
        let session = self.session.borrow();
        let doc = &session.doc;
        let mut at: std::collections::HashMap<Value, usize> = out.iter().enumerate().map(|(i, d)| (d.id.clone(), i)).collect();
        let mut frames = std::collections::HashMap::new();
        let mut ink_geometry: std::collections::HashMap<_, Vec<Value>> = std::collections::HashMap::new();
        let mut ink_counts: std::collections::HashMap<_, std::collections::BTreeMap<usize, usize>> = std::collections::HashMap::new();
        for (page_index, page) in layout.pages.iter().enumerate() {
            for item in &page.items {
                if let kition_doc_engine::layout::Placed::Object { story, path, off, rect, .. } = item
                    && let Some(kition_doc_engine::doc::InlineObject::Shape { freeform: Some(f), stroke, stroke_width, uid, .. }) =
                        doc.para(*story, path).and_then(|p| p.object_at(*off))
                    && f.is_ink()
                {
                    let key = (*story, path.clone());
                    ink_geometry.entry(key.clone()).or_default().push(serde_json::json!([uid, off, page_index, rect, f, stroke, stroke_width]));
                    let count = ink_counts.entry(key).or_default().entry(page_index).or_default();
                    *count = count.saturating_add(1);
                }
            }
            for item in page.items.iter().chain(&page.header).chain(&page.footer) {
                if let kition_doc_engine::layout::Placed::Object { story, path, off, rect, origin, spin, wrap, .. } = item {
                    frames
                        .entry((*story, path.clone(), *off))
                        .or_insert_with(|| serde_json::json!([off, page_index, rect, origin, spin.deg, spin.flip_h, spin.flip_v, wrap]));
                }
            }
        }
        for ((story, path), pieces) in &layout.index {
            let Some(v) = doc.para(*story, path).and_then(|p| p.props.para_id) else { continue };
            let id = format!("p{v:08X}");
            if !wanted.is_empty() && !wanted.contains(&id.as_str()) {
                continue;
            }
            let placed: Vec<Value> = pieces
                .iter()
                .filter_map(|(page, item)| {
                    let p = layout.pages.get(*page)?;
                    match p.items.get(*item)? {
                        kition_doc_engine::layout::Placed::Lines { para, l0, l1, x, y, .. } => {
                            let lines: Vec<Value> = para
                                .lines
                                .get(*l0..*l1)
                                .unwrap_or_default()
                                .iter()
                                .map(|line| serde_json::json!([line.top, line.height, line.baseline, line.start, line.left, line.right]))
                                .collect();
                            Some(serde_json::json!([page, p.number, p.w, p.h, p.body, l0, l1, x, y, lines]))
                        }
                        _ => Some(serde_json::json!([page, p.number, p.w, p.h, p.body])),
                    }
                })
                .collect();
            let objects: Vec<Value> = doc
                .para(*story, path)
                .into_iter()
                .flat_map(|p| p.object_offsets())
                .filter_map(|off| frames.get(&(*story, path.clone(), off)).cloned())
                .collect();
            let key = (*story, path.clone());
            let mut values = serde_json::Map::new();
            for (name, value) in [
                ("layout", serde_json::json!([placed, objects])),
                ("inkCount", serde_json::json!(ink_counts.get(&key))),
                ("inkGeometry", serde_json::json!(ink_geometry.get(&key))),
            ] {
                if names.is_empty() || names.iter().any(|wanted| wanted == name) {
                    values.insert(name.to_owned(), Value::String(fingerprint_of(&value)));
                }
            }
            let id = Value::String(id);
            match at.get(&id).and_then(|i| out.get_mut(*i)) {
                Some(entry) => {
                    entry.values.extend(values);
                }
                None => {
                    at.insert(id.clone(), out.len());
                    out.push(kition_kit::DerivedValues { id, values });
                }
            }
        }
    }
}
