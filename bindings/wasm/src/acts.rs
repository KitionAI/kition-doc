//! What each command acts on and reads besides its parameters (C8-land-14-doc items 2 and 3),
//! declared for the premise guard and the call context, from reading each handler in
//! `crates/engine/engine/src/cmd/*.rs` and kition-office-core's `DrawingOp::parse`:
//!
//! - **Places**: the parameters that address text by position: a body block index and a byte
//!   offset (`{block, off}`), or a story, a path of indexes and an offset. The kit answers what
//!   they name ([`crate::place`]); they are never read as object ids.
//! - **Objects**: drawing objects (shapes, text boxes, groups, charts) by their number, the
//!   `Inline` object's tool id `uid`; `drawing.*` take them in `ids` (their targets, which `on`
//!   fills) or `id`, a connector's ends in `from` and `to`.
//! - **Reads**: almost every command acts on the selection or the caret (`select.range` sets it
//!   in the same run), so every command declares [`EditorState::Selection`] except those
//!   `tests/acts.rs` proves give the same result whatever the selection is. No command reads a
//!   part or a time (Doc has neither); none reads the current page (`tests/acts.rs` again).
use kition_kit::EditorState::{self, Mode, Selection};
use kition_kit::KitCommand;

const SELECTION: &[EditorState] = &[Selection];
const SELECTION_AND_MODE: &[EditorState] = &[Selection, Mode];

/// Commands that read the editing mode (C8-doc-4, from the independent reading): typing in an
/// equation (`Session::math`: typing, deleting and pasting leave it with the caret after the
/// equation, `equation.*` edit it) and a column selection (`Session::column`: cutting, case and
/// size changes, styles and languages act on each of its lines).
pub fn reads_mode(id: &str) -> bool {
    kition_doc_engine::cmd::family::family(id) == "text"
        || matches!(
            id,
            "equation.backspace"
                | "equation.convert"
                | "equation.delete"
                | "equation.display"
                | "equation.enter"
                | "equation.insertStructure"
                | "equation.insertSymbol"
                | "equation.justify"
                | "equation.number"
                | "equation.set"
                | "equation.structure"
                | "equation.type"
                | "edit.cut"
                | "edit.paste"
                | "format.changeCase"
                | "format.charStyle"
                | "format.increaseFontSize"
                | "format.increaseFontSizeByOne"
                | "format.decreaseFontSize"
                | "format.decreaseFontSizeByOne"
                | "paragraph.style"
                | "spelling.language"
        )
}

/// Commands whose document effect does not depend on the selection, caret or pending formatting,
/// each proven by `tests/acts.rs` (run twice with different selections). Metadata writes may echo
/// the unchanged selection for the UI; those full answers are checked separately. After the
/// first four, those the independent reading (`tests/declarations_independent.rs`, C8-doc-4) found
/// read no editor state: settings, themes, styles by name, comments by id, mail merge records,
/// paragraphs by index, the whole document's text.
pub const SELECTION_FREE: &[&str] = &[
    "select.range",
    "caret.set",
    "file.text",
    "file.paragraph",
    "theme.effects",
    "page.color",
    "theme.paragraphSpacing",
    "theme.styleSet",
    "theme.set",
    "theme.colors",
    "theme.fonts",
    "page.watermark",
    "comment.reply",
    "comment.resolve",
    "file.setText",
    "drawing.arrange",
    "file.encrypt",
    "file.new",
    "file.properties",
    "file.protect",
    "page.hyphenation",
    "page.differentOddEven",
    "mailMerge.envelopes",
    "mailMerge.findRecipient",
    "mailMerge.finish",
    "mailMerge.labels",
    "mailMerge.next",
    "mailMerge.preview",
    "mailMerge.previous",
    "paragraph.insert",
    "footnote.options",
    "citation.sources",
    "file.compare",
    "comment.edit",
    "comment.replyCurrent",
    "file.restrictEditing",
    "change.track",
    "style.addToGallery",
    "style.delete",
    "style.set",
    "style.setVisibility",
];

