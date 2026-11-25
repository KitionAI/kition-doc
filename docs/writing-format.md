# Doc writing format

How a model writes Kition Doc documents: the steps it sends, how it names what it changes,
what a new document starts with, the named values commands accept, and worked examples.
The package ships the short version for models as [agent/guide.md](../agent/guide.md); this file is
the complete reference it is drawn from. Everything below comes from the engine itself
(its 491 commands and their parameter schemas, and what `inspect` reports) and from the guide.

## What a model writes

A model never writes the file bytes. It calls the kit's agent tools (prefix `doc`), and the
engine applies each change, keeps it in undo history and saves the file:

- `doc_inspect`: read the document; `doc_commands`: find commands by keyword;
- `doc_run`: run steps `{command, params}` in order;
- `doc_check`: list problems; `doc_query` / `doc_batch`: read or set objects of the object model.

## Syntax

A step is one engine command with its parameters, as JSON:

```json
{"command": "<command id>", "params": {"<name>": <value>}}
```

`doc_run` takes a list of steps and runs them in order, stopping at the first failure. Parameter
names, types and required fields come from each command's schema; `doc_commands` returns them.
Commands by namespace (the part of the id before the first dot, which names the object a command
acts on: `page`, `comment`, `file`; not the ribbon tab it is on):

| Namespace | Commands | Change the document | Open to the agent | Examples |
| --- | ---: | ---: | ---: | --- |
| `arrange` | 12 | 11 | 12 | `arrange.rotate`, `arrange.rotation`, `arrange.wrap` |
| `artText` | 1 | 1 | 1 | `artText.insert` |
| `autocorrect` | 1 | 0 | 1 | `autocorrect.options` |
| `autoText` | 1 | 1 | 1 | `autoText.insert` |
| `bibliography` | 1 | 1 | 1 | `bibliography.insert` |
| `bookmark` | 1 | 1 | 1 | `bookmark.insert` |
| `caption` | 1 | 1 | 1 | `caption.insert` |
| `caret` | 16 | 0 | 16 | `caret.left`, `caret.right`, `caret.up` |
| `change` | 11 | 5 | 11 | `change.track`, `change.acceptAll`, `change.rejectAll` |
| `chart` | 1 | 1 | 1 | `chart.insert` |
| `citation` | 3 | 3 | 3 | `citation.sources`, `citation.insert`, `citation.style` |
| `comment` | 16 | 9 | 16 | `comment.new`, `comment.replyCurrent`, `comment.edit` |
| `crossReference` | 1 | 1 | 1 | `crossReference.insert` |
| `drawing` | 14 | 13 | 14 | `drawing.describe`, `drawing.insert`, `drawing.delete` |
| `dropCap` | 1 | 1 | 1 | `dropCap.set` |
| `edit` | 26 | 12 | 26 | `edit.selectAll`, `edit.undo`, `edit.redo` |
| `endnote` | 1 | 1 | 1 | `endnote.insert` |
| `equation` | 21 | 13 | 21 | `equation.insert`, `equation.edit`, `equation.exit` |
| `field` | 4 | 4 | 4 | `field.insertDateTime`, `field.insert`, `field.updateAll` |
| `file` | 34 | 16 | 34 | `file.wordCount`, `file.new`, `file.open` |
| `footer` | 3 | 3 | 3 | `footer.insert`, `footer.edit`, `footer.remove` |
| `footnote` | 4 | 2 | 4 | `footnote.insert`, `footnote.next`, `footnote.show` |
| `format` | 40 | 38 | 40 | `format.bold`, `format.italic`, `format.underline` |
| `header` | 4 | 3 | 4 | `header.insert`, `header.edit`, `header.close` |
| `headerFooter` | 3 | 1 | 3 | `headerFooter.next`, `headerFooter.previous`, `headerFooter.position` |
| `horizontalLine` | 1 | 1 | 1 | `horizontalLine.insert` |
| `image` | 14 | 14 | 14 | `image.insert`, `image.size`, `image.crop` |
| `index` | 3 | 3 | 3 | `index.markEntry`, `index.insert`, `index.update` |
| `ink` | 10 | 2 | 10 | `ink.select`, `ink.shape`, `ink.eraser` |
| `link` | 2 | 2 | 2 | `link.insert`, `link.remove` |
| `list` | 2 | 1 | 2 | `list.define`, `list.get` |
| `macro` | 2 | 0 | 2 | `macro.record`, `macro.list` |
| `mailMerge` | 17 | 11 | 17 | `mailMerge.start`, `mailMerge.recipients`, `mailMerge.editRecipients` |
| `object` | 1 | 1 | 1 | `object.insert` |
| `page` | 20 | 19 | 20 | `page.insertBreak`, `page.insertBlank`, `page.insertCover` |
| `pageNumber` | 1 | 1 | 1 | `pageNumber.insert` |
| `paragraph` | 43 | 42 | 43 | `paragraph.distribute`, `paragraph.align`, `paragraph.rtl` |
| `presence` | 2 | 0 | 2 | `presence.caret`, `presence.locate` |
| `quickPart` | 1 | 1 | 1 | `quickPart.insert` |
| `readAloud` | 8 | 0 | 6 | `readAloud.open`, `readAloud.playPause`, `readAloud.next` |
| `research` | 1 | 0 | 1 | `research.open` |
| `select` | 12 | 0 | 12 | `select.range`, `select.word`, `select.sentence` |
| `shape` | 5 | 5 | 5 | `shape.insert`, `shape.fill`, `shape.outline` |
| `signatureLine` | 1 | 1 | 1 | `signatureLine.insert` |
| `spelling` | 10 | 2 | 10 | `spelling.check`, `spelling.issues`, `spelling.suggestions` |
| `spreadsheet` | 1 | 1 | 1 | `spreadsheet.insert` |
| `style` | 13 | 7 | 13 | `style.add`, `style.set`, `style.updateToMatch` |
| `symbol` | 1 | 1 | 1 | `symbol.insert` |
| `table` | 41 | 37 | 41 | `table.viewGridlines`, `table.insert`, `table.insertRowAbove` |
| `tableOfAuthorities` | 2 | 2 | 2 | `tableOfAuthorities.markCitation`, `tableOfAuthorities.insert` |
| `tableOfFigures` | 2 | 2 | 2 | `tableOfFigures.insert`, `tableOfFigures.update` |
| `text` | 15 | 15 | 15 | `text.insert`, `text.newParagraph`, `text.lineBreak` |
| `textBox` | 1 | 1 | 1 | `textBox.insert` |
| `theme` | 8 | 6 | 8 | `theme.set`, `theme.fonts`, `theme.colors` |
| `toc` | 4 | 4 | 4 | `toc.insert`, `toc.update`, `toc.remove` |
| `view` | 26 | 0 | 26 | `view.marks`, `view.printLayout`, `view.webLayout` |

