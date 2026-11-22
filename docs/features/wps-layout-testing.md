# WPS layout comparison

The local `golden_word.zip` corpus supplies original DOCX files and immutable WPS
page images. The harness pairs originals with their own `_WORD_TO_PNG_WPS`
folders, excluding temporary files and PDF-to-Word reconversions. Original names
remain in the ignored corpus manifest; tracked cases and reports use ASCII case
IDs. No source documents or WPS references are published by these commands.

## Set up and run

From the repository root, using a Rust toolchain supported by the workspace:

```sh
python3 -m venv target/layout-venv
target/layout-venv/bin/python -m pip install -r tools/requirements-corpus.txt
cargo build -p xtask
target/layout-venv/bin/python tools/corpus_word.py inventory \
  --archive ../golden_word.zip --corpus corpus/golden-word
target/layout-venv/bin/python tools/corpus_word.py run \
  --corpus corpus/golden-word --renderer target/debug/xtask \
  --out target/wps-layout --pages 1000 --jobs 3 --check
```

`report.json` contains the measurements; `report.html` links each case's WPS,
Kition, and difference preview. The first two preview columns are the reference
and actual page; the third amplifies their pixel difference. Source and reference
checksums are checked before every run. A changed reference is an error, never a
new expected result. Inventory also refuses to overwrite modified corpus files.

The archive inventory has 115 original DOCX files. Of these, 63 have
paired WPS images (1,290 pages). The remaining originals have no image reference
and cannot receive an image-fidelity result. Encryption, invalid input and
timeouts are reported as errors, not successful or skipped cases.

Page counts cover the whole document on every run. Image measurements cover only
the first `--pages` pages of each document. Use `--pages 1000` to compare every
reference page in this corpus, or `--case word-<id>` to inspect a specific case.
The native renderer reports full physical page sizes and displayed page numbers
even when only some pages are rasterized. It also records drawn text, selected
font families, paragraph metrics, and footer spans. Comparisons preserve page
proportions when normalizing DPI. `--jobs` accepts 1 through 4 parallel documents
(default 1); results remain in manifest order. Every run uses a checksummed copy
of its renderer, so another build cannot replace the executable during a run.
The report also fingerprints this checkout before and after comparison, including
untracked source files and excluding ignored build output. It lists changed
relative paths if the checkout changes during measurement. These are observed
source snapshots, not proof of which source built the pinned binary; sibling
dependencies and installed fonts are outside their scope. The binary checksum
identifies the measured renderer independently.
Native sampling follows the reference PNG density only when all reference pages
agree on a square pixel density, that density matches their PDF paper dimensions
within raster rounding, and the renderer supports the requested scale. Otherwise
it uses 144 DPI. The renderer must report the requested scale in its layout
metadata; the per-document report records the actual scale and selection method.
The original PNG bytes and every acceptance threshold remain unchanged. Reports
with different native sampling methods do not have comparable image pass totals.
When a reference PDF is present, its physical dimensions and the native layout's
physical dimensions determine raster proportions. Consistent PNG pixel-density
metadata and the native renderer's known scale preserve content coordinates
without stretching integer-rounded page edges. Images without consistent density
metadata use proportional normalization. Physical paper-size errors remain hard
failures, and the original raster aspect error is still reported. Reports record
the normalization method; image scores from different methods are not equivalent.

## Detection and regression checks

The report records page-count differences, aspect-ratio differences, pixel RMSE,
changed-pixel fraction, physical page-size errors against the WPS PDF, ink bounds,
and an ink F1 score. The diagnostic ink score compares both
reference recall and actual precision within a two-pixel neighborhood at a
1,200-pixel comparison width. It exposes missing or shifted paragraphs even when
most of a page is white. It is a detection aid, not proof of identical typography.

The maintainers' acceptance target is **identical pagination,
identical page numbers and no large layout differences; font differences are
ignored**. Glyph shapes, stroke weight and anti-aliasing never decide a page:
WPS's own reference PDF, rasterized by an independent renderer with WPS's exact
layout and fonts, scores only 0.71–0.76 pixel similarity on text pages against
WPS's PNGs, so a pixel score cannot separate layout from font rendering.

A page is accepted when all of these hold:

- **Text allocation.** The reference PDF's text and the renderer's drawn text have
  the same NFC-normalized non-whitespace character multiset on that physical page,
  including page-number characters. This pins every line to the right page.
- **Content occupancy (`layout_f1` at least 0.98).** Ink is every pixel darker than
  180 in luminance, so dark and coloured text, rules and dark pictures count while
  light fills, highlights and watermarks do not. Both pages are divided into
  8-pixel cells (about 4 points at the 1,200-pixel comparison width); the F1 of
  cells holding ink, with one cell of tolerance, exposes missing, extra or
  displaced lines, paragraphs, pictures and tables.
