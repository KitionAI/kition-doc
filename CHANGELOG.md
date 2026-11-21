# Changelog

Changes to Kition Doc, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

## 0.1.4

- The same kit as 0.1.3, built on kition-sdk 6672663 (the replica's undo passes over an undo group whose changes came to nothing): the 0.1.3 package was built on kition-sdk 6b4d471.

## 0.1.3

- A host asks a Word file for its document id (`document.identity`) and copies a document as a new document (`document.fork`): a `.docx` is written from the document and embeds no replica, so it carries no id and its fork is the same bytes (Kit Process Protocol section 5.6, kition-sdk 6b4d471).
- The headless program speaks Kit Process Protocol version 1 (`headless.protocol` is `kition-kit/1`), as hosts on kition-sdk from bb1b07b require, and leaves out of what an agent reads exactly the editor state `kit.json` declares (`engine.editorState`).
- kition-sdk 6672663: the replica's undo passes over an undo group whose changes came to nothing (a value written and written back), and the same Undo takes back the step before it, as before kition-sdk 4b81569.

## 0.1.2

- `kit.json` declares the engine's editor state (`engine.editorState`), so a host that does not run the engine's Rust code reads `inspect()` without it.
- README: an agent connects with `npm install -g @kition/cli` and `kition agent add`.

## 0.1.1

- The pasteboard around the pages follows the host's theme, live: in a dark host it is dark, and it repaints when the host's theme changes (before, it kept its built-in light grey).

## 0.1.0

First public release of the source, under the Apache License 2.0. It contains:

- The Rust engine: a document model of stories, paragraphs, tables, sections, styles, numbering,
  fields, notes, comments, revisions and equations; page layout with headers and footers,
  columns, floating objects and East Asian line grids; the `.docx` reader and writer, a Word
  97-2003 `.doc` reader, and export to PDF and PNG.
- The kit engine (`kition-doc-kit`), native and WebAssembly: the document is a KDM replica,
  replicas merge concurrent edits, and undo is per person and per agent. A check reports what a
  document is missing, with the command that fixes it.
- The headless kit (`kition-doc`) for agents over the Kit Protocol: it reads a document's
  paragraphs and tables, edits them with the editor's own commands, and works in drafts. A write
  names the revision it read and is refused when someone else changed what it rests on. It reads
  nothing outside its workspace and starts no program.
- The kit's editor (`web/`): pages with a ribbon, a styles gallery, tables, headers and footers,
  comments and find, in kit-editor's frame.
- NOTICE and docs/attribution.md list the upstream work, the fonts and every asset, and Loro
  (MIT), bundled by the editor and linked by the engine.