/// The place parameters of each command (see the module docs).
pub fn places(id: &str) -> &'static [&'static str] {
    match id {
        "select.range" | "select.column" => &["anchor", "focus"],
        "caret.set" | "spelling.suggestions" | "equation.edit" => &["pos"],
        "select.addObject" => &["pos", "index", "objects"],
        "select.objects" => &["index"],
        "paragraph.text" => &["index"],
        "paragraph.insert" => &["after", "before"],
        "file.paragraph" | "file.text" | "paragraph.dialog" => &["path"],
        "edit.goto" | "comment.goto" => &["paragraph"],
        // Page points (C8-doc-2 item 4): what the eraser's path takes depends on where every ink
        // stroke is (`place::under` names them all).
        "drawing.erase" => &["placement", "path", "x", "y"],
        _ => &[],
    }
}

/// Fills `command`'s declarations.
pub fn declare(command: &mut KitCommand) {
    let id = command.id.as_str();
    command.places = places(id);
    command.reads = if SELECTION_FREE.contains(&id) {
        &[]
    } else if reads_mode(id) {
        SELECTION_AND_MODE
    } else {
        SELECTION
    };
    match id {
        "drawing.describe" | "drawing.delete" | "drawing.transform" | "drawing.format" | "drawing.group" | "drawing.ungroup" | "drawing.arrange" => {
            command.targets = Some("ids");
            command.objects = &["id"];
            command.object_types = &[("ids", &["Inline"]), ("id", &["Inline"])];
        }
        "drawing.chart" => {
            // Its `ids` schema is an untyped array: an object parameter, not the targets `on`
            // fills (`schema::check_targets` takes only typed id lists).
            command.objects = &["id", "ids"];
            command.object_types = &[("ids", &["Inline"]), ("id", &["Inline"])];
        }
        "drawing.text" => {
            command.objects = &["id"];
            command.object_types = &[("id", &["Inline"])];
        }
        "drawing.reconnect" => {
            command.objects = &["id", "to"];
            command.object_types = &[("id", &["Inline"]), ("to", &["Inline"])];
        }
        "drawing.connect" => {
            command.objects = &["from", "to"];
            command.object_types = &[("from", &["Inline"]), ("to", &["Inline"])];
        }
        _ => {}
    }
}

/// Whether `command` with `params` pastes what the session's clipboard holds (C8-doc-5, from the
/// independent reading): `edit.paste` always (given `text`, it still compares it with the
/// clipboard to keep the clipboard's formatting), the clipboard history, a formatting copied with
/// `edit.copyFormat`, and the plain-text and special pastes given no content. No stored field
/// holds the clipboard and an agent cannot read it: the kit refuses these for an agent unless its
/// own call copied first (`DocDocument::execute`).
pub fn reads_clipboard(command: &str, params: &serde_json::Value) -> bool {
    let given = |name: &str| params.get(name).is_some_and(|value| !value.is_null());
    match command {
        "edit.paste" | "edit.pasteAllClipboard" | "edit.pasteClipboardItem" | "edit.pasteFormat" => true,
        "edit.pasteText" | "edit.pasteMerge" => !given("text"),
        "edit.pasteSpecial" => !given("text") && !given("html"),
        _ => false,
    }
}

/// The name of the kit's external input that is the session's mail merge list (C8-doc-6).
pub const MERGE_SOURCE: &str = "mergeSource";

/// Whether `command` with `params` reads the session's mail merge list (C8-doc-6, from the
/// independent reading): the records it previews, steps through, finds, merges or prints on
/// envelopes and labels. The list is the kit's external input [`MERGE_SOURCE`]: a write resting
/// on it is refused when the list changed since the agent's basis.
pub fn reads_merge_source(command: &str, params: &serde_json::Value) -> bool {
    let given = |name: &str| params.get(name).is_some_and(|value| !value.is_null());
    match command {
        "mailMerge.next" | "mailMerge.previous" | "mailMerge.preview" | "mailMerge.findRecipient" | "mailMerge.finish" => true,
        "mailMerge.envelopes" => !given("delivery"),
        "mailMerge.labels" => !given("text"),
        _ => false,
    }
}