- **Background tone (`tone_rmse` at most 0.03).** Pixels within 3 pixels of a glyph
  stroke in either page are ignored (solid ink areas of at least 5 by 5 cells stay
  in), then the mean colour of 48-pixel blocks (about 24 points) is compared over
  every block with content. This exposes missing or wrongly coloured backgrounds,
  pictures, watermarks and highlights without measuring stroke weight.
- **Paper.** Physical page-size error at most 0.05 point and physical aspect error
  below 0.0001.

The thresholds were calibrated, not chosen: WPS's reference PDFs rasterized by
Poppler at the PNG density (identical layout, different font rendering) have a
99th-percentile background tone RMSE of 0.010 on sampled pages, while Kition pages
without their watermark score 0.050–0.073, a page without its highlight 0.056, and
pages shifted by half a line a median occupancy F1 of 0.82. The pages where the
Poppler raster still fails are, on inspection, real differences between WPS's PDF
and PNG exports of the same document (border colours, dashed borders, missing math
glyphs, page scale) and, in one drop-cap document, glyphs of about 120 points.
Strict pixel similarity (`min(exact ink F1, 1 - foreground RMSE)`), full-page RMSE
and the neighborhood ink F1 stay in the report as diagnostics only.

`--check` returns 1 if any selected document has an error, incomplete reference,
uncompared reference pages, different page count, page text differences, or a page
outside the occupancy, tone or paper limits. A sampled run cannot certify a
complete document. Without `--check`, a completed measurement run returns 0 and
still records every failure. The corpus is expected to contain
differences; do not lower thresholds or update WPS files to make it green.

Keep separate output directories for before and after runs, then compare them:

```sh
target/layout-venv/bin/python tools/corpus_word.py diff \
  --before target/wps-baseline/report.json --after target/wps-layout/report.json \
  --out target/wps-layout/before-after.json
target/layout-venv/bin/python -m unittest discover -s tools -p test_corpus_word.py
cargo test -p kition-doc-engine --test document_grid
KITION_CORPUS_PYTHON=target/layout-venv/bin/python cargo xtask ci
```

Font substitution, differing source/reference revisions, paragraph spacing,
floating objects, and unsupported layout features can all cause differences.
Review affected pages before attributing every discrepancy to the engine. WPS
agreement measures WPS fidelity; it does not establish Microsoft Word parity.

Native and browser font availability are different. Native macOS discovery
includes the installed FontServices Reserved collection, including PingFang on
the measurement machine. The browser uses its self-contained kit fonts and
cannot inherit native font availability from this test. No proprietary fonts
are copied or bundled. A native score does not certify browser raster fidelity.

## Paragraph and font corrections

Adjacent paragraph before/after constraints overlap using their maximum,
with contextual suppression applied independently to each paragraph's side.
The rule covers body text and boxed stories, including table cells and headers.
It follows [ISO 29500 paragraph spacing](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.spacingbetweenlines).

East Asian text resolves its inherited `eastAsia` font independently of
Latin text. Regional installed fallbacks are selected before per-character
coverage fallback, so already loaded Japanese or Traditional Chinese
faces never supply Simplified Chinese glyphs. Theme script mappings and
ambiguous-character hints remain incomplete.

## Document grids

Section `docGrid` type, line pitch and character-pitch metadata survive DOCX
round trips. Line grids affect print layout; exact paragraph line spacing and
inherited `snapToGrid=false` take precedence. Table cells use the line grid only
with `adjustLineHeightInTable`; text boxes follow it too, as WPS for Mac spaces them.
Character-grid spacing is retained but is not yet applied during shaping.

The paragraph overrides and table exception follow the
[ECMA-376 line-pitch semantics](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.docgrid.linepitch).

## Text box overflow

A text box too short for its text keeps every line that starts inside it; the
last such line is drawn clipped at the box's edge, on screen and in PDF, as
WPS for Mac draws it. Lines that would start below the box are not shown.

## Content controls

WPS makes room for a content control's start and end marks on their lines as if
they were glyphs of the marks' size: 1.125 em above the baseline and 0.39 em
below. The size is `w:sz` in the control's `w:sdtPr` (start) or `w:sdtEndPr`
(end) run properties, or the paragraph style's when the control sets none. A
control around paragraphs marks its first and last paragraph; one around runs,
the line where it starts and the line where it ends. On a 15.6 pt line grid this
turns a 10.5 pt line into two rows. The marks are read for layout only; content
controls are not written back.

## Page breaks ending a paragraph

