//! The family of a command: the group it shares its handler's reading and its guards with.
//!
//! A command's namespace names the object it acts on (`page`, `comment`, `file`), so it does not
//! say what the handler reads or which guard applies: `page.insertBreak` writes text at the
//! caret, `page.margins` the section, `page.color` a setting. The families group the commands by
//! that instead. `Session::run`'s guards, the column block and the kit's premises
//! (`bindings/wasm/src/family.rs`, table in `agent/guide.md`) ask for the family, never for a
//! prefix of the id.
//!
//! | Family | Commands |
//! |---|---|
//! | `paragraph` | `paragraph.*` and the four alignments `format.alignLeft`, `format.alignCenter`, `format.alignRight`, `format.justify` |
//! | `format` | the other `format.*` (character formatting) |
//! | `insert` | what puts one thing at the caret: `page.insertBreak`, `page.insertBlank`, `page.insertCover`, `table.insert`, `image.insert`, `shape.insert`, `equation.insert`, `text.insertFromFile`, `field.insert`, `field.insertDateTime`, `field.insertDocProperty`, and `artText.*`, `autoText.*`, `bookmark.*`, `chart.*`, `crossReference.*`, `dropCap.*`, `footer.*`, `header.*`, `horizontalLine.*`, `link.*`, `object.*`, `pageNumber.*`, `quickPart.*`, `signatureLine.*`, `spreadsheet.*`, `symbol.*`, `textBox.*` |
//! | `section` | the other `page.*` (margins, size, columns, breaks, line numbers) |
//! | `design` | `theme.*`, `page.watermark`, `page.color`, `page.borders` |
//! | `reference` | `toc.*`, `footnote.*`, `endnote.*`, `caption.*`, `citation.*`, `bibliography.*`, `tableOfFigures.*`, `index.*`, `tableOfAuthorities.*`, `research.*`, `field.updateAll` |
//! | `review` | `change.*`, `spelling.*`, the comment commands on the current comment (`comment.new`, `comment.replyCurrent`, `comment.edit`, `comment.deleteCurrent`, `comment.resolveCurrent`, `comment.next`, `comment.previous`, `comment.pane`), `file.wordCount`, `file.compare`, `file.combine`, `file.restrictEditing`, `ink.hide`, `readAloud.open` |
//! | `comment` | the other `comment.*` (a comment or thread named by id) |
//! | `content` | reading and replacing the content: `file.inspect`, `file.text`, `file.paragraph`, `file.selection`, `file.layout`, `file.setText` |
//! | `file` | the other `file.*` (open, save, export, print, properties, protection) |
//! | `select` | `select.*` and `edit.selectAll` |
//! | `tools` | `macro.*`, `autocorrect.*` |
//! | any other | the namespace itself (`edit`, `text`, `table`, `style`, `image`, `ink`, `mailMerge`, `headerFooter`, `equation`, `shape`, …) |

/// The family of command `id` (see the module docs). An id no command has answers its namespace.
pub fn family(id: &str) -> &str {
    let namespace = id.split_once('.').map_or(id, |(namespace, _)| namespace);
    match id {
        "format.alignLeft" | "format.alignCenter" | "format.alignRight" | "format.justify" => "paragraph",
        "edit.selectAll" => "select",
        "page.insertBreak"
        | "page.insertBlank"
        | "page.insertCover"
        | "table.insert"
        | "image.insert"
        | "shape.insert"
        | "equation.insert"
        | "text.insertFromFile"
        | "field.insert"
        | "field.insertDateTime"
        | "field.insertDocProperty" => "insert",
        "page.watermark" | "page.color" | "page.borders" => "design",
        "field.updateAll" => "reference",
        "comment.new"
        | "comment.replyCurrent"
        | "comment.edit"
        | "comment.deleteCurrent"
        | "comment.resolveCurrent"
        | "comment.next"
        | "comment.previous"
        | "comment.pane"
        | "file.wordCount"
        | "file.compare"
        | "file.combine"
        | "file.restrictEditing"
        | "ink.hide"
        | "readAloud.open" => "review",
        "file.inspect" | "file.text" | "file.paragraph" | "file.selection" | "file.layout" | "file.setText" => "content",
        _ => match namespace {
            "artText" | "autoText" | "bookmark" | "chart" | "crossReference" | "dropCap" | "footer" | "header" | "horizontalLine" | "link"
            | "object" | "pageNumber" | "quickPart" | "signatureLine" | "spreadsheet" | "symbol" | "textBox" => "insert",
            "page" => "section",
            "theme" => "design",
            "bibliography" | "caption" | "citation" | "endnote" | "footnote" | "index" | "research" | "tableOfAuthorities" | "tableOfFigures"
            | "toc" => "reference",
            "change" | "spelling" => "review",
            "macro" | "autocorrect" => "tools",
            other => other,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::family;

    #[test]
    fn a_family_is_not_the_namespace_where_the_handlers_differ() {
        for (id, expected) in [
            ("format.bold", "format"),
            ("format.justify", "paragraph"),
            ("paragraph.indent", "paragraph"),
            ("page.insertBreak", "insert"),
            ("page.margins", "section"),
            ("page.color", "design"),
            ("theme.set", "design"),
            ("field.updateAll", "reference"),
            ("field.insert", "insert"),
            ("comment.new", "review"),
            ("comment.add", "comment"),
            ("file.setText", "content"),
            ("file.restrictEditing", "review"),
            ("file.removeHiddenContent", "file"),
            ("file.save", "file"),
            ("edit.selectAll", "select"),
            ("ink.hide", "review"),
            ("ink.stroke", "ink"),
            ("image.insert", "insert"),
            ("image.crop", "image"),
            ("text.insertFromFile", "insert"),
            ("text.insert", "text"),
            ("macro.record", "tools"),
        ] {
            assert_eq!(family(id), expected, "{id}");
        }
    }
}
