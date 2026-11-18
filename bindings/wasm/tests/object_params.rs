//! C8-land-14-doc item 2: every parameter that names an object of the document, by id or by
//! name, is marked for the premise guard and typed by what its handler resolves it to; every one
//! left unmarked is allowed with the one-line reason it names nothing a premise could hold.
//! Declarations come from reading each handler (`crates/engine/engine/src/cmd/*.rs`,
//! kition-office-core `DrawingOp::parse`), see `src/acts.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_doc_kit::DocDocument;
use kition_kit::KitDocument;
use serde_json::Value;

fn commands() -> Vec<kition_kit::KitCommand> {
    DocDocument::create("a.docx", 612, 792).unwrap().commands()
}

/// Integer parameters that say "id" and name no object of the object model.
const ID_ALLOW: &[(&str, &str)] = &[
    // A comment's number in this session (`w:id`, `c<n>`) or its tool id (`comment:<durable id>`,
    // C8-doc-6): the handler resolves either (`review.rs` `comment_param`), and `under` holds the
    // comment and its thread by their tool ids (`family.rs` `comment_premises`). Not marked as an
    // object: the session number, which every existing caller passes, is no tool id.
    ("review.reply", "id"),
    ("review.editComment", "id"),
    ("review.deleteComment", "id"),
    ("review.resolveComment", "id"),
    ("comment.reply", "id"),
    // A list's numbering id (`w:numId`) in this session: lists are entries of the root's `lists`
    // map keyed by UUID, with no tool id (reported gap: redefining a list in place holds nothing).
    ("list.define", "num"),
    // A read: answers a list's definition, changes nothing.
    ("list.get", "num"),
];

#[test]
fn every_integer_id_parameter_is_marked() {
    kition_kit::testing::check_id_params(&commands(), ID_ALLOW).unwrap_or_else(|problems| panic!("{}", problems.join("\n")));
}

/// Parameter names that take a name of something in a document.
const NAME_WORDS: &[&str] = &["name", "style", "basedOn", "next", "bookmark", "thread", "comment", "target", "tag", "field", "font", "source"];

/// String parameters that take a name and name no object of the object model, with why.
const NAME_ALLOW: &[(&str, &str, &str)] = &[
    (
        "*",
        "style",
        "a style: an entry of the root's `styles` register, no object; `under` holds that entry (`styles.styles[id=…]`) for the commands that name one",
    ),
    ("*", "basedOn", "a style, as `style`"),
    ("*", "next", "a style, as `style`"),
    ("styles.create", "name", "the new style's name"),
    ("styles.modify", "name", "the style's new name"),
    ("table.newStyle", "name", "the new table style's name"),
    ("table.modifyStyle", "name", "the table style's new name"),
    ("comment.add", "name", "the author's display name, written into the new comment"),
    ("comment.reply", "name", "the author's display name, written into the reply"),
    ("file.setAuthor", "name", "the author of new tracked changes: a session setting"),
    ("format.font", "name", "a font family name, no object"),
    ("format.fontDialog", "name", "a font family name, no object"),
    ("insert.symbol", "font", "the symbol's font family name, no object"),
    ("design.theme", "name", "a built-in theme of the gallery, no object of the document"),
    ("design.themeColors", "name", "a built-in colour set of the gallery"),
    ("design.effects", "name", "a built-in effects set of the gallery"),
    ("design.styleSet", "name", "a built-in style set of the gallery"),
    ("layout.size", "name", "a paper size name (Letter, A4)"),
    ("list.define", "name", "the new list's name"),
    ("insert.bookmark", "name", "the new bookmark's name, made at the selection"),
    ("insert.docProperty", "name", "a document property by its name (Title, Author): a member of the root's `core` register"),
    ("tools.recordMacro", "name", "a macro of this session, never in the document"),
    ("edit.goto", "bookmark", "moves the caret only (no edit): navigation an agent's later step reads through the selection"),
    ("comment.goto", "bookmark", "moves the caret only (no edit)"),
    ("comment.goto", "thread", "moves the caret only (no edit)"),
    (
        "comment.reply",
        "thread",
        "a comment's session number (`c<n>`) or its tool id (`comment:<id>`, `thread:<id>`): `under` holds it by its tool id (C8-doc-6)",
    ),
    (
        "comment.resolve",
        "thread",
        "a comment's session number (`c<n>`) or its tool id (`comment:<id>`, `thread:<id>`): `under` holds it by its tool id (C8-doc-6)",
    ),
    (
        "comment.delete",
        "thread",
        "a comment's session number (`c<n>`) or its tool id (`comment:<id>`, `thread:<id>`): `under` holds it by its tool id (C8-doc-6)",
    ),
    (
        "comment.delete",
        "comment",
        "a comment's session number (`c<n>`) or its tool id (`comment:<id>`, `thread:<id>`): `under` holds it by its tool id (C8-doc-6)",
    ),
    (
        "insert.crossReference",
        "target",
        "a heading's text, a bookmark or a caption number the new field refers to; the field is written at the caret and reads its target when updated",
    ),
    ("references.citation", "tag", "a source's tag in the root's `sources` register, no object"),
    ("mailings.insertField", "field", "a merge field of the mail merge data source, outside the document"),
    ("mailings.rules", "field", "a merge field of the mail merge data source, outside the document"),
    ("references.citation", "source", "a new bibliography source's values, no reference"),
];