When nothing but zero-width marks (bookmarks, a content control's end) follows a
paragraph's last page break, they stay on the break's line; no empty line starts
the next page. The paragraph that then starts the next page drops its space
before, as a paragraph that text flowed onto the page does. A paragraph with
`w:pageBreakBefore` keeps its space before.

## Paragraph borders

A paragraph's top border takes its space and width above the first line, and its
bottom border the same below the last line. The room sits outside the space
before and after and outside the line grid's rows. Paragraphs in a row with the
same borders share them: the first gets the top border's room, the last the
bottom's, and a between border takes its space on both sides plus its width.

## Chinese font names

A missing Chinese family is drawn in the face of the style its name says:
宋 (not 仿宋) is Song, 仿宋 FangSong, 黑 Hei and 楷 Kai, so 方正小标宋简体 and 方正大标宋公文
are Song and 方正黑体_GBK is Hei. macOS faces are found by their Chinese names
(冬青黑体简体中文 → Hiragino Sans GB, 华文黑体 → STXihei, 华文行楷 → Xingkai SC,
隶书 → Baoli SC). Names of no style (幼圆, 方正魏碑) and 微软雅黑 go to the sans
fallback. When the East Asian font is an installed Latin font without Chinese
glyphs, the Chinese is drawn in the Song face, not the sans fallback.
The English names KaiTi_GB2312 and FangSong_GB2312 are not Kai and FangSong in
WPS for Mac: it draws both in its sans fallback, FangSong_GB2312 at 微软雅黑's
line height.

## Line units of paragraph spacing

Without a line grid, one line of `w:beforeLines` / `w:afterLines` is 12 pt in
WPS for Mac whatever the paragraph's font and size; with a line grid it is the
grid's pitch.

## Missing Latin fonts

WPS for Mac draws every missing Latin family in PingFang SC (so its lines are
PingFang's height), except Calibri, Calibri Light and Aptos, which it draws in
Helvetica Neue. Kition follows it: PingFang SC first, then (on systems without
it) metric-compatible or same-style open fonts. Cambria Math stays on serif
faces for equations.

## Japanese font names

A document whose theme font language is Japanese takes ＭＳ 明朝 for its East Asian
body text. WPS for Mac draws ＭＳ 明朝, MS Mincho and Meiryo in Hiragino Sans, and
the characters Hiragino lacks (simplified-only ones such as 导) in the Song face;
ヒラギノ角ゴ Pro W3 is the installed Hiragino Kaku Gothic Pro. Other Japanese names
(MS Gothic, 游明朝, Yu Gothic, メイリオ) go to the sans fallback.

## Line breaks inside words

WPS for Mac does not break a line inside a word after `/`, `?`, `!` or `-` when a
letter or digit follows (a web address's query, "well-known"); the word moves to
the next line whole when it fits there. Other punctuation follows the Unicode
line breaking rules.

## WPS's own Song, Hei and Kai faces

宋体, 黑体 and 楷体 (and Chinese names drawn in them, such as 方正小标宋简体) are drawn in
WPS's own faces, which stand 0.969 em above the baseline and 0.329 em below; the
installed stand-ins stand taller. A Latin font's line gap goes above its text, so
beside a Song line Helvetica Neue (0.952 + 0.028 em) stands taller than the Song
and a 12 pt line takes two 15.6 pt grid rows, while Times New Roman does not.

## Symbols the run's font lacks

Arrows, enclosed numbers and other symbols the run's font lacks are drawn in the
Song face, then PingFang SC, before the general fallback.

## Sections without margins

A section whose `w:sectPr` has no `w:pgMar` gets 1.25-inch left and right margins
and 1-inch top and bottom ones.

## Hyphenation

WPS for Mac has no automatic hyphenation: `w:autoHyphenation` is kept and saved
but doesn't break words. Soft hyphens still do.

## Tall footers

A footer that reaches past the bottom margin (its height up from the footer
distance) ends the body above it, as a tall header starts the body below it. Each
page uses the footer it shows: the first page's, the even pages', or the default.

## Balanced single-byte and double-byte widths

With `w:balanceSingleByteDoubleByteWidth` (WPS writes it into most documents), a
space next to a Chinese character is half an em wide; its own font still sets
the line's height. So is every space of a run of two or more, between Latin
letters too (10.5 pt: "A  B" puts 10.5 pt between the letters in Helvetica
Neue, Times New Roman and Arial). A single space between Latin words keeps its
width.

A justified line's spaces take back up to a tenth of their width to fit more
text (7 spaces: 9.8 % fits, 10.8 % doesn't; 14 spaces the same); half-em
balanced spaces take back nothing.

## Lines between columns

A section's line between columns (`w:sep`) is drawn in every stretch of columns on a page,
including one a continuous section break starts mid-page, and only beside a column that holds
something. Columns the page ends run their lines from the top of the columns to the bottom
margin; columns a continuous section break ends (balanced), to the bottom of the tallest one.

## Punctuation compression

With `compressPunctuation`, WPS for Mac lets a line's full-width marks give up at
most half the largest mark's size in all, shared between them (one mark may give
all of it): one 10.5 pt comma absorbs 4.7 pt, four absorb 4.7 pt but not 5.7 pt,
two 20 pt marks absorb 10 pt but not 11 pt. Ordinary characters are never
squeezed.

The automatic quarter-em gaps between Chinese text and Latin letters or digits
give up 0.48 of themselves too (at 10.5 pt, one to three gaps take back 1.25 pt
each; without compression none). The gaps can't bring back a final ideograph
reaching more than about a third of itself past the edge (lines of four to eight
gaps ending in one take back 3.2 to 4.7 pt), while a final digit comes back if it
starts inside the line. A line with full-width punctuation compresses only that
(its gaps stay whole).

## Character borders

A character border's width adds above and below its run's text in the line's
height; its distance from the text doesn't. A 0.75 pt border leaves a 10.5 pt
Song line in one 15.6 pt grid row, a 1 pt border makes it two.
Composite border styles take more room than their width: a double line three
widths, a triple five, a wave 2.16 pt more and a double wave 4.56 pt more.
Thin-thick and 3-D styles are still read as single lines.

## Drop caps

A document's drop cap keeps the size Word stored in its run and the distance from
the text in its frame (`w:framePr/@w:hSpace`); the lines it spans are those of the
text after it. With no room beside it WPS starts the text below it (not done yet).

## Page break before a section mark

A paragraph ending with a page break, followed by the empty paragraph that only
ends its section (before a section that starts a new page), makes one break: the
mark stays on the break's page.

## Table cell spacing

`w:tblCellSpacing` holds half the gap between cells. WPS for Mac opens the full gap
above every row and below the last, plus a border width each time because the
cells' borders don't share a line: two rows with 7.8 pt stored are 3 × (15.6 +
border) taller.

## "At least" line spacing on a grid

On a line grid, an "at least" height is a floor that isn't rounded to whole rows:
"at least 18 pt" lines on a 15.6 pt grid are 18 pt apart with their text at the
bottom, while "at least 12 pt" keeps the text's one row.

## Objects positioned from their paragraph

An offset from the paragraph counts from the paragraph's top above its space
before: a box 69 pt below its paragraph stays put whether the paragraph has 0 or
40 pt before it, and after a page break before, the box sits at the page's top
with the text below the space. A paragraph kept with the next moves to the next
page when its box (no room beside it) pushes the next paragraph's first line off
the page; that line starts right below the box, without its space before.

## Lines meeting floating objects on a grid

WPS for Mac wraps a gridded line around a floating object wherever the
line's rows meet it, even where the text centred in those rows doesn't, and a line pushed below an object starts
right at its foot. A box reaching above its own paragraph (a turned box's bounds,
or a negative offset) pushes aside the earlier lines it meets on its page and
stays where it was first placed.

## Floating tables past the page's foot

A floating table positioned from its anchor paragraph (`w:tblpPr`
`w:vertAnchor="text"`) that would run past the page's foot isn't moved to the
next page whole: WPS for Mac starts it where its offset puts it and carries the
rows that don't fit to the next page's top, still floating; the anchor paragraph
and the text after it start on that page, below a column-wide table's rows and
beside a narrow one's.

## Tight and through wrapping

WPS for Mac follows a tight or through wrap's polygon for pictures and
shapes as well as charts and diagrams: each line keeps clear of the polygon's span over the
line's text, the distance from text added at the sides (luan's picture: seven
lines beside it within 0.5 pt). A line whose rows but not text reach the polygon
runs past it.

## Where a table stands

WPS for Mac lines a table's first cell text up with the margin (plus the table's
indent) whatever the document's compatibility mode: the border stands a cell
margin outside it. Word 2013 and later put the border at the margin in mode 15;
the maintainers chose WPS's placement.

A table's room reaches its borders, a cell margin outside the text column on
each side (426.1 pt for a 415.3 pt column with 5.4 pt margins, the width WPS
saves in its own grids). An autofit table with no width of its own whose grid is
wider narrows to that room, every column in proportion, from its indent when it
has one (a -50 pt indent gives it 50 pt more); a percentage width is a share of
the room, past 100 % too (120 % of 426.1 pt is 511.3 pt); a width in points
stands however wide, even past the page (synthetic references, 12 tables, every
column edge within 0.1 pt).

## Contents lines

WPS for Mac steps a dot leader's marks a whole tenth of a point apart, the
mark's advance rounded up: 10.5 pt Helvetica Neue's 2.92 pt full stop 3 pt
apart, 11 pt's 3.06 pt 3.1 pt, 12 pt PingFang's 3.17 pt 3.2 pt (luan's contents
and the reference corpus). It updates page references when it prints or
exports, so a contents entry shows the page its heading lands on rather than the
number saved with the file; Kition does the same for printing and export.
