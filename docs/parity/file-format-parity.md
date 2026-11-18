# File-format parity with Microsoft Word

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft Word (Microsoft 365) for Mac 16.113.4, plus Word for Windows' extra formats

Every format Word opens or saves, what Kition Doc does with it, and how it's tested. Fidelity is the
share of a typical real-world file's content and formatting that survives (read: shows correctly;
write: Word opens it showing the same). Layout and pagination fidelity is in
[`layout-parity.md`](layout-parity.md); typing behaviour is in [`typing-parity.md`](typing-parity.md).

**Dimension: ~60% (estimated), 185–300 h to full** (the sum of the rows below). The weighted table
gives ~64%; we discount it to ~60% because DOCX fidelity is not yet checked on real-world
files. DOCX dominates the weight: a Word user sends and receives .docx; everything else is
occasional.

## Formats

Weights are the share of a Word user's file traffic (estimate). Read/write: ✅ yes, 🟡 partial,
❌ no, — Word doesn't either.

| Format | Weight | Word | Kition Doc read | Kition Doc write | Fidelity | Hours | How it's tested | Notes |
|---|---|---|---|---|---|---|---|---|
| `.docx` Word Document (ECMA-376 Transitional) | 70% | R/W | ✅ | ✅ | ~65% | 70–110 | `crates/formats/docx/tests` (roundtrip 34, fixtures 29, malformed 8, bidi 4, citations 5); synthetic fixtures only | Word opens our files. Untested on a real-world corpus. Details below |
| `.docx` Strict Open XML | 1% | R/W | ✅ (namespace mapping in `xml.rs`) | ❌ | ~60% | 2–4 | one fixture | Writing Strict is rare |
| `.docm` / `.dotx` / `.dotm` | 3% | R/W | ✅ | ✅ | ~65% | incl. above | `macro_packages.rs` | Macros and signatures kept on save; macros never run |
| `.doc` Word 97-2003 (and `.dot`) | 8% | R/W | 🟡 | ❌ | ~55% | 25–40 | `crates/formats/docbin/src/tests.rs` (50) | Spec-based reader ([MS-DOC]): text, formatting, styles, sections, headers/footers, tables, lists, notes, fields, bookmarks, pictures. Metafile (WMF/EMF) pictures, Word 6/95 files and encrypted files are refused or dropped. No writer |
| PDF export | 8% | W | — | ✅ | ~80% | 6–10 | `crates/formats/pdf/src/tests.rs` | krilla: embedded subsets, selectable text incl. field results, links, outline, tagging. Fonts it can't subset become outlines. No PDF/A option, no "best for printing/online" choice |
| PDF open (PDF Reflow, Word for Windows) | 1% | R | ❌ | — | 0% | 30–50 | — | Converting a PDF into an editable document. A sibling app (Kition PDF) may supply the parser |
| `.rtf` Rich Text Format | 3% | R/W | 🟡 | 🟡 | ~50% | 10–15 | `crates/formats/formats/src/tests.rs` | ~1,500 lines: text, character/paragraph formatting, styles, lists, tables, pictures (`\shppict`), fields, bookmarks. Thin: comments, notes, revisions, sections, RTL |
| `.odt` OpenDocument Text | 2% | R/W | 🟡 | 🟡 | ~50% | 8–12 | `crates/formats/formats/src/tests.rs` | Styles, lists, tables, pictures, headers/footers, page breaks. No footnotes, comments, tracked changes, fields or RTL |
| `.txt` Plain text | 2% | R/W | ✅ | ✅ | ~95% | 1–2 | unit tests | Encoding detection; Word's encoding choice dialog and line-ending options missing |
| `.htm/.html` Web Page and Web Page, Filtered | 1% | R/W | 🟡 | 🟡 | ~60% | 6–10 | `crates/formats/formats/src/tests.rs` | Tables, relative pictures embedded, missing `</head>` tolerated. No Word "Web Page" (with `_files` folder) round-trip |
| `.mht/.mhtml` Single File Web Page | <1% | R/W | ❌ | ❌ | 0% | 3–5 | — | |
| `.xml` Word XML Document (Flat OPC) | <1% | R/W | ❌ | ❌ | 0% | 3–5 | — | The DOCX parts in one XML file; cheap once the DOCX code is reused |
| `.xml` Word 2003 XML (WordprocessingML 2003) | <1% | R/W | ❌ | ❌ | 0% | 5–8 | — | Legacy |
| `.wps` Works 6–9, WordPerfect `.wpd` (Windows) | <1% | R | ❌ | — | 0% | 10–20 | — | Windows-only converters; low value |
| Encrypted/password-protected `.docx`/`.doc` | 1% | R/W | ❌ | ❌ | 0% | 6–10 | — | ECMA-376 Agile Encryption; `.doc` RC4/XOR detected and refused (`docbin` fib) |
| `.md` Markdown | — | (no) | ✅ | ✅ | — | — | `crates/formats/formats/src/tests.rs` | Beyond Word |
| `.tex` LaTeX | — | — | ✅ | ✅ | — | — | `crates/formats/formats/src/tests.rs` | Beyond Word, incl. math |
| `.png` page images | — | (Save as picture, Mac) | — | ✅ | — | — | `io_ext.rs` | |
| `.json` document model | — | — | ✅ | ✅ | — | — | engine tests | For agents |