#[test]
fn every_name_parameter_is_marked_or_allowed() {
    let mut problems = Vec::new();
    for command in commands() {
        let command = command.with_kit_schema();
        let Some(properties) = command.schema.as_ref().and_then(Value::as_object) else { continue };
        for (name, node) in properties {
            if !NAME_WORDS.contains(&name.as_str()) {
                continue;
            }
            let takes_text = node.get("type").and_then(Value::as_str) == Some("string") && node.get("enum").is_none()
                || node
                    .get("oneOf")
                    .and_then(Value::as_array)
                    .is_some_and(|branches| branches.iter().any(|b| b["type"] == "string" && b.get("enum").is_none()));
            if !takes_text {
                continue;
            }
            let marked = command.targets == Some(name.as_str())
                || command.objects.contains(&name.as_str())
                || command.name_params.iter().any(|(param, _)| param == name)
                || command.places.contains(&name.as_str());
            let allowed = NAME_ALLOW.iter().any(|(id, param, _)| (*id == "*" || *id == command.id) && param == name);
            if !marked && !allowed {
                problems.push(format!("{}: `{name}` takes a name but is neither marked nor allowed", command.id));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_allow_list_entry_is_used() {
    let commands: Vec<_> = commands().into_iter().map(|command| command.with_kit_schema()).collect();
    for (id, param, _) in NAME_ALLOW.iter().filter(|(id, _, _)| *id != "*") {
        let command = commands.iter().find(|command| command.id == *id).unwrap_or_else(|| panic!("{id}: no such command"));
        assert!(command.schema.as_ref().and_then(|schema| schema.get(param)).is_some(), "{id}: no parameter `{param}`");
    }
    for (id, param) in ID_ALLOW {
        let command = commands.iter().find(|command| command.id == *id).unwrap_or_else(|| panic!("{id}: no such command"));
        assert!(command.schema.as_ref().and_then(|schema| schema.get(param)).is_some(), "{id}: no parameter `{param}`");
    }
}

/// Drawing objects are named by their number, the `Inline` object's tool id, in the parameters
/// whose handler resolves them (`DrawingOp::parse`): `ids` (the targets `on` fills), `id`, and a
/// connector's `from` and `to`.
#[test]
fn drawing_object_parameters_are_typed() {
    let commands = commands();
    let get = |id: &str| commands.iter().find(|command| command.id == id).unwrap();
    for id in ["drawing.transform", "drawing.format", "drawing.delete", "drawing.group", "drawing.ungroup", "drawing.arrange"] {
        assert_eq!(get(id).targets, Some("ids"), "{id}");
        assert!(get(id).object_types.contains(&("ids", &["Inline"][..])), "{id}");
    }
    assert_eq!(get("drawing.connect").objects, &["from", "to"]);
    assert_eq!(get("drawing.text").objects, &["id"]);
    // Positions are places, never ids.
    assert_eq!(get("select.range").places, &["anchor", "focus"]);
    assert_eq!(get("para.text").places, &["index"]);
}