### The document

- The **body** is a list of **blocks**: paragraphs and tables. `doc_inspect` lists them as
  `content.paragraphs`, each with its 0-based `index`, `style` (`Heading1`, `Normal`, …),
  `type` (`paragraph` or `table`) and `text`. A table shows as one block with empty `text`; read its
  cells with `doc_query`.
- A paragraph's text is split into **runs** with the same formatting. You never address runs:
  select text, then run a formatting command, and the kit splits runs for you.
- Headers, footers, footnotes and endnotes are separate **stories** (`parts` in `doc_query`).
- `doc_query { "select": "root" }` returns the whole object model: `body` (block ids), `styles`
  (one entry per style, keyed by style id), `styleDefaults` (the document's default character and
  paragraph formatting), `settings` (theme fonts), `lastSection` (page size and margins in points).
  It is large; ask for the objects you need. A table's columns are its `cols` (each a `width` in
  points).

### Addressing: positions

Most editing commands act on the **selection**. Set it with `select.range`:

```json
{ "command": "select.range", "params": { "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 5 } } }
```

- `block` is the 0-based index of a body paragraph, as `doc_inspect` lists it.
- `off` is a **byte offset** into the paragraph's text (UTF-8), from 0. The end of a paragraph is
  its text's byte length. A caret is a selection whose `anchor` equals its `focus`.
- Inside a table, or in another story, give the full path:
  `{ "story": "body", "path": [table, row, cell, paragraph], "off": n }`, all 0-based.
  `{ "path": [...] }` without `story` is refused.
- Every `doc_run` step answers the selection it left (`result.anchor`, `result.focus`), so you can
  check where the next step will act.

## Text writing format

The engine reads no text writing format; a model writes through the commands above.

## Defaults

What a new, empty document starts with:

- File name `Untitled doc.docx`, size 612 × 792 (`kit.json` `creates`).