## DOCX in depth

Measured by grepping `crates/formats/docx/src/read` and `src/write` for the OOXML elements.

| Feature | Read | Write | Notes |
|---|---|---|---|
| Text, runs, paragraph and character properties, styles, numbering, sections | ✅ | ✅ | Including complex-script props (`w:rtl`, `w:cs`, `w:szCs`, `w:bCs`…), East Asian fonts kept apart from Latin |
| Tables incl. floating (`w:tblpPr`), table styles and conditional formatting | ✅ | ✅ | Custom table styles round-trip |
| Headers/footers (first, even/odd), page borders, line numbers, gutter, mirror margins | ✅ | ✅ | |
| Footnotes, endnotes, comments | ✅ | ✅ | `commentsExtended` written; `commentsIds`, modern threaded comments (`w16cex`) partly |
| Tracked insertions and deletions (`w:ins`/`w:del`), paragraph-mark revisions | ✅ | ✅ | — |
| Formatting revisions (`w:rPrChange`, `w:pPrChange`, `w:sectPrChange`, `w:tblPrChange`, `w:trPrChange`, `w:tcPrChange`, `w:numberingChange`) | ✅ | ✅ | Kept on open and save, schema order (change element last); changes inside `styles.xml` are dropped. RTF/ODT ignore them |
| Move tracking (`w:moveFrom`/`w:moveTo`) | 🟡 | ❌ | Read as plain insert/delete |
| Fields (`w:fldChar`, `w:fldSimple`), TOC, cross-references, `ADDIN` citations | ✅ | ✅ | Field codes Word supports but we don't evaluate keep their cached result. Each `w:instrText` run's formatting survives repeated saves and replicas (`issue-eq-field-code`) |
| Content controls (`w:sdt`) | 🟡 | ❌ | Content kept, the control (type, binding, placeholder, lock) dropped |
| Legacy form fields (`w:ffData`), check boxes, drop-downs | ❌ | ❌ | |
| DrawingML pictures inline and anchored (`wp:anchor`), wrap square/tight/through/top-bottom | ✅ | ✅ | Tight/through read and written (their wrap polygon's bounds kept; charts and diagrams wrap to it); colour adjustments (`a:lum` brightness and contrast, washout, `a:grayscl`, `a:biLevel`, `a:alphaModFix`) drawn and saved (`issue-picture-adjust-read`, `issue-picture-adjust-draw`) |
| Group shapes (`wpg:`), drawing canvas | 🟡 | 🟡 | Group shapes read and written; drawing canvas (`wpc:`) dropped |
| DrawingML shapes and text boxes (`wps:`) | ✅ | ✅ | Preset geometries subset; rotation and flips (`a:xfrm` `rot`/`flipH`/`flipV`, pictures and groups too), the effect extent covering the rotated bounds |
| Freeform shapes (`a:custGeom`: `a:moveTo`, `a:lnTo`, quadratic/cubic Bezier controls) and Kition Doc ink | ✅ | ✅ | Ink is written as a freeform `wps:wsp` in a `wp:anchor` (round caps, alpha for highlighter) and recognised again by its drawing name. It is saved as a custom-geometry shape, not as Word's own ink (InkML in `w14:contentPart`), so Word shows Kition Doc ink as a freeform shape it can move and recolour but not erase with its ink eraser; arc commands are not retained; group members use the same custom-path reader, with repeated-save and nested-scale regressions (`issue-group-freeform`); independent fill, stroke, closure and compound contours survive saving (`issue-custom-path-paints`); Bezier controls survive repeated saves and use exact curve geometry for rendering and hit testing (`issue-custom-path-curves`); formula coordinates remain incomplete |
| VML (`w:pict`, `v:shape`, `v:textbox`, `v:textpath`) | 🟡 | ❌ | Pictures and text boxes, best effort; art text in VML text paths (text watermarks) is drawn as its string stretched to the shape in its fill colour at its opacity, turned with the shape, and saved back as the same VML with the edited string (`issue-vml-art-text`, `issue-art-text-turn`); a shape's line comes from its `v:stroke` element when its attributes don't give it (`issue-vml-stroke`) |
| Charts (`c:chart`), smart graphics (`dgm`) | ✅ | ✅ | Drawn from the chart's cached data and smart graphics' stored drawing, its shapes outlined by the preset catalog the kits share (turned shapes, connectors, rounded corners), translucent fills shown over the page, list-style sizes, indents and bullets applied, and empty placeholder nodes showing their prompt as outlines, not text, as WPS draws them (`issue-smart-graphic-drawing`, `issue-smart-graphic-prompt-outline`); not editable. Saved back as read, with every part they reach (chart, embedded workbook, colours and style; diagram data, layout, quick style, colours and drawing) and their content types; a moved or resized one keeps its new frame, and a pasted copy gets its own parts |
| OLE objects (`w:object`: embedded or linked files, ActiveX) | 🟡 | ✅ | Shown as their picture (VML or DrawingML); saved back whole with the embedding and picture parts, at the size, rotation and flips set in Kition Doc. Can't be opened or edited |
| Word ink (`w14:contentPart`, InkML), 3D models | ❌ | ❌ | Dropped on read, not preserved on save |
| Equations (OMML `m:oMath`) | ✅ | ✅ | Manual breaks (`m:brk`) and the settings' `m:mathPr` wrapping options |
| Themes, font table, settings, compatibility mode | ✅ | ✅ | Theme-only font references select language-specific or regional Latin/EA/CS faces, with references retained through saves. Explicit cached-name priority and theme-edit propagation remain partial. Font-table charsets guide missing-font fallback; untouched tables remain verbatim. Embedded font tables (`w:embedRegular`), named charsets and full Panose/family matching remain incomplete. |
| Custom XML parts, document properties, bibliography sources | 🟡 | 🟡 | Custom properties round-trip; bibliography sources pending |
| Ruby | ❌ | ❌ | East Asian layout |
| East Asian run settings (`w:eastAsianLayout`) | ✅ | ✅ | Signed group IDs, combine, bracket kinds, vertical and compression flags survive three DOCX saves, including explicit false. Basic two-lines-in-one layout is partial; bracket rendering, rotation, bidi and mixed-control combinations remain incomplete (`issue-two-lines-in-one`). |
| Manual run width (`w:fitText`) | 🟡 | 🟡 | Widths and signed IDs survive repeated DOCX saves; contiguous IDs share one layout width across formatting. Anonymous direct runs stay independent. Explicit controls/objects, inherited anonymous boundaries and anonymous replica boundaries remain incomplete (`issue-manual-run-width`). |
| Document grid (`w:docGrid`) | ✅ | ✅ | Section type, line pitch and character spacing survive DOCX save/reopen; layout uses line grids with paragraph opt-out and exact-spacing overrides (`document_grid.rs`). |
| Glossary document (building blocks), `w:altChunk`, sub-documents | ❌ | ❌ | |
| Hostile input | ✅ | — | `malformed.rs`, capped allocations, never-crash standard |

## What decides beta for formats

1. A **real-world DOCX corpus** opened, rendered and round-tripped, compared page by page with
   Word (local only: Word output can't be committed; keep a manifest and our own renders).
2. **Preserve what we can't render**: ink, group shapes and content controls survive a round trip
   (keep the parts and the run). Charts, smart graphics and OLE objects do.
3. Content controls read and written.
4. Encrypted `.docx` opens with a password.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
