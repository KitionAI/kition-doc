# Kition Doc architecture

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft Word (Microsoft 365) for Mac 16.113.4

How Kition Doc is built. Rules for working in the code are in [`AGENTS.md`](../AGENTS.md).

## Layers

11 crates, one headless app and `xtask`. Layering is enforced by `cargo xtask layers`: a crate may
only depend on crates in lower layers.

| Layer | Crate | Job |
|---|---|---|
| L0 | `geom` | Units (points, twips, EMU, inches, cm), rectangles, measurement parsing |
| L1 | `doc` | Document model: stories, paragraphs, runs, tables, sections, styles, numbering, fields, notes, comments, revisions, equations (OMML-shaped math tree, linear/LaTeX input); edit primitives; bidi |
| L1 | `fonts` | Font database (bundled OFL + system fonts), metrics, outlines, shaping with HarfRust, Word font substitution (`word.rs`: Carlito ↔ Calibri, Caladea ↔ Cambria, Liberation ↔ Arial/Times/Courier), OpenType MATH |
| L1 | `proof` | Spelling (public-domain English word list), suggestions, rule-based grammar, hyphenation patterns |
| L2 | `layout` | Shaping runs, line breaking, tabs, lists, tables (row splitting, floating tables), pagination, headers/footers, notes, floating objects and wrap, math layout, fields, hit testing; produces a display list |
| L2 | `docx` | OOXML WordprocessingML reader and writer (Transitional and Strict namespaces, `.docx/.docm/.dotx/.dotm`), OMML, custom properties, VBA parts preserved |
| L2 | `docbin` | Word 97-2003 `.doc` reader ([MS-DOC]: FIB, piece table, FKPs, SPRMs, stylesheet, lists, tables, notes, pictures) over a CFB container |
| L2 | `formats` | TXT, Markdown, HTML, RTF, ODT and LaTeX import/export through a shared intermediate model |
| L3 | `render` | vello_cpu rasteriser: pages and thumbnails from the display list; PNG/JPEG |
| L3 | `pdf` | krilla PDF export: font subsets, selectable text, links, outline, tagging |
| L3 | `kdm-model` | Kition's `kit.doc` KDM bridge (Apache-2.0): the `Document` as a replica (`model/schema.json`), a whole write, a diff write of what a command changed, the read back, and the repair after a merge |
| L4 | `engine` | `Session` (document, selection, undo, layout cache, view state), **command registry** (`CommandSpec`s in `cmd/*.rs`), Word feature catalog (`catalog.rs`), samples, file I/O dispatch (`io.rs`), Read Aloud sentence model |
| L5 | `kition-doc-kit` | Kition's in-page editor engine (Apache-2.0): the document as a `KitDocument`, typing as `KitTools`, compiled to WebAssembly |
| apps | `kition-doc` | Headless kit executable for Kition's agent, over the Kit Protocol |

## Data flow

1. **Open:** `engine::io` picks a reader by extension (`docx`, `docbin`, `formats`, JSON) and
   produces a `doc::Document`.
2. **Edit:** every user-visible action is a command id run through `Session::run`. `Session::run`
   snapshots for undo (blocks are `Arc`-shared, so snapshots are cheap) and catches an escaped
   panic, restoring the document (never-crash standard).
3. **Layout:** `layout` turns the document into pages of positioned glyphs, boxes and images
   (display list), incrementally: an edit relayouts only what changed.
4. **Draw:** `kition-doc-kit` paints pages through `render` (CPU raster, cached per page,
   pixel-aligned) for Kition's in-page editor; PDF export walks the same display list through
   `pdf`.
5. **Save:** `engine::io` picks a writer by extension.

## Testing and gates

- `#[test]` functions and property-test blocks: model, layout, DOCX round-trip and hostile input,
  `.doc`, formats, typing parity (`engine/src/tests_typing.rs`), bidi; `hostile_params_never_panic`
  runs every command with junk params.
- `cargo xtask ci`: fmt, clippy `-D warnings`, tests, asset attribution, layering, wasm build.
- `cargo xtask parity` regenerates [`parity-checklist.md`](parity/parity-checklist.md); a test keeps the
  catalog percentage above a floor that only rises.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