What `doc_inspect` reports for a new document made at that size (generated from the engine; lists shortened to 20 entries):

```json
{
  "canRedo": false,
  "canUndo": false,
  "chart": null,
  "equation": false,
  "page": 0,
  "pages": [
    {
      "area": {
        "height": 792,
        "width": 612,
        "x": 40,
        "y": 40
      },
      "index": 0
    }
  ],
  "selected": 0,
  "shape": null,
  "shapes": [],
  "story": "body",
  "words": 0
}
```

## Themes, layouts and other named values

Parameters that take one of a fixed list of names (themes, layouts, presets, styles, kinds, modes,
units and similar), from the command schemas. A parameter not listed here takes free values; its
schema says which.

| Parameter | Values | Used by |
| --- | --- | --- |
| `align` | `left`, `center`, `right` | `pageNumber.insert`, `table.properties` |
| `direction` | `right`, `left`, `flipVertical`, `flipHorizontal` | `arrange.rotate` |
| `effect` | `blur`, `sharpen`, `invert`, `posterize`, `pixelate` | `image.effects` |
| `footnoteFormat` | `decimal`, `lowerRoman`, `upperRoman`, `lowerLetter`, `upperLetter` | `footnote.options` |
| `format` | `unicode`, `latex` | `equation.inputFormat` |
| `format` | `decimal`, `lowerRoman`, `upperRoman`, `lowerLetter`, `upperLetter` | `page.pageNumberFormat` |
| `kind` | `numbered`, `numberedParen`, `upperLetter`, `lowerLetter`, `lowerRoman`, `outline` | `paragraph.numbering` |
| `kind` | `legal`, `outline` | `paragraph.multilevel` |
| `kind` | `bottom`, `top`, `left`, `right`, `none`, `all`, `outside`, `inside`, `horizontalLine` | `paragraph.borders` |
| `kind` | `rectangle`, `roundedRectangle`, `ellipse`, `triangle`, `diamond`, `line`, `arrow`, `star`, `heart` | `shape.insert` |
| `kind` | `page`, `column`, `textWrapping`, `nextPage`, `continuous`, `evenPage`, `oddPage` | `page.break` |
| `kind` | `all`, `outside`, `inside`, `none`, `top`, `bottom`, `left`, `right` | `table.borders` |
| `kind` | `calendar`, `tabular`, `matrix` | `table.quick` |
| `kind` | `box`, `none` | `page.borders` |
| `kind` | `letters`, `emails`, `envelopes`, `labels`, `directory`, `normal` | `mailMerge.start` |
| `kind` | `shape`, `textBox`, `chart` | `drawing.insert` |
| `mode` | `sentence`, `lower`, `upper`, `title`, `toggle` | `format.changeCase` |
| `mode` | `contents`, `window`, `fixed` | `table.autofit` |
| `mode` | `grayscale`, `sepia`, `washout`, `blackAndWhite`, `saturation`, `tint` | `image.color` |
| `mode` | `none`, `readOnly`, `comments`, `trackedChanges`, `forms` | `file.restrictEditing` |
| `mode` | `none`, `readOnly`, `comments`, `trackedChanges` | `file.protect` |
| `preset` | `blank`, `blankThree`, `title` | `header.insert` |
| `preset` | `blank`, `blankThree`, `pageNumber` | `footer.insert` |
| `preset` | `normal`, `narrow`, `moderate`, `wide`, `mirrored`, `office2003` | `page.margins` |
| `preset` | `left`, `right` | `page.columns` |
| `preset` | `topLeft`, `topCenter`, `topRight`, `middleLeft`, `middleCenter`, `middleRight`, `bottomLeft`, `bottomCenter`, `bottomRight` | `arrange.position` |
| `preset` | `straightConnector1`, `bentConnector3`, `curvedConnector3` | `drawing.connect` |
| `style` | `single`, `double`, `dotted`, `dashed`, `thick`, `triple`, `dotDash`, `wave` | `page.borders`, `paragraph.borders` |
| `style` | `single`, `double`, `thick`, `dotted`, `dash`, `dotDash`, `dotDotDash`, `wave`, `words` | `format.underline` |
| `style` | `APA`, `MLA`, `Chicago`, `IEEE` | `citation.style` |
| `style` | `simpleFrame`, `thickFrame`, `rounded`, `softEdge`, `shadow` | `image.style` |
| `template` | `blank`, `sample`, `letter`, `resume`, `report` | `file.new` |
| `type` | `column`, `stackedColumn`, `percentColumn`, `bar`, `stackedBar`, `percentBar`, `line`, `lineMarkers`, `stackedLine`, `area`, `stackedArea`, `pie`, `doughnut`, `scatter`, `radar` | `chart.insert` |
| `valign` | `top`, `center`, `bottom` | `table.properties` |

