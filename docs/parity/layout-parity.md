# Layout and pagination parity with Microsoft Word

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft Word (Microsoft 365) for Mac 16.113.4

A document only "opens correctly" if its pages break where Word breaks them: the same lines, the
same page count, footnotes and pictures on the same pages. This checklist covers line breaking,
pagination and object placement in `crates/engine/layout`. Typing behaviour is in
[`typing-parity.md`](typing-parity.md); the reading and writing of the properties is in
[`file-format-parity.md`](file-format-parity.md).

**Dimension: ~55% (estimated), 90–145 h to full** (the sum of the rows below). The engine implements most of Word's layout
rules. WPS references support automated page comparisons, while Word itself has not been
verified page-for-page on these files. The default font differs from Word's metrics; the
readiness estimate above is not calculated from the WPS scores.

## Checklist

✅ matches Word as far as our tests show · 🟡 implemented, differs or untested against Word · ❌ missing

| Feature | State | Evidence / gap | Hours |
|---|---|---|---|
| Line breaking (UAX #14 with Word's exceptions: no break after `/`) | ✅ | `layout::tests::no_line_break_right_after_a_slash` | — |
| Justification, Word 2013+ space shrinking (compat mode 15) | ✅ | — | — |
| Kashida justification (Arabic) | ❌ | no kashida in `crates/engine/layout` | 3–5 |
| Distributed / Thai distributed alignment | 🟡 | read; layout untested | 1–2 |
| Default font metrics (Aptos, Calibri, Cambria…) | 🟡 | Carlito/Caladea/Liberation are metric-compatible for Calibri/Cambria/Arial/Times/Courier; **Aptos (Word's default since 2024) has no metric-compatible open substitute**, so new Word documents wrap differently ([typing-parity.md](typing-parity.md) Known gaps). Needs a maintainer decision (commission or license a metric-matched font) | 6–10 + maintainers |
| Line spacing (single, 1.5, double, at least, exactly, multiple), spacing before/after, contextual spacing | 🟡 | Adjacent paragraph constraints overlap by maximum in body and boxed stories, with independent contextual flags. Nonzero inherited line units override points; automatic HTML spacing overrides both with 14 pt, suppressed at starts, cell ends and adjacent items of the same list. Fixed automatic spacing resolves to 5 pt before and 10 pt after when requested. Point edits clear competing constraints. Legacy interactions, complex lists and multiple-spacing baselines remain incomplete. | 3–5 |
| East Asian font selection | 🟡 | Inherited East Asian hints route ambiguous symbols and Chinese-language Latin ranges; covered glyphs retain the requested face and missing glyphs use regional fallbacks. Font-table charsets guide missing-font substitution. Theme-only references use document theme languages and regional defaults for Latin, East Asian and complex-script slots. Browser fonts differ; unlisted locales, cached-name priority, theme edits, named charsets, Panose matching and the Times New Roman slot exception remain incomplete. | 2–4 |
| Character width (`w:w`) | 🟡 | Glyph outlines share the horizontal scale used for character advances. Native raster, PDF text, fallback outlines and warped text apply it around placed glyph origins; PDF links include the scaled final advance. Four regressions cover compressed/expanded ink, unchanged height, independent spacing, PDF transforms and selectable text. Combined East Asian runs remain incomplete. | 1–2 |
| Manual run width (`w:fitText`) | 🟡 | Import/save retains point widths and signed group IDs. Contiguous IDs share one width across formatting and fallback faces; ordinary anonymous runs remain independent through three DOCX saves. Shaped glyphs, advances and editable character ranges share the group geometry. A group overflows intact at line start, moves whole after preceding text, and avoids internal justification, compression and automatic spacing. Explicit controls/objects, inherited anonymous boundaries and anonymous replica boundaries remain incomplete. See `crates/engine/engine/tests/fit_text.rs` and `crates/kdm/kdm-model/tests/fit_text.rs`. | 2–3 |
| Two lines in one (`w:eastAsianLayout/@w:combine`) | 🟡 | Contiguous IDs share two half-size sub-rows within one logical line, preserving fallback faces, formatting and source grapheme ranges. Sub-row glyphs, caret bands, mouse hits, selection, arrows and highlights share geometry; group widths resist internal line breaking and distribution. Settings, signed IDs, explicit false values and bracket kinds survive repeated DOCX saves and replica marks. Bracket rendering, vertical/bidirectional groups, controls/objects, simultaneous manual widths and inherited anonymous/replica boundaries remain incomplete. See `crates/engine/engine/tests/two_lines.rs`. | 3–5 |
| Automatic East Asian/Latin and East Asian/numeric spacing | 🟡 | Independent paragraph switches add quarter-em spacing across formatting runs and participate in wrapping, with no leading/trailing gap at a wrap or duplicate explicit space. Numeric symbols honor their resolved East Asian font slot. Field boundaries and historical spacing flags remain incomplete. | 1–2 |
| Text effects (`w14:textOutline`, `w14:textFill`) | 🟡 | Text with a visible outline is drawn as outlined letters (filled and stroked, also in bent art text), as WPS prints it (`issue-text-outline-read`, `issue-text-outline-draw`, `issue-text-outline-warp`); letters with a blurred shadow, a glow or a reflection are drawn as ink, not text, as WPS prints them as a picture (`issue-text-blur-read`, `issue-text-blur-draw`); colour modifiers, gradients past their first stop, and the shadow, glow, reflection and 3-D effects themselves are not drawn | 3–5 |
| Full-width punctuation compression | 🟡 | Enabled `characterSpacingControl` lets commas, stops, colons, semicolons and exclamation/question marks absorb bounded overflow before wrapping, a line's marks giving up at most half the largest mark's size in all, as WPS compresses them (`issue-punctuation-squeeze`); a closing comma or full stop ending a full line hangs past the right margin in Chinese, Japanese or Korean text (`w:overflowPunct` and `w:lang/@w:eastAsia`, `issue-punctuation-hang`, `issue-punctuation-hang-language`); glyph advances stay intact. Changing the setting invalidates cached paragraphs. Opening brackets, quotes, kana and historical flags still need validation. | 1–2 |
| Widow/orphan control, keep with next, keep lines together, page break before | 🟡 | Keep-with-next reserves fitting paragraph chains, labelled paragraphs and the actual first table row or splittable fragment. Isolated preview counters and context-keyed table measurements preserve numbering. Fitting floats consume no reserved flow height. Oversized first rows, table-row keep chains and floating exclusions still need reference validation. | 1–2 |
| Tabs (left, center, right, decimal, bar), leaders | 🟡 | Field-result tabs use paragraph stops and preserve font runs and atomic editing ranges. Decimal finalization keeps the point at the stop, and leaders carry their drawn Unicode text into PDF/export metadata. Ten focused regressions cover left/center/right/decimal stops, wraps, font changes and ligatures; bar tabs remain untested. | 1–2 |
| Hyphenation (automatic, 0.25" zone, soft hyphens, consecutive limit) | 🟡 | English patterns only; other languages hyphenate wrongly or not at all | in localization |
| Drop caps | ✅ | — | — |
| Columns, column breaks, separators | 🟡 | Columns before a continuous section break are balanced: the section is placed again with the lowest column bottom that keeps it on its page without splitting kept-together paragraphs or breaking widow control, and the next section starts below the tallest column (`issue-column-balance`; WPS's `word-edaa6e5b8489` balances a three-line heading and one line as 3 + 1). Sections whose columns span pages are balanced only by normal flow. | 2–3 |
| Section breaks (next page, continuous, even, odd), different first page, odd/even headers | ✅ | Section breaks don't show in Draft view. The empty paragraph that only ends a section never makes a blank page of its own, and the blank page inserted before an even or odd page section shows no header or footer, a paragraph ending with a page break keeps its mark on the break line, and a paragraph holding only a page break stays at the foot of a full page, as WPS prints them (`issue-section-mark-overflow`, `issue-blank-page-header`, `issue-page-break-mark`, `issue-page-break-overflow`) | 1–2 |
| Body top below a tall header, footer pushes body up | ✅ | — | — |
| Footnotes at the page bottom, endnotes at the end, note numbering | 🟡 | A long note continues onto the next page at a line boundary (its first line stays with the reference), first in that page's note area under a full-width continuation separator; the document's own `w:continuationSeparator`/`w:continuationNotice` aren't read (default line, no notice); per-section restart partial | 2–4 |
| Tables: row heights (at least/exact), rows split across pages, header rows repeat, Can't Split | ✅ | A split row keeps every cell's first line above the cut or moves on whole, as WPS (`issue-table-row-split`) | — |
| Tables: autofit to contents/window, fixed widths, Word 2013 edge | 🟡 | AutoFit Contents measures each column's narrowest and widest text and shares the width like Word, once (not live as you type); cell preferred widths and spans still differ | 3–5 |
| Floating tables (`w:tblpPr`) | 🟡 | Overlap rules untested | 2–3 |
| Nested tables | 🟡 | render; polish missing | 2–4 |
| Floating pictures/shapes placement (relative to page/margin/column/paragraph/line/character) | ✅ | Behind/in-front paragraph anchors follow the final top after line-unit spacing (`issue-line-unit-float-anchor`); a text box that fits its text (`wrap="none"` with `a:spAutoFit`) is placed at its text's size (`issue-fit-text-box`), and one that doesn't wrap without autofit keeps its size with each line whole (`issue-no-wrap-text-box`); on a line grid a line wraps wherever its rows meet the object, as WPS for Mac sets it (`issue-grid-wrap-text-band`); offsets from the paragraph count from above its space before (`issue-float-paragraph-origin`); a float reaching above its paragraph wraps the text it meets there (`issue-float-reach-up`). | — |
| Text wrap: square, top-and-bottom, behind, in front | ✅ | — | — |
| Pictures cut to a shape (`pic:spPr/a:prstGeom`) | 🟡 | A picture cut to a preset outline is drawn cut to it on screen, in page images and in PDF, and keeps it through saves (`issue-picture-shape-read`, `issue-picture-shape-draw`); its line, blurred shadow and glow are drawn and saved (`issue-picture-edge-read`, `issue-picture-edge-draw`), a picture that may have see-through pixels casting no shadow from its frame (`issue-transparent-picture-shadow`); vector pictures and custom outlines are not cut, soft edges, reflections and 3-D rotation are not drawn | 1–2 |
| Groups with blurred effects | 🟡 | A group holding a blurred shadow, a glow or soft edges prints its text boxes' letters as ink, not text, as WPS prints such a group as one picture (`issue-blurred-group-text`); the group is still drawn as vectors | 1–2 |
| Text wrap: tight, through (contour, wrap polygon) | 🟡 | Text keeps clear of an unturned chart's or diagram's wrap polygon bounds (read from and saved as `wp:wrapPolygon`), and of a picture's or shape's whole frame, as WPS lays them out (`issue-wrap-polygon`); wrapping along the polygon's outline is not done | 3–5 |
| Text boxes, overflow, linked text boxes | 🟡 | Linked text boxes missing | 3–5 |
| Rotated objects and text | 🟡 | Pictures, shapes, charts and groups drawn turned and flipped about their centre (screen and PDF); square wrap and inline lines keep clear of the rotated bounds; hit testing on the turned shape. Text in a rotated text box stays upright | 2–3 |
| Page borders (from page edge or text), page colour, watermark | ✅ | | — |
| Line numbers (restart per page/section, count by) | ✅ | | — |
| Vertical page alignment | ✅ | | — |
| Gutter, mirror margins, book fold, 2 pages per sheet | 🟡 | gutter and mirror margins; book fold missing | 2–3 |
| Document grid (`w:docGrid`, lines per page, characters per line) | 🟡 | Line-grid metadata survives round trips and controls print pagination; exact spacing, paragraph opt-out, table exceptions and inline pictures are covered by tests; a line holding an inline picture or diagram takes whole grid rows like a text line, as WPS lays it out (`issue-inline-picture-grid`), a line of only pictures, charts or diagrams leaving out the descent below them and a line with text beside them counting it (`issue-picture-grid-descent`). Multiples cap the natural line unit at the grid pitch before scaling, retaining unscaled baseline metrics. A line at a multiple whose text is taller than one row is the multiple of the pitch tall, not rounded to whole rows (`issue-grid-tall-multiple`). Character spacing is retained but not shaped; whole-document baselines and pagination still differ from WPS. See [WPS layout testing](../features/wps-layout-testing.md). | 2–4 |
| Vertical text (`tbRl`) in sections and text boxes; cell text direction | 🟡 | cell text direction is done; page-level and text-box vertical text missing | 8–12 |
| Ruby / phonetic guide, enclose characters, combined characters | ❌ | | 4–6 |
| Chinese list numbering (`chineseCounting`, `chineseCountingThousand`, `chineseLegalSimplified`, `decimalEnclosedCircle`, `decimalEnclosedCircleChinese`) | 🟡 | Labels use Chinese counting numerals (no leading one for ten to nineteen, a single zero between places), legal numerals (always with the leading one) or circled numbers up to twenty (digits beyond), and the formats survive saves (`issue-chinese-numbering`). WPS references confirm values up to three; larger forms are unconfirmed. Other unsupported formats still fall back to decimal. | 1–2 |
| Missing Latin font fallback | 🟡 | A missing Latin family with no installed metric-compatible open substitute is drawn in PingFang SC, as WPS for Mac draws it, except Calibri and Aptos, drawn in Helvetica Neue after Carlito (`issue-missing-latin-font`). See `crates/support/fonts/src/word.rs`. | 1 |
| East Asian single line height | 🟡 | A font that declares an East Asian code page (OS/2 code pages 932, 936, 949, 950, 1361) has 1.3 times its Windows ascent plus descent as single line spacing, the extra split evenly above and below, as measured for Word ([ONLYOFFICE issue 487](https://github.com/ONLYOFFICE/DocumentServer/issues/487)). Across the paired WPS corpus, PingFang lines at 9–20pt with 0.8–1.5 multiples match WPS's pitch within 0.2% (299 line pairs at 10.5pt), and on 15.6pt grids 10.5pt text takes two rows as in WPS. 45 of 63 documents match WPS's page count. Fallback faces inside Latin runs and 8pt table text differ; a 2.4-multiple grid heading matches (`issue-grid-tall-multiple`). See `crates/support/fonts/src/tests_line_metrics.rs`. | 1–2 |
| `EQ` fields (combined characters, `\o`, `\s`, `\l`, `\f`, `\b`, `\x`, `\d`, `\a`, `\r`, `\i`) | 🟡 | Uncached `EQ` fields are drawn from their codes as one editing unit, each character in its own code run's size and font; escapes and comma or semicolon separators are read, and general switches (`\*`) are not drawn. Phonetic Guide fields size and set the font of their guide text from `\* hpsN` and `\* "Font:name"` (pinyin over a paired WPS sample: 9pt guides centred over 18pt bases, raised 17pt). Overstrike alignment, `\s` raise/lower (default 2pt) and the field run's own line metrics match the paired WPS combined-characters sample (pieces 18pt wide at the margin, 8pt apart). Fractions, brackets, boxes, displacement, arrays, radicals and integrals use spec geometry not yet compared with a reference; `\o\ad`, `\* jcN` alignments other than centred, `\s` stacks and nested non-`EQ` fields are incomplete. One WPS justified line breaks before adjacent Phonetic Guide fields that would still fit (Kition keeps one more); the line-break rule around `EQ` fields is not yet confirmed. See `crates/engine/engine/tests/eq_fields.rs`. | 2–4 |
| Bidirectional paragraphs, mixed-direction lines | 🟡 | RTL sections and tables (`w:bidiVisual`) missing | 4–6 |
| Equations: display layout, numbering, wrapping long display equations | ✅ | Long display equations break at top-level operators (`m:brkBin`, `m:brkBinSub`), manual breaks and `m:alnAt`, continuation lines indented by `m:wrapIndent` or set right (`m:wrapRight`); normal text in an equation draws letters its font lacks (Chinese in a placeholder) from a face that has them (`issue-math-text-fallback`) | — |
| Hidden text excluded from breaking and hyphenation | ✅ | — | — |
| Fields: PAGE, NUMPAGES, SECTIONPAGES, TOC page numbers | ✅ | — | — |
| Compatibility modes (Word 2003/2007/2010 layout for old files: `w:compat` options) | 🟡 | mode 15 behaviour; the ~60 legacy compat options are mostly ignored | 10–20 |
| Page-by-page comparison harness against Word | 🟡 | Local WPS acceptance requires identical page counts, exact per-page text allocation including page numbers, and no large layout differences on any page (content occupancy and coarse tone, calibrated so font rendering never decides a page). Sampled runs cannot pass whole documents. This target is not yet achieved and is not verified Word parity. See [WPS layout testing](../features/wps-layout-testing.md). | 4–6 |

### Empty columns and leading paragraph spacing

Leading spacing does not make an empty column appear occupied during first-line overflow and
widow checks. Oversized first lines stay on their starting page, with explicit spacing preserved;
content after earlier paragraphs advances when necessary. Three synthetic regressions cover these
conditions. The complete WPS corpus acceptance does not pass yet.

## Performance

| Measure | Kition Doc | Word | Kind |
|---|---|---|---|
| Cold layout, 188-page document | 61 ms | not measured | measured |
| Relayout after an edit | 1.4 ms | not measured | measured |
| Font picker with many installed fonts | no freeze | instant | tested |
| Large real-world documents (500+ pages, many pictures) | not measured | | gap |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
