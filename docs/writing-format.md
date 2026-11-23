# Doc writing format

How a model writes Kition Doc documents: the steps it sends, how it names what it changes,
what a new document starts with, the named values commands accept, and worked examples.
The package ships the short version for models as [agent/guide.md](../agent/guide.md); this file is
the complete reference it is drawn from. Everything below comes from the engine itself
(its 487 commands and their parameter schemas, and what `inspect` reports) and from the guide.

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
Commands by namespace (the part of the id before the first dot):

| Namespace | Commands | Change the document | Open to the agent | Examples |
| --- | ---: | ---: | ---: | --- |
| `arrange` | 12 | 11 | 12 | `arrange.rotate`, `arrange.rotation`, `arrange.wrap` |
| `caret` | 16 | 0 | 16 | `caret.left`, `caret.right`, `caret.up` |
| `comment` | 6 | 4 | 6 | `comment.threads`, `comment.add`, `comment.reply` |
| `design` | 11 | 9 | 11 | `design.theme`, `design.themeFonts`, `design.themeColors` |
| `document` | 6 | 1 | 6 | `document.inspect`, `document.text`, `document.paragraph` |
| `draw` | 9 | 2 | 9 | `draw.select`, `draw.shape`, `draw.eraser` |
| `drawing` | 14 | 13 | 14 | `drawing.describe`, `drawing.insert`, `drawing.delete` |
| `edit` | 25 | 12 | 25 | `edit.undo`, `edit.redo`, `edit.cut` |
| `equation` | 20 | 12 | 20 | `equation.edit`, `equation.exit`, `equation.type` |
| `file` | 23 | 12 | 23 | `file.new`, `file.open`, `file.save` |
| `format` | 35 | 33 | 35 | `format.bold`, `format.italic`, `format.underline` |
| `hf` | 3 | 1 | 3 | `hf.next`, `hf.previous`, `hf.position` |
| `insert` | 34 | 33 | 34 | `insert.pageBreak`, `insert.blankPage`, `insert.coverPage` |
| `layout` | 14 | 13 | 14 | `layout.margins`, `layout.orientation`, `layout.size` |
| `list` | 2 | 1 | 2 | `list.define`, `list.get` |
| `mailings` | 17 | 11 | 17 | `mailings.start`, `mailings.recipients`, `mailings.editRecipients` |
| `para` | 47 | 46 | 47 | `para.alignLeft`, `para.alignCenter`, `para.alignRight` |
| `picture` | 13 | 13 | 13 | `picture.size`, `picture.crop`, `picture.altText` |
| `presence` | 2 | 0 | 2 | `presence.caret`, `presence.locate` |
| `readAloud` | 7 | 0 | 7 | `readAloud.playPause`, `readAloud.next`, `readAloud.previous` |
| `references` | 23 | 20 | 23 | `references.toc`, `references.updateToc`, `references.removeToc` |
| `review` | 35 | 15 | 35 | `review.newComment`, `review.reply`, `review.editComment` |
| `select` | 13 | 0 | 13 | `select.all`, `select.range`, `select.word` |
| `shape` | 4 | 4 | 4 | `shape.fill`, `shape.outline`, `shape.effects` |
| `styles` | 13 | 7 | 13 | `styles.create`, `styles.modify`, `styles.updateToMatch` |
| `table` | 40 | 36 | 40 | `table.viewGridlines`, `table.insertRowAbove`, `table.insertRowBelow` |
| `text` | 14 | 14 | 14 | `text.insert`, `text.newParagraph`, `text.lineBreak` |
| `tools` | 3 | 0 | 3 | `tools.recordMacro`, `tools.macros`, `tools.autocorrect` |
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
| `align` | `left`, `center`, `right` | `insert.pageNumber`, `table.properties` |
| `direction` | `right`, `left`, `flipVertical`, `flipHorizontal` | `arrange.rotate` |
| `effect` | `blur`, `sharpen`, `invert`, `posterize`, `pixelate` | `picture.effects` |
| `footnoteFormat` | `decimal`, `lowerRoman`, `upperRoman`, `lowerLetter`, `upperLetter` | `references.noteOptions` |
| `format` | `unicode`, `latex` | `equation.inputFormat` |
| `format` | `decimal`, `lowerRoman`, `upperRoman`, `lowerLetter`, `upperLetter` | `layout.pageNumberFormat` |
| `kind` | `numbered`, `numberedParen`, `upperLetter`, `lowerLetter`, `lowerRoman`, `outline` | `para.numbering` |
| `kind` | `legal`, `outline` | `para.multilevel` |
| `kind` | `bottom`, `top`, `left`, `right`, `none`, `all`, `outside`, `inside`, `horizontalLine` | `para.borders` |
| `kind` | `rectangle`, `roundedRectangle`, `ellipse`, `triangle`, `diamond`, `line`, `arrow`, `star`, `heart` | `insert.shape` |
| `kind` | `page`, `column`, `textWrapping`, `nextPage`, `continuous`, `evenPage`, `oddPage` | `layout.break` |
| `kind` | `all`, `outside`, `inside`, `none`, `top`, `bottom`, `left`, `right` | `table.borders` |
| `kind` | `calendar`, `tabular`, `matrix` | `table.quick` |
| `kind` | `box`, `none` | `design.pageBorders` |
| `kind` | `letters`, `emails`, `envelopes`, `labels`, `directory`, `normal` | `mailings.start` |
| `kind` | `shape`, `textBox`, `chart` | `drawing.insert` |
| `mode` | `sentence`, `lower`, `upper`, `title`, `toggle` | `format.changeCase` |
| `mode` | `contents`, `window`, `fixed` | `table.autofit` |
| `mode` | `grayscale`, `sepia`, `washout`, `blackAndWhite`, `saturation`, `tint` | `picture.color` |
| `mode` | `none`, `readOnly`, `comments`, `trackedChanges`, `forms` | `review.restrict` |
| `mode` | `none`, `readOnly`, `comments`, `trackedChanges` | `file.protect` |
| `preset` | `blank`, `blankThree`, `title` | `insert.header` |
| `preset` | `blank`, `blankThree`, `pageNumber` | `insert.footer` |
| `preset` | `normal`, `narrow`, `moderate`, `wide`, `mirrored`, `office2003` | `layout.margins` |
| `preset` | `left`, `right` | `layout.columns` |
| `preset` | `topLeft`, `topCenter`, `topRight`, `middleLeft`, `middleCenter`, `middleRight`, `bottomLeft`, `bottomCenter`, `bottomRight` | `arrange.position` |
| `preset` | `straightConnector1`, `bentConnector3`, `curvedConnector3` | `drawing.connect` |
| `style` | `single`, `double`, `dotted`, `dashed`, `thick`, `triple`, `dotDash`, `wave` | `design.pageBorders`, `para.borders` |
| `style` | `single`, `double`, `thick`, `dotted`, `dash`, `dotDash`, `dotDotDash`, `wave`, `words` | `format.underline` |
| `style` | `APA`, `MLA`, `Chicago`, `IEEE` | `references.citationStyle` |
| `style` | `simpleFrame`, `thickFrame`, `rounded`, `softEdge`, `shadow` | `picture.style` |
| `template` | `blank`, `sample`, `letter`, `resume`, `report` | `file.new` |
| `type` | `column`, `stackedColumn`, `percentColumn`, `bar`, `stackedBar`, `percentBar`, `line`, `lineMarkers`, `stackedLine`, `area`, `stackedArea`, `pie`, `doughnut`, `scatter`, `radar` | `insert.chart` |
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
    { "command": "insert.table", "params": { "rows": 2, "cols": 2 } },
    { "command": "select.range", "params": { "anchor": { "story": "body", "path": [3, 0, 0, 0], "off": 0 }, "focus": { "story": "body", "path": [3, 0, 0, 0], "off": 0 } } },
    { "command": "text.insert", "params": { "text": "Region" } },
    { "command": "select.range", "params": { "anchor": { "story": "body", "path": [3, 0, 1, 0], "off": 0 }, "focus": { "story": "body", "path": [3, 0, 1, 0], "off": 0 } } },
    { "command": "text.insert", "params": { "text": "Sales" } }
  ]
}
```

`insert.table` answers `{ "table": [3] }`: the table is block 3, and the empty paragraph the caret
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
  `doc.tracked-changes` reports them. Its fix `review.acceptAll` accepts every revision, so run it
  only when the person asked for that.