## Examples

### Example 1: bold a word and add a sentence

The document has `Quarterly report` (block 0, Heading1), `Sales rose in every region.` (block 1)
and `The team thanks everyone involved.` (block 2).

```json
doc_run {
  "path": "report.docx",
  "steps": [
    { "command": "select.range", "params": { "anchor": { "block": 1, "off": 0 }, "focus": { "block": 1, "off": 5 } } },
    { "command": "format.bold", "params": {} },
    { "command": "select.range", "params": { "anchor": { "block": 2, "off": 34 }, "focus": { "block": 2, "off": 34 } } },
    { "command": "text.insert", "params": { "text": " Next quarter's targets follow." } }
  ]
}
```

It answers `Ran 4 commands on report.docx (saved)`. `doc_inspect` then shows block 2 as
`The team thanks everyone involved. Next quarter's targets follow.` and `doc_check` answers
`the kit found nothing wrong`.

### Example 2: a table after a paragraph, with a header row

```json
doc_run {
  "path": "report.docx",
  "steps": [
    { "command": "select.range", "params": { "anchor": { "block": 2, "off": 34 }, "focus": { "block": 2, "off": 34 } } },
    { "command": "text.newParagraph", "params": {} },
    { "command": "table.insert", "params": { "rows": 2, "cols": 2 } },
    { "command": "select.range", "params": { "anchor": { "story": "body", "path": [3, 0, 0, 0], "off": 0 }, "focus": { "story": "body", "path": [3, 0, 0, 0], "off": 0 } } },
    { "command": "text.insert", "params": { "text": "Region" } },
    { "command": "select.range", "params": { "anchor": { "story": "body", "path": [3, 0, 1, 0], "off": 0 }, "focus": { "story": "body", "path": [3, 0, 1, 0], "off": 0 } } },
    { "command": "text.insert", "params": { "text": "Sales" } }
  ]
}
```

`table.insert` answers `{ "table": [3] }`: the table is block 3, and the empty paragraph the caret
was in follows it as block 4. Cell paths are `[3, row, cell, 0]`. Afterwards `doc_check` may report
`doc.font-substituted` (info) for the new cells' theme font; that is expected.

## Workflow

1. `doc_inspect` the document: blocks, their indexes and texts, the page, the word count, and a
   preview.
2. Find the command: `doc_commands { "query": "bold" }`. Use two or three specific words
   (`insert table`, `heading style`); a broad query answers the first 12 of many. Each result has its
   parameter schema; follow it exactly and don't invent ids.
3. Run small batches with `doc_run { "steps": [...] }`: select, then act. A batch saves when it
   succeeds.
4. `doc_check`, then fix what it reports. Each diagnostic has a `code`, a `target` (page and
   `path`, e.g. `body:3/0/0/0`) and often a `fix` (the command to run).
5. `doc_inspect` again to confirm the text, and look at the preview.

## Pitfalls

- **New paragraphs:** `text.insert` with `"\n"` puts a line-break character inside the paragraph.
  It does not make a new paragraph. Use `text.newParagraph` at the caret.
- **Offsets after an edit:** inserting text moves every later offset in that paragraph. Use the
  `result.anchor` the step answered, or `doc_inspect` again, rather than recomputing by hand.
- **A failed step:** `doc_run` stops at the first failing step (`error.index`) and undoes the steps
  before it, so the document is as it was before the call and nothing is saved. Fix the failing step
  and send the whole run again.
- **Styles vs. direct formatting:** for a heading, apply the style (search `heading style`) rather
  than bold and a larger size; tables of contents and navigation read the style.
- **Fonts:** `doc.font-substituted` (info) means a font, often the theme's Aptos, isn't installed
  here. The file keeps the font; only the preview and line breaks use a substitute. Don't change
  fonts to silence it unless asked.
- **Tracked changes:** if `settings.trackChanges` is on, your edits become tracked revisions;
  `doc.tracked-changes` reports them. Its fix `change.acceptAll` accepts every revision, so run it
  only when the person asked for that.
