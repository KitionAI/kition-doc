//! The independent reading of every write command (task card C8-doc-3, item 1): for each of the
//! commands the kit offers that change the document (`mutates`), what its handler
//! (`crates/engine/engine/src/cmd/*.rs`) and the engine edit behind it resolve and read, written
//! down from the engine code by readers who did not open the declarations (`bindings/`), their
//! tests or the agent guide, one row per code path ([`READINGS`]). The rows were written before
//! the comparison and are not changed to fit it; the comparison ([`declared`], [`differences`]) was
//! written after.
//!
//! A row: `ROW <command> | sel=<the call's selection> | params=<JSON> | holds=<object{fields}>;… |
//! windows=<holder.list>,… | facts=<what no stored field holds> | reads=<editor/session state> |
//! note=<the code path>`. Objects are named by their place on [`fixture`]: `doc` the root, `bN` a
//! body block, `b4/R/C/0` a cell's paragraph, `t4` the table, `r4/R` a row, `c4/R/C` a cell,
//! `part:header|footer|footnote` (and `part:header/0` its paragraph), `thread:0`, `comment:N`,
//! `rev:0`, `media:0`, `inline:pic|shape|eq`; fields are the object model's
//! (`crates/kdm/kdm-model/model/schema.json`), `@` the object's place in its list, `*` only that
//! it is there, `text~` part of the text (the note says which), `allblocks` every paragraph,
//! `allinline` every inline object.
//!
//! Every write's handler is reached through `Session::run_command`, which reads the root's
//! `settings` (protection, track changes) for every command: each row holds `doc{settings}` besides
//! what it says. Drawing variants provide the extra objects the rows name (`inline:a`,
//! `inline:conn`, `inline:group`, `inline:chart`); `bA`, `bB`, `bG` name their actual anchors.
//! Other unresolved names are listed by the test.
//! Drawing numbers in `params` were written by the readers as `8` for the fixture's shape: its
//! number on the fixture is `1`, substituted mechanically.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use base64::Engine as _;
use kdm_model::DocModel;
use kition_doc_doc::{Block, PartKind, StoryRef};
use kition_doc_kit::DocDocument;
use kition_kit::kdm::{ActorId, FieldKind, ObjectId, Replica};
use kition_kit::session::{KitModel, MemoryBlobs};
use kition_kit::{KitCommand, KitDocument};
use serde_json::{Value, json};

/// The readings, one row per code path (see the module docs).
const READINGS: &str = r##"
# ---- cmd/para.rs
ROW para.addSpaceBefore | sel=b1:6-10 | params={} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=toggle decided from focus paragraph's resolved spacing; each paragraph's own resolved lines/auto checked
ROW para.addSpaceBefore | sel=b4/0/1/0:0 | params={} | holds=b4/0/1/0{para,@};doc{styles} | windows=doc.body,t4.rows,r4/0.cells,c4/0/1.blocks | facts=none | reads=selection | note=cell paragraph; table style NOT consulted (resolve_para, not resolve_para_in)
ROW para.align | sel=b1:6-10 | params={"value":"start"} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=writes para.align only, reads no props
ROW para.align | sel=b1:6-10 | params={"value":"right"} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=focus paragraph resolved bidi + each paragraph's direct bidi; non-focus paragraphs with style-inherited bidi use the focus's (see DEFECTS)
ROW para.alignCenter | sel=b1:6-10 | params={} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=toggle from focus paragraph's resolved align/bidi (style chain); per-paragraph direct bidi
ROW para.alignLeft | sel=b1:6-10 | params={} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=same path as alignCenter (Left never toggles; still reads bidi)
ROW para.alignRight | sel=b1:6-10 | params={} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=same path as alignCenter
ROW para.justify | sel=b1:6-10 | params={} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=same path as alignCenter
ROW para.distribute | sel=b1:6-10 | params={} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=same path as alignCenter
ROW para.asianTypography | sel=b1:6-10 | params={"kinsoku":true} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=write needs no read; result reads focus resolved flags
ROW para.asianTypography | sel=b1:6-10 | params={} | holds=b1{para};doc{styles} | windows=doc.body | facts=none | reads=selection | note=no write; only returns focus paragraph's resolved flags
ROW para.borders | sel=b1:6-10 | params={"sides":{"top":{}}} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=replaces borders wholesale
ROW para.borders | sel=b1:6-10 | params={"kind":"horizontalLine"} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=replaces borders wholesale
ROW para.borders | sel=b1:6-10 | params={"kind":"bottom"} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=toggles from each paragraph's direct borders (style borders ignored)
ROW para.borders | sel=b1:6-10 | params={"kind":"outside"} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=merges into each paragraph's direct borders (keeps between)
ROW para.bullets | sel=b1:6-10 | params={} | holds=b1{para,@};b0{para,@};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=b1 not listed -> apply; prev paragraph b0 checked for continuation; find_kind may reuse a far bullet list
ROW para.bullets | sel=b2:0 (caret) | params={} | holds=b2{para,@};b1{para,@};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=b2 numbered (not bullet) -> converts to bullet list
ROW para.bullets | sel=b1:6-10 | params={"off":true} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=remove path; reads style (ListParagraph) only
ROW para.bullets | sel=b5:0 (caret) | params={} | holds=b5{para,@};b4/1/1/0{para,@};doc{numbering,lists} | windows=doc.body,t4.rows,r4/1.cells,c4/1/1.blocks | facts=none | reads=selection | note=previous paragraph in story order is the last table cell's paragraph
ROW para.double | sel=b1:6-10 | params={} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=sets line_spacing Multiple(2.0); the assigned paragraph properties require same-field protection [review: handler premises and same-field protection independently checked]
ROW para.hangingIndent | sel=b1:6-10 | params={} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=reads direct indent_left (not resolved; style/list indent ignored), +36pt, first=-36
ROW para.heading1 | sel=b1:6-10 | params={} | holds=b1{para,text,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=single paragraph: clears direct font/size/color in all b1 runs + mark; writes pending=None
ROW para.heading1 | sel=b2:0 (caret) | params={} | holds=b2{para,text,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=direct list numbering removed (b2 leaves the list)
ROW para.heading2 | sel=b1:6-10 | params={} | holds=b1{para,text,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=same path as heading1; fails if doc has no Heading2
ROW para.heading3 | sel=b1:6-10 | params={} | holds=b1{para,text,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=same path as heading1
ROW para.normal | sel=b1:6-10 | params={} | holds=b1{para,text,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=same path; Normal also removes direct numbering
ROW para.style | sel=b1:6-10 | params={"style":"Heading 1"} | holds=b1{para,text,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=paragraph style; multi-paragraph selection (b1:0..b3:1) skips the run clearing and holds only {para}
ROW para.style | sel=b1:6-10 | params={"style":"Strong"} | holds=b1{text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,mode | note=character style: sets run style on bytes 6..10 (also mark if paragraph fully covered); column mode -> per row
ROW para.style | sel=b1:6 (caret) | params={"style":"Strong"} | holds=b1{text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between word chars? b1:6 is after space -> no word -> pending=typing_props(pending or run left of caret); inside a word (b1:7) formats the word
ROW para.indent | sel=b1:6-10 | params={} | holds=b1{para,@};doc{styles,settings} | windows=doc.body | facts=none | reads=selection | note=not numbered: resolved indent_left + default tab stop
ROW para.indent | sel=b2:0 (caret) | params={} | holds=b2{para,@} | windows=doc.body | facts=none | reads=selection | note=numbered: list level +1, numId kept
ROW para.indents | sel=b1:6-10 | params={"left":36} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=sets only the given indents (clamped ±1584), no read
ROW para.keepLines | sel=b1:6-10 | params={} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=toggle from focus paragraph's direct keep_lines only
ROW para.keepLines | sel=b1:6-10 | params={"value":true} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=explicit value (undocumented param)
ROW para.keepNext | sel=b0:0 (caret) | params={} | holds=b0{para,@} | windows=doc.body | facts=none | reads=selection | note=direct only: Heading 1's keepNext from the style is not seen (see DEFECTS)
ROW para.pageBreakBefore | sel=b1:6-10 | params={} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=toggle from focus direct flag
ROW para.widowControl | sel=b1:6-10 | params={} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=resolved default is ON but toggle reads direct None -> sets true (see DEFECTS)
ROW para.lineSpacing | sel=b1:6-10 | params={"value":1.15} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=writes line_spacing, no read
ROW para.listLevel | sel=b2:0 (caret) | params={"level":2} | holds=b2{para,@} | windows=doc.body | facts=none | reads=selection | note=sets level on direct numbering; paragraphs without direct numbering unchanged (b1 default sel: no-op); missing level -> 0
ROW para.ltr | sel=b1:6-10 | params={} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=sets bidi=false (value:false -> true)
ROW para.rtl | sel=b1:6-10 | params={} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=sets bidi=true (value:false -> false)
ROW para.multilevel | sel=b1:6-10 | params={} | holds=b1{para,@};b0{para,@};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=b1 not listed; prev b0 checked; non-bullet always -> add_list (new list)
ROW para.multilevel | sel=b2:0 (caret) | params={} | holds=b2{para,@};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=b2 already numbered: same_kind only compares bullet vs non-bullet when no kind -> REMOVES numbering (see DEFECTS)
ROW para.numbering | sel=b1:6-10 | params={} | holds=b1{para,@};b0{para,@};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=apply; prev b0 not numbered -> add_list
ROW para.numbering | sel=b3:0 (caret) | params={} | holds=b3{para,@};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=already numbered list -> removes (num 0, indents cleared)
ROW para.numbering | sel=b3:0 (caret) | params={"kind":"upperLetter"} | holds=b3{para,@};b2{para,@};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=kind given: level-0 text compared; differs -> prev b2's list not same kind -> new list
ROW para.numbering | sel=b1:6-10 | params={"off":true} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=remove path
ROW para.oneAndHalf | sel=b1:6-10 | params={} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=Multiple(1.5)
ROW para.single | sel=b1:6-10 | params={} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=Multiple(1.0)
ROW para.outdent | sel=b1:6-10 | params={} | holds=b1{para,@};doc{styles,settings} | windows=doc.body | facts=none | reads=selection | note=not numbered: previous default-tab multiple below resolved indent_left, min 0
ROW para.outdent | sel=b2:0 (caret) | params={} | holds=b2{para,@} | windows=doc.body | facts=none | reads=selection | note=numbered: level-1 (saturating)
ROW para.outlineLevel | sel=b1:6-10 | params={"level":1} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=reads undocumented `level` (min 9); params {} -> outline_level None (clears)
ROW para.removeHanging | sel=b1:6-10 | params={} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=direct indent_left-36 (min 0), first=0
ROW para.removeSpaceAfter | sel=b1:6-10 | params={} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=toggle from focus resolved space_after/lines/auto (0 or 8pt); per paragraph own resolved lines/auto
ROW para.restartNumbering | sel=b1:6-10 | params={} | holds=b1{para} | windows=doc.body | facts=none | reads=selection | note=b1 not numbered -> Failed, no write
ROW para.restartNumbering | sel=b2:0 (caret) | params={} | holds=b2{para,@};b3{para,@};b4/0/0/0{para,text,@};doc{numbering,lists} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=b2,b3 moved to new num; stops at first later unnumbered non-empty paragraph (A1); empty unnumbered and other-list paragraphs are walked past
ROW para.set | sel=b1:6-10 | params={"props":{"align":"center"}} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=overlay of the given ParaProps fields (None fields kept); missing props -> deserialize Null error
ROW para.shading | sel=b1:6-10 | params={"color":"FFFF00"} | holds=b1{*,@} | windows=doc.body | facts=none | reads=selection | note=absent or unparsable color clears shading
ROW para.sort | sel=b1:6-10 | params={} | holds=b1{text,@} | windows=doc.body | facts=none | reads=selection | note=one paragraph: no reorder but still rewrites sel anchor to b1:0
ROW para.sort | sel=b1:0..b3:3 | params={} | holds=b1{text,@};b2{text,@};b3{text,@} | windows=doc.body | facts=none | reads=selection | note=reorders doc.body[1..=3] by text; a selection b3..b5 fails (b4 is a table)
ROW para.sort | sel=b4/0/0/0:0 | params={"descending":true} | holds=b4/0/0/0{text,@} | windows=doc.body,t4.rows,r4/0.cells,c4/0/0.blocks | facts=none | reads=selection | note=container is the cell's block list
ROW para.sort | sel=b1:0..b4/0/0/0:1 | params={} | holds=b1{*} | windows=doc.body | facts=none | reads=selection | note=different parents -> Failed
ROW para.spacing | sel=b1:6-10 | params={"before":6,"after":0} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=sets points; zeroes lines/auto when each paragraph's resolved value has them
ROW para.tabs | sel=b1:6-10 | params={"default":36} | holds=doc{*} | windows= | facts=none | reads=selection | note=only writes settings.default_tab
ROW para.tabs | sel=b1:6-10 | params={"set":[{"pos":72}]} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=merges into direct tabs
ROW para.tabs | sel=b1:6-10 | params={"clear":[72]} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=clear stop recorded when style-resolved tabs have 72
ROW para.tabs | sel=b1:6-10 | params={"tabs":[{"pos":36}],"clearAll":true} | holds=b1{para,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=replace list; inherited stops all cleared
ROW styles.addToGallery | sel=none | params={"style":"Heading 1"} | holds=doc{styles} | windows= | facts=none | reads=none | note=REQUIRES `style` (notation says {}), sets quick=value (default true); params {} -> Params error; returns selection
ROW styles.create | sel=b1:6-10 | params={"name":"Mine"} | holds=b1{para,text,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=copies focus para props + char props of the run left of byte 10; then applies (single paragraph: clears b1 runs' font/size/colour, pending=None)
ROW styles.create | sel=b1:6-10 | params={"name":"Mine","fromSelection":false,"basedOn":"Heading 1"} | holds=b1{para,text,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=no copy; still applies to selection (writes b1 style + clears its run fonts)
ROW styles.delete | sel=none | params={"style":"Mine"} | holds=doc{styles};allblocks{para,text} | windows= | facts=none | reads=none | note=reads/writes every paragraph's style, mark style and run styles in body and parts
ROW styles.delete | sel=none | params={"style":"Heading 1"} | holds=doc{styles} | windows= | facts=none | reads=none | note=built-in -> Failed, no write
ROW styles.delete | sel=none | params={"style":"MyTable"} | holds=doc{styles};t4{props};allblocks{*} | windows= | facts=none | reads=none | note=table-style path: walks every table in body+parts and retargets table style
ROW styles.modify | sel=none | params={"style":"Heading 1","para":{"align":"center"}} | holds=doc{styles} | windows= | facts=none | reads=none | note=overlay on style record; returns selection
ROW styles.modify | sel=none | params={"style":"Heading 1","next":"Normal","name":"Head One"} | holds=doc{styles} | windows= | facts=none | reads=none | note=next raw string, name unchecked (see DEFECTS)
ROW styles.setVisibility | sel=none | params={"style":"Heading 1","gallery":false} | holds=doc{styles} | windows= | facts=none | reads=none | note=sets quick/hidden; gallery:true also unhides; neither -> Params error
ROW styles.updateToMatch | sel=b1:6-10 | params={} | holds=b1{para,text,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=b1's Normal updated from char props left of byte 10 + b1 para props; b1 direct formatting wiped (para_id/fmtChange/numChange/control too)
ROW styles.updateToMatch | sel=b1:6-10 | params={"style":"Heading 2"} | holds=b1{para,text,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=updates Heading 2 although b1 is not in it, and still wipes b1's direct formatting (see DEFECTS)
# ---- cmd/format.rs, lists.rs, inspector.rs, paste.rs
ROW format.allCaps | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.allCaps | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.allCaps | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.allCaps | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.bold | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.bold | sel=b1:6-10 | params={"value":true} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=explicit value: writes run marks in 6..10 only (text~ = run boundaries at 6 and 10)
ROW format.bold | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.bold | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.bold | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.border | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.border | sel=b1:6-10 | params={"value":true} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=explicit value: writes run marks in 6..10 only (text~ = run boundaries at 6 and 10)
ROW format.border | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.border | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.border | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.changeCase | sel=b1:6-10 | params={} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=mode derived from plain text of 6..10 and the same bytes rewritten in place (text~ = chars 6..10)
ROW format.changeCase | sel=b1:6-10 | params={"mode":"upper"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=explicit mode: reads only b1 text 6..10 and rewrites it; if byte length changes (e.g. ß) the range is deleted (objects inside lost) and reinserted with the formatting of char 6
ROW format.changeCase | sel=b1:8 (caret) | params={"mode":"upper"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=caret in word: word 6..10 from text around 8; rewrites those bytes
ROW format.changeCase | sel=b4/0/0/0:0-2 .. b4/1/0/0:0-2 (column block) | params={"mode":"upper"} | holds=b4/R/C/0{text~,@} | windows=doc.body,t4.rows,r4/R.cells | facts=none | reads=selection,mode(column segments) | note=column::per_row runs the handler once per segment (selection set to each row)
ROW format.charStyle | sel=b1:6-10 | params={"style":"Strong"} | holds=doc{styles};b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=style looked up by id or name in doc.styles; writes rStyle on runs 6..10
ROW format.charStyle | sel=b1:6 (caret) | params={"style":"Strong"} | holds=doc{styles};b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: patches session.pending (pending or char-before props)
ROW format.charStyle | sel=b1:6-10 | params={"style":"Strong"} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.clear | sel=b1:6-10 | params={} | holds=b1{text~,para,@} | windows=doc.body | facts=none | reads=selection | note=clears run formatting 6..10 (keeps link/revision marks read from those runs) and resets b1's whole para props (incl. its paraId)
ROW format.clear | sel=b2:0-3 | params={} | holds=b2{text~,para,@} | windows=doc.body | facts=none | reads=selection | note=list paragraph: numbering removed (numbering filtered to None); doc.numbering untouched
ROW format.clear | sel=b1:6 (caret) | params={} | holds=b1{text~,para,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words (word test reads chars 5/6): pending=default, b1 para props reset
ROW format.clear | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.color | sel=b1:6-10 | params={"color":"FF0000"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=writes run marks 6..10 only
ROW format.color | sel=b1:6 (caret) | params={"color":"FF0000"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: pending (or char-before props at 6) patched into session.pending; empty paragraph mark too
ROW format.color | sel=b1:6-10 | params={"color":"FF0000"} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.doubleStrikethrough | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.doubleStrikethrough | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.doubleStrikethrough | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.doubleStrikethrough | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.doubleUnderline | sel=b1:6-10 | params={} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=no read of current underline; writes underline=double on runs 6..10
ROW format.doubleUnderline | sel=b1:6 (caret) | params={} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: patches typing_props (pending or char before 6) into session.pending; empty paragraph mark also written
ROW format.doubleUnderline | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.emboss | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.emboss | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.emboss | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.emboss | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.engrave | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.engrave | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.engrave | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.engrave | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.font | sel=b1:6-10 | params={"name":"Arial"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=writes run marks 6..10 only
ROW format.font | sel=b1:6 (caret) | params={"name":"Arial"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: pending (or char-before props at 6) patched into session.pending; empty paragraph mark too
ROW format.font | sel=b1:6-10 | params={"name":"Arial"} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.growFont | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=size of the FIRST run overlapping 6..10 (resolved through para style/char style/defaults) decides the one size written to every run in range
ROW format.growFont | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: size from pending or char before 6 under b1's style; writes session.pending
ROW format.growFont | sel=b4/0/0/0:0-2 .. b4/1/0/0:0-2 (column block) | params={} | holds=b4/R/C/0{para,text~,@};doc{styles} | windows=doc.body,t4.rows,r4/R.cells | facts=none | reads=selection,mode(column segments) | note=active column selection (session.column, valid while sel unchanged): each segment clamped (doc.clamp) and formatted; toggle state from resolved runs of all segments; segments that are empty or cross paragraphs dropped
ROW format.growFont | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.growFont1 | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=size of the FIRST run overlapping 6..10 (resolved through para style/char style/defaults) decides the one size written to every run in range
ROW format.growFont1 | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: size from pending or char before 6 under b1's style; writes session.pending
ROW format.growFont1 | sel=b4/0/0/0:0-2 .. b4/1/0/0:0-2 (column block) | params={} | holds=b4/R/C/0{para,text~,@};doc{styles} | windows=doc.body,t4.rows,r4/R.cells | facts=none | reads=selection,mode(column segments) | note=active column selection (session.column, valid while sel unchanged): each segment clamped (doc.clamp) and formatted; toggle state from resolved runs of all segments; segments that are empty or cross paragraphs dropped
ROW format.growFont1 | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.hidden | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.hidden | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.hidden | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.hidden | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.highlight | sel=b1:6-10 | params={"color":"yellow"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=writes run marks 6..10 only
ROW format.highlight | sel=b1:6 (caret) | params={"color":"yellow"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: pending (or char-before props at 6) patched into session.pending; empty paragraph mark too
ROW format.highlight | sel=b1:6-10 | params={"color":"yellow"} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.italic | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.italic | sel=b1:6-10 | params={"value":true} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=explicit value: writes run marks in 6..10 only (text~ = run boundaries at 6 and 10)
ROW format.italic | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.italic | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.italic | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.outline | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.outline | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.outline | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.outline | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.position | sel=b1:6-10 | params={"points":3} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=writes run marks 6..10 only
ROW format.position | sel=b1:6 (caret) | params={"points":3} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: pending (or char-before props at 6) patched into session.pending; empty paragraph mark too
ROW format.position | sel=b1:6-10 | params={"points":3} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.resetChar | sel=b1:6-10 | params={} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=runs 6..10 cleared keeping link/revision marks
ROW format.resetChar | sel=b1:6 (caret) | params={} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: pending=default; only b5:0-like empty paragraphs get their mark cleared
ROW format.resetChar | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.scale | sel=b1:6-10 | params={"percent":150} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=writes run marks 6..10 only
ROW format.scale | sel=b1:6 (caret) | params={"percent":150} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: pending (or char-before props at 6) patched into session.pending; empty paragraph mark too
ROW format.scale | sel=b1:6-10 | params={"percent":150} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.set | sel=b1:6-10 | params={"props":{"bold":true}} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=writes run marks 6..10 only
ROW format.set | sel=b1:6 (caret) | params={"props":{"bold":true}} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: pending (or char-before props at 6) patched into session.pending; empty paragraph mark too
ROW format.set | sel=b1:6-10 | params={"props":{"ins":7,"style":"Nope"}} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=revision index / style id written unchecked (no read of doc.revisions or doc.styles)
ROW format.set | sel=b1:6-10 | params={"props":{"bold":true}} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.shading | sel=b1:6-10 | params={"color":"FFFF00"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=writes run marks 6..10 only
ROW format.shading | sel=b1:6 (caret) | params={"color":"FFFF00"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: pending (or char-before props at 6) patched into session.pending; empty paragraph mark too
ROW format.shading | sel=b1:6-10 | params={"color":"FFFF00"} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.shadow | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.shadow | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.shadow | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.shadow | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.shrinkFont | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=size of the FIRST run overlapping 6..10 (resolved through para style/char style/defaults) decides the one size written to every run in range
ROW format.shrinkFont | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: size from pending or char before 6 under b1's style; writes session.pending
ROW format.shrinkFont | sel=b4/0/0/0:0-2 .. b4/1/0/0:0-2 (column block) | params={} | holds=b4/R/C/0{para,text~,@};doc{styles} | windows=doc.body,t4.rows,r4/R.cells | facts=none | reads=selection,mode(column segments) | note=active column selection (session.column, valid while sel unchanged): each segment clamped (doc.clamp) and formatted; toggle state from resolved runs of all segments; segments that are empty or cross paragraphs dropped
ROW format.shrinkFont | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.shrinkFont1 | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=size of the FIRST run overlapping 6..10 (resolved through para style/char style/defaults) decides the one size written to every run in range
ROW format.shrinkFont1 | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: size from pending or char before 6 under b1's style; writes session.pending
ROW format.shrinkFont1 | sel=b4/0/0/0:0-2 .. b4/1/0/0:0-2 (column block) | params={} | holds=b4/R/C/0{para,text~,@};doc{styles} | windows=doc.body,t4.rows,r4/R.cells | facts=none | reads=selection,mode(column segments) | note=active column selection (session.column, valid while sel unchanged): each segment clamped (doc.clamp) and formatted; toggle state from resolved runs of all segments; segments that are empty or cross paragraphs dropped
ROW format.shrinkFont1 | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.size | sel=b1:6-10 | params={"size":14} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=writes run marks 6..10 only
ROW format.size | sel=b1:6 (caret) | params={"size":14} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: pending (or char-before props at 6) patched into session.pending; empty paragraph mark too
ROW format.size | sel=b1:6-10 | params={"size":14} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.smallCaps | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.smallCaps | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.smallCaps | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.smallCaps | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.spacing | sel=b1:6-10 | params={"points":1} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=writes run marks 6..10 only
ROW format.spacing | sel=b1:6 (caret) | params={"points":1} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: pending (or char-before props at 6) patched into session.pending; empty paragraph mark too
ROW format.spacing | sel=b1:6-10 | params={"points":1} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.strikethrough | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.strikethrough | sel=b1:6-10 | params={"value":true} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=explicit value: writes run marks in 6..10 only (text~ = run boundaries at 6 and 10)
ROW format.strikethrough | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.strikethrough | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.strikethrough | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.subscript | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.subscript | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.subscript | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.subscript | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.superscript | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=value omitted: resolved formatting of runs in b1 6..10 decides on/off; writes run marks 6..10 (text~ = runs in range)
ROW format.superscript | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: reads pending or the formatting of the char before 6 and the chars at 5/6 (word test); writes session.pending only (b1 not empty); on b5:0 it also writes b5 mark (holds b5{text,para,@})
ROW format.superscript | sel=b1:8 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=caret inside a word: word bounds from b1 text around 8 (6..10), resolved runs there; writes run marks 6..10
ROW format.superscript | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.underline | sel=b1:6-10 | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection | note=no value/style: resolved underline of runs 6..10 decides toggle
ROW format.underline | sel=b1:6-10 | params={"style":"wave"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=style or value given: no read; writes run marks 6..10
ROW format.underline | sel=b1:6 (caret) | params={} | holds=b1{para,text~,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: resolved typing_props; writes session.pending (+ mark of an empty paragraph)
ROW format.underline | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW format.wordUnderline | sel=b1:6-10 | params={} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=no read of current underline; writes underline=words on runs 6..10
ROW format.wordUnderline | sel=b1:6 (caret) | params={} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: patches typing_props (pending or char before 6) into session.pending; empty paragraph mark also written
ROW format.wordUnderline | sel=b1:6-10 | params={} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW list.define | sel=b1:6-10 | params={"levels":[{"format":"upperRoman"}]} | holds=doc{numbering,lists};b1{para,@} | windows=doc.body,doc.lists | facts=none | reads=selection | note=new list: reads existing abstract/num ids for max+1; b1 numbering/indent/style rewritten from its current numbering and style
ROW list.define | sel=b2:0-3 .. b3:0-3 | params={"levels":[{"start":3}],"num":1} | holds=doc{numbering,lists};b2{para,@};b3{para,@} | windows=doc.body,doc.lists | facts=none | reads=selection | note=in place: base = num's level (override or abstract); abstract levels replaced; every paragraph in body+parts scanned for numbering.num sharing the abstract (touch); b2/b3 keep their ilvl [corrected C8-doc-6: allblocks{para} removed: the scan of every paragraph by list.define (touch_lists/touch_styles, crates/engine/engine/src/cmd/lists.rs:237-257) only marks paragraphs to be laid out again (Paragraph::touch, a cache counter outside equality, crates/engine/doc/src/para.rs:736); it reads nothing the write depends on]
ROW list.define | sel=b1:6-10 | params={"levels":[{"style":"Heading 1"}],"apply":false} | holds=doc{numbering,lists,styles} | windows=doc.lists | facts=none | reads=none | note=linked style: Heading 1 found by name; style para numbering set; all paragraphs with style id Heading1 (b0) touched; level.style keeps 'Heading 1' (name) [corrected C8-doc-6: allblocks{para} removed: the scan of every paragraph by list.define (touch_lists/touch_styles, crates/engine/engine/src/cmd/lists.rs:237-257) only marks paragraphs to be laid out again (Paragraph::touch, a cache counter outside equality, crates/engine/doc/src/para.rs:736); it reads nothing the write depends on]
ROW list.define | sel=b1:6-10 | params={"levels":[{}],"apply":false} | holds=doc{numbering,lists} | windows=doc.lists | facts=none | reads=none | note=apply=false, no style: selection not read; only numbering written
ROW styles.inspectorClear | sel=b1:6-10 | params={"level":"paragraphStyle"} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=b1 style removed (Normal); other para props read and kept
ROW styles.inspectorClear | sel=b1:6-10 | params={"level":"paragraphFormatting"} | holds=b1{para,@} | windows=doc.body | facts=none | reads=selection | note=b1 props rebuilt from its style+numbering only (paraId lost)
ROW styles.inspectorClear | sel=b1:6-10 | params={"level":"characterFormatting"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection | note=runs 6..10 reset keeping style/link/ins/del/lang/noProof/rtl read from them
ROW styles.inspectorClear | sel=b1:6 (caret) | params={"level":"characterStyle"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=caret between words: session.pending (pending or char-before props) with style None; paragraph levels with a caret act on b1's para (holds b1{para,@})
ROW styles.inspectorClear | sel=b1:6-10 | params={"level":"paragraphFormatting"} | holds=doc{settings,revisions} | windows=doc.revisions | facts=time | reads=author,prefs.markup.trackFormatting | note=post-command (Session::run -> fmt_revisions::record) when settings.trackChanges && prefs.markup.track_formatting: every paragraph of body and parts compared with the pre-command copy; changed runs/mark/para props get fmtChange, a Format revision (author, now) appended or reused [corrected C8-doc-5: fmt_revisions.rs:142 skips every block the command left unchanged (Arc::ptr_eq with its own pre-command copy, session.rs:1011-1013), so only the blocks it changed, which its own holds name, are read]
ROW edit.pasteSpecial | sel=b1:6-10 | params={"as":"text","text":"x"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=typing props from pending or char before focus (10); text 6..10 deleted; x inserted at 6; (tracking: doc{settings,revisions} + time + author) [corrected C8-doc-6: removed para: cmd/paste.rs:112-116 delegates text to cmd/mod.rs:191-217 type_text; this nonempty single-paragraph replacement with no delimiter reads run formatting, not paragraph properties]
ROW edit.pasteSpecial | sel=b1:6-10 | params={"as":"text"} | holds=b1{text~,para,@};doc{styles} | windows=doc.body | facts=clipboard | reads=selection,pending,clipboard | note=no text param: session.clipboard plain text used [corrected C8-doc-6: a single-line source was an unjustified restriction; multiline clipboard text copies para and resolves style.next; cmd/paste.rs:112-116 and cmd/mod.rs:191-217 call split_para only for a paragraph delimiter] [review: handler premises and same-field protection independently checked]
ROW edit.pasteSpecial | sel=b1:6-10 | params={"as":"formatted"} | holds=b1{text~,para,@};doc{settings,revisions} | windows=doc.body | facts=clipboard,paraIds | reads=selection,clipboard | note=session.clipboard fragment inserted; all paragraph ids read to avoid duplicates; delete of 6..10 tracked if trackChanges [corrected C8-doc-6: allblocks{para} -> fact paraIds: crates/engine/doc/src/edit.rs:65-89 scans only paragraph and row ids; insert_fragment:509-511 uses the set to clear copied ids]
ROW edit.pasteSpecial | sel=b1:6-10 | params={"as":"html","html":"<p>a</p><p>b</p>"} | holds=b1{text~,para,@};doc{numbering,lists,styles,media,settings,revisions} | windows=doc.body,doc.numbering,doc.media | facts=paraIds | reads=selection | note=html imported, numbering/styles/media merged (existing ids read for max+1 and collisions), two paragraphs split into b1 at 6 [corrected C8-doc-6: allblocks{para} -> fact paraIds: crates/engine/doc/src/edit.rs:65-89 scans only paragraph and row ids; insert_fragment:509-511 uses the set to clear copied ids]
# ---- cmd/table.rs, table_style.rs, table_draw.rs
ROW table.autofit | sel=b4/0/1/0:0 | params={"mode":"window"} | holds=t4{props,grid,rows};r4/0{cells};r4/1{cells};c4/0/0{props};c4/0/1{props};c4/1/0{props};c4/1/1{props};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=grid length read; every cell's span read by sync_cell_widths, props.width written; section text width from caret's top-level block index [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it] [review: handler premises and same-field protection independently checked]
ROW table.autofit | sel=b4/0/1/0:0 | params={"mode":"fixed"} | holds=t4{props} | windows=doc.body | facts=none | reads=selection | note=writes props.fixed only; sect is computed but unused here [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it] [review: discarded content, returned reports and unused calculations checked]
ROW table.autofit | sel=b4/0/1/0:0 | params={"mode":"contents"} | holds=t4{props,grid,rows};r4/0{cells};r4/1{cells};c4/0/0{props,blocks};c4/0/1{props,blocks};c4/1/0{props,blocks};c4/1/1{props,blocks};b4/0/0/0{para,text};b4/0/1/0{para,text};b4/1/0/0{para,text};b4/1/1/0{para,text};doc{styles};sectionspan{para};doc{lastSection} | windows=doc.body | facts=fonts | reads=selection | note=text measured with the table style region formatting and fonts (layout measuring, not pagination); indent, margins, vmerge, textDirection of cells read [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it] [review: handler premises and same-field protection independently checked]
ROW table.borders | sel=b4/0/1/0:0 | params={"kind":"all"} | holds=t4{props,rows};r4/0{cells};r4/1{cells};c4/0/0{props,@};c4/0/1{props,@};c4/1/0{props,@};c4/1/1{props,@} | windows=doc.body,t4.rows | facts=none | reads=selection | note=whole-table path; "top/bottom/left/right/outside/inside" keep the other table sides (reads t4.props.borders); per-cell sides chosen by the cell's row/cell index
ROW table.borders | sel=b4/0/0/0:0-b4/0/1/0:2 | params={"kind":"outside","width":1} | holds=t4{rows};r4/0{cells};r4/1{cells};c4/0/0{props,@};c4/0/1{props,@};c4/1/0{props,@};c4/1/1{props,@} | windows=doc.body,t4.rows,r4/0.cells,r4/1.cells | facts=none | reads=selection,also | note=block path (row 0 selected is not "whole" since rows=2); neighbour cells below (row 1) get their top set; rectangle by cell index not grid column; "also"=anchor
ROW table.borders | sel=b3:0-b4/0/1/0:0 | params={"kind":"all"} | holds=t4{rows};r4/0{cells};c4/0/0{props,@} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=anchor outside the table: selected_cells empty, writes A1's borders (and its right/bottom neighbours' matching sides) instead of the caret cell's
ROW table.cellAlign | sel=b4/0/1/0:0 | params={"value":"center"} | holds=t4{rows};r4/0{cells};c4/0/1{blocks} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=caret cell only; writes c4/0/1.props.valign and b4/0/1/0.para align (the blocks list is iterated, paragraph content not read)
ROW table.cellAlign | sel=b4/0/0/0:0-b4/1/1/0:2 | params={"value":"bottomRight"} | holds=t4{rows};r4/0{cells};r4/1{cells};c4/0/0{blocks};c4/0/1{blocks};c4/1/0{blocks};c4/1/1{blocks} | windows=doc.body,t4.rows,r4/0.cells,r4/1.cells | facts=none | reads=selection,also | note=rectangle of selected cells by cell index; empty when the anchor is outside the table (no-op success)
ROW table.columnWidth | sel=b4/0/1/0:0 | params={"width":100} | holds=t4{props,grid,rows};r4/0{cells};r4/1{cells};c4/0/0{props};c4/0/1{props};c4/1/0{props};c4/1/1{props} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=c4/0/0 span read for grid_col; every cell's span read by sync; props.width of each cell written; anchor ignored (one column only)
ROW table.deleteCells | sel=b4/0/1/0:0 | params={} | holds=t4{rows};r4/0{cells};c4/0/1{*} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=deletes B1 only (anchor/selection ignored); row 0 then covers fewer grid columns than the grid (ragged table)
ROW table.deleteColumn | sel=b4/0/1/0:0 | params={} | holds=t4{grid,rows};r4/0{cells};r4/1{cells};c4/0/0{props};c4/0/1{*};c4/1/0{props};c4/1/1{*} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=reads spans of cells up to g in each row; removes B1 and B2 and grid[1]; selection beyond the caret ignored (one column)
ROW table.deleteColumn | sel=b4/0/0/0:0 | params={} | holds=t4{grid};doc.body{@} | windows=doc.body | facts=none | reads=selection | note=only on a 1-column table: removes block t4; remove_block appends an empty paragraph if the container ends with a table or becomes empty
ROW table.deleteRow | sel=b4/0/1/0:0 | params={} | holds=t4{rows};r4/0{*} | windows=doc.body,t4.rows | facts=none | reads=selection | note=caret row deleted
ROW table.deleteRow | sel=b4/0/0/0:0-b4/1/1/0:0 | params={} | holds=t4{rows};doc.body{@} | windows=doc.body | facts=none | reads=selection,also | note=both rows selected -> table removed
ROW table.deleteRow | sel=b3:0-b4/0/1/0:0 | params={} | holds=t4{rows} | windows=doc.body | facts=none | reads=selection | note=anchor outside table: deletes nothing but returns success (DEFECTS)
ROW table.deleteTable | sel=b4/0/1/0:0 | params={} | holds=t4{*};doc.body{@} | windows=doc.body | facts=none | reads=selection | note=deletes the innermost table holding the focus; appends an empty paragraph when the container would end with a table / be empty
ROW table.distributeColumns | sel=b4/0/1/0:0 | params={} | holds=t4{grid,rows};r4/0{cells};r4/1{cells};c4/0/0{props};c4/0/1{props};c4/1/0{props};c4/1/1{props} | windows=doc.body | facts=none | reads=selection | note=selection ignored (whole table); cell spans read, cell widths written
ROW table.distributeRows | sel=b4/0/1/0:0 | params={} | holds=t4{rows};r4/0{props};r4/1{props} | windows=doc.body | facts=none | reads=selection | note=reads only the stored heights, not laid-out heights; selection ignored
ROW table.formula | sel=b4/1/1/0:0 | params={} | holds=t4{rows};r4/0{cells};c4/0/1{blocks};b4/0/1/0{text};b4/1/1/0{text~} | windows=doc.body,t4.rows,r4/1.cells,r4/0.cells | facts=none | reads=selection,pending | note=ABOVE reads cells in the caret's cell-index column above; text~ = formatting continuing at the caret offset (typing_props); "B1" is not a number -> SUM of none; other references read any cell by (row, cell index)
ROW table.formula | sel=b4/1/1/0:0-b4/1/1/0:2 | params={"formula":"=SUM(LEFT)"} | holds=t4{rows};r4/1{cells};c4/1/0{blocks};b4/1/0/0{text};b4/1/1/0{text~};doc{settings} | windows=doc.body,t4.rows,r4/1.cells | facts=none | reads=selection,pending | note=selection "B2" deleted (tracked when settings.track_changes) before inserting; value computed from the table BEFORE the deletion; text~ = the range and the char at its start
ROW table.formula | sel=b3:0-b4/1/1/0:0 | params={} | holds=t4{rows};b3{text~};doc{settings} | windows=doc.body,t4.rows,r4/1.cells | facts=none | reads=selection,also,pending | note=value computed for B2 but the field lands at the start of the deleted range (outside the table) (DEFECTS)
ROW table.fromText | sel=b2:0-b3:3 | params={} | holds=b2{ALL};b3{ALL};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection,also | note=removes b2,b3 (numbered list membership lost), inserts a 2x1 table at index 2; section by focus block index [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it] [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW table.fromText | sel=b1:6-10 | params={"separator":"comma"} | holds=b1{ALL};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=whole b1 becomes a 1x1 table; bookmark Start and the footnote reference in b1's text are dropped (plain text) [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it] [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW table.fromText | sel=b3:0-b5:0 | params={} | holds=b3{ALL};b5{ALL};t4{ALL};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection,also | note=t4 in the range is silently deleted with its contents (DEFECTS) [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it] [review: discarded content, returned reports and unused calculations checked] [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW table.fromText | sel=b9:0 | params={} | holds=b9{ALL};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=last body paragraph: body ends with the new table, no trailing paragraph (DEFECTS); tracked insertion mark rev:0 flattened [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it] [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW table.insertColumnLeft | sel=b4/0/1/0:0 | params={} | holds=t4{grid,rows};r4/0{cells};r4/1{cells};c4/0/0{props};c4/1/0{props} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=spans of cells before g in every row read; new cells have no formatting copied from neighbours (Cell::empty); at 63 columns nothing inserted but grid still shrinks (DEFECTS)
ROW table.insertColumnRight | sel=b4/0/1/0:0 | params={} | holds=t4{grid,rows};r4/0{cells};r4/1{cells};c4/0/0{props};c4/0/1{props};c4/1/0{props};c4/1/1{props} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=c4/0/1 span read for the insert point; boundary search reads spans of the row's cells
ROW table.insertRowAbove | sel=b4/1/1/0:0 | params={} | holds=t4{rows};r4/1{props,cells};c4/1/0{props};c4/1/1{props} | windows=doc.body,t4.rows | facts=none | reads=selection | note=cell text formatting not copied (new empty paragraphs with default props)
ROW table.insertRowAbove | sel=b4/1/0/0:0-b4/0/1/0:0 | params={} | holds=t4{rows};r4/0{props,cells};c4/0/0{props};c4/0/1{props} | windows=doc.body,t4.rows | facts=none | reads=selection,also | note=2 rows inserted above the FOCUS row (row 0) copying row 0
ROW table.insertRowBelow | sel=b4/0/1/0:0 | params={} | holds=t4{rows};r4/0{props,cells};c4/0/0{props};c4/0/1{props} | windows=doc.body,t4.rows | facts=none | reads=selection | note=copies row r4/0 formatting
ROW table.insertRowBelow | sel=b4/1/0/0:0-b4/0/1/0:0 | params={} | holds=t4{rows};r4/0{props,cells};c4/0/0{props};c4/0/1{props} | windows=doc.body,t4.rows | facts=none | reads=selection,also | note=focus in row 0, anchor in row 1: 2 rows inserted after row 0 (between the selected rows), not below the selection (DEFECTS)
ROW table.look | sel=b4/0/1/0:0 | params={"headerRow":false} | holds=t4{props} | windows=doc.body | facts=none | reads=selection | note=reads current look flags for unspecified keys
ROW table.merge | sel=b4/0/0/0:0-b4/0/1/0:2 | params={} | holds=t4{rows};r4/0{cells};c4/0/0{props,blocks};c4/0/1{props,blocks};b4/0/0/0{text};b4/0/1/0{text} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection,also | note=paragraph emptiness read to drop empty paragraphs; spans summed
ROW table.merge | sel=b4/0/0/0:0-b4/1/0/0:2 | params={} | holds=t4{rows};r4/0{cells};r4/1{cells};c4/0/0{props,blocks};c4/1/0{props,blocks};b4/1/0/0{text} | windows=doc.body,t4.rows,r4/0.cells,r4/1.cells | facts=none | reads=selection,also | note=vertical merge: A2 content moved into A1, A1 Restart, A2 Continue
ROW table.merge | sel=b4/0/1/0:0 | params={} | holds=t4{*} | windows=doc.body | facts=none | reads=selection | note=caret only -> Failed, no write
ROW table.properties | sel=b4/0/1/0:0 | params={} | holds=t4{props,grid,rows};r4/0{props,cells};c4/0/0{props};c4/0/1{props} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=read-only branch
ROW table.properties | sel=b4/0/1/0:0 | params={"align":"center","indent":10,"width":300} | holds=t4{props} | windows=doc.body | facts=none | reads=selection | note=table settings only (touch_cells writes paragraphs)
ROW table.properties | sel=b4/0/1/0:0 | params={"rowHeightRule":"exact","headerRow":true,"cellWidth":50,"valign":"center"} | holds=t4{rows};r4/0{props,cells};c4/0/1{props} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=rule alone applies only when r4/0 already has a height (reads it); headerRow set on any row, even a non-top row
ROW table.properties | sel=b4/0/1/0:0 | params={"columnWidth":90} | holds=t4{props,grid,rows};r4/0{cells};r4/1{cells};c4/0/0{props};c4/0/1{props};c4/1/0{props};c4/1/1{props} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=same as table.columnWidth
ROW table.quick | sel=b1:6 (caret) | params={} | holds=b1{para,text~};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=b1 split at "Alpha |beta gamma." (split_off: para props copied to the tail; para id stays with the head), table inserted between; works in a cell too (nested table) or in a header part (story of focus, section still by body index) [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW table.quick | sel=b1:6-10 | params={"kind":"calendar"} | holds=b1{para,text~};doc{settings};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note="beta" deleted first (tracked if settings.track_changes), text~ = the range and the split point [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW table.quick | sel=b1:6 (caret) | params={"kind":"matrix"} | holds=b1{para,text~};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=style GridTable4AccentBlue referenced even if doc{styles} lacks it (dangling style id, DEFECTS) [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW table.repeatHeader | sel=b4/0/1/0:0 | params={} | holds=t4{rows};r4/0{props} | windows=doc.body,t4.rows | facts=none | reads=selection | note=toggles r4/0.header (reads current); with caret in r4/1 it marks row 1 a header although row 0 isn't (no top-rows check), selection ignored
ROW table.rowHeight | sel=b4/0/1/0:0 | params={"height":30} | holds=t4{rows};r4/0{*} | windows=doc.body,t4.rows | facts=none | reads=selection | note=caret row
ROW table.rowHeight | sel=b4/0/0/0:0-b4/1/1/0:0 | params={"height":30,"rule":"exact"} | holds=t4{rows};r4/0{*};r4/1{*} | windows=doc.body,t4.rows | facts=none | reads=selection,also | note=every selected row
ROW table.shading | sel=b4/0/1/0:0 | params={"color":"FF0000"} | holds=t4{rows};r4/0{cells};c4/0/1{*} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=caret cell; an unparsable colour silently clears shading
ROW table.shading | sel=b4/0/0/0:0-b4/1/1/0:0 | params={"color":null} | holds=t4{rows};r4/0{cells};r4/1{cells};c4/0/0{*};c4/0/1{*};c4/1/0{*};c4/1/1{*} | windows=doc.body,t4.rows,r4/0.cells,r4/1.cells | facts=none | reads=selection,also | note=rectangle by cell index; nothing when the anchor is outside the table
ROW table.sort | sel=b4/0/1/0:0 | params={} | holds=t4{rows};r4/0{props,cells,@};r4/1{cells,@};c4/0/0{blocks};c4/1/0{blocks};b4/0/0/0{text};b4/1/0/0{text} | windows=doc.body,t4.rows | facts=none | reads=selection | note=header flag of r4/0 read; rows reordered (vmerge groups and spans not considered; nested tables ignored in keys)
ROW table.sort | sel=b4/0/1/0:0 | params={"column":1,"descending":true,"header":false} | holds=t4{rows};r4/0{cells,@};r4/1{cells,@};c4/0/1{blocks};c4/1/1{blocks};b4/0/1/0{text};b4/1/1/0{text} | windows=doc.body,t4.rows,r4/0.cells,r4/1.cells | facts=none | reads=none | note=column is a cell index, not a grid column; selection only names the table
ROW table.split | sel=b4/0/0/0:0 | params={} | holds=t4{grid,rows};r4/0{cells};r4/1{cells};c4/0/0{props};c4/1/0{props} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=columns default 2; A2's row also gets a new empty cell, grid grows by one column (table widens) (DEFECTS)
ROW table.split | sel=b4/0/0/0:0 | params={"columns":1} | holds=t4{rows};r4/0{cells};c4/0/0{props} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=n<=1: no change, still touches paragraphs
ROW table.splitTable | sel=b4/1/0/0:0 | params={} | holds=t4{props,grid,rows};r4/1{@} | windows=doc.body,t4.rows | facts=none | reads=selection | note=split before r4/1; doc.body indexes after t4 shift by 2; header rows not repeated in the tail; vmerge Continue cells at the split point stay Continue
ROW table.splitTable | sel=b4/0/1/0:0 | params={} | holds=t4{*} | windows=doc.body,t4.rows | facts=none | reads=selection | note=row 0 -> Failed
ROW table.style | sel=b4/0/1/0:0 | params={"style":"Table Grid"} | holds=t4{*};doc{styles} | windows=doc.body | facts=none | reads=selection | note=writes t4.props.style; unknown style -> Params error
ROW table.textDirection | sel=b4/0/1/0:0 | params={"value":"down"} | holds=t4{rows};r4/0{cells};c4/0/1{*} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=caret cell
ROW table.textDirection | sel=b4/0/1/0:0 | params={} | holds=t4{rows};r4/0{cells};c4/0/1{props} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=cycles from the caret cell's direction
ROW table.textDirection | sel=b4/0/0/0:0-b4/1/1/0:0 | params={} | holds=t4{rows};r4/0{cells};r4/1{cells};c4/1/1{props};c4/0/0{*};c4/0/1{*};c4/1/0{*} | windows=doc.body,t4.rows,r4/0.cells,r4/1.cells | facts=none | reads=selection,also | note=next of the FOCUS cell's direction applied to all selected cells
ROW table.toText | sel=b4/0/1/0:0 | params={} | holds=t4{rows};r4/0{cells};r4/1{cells};c4/0/0{blocks};c4/0/1{blocks};c4/1/0{blocks};c4/1/1{blocks};b4/0/0/0{text};b4/0/1/0{text};b4/1/0/0{text};b4/1/1/0{text};doc.body{@} | windows=doc.body | facts=none | reads=selection | note=produces "A1\tB1","A2\tB2"; remove_block may first append an extra empty paragraph if the container would end with a table
ROW table.toText | sel=b4/0/1/0:0 | params={"separator":"paragraph"} | holds=t4{rows};r4/0{cells};r4/1{cells};c4/0/0{blocks};c4/0/1{blocks};c4/1/0{blocks};c4/1/1{blocks};b4/0/0/0{text};b4/0/1/0{text};b4/1/0/0{text};b4/1/1/0{text};doc.body{@} | windows=doc.body | facts=none | reads=selection | note=one paragraph per cell paragraph
ROW table.deleteStyle | sel=b4/0/1/0:0 | params={} | holds=t4{props};doc{styles};doc.body{*};doc.parts{*} | windows=doc.body | facts=none | reads=selection | note=target = t4's style; reads every table's props.style in body and all parts (walking cell blocks); fails on built-in Table Grid
ROW table.deleteStyle | sel=none | params={"style":"MyGrid"} | holds=doc{styles};doc.body{*};doc.parts{*} | windows=none | facts=none | reads=none | note=selection not read; returns deleted/tables/rebased
ROW table.modifyStyle | sel=b4/0/1/0:0 | params={"headerRow":{"bold":true}} | holds=t4{props};doc{styles} | windows=doc.body | facts=none | reads=selection | note=style taken from the caret table; existing style's table parts/chr read and overlaid
ROW table.modifyStyle | sel=none | params={"style":"MyGrid","name":"Grid2","basedOn":"Table Grid"} | holds=doc{styles} | windows=none | facts=none | reads=none | note=basedOn chain read for cycle check; name uniqueness across all styles
ROW table.modifyStyle | sel=b4/0/1/0:0 | params={"style":"MyGrid","apply":true} | holds=t4{*};doc{styles} | windows=doc.body | facts=none | reads=selection | note=apply writes t4.props.style (if the focus is in a table)
ROW table.newStyle | sel=b4/0/1/0:0 | params={"name":"MyGrid"} | holds=t4{*};doc{styles} | windows=doc.body | facts=none | reads=selection | note=apply default true writes t4.props.style
ROW table.newStyle | sel=b1:6-10 | params={"name":"MyGrid","basedOn":"Table Grid","wholeTable":{"fill":"EEEEEE"}} | holds=doc{styles} | windows=none | facts=none | reads=selection | note=focus not in a table: apply silently does nothing
ROW table.draw | sel=b1:6-10 | params={"page":0,"x0":100,"y0":200,"x1":300,"y1":260} | holds=bHit{text~};sectionspan{para};doc{lastSection} | windows=doc.body | facts=layout | reads=selection | note=paragraph under the start found by layout hit; its length read; text width from the CARET's section (DEFECTS) [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: sections only on top-level body paragraphs, Document::sections doc/src/lib.rs:736-747 and section_index_of:750-752] [corrected C8-doc-6: speculative b6 -> bHit and illustrative rectangle -> a 200x60 rectangle starting on b6's actual laid-out line outside the table; table_draw.rs:91-104 classifies from actual boxes and :137-144 reads layout.hit's paragraph, returning before text reads for a cell hit]
ROW table.draw | sel=none | params={"page":0,"x0":100,"y0":40,"x1":300,"y1":100,"story":{"part":0}} | holds=part:header/0{text~,@};sectionspan{para};doc{lastSection} | windows=none | facts=layout | reads=selection | note=story param picks the hit story; section still from focus block index [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it] [corrected C8-doc-6: bind story.part to the actual header part id, not literal 0; table_draw.rs:137-138 passes the requested story directly to layout.hit] [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW table.draw | sel=none | params={"page":0,"x0":200,"y0":100,"x1":200,"y1":300} | holds=t4{grid,rows};r4/0{cells};r4/1{cells};c4/0/0{props};c4/0/1{props};c4/1/0{props};c4/1/1{props} | windows=doc.body,t4.rows,r4/0.cells,r4/1.cells | facts=layout | reads=none | note=vertical line inside A1: table identified by layout cell boxes; vmerge/span read [corrected C8-doc-6: bind the illustrative coordinates to the actual A1/A2 cell centers and extents; table_draw.rs:91-104 classifies by actual boxes, :182-187 crosses their interiors]
ROW table.draw | sel=none | params={"page":0,"x0":100,"y0":150,"x1":300,"y1":150} | holds=t4{rows};r4/0{props,cells};c4/0/0{props};c4/0/1{props} | windows=doc.body,t4.rows,r4/0.cells | facts=layout | reads=none | note=horizontal line across row 0: row extent from layout, row props copied, cells' vmerge read and written [corrected C8-doc-6: bind coordinates to the actual first-row center and horizontal extents; table_draw.rs:91-104 classifies by actual boxes, :185-187 crosses their interiors]
ROW table.eraser | sel=none | params={"page":0,"x":200,"y":150} | holds=t4{grid,rows};r4/0{cells};c4/0/0{props,blocks};c4/0/1{props,blocks};b4/0/1/0{text} | windows=doc.body,t4.rows,r4/0.cells | facts=layout | reads=none | note=border between A1 and B1: cells named by laid-out (row, cell); B1 content appended to A1 [corrected C8-doc-6: point -> actual A1/B1 border midpoint; table_draw.rs:398-418 derives this vertical border from the two laid-out cells, so the same literal point cannot also name A1/A2]
ROW table.eraser | sel=none | params={"page":0,"x":200,"y":150} | holds=t4{rows};r4/0{cells};r4/1{cells};c4/0/0{props,blocks};c4/1/0{props,blocks};b4/0/0/0{text};b4/1/0/0{text} | windows=doc.body,t4.rows,r4/0.cells,r4/1.cells | facts=layout | reads=none | note=border between A1 and A2: grid col/span of both read, A2 content moved into A1 (emptiness of A1's single paragraph read) [corrected C8-doc-6: point -> actual A1/A2 border midpoint; table_draw.rs:419-434 derives this horizontal border from the two laid-out cells]
# ---- cmd/objects.rs, ink.rs, draw.rs
ROW arrange.align | sel=b7:12-15 | params={"value":"right"} | holds=b7{text~};inline:pic{value};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=text~: OBJ count before off 12 locates the object; value: w + float(wrap,dist) read, float written; section = focus block b7's section (page width/margins), allblocks{para} for section breaks [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW arrange.align | sel=b8:10-13 | params={"value":"center"} | holds=b8{text~};inline:shape{frame};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=shape: w and float in `frame`; writes frame.float [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW arrange.align | sel=b1:6-10 | params={} | holds=b1{text~} | windows=doc.body | facts=none | reads=selection | note=no drawing in range: Disabled "select a picture or shape first"; reads only the OBJ offsets of b1 (bookmark/footnote markers are not drawings)
ROW arrange.bounds | sel=b7:12-15 | params={"width":100,"height":50} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection | note=reads w,h (and nothing else for a picture); writes value.w/h; reselects the object
ROW arrange.bounds | sel=b8:10-13 | params={"width":100} | holds=b8{text~};inline:shape{frame,look} | windows=doc.body | facts=none | reads=selection | note=frame w,h; look.story (text box?) for the 18pt minimum; height kept from frame
ROW arrange.bounds | sel=b7:12-15 | params={"x":100,"y":300,"page":0} | holds=b7{text~,@};inline:pic{value};bK{text~,@} | windows=doc.body | facts=layout | reads=selection,page,view | note=bK = top-level body paragraph under y on page 0 per layout; OBJ removed from b7 and inserted at bK off 0 with default char props; same paragraph -> stays [corrected C8-doc-6: remove allblocks and root layout inputs, destination * -> text; objects.rs:542-553 reads the cached layout and anchor, :555-570 reads/moves only the source/destination text, :588-595 consumes the resulting origin; no destination paragraph formatting is read]
ROW arrange.bounds | sel=part:header/0:4-7 | params={"x":10,"y":10} | holds=part:header/0{text~,@};inline:X{frame} | windows=none | facts=none | reads=selection | note=object in a header (or table cell, depth>0): Disabled "only objects in the body text can be moved" (any resize before it rolled back) [corrected C8-doc-6: part:header{blocks} -> selected paragraph text; objects.rs:381-398 locates the selected object, move_object:538-540 returns on a part before layout or other paragraph content is read] [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW arrange.bringForward | sel=b7:12-15 | params={} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection | note=reads float.wrap and float.dist (was-inline check), writes float.wrap/dist
ROW arrange.bringForward | sel=b8:10-13 | params={} | holds=b8{text~};inline:shape{frame} | windows=doc.body | facts=none | reads=selection | note=frame.float written
ROW arrange.group | sel=b7:12-15 | params={} | holds=b7{text~,@};b8{text~,@};inline:pic{value};inline:shape{*} | windows=doc.body | facts=none | reads=selection,also | note=also=[b8:10]; fixture objects are inline -> Disabled "objects in line with text can't be grouped" [corrected C8-doc-6: remove allblocks and root layout inputs; objects.rs:953 gets the cached layout, :960-966 checks the picked objects and returns on the first inline member; no raw layout inputs are consumed] [review: handler premises and same-field protection independently checked]
ROW arrange.group | sel=b7:12-15 | params={} | holds=b7{text~,@};b8{text~,@};inline:pic{value};inline:shape{frame,look};bK{text~,@} | windows=doc.body | facts=layout | reads=selection,also,page,view | note=both floating, same page: whole members copied into the group; members deleted, group inserted at b7:12 then moved to the paragraph under the box top [corrected C8-doc-6: remove allblocks and root layout inputs, destination * -> text; objects.rs:953-981 reads member layout frames, :988-1008 copies members and moves their character using move_object:542-595; no destination paragraph formatting is read]
ROW arrange.group | sel=b7:12-15 | params={} | holds=b7{text~} | windows=doc.body | facts=none | reads=selection,also | note=also empty -> Disabled (can_group) without reading objects
ROW arrange.nudge | sel=b7:12-15 | params={"dx":6} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection | note=fixture picture is inline -> Disabled "select a floating picture or shape first"; reads float.wrap
ROW arrange.nudge | sel=b8:10-13 | params={"dx":6,"dy":-2} | holds=b8{text~};inline:shape{frame} | windows=doc.body | facts=none | reads=selection | note=floating, no align: reads frame.float x,y, writes them
ROW arrange.nudge | sel=b8:10-13 | params={"dx":6} | holds=b8{text~};inline:shape{frame} | windows=doc.body | facts=layout | reads=selection,page,view | note=floating with h_align/v_align: laid-out position converts alignment to offsets first [corrected C8-doc-6: remove allblocks and root layout inputs; objects.rs:604-622 unalign consumes only the selected object's laid-out rect/origin and updates its float]
ROW arrange.position | sel=b7:12-15 | params={"preset":"topRight"} | holds=b7{text~};inline:pic{value};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=reads w,h,float.wrap,dist; text width/height of the FOCUS block's section [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW arrange.position | sel=b8:10-13 | params={"x":10,"y":20} | holds=b8{text~};inline:shape{frame} | windows=doc.body | facts=none | reads=selection | note=both x,y given: section still computed (closure reads it only for the preset fallback); frame.float written [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it] [review: discarded content, returned reports and unused calculations checked]
ROW arrange.rotate | sel=b7:12-15 | params={"direction":"right"} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection | note=reads float.rot/flipH/flipV (+wrap,dist for the was-inline check, unchanged); writes float.rot/flips
ROW arrange.rotate | sel=b8:10-13 | params={"direction":"flipVertical"} | holds=b8{text~};inline:shape{frame} | windows=doc.body | facts=none | reads=selection | note=frame.float
ROW arrange.rotation | sel=b7:12-15 | params={"degrees":45} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection | note=reads current flips (when flipH/flipV omitted); writes float.rot/flips
ROW arrange.rotation | sel=b8:10-13 | params={"degrees":45,"flipH":true,"flipV":false} | holds=b8{text~};inline:shape{frame} | windows=doc.body | facts=none | reads=selection | note=both flips given: rot/flips written without reading them (only wrap/dist read by with_float)
ROW arrange.sendBackward | sel=b7:12-15 | params={} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection | note=float.wrap/dist read, written
ROW arrange.sendBackward | sel=b8:10-13 | params={} | holds=b8{text~};inline:shape{frame} | windows=doc.body | facts=none | reads=selection | note=frame.float
ROW arrange.ungroup | sel=b7:12-15 | params={} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection | note=fixture has no group: Disabled after reading the object's type
ROW arrange.ungroup | sel=bG:o-o+3 | params={} | holds=bG{text~,@};inline:group{frame,value} | windows=doc.body | facts=layout | reads=selection,page,view | note=floating group: whole group read; layout only when aligned; writes members, sel and also_selected [corrected C8-doc-6: remove allblocks and root layout inputs; objects.rs:1030 calls unalign:604-622, which consumes only the group's laid-out rect/origin]
ROW arrange.ungroup | sel=bG:o-o+3 | params={} | holds=bG{text~,@};bK{text~,@};inline:group{frame,value} | windows=doc.body | facts=layout | reads=selection,page,view | note=inline group: move_object first (OBJ may move to paragraph bK under its top) [corrected C8-doc-6: remove allblocks and root layout inputs, destination * -> text; objects.rs:1027-1028 consumes the laid-out group frame then move_object:542-595 moves its text anchor; no destination paragraph formatting is read]
ROW arrange.wrap | sel=b7:12-15 | params={"wrap":"square"} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection | note=reads float.wrap,dist; writes them
ROW arrange.wrap | sel=b8:10-13 | params={"wrap":"behindText"} | holds=b8{text~};inline:shape{frame} | windows=doc.body | facts=none | reads=selection | note=frame.float
ROW picture.altText | sel=b7:12-15 | params={"text":"A"} | holds=b7{text~};inline:pic{*} | windows=doc.body | facts=none | reads=selection | note=writes value.alt without reading it (only the object type read)
ROW picture.altText | sel=b8:10-13 | params={"text":"A"} | holds=b8{text~};inline:shape{look} | windows=doc.body | facts=none | reads=selection | note=shape: Disabled "select a picture or chart first"
ROW picture.border | sel=b7:12-15 | params={"color":"FF0000","width":4} | holds=b7{text~};inline:pic{value};media:0{key,blob};doc{media} | windows=doc.body | facts=none | reads=selection,originals | note=value.media read -> media:0 blob decoded; doc{media}: add_media scans all blobs for an identical one and picks a free key; writes value.media + new media + session.originals
ROW picture.border | sel=b8:10-13 | params={} | holds=b8{text~};inline:shape{look} | windows=doc.body | facts=none | reads=selection | note=not a picture: Disabled
ROW picture.change | sel=b7:12-15 | params={"data":"<base64>"} | holds=b7{text~};inline:pic{*};doc{media} | windows=doc.body | facts=none | reads=selection | note=old media not read; value.media overwritten; doc{media} dedupe/key scan
ROW picture.change | sel=b7:12-15 | params={"path":"x.png"} | holds=b7{text~};inline:pic{*};doc{media} | windows=doc.body | facts=file | reads=selection | note=reads an external file through the host (hosted) or the local disk
ROW picture.color | sel=b7:12-15 | params={"mode":"sepia"} | holds=b7{text~};inline:pic{value};media:0{key,blob};doc{media} | windows=doc.body | facts=none | reads=selection,originals | note=as picture.border
ROW picture.color | sel=b1:6-10 | params={"mode":"grayscale"} | holds=b1{text~} | windows=doc.body | facts=none | reads=selection | note=no picture: Disabled
ROW picture.compress | sel=b7:12-15 | params={"maxPixels":800} | holds=b7{text~};inline:pic{value};media:0{key,blob};doc{media} | windows=doc.body | facts=none | reads=selection,originals | note=as picture.border
ROW picture.corrections | sel=b7:12-15 | params={"brightness":20} | holds=b7{text~};inline:pic{value};media:0{key,blob};doc{media} | windows=doc.body | facts=none | reads=selection,originals | note=as picture.border; all-zero params still re-encode
ROW picture.crop | sel=b7:12-15 | params={"left":0.1} | holds=b7{text~};inline:pic{*} | windows=doc.body | facts=none | reads=selection | note=crop written whole without reading old crop (omitted sides reset to 0)
ROW picture.effects | sel=b7:12-15 | params={"effect":"invert"} | holds=b7{text~};inline:pic{value};media:0{key,blob};doc{media} | windows=doc.body | facts=none | reads=selection,originals | note=as picture.border
ROW picture.removeBackground | sel=b7:12-15 | params={"tolerance":30} | holds=b7{text~};inline:pic{value};media:0{key,blob};doc{media} | windows=doc.body | facts=none | reads=selection,originals | note=as picture.border
ROW picture.reset | sel=b7:12-15 | params={} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection,originals | note=reads value.media to look up the session's originals map; writes media + crop
ROW picture.reset | sel=b8:10-13 | params={} | holds=b8{text~};inline:shape{look} | windows=doc.body | facts=none | reads=selection | note=Disabled (not a picture)
ROW picture.size | sel=b7:12-15 | params={"width":100} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection | note=reads w,h; writes w,h
ROW picture.size | sel=b8:10-13 | params={"width":100,"height":50} | holds=b8{text~};inline:shape{frame} | windows=doc.body | facts=none | reads=selection | note=shape too (no text-box minimum here, unlike arrange.bounds)
ROW picture.size | sel=bG:o-o+3 | params={"width":100} | holds=bG{text~};inline:group{frame} | windows=doc.body | facts=none | reads=selection | note=group passes the guard but the handler's match skips Group: success, nothing written (DEFECT)
ROW picture.style | sel=b7:12-15 | params={"style":"rounded"} | holds=b7{text~};inline:pic{value};media:0{key,blob};doc{media} | windows=doc.body | facts=none | reads=selection,originals | note=as picture.border
ROW picture.transparency | sel=b7:12-15 | params={"percent":40} | holds=b7{text~};inline:pic{value};media:0{key,blob};doc{media} | windows=doc.body | facts=none | reads=selection,originals | note=as picture.border
ROW shape.change | sel=b8:10-13 | params={"kind":"ellipse"} | holds=b8{text~};inline:shape{*} | windows=doc.body | facts=none | reads=selection | note=writes look.kind without reading it (type check only)
ROW shape.change | sel=b7:12-15 | params={"kind":"ellipse"} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection | note=picture: Disabled "select a shape first"
ROW shape.effects | sel=b8:10-13 | params={"shadow":"offsetBottomRight"} | holds=b8{text~};inline:shape{look,frame} | windows=doc.body | facts=none | reads=selection,also | note=reads look.effects (glow/softEdge kept), writes look.effects and frame.float.effect
ROW shape.effects | sel=b0:0-b9:end | params={"glow":4} | holds=bodyparas{text~};inline:shape{look,frame} | windows=doc.body | facts=none | reads=selection | note=range: every shape in every paragraph b0..b9 (incl. table cells) is a target; pictures skipped; inline:pic not touched [corrected C8-doc-6: allblocks -> bodyparas; objects.rs:383-389 scans only the selection's story through paths_between, never header/footer/note/comment paragraphs]
ROW shape.effects | sel=b7:12-15 | params={"softEdge":2} | holds=b7{text~};inline:pic{value} | windows=doc.body | facts=none | reads=selection,also | note=guard has_shape fails (first selected is the picture) -> Disabled even with also=[b8:10] [corrected C8-doc-6: remove b8 and shape fields; objects.rs:59-64 has_shape checks only the first selected object and the .when guard at objects.rs:350 prevents entering shape_effects]
ROW shape.fill | sel=b8:10-13 | params={"color":"FF0000"} | holds=b8{text~};inline:shape{*} | windows=doc.body | facts=none | reads=selection | note=writes look.fill without reading it
ROW shape.fill | sel=b8:10-13 | params={"color":"red"} | holds=b8{text~};inline:shape{*} | windows=doc.body | facts=none | reads=selection | note=bad colour silently clears the fill (no Params error)
ROW shape.outline | sel=b8:10-13 | params={"color":"000000","width":2,"dash":"dot"} | holds=b8{text~};inline:shape{*} | windows=doc.body | facts=none | reads=selection | note=writes look.stroke/stroke_width/dash without reading them
ROW shape.outline | sel=b8:10-13 | params={"width":2} | holds=b8{text~};inline:shape{*} | windows=doc.body | facts=none | reads=selection | note=color omitted: outline colour cleared to none as a side effect of changing width
ROW drawing.ink | sel=none | params={"placement":{"page":0},"strokes":[{"tool":"pen","color":"C00000","width":2,"points":[[100,100],[200,100]]}]} | holds=bK{text~,@};allinline{uid} | windows=doc.body | facts=layout,inkCount,inkIds | reads=view | note=agent path; bK under the first point's y on page 0 [corrected C8-doc-6: raw allblocks/root layout inputs -> layout,inkCount,inkIds and the drawing id scan; destination * -> text; draw.rs:255-279 reads page geometry/count, :309-338 inserts only at its anchor; ink.rs:108-115 reads ink ids and assigns object ids across stories]
ROW drawing.ink | sel=b1:6-10 | params={"strokes":[{"points":[[100,100]]}]} | holds=bK{text~,@};allinline{uid} | windows=doc.body | facts=layout,inkCount,inkIds | reads=selection,page,view,draw | note=person: page from the caret; pen from session view.draw [corrected C8-doc-6: raw allblocks/root layout inputs -> layout,inkCount,inkIds and drawing id scan; destination * -> text; ink.rs:60-70 resolves the caret page, draw.rs:255-338 consumes page/count/anchor, ink.rs:108-115 reads ids]
ROW drawing.ink | sel=none | params={"strokes":[{"points":[[1,1]]}]} | holds=none | windows=none | facts=none | reads=origin | note=agent without tool/color/width or page: Params, nothing read
ROW drawing.erase | sel=none | params={"placement":{"page":0},"path":[[150,80],[150,120]]} | holds=none | windows=none | facts=layout,inkGeometry | reads=view | note=fixture has no ink -> erased 0, no write [corrected C8-doc-6: inkN is the set of ink strokes, empty here, not a literal object; no bK deletion anchor or body window exists; ink.rs:147-156 consumes laid-out strokes/geometry, :160-172 deletes only actual hits; raw layout inputs are not read]
ROW drawing.erase | sel=b1:6-10 | params={"x":10,"y":10} | holds=none | windows=none | facts=layout | reads=selection,page,view | note=person without page: the caret's page; view.hide_ink true -> nothing erased [corrected C8-doc-6: remove raw allblocks/root layout inputs and body window; ink.rs:150 gets the page then :151-153 returns while hidden before any stroke/content read]
ROW drawing.erase | sel=none | params={"x":10,"y":10} | holds=none | windows=none | facts=none | reads=origin | note=agent without page: Params
ROW draw.erase | sel=b1:6-10 | params={"page":0,"x":150,"y":101} | holds=none | windows=none | facts=layout,inkGeometry | reads=selection,view | note=fixture has no ink -> Failed "no ink stroke there" [corrected C8-doc-6: inkN is the empty ink set; remove nonexistent deletion anchor/window and raw layout inputs; draw.rs:189-217 tests only laid-out ink freeform/width, :234 returns before deletion when none is hit]
ROW draw.stroke | sel=b1:6-10 | params={"page":0,"points":[[100,100],[200,100]],"tool":"pen","color":"C00000","width":2} | holds=bK{text~,@} | windows=doc.body | facts=layout,inkCount | reads=selection,view | note=bK under y=100 on page 0; selection offsets adjusted; existing ink count read [corrected C8-doc-6: raw allblocks/root layout inputs -> layout,inkCount, destination * -> text; draw.rs:255-279 reads page geometry and ink count, :309-338 inserts at the resolved anchor without reading paragraph formatting]
ROW draw.stroke | sel=b1:6-10 | params={"page":0,"points":[[100,100]]} | holds=bK{text~,@} | windows=doc.body | facts=layout,inkCount | reads=selection,view,draw | note=tool/color/width from session view.draw [corrected C8-doc-6: same reads as the explicit pen path, plus view.draw defaults; draw.rs:250-257 resolves those defaults and :255-338 consumes only page/count/anchor]
# ---- cmd/insert.rs, references.rs
ROW insert.pageBreak | sel=b1:6-10 | params={} | holds=b1{text~};doc{settings} | windows=doc.body | facts=none | reads=selection,pending | note=deletes 6..10 then inserts U+000C with the formatting of the last selected char (or pending); tracked when settings.track_changes (then also doc.revisions@, author, time)
ROW insert.pageBreak | sel=b1:6 (caret) | params={} | holds=b1{text~};doc{settings} | windows=doc.body | facts=none | reads=selection,pending | note=formatting of the char before the caret
ROW insert.blankPage | sel=b1:6-10 | params={} | holds=b1{text~,para};doc{settings,styles} | windows=doc.body | facts=none | reads=selection,pending | note=text: formatting of last selected char; para: style (+styles.next when caret at end) and numbering; splits b1 after the first \f, inserts new block at b1+1 (doc.body shifts)
ROW insert.blankPage | sel=b2:3 (caret, end of list item "One") | params={} | holds=b2{text~,para};doc{settings,styles} | windows=doc.body | facts=none | reads=selection,pending | note=at paragraph end split_para applies the style's next style; new paragraph copies b2's numbering via split_off
ROW insert.coverPage | sel=b1:6-10 | params={} | holds=doc{core,lastSection};doc.body{@} | windows=doc.body | facts=none | reads=selection,author | note=title/subtitle/author fall back to core.title/subject/creator then session.author; size/spacing from first section page_h, margin_top/bottom; blocks inserted before b0; selection shifted by n (also_selected, body_return, page_hint, column NOT shifted)
ROW insert.coverPage | sel=b1:6-10 | params={"title":"T","subtitle":"S","author":"A","date":"D"} | holds=doc{lastSection};doc.body{@} | windows=doc.body | facts=none | reads=selection | note=all text from params; only the page size/margins of the first section read
ROW insert.table | sel=b1:6-10 | params={"rows":2,"cols":2} | holds=b1{text~};doc{settings,lastSection} | windows=doc.body | facts=none | reads=selection | note=deletes "beta" (tracked if settings say), splits b1 at 6, table inserted at body index 2 (b2.. shift); width from first section; split_paragraph is raw (no style-next, untracked)
ROW insert.table | sel=b1:0 (caret) | params={"rows":2,"cols":2,"style":"Table Grid"} | holds=b1{*};doc{styles,lastSection} | windows=doc.body | facts=none | reads=selection | note=off 0: table inserted before b1 without touching b1; style resolved by id or name
ROW insert.table | sel=b4/0/1/0:0 | params={"rows":1,"cols":1} | holds=b4/0/1/0{*};c4/0/1{blocks@};doc{lastSection} | windows=t4.rows,r4/0.cells,c4/0/1.blocks | facts=none | reads=selection | note=nested table inside the cell before its paragraph
ROW insert.picture | sel=b1:6-10 | params={"data":"<png>","width":100,"alt":"x"} | holds=b1{text~};doc{settings,lastSection,media} | windows=doc.body | facts=none | reads=selection,pending | note=object gets formatting of the last selected char incl. its ins/del marks; media deduped against existing blobs (media:0 reused when same bytes)
ROW insert.picture | sel=b1:6 (caret) | params={"path":"/x.png"} | holds=b1{text~};doc{lastSection,media} | windows=doc.body | facts=file | reads=selection,pending | note=bytes from file (host-lent in hosted session)
ROW insert.picture | sel=b1:6-10 | params={} | holds=none | windows=none | facts=none | reads=none | note=no data/path: pushes ui_requests {open: insertPicture}, document untouched
ROW insert.removeFooter | sel=none | params={} | holds=doc{lastSection*} | windows=none | facts=none | reads=none | note=overwrites lastSection.footers; part:footer stays in doc.parts (orphan); earlier sections' footers not cleared
ROW insert.removeHeader | sel=none | params={} | holds=doc{lastSection*};bodyparas{para} | windows=none | facts=none | reads=none | note=scans every body paragraph for a section break and clears its headers; part:header stays in doc.parts (orphan) [corrected C8-doc-6: allblocks{para} -> bodyblocks{para}: insert.rs:65-73 uses recursive para_paths(Body), including table cells] [review: handler premises and same-field protection independently checked]
ROW insert.removeLink | sel=b1:6-10 | params={} | holds=b1{text~} | windows=doc.body | facts=none | reads=selection | note=run props of 6..10 rewritten (link, style if == Hyperlink)
ROW insert.removeLink | sel=b1:7 (caret) | params={} | holds=b1{text~} | windows=doc.body | facts=none | reads=selection | note=caret inside "beta": text around the caret decides the word range 6..10, formatted
ROW insert.removeLink | sel=b5:0 (caret) | params={} | holds=b5{text~,para*} | windows=doc.body | facts=none | reads=selection,pending | note=no word: pending = typing_props with link cleared; empty paragraph: its mark too
ROW insert.shape | sel=b1:6-10 | params={"kind":"ellipse","width":50} | holds=b1{text~};doc{settings} | windows=doc.body | facts=none | reads=selection,pending | note=deletes the selection, object gets the last selected char's formatting (incl. ins/del marks); selection set over the new object
ROW insert.shape | sel=b9:16 (caret inside rev:0 " Added") | params={"kind":"rectangle"} | holds=b9{text~} | windows=doc.body | facts=none | reads=selection,pending | note=object inherits ins=rev:0 from the char before (no new revision even when tracking)
ROW insert.symbol | sel=b1:6-10 | params={"char":"♥"} | holds=b1{text~};doc{settings} | windows=doc.body | facts=none | reads=selection,pending | note=as typing
ROW insert.symbol | sel=b1:6 (caret) | params={"char":"♥","font":"Symbol"} | holds=b1{text~};doc{settings} | windows=doc.body | facts=none | reads=selection,pending | note=writes session.pending (left = caret formatting without the font)
ROW insert.textBox | sel=b1:6-10 | params={"text":"hi"} | holds=b1{text~};doc{settings,parts} | windows=doc.body | facts=none | reads=selection,pending | note=parts read for the guard (focus story kind) and new id = max id + 1; anchor gets last selected char's formatting
ROW insert.textBox | sel=part:header/0:0 | params={} | holds=part:header{kind,blocks@};doc{parts} | windows=part:header.blocks | facts=none | reads=selection,pending | note=allowed in a header (only TextBox parts refused)
ROW insert.textFromFile | sel=b1:6-10 | params={"path":"/x.docx"} | holds=b1{text~,para};doc{settings,parts} | windows=doc.body | facts=file | reads=selection | note=b1 split at 6; first fragment para merges into b1 (b1 props kept unless empty of formatting), last merges with b1's tail; ids in use read (para_ids_in_use over all paragraphs) to drop copied ids
ROW insert.textFromFile | sel=b4/0/1/0:0 | params={"path":"/x.txt"} | holds=b4/0/1/0{text~,para} | windows=t4.rows,r4/0.cells,c4/0/1.blocks | facts=file | reads=selection | note=single paragraph inline insert
ROW insert.artText | sel=b1:6-10 | params={"text":"Hi","warp":"textWave1"} | holds=b1{text~};doc{settings,parts};allinline{uid} | windows=doc.body | facts=none | reads=selection,pending | note=selection deleted by nested insert.textBox; assign_object_ids walks every object of the document; shapes() scan finds the anchor [corrected C8-doc-6: allblocks{text} -> allinline{uid}; insert.rs:90-104 delegates to drawing.insert; doc/src/lib.rs:1382-1419 reads numbering ids, not unrelated paragraph text]
ROW insert.artText | sel=b7:12 (caret right after inline:pic) | params={} | holds=b7{text~};doc{parts};allinline{uid} | windows=doc.body | facts=none | reads=selection,pending | note=box inserted through drawing.insert with default text [corrected C8-doc-6: include the id scan on this path too; insert.rs:90-104 delegates to drawing.insert even without text/warp params; doc/src/lib.rs:1382-1419 reads numbering ids]
ROW insert.bookmark | sel=b1:6-10 | params={"name":"Mark"} | holds=b1{text~} | windows=doc.body | facts=bookmarks | reads=selection | note=reads every body paragraph's objects for same-name bookmarks; inserts objects at 10 then 6 [corrected C8-doc-6: Document::bookmarks, doc/src/lib.rs:958-971, consumes only body BookmarkStart names and offsets; parts and other text are not read; name/anchor index is the bookmarks fact]
ROW insert.bookmark | sel=b1:6-10 | params={"name":"Start"} | holds=b1{text} | windows=doc.body | facts=bookmarks | reads=selection | note=replaces bookmark Start: deletes its start (b1:0) and end objects in b1, then inserts at the stale offsets 6/10 (shifted by the removed objects) [corrected C8-doc-6: Document::bookmarks, doc/src/lib.rs:958-971, consumes only body BookmarkStart names and offsets; cmd/insert.rs:360-365 reads text only in paragraphs containing the matching bookmark, here b1]
ROW insert.dateTime | sel=b1:6-10 | params={"format":"M/d/yyyy"} | holds=b1{text~};doc{settings} | windows=doc.body | facts=time | reads=selection,pending | note=typed text, tracked if settings say
ROW insert.dateTime | sel=b1:6 (caret) | params={"update":true} | holds=b1{text~};doc{settings} | windows=doc.body | facts=time | reads=selection,pending | note=field object with caret formatting (ins/del marks kept, not tracked)
ROW insert.dropCap | sel=b1:6-10 | params={"lines":3} | holds=b1{para*} | windows=doc.body | facts=none | reads=selection | note=sets para.drop_cap only
ROW insert.dropCap | sel=b1:0-b3:1 | params={"lines":0} | holds=b1{para*};b2{para*};b3{para*} | windows=doc.body | facts=none | reads=selection | note=every paragraph between anchor and focus
ROW insert.editFooter | sel=b1:6-10 | params={} | holds=doc{lastSection,parts} | windows=none | facts=none | reads=selection | note=section chosen by sel.focus.path[0]; part:footer exists -> no write, caret to part:footer end
ROW insert.editFooter | sel=b1:6-10 | params={} | holds=doc{lastSection,parts} | windows=none | facts=none | reads=selection | note=no footer (or dangling id): adds a Footer part and writes lastSection.footers.default despite being pure
ROW insert.editHeader | sel=b1:6-10 | params={} | holds=doc{lastSection,parts} | windows=none | facts=none | reads=selection | note=as editFooter for headers.default; part:header exists -> caret to its end ("Head")
ROW insert.editHeader | sel=part:footnote/0:0 | params={} | holds=doc{lastSection,parts} | windows=none | facts=none | reads=selection | note=focus in a part: its path[0] (0) is taken as a body block index -> section of b0
ROW insert.field | sel=b1:6-10 | params={"instr":"PAGE","result":"1"} | holds=b1{text~};doc{settings,core,sources,styles,numbering,lists};allblocks{text,para} | windows=doc.body | facts=layout,time | reads=selection,pending,bibStyle | note=insert then update_fields: every body field (DATE/SEQ/REF/PAGEREF/TITLE/AUTHOR/NUMWORDS/=formula), citations and the TOC rebuilt
ROW insert.field | sel=b1:6 (caret) | params={"instr":"REF Start"} | holds=b1{text};allblocks{text,para};doc{core} | windows=doc.body | facts=layout,time | reads=selection,pending | note=its result becomes bookmark Start's text ("Alpha") via update_fields; user `result` overwritten
ROW insert.footer | sel=b1:6-10 | params={"text":"F"} | holds=doc{lastSection,parts};part:footer{*} | windows=none | facts=none | reads=selection | note=section by focus block; replaces all of part:footer's blocks ("Foot" lost)
ROW insert.footer | sel=b1:6-10 | params={"preset":"pageNumber"} | holds=doc{lastSection,parts};part:footer{*} | windows=none | facts=none | reads=selection | note=one centered para with PAGE field (result "1")
ROW insert.footer | sel=b1:6-10 | params={} | holds=doc{lastSection,parts} | windows=none | facts=none | reads=selection | note=only creates the footer if missing; caret moves into it
ROW insert.header | sel=b1:6-10 | params={"text":"H"} | holds=doc{lastSection,parts};part:header{*} | windows=none | facts=none | reads=selection | note=replaces part:header blocks
ROW insert.header | sel=b1:6-10 | params={"preset":"title"} | holds=doc{lastSection,parts,core};part:header{*} | windows=none | facts=none | reads=selection | note=text = core.title or "[Document title]", centered
ROW insert.header | sel=b1:6-10 | params={"preset":"blankThree"} | holds=doc{lastSection,parts};part:header{*} | windows=none | facts=none | reads=selection | note=fixed text with tabs
ROW insert.horizontalLine | sel=b1:6-10 | params={} | holds=b1{para*} | windows=doc.body | facts=none | reads=selection | note=whole para.borders overwritten, nothing read
ROW insert.link | sel=b1:6-10 | params={"url":"https://x"} | holds=b1{text~} | windows=doc.body | facts=none | reads=selection | note=run props of 6..10 rewritten; para mark too if fully covered
ROW insert.link | sel=b1:6 (caret) | params={"url":"https://x","text":"x"} | holds=b1{text~} | windows=doc.body | facts=none | reads=selection,pending | note=formatting of char before caret (incl. ins/del, not tracked); writes pending=default
ROW insert.link | sel=b1:6-10 | params={} | holds=none | windows=none | facts=none | reads=none | note=ui_requests {open: link} only
ROW insert.pageNumber | sel=b1:6-10 | params={} | holds=doc{lastSection,parts};part:footer{*} | windows=none | facts=none | reads=selection | note=bottom default: replaces whole part:footer ("Foot" lost); section from focus block
ROW insert.pageNumber | sel=b1:6-10 | params={"position":"top","format":"x of y"} | holds=doc{lastSection,parts};part:header{*} | windows=none | facts=none | reads=selection | note=replaces part:header
ROW insert.pageNumber | sel=b1:6-10 | params={"position":"current","align":"right"} | holds=b1{text~,para*};doc{settings} | windows=doc.body | facts=none | reads=selection,pending | note=caret formatting (ins/del kept); b1.para.align set
ROW references.addText | sel=b1:6-10 | params={"level":1} | holds=b1{para*} | windows=doc.body | facts=none | reads=selection | note=sets outline_level only
ROW references.caption | sel=b1:6-10 | params={"label":"Figure","text":"x"} | holds=b1{text~,para};allblocks{para,text} | windows=doc.body | facts=none | reads=selection | note=reads style + text prefix of every body para before b1; split at b1 end inserts caption at body index 2 (b2.. shift); b1's section moves with split_off to the caption
ROW references.caption | sel=b4/1/0/0:0 | params={"label":"Table"} | holds=b4/1/0/0{text~};allblocks{para,text} | windows=t4.rows,r4/1.cells,c4/1/0.blocks | facts=none | reads=selection | note=caption para inside the cell after A2
ROW references.caption | sel=part:header/0:0 | params={} | holds=part:header{blocks@};allblocks{para,text} | windows=part:header.blocks | facts=none | reads=selection | note=count compares body paths with a part path
ROW references.endnote | sel=b1:6-10 | params={"text":"n"} | holds=b1{text~};doc{settings,parts} | windows=doc.body | facts=none | reads=selection | note=see footnote
ROW references.endnote | sel=b1:6 (caret) | params={} | holds=b1{text~};doc{parts} | windows=doc.body | facts=none | reads=selection | note=writes body_return, caret into note
ROW references.footnote | sel=b1:6-10 | params={"text":"n"} | holds=b1{text~};doc{settings,parts} | windows=doc.body | facts=none | reads=selection | note=deletes beta (tracked per settings), inserts ref at 6 (untracked)
ROW references.footnote | sel=b1:6 (caret) | params={} | holds=b1{text~};doc{parts} | windows=doc.body | facts=none | reads=selection | note=writes session.body_return, caret into new note part
ROW references.footnote | sel=part:footnote/0:3 | params={} | holds=part:footnote{blocks@};doc{parts} | windows=part:footnote.blocks | facts=none | reads=selection | note=ref inserted inside the footnote; body_return overwritten with None
ROW references.removeToc | sel=none | params={} | holds=allblocks{text,para};doc.body{@} | windows=doc.body | facts=none | reads=selection | note=reads every top-level para's objects and the styles of those after; fixture has no TOC field -> no write; clamp_selection reads sel
ROW references.toc | sel=b1:6-10 | params={"levels":2,"title":"Contents"} | holds=b1{text~};allblocks{text,para};doc{settings,styles,numbering,lists,lastSection} | windows=doc.body | facts=layout | reads=selection | note=b1 split at 6, TOC inserted at index 2; entries from every body para's outline level, numbering counters, text; page numbers from 2 layouts; width from caret's section
ROW references.toc | sel=b4/0/0/0:0 | params={} | holds=b4/0/0/0{*} | windows=t4.rows,r4/0.cells | facts=none | reads=selection | note=error (not top-level) after delete_selection
ROW references.toc | sel=b1:6-10 | params={"levels":9} | holds=allblocks{text,para};doc{styles,numbering,lists,lastSection} | windows=doc.body | facts=layout | reads=selection | note=existing TOC: update only
ROW references.updateFields | sel=b1:6-10 | params={} | holds=allblocks{text,para};doc{core,sources,styles,numbering,lists,lastSection} | windows=doc.body | facts=layout,time | reads=bibStyle,selection | note=only body fields; headers/footers/notes untouched; selection read by update_toc's section width
ROW references.updateFields | sel=b4/0/1/0:0 | params={} | holds=t4{rows};allblocks{text} | windows=t4.rows | facts=none | reads=none | note= '=' formula fields read every cell of their table
ROW references.updateToc | sel=b1:6-10 | params={} | holds=allblocks{text,para};doc{styles,numbering,lists,lastSection};doc.body{@} | windows=doc.body | facts=layout | reads=selection | note=no TOC -> nothing; else entries removed and rebuilt; text width from the CARET's section (page::sect), not the TOC's
# ---- cmd/tools.rs, citations.rs
ROW design.effects | sel=none | params={} | holds=doc{settings} | windows= | facts=none | reads= | note=reads settings.themeName base, writes "<base> (Subtle)"; undeclared param name
ROW file.autosave | sel=none | params={} | holds= | windows= | facts=none | reads=other(autosave) | note=session flag only; doc untouched
ROW file.inspect | sel=none | params={} | holds=doc{core,parts};bodyparas{text} | windows= | facts=commentCount | reads=selection | note=report only: comment threads count, body ins/del run marks, core creator/lastModifiedBy, direct-hidden runs, header/footer part kinds; still a mutating step [corrected C8-doc-6: cmd/tools.rs:794-804 traverses para_paths(StoryRef::Body), never part paragraphs, for its report] [review: discarded content, returned reports and unused calculations checked] [review: report count is a root derived fact; comment threads are detached from Document.comments]
ROW file.inspect | sel=none | params={"remove":["comments"]} | holds=thread:0{comments};comment:0{part};comment:1{part};bodyparas{text};doc{parts,core} | windows= | facts=commentCount | reads=selection | note=removes comment anchor objects (b6 0..5 marks) from every body para, deletes threads and their comment parts; clamps selection [corrected C8-doc-6: cmd/tools.rs:794-804 reports body paragraphs only; review.rs:214-230 removes anchors only in StoryRef::Body; neither reads text in other parts] [review: discarded content, returned reports and unused calculations checked] [review: report count is a root derived fact; comment threads are detached from Document.comments]
ROW file.inspect | sel=none | params={"remove":["revisions"]} | holds=allblocks{text,para};part:header{blocks};part:footer{blocks};part:footnote{blocks};t4{props};doc{revisions,lastSection,parts,core};rev:0{*} | windows= | facts=commentCount | reads=selection | note=acceptAll: every story's ins/del/fmt marks resolved (b9 " Added" kept, mark cleared), containers resolved, revisions list cleared [review: discarded content, returned reports and unused calculations checked] [review: report count is a root derived fact; comment threads are detached from Document.comments]
ROW file.inspect | sel=none | params={"remove":["properties","hidden","headers"]} | holds=doc{core,lastSection,parts};bodyparas{text,para} | windows= | facts=commentCount | reads=selection | note=core reset keeping title; direct-hidden body runs deleted untracked; headers refs cleared in all sections but footers only in lastSection [corrected C8-doc-6: cmd/tools.rs:820-840 scans body paragraphs for hidden runs; Remove Header traverses body section breaks recursively (insert.rs:65-73), never part contents] [review: handler premises and same-field protection independently checked] [review: discarded content, returned reports and unused calculations checked] [review: report count is a root derived fact; comment threads are detached from Document.comments]
ROW file.newFromTemplate | sel=none | params={"path":"t.dotx"} | holds=doc{*} | windows= | facts=file | reads= | note=pure but replaces the whole document from the file; reads nothing of the current doc; clears pending/bodyReturn/also/column/undo
ROW file.protect | sel=none | params={"mode":"readOnly"} | holds=doc{*} | windows= | facts=none | reads= | note=via review.restrict: writes settings.protection only, reads nothing
ROW file.protect | sel=none | params={"mode":"trackedChanges"} | holds=doc{*} | windows= | facts=none | reads= | note=also writes settings.trackChanges=true
ROW file.versions | sel=none | params={"save":"v1"} | holds=doc{*} | windows= | facts=time | reads=other(versions) | note=snapshots whole doc (reads all of it) into session; doc not written
ROW file.versions | sel=none | params={"restore":0} | holds=doc{*} | windows= | facts=none | reads=other(versions),selection | note=replaces whole doc with the session snapshot; pure so no comment-only/protection gate; pending/bodyReturn/also not cleared
ROW file.versions | sel=none | params={} | holds= | windows= | facts=none | reads=other(versions) | note=lists session versions only
ROW hf.position | sel=b1:6-10 | params={"header":36} | holds=doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=section of body block 1 (single section -> lastSection); writes header distance; non-last section would be its ending block's para.section
ROW hf.position | sel=part:header/0:0 | params={"footer":30} | holds=doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=defect: part paragraph index 0 taken as body block index -> section of b0, not the section whose header is edited; bodyReturn not used
ROW insert.autoText | sel=b1:6-10 | params={"save":"q"} | holds=b1{text~} | windows= | facts=none | reads=selection,other(buildingBlocks) | note=copy_range 6..10: text, run formatting and objects inside the range into session building blocks; doc not written (still a mutating step)
ROW insert.autoText | sel=b1:6-10 | params={"insert":"q"} | holds=b1{text,para};doc{settings} | windows=doc.body | facts=none | reads=selection,other(buildingBlocks) | note=deletes 6..10 (tracked if trackChanges), inserts saved fragment at 6; 1-para fragment inline (b1 keeps its para), multi-block splits b1 and new blocks follow; insert itself untracked
ROW insert.docProperty | sel=b1:6-10 | params={"name":"Title"} | holds=doc{core,settings};b1{text} | windows=doc.body | facts=none | reads=selection,pending | note=reads core.title; formatting from pending or char at focus off 10 (before deletion); deletes 6..10, inserts TITLE field at 6
ROW insert.docProperty | sel=b1:6-10 | params={"name":"Nope"} | holds=none | windows= | facts=none | reads= | note=unknown name -> Params error before any write [corrected C8-doc-6: tools.rs:180-188 matches the parameter name; the unknown arm returns before reading a core member (taking a reference to core consumes no value)]
ROW insert.object | sel=b1:6-10 | params={"path":"x.docx"} | holds=b1{text,para};doc{settings} | windows=doc.body | facts=file | reads=selection | note=inserts the file's body (not an embedded object) at 6 after deleting 6..10; untracked insert
ROW insert.quickParts | sel=b1:6-10 | params={"save":"q"} | holds=b1{text~} | windows= | facts=none | reads=selection,other(buildingBlocks) | note=text/formatting/objects inside 6..10 copied to session; collapsed selection -> error
ROW insert.quickParts | sel=b1:6-10 | params={"insert":"q"} | holds=b1{text,para};doc{settings} | windows=doc.body | facts=none | reads=selection,other(buildingBlocks) | note=delete 6..10 then insert fragment at 6 (multi-block splits b1, merges first/last into halves, unformatted b1 takes fragment's para props); untracked insert
ROW insert.quickParts | sel=none | params={"delete":"q"} | holds= | windows= | facts=none | reads=other(buildingBlocks) | note=session only; returns names
ROW insert.signatureLine | sel=b1:6-10 | params={} | holds=b1{text,para};doc{settings} | windows= | facts=none | reads=selection | note=one-line fragment -> inserted inline into b1 at 6; its NoSpacing style dropped (b1 keeps own para)
ROW insert.signatureLine | sel=b1:6-10 | params={"signer":"Ann","title":"CEO"} | holds=b1{text,para};doc{settings} | windows=doc.body | facts=none | reads=selection | note=multi-block: b1 split at 6, first line merges into b1 (b1 props kept unless unformatted), others inserted at index 2.., last merges with tail
ROW insert.spreadsheet | sel=b1:6-10 | params={"csv":"a,b\n1,2"} | holds=b1{text,para};doc{styles,lastSection,settings} | windows=doc.body | facts=none | reads=selection | note=new table at index 2 after b1 split at 6; GridTable1Light must exist; width from first section
ROW insert.spreadsheet | sel=b4/0/1/0:0 | params={} | holds=c4/0/1{blocks};b4/0/1/0{text,para};doc{styles,lastSection} | windows=c4/0/1.blocks | facts=none | reads=selection | note=caret off 0 -> nested 3x3 table inserted before the cell paragraph inside c4/0/1
ROW layout.linkToPrevious | sel=b1:6-10 | params={} | holds=doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=single/first section: header refs cleared = header removed (part:header orphaned); writes without reading
ROW layout.linkToPrevious | sel=b1:6-10 | params={"footer":true} | holds=doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=footer refs cleared
ROW layout.linkToPrevious | sel=part:header/0:0 | params={} | holds=doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=defect: part paragraph index used as body block index (section of b0)
ROW para.defineBullet | sel=b1:6-10 | params={"char":"★"} | holds=b1{para};b0{para};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=b0 (previous para) numbering checked for continuation; existing ★ list reused else new list; b1 numbering/indents/style written
ROW para.defineBullet | sel=b3:0 (caret) | params={"char":"★"} | holds=b3{para};b2{para};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=b3 numbered (not bullet) -> not toggle; b2 numbered -> no continuation; b3 moved to a ★ list, level kept
ROW para.defineBullet | sel=b1:6-10 | params={"char":"ab"} | holds=b1{para};b0{para};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=multi-char -> parse fails -> default bullet (•), silently
ROW para.defineNumber | sel=b1:6-10 | params={"format":"upperRoman","text":"%1)","start":3} | holds=doc{numbering,lists};b1{*} | windows=doc.body | facts=none | reads=selection | note=new list; b1 numbering and style overwritten, its para not read
ROW para.defineNumber | sel=b0:0-b3:3 | params={"format":"decimal"} | holds=doc{numbering,lists};b0{*};b1{*};b2{*};b3{*} | windows=doc.body | facts=none | reads=selection | note=all four get the new list at level 0; b0 loses Heading 1 (style forced ListParagraph); b2/b3 leave their list
ROW para.setNumberingValue | sel=b1:6-10 | params={"value":5} | holds=b1{para} | windows= | facts=none | reads=selection | note=b1 not numbered -> Failed, nothing written
ROW para.setNumberingValue | sel=b2:0 (caret) | params={"value":5} | holds=b2{para};b3{para};b4/0/0/0{para};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=b2,b3 moved to new num starting at 5; stops at first cell paragraph (not in list)
ROW para.setNumberingValue | sel=b3:0 (caret) | params={"value":1} | holds=b3{para};b4/0/0/0{para};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection | note=only b3 restarted; b2 stays on old num
ROW review.combine | sel=none | params={"path":"rev.docx"} | holds=doc{body,revisions@};bodyinline{ALL} | windows=doc.body | facts=file,time | reads= | note=as review.compare: body rebuilt from top-level paragraphs only (table b4 dropped), diff marks with 2 new "Compare" revisions; caret to body start [corrected C8-doc-6: allblocks -> bodyblocks and t4{*} -> t4{kind}: crates/engine/engine/src/cmd/tools.rs:600 clones only body.iter().filter_map(as_para); no cell or part paragraphs or table contents are read] [review: handler premises and same-field protection independently checked]
ROW review.compare | sel=none | params={"text":"Title\nAlpha beta delta."} | holds=doc{body,revisions@};bodyinline{ALL} | windows=doc.body | facts=time | reads= | note=unchanged paras kept whole; changed ones lose objects (b1 bookmark Start, footnote ref) and formatting; b4 table removed untracked; comment anchors/pictures in deleted-or-changed paras lost [corrected C8-doc-6: allblocks -> bodyblocks and t4{*} -> t4{kind}: crates/engine/engine/src/cmd/tools.rs:600 clones only body.iter().filter_map(as_para); no cell or part paragraphs or table contents are read] [review: handler premises and same-field protection independently checked]
ROW review.compare | sel=none | params={"path":"rev.docx"} | holds=doc{body,revisions@};bodyinline{ALL} | windows=doc.body | facts=file,time | reads= | note=revised read from file (only its top-level paragraphs used) [corrected C8-doc-6: allblocks -> bodyblocks and t4{*} -> t4{kind}: crates/engine/engine/src/cmd/tools.rs:600 clones only body.iter().filter_map(as_para); no cell or part paragraphs or table contents are read] [review: handler premises and same-field protection independently checked]
ROW review.compare | sel=none | params={} | holds= | windows= | facts=none | reads= | note=Params error
ROW review.language | sel=b1:6-10 | params={"lang":"fr-FR","noProof":true} | holds=b1{text~} | windows= | facts=none | reads=selection,mode(column) | note=format_range 6..10 sets lang/noProof; tracked formatting revision if trackChanges+trackFormatting (doc{settings,revisions}, author)
ROW review.language | sel=b1:2 (caret) | params={"lang":"fr-FR"} | holds=b1{text~} | windows= | facts=none | reads=selection | note=caret inside a word: word bounds read, "Alpha" 0..5 formatted
ROW review.language | sel=b5:0 (caret) | params={"lang":"fr-FR"} | holds=b5{text} | windows= | facts=none | reads=selection,pending | note=no word: pending = pending or continuing props + lang; b5 empty -> its mark formatted too
ROW review.restrict | sel=none | params={"mode":"comments"} | holds=doc{*} | windows= | facts=none | reads= | note=writes settings.protection, reads nothing; mode not validated
ROW review.restrict | sel=none | params={"mode":"trackedChanges"} | holds=doc{*} | windows= | facts=none | reads= | note=also settings.trackChanges=true
ROW review.restrict | sel=none | params={"mode":"none"} | holds=doc{*} | windows= | facts=none | reads= | note=protection cleared; trackChanges left as it was
ROW table.borderPainter | sel=b1:6-10 | params={} | holds= | windows= | facts=none | reads=selection | note=focus not in a table -> Disabled
ROW table.borderPainter | sel=b4/0/1/0:0 | params={} | holds=t4{props};c4/0/0{props};c4/0/1{props};c4/1/0{props};c4/1/1{props} | windows=t4.rows,r4/0.cells,r4/1.cells | facts=none | reads=selection | note=caret -> whole table all borders 0.5pt; every cell's own border sides cleared
ROW table.borderPainter | sel=b4/0/0/0:0-b4/0/1/0:2 | params={"kind":"outside"} | holds=t4{props};c4/0/0{props};c4/0/1{props};c4/1/0{props};c4/1/1{props} | windows=t4.rows,r4/0.cells,r4/1.cells | facts=none | reads=selection | note=row 0 block: cell borders set on r0 cells, row-1 cells get the matching top edge
ROW table.cellMargins | sel=b1:6-10 | params={"top":3} | holds= | windows= | facts=none | reads=selection | note=not in a table -> Disabled
ROW table.cellMargins | sel=b4/1/0/0:1 | params={"top":3} | holds=t4{*} | windows= | facts=none | reads=selection | note=writes all four margins (left/right reset to 5.4, bottom 0); existing margins not read
ROW insert.crossReference | sel=none | params={} | holds=allblocks{text,para};doc{styles} | windows= | facts=none | reads= | note=returns ["Title"] (b0 Heading 1 via style outline level); still a mutating step
ROW insert.crossReference | sel=b1:6-10 | params={"to":"heading","target":"Title"} | holds=allblocks{text,para};doc{styles,settings};b0{text};b1{text} | windows=doc.body | facts=none | reads=selection,pending | note=deletes 6..10, adds _Ref000000001 around b0 text (number = max _Ref in all body bookmarks +1), REF field result "Title" at b1:6
ROW insert.crossReference | sel=b1:6-10 | params={"to":"bookmark","target":"Start","show":"text"} | holds=allblocks{text};b1{text};doc{settings} | windows= | facts=none | reads=selection,pending | note=result = text between Start marks in b1 ("Alpha"); no new bookmark; REF Start \h at 6
ROW insert.crossReference | sel=b1:6-10 | params={"to":"heading","target":"Title","show":"page"} | holds=allblocks{text,para};doc{styles,settings};b0{text};b1{text} | windows=doc.body | facts=layout | reads=selection,pending | note=PAGEREF with page from layout; show=aboveBelow instead compares b0 position with sel.focus
ROW insert.crossReference | sel=b1:6-10 | params={"to":"figure","target":"Figure 1","show":"number"} | holds=allblocks{text,para};b1{text};doc{settings} | windows=doc.body | facts=none | reads=selection,pending | note=fixture has no Caption-styled paras -> Params error; otherwise bookmark up to ':' and result text before ':'
ROW references.bibliography | sel=b1:6-10 | params={} | holds=b1{text,para};allblocks{text,para};doc{sources,styles,settings,lastSection} | windows=doc.body | facts=none | reads=selection,other(bibStyle) | note=b1 split at 6, heading at index 2, entries after; layout only if other generated lists exist (rebuilt too)
ROW references.bibliography | sel=b5:0 (caret) | params={"title":"Works"} | holds=b5{text};allblocks{text,para};doc{sources,styles,lastSection} | windows=doc.body | facts=none | reads=selection,other(bibStyle) | note=b5 empty -> heading inserted at index 5 before it, no split
ROW references.bibliography | sel=b4/0/0/0:0 | params={} | holds= | windows= | facts=none | reads=selection | note=cell (or part) -> Failed "lists go in the main text" (selection deletion rolled back)
ROW references.citation | sel=b1:6-10 | params={"tag":"Riv21","pages":"4"} | holds=doc{sources@,settings};b1{text} | windows=doc.sources | facts=none | reads=selection,pending,other(bibStyle) | note=source by tag, its index for IEEE number; formatting from char at focus off 10; field at 6 after deleting 6..10
ROW references.citation | sel=b1:6-10 | params={"source":{"author":"Rivera, Alex","title":"T","year":"2021"}} | holds=doc{sources@,settings};b1{text} | windows=doc.sources | facts=none | reads=selection,pending,other(bibStyle) | note=source added/replaced (moved to end) then the LAST source cited
ROW references.citation | sel=b1:6-10 | params={} | holds= | windows= | facts=none | reads= | note=Params error
ROW references.citationStyle | sel=b1:6-10 | params={"style":"MLA"} | holds=allblocks{text,para};doc{sources@,styles,lastSection} | windows=doc.sources | facts=none | reads=selection,other(bibStyle) | note=style kept only in the session; body CITATION results rewritten (parts/footnotes not); generated lists rebuilt (layout if index/TOA/figures present)
ROW references.citationStyle | sel=none | params={"style":"Harvard"} | holds= | windows= | facts=none | reads= | note=Params error
ROW references.index | sel=b1:6-10 | params={} | holds=b1{text,para};allblocks{text,para};doc{styles,settings,lastSection} | windows=doc.body | facts=layout | reads=selection | note=b1 split at 6, heading at 2; pages of XE marks from (possibly stale) layout; all other generated lists rebuilt too
ROW references.index | sel=b4/1/1/0:0 | params={} | holds= | windows= | facts=none | reads=selection | note=in a cell -> Failed
ROW references.markCitation | sel=b1:6-10 | params={} | holds=b1{text~} | windows= | facts=none | reads=selection | note=entry "beta" from text 6..10; field inserted at off 10 (before the footnote ref object); not typing props
ROW references.markCitation | sel=b1:6 (caret) | params={"entry":"Smith v. Jones"} | holds=b1{*} | windows= | facts=none | reads=selection | note=entry from param, insert at caret
ROW references.markCitation | sel=b1:6 (caret) | params={} | holds= | windows= | facts=none | reads=selection | note=empty selection -> Params error
ROW references.markEntry | sel=b1:6-10 | params={} | holds=b1{text~} | windows= | facts=none | reads=selection | note=entry "beta"; hidden XE "beta" field at off 10; selection kept
ROW references.markEntry | sel=b1:6 (caret) | params={"entry":"Kilns"} | holds=b1{*} | windows= | facts=none | reads=selection | note=entry from param
ROW references.markEntry | sel=b1:6 (caret) | params={} | holds= | windows= | facts=none | reads=selection | note=Params error
ROW references.noteOptions | sel=none | params={"footnoteFormat":"lowerRoman"} | holds=doc{*} | windows= | facts=none | reads= | note=writes settings.footnoteFormat/endnoteFormat (NumFormat::from_ooxml); nothing read; notes not touched
ROW references.noteOptions | sel=none | params={} | holds= | windows= | facts=none | reads= | note=no-op but a mutating step; returns current formats (doc{settings} read for the echo)
ROW references.sources | sel=none | params={} | holds=doc{sources} | windows= | facts=none | reads= | note=returns list; still mutating step
ROW references.sources | sel=none | params={"add":{"author":"Chen, Mei","year":"2020","title":"X"}} | holds=doc{sources@} | windows=doc.sources | facts=none | reads= | note=tag defaulted "Chen2020"; same tag removed and source appended at END (changes IEEE numbers); citations not refreshed
ROW references.sources | sel=none | params={"remove":"Chen2020"} | holds=doc{sources@} | windows=doc.sources | facts=none | reads= | note=removed; existing CITATION results left stale
ROW references.tableOfAuthorities | sel=b1:6-10 | params={} | holds=b1{text,para};allblocks{text,para};doc{styles,settings,lastSection} | windows=doc.body | facts=layout | reads=selection | note=heading "Table of Authorities" + TOA field; entries from all body TA fields grouped "entry<TAB>pages" with dot-leader tab at caret-section width
ROW references.tableOfFigures | sel=b1:6-10 | params={"label":"Table"} | holds=b1{text,para};allblocks{text,para};doc{styles,settings,lastSection} | windows=doc.body | facts=layout | reads=selection | note=fixture has no Caption paras -> empty list, heading para only; direct style only (not based-on chain)
ROW references.updateFigures | sel=b1:6-10 | params={} | holds=allblocks{text,para};doc{styles,sources,lastSection} | windows=doc.body | facts=layout | reads=selection,other(bibStyle) | note=rebuilds every BIBLIOGRAPHY/INDEX/TOA/TOC\c list; fixture has none -> only upserts hidden ListEntry style (doc.styles written)
ROW references.updateIndex | sel=b1:6-10 | params={} | holds=allblocks{text,para};doc{styles,sources,lastSection} | windows=doc.body | facts=layout | reads=selection,other(bibStyle) | note=identical to updateFigures (same update_generated); width from caret's section
# ---- cmd/review.rs, mailings.rs, edit.rs
ROW comment.add | sel=b1:6-10 | params={"text":"Hi"} | holds=b1{text~,@};doc{comments*,parts*} | windows=doc.body | facts=time | reads=selection,author | note=anchors inserted at bytes 6 and 10 of b1 (text read only for offsets/char boundary); new comment id = last key+1, new part id = last part key+1
ROW comment.add | sel=b1:6 (caret) | params={"text":"Hi","name":"Ann","created":"2026-01-01T00:00:00Z"} | holds=b1{text,@};doc{comments*,parts*} | windows=doc.body | facts=none | reads=selection | note=word_at reads b1 text before and after the caret to find "beta"; created given so no clock
ROW comment.add | sel=b5:0 (caret) | params={"text":"Hi"} | holds=b5{text,@};doc{comments*,parts*} | windows=doc.body | facts=time | reads=selection,author | note=empty paragraph: zero-width anchor at caret; name defaults to session.author
ROW comment.add | sel=b4/0/1/0:0 | params={"text":"Hi"} | holds=c4/0/1{blocks@};b4/0/1/0{text,@} | windows=doc.body,t4.rows,r4/0.cells | facts=time | reads=selection,author | note=caret in cell B1: word "B1" anchored in the cell paragraph
ROW comment.delete | sel=none | params={"thread":"c0"} | holds=thread:0{comments};comment:0{part,parent};comment:1{part,parent};doc{comments,parts*} | windows=none | facts=commentAnchors | reads=selection | note=reads every comment's parent to collect the thread; scans matching body anchors; selection only clamped [corrected C8-doc-6: allblocks{text} -> commentAnchors; crates/engine/engine/src/cmd/review.rs:212-232 collects only matching CommentStart/End offsets, not unrelated text or part paragraphs]
ROW comment.delete | sel=none | params={"thread":"c0","comment":"c1"} | holds=comment:1{ALL};doc{comments} | windows=none | facts=commentAnchors | reads=selection | note=reply removed; any reply whose parent==1 gets parent 0; reads all comments' parent [corrected C8-doc-6: allblocks{text} -> commentAnchors; crates/engine/engine/src/cmd/review.rs:212-232 consumes matching body marker offsets only] [review: deleting a comment discards its complete record, including initials]
ROW comment.delete | sel=none | params={"thread":"c1"} | holds=comment:1{parent};thread:0{comments} | windows=none | facts=commentAnchors | reads=selection | note=a reply id names its whole thread (root 0) -> whole thread deleted [corrected C8-doc-6: allblocks{text} -> commentAnchors; crates/engine/engine/src/cmd/review.rs:212-232 consumes matching body marker offsets only]
ROW comment.reply | sel=none | params={"thread":"c0","text":"ok"} | holds=comment:0{parent};doc{comments*,parts*} | windows=none | facts=time | reads=author | note=no anchor written; reads parent chain of the named comment; id = last key + 1
ROW comment.reply | sel=none | params={"thread":"c1","text":"ok","name":"Cy","created":"2026-01-01T00:00:00Z"} | holds=comment:1{parent};comment:0{parent};doc{comments*,parts*} | windows=none | facts=none | reads=none | note=reply to a reply goes to the root thread:0
ROW comment.resolve | sel=none | params={"thread":"c0","resolved":false} | holds=comment:0{parent};thread:0{resolved*} | windows=none | facts=none | reads=none | note=writes resolved on the root comment; default resolved=true (not a toggle)
ROW comment.resolve | sel=none | params={"thread":"c1"} | holds=comment:1{parent};comment:0{resolved*} | windows=none | facts=none | reads=none | note=reply id -> root's resolved
ROW review.accept | sel=b1:6-10 | params={} | holds=b1{text~,para,@};doc{revisions} | windows=doc.body | facts=none | reads=selection | note=runs of b1 overlapping 6..10 checked for ins/del/fmt_change, b1 para fmt_change/num_change; nothing tracked -> no change, caret to b1:6
ROW review.accept | sel=b9:13 (caret) | params={} | holds=b9{text~,@} | windows=doc.body | facts=none | reads=selection | note=caret inside rev:0's run: whole " Added" run's ins mark cleared (run boundaries read)
ROW review.accept | sel=b4/0/1/0:0 | params={} | holds=b4/0/1/0{text,para};t4{props};r4/0{props};c4/0/1{props} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=no tracked run: falls to the cell's/row's/table's fmt_change, else navigates to the next change (reads every body paragraph's revisions)
ROW review.accept | sel=b8:0-b9:2 | params={} | holds=b8{text,para,@};b9{text~,para,@} | windows=doc.body | facts=none | reads=selection | note=b8's mark inside range: a deleted mark accepted joins b9 into b8
ROW review.acceptAll | sel=none | params={} | holds=allblocks{text,para};part:header{blocks};part:footer{blocks};part:footnote{blocks};t4{props};r4/0{props};r4/1{props};doc{lastSection,revisions} | windows=none | facts=none | reads=selection | note=every run/mark/para/table revision read; deleted marks join paragraphs; selection clamped only
ROW review.reject | sel=b9:13 (caret) | params={} | holds=b9{text~,@} | windows=doc.body | facts=none | reads=selection | note=rev:0's " Added" run deleted from b9
ROW review.reject | sel=b1:6-10 | params={} | holds=b1{text~,para,@} | windows=doc.body | facts=none | reads=selection | note=no tracked change in range: nothing changes
ROW review.reject | sel=b6:8 (caret) | params={} | holds=b6{text,para};bodyparas{text,para};t4{props};r4/0{props};r4/1{props};c4/0/0{props};c4/0/1{props};c4/1/0{props};c4/1/1{props};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=no change at caret, not in a cell: nav_change scans all body changes, selects next one after caret [corrected C8-doc-6: crates/engine/engine/src/cmd/review.rs:518 traverses para_paths(StoryRef::Body) only, never part contents] [review: handler premises and same-field protection independently checked]
ROW review.rejectAll | sel=none | params={} | holds=allblocks{text,para};part:header{blocks};part:footer{blocks};part:footnote{blocks};t4{props};r4/0{props};r4/1{props};doc{lastSection,revisions} | windows=none | facts=none | reads=selection | note=as acceptAll with reject; inserted text removed, doc.revisions cleared
ROW review.applySuggestion | sel=b1:6-10 | params={"text":"beth"} | holds=b1{text,@};doc{settings,revisions} | windows=doc.body | facts=none | reads=selection,pending,other:proofDictionary | note=focus b1:10 (end of "beta"); no issue there -> replaces the current selection "beta" anyway; formatting of the char before 10 (or pending)
ROW review.applySuggestion | sel=b9:2 (caret) | params={"text":"Last"} | holds=b9{text,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,author,other:proofDictionary | note=misspelled word at caret selected first then replaced; tracked if track_changes (new revision by author at now)
ROW review.deleteComment | sel=b6:4 (caret) | params={} | holds=b6{text};comment:0{ALL};comment:1{ALL};doc{comments} | windows=doc.body | facts=commentAnchors | reads=selection | note=caret inside "After": comment 0 + reply 1 deleted, anchors removed from b6 [corrected C8-doc-6: allblocks{text} -> commentAnchors; crates/engine/engine/src/cmd/review.rs:212-232 consumes matching body marker offsets only; caret 2 -> 4 since the start marker is a three-byte OBJ (doc/src/para.rs::OBJ), not a text character] [review: deleting a comment discards its complete record, including initials]
ROW review.deleteComment | sel=b6:12 (caret) | params={} | holds=b6{text} | windows=doc.body | facts=none | reads=selection | note=caret outside the comment: no comment here [corrected C8-doc-6: caret 9 -> 12, beyond the three-byte end marker at 8 (doc/src/para.rs::OBJ); no fallback or thread scan: crates/engine/engine/src/cmd/review.rs:209 requires range overlap, delete_comment:242 returns before removing anchors when no id is found]
ROW review.deleteComment | sel=none | params={"id":1} | holds=comment:1{ALL};doc{comments} | windows=none | facts=commentAnchors | reads=selection | note=reply deleted with its direct replies; any deeper reply keeps a dangling parent [corrected C8-doc-6: allblocks{text} -> commentAnchors; crates/engine/engine/src/cmd/review.rs:212-232 consumes matching body marker offsets only] [review: deleting a comment discards its complete record, including initials]
ROW review.deleteComment | sel=none | params={"all":true} | holds=thread:0{comments};comment:0{ALL};comment:1{ALL};doc{parts*} | windows=none | facts=commentCount,commentAnchors | reads=selection | note=every comment and its part; anchors scanned in body only [corrected C8-doc-6: allblocks{text} -> commentAnchors; crates/engine/engine/src/cmd/review.rs:212-232 consumes matching body marker offsets only] [review: deleting a comment discards its complete record, including initials]
ROW review.editComment | sel=none | params={"id":1,"text":"a\nb"} | holds=comment:1{part};part:comment1{blocks*} | windows=none | facts=none | reads=none | note=comment part's blocks replaced, one default paragraph per line (max 10000); author/date untouched
ROW review.newComment | sel=b1:6-10 | params={"text":"x"} | holds=b1{text~,@};doc{comments*,parts*} | windows=doc.body | facts=time | reads=selection,author | note=anchors at b1 6 and 10
ROW review.newComment | sel=b1:6 (caret) | params={"text":"x"} | holds=b1{text,@};doc{comments*,parts*} | windows=doc.body | facts=time | reads=selection,author | note=word at caret ("beta") from text around caret
ROW review.reply | sel=none | params={"id":0,"text":"ok"} | holds=comment:0{*};doc{comments*,parts*} | windows=none | facts=time | reads=author | note=parent must exist; new comment id last+1 with parent 0; no anchor; id truncated `as u32` [corrected C8-doc-6: comment:0* -> comment:0{*}; crates/engine/engine/src/cmd/review.rs:160-163 tests the existence of comment 0, no object named comment:0*]
ROW review.resolveComment | sel=b6:4 (caret) | params={} | holds=b6{text};comment:0{resolved} | windows=doc.body | facts=none | reads=selection | note=toggle: reads current resolved [corrected C8-doc-6: caret 2 -> 4; doc/src/para.rs::OBJ is three bytes and review.rs:209 requires a caret after the start marker]
ROW review.resolveComment | sel=b1:6-10 | params={} | holds=b1{text} | windows=doc.body | facts=none | reads=selection | note=no comment anchor in b1 -> error `id` required
ROW review.resolveComment | sel=none | params={"id":1,"value":true} | holds=comment:1{resolved*} | windows=none | facts=none | reads=none | note=a reply's flag is set, the thread root's not
ROW review.trackChanges | sel=none | params={} | holds=doc{settings} | windows=none | facts=none | reads=none | note=toggle reads settings.track_changes
ROW review.trackChanges | sel=none | params={"value":true} | holds=doc{settings*} | windows=none | facts=none | reads=none | note=writes track_changes only (but protection is read by run_command first)
ROW mailings.addressBlock | sel=b1:6-10 | params={} | holds=b1{text,@};bodyparas{text};doc{settings,revisions} | windows=doc.body | facts=none | reads=selection,pending,mergeRecord,other:mergeRows | note=selection "beta" deleted first; field takes formatting of the char before 10; refresh rewrites every body merge field's result [review: handler premises and same-field protection independently checked]
ROW mailings.addressBlock | sel=b1:6 (caret) | params={} | holds=b1{text~,@};bodyparas{text} | windows=doc.body | facts=none | reads=selection,pending,mergeRecord,other:mergeRows | note=pure insert; formatting at caret (props_at 6) [review: handler premises and same-field protection independently checked]
ROW mailings.envelopes | sel=none | params={"delivery":"Jo\n1 St"} | holds=doc{*} | windows=none | facts=none | reads=none | note=the whole document is replaced; nothing of the old one read
ROW mailings.envelopes | sel=none | params={} | holds=doc{*} | windows=none | facts=mergeSource | reads=mergeRecord | note=delivery from the current recipient's address columns
ROW mailings.findRecipient | sel=none | params={"text":"ada"} | holds=allblocks{text} | windows=none | facts=mergeSource | reads=other:mergeRows | note=`text` is REQUIRED although params say {}; first row containing it -> record, preview on, refresh rewrites body merge field results
ROW mailings.findRecipient | sel=none | params={} | holds=none | windows=none | facts=none | reads=none | note=error `text` required (when no recipients: disabled)
ROW mailings.finish | sel=none | params={} | holds=doc{*};allblocks{text,para};t4{rows};c4/0/0{blocks};c4/0/1{blocks};c4/1/0{blocks};c4/1/1{blocks} | windows=none | facts=mergeSource | reads=other:mergeRows,other:mergeKind | note=whole document rebuilt from every body block (and its fields) per record
ROW mailings.finish | sel=none | params={"path":"/tmp/out.docx","from":1,"to":1} | holds=allblocks{text,para};doc{styles,numbering,lists,parts,comments,settings} | windows=none | facts=mergeSource,file | reads=other:mergeRows,other:mergeKind | note=writes only the external file
ROW mailings.finish | sel=none | params={"from":9} | holds=doc{*} | windows=none | facts=mergeSource | reads=other:mergeRows | note=from > record count: empty body; the document is replaced by an empty one (see DEFECTS)
ROW mailings.greetingLine | sel=b1:6 (caret) | params={} | holds=b1{text~,@};allblocks{text} | windows=doc.body | facts=none | reads=selection,pending,mergeRecord,other:mergeRows | note=as addressBlock with GREETINGLINE
ROW mailings.greetingLine | sel=b1:6-10 | params={} | holds=b1{text,@};allblocks{text};doc{settings,revisions} | windows=doc.body | facts=none | reads=selection,pending,mergeRecord,other:mergeRows | note=selection deleted first (tracked when tracking)
ROW mailings.insertField | sel=b1:6 (caret) | params={"field":"City"} | holds=b1{text~,@};allblocks{text} | windows=doc.body | facts=none | reads=selection,pending,mergeRecord,other:mergeRows | note=MERGEFIELD City at caret, formatting at caret; refresh of all body merge fields
ROW mailings.insertField | sel=b1:6-10 | params={"field":"First Name"} | holds=b1{text,@};allblocks{text};doc{settings,revisions} | windows=doc.body | facts=none | reads=selection,pending,mergeRecord,other:mergeRows | note=selection replaced; quoted name
ROW mailings.insertField | sel=b4/0/1/0:0 | params={"field":"City"} | holds=b4/0/1/0{text~,@};allblocks{text} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection,pending | note=cell paragraph by path
ROW mailings.labels | sel=none | params={"text":"Hello"} | holds=doc{*} | windows=none | facts=none | reads=none | note=whole document replaced, nothing read
ROW mailings.labels | sel=none | params={} | holds=doc{*} | windows=none | facts=mergeSource | reads=other:mergeRows | note=with recipients: one address per label in row order (record not read)
ROW mailings.next | sel=none | params={} | holds=allblocks{text} | windows=none | facts=mergeSource | reads=mergeRecord,other:mergeRows | note=record+1 clamped, preview on; refresh writes every body merge field's result (headers/parts not)
ROW mailings.preview | sel=none | params={} | holds=allblocks{text} | windows=none | facts=mergeSource | reads=mergeRecord,other:mergeRows,other:mergePreview | note=toggles preview (or `value`); refresh writes body merge field results
ROW mailings.previous | sel=none | params={} | holds=allblocks{text} | windows=none | facts=mergeSource | reads=mergeRecord,other:mergeRows | note=record-1 clamped; refresh
ROW mailings.rules | sel=b1:6 (caret) | params={"rule":"IF","field":"City","value":"Paris","then":"a","else":"b"} | holds=b1{text~,@};allblocks{text} | windows=doc.body | facts=none | reads=selection,pending,mergeRecord,other:mergeRows | note=field object with built instr and empty result; refresh
ROW mailings.rules | sel=b1:6-10 | params={"rule":"NEXT"} | holds=b1{text,@};allblocks{text};doc{settings,revisions} | windows=doc.body | facts=none | reads=selection,pending | note=unknown/omitted rule -> NEXT; selection deleted first
ROW edit.cut | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,other:clipHistory | note=text and formatting of "beta" read; writes clipboard (session) and deletes/marks the range; time only when tracked [corrected C8-doc-5: the clipboard is written, not read: edit.rs:105-108 (copy) then edit.rs:114 (delete), edit.rs:112-116]
ROW edit.cut | sel=b4/0/0/0:0-b5:0 | params={} | holds=b4/0/0/0{text,para};t4{props,grid,rows};b5{text,@} | windows=doc.body,t4.rows | facts=none | reads=selection | note=range across the table: whole table copied [corrected C8-doc-5: the clipboard is written, not read: edit.rs:105-108 (copy) then edit.rs:114 (delete), edit.rs:112-116]
ROW edit.cut | sel=b1:6-10 | params={} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,mode | note=column mode: each segment copied as one row paragraph and deleted (column::copy/delete) [corrected C8-doc-5: the clipboard is written, not read: edit.rs:105-108 (copy) then edit.rs:114 (delete), edit.rs:112-116]
ROW edit.paste | sel=b1:6-10 | params={"text":"x"} | holds=b1{text,para,@};doc{settings,revisions} | windows=doc.body | facts=clipboard | reads=selection | note=text compared with own clipboard; plain text replaces "beta"; reads para ids in use (all paragraphs) to drop copied ids
ROW edit.paste | sel=b1:6 (caret) | params={} | holds=b1{text,para,@} | windows=doc.body | facts=clipboard,paraIds | reads=selection | note=clipboard fragment; multi-paragraph splits b1 at 6; copied NoteRef/Comment/Bookmark objects keep their ids [corrected C8-doc-6: allblocks{para} -> fact paraIds: doc/src/edit.rs:65-89,509-511 reads only the id set; removed doc.parts: adopt_text_boxes at edit.rs:460-466 returns before accessing document parts when the fragment has no text box]
ROW edit.paste | sel=b4/0/1/0:0 | params={} | holds=b4/0/1/0{text,para,@};c4/0/1{blocks} | windows=doc.body,t4.rows,r4/0.cells | facts=clipboard | reads=selection | note=into cell paragraph by path
ROW edit.pasteAllClipboard | sel=b1:6 (caret) | params={} | holds=b1{text,para,@};doc{settings,revisions} | windows=doc.body | facts=clipboard,paraIds | reads=selection,other:clipHistory | note=every history item oldest-first through paste (clipboard temporarily swapped and restored); selection replaced by first only [corrected C8-doc-6: allblocks{para} -> fact paraIds: crates/engine/doc/src/edit.rs:65-89 scans only paragraph and row ids; insert_fragment:509-511 uses the set to clear copied ids]
ROW edit.pasteClipboardItem | sel=b1:6-10 | params={"index":0} | holds=b1{text,para,@};doc{settings,revisions} | windows=doc.body,other:clipHistory | facts=clipboard,paraIds | reads=selection,other:clipHistory | note=index into session clip history (bounds checked); pasted like edit.paste [corrected C8-doc-6: allblocks{para} -> fact paraIds: crates/engine/doc/src/edit.rs:65-89 scans only paragraph and row ids; insert_fragment:509-511 uses the set to clear copied ids]
ROW edit.pasteFormat | sel=b1:6-10 | params={} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,other:painter | note=char props of "beta" overwritten (own link kept); para props untouched (one paragraph, start 6)
ROW edit.pasteFormat | sel=b1:6 (caret) | params={} | holds=b1{text,para,@} | windows=doc.body | facts=none | reads=selection,other:painter | note=word at caret found from text; b1 para props (incl. para id, numbering, style) replaced by painter's
ROW edit.pasteFormat | sel=b2:0-b3:3 | params={} | holds=b2{text,para,@};b3{text,para,@} | windows=doc.body | facts=none | reads=selection,other:painter | note=both list paragraphs get the painter's paragraph props (numId replaced)
ROW edit.pasteMerge | sel=b1:6-10 | params={"text":"x"} | holds=b1{text,@};doc{settings,revisions} | windows=doc.body | facts=none | reads=selection,pending | note=same handler as pasteText (type_text): formatting from pending or char before focus 10; "beta" deleted (tracked when tracking)
ROW edit.pasteMerge | sel=b1:6 (caret) | params={} | holds=b1{text~,para,@};doc{styles,settings,revisions} | windows=doc.body | facts=clipboard | reads=selection,pending | note=clipboard's plain text; newlines become paragraph splits (split_para reads style's next style, list state) [review: handler premises and same-field protection independently checked]
ROW edit.pasteText | sel=b1:6-10 | params={"text":"x"} | holds=b1{text,@};doc{settings,revisions} | windows=doc.body | facts=none | reads=selection,pending | note=type_text over selection
ROW edit.pasteText | sel=b1:6 (caret) | params={"text":"a\nb"} | holds=b1{text~,para,@};doc{styles,settings,revisions} | windows=doc.body | facts=none | reads=selection,pending | note=line breaks -> split_para: next-style chain of b1's style read; empty list item would leave the list
ROW edit.pasteText | sel=b1:6 (caret) | params={} | holds=b1{text~,para,@};doc{styles} | windows=doc.body | facts=clipboard | reads=selection,pending | note=no text: clipboard fragment's plain text (empty when none: no error) [review: handler premises and same-field protection independently checked]
ROW edit.redo | sel=none | params={} | holds=doc{*} | windows=none | facts=none | reads=other:redoStack,other:origin,selection | note=pure; newest redo step of the caller's scope: whole document + selection swapped back
ROW edit.redo | sel=none | params={"scope":"agent"} | holds=doc{*} | windows=none | facts=none | reads=other:redoStack,other:kdmReplica | note=other scope has later steps: replica step_under redoes only that step
ROW edit.replace | sel=b1:6-10 | params={"text":"beta","with":"B"} | holds=allblocks{text};b1{text,@};doc{settings,revisions} | windows=doc.body | facts=none | reads=selection,pending,find | note=selection == match: replaced, next match selected; every body paragraph's text searched (twice + remaining count)
ROW edit.replace | sel=b1:0-5 | params={"text":"beta","with":"B"} | holds=allblocks{text} | windows=doc.body | facts=none | reads=selection,find | note=selection is not a match: nothing replaced, selection moved to next match
ROW edit.replace | sel=part:header/0:0 | params={"text":"Head","with":"H"} | holds=part:header{blocks};part:header/0{text,@} | windows=part:header.blocks | facts=none | reads=selection,find | note=story = selection's story (header part)
ROW edit.replaceAll | sel=b1:6-10 | params={"text":"e","with":"E"} | holds=allblocks{text} | windows=doc.body | facts=none | reads=selection,find | note=only the focus story (body incl. cells); headers, footnotes, comments not searched; formatting of each match's first char
ROW edit.replaceAll | sel=b1:6-10 | params={"text":"a","with":"b","matchCase":true} | holds=allblocks{text};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,find,pending,author | note=track_changes on: per match type_text (props from pending if set, else char before match end), each a revision
ROW edit.replaceAll | sel=part:footnote/0:0 | params={"text":"note","with":"N"} | holds=part:footnote{blocks} | windows=none | facts=none | reads=selection,find | note=story of focus = the footnote part
ROW edit.undo | sel=none | params={} | holds=doc{*} | windows=none | facts=none | reads=other:undoStack,other:origin,selection | note=pure; newest step of the caller's scope on top: whole doc+selection restored from snapshot
ROW edit.undo | sel=none | params={"scope":"person"} | holds=doc{*} | windows=none | facts=none | reads=other:undoStack,other:kdmReplica | note=other scope's steps above it: replica takes back only that step (step_under)
# ---- cmd/text.rs, equation.rs, drawing.rs
ROW text.insert | sel=b1:6-10 | params={"text":"x"} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,mode,math,author | note=replaces "beta"; formatting from the char before the focus (pending wins); revisions read only when tracking
ROW text.insert | sel=b1:6 (caret) | params={"text":" "} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,autocorrect,autocorrectUser,mode,math | note=AutoCorrect of the word before the caret (text before caret; props of replaced word); may set pending (URL)
ROW text.insert | sel=b5:2 (caret) | params={"text":" "} | holds=b5{text,para,@};doc{numbering,lists} | windows=doc.body | facts=none | reads=selection,pending,mode,math | note=list AutoFormat when b5 text was "1." before typing the space: reads whole text + direct numbering; bullets look up an existing list kind, numbers add a list; writes style ListParagraph [corrected C8-doc-6: sel b5:0 -> b5:2 with b5="1.": crates/engine/engine/src/cmd/text.rs:85-103 requires the caret at the paragraph end and resulting text exactly "1. "; typing at offset 0 cannot reach this path]
ROW text.insert | sel=b1:6 (caret) | params={"text":"\""} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending,mode,math | note=smart quote decided by the char before the caret
ROW text.newParagraph | sel=b1:6-10 | params={} | holds=b1{text~,para,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,autocorrect,autocorrectUser,mode,math | note=deletes "beta", splits at 6; new para copies b1 para props + mark from char before; style.next only at end [corrected C8-doc-6: crates/engine/engine/src/cmd/mod.rs:257-258 looks up styles only if at_end and a style id is present; this interior split is not at_end]
ROW text.newParagraph | sel=b0:5 (caret) | params={} | holds=b0{text,para,@};doc{styles} | windows=doc.body | facts=none | reads=selection,pending,autocorrect,mode | note=at end of Heading 1: new para takes Heading 1's next style, numbering and mark cleared
ROW text.newParagraph | sel=b3:0 (caret) | params={} | holds=b3{text,para,@} | windows=doc.body | facts=none | reads=selection,mode | note=only for an EMPTY list item: leave list / level up, no split (b3 "Two" is non-empty -> path A)
ROW text.newParagraph | sel=b6:3 (caret) | params={} | holds=b6{text,para,@};b5{para,@} | windows=doc.body | facts=none | reads=selection,previousCommand,autocorrect,mode | note=border AutoFormat when b6 text is "---" and the previous command typed one char; border goes on b5 (previous block, if a paragraph)
ROW text.lineBreak | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,mode,math,author | note=replaces selection; formatting of char before focus
ROW text.pageBreak | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,mode,math,author | note=same as lineBreak with U+000C; no check that the caret is in the body (allowed in a cell/header)
ROW text.columnBreak | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,mode,math,author | note=U+000E, same path
ROW text.nbsp | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,mode,math,author | note=U+00A0, same path
ROW text.nbHyphen | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,mode,math,author | note=U+2011, same path
ROW text.optionalHyphen | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,mode,math,author | note=U+00AD, same path
ROW text.tab | sel=b4/0/1/0:0 | params={} | holds=t4{rows};r4/0{cells,@};c4/1/0{blocks,@} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=moves to next cell and selects its content; from c4/1/1 (last cell) inserts r4/2 copied from r4/1{props} and its cells' props
ROW text.tab | sel=b2:0 (caret) | params={} | holds=b2{para,@} | windows=doc.body | facts=none | reads=selection,mode | note=direct numbering level+1; numbering from a style is ignored (tab char instead)
ROW text.tab | sel=b1:0-b3:1 | params={} | holds=b1{para,@};b2{para,@};b3{para,@};doc{styles,settings} | windows=doc.body | facts=none | reads=selection,mode | note=para.indent: list items level+1, others indent_left from resolved style indent + default tab
ROW text.tab | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,mode,math,author | note=same-paragraph selection: replaced by a tab char
ROW text.backTab | sel=b4/1/0/0:0 | params={} | holds=t4{rows};r4/0{cells,@} | windows=doc.body,t4.rows,r4/0.cells | facts=none | reads=selection | note=goes to the last cell of the previous row (cell count of r4/0); caret only
ROW text.backTab | sel=b3:0 (caret) | params={} | holds=b3{para,@} | windows=doc.body | facts=none | reads=selection,mode | note=direct numbering level-1 on the focus paragraph only, even with a multi-paragraph selection
ROW text.backTab | sel=b1:6-10 | params={} | holds=b1{para,@};doc{styles,settings} | windows=doc.body | facts=none | reads=selection,mode | note=outdent: indent_left from resolved style left indent and default tab
ROW text.backspace | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,math,author | note=deletes the selection (tracked: marks it deleted, removes own insertions)
ROW text.backspace | sel=b1:6 (caret) | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,math,author | note=deletes the grapheme before the caret; an invisible marker (bookmark Start end, comment anchor, footnote ref) is deleted like a char
ROW text.backspace | sel=b2:0 (caret) | params={} | holds=b2{para,@} | windows=doc.body | facts=none | reads=selection,mode | note=at start: numbering off (num 0), then ListParagraph style -> leave_list, then first-line indent -> 0
ROW text.backspace | sel=b1:0 (caret) | params={} | holds=b1{text,para,@};b0{text,para,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,author | note=joins b1 into b0 (keeps b0 props); tracked: b0 mark marked deleted
ROW text.backspace | sel=b5:0 (caret) | params={} | holds=b5{text,@};t4{rows};c4/1/1{blocks} | windows=doc.body | facts=none | reads=selection,mode | note=prev is table: caret to end of b4/1/1/0; empty b5 removed (container has blocks after it)
ROW text.delete | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,math,author | note=deletes the selection
ROW text.delete | sel=b1:6 (caret) | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,math,author | note=deletes the next grapheme (or the next object marker)
ROW text.delete | sel=b5:0 (caret) | params={} | holds=b5{text,para,@};b6{text,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,author | note=at end of b5: joins b6 into b5 (b5 props kept, b6 id kept since b5 empty); tracked: b5 mark marked deleted
ROW text.delete | sel=b3:3 (caret) | params={} | holds=b3{text,@};b4{kind} | windows=doc.body | facts=none | reads=selection,mode | note=next block is a table: only an EMPTY paragraph is removed (b3 "Two" is not: nothing happens)
ROW text.deleteWordBack | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,math,author | note=non-collapsed: backspace path A
ROW text.deleteWordBack | sel=b1:10 (caret) | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,math,author | note=deletes text before caret back to the word start ("beta")
ROW text.deleteWordBack | sel=b1:0 (caret) | params={} | holds=b1{text,para,@};b0{text,para,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,author | note=off 0: backspace path C/D (join into b0)
ROW text.deleteWordForward | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,math,author | note=non-collapsed: delete path A
ROW text.deleteWordForward | sel=b1:6 (caret) | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,math,author | note=deletes "beta" and what follows up to the next word (word_end: trailing spaces; a non-word char such as U+FFFC note ref ends it)
ROW text.deleteWordForward | sel=b5:0 (caret) | params={} | holds=b5{text,para,@};b6{text,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,mode,author | note=at end: delete path C (join)
ROW equation.backspace | sel=b5:0 (caret) | params={} | holds=inline:eq{value};b5{text~} | windows=none | facts=none | reads=math | note=deletes in the equation's nodes at the edit position; the equation is named by s.math.at (path+off), not by the selection
ROW equation.backspace | sel=b5:0 (caret) | params={} | holds=inline:eq{value};b5{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=math,author | note=empty equation: removes the object char (tracked: marks it deleted); s.math cleared
ROW equation.delete | sel=b5:0 (caret) | params={} | holds=inline:eq{value};b5{text~} | windows=none | facts=none | reads=math | note=forward delete in nodes; empty equation removed as above
ROW equation.convert | sel=b5:3 (caret) | params={"to":"linear"} | holds=inline:eq{value};b5{text~} | windows=doc.body | facts=none | reads=selection,math,mathLatex | note=equation at/just before the caret (or the edited one); linear text in LaTeX or UnicodeMath per mathLatex
ROW equation.convert | sel=none | params={"to":"professional","all":true} | holds=allblocks{text};allequations{value} | windows=none | facts=none | reads=selection,mathLatex,math | note=every equation in every story; OMML dropped on all; resets s.math.pos [review: handler premises and same-field protection independently checked] [review: report count is a root derived fact; comment threads are detached from Document.comments]
ROW equation.display | sel=b5:3 (caret) | params={"value":true} | holds=inline:eq{value};b5{text~} | windows=doc.body | facts=none | reads=selection,math | note=sets display (default toggles current display); clears OMML
ROW equation.enter | sel=b5:0 (caret) | params={} | holds=inline:eq{value} | windows=none | facts=none | reads=math,mathLatex | note=builds up what was typed / numbers "#(1)"
ROW equation.enter | sel=b5:0 (caret) | params={} | holds=inline:eq{value};b5{text,para,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=math,mathLatex,author | note=nothing to build: leaves the equation and splits b5 after it (OMML already cleared by with_eq) [corrected C8-doc-6: crates/engine/engine/src/cmd/mod.rs:257-258 reads styles only when the paragraph has a style id; fixture b5 has none, as fixture inserts the blank paragraph after the table and sets no style on it]
ROW equation.insertStructure | sel=b1:6-10 | params={"id":"frac.stacked"} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,math | note=not editing: replaces "beta" with a new inline equation; formatting of char before focus; not marked as a tracked insertion
ROW equation.insertStructure | sel=b5:0 (caret) | params={"linear":"a/b"} | holds=inline:eq{value} | windows=none | facts=none | reads=math | note=editing: inserted at the edit position
ROW equation.insertSymbol | sel=b5:0 (caret) | params={"char":"π"} | holds=inline:eq{value} | windows=none | facts=none | reads=math,mathNormalText | note=editing: text inserted at pos (nor style from mathNormalText)
ROW equation.insertSymbol | sel=b1:6-10 | params={"char":"π"} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,math | note=not editing: new equation from parse_linear(char) replacing the selection
ROW equation.justify | sel=b5:3 (caret) | params={"jc":"left"} | holds=inline:eq{value};b5{text~} | windows=doc.body | facts=none | reads=selection,math | note=sets jc (unknown -> centerGroup) AND forces display=true
ROW equation.number | sel=b5:3 (caret) | params={"value":true} | holds=inline:eq{value};b5{text~} | windows=doc.body | facts=none | reads=selection,math | note=reads current nodes to decide numbered; resets s.math.pos
ROW equation.set | sel=b5:3 (caret) | params={"latex":"\\frac{a}{b}"} | holds=inline:eq{value};b5{text~} | windows=doc.body | facts=none | reads=selection,math | note=replaces nodes (id/builtin also accepted); linear form flag cleared; content not read
ROW equation.structure | sel=b5:0 (caret) | params={"action":"removeLimits"} | holds=inline:eq{value} | windows=none | facts=none | reads=math | note=action resolved against the structure around s.math.pos; `level` param skips that check
ROW equation.type | sel=b5:0 (caret) | params={"text":"x^2 "} | holds=inline:eq{value} | windows=none | facts=none | reads=math,mathLatex,mathNormalText | note=edits nodes at pos; build-up depends on the input format in session
ROW insert.equation | sel=b1:6-10 | params={} | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending | note=selected "beta" becomes the equation; replaced; inline (b1 not blank); typing props of char before focus copied incl. any ins/del revision marks [corrected C8-doc-6: cmd/equation.rs:167 resets s.math before insertion; the prior math mode is never read]
ROW insert.equation | sel=b5:0 (caret) | params={"builtin":"pythagoras"} | holds=b5{text,@} | windows=doc.body | facts=none | reads=selection,pending | note=blank paragraph -> display equation; text read only to test blankness
ROW insert.equation | sel=b9:16 (caret) | params={"linear":"x","edit":false} | holds=b9{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=continuing props at caret end of " Added" carry ins=rev:0 onto the new equation (see DEFECTS) [corrected C8-doc-6: cmd/equation.rs:168-179 copies typing_props into insert_object; the revision id is copied without reading its record]
ROW drawing.arrange | sel=none | params={"ids":["7"],"order":"front"} | holds=allinline{uid,frame} | windows=none | facts=none | reads=none | note=stack = every drawing object in all stories sorted by layer(wrap) then float z then document order; inline ones error; every floating object gets z rewritten (Z_BASE+1024*i)
ROW drawing.chart | sel=none | params={"id":"9","title":"T"} | holds=inline:chart{uid,value,chart,frame};doc{passthrough,settings};allinline{uid} | windows=none | facts=layout | reads=page | note=find by uid (must be a chart); spec from graphic.spec or chart part (passthrough); part patched, redrawn with theme colours; frame/size from layout (unchanged -> no bounds)
ROW drawing.connect | sel=b1:6-10 | params={"from":"5","to":"6"} | holds=inline:a{uid,frame,value};inline:b{uid,frame,value};allinline{uid,frame};b1{text~,@};bA{text~,@};bB{text~,@};bK{text~,@} | windows=doc.body | facts=layout | reads=selection,page,pending | note=ends must be floating shapes; sites from their layout frames; selection collapsed to focus and connector inserted at the focus (b1:10) via insert.shape, then moved by arrange.bounds to the paragraph layout puts under its y (writes that block's text); glue set; z on top of all floats [review: handler premises and same-field protection independently checked]
ROW drawing.connect | sel=b4/0/1/0:0 | params={"from":"5","to":"6"} | holds=inline:a{uid,frame,value};inline:b{uid,frame,value};allinline{uid};bA{text~,@};bB{text~,@};b4/0/1/0{text~,@} | windows=doc.body,t4.rows,r4/0.cells | facts=layout | reads=selection,page,pending | note=caret in a table cell or header: connector inserted there, move_object refuses ("only objects in the body") -> command fails (DEFECT) [review: handler premises and same-field protection independently checked]
ROW drawing.delete | sel=b1:6-10 | params={"ids":["1"]} | holds=inline:shape{uid*};b8{text~};allinline{uid};doc{parts} | windows=none | facts=none | reads=selection | note=find by uid; describes first (layout) only to validate; removes the object char with doc.delete_range (never tracked); text-box story pruned; selection clamped; connectors glued to it keep dangling ids (no reroute) [corrected C8-doc-6: facts=layout -> none: the layout-derived `before` (crates/engine/engine/src/cmd/drawing.rs:797, to_drawing) is not used by a delete, which removes the object's character and returns (drawing.rs:802-810)]
ROW drawing.format | sel=none | params={"ids":["1"],"fill":{"color":"#FF0000"}} | holds=inline:shape{uid,look,value,frame};b8{text~};allinline{uid,frame} | windows=none | facts=frameValidity | reads=page | note=before from to_drawing; writes fill/gradient/stroke/dash/geometry (canonical kind unless text box); group: refused unless only frame changes; then reroute_connectors (all connectors) [corrected C8-doc-6: facts=layout -> none: the laid-out frame (drawing.rs:797, to_drawing) is written back only when moved (drawing.rs:814-815, write_back 1104-1108), and a format changes no frame; reroute_connectors reads the layout only of connectors (drawing.rs:450-452), and the fixture has none] [review: write_back validates the frame even without moving it; the validity result, rather than all paragraph geometry, is the dependency]
ROW drawing.group | sel=b1:6-10 | params={"ids":["5","6"]} | holds=inline:a{uid,frame,value};inline:b{uid,frame,value};bA{text~};bB{text~};allinline{uid} | windows=none | facts=layout | reads=page | note=each found by uid, must be floating, not chart; overwrites selection + also_selected then nested arrange.group: body depth 0 only, same layout page, bounding box from layout rects; members deleted, group inserted at first one's place then moved by move_object (layout anchor paragraph)
ROW drawing.insert | sel=b1:6-10 | params={"geometry":{"preset":"ellipse"}} | holds=b1{text~,@};doc{settings,revisions};allinline{uid} | windows=doc.body | facts=layout,time | reads=selection,pending,page | note=no frame: replaces "beta" with an inline shape at the focus, typing props of char before focus; geometry written
ROW drawing.insert | sel=b1:6 (caret) | params={"frame":{"x":100,"y":300,"w":120,"h":80},"placement":{"page":0}} | holds=b1{text~,@};bK{text~,@};allinline{uid,frame} | windows=doc.body | facts=layout | reads=selection,pending,page | note=inserted at caret then arrange.bounds moves it to the top-level paragraph layout finds under y on the page (bK) and floats it; z on top of all floats
ROW drawing.insert | sel=b1:6 (caret) | params={"kind":"textBox","text":{"paragraphs":[]}} | holds=b1{text~,@};doc{parts};allinline{uid} | windows=doc.body | facts=layout | reads=selection,pending,page | note=new TextBox part; story found by s.sel.focus.story after insert.textBox; text into its blocks
ROW drawing.insert | sel=b1:6 (caret) | params={"kind":"chart","chart":{"kind":"pie"}} | holds=b1{text~,@};doc{settings};allinline{uid} | windows=doc.body | facts=layout | reads=selection,pending,page | note=inline chart at caret; theme colours for drawing; caret after it
ROW drawing.reconnect | sel=none | params={"id":"7","end":"end","to":"6","site":2} | holds=inline:conn{uid,value,frame};inline:b{uid,frame};allinline{uid,frame} | windows=none | facts=layout | reads=page | note=connector by uid; new glue end must exist (find_shape); frame from office-core op; moved -> arrange.bounds; then reroute all connectors
ROW drawing.text | sel=none | params={"id":"1","text":"Hi"} | holds=inline:shape{uid,value,frame};b8{text~};doc{settings,parts};allinline{uid} | windows=none | facts=frameValidity | reads=page | note=shape without story: new TextBox part, story set, first text centred, white from theme lt1, anchor middle; with story: part blocks replaced wholesale; groups/charts/ink refused [corrected C8-doc-6: facts=layout -> none: the laid-out frame (drawing.rs:797, to_drawing) is written back only when moved (drawing.rs:814-815, write_back 1104-1108), and a text changes no frame; reroute_connectors reads the layout only of connectors (drawing.rs:450-452), and the fixture has none] [review: write_back validates the frame even without moving it; the validity result, rather than all paragraph geometry, is the dependency]
ROW drawing.transform | sel=none | params={"ids":["1"],"dx":10} | holds=inline:shape{uid,frame,value};b8{text~,@};bK{text~,@};allinline{uid,frame} | windows=doc.body | facts=layout | reads=page | note=frame from layout; moved -> arrange.bounds: inline shape starts floating (Square) and its char moves to the paragraph layout puts under the new y (bK); selection set to it; rot/flip into float spin; reroute
ROW drawing.transform | sel=none | params={"ids":["1"],"rotateBy":90} | holds=inline:shape{uid,frame,value};allinline{uid,frame} | windows=none | facts=layout | reads=page | note=spin only, no bounds call
ROW drawing.transform | sel=none | params={"ids":["1","5"],"dx":10} | holds=inline:shape{uid,frame,value};inline:a{uid,frame,value};allinline{uid,frame} | windows=doc.body | facts=layout | reads=page | note=all "before" frames from one layout; the second object's absolute x/y can be stale after the first moved/reflowed (DEFECT)
ROW drawing.ungroup | sel=b1:6-10 | params={"ids":["10"]} | holds=inline:group{uid,frame,value};bG{text~,@};allinline{uid} | windows=none | facts=layout | reads=page | note=group by uid; selection overwritten; nested arrange.ungroup: inline group first floated via move_object (layout), floating one unaligned (layout); group char replaced by member chars with offsets from group float; members' ids answered; also_selected set by arrange.ungroup is cleared again at the end of drawing.ungroup
ROW insert.chart | sel=b1:6-10 | params={"type":"pie"} | holds=b1{text~,@};doc{settings,revisions};allinline{uid} | windows=doc.body | facts=layout,time | reads=selection,pending,page | note=type->kind, width/height->frame w/h (no x/y so never moved); inline chart replaces the selection at the focus; typing props of char before focus; theme colours; caret after; no on_top
ROW insert.chart | sel=b5:0 (caret) | params={"categories":["a"],"series":[{"name":"s","values":[1]}],"width":300} | holds=b5{text~,@};doc{settings};allinline{uid} | windows=doc.body | facts=layout | reads=selection,pending,page | note=data from params; height default 252
# ---- cmd/page.rs, file.rs, design.rs
ROW layout.break | sel=b1:6-10 | params={"kind":"page"} | holds=b1{text~,@} | windows=doc.body | facts=none | reads=selection,pending | note=deletes "beta" then inserts the page-break char; typing props = pending or formatting continuing at off 6
ROW layout.break | sel=b1:6-10 | params={"kind":"page"} +trackChanges | holds=b1{text~,@};doc{settings,revisions} | windows=doc.body | facts=time | reads=selection,pending,author | note=track_delete marks "beta" deleted (own insertions are removed); new Insert revision by session author/date
ROW layout.break | sel=b1:6-10 | params={"kind":"nextPage"} | holds=b1{text~,para,@};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=focus must be in the body at top level; deletes the selection, splits b1 at 6 (text, run marks, bookmark, footnote ref after 6 go to the new paragraph); b1.para.section = a copy of the current section; the following section (lastSection here) = the copy with start=nextPage [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.break | sel=b4/0/1/0:0 | params={"kind":"continuous"} | holds=none | windows=none | facts=none | reads=selection | note=refused (focus in a table cell); page/column/textWrapping are not refused there
ROW layout.columns | sel=b1:6-10 | params={"count":2} | holds=sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=section found from the selection's top-level block indices; writes columns; text width read from the focus section [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.columns | sel=b1:6-10 | params={"widths":[[150,24],[200,0]]} | holds=sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=widths checked against the focus section's text width (page_w, margins, gutter) [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.columns | sel=none | params={"count":3,"apply":"document"} | holds=bodyblocks{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=every section's columns; selection read only for the answer and the text width (tw from the focus section) [corrected C8-doc-6: allblocks{para} -> bodyblocks{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747)]
ROW layout.columns | sel=b1:6-10 | params={"count":2,"apply":"forward"} | holds=b1{text~,para,@};sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=caret at b1:6 (the selection is NOT deleted: the caret is collapsed first), continuous section break splits b1, then the new section's columns [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.differentFirstPage | sel=b1:6-10 | params={} | holds=sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=toggle from the focus section's titlePg, written to each section the selection spans [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.differentFirstPage | sel=b1:6-10 | params={"value":true} | holds=sectionspan{para};doc{lastSection*} | windows=doc.body | facts=none | reads=selection | note=sets it without reading the current value [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.differentOddEven | sel=none | params={} | holds=doc{settings} | windows=none | facts=none | reads=none | note=toggle reads settings.evenAndOddHeaders
ROW layout.differentOddEven | sel=none | params={"value":true} | holds=doc{settings*} | windows=none | facts=none | reads=none | note=set without reading
ROW layout.hyphenation | sel=none | params={} | holds=doc{settings} | windows=none | facts=none | reads=none | note=toggle of settings.autoHyphenation
ROW layout.hyphenation | sel=none | params={"value":false} | holds=doc{settings*} | windows=none | facts=none | reads=none | note=set
ROW layout.lineNumbers | sel=b1:6-10 | params={"value":"restartPage"} | holds=sectionspan{para};doc{lastSection*} | windows=doc.body | facts=none | reads=selection | note=section(s) from the selection; lnNumType replaced wholesale, not read [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.margins | sel=b1:6-10 | params={"preset":"narrow"} | holds=sectionspan{para};doc{lastSection,settings*} | windows=doc.body | facts=none | reads=selection | note=room checked against the focus section's page size and gutter; writes margins, settings.mirrorMargins [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.margins | sel=b1:6-10 | params={"top":50} | holds=sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=bottom/left/right taken from the focus section, then written to every section of the selection; mirror setting untouched [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.orientation | sel=b1:6-10 | params={"value":"landscape"} | holds=sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=set_landscape reads each section's page_w/page_h to swap them [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.orientation | sel=b1:6-10 | params={} | holds=sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=toggle from the focus section's orientation, applied to all sections in the selection [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.pageNumberFormat | sel=b1:6-10 | params={"format":"lowerRoman","start":1} | holds=sectionspan{para};doc{lastSection*} | windows=doc.body | facts=none | reads=selection | note=pgNumType fmt/start written blind [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.pageNumberFormat | sel=b1:6-10 | params={"format":"lowerRoman"} | holds=sectionspan{para};doc{lastSection*} | windows=doc.body | facts=none | reads=selection | note=clears an existing start number (see DEFECTS) [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.pageSetup | sel=b1:6-10 | params={"section":{"marginTop":50}} | holds=sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=fields not given keep each section's own values (the section is read whole) [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.pageSetup | sel=b1:6-10 | params={} | holds=sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=no write: opens the dialog (ui_requests) and answers the focus section [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.size | sel=b1:6-10 | params={"name":"A4"} | holds=sectionspan{para};doc{lastSection} | windows=doc.body | facts=none | reads=selection | note=reads each section's orientation [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.size | sel=b1:6-10 | params={"width":612,"height":792} | holds=sectionspan{para};doc{lastSection*} | windows=doc.body | facts=none | reads=selection | note=written blind [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW layout.verticalAlign | sel=b1:6-10 | params={"value":"center"} | holds=sectionspan{para};doc{lastSection*} | windows=doc.body | facts=none | reads=selection | note=vAlign of each section the selection spans [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW document.setText | sel=none | params={"text":"x\ny"} | holds=doc{body*} | windows=none | facts=none | reads=none | note=body replaced wholesale; the old blocks are not read; selection written (caret at start). Leaves thread:0/comment:0/1 without an anchor, part:footnote without a reference, rev:0 without text, and multi-section setups collapse into lastSection
ROW file.close | sel=none | params={} | holds=none | windows=none | facts=none | reads=selection | note=UI request only
ROW file.encrypt | sel=none | params={"password":"pw"} | holds=none | windows=none | facts=none | reads=password | note=writes session password + dirty; nothing in the document
ROW file.new | sel=none | params={"template":"letter"} | holds=doc{*} | windows=none | facts=none | reads=none | note=the whole document is replaced from a built-in template; the old one is not read
ROW file.open | sel=none | params={"path":"a.docx"} | holds=doc{*} | windows=none | facts=file | reads=none | note=document replaced from the file
ROW file.open | sel=none | params={"path":"a.docx","data":"…","password":"pw"} | holds=doc{*} | windows=none | facts=none | reads=none | note=bytes from the params; password kept for later saves when the file was encrypted
ROW file.open | sel=none | params={} | holds=none | windows=none | facts=none | reads=selection | note=UI request only
ROW file.properties | sel=none | params={"title":"T"} | holds=doc{core*} | windows=none | facts=none | reads=none | note=written blind
ROW file.properties | sel=none | params={"custom":{"k":"v","old":null}} | holds=doc{customProps} | windows=none | facts=none | reads=none | note=set/remove by name (remove reads which exists)
ROW file.save | sel=none | params={"path":"out.docx"} | holds=doc{*,core} | windows=none | facts=file,time | reads=other(path,password,author,saved,dirty) | note=whole document serialized to a file; core stamped when the document differs from the last save
ROW file.save | sel=none | params={} | holds=none | windows=none | facts=none | reads=other(path) | note=no path known: opens Save As, no write
ROW file.saveAs | sel=none | params={"path":"out.docx","password":"pw"} | holds=doc{*,core} | windows=none | facts=file,time | reads=other(password,author,saved) | note=as file.save
ROW para.insert | sel=b1:6-10 | params={"after":2,"text":"Mid"} | holds=b2{text,para,@};doc{styles,settings} | windows=none | facts=none | reads=other(autocorrectOn,autocorrectUser,typedJustBefore) | note=AutoCorrect reads/rewrites b2's last word "One"; the new paragraph copies b2's para (same numId → a new list item) and continues its mark formatting; the selection is replaced (not read) [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW para.insert | sel=b1:6-10 | params={"after":0,"text":"X"} | holds=b0{text,para,@};doc{styles,settings} | windows=none | facts=none | reads=other(autocorrectOn,autocorrectUser) | note=Heading 1's next style (Normal) for the new paragraph, numbering and mark reset; single-char text triggers autoformat [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW para.insert | sel=b1:6-10 | params={"before":6,"text":"Pre"} | holds=b6{para,text~,@};doc{settings} | windows=none | facts=none | reads=none | note=split at 0: the empty head keeps b6's props (paraId moves to the tail); text typed in a copy of b6's paragraph style; thread:0's anchor stays with "After" [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW para.insert | sel=b1:6-10 | params={"after":4,"text":"x"} | holds=b4{*} | windows=doc.body | facts=none | reads=none | note=refused: block 4 is a table
ROW para.insert | sel=b1:6-10 | params={"after":9,"text":"Y"} +trackChanges | holds=b9{text,para,@};doc{settings,revisions,styles} | windows=none | facts=time | reads=author | note=while tracking, an Insert revision for the paragraph mark and the typed text [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW para.text | sel=b1:6-10 | params={"index":1,"text":"New"} | holds=b1{text,para*,@} | windows=none | facts=none | reads=none | note=all of b1's text replaced: bookmark Start and the footnote reference go with it (part:footnote left orphaned); the new text takes the run formatting continuing at b1's end (typing_props reads focus = end, before the delete) [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW para.text | sel=b1:6-10 | params={"index":9,"text":"New"} +trackChanges | holds=b9{text,@};doc{settings,revisions} | windows=none | facts=time | reads=author | note=track_delete: rev:0's " Added" removed if inserted by the same author, else marked deleted; the rest marked deleted; new Insert revision [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW para.text | sel=b1:6-10 | params={"index":5,"text":""} | holds=b5{text,@} | windows=none | facts=none | reads=none | note=empty paragraph and empty text: only the caret moves [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW para.text | sel=b1:6-10 | params={"index":6,"text":"x"} | holds=b6{text,@};doc{settings} | windows=none | facts=none | reads=other(autocorrectOn) | note=single-char text runs text.insert's autoformat (smart quotes for '"', AutoCorrect for punctuation); thread:0's anchor on "After" is deleted with the text [review: deletion holds discarded fields; indexed positions do not require whole-list membership]
ROW design.pageBorders | sel=b1:6-10 | params={"kind":"box"} | holds=sectionspan{para};doc{lastSection*} | windows=doc.body | facts=none | reads=selection | note=the focus block's section; written blind [corrected C8-doc-6: allblocks{para} -> sectionspan{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747), and the selection's section is the first one ending at or after its block (section_index_of, lib.rs:750-752; page.rs with_sect:124-126): breaks before it do not change it]
ROW design.pageBorders | sel=none | params={"kind":"none","applyTo":"document"} | holds=bodyblocks{para};doc{lastSection*} | windows=doc.body | facts=none | reads=none | note=every section [corrected C8-doc-6: allblocks{para} -> bodyblocks{para}: a section break is only on a top-level body paragraph (Document::sections, crates/engine/doc/src/lib.rs:736-747)]
ROW design.pageColor | sel=none | params={"color":"FF0000"} | holds=doc{settings*} | windows=none | facts=none | reads=none | note=written blind; a bad colour silently removes the page colour
ROW design.paragraphSpacing | sel=none | params={"value":"compact"} | holds=doc{styles*} | windows=none | facts=none | reads=none | note=docDefaults pPr written blind
ROW design.styleSet | sel=none | params={"name":"lines"} | holds=doc{styles,settings,lastSection};allblocks{text,para};part:header{blocks};part:footer{blocks};part:footnote{blocks} | windows=none | facts=none | reads=none | note=builtin styles rebuilt; customs kept; the theme accent and fonts (settings) read; retheme rewrites run/para formatting carrying a theme reference anywhere
ROW design.theme | sel=none | params={"name":"Studio"} | holds=doc{settings,styles,lastSection};allblocks{text,para};part:header{blocks};part:footer{blocks};part:footnote{blocks} | windows=none | facts=none | reads=none | note=fonts and colours of styles by id/font; theme refs re-resolved through the whole document (comment bodies are parts too)
ROW design.themeColors | sel=none | params={"name":"Gallery"} | holds=doc{settings,styles,lastSection};allblocks{text,para};part:header{blocks};part:footer{blocks};part:footnote{blocks} | windows=none | facts=none | reads=none | note=only accent1 changes; retheme over the whole document
ROW design.themeColors | sel=none | params={} | holds=none | windows=none | facts=none | reads=none | note=Params error: `name` missing (contradicts its params doc)
ROW design.themeFonts | sel=none | params={"heading":"Georgia","body":"Arial"} | holds=doc{settings,styles,lastSection};allblocks{text,para};part:header{blocks};part:footer{blocks};part:footnote{blocks} | windows=none | facts=none | reads=none | note=styles mapped by id/current font; theme font refs re-resolved everywhere
ROW design.watermark | sel=none | params={"text":"DRAFT"} | holds=doc{settings} | windows=none | facts=none | reads=none | note=the fields not given keep the current watermark's
ROW design.watermark | sel=none | params={"remove":true} | holds=doc{settings*} | windows=none | facts=none | reads=none | note=removed blind
"##;

fn run(doc: &mut DocDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn at(path: &[usize], off: usize) -> Value {
    json!({ "story": "body", "path": path, "off": off })
}

fn select(doc: &mut DocDocument, anchor: Value, focus: Value) {
    run(doc, "select.range", json!({ "anchor": anchor, "focus": focus }));
}

fn png() -> String {
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 30, 30, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
}

pub fn fixture() -> DocDocument {
    let mut doc = DocDocument::create("Report.docx", 0, 0).unwrap();
    let lines = ["Title", "Alpha beta gamma.", "One", "Two", "After table.", "Picture here", "Shape here", "Last words."];
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            run(&mut doc, "text.newParagraph", json!({}));
        }
        run(&mut doc, "text.insert", json!({ "text": line }));
    }
    select(&mut doc, at(&[0], 0), at(&[0], 0));
    run(&mut doc, "para.style", json!({ "style": "Heading 1" }));
    select(&mut doc, at(&[2], 0), at(&[3], 0));
    run(&mut doc, "para.numbering", json!({}));
    // The table after "Two".
    select(&mut doc, at(&[3], 3), at(&[3], 3));
    run(&mut doc, "text.newParagraph", json!({}));
    run(&mut doc, "insert.table", json!({ "rows": 2, "cols": 2 }));
    for (row, col, text) in [(0, 0, "A1"), (0, 1, "B1"), (1, 0, "A2"), (1, 1, "B2")] {
        select(&mut doc, at(&[4, row, col, 0], 0), at(&[4, row, col, 0], 0));
        run(&mut doc, "text.insert", json!({ "text": text }));
    }
    // A comment on "After" with a reply.
    select(&mut doc, at(&[6], 0), at(&[6], 5));
    let thread = run(&mut doc, "comment.add", json!({ "text": "Check this", "name": "Ann" }));
    run(&mut doc, "comment.reply", json!({ "thread": thread["thread"], "text": "Done", "name": "Bo" }));
    // A picture after "Picture here", a shape after "Shape here". The shape first: it is
    // number 1, as the readings' drawing numbers say (a picture has a number too since C8-doc-6,
    // the next one).
    select(&mut doc, at(&[8], 10), at(&[8], 10));
    run(&mut doc, "insert.shape", json!({ "kind": "rectangle", "width": 30, "height": 20 }));
    select(&mut doc, at(&[7], 12), at(&[7], 12));
    run(&mut doc, "insert.picture", json!({ "data": png(), "width": 20 }));
    // An equation in the empty paragraph after the table.
    select(&mut doc, at(&[5], 0), at(&[5], 0));
    run(&mut doc, "insert.equation", json!({ "linear": "x^2", "edit": false }));
    // A bookmark on "Alpha", a footnote after "beta".
    select(&mut doc, at(&[1], 0), at(&[1], 5));
    run(&mut doc, "insert.bookmark", json!({ "name": "Start" }));
    select(&mut doc, at(&[1], 10), at(&[1], 10));
    run(&mut doc, "references.footnote", json!({ "text": "A note." }));
    run(&mut doc, "insert.header", json!({ "text": "Head" }));
    run(&mut doc, "insert.footer", json!({ "text": "Foot" }));
    // A tracked insertion at the end of "Last words."; tracking off again.
    select(&mut doc, at(&[9], 11), at(&[9], 11));
    run(&mut doc, "review.trackChanges", json!({ "value": true }));
    run(&mut doc, "text.insert", json!({ "text": " Added" }));
    run(&mut doc, "review.trackChanges", json!({ "value": false }));
    // The readings' successful custom-style deletion paths need those styles to exist.
    select(&mut doc, at(&[9], 0), at(&[9], 0));
    run(&mut doc, "styles.create", json!({ "name": "Mine", "fromSelection": false }));
    run(&mut doc, "para.style", json!({ "style": "Normal" }));
    run(&mut doc, "table.newStyle", json!({ "name": "MyTable", "apply": false }));
    let bytes = doc.save(None).unwrap();
    let mut doc = DocDocument::open("Report.docx", &bytes).unwrap();
    select(&mut doc, at(&[1], 6), at(&[1], 10));
    doc
}

// ---- The comparison (written after the readings above; it does not change them).

/// Names of the fixture's objects by the tool ids `under` gives them.
fn names(doc: &mut DocDocument) -> (BTreeMap<String, String>, Vec<String>) {
    let snapshot = doc.snapshot().unwrap();
    let replica = Replica::load(DocModel::schema(), &snapshot, ActorId::random()).unwrap();
    let blobs = MemoryBlobs::default();
    let (document, _) = kdm_model::read_document(&replica, &blobs).unwrap();
    let mut map = BTreeMap::new();
    let mut paragraphs = Vec::new();
    map.insert("document".to_owned(), "doc".to_owned());
    fn walk(blocks: &[std::sync::Arc<Block>], prefix: &str, map: &mut BTreeMap<String, String>, paragraphs: &mut Vec<String>) {
        for (i, block) in blocks.iter().enumerate() {
            let name = format!("{prefix}{i}");
            match &**block {
                Block::Para(p) => {
                    if let Some(id) = p.props.para_id {
                        map.insert(format!("p{id:08X}"), name.clone());
                    }
                    paragraphs.push(name);
                }
                Block::Table(t) => {
                    let n = name.trim_start_matches('b');
                    if let Some(id) = t.rows.first().and_then(|r| r.props.para_id) {
                        map.insert(format!("t{id:08X}"), format!("t{n}"));
                    }
                    for (r, row) in t.rows.iter().enumerate() {
                        if let Some(id) = row.props.para_id {
                            map.insert(format!("r{id:08X}"), format!("r{n}/{r}"));
                        }
                        for (c, cell) in row.cells.iter().enumerate() {
                            walk(&cell.blocks, &format!("{name}/{r}/{c}/"), map, paragraphs);
                        }
                    }
                }
            }
        }
    }
    walk(&document.body, "b", &mut map, &mut paragraphs);
    for (part_id, part) in &document.parts {
        let kind = match part.kind {
            PartKind::Header => "header",
            PartKind::Footer => "footer",
            PartKind::Footnote => "footnote",
            PartKind::Endnote => "endnote",
            PartKind::Comment => "comment",
            PartKind::TextBox => "textbox",
        };
        let kind = if part.kind == PartKind::Comment {
            document.comments.values().enumerate().find(|(_, c)| c.part == *part_id).map_or(kind.to_owned(), |(n, _)| format!("comment{n}"))
        } else {
            kind.to_owned()
        };
        walk(&part.blocks, &format!("part:{kind}/"), &mut map, &mut paragraphs);
    }
    // Parts by their tool ids (C8-doc-6): `part:<kind>`, the fixture having one of each.
    for name in document.part_names().into_values() {
        let kind = name.split('-').next().unwrap_or_default().to_owned();
        map.insert(format!("part:{name}"), format!("part:{kind}"));
    }
    // Comments and threads by their tool ids (C8-doc-6): `comment:N` in the document's comment
    // order (the fixture's comment 0 and its reply 1), `thread:N` in its first comments' order.
    let mut thread = 0;
    for (n, (id, c)) in document.comments.iter().enumerate() {
        if c.durable_id.is_empty() {
            continue;
        }
        map.insert(format!("comment:{}", c.durable_id), format!("comment:{n}"));
        if c.parent.is_none_or(|p| !document.comments.contains_key(&p) || p == *id) {
            map.insert(format!("thread:{}", c.durable_id), format!("thread:{thread}"));
            thread += 1;
        }
    }
    for object in replica.objects_of_type("Inline") {
        let uid = replica.get(&object, "uid").unwrap_or_default();
        let value = replica.get(&object, "value").unwrap_or_default();
        // A picture and an equation have their numbers too (C8-doc-6).
        let kind = match value["type"].as_str() {
            Some("image") => "inline:pic",
            Some("equation") => "inline:eq",
            _ => "inline:shape",
        };
        if let Some(uid) = uid.as_u64() {
            let kind = match uid {
                5 => "inline:a",
                6 => "inline:b",
                7 => "inline:conn",
                9 => "inline:chart",
                10 => "inline:group",
                13 => "inline:X",
                _ => kind,
            };
            map.insert(uid.to_string(), kind.to_owned());
        }
    }
    (map, paragraphs)
}

/// What one row holds: objects with their fields (`@` its place, `*` only that it is there,
/// `ALL` every field), windows (`holder.list`), facts, editor state read.
#[derive(Debug, Default, PartialEq, Clone, serde::Serialize)]
struct Holds {
    objects: BTreeMap<String, BTreeSet<String>>,
    windows: BTreeSet<String>,
    facts: BTreeSet<String>,
    reads: BTreeSet<String>,
}

/// Owning fields hold their descendants' content as well as list membership. The pinned SDK
/// guard.rs:3030-3044 follows these schema edges even across a descendant's own tool id.
/// Text marks and non-owning refs are deliberately not edges (notably paragraph -> Inline).
struct Ownership(BTreeMap<(String, String), BTreeSet<String>>, BTreeMap<String, Vec<String>>);

impl Ownership {
    fn fixture(doc: &mut DocDocument, names: &BTreeMap<String, String>) -> Self {
        let replica = Replica::load(DocModel::schema(), &doc.snapshot().unwrap(), ActorId::random()).unwrap();
        let schema = replica.schema();
        let mut named = BTreeMap::new();
        for (ty, def) in &schema.types {
            for id in replica.objects_of_type(ty) {
                if ty == "Document" {
                    named.insert(id, "doc".to_owned());
                } else if let Some(tool) = &def.tool_id {
                    let value = replica.get(&id, &tool.field).unwrap();
                    let value = value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string());
                    if let Some(name) = names.get(&format!("{}{value}", tool.prefix)) {
                        named.insert(id, name.clone());
                    }
                }
            }
        }
        // The readings name Table fields on their owning Block (t4), and a cell by its
        // row/column indexes. Neither unnamed object introduces a new tool identity.
        for (id, name) in named.clone() {
            match replica.object_type(&id).unwrap().as_str() {
                "Block" => {
                    if let Some(table) = replica.get(&id, "table").unwrap().as_str() {
                        named.insert(ObjectId::from_string(table), name);
                    }
                }
                "Row" => {
                    for (c, cell) in replica.list(&id, "cells").unwrap().into_iter().enumerate() {
                        named.insert(cell, format!("c{}/{c}", name.trim_start_matches('r')));
                    }
                }
                "Document" => {
                    for (i, rev) in replica.list(&id, "revisions").unwrap().into_iter().enumerate() {
                        named.insert(rev, format!("rev:{i}"));
                    }
                    for media in replica.list(&id, "media").unwrap() {
                        let key = replica.get(&media, "key").unwrap();
                        named.insert(media, format!("media:{}", key.as_str().unwrap()));
                    }
                }
                _ => {}
            }
        }
        let mut edges: BTreeMap<ObjectId, Vec<(String, ObjectId)>> = BTreeMap::new();
        for (ty, def) in &schema.types {
            for id in replica.objects_of_type(ty) {
                for (field, def) in def.fields.iter().filter(|(_, def)| def.owning()) {
                    let children = match def.kind {
                        FieldKind::List => replica.list(&id, field).unwrap(),
                        FieldKind::Tree => replica.tree(&id, field).unwrap().into_iter().map(|n| n.object).collect(),
                        FieldKind::Ref => replica.get(&id, field).unwrap().as_str().map(ObjectId::from_string).into_iter().collect(),
                        FieldKind::Map => {
                            let map = Value::Object(replica.map_entries(&id, field).unwrap().into_iter().collect());
                            kition_kit::kdm::FieldDef::map_held(&map, |s| replica.exists(&ObjectId::from_string(s)))
                                .into_iter()
                                .map(|(id, _)| ObjectId::from_string(id))
                                .collect()
                        }
                        _ => Vec::new(),
                    };
                    edges.entry(id.clone()).or_default().extend(children.into_iter().map(|child| (field.clone(), child)));
                }
            }
        }
        let mut owned: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for (id, name) in &named {
            for (field, child) in edges.get(id).into_iter().flatten() {
                let mut queue = vec![child.clone()];
                let mut seen = BTreeSet::new();
                while let Some(child) = queue.pop() {
                    if !seen.insert(child.clone()) {
                        continue;
                    }
                    if let Some(child_name) = named.get(&child).filter(|n| *n != name) {
                        owned.entry((name.clone(), field.clone())).or_default().insert(child_name.clone());
                    }
                    queue.extend(edges.get(&child).into_iter().flatten().map(|(_, id)| id.clone()));
                }
            }
        }
        Self(owned, anchors(doc, names))
    }

    fn supplied(&self, holds: &Holds) -> BTreeSet<String> {
        self.0
            .iter()
            .filter(|((owner, field), _)| holds.objects.get(owner).is_some_and(|f| f.contains("ALL") || f.contains(field)))
            .flat_map(|(_, children)| children.iter().cloned())
            .collect()
    }

    fn supplied_content(&self, holds: &Holds) -> BTreeSet<String> {
        let mut out = self.supplied(holds);
        // The guard attributes a descendant's private derived changes to that descendant
        // too. Owning it whole therefore covers its ~objects, unlike holding only its text.
        let whole = holds.objects.iter().filter(|(_, fields)| fields.contains("ALL")).map(|(name, _)| name.clone());
        for holder in out.clone().into_iter().chain(whole) {
            out.extend(self.1.get(&holder).into_iter().flatten().cloned());
        }
        out
    }
}

fn field_name(path: &str) -> String {
    if path == "props.blocks" {
        return "blocks".into();
    }
    // A JSON Pointer (`/styles/Heading1`, a map entry): its first segment is the field.
    if let Some(pointer) = path.strip_prefix('/') {
        return pointer.split('/').next().unwrap_or_default().to_owned();
    }
    let name = path.split(['.', '[', '(']).next().unwrap_or_default().trim();
    // `sources@`: the field and its order (C8-doc-6): a list field held holds its order.
    if name == "*" || name == "@" { name.to_owned() } else { name.trim_end_matches(['~', '*', '@']).to_owned() }
}

/// The list holding `object`, by the fixture's names: where its place (`@`) is counted.
fn parent_list(object: &str) -> String {
    if let Some(rest) = object.strip_prefix("part:") {
        return match rest.split_once('/') {
            Some((kind, _)) => format!("part:{kind}.blocks"),
            None => "doc.parts".to_owned(),
        };
    }
    let steps: Vec<&str> = object.split('/').collect();
    match (object.chars().next(), steps.len()) {
        (Some('b'), 1) | (Some('t'), 1) => "doc.body".to_owned(),
        (Some('b'), 4) => format!("c{}/{}/{}.blocks", steps[0].trim_start_matches('b'), steps[1], steps[2]),
        (Some('r'), 2) => format!("t{}.rows", steps[0].trim_start_matches('r')),
        (Some('c'), 3) if !object.starts_with("comment") => format!("r{}/{}.cells", steps[0].trim_start_matches('c'), steps[1]),
        _ if object.starts_with("thread:") => "doc.comments".to_owned(),
        _ if object.starts_with("rev:") => "doc.revisions".to_owned(),
        _ if object.starts_with("media:") => "doc.media".to_owned(),
        _ => format!("{object}.place"),
    }
}

/// `b4` names body block 4 even when it is a table. `names` calls that same Block `t4`
/// by its tool id (the schema's Block.table owns the Table). Translate only actual fixture
/// tables; a paragraph or a symbolic block name is not a table alias.
fn table_block_names(holds: &mut Holds, names: &BTreeMap<String, String>) {
    for table in names.values().filter(|n| n.starts_with('t') && n[1..].chars().all(|c| c.is_ascii_digit())) {
        if let Some(fields) = holds.objects.remove(&format!("b{}", &table[1..])) {
            holds.objects.entry(table.clone()).or_default().extend(fields);
        }
    }
}

/// A reading's row in [`Holds`].
fn parse_row(line: &str, paragraphs: &[String], parts: &BTreeMap<String, u32>) -> (String, Value, Option<(Value, Value)>, Holds) {
    let cells: Vec<&str> = line.trim_start_matches("ROW ").split(" | ").collect();
    let command = cells[0].trim().to_owned();
    let mut holds = Holds::default();
    let (mut params, mut sel) = (json!({}), None);
    for cell in &cells[1..] {
        let Some((key, value)) = cell.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "sel" => sel = selection(value, parts),
            "params" => {
                // The JSON, and after it a variant of the fixture (`+trackChanges`).
                let mut stream = serde_json::Deserializer::from_str(value).into_iter::<Value>();
                params = stream.next().and_then(Result::ok).unwrap_or_else(|| panic!("{command}: params {value}"));
            }
            "holds" => {
                for item in value.split(';').map(str::trim).filter(|s| !s.is_empty() && *s != "none" && *s != "-") {
                    let (object, fields) = item.split_once('{').unwrap_or((item, "*}"));
                    // `doc.body{@}`, `doc.parts{*}`: the root's list itself (its members and their
                    // order), the window on it (C8-doc-6).
                    if let Some(list) = object.trim().strip_prefix("doc.").filter(|list| root_list_field(list)) {
                        holds.windows.insert(format!("doc.{list}"));
                        continue;
                    }
                    let fields: BTreeSet<String> =
                        fields.trim_end_matches('}').split(',').map(str::trim).filter(|f| !f.is_empty()).map(field_name).collect();
                    let objects: Vec<String> = match object.trim() {
                        "allblocks" => paragraphs.to_vec(),
                        "bodyparas" => paragraphs.iter().filter(|p| p.starts_with('b')).cloned().collect(),
                        // Corrections (C8-doc-6): the top-level body paragraphs, where section
                        // breaks are; from the selection's first block (the fixture has one
                        // section, so its section ends at the last block).
                        "bodyblocks" => paragraphs.iter().filter(|p| is_top_level(p)).cloned().collect(),
                        "sectionspan" => paragraphs.iter().filter(|p| is_top_level(p) && block_of(p) >= first_block(&cells)).cloned().collect(),
                        other => vec![other.to_owned()],
                    };
                    for object in objects {
                        holds.objects.entry(object).or_default().extend(fields.iter().cloned());
                    }
                }
            }
            "windows" => holds.windows.extend(list(value)),
            "facts" => holds.facts.extend(list(value)),
            "reads" => holds.reads.extend(list(value)),
            _ => {}
        }
    }
    if line.contains("(column block)")
        && let Some((a, b)) = &sel
    {
        let (a, b) = (a["path"].as_array().unwrap(), b["path"].as_array().unwrap());
        assert_eq!(a.len(), 4);
        assert_eq!(b.len(), 4);
        assert_eq!(a[0], b[0]);
        assert_eq!(a[2], b[2]);
        let (first, last) = (a[1].as_u64().unwrap(), b[1].as_u64().unwrap());
        let column = a[2].as_u64().unwrap().to_string();
        let expand = |name: &str| -> Vec<String> {
            if name.split(['/', '.']).any(|part| part == "R" || part == "C") {
                (first.min(last)..=first.max(last)).map(|row| name.replace("/R", &format!("/{row}")).replace("/C", &format!("/{column}"))).collect()
            } else {
                vec![name.to_owned()]
            }
        };
        for (name, fields) in std::mem::take(&mut holds.objects) {
            for name in expand(&name) {
                holds.objects.entry(name).or_default().extend(fields.iter().cloned());
            }
        }
        holds.windows = holds.windows.iter().flat_map(|name| expand(name)).collect();
    }
    tidy(&mut holds);
    (command, params, sel, holds)
}

/// A top-level body paragraph's name (`b7`, not `b4/0/1/0` or a part's).
fn is_top_level(name: &str) -> bool {
    name.strip_prefix('b').is_some_and(|n| n.parse::<usize>().is_ok())
}

fn block_of(name: &str) -> usize {
    name.trim_start_matches('b').split('/').next().and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// The body block a row's selection starts in (`b1:6-10` by default, as the test selects).
fn first_block(cells: &[&str]) -> usize {
    cells
        .iter()
        .find_map(|cell| cell.trim().strip_prefix("sel="))
        .and_then(|sel| sel.trim().strip_prefix('b'))
        .and_then(|rest| rest.split(['/', ':']).next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(1)
}

fn list(value: &str) -> Vec<String> {
    value.split(',').map(str::trim).filter(|s| !s.is_empty() && *s != "none" && *s != "-").map(str::to_owned).collect()
}

/// Only these root fields are KDM lists (model/schema.json, Document.fields).
fn root_list_field(field: &str) -> bool {
    matches!(field, "body" | "parts" | "comments" | "revisions" | "media")
}

/// `*` says nothing beside a field. Also the schema-freeze words (both sides): the readings were
/// written when the root's `styles` was one register (the document defaults and the styles) and a
/// table's grid its `grid` register; now the defaults are `styleDefaults` beside the `styles` map
/// (compared as the styles, as before), and the grid is the table's `cols` list. The root's
/// `comments` list, which held nothing since comments became threads of their own (rule M23,
/// held by their tool ids), is gone.
fn tidy(holds: &mut Holds) {
    if let Some(fields) = holds.objects.get_mut("doc") {
        if fields.remove("styleDefaults") {
            fields.insert("styles".into());
        }
        fields.remove("comments");
    }
    for fields in holds.objects.values_mut() {
        if fields.remove("grid") {
            fields.insert("cols".into());
        }
    }
    for fields in holds.objects.values_mut() {
        if fields.len() > 1 {
            fields.remove("*");
        }
        if fields.is_empty() {
            fields.insert("*".into());
        }
    }
}

/// `b1:6-10`, `b1:6`, `b4/0/1/0:0-2`, `b1:5-b2:3` as an anchor and a focus.
fn selection(value: &str, parts: &BTreeMap<String, u32>) -> Option<(Value, Value)> {
    if let Some((first, last)) = value.split_once(" .. ") {
        let first = first.split_whitespace().next()?;
        let last = last.split_whitespace().next()?;
        let (a, _) = selection(first, parts)?;
        let (_, b) = selection(last, parts)?;
        return Some((a, b));
    }
    let value = value.split_whitespace().next()?.replace("..", "-");
    let value = value.as_str();
    if value == "none" || value == "-" {
        return None;
    }
    // `bN/…:off` or `part:<kind>/N:off`.
    let pos = |text: &str| -> Option<Value> {
        let (path, off) = text.rsplit_once(':')?;
        let off: usize = if off == "end" { usize::MAX } else { off.parse().ok()? };
        let (story, path) = match path.strip_prefix("part:") {
            Some(rest) => {
                let (kind, path) = rest.split_once('/')?;
                (json!({ "part": parts.get(kind)? }), path)
            }
            None => (json!("body"), path.strip_prefix('b')?),
        };
        let path: Option<Vec<usize>> = path.split('/').map(|n| n.parse().ok()).collect();
        Some(json!({ "story": story, "path": path?, "off": off }))
    };
    // `X:a-b`, `X:a`, `X:a-Y:b`.
    let (left, right) = match value.find("-b").or_else(|| value.find("-part:")) {
        Some(at) => (value[..at].to_owned(), value[at + 1..].to_owned()),
        None => {
            let (path, offs) = value.rsplit_once(':')?;
            let (a, b) = offs.split_once('-').unwrap_or((offs, offs));
            (format!("{path}:{a}"), format!("{path}:{b}"))
        }
    };
    Some((pos(&left)?, pos(&right)?))
}

/// What `under` and the command's declarations hold, in the words of [`Holds`].
fn declared(doc: &mut DocDocument, names: &BTreeMap<String, String>, command: &KitCommand, params: &Value) -> Holds {
    let mut holds = Holds::default();
    let name = |id: &Value| -> String {
        let id = id.as_str().map(str::to_owned).unwrap_or_else(|| id.to_string());
        names.get(&id).cloned().unwrap_or(id)
    };
    holds.reads.extend(command.reads.iter().map(|state| state.as_str().to_owned()));
    let under = doc.under(&command.id, params).unwrap_or_else(|e| panic!("{}: under: {e}", command.id));
    // Objects named by id parameters hold every field unless a premise says otherwise.
    let mut named: Vec<Value> = Vec::new();
    for param in command.objects.iter().chain(command.targets.iter()) {
        match params.get(*param) {
            Some(Value::Array(ids)) => named.extend(ids.iter().cloned()),
            Some(id) if !id.is_null() => named.push(id.clone()),
            _ => {}
        }
    }
    let Some(under) = under else {
        for id in named {
            holds.objects.entry(name(&id)).or_default().insert("ALL".into());
        }
        tidy(&mut holds);
        return holds;
    };
    for premise in &under.premises {
        let entry = holds.objects.entry(name(&premise.id)).or_default();
        match &premise.fields {
            Some(fields) if fields.is_empty() => {
                entry.insert("*".into());
            }
            Some(fields) => entry.extend(fields.iter().map(|f| {
                // t4 is the Table's owning Block's tool id; Table fields cross its table ref.
                let table = name(&premise.id).strip_prefix('t').is_some_and(|n| n.parse::<usize>().is_ok());
                field_name(if table { f.strip_prefix("table.").unwrap_or(f) } else { f })
            })),
            None => {
                entry.insert("ALL".into());
            }
        }
        if premise.positions.as_ref().is_none_or(|positions| !positions.is_empty()) {
            entry.insert("@".into());
        }
    }
    for id in under.objects.iter().chain(named.iter()) {
        let listed = under.premises.iter().any(|premise| premise.id == *id);
        let entry = holds.objects.entry(name(id)).or_default();
        if !listed {
            entry.insert("ALL".into());
        }
        if under.covers.as_ref().is_some_and(|covers| covers.fields.iter().any(|f| f == "text")) {
            entry.insert("text".into());
        }
    }
    for window in &under.within {
        holds.windows.insert(format!("{}.{}", name(&window.parent), window.field));
    }
    for content in &under.content {
        holds.objects.entry(name(&content.id)).or_default().insert(field_name(&content.field));
    }
    // A paragraph's `~objects` (a fingerprint of the drawing objects, pictures and equations
    // anchored in it, C8-doc-4) holds each of them whole: the readings name them, not the fact.
    let anchored = anchors(doc, names);
    let fingerprinted: Vec<String> = holds.objects.iter().filter(|(_, f)| f.contains("~objects")).map(|(o, _)| o.clone()).collect();
    for paragraph in fingerprinted {
        if let Some(fields) = holds.objects.get_mut(&paragraph) {
            fields.remove("~objects");
            if fields.is_empty() {
                fields.insert("*".into());
            }
        }
        for object in anchored.get(&paragraph).into_iter().flatten() {
            holds.objects.entry(object.clone()).or_default().insert("ALL".into());
        }
    }
    shape_frame_words(doc, &mut holds, names);
    // A thread's `~comments` (a fingerprint of its comments, C8-doc-6) is what a reread tells of
    // the thread and comments its other premises hold: the readings name those, not the fact.
    let comment_holders = comment_anchor_holders(doc, names);
    let comment_members = comment_thread_members(doc, names);
    let mut comment_content = BTreeSet::new();
    holds.objects.retain(|holder, fields| {
        let held = fields.remove("~comments");
        if held {
            holds.facts.insert(format!("comments:{holder}"));
            for thread in comment_holders.get(holder).into_iter().flatten() {
                for member in comment_members.get(thread).into_iter().flatten() {
                    holds.facts.insert(format!("commentAnchors:{member}"));
                    comment_content.insert(member.clone());
                }
            }
        }
        !(held && fields.is_empty())
    });
    for member in comment_content {
        holds.objects.entry(member).or_default().extend(["body".into(), "blocks".into()]);
    }
    for fact in ["drawingIds", "drawingFrames", "commentCount"] {
        if holds.objects.get_mut("doc").is_some_and(|held| held.remove(&format!("~{fact}"))) {
            holds.facts.insert(fact.into());
        }
    }
    // A paragraph's `~layout` (where it is laid out, C8-doc-6) is the layout the reading names as
    // a fact.
    for (holder, fields) in &mut holds.objects {
        if fields.remove("~tableOrder") {
            holds.windows.insert(parent_list(holder));
        }
    }
    for fact in ["layout", "frameValidity", "inkCount", "inkGeometry"] {
        holds.objects.retain(|holder, fields| {
            let held = fields.remove(&format!("~{fact}"));
            if held {
                holds.facts.insert(format!("{fact}:{holder}"));
            }
            !(held && fields.is_empty())
        });
    }
    if holds.objects.get_mut("doc").is_some_and(|fields| fields.remove("~inkIds")) {
        holds.facts.insert("inkIds".into());
    }
    if holds.objects.get_mut("doc").is_some_and(|fields| fields.remove("~bookmarks")) {
        holds.facts.insert("bookmarks".into());
    }
    if holds.objects.get_mut("doc").is_some_and(|fields| fields.remove("~paraIds")) {
        holds.facts.insert("paraIds".into());
    }
    // An external input the kit names holds the fact of that name (C8-doc-6: `mergeSource`, the
    // session's mail merge list).
    for input in &under.inputs {
        holds.facts.insert(input.clone());
    }
    tidy(&mut holds);
    holds
}

/// C8-doc-7 comparison translation: ~shapeFrames fingerprints the frame of
/// every Shape/Graphic in its paragraph, including uid membership, and no look
/// or value fields. Compare it as those frames, never as complete objects.
fn shape_frame_words(doc: &DocDocument, holds: &mut Holds, names: &BTreeMap<String, String>) {
    let holders: BTreeSet<String> = holds
        .objects
        .iter_mut()
        .filter_map(|(id, fields)| {
            if fields.remove("~shapeFrames") {
                if fields.is_empty() {
                    fields.insert("*".into());
                }
                Some(id.clone())
            } else {
                None
            }
        })
        .collect();
    if holders.is_empty() {
        return;
    }
    let native: kition_doc_doc::Document = serde_json::from_slice(&doc.save(Some("frame.json")).unwrap()).unwrap();
    let stories = std::iter::once(StoryRef::Body).chain(native.parts.keys().map(|part| StoryRef::Part(*part)));
    for story in stories {
        for path in native.para_paths(story) {
            let Some(p) = native.para(story, &path) else { continue };
            let Some(holder) = p.props.para_id.and_then(|id| names.get(&format!("p{id:08X}"))) else { continue };
            if !holders.contains(holder) {
                continue;
            }
            for object in &p.objects {
                if matches!(object, kition_doc_doc::InlineObject::Shape { .. } | kition_doc_doc::InlineObject::Graphic { .. })
                    && let Some(name) = object.tool_uid().and_then(|uid| names.get(&uid.to_string()))
                {
                    holds.objects.entry(name.clone()).or_default().insert("frame".into());
                }
            }
        }
    }
}

/// The inline objects of the fixture by the paragraph they are anchored in (`inline:pic`,
/// `inline:shape` by its number, `inline:eq`), by the fixture's names.
fn anchors(doc: &mut DocDocument, names: &BTreeMap<String, String>) -> BTreeMap<String, Vec<String>> {
    let snapshot = doc.snapshot().unwrap();
    let replica = Replica::load(DocModel::schema(), &snapshot, ActorId::random()).unwrap();
    let (document, _) = kdm_model::read_document(&replica, &MemoryBlobs::default()).unwrap();
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for story in std::iter::once(StoryRef::Body).chain(document.parts.keys().map(|id| StoryRef::Part(*id))) {
        for path in document.para_paths(story) {
            let p = document.para(story, &path).unwrap();
            let Some(paragraph) = p.props.para_id.and_then(|id| names.get(&format!("p{id:08X}"))) else { continue };
            for off in p.object_offsets() {
                if let Some(name) = p.object_at(off).and_then(|o| o.tool_uid()).and_then(|uid| names.get(&uid.to_string())) {
                    out.entry(paragraph.clone()).or_default().push(name.clone());
                }
            }
        }
    }
    out
}

fn inline_scans(command: &str, holds: &mut Holds, names: &BTreeMap<String, String>, anchored: &BTreeMap<String, Vec<String>>) {
    if let Some(mut fields) = holds.objects.remove("allinline") {
        // A sorted ID scan is not the value of each object's uid: swapping two IDs keeps
        // the set unchanged. Preserve the scan as a set fact, never as per-object fields.
        let frames = fields.remove("frame");
        if fields.remove("uid") && (!frames || matches!(command, "drawing.connect" | "drawing.insert")) {
            holds.facts.insert("drawingIds".into());
        }
        if frames {
            holds.facts.insert("drawingFrames".into());
        }
        if !fields.is_empty() {
            for name in names.values().filter(|name| name.starts_with("inline:")) {
                holds.objects.entry(name.clone()).or_default().extend(fields.iter().cloned());
            }
        }
    }
    if let Some(fields) = holds.objects.remove("bodyinline") {
        for object in anchored.iter().filter(|(p, _)| p.starts_with('b')).flat_map(|(_, objects)| objects) {
            holds.objects.entry(object.clone()).or_default().extend(fields.iter().cloned());
        }
    }
    if let Some(fields) = holds.objects.remove("allequations") {
        for object in names.values().filter(|name| *name == "inline:eq") {
            holds.objects.entry(object.clone()).or_default().extend(fields.iter().cloned());
        }
    }
}

fn comment_anchor_holders(doc: &mut DocDocument, names: &BTreeMap<String, String>) -> BTreeMap<String, BTreeSet<String>> {
    let replica = Replica::load(DocModel::schema(), &doc.snapshot().unwrap(), ActorId::random()).unwrap();
    let (native, _) = kdm_model::read_document(&replica, &MemoryBlobs::default()).unwrap();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let session = kition_doc_engine::Session::new(native.clone());
    for story in std::iter::once(StoryRef::Body).chain(native.parts.keys().map(|id| StoryRef::Part(*id))) {
        for path in native.para_paths(story) {
            let p = native.para(story, &path).unwrap();
            let Some(holder) = p.props.para_id.and_then(|id| names.get(&format!("p{id:08X}"))) else { continue };
            for object in &p.objects {
                if let kition_doc_doc::InlineObject::CommentStart { id } = object {
                    let Some(comment) = native.comments.get(id).filter(|_| kition_doc_engine::cmd::review::thread_root(&session, *id) == *id) else {
                        continue;
                    };
                    if let Some(thread) = names.get(&format!("thread:{}", comment.durable_id)) {
                        out.entry(holder.clone()).or_default().insert(thread.clone());
                    }
                }
            }
        }
    }
    out
}

fn comment_thread_members(doc: &mut DocDocument, names: &BTreeMap<String, String>) -> BTreeMap<String, BTreeSet<String>> {
    let replica = Replica::load(DocModel::schema(), &doc.snapshot().unwrap(), ActorId::random()).unwrap();
    let (native, _) = kdm_model::read_document(&replica, &MemoryBlobs::default()).unwrap();
    let session = kition_doc_engine::Session::new(native.clone());
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (id, comment) in &native.comments {
        let root = kition_doc_engine::cmd::review::thread_root(&session, *id);
        if let Some((thread, member)) = native
            .comments
            .get(&root)
            .and_then(|root| names.get(&format!("thread:{}", root.durable_id)))
            .zip(names.get(&format!("comment:{}", comment.durable_id)))
        {
            out.entry(thread.clone()).or_default().insert(member.clone());
        }
    }
    out
}

fn reading_comment_anchors(doc: &mut DocDocument, names: &BTreeMap<String, String>, command: &str, params: &Value, holds: &mut Holds) {
    if !holds.facts.remove("commentAnchors") {
        return;
    }
    let replica = Replica::load(DocModel::schema(), &doc.snapshot().unwrap(), ActorId::random()).unwrap();
    let (native, _) = kdm_model::read_document(&replica, &MemoryBlobs::default()).unwrap();
    let mut session = kition_doc_engine::Session::new(native);
    let selection = doc.execute("document.selection", json!({})).unwrap();
    session.sel.anchor = serde_json::from_value(selection["anchor"].clone()).unwrap();
    session.sel.focus = serde_json::from_value(selection["focus"].clone()).unwrap();
    use kition_doc_engine::cmd::review;
    let ids: Vec<u32> = if command == "comment.delete" {
        let root = review::thread_root(&session, review::comment_param(&session, params, "thread").unwrap());
        let chosen = params.get("comment").map(|_| review::comment_param(&session, params, "comment").unwrap()).unwrap_or(root);
        session
            .doc
            .comments
            .keys()
            .copied()
            .filter(|id| if chosen == root { review::thread_root(&session, *id) == root } else { *id == chosen })
            .collect()
    } else if params.get("all") == Some(&json!(true)) || command == "file.inspect" {
        session.doc.comments.keys().copied().collect()
    } else {
        let chosen = params.get("id").and_then(Value::as_u64).map(|id| id as u32).or_else(|| review::comment_at_caret(&session));
        session.doc.comments.iter().filter(|(id, c)| Some(**id) == chosen || c.parent == chosen && chosen.is_some()).map(|(id, _)| *id).collect()
    };
    for id in ids {
        if let Some(member) = session.doc.comments.get(&id).and_then(|c| names.get(&format!("comment:{}", c.durable_id))) {
            holds.facts.insert(format!("commentAnchors:{member}"));
        }
    }
}

fn part_numbers(doc: &mut DocDocument) -> BTreeMap<String, u32> {
    let mut parts = BTreeMap::new();
    for k in 0..64u32 {
        let Ok(answer) = doc.execute("document.paragraph", json!({ "path": [0], "story": { "part": k } })) else { continue };
        let text = answer["paragraph"]["text"].as_str().unwrap_or_default();
        for (kind, words) in [("header", "Head"), ("footer", "Foot"), ("footnote", "A note."), ("comment", "Check this")] {
            if text.contains(words) {
                parts.entry(kind.to_owned()).or_insert(k);
            }
        }
    }
    parts
}

/// Resolve the reading's layout queries against the engine fixture, independently of under.
/// Geometry facts retain their paragraph holder; no fact on another holder can cover one.
fn reading_geometry(doc: &mut DocDocument, names: &BTreeMap<String, String>, command: &str, params: &Value, source: &str, holds: &mut Holds) {
    let facts: Vec<String> =
        ["layout", "frameValidity", "inkCount", "inkGeometry"].into_iter().filter(|fact| holds.facts.remove(*fact)).map(str::to_owned).collect();
    if facts.is_empty() {
        return;
    }
    let replica = Replica::load(DocModel::schema(), &doc.snapshot().unwrap(), ActorId::random()).unwrap();
    let (native, _) = kdm_model::read_document(&replica, &MemoryBlobs::default()).unwrap();
    let selection = doc.execute("document.selection", json!({})).unwrap();
    let focus: kition_doc_doc::Pos = serde_json::from_value(selection["focus"].clone()).unwrap();
    let mut session = kition_doc_engine::Session::new(native.clone());
    let layout = session.layout();
    let anchored = anchors(doc, names);
    let selected: BTreeSet<String> =
        anchored.iter().filter(|(_, objects)| objects.iter().any(|object| holds.objects.contains_key(object))).map(|(p, _)| p.clone()).collect();
    let paragraph_name = |story, path: &kition_doc_doc::Path| {
        native.para(story, path).and_then(|p| p.props.para_id).and_then(|id| names.get(&format!("p{id:08X}")).cloned())
    };
    let focus_name = paragraph_name(focus.story, &focus.path);
    let explicit_page =
        params.get("page").or_else(|| params.get("placement").and_then(|p| p.get("page"))).and_then(Value::as_u64).map(|p| p as usize);
    let moves = matches!(command, "drawing.connect" | "drawing.group" | "arrange.group")
        || (command == "drawing.transform" && ["dx", "dy", "x", "y"].iter().any(|key| params.get(*key).is_some()))
        || (command == "arrange.bounds" && params.get("x").is_some() && params.get("y").is_some())
        || (command == "arrange.ungroup" && source.contains("note=inline group"));
    let page_query = matches!(command, "table.draw" | "table.eraser" | "draw.stroke" | "draw.erase" | "drawing.ink")
        || (command == "drawing.erase" && !source.contains("view.hide_ink true"))
        || (command == "drawing.insert" && explicit_page.is_some())
        || moves;
    let every = command.starts_with("references.") || matches!(command, "insert.field" | "insert.crossReference");
    let mut pages = BTreeSet::new();
    if let Some(page) = explicit_page {
        pages.insert(page);
    }
    if page_query && pages.is_empty() {
        for (pos, _) in native.numbered().into_iter().filter(|(pos, _)| paragraph_name(pos.story, &pos.path).is_some_and(|p| selected.contains(&p))) {
            if let Some(hit) = layout.object(&pos, 0) {
                pages.insert(hit.page);
            }
        }
        if pages.is_empty() {
            pages.insert(layout.caret_on(&focus, 0).map_or(0, |caret| caret.page));
        }
    }
    let mut holders = if page_query || every {
        layout
            .index
            .iter()
            .filter(|(_, pieces)| every || pieces.iter().any(|(page, _)| pages.contains(page)))
            .filter_map(|((story, path), _)| paragraph_name(*story, path))
            .collect::<BTreeSet<_>>()
    } else if !selected.is_empty() {
        selected.clone()
    } else {
        focus_name.into_iter().collect()
    };
    if page_query {
        holders.extend(selected);
    }
    let mut ink_holders = holders.clone();
    for (pos, object) in native.shapes().into_iter().filter(|(_, o)| o.ink().is_some()) {
        if layout.object(&pos, 0).is_some_and(|hit| pages.contains(&hit.page))
            && let Some(name) = paragraph_name(pos.story, &pos.path)
        {
            ink_holders.insert(name);
        }
        let _ = object;
    }
    for fact in facts {
        for holder in if matches!(fact.as_str(), "layout" | "frameValidity") { &holders } else { &ink_holders } {
            holds.facts.insert(format!("{fact}:{holder}"));
        }
    }
}

/// One naming for what the object model stores of comments (C8-doc-6, `kdm_model::comments`):
/// a comment's parent is its `props` (`replyTo`) and its thread's list, a reply's resolved flag
/// its `props`, its text (`part`, the comment part `part:commentN`) its `body`; a thread's first
/// comment's resolved flag is the thread's `resolved` (`registers` writes it on the thread only).
/// The readings name them by the engine's fields; both sides are compared in these words.
fn comment_words(holds: &mut Holds, roots: &BTreeMap<String, String>) {
    let objects = std::mem::take(&mut holds.objects);
    for (object, fields) in objects {
        // Comment paragraphs are serialized under props.blocks (including formatting) and
        // their plain text in body, not independent Blocks (kdm-model comments.rs:460-463,600).
        if let Some((n, _)) = object.strip_prefix("part:comment").and_then(|rest| rest.split_once('/')) {
            if n.parse::<usize>().is_ok() {
                let target = holds.objects.entry(format!("comment:{n}")).or_default();
                if fields.contains("text") || fields.contains("ALL") {
                    target.extend(["body".into(), "blocks".into()]);
                }
                if fields.contains("para") {
                    target.insert("blocks".into());
                }
                if target.is_empty() {
                    target.insert("*".into());
                }
            }
            continue;
        }
        let comment = object.strip_prefix("part:comment").filter(|n| n.parse::<usize>().is_ok()).map(|n| format!("comment:{n}"));
        if let Some(comment) = comment {
            let target = holds.objects.entry(comment).or_default();
            if fields.contains("blocks") || fields.contains("text") || fields.contains("ALL") {
                target.extend(["body".into(), "blocks".into()]);
            } else {
                target.insert("*".into());
            }
            continue;
        }
        if !object.starts_with("comment:") || object.contains('/') {
            holds.objects.entry(object).or_default().extend(fields);
            continue;
        }
        let root = roots.contains_key(&object);
        let moved = root && fields.contains("resolved");
        let mut own = BTreeSet::new();
        for field in fields {
            match field.as_str() {
                "part" => {
                    own.insert("body".to_owned());
                    own.insert("blocks".to_owned());
                }
                "resolved" if root => {
                    holds.objects.entry(roots[&object].clone()).or_default().insert("resolved".into());
                }
                "resolved" => {
                    own.insert("resolved".to_owned());
                }
                _ => {
                    own.insert(field);
                }
            }
        }
        // A comment whose fields all moved to its thread is held by the thread.
        if own.is_empty() && moved {
            continue;
        }
        if own.is_empty() {
            own.insert("*".into());
        }
        holds.objects.entry(object).or_default().extend(own);
    }
    for fields in holds.objects.values_mut() {
        if fields.len() > 1 {
            fields.remove("*");
        }
    }
}

fn field_held(object: &str, fields: &BTreeSet<String>, field: &str) -> bool {
    fields.contains(field) || (object.starts_with("comment:") && fields.contains("props") && matches!(field, "parent" | "resolved" | "blocks"))
}

/// What the reading holds that the declaration does not (`less`) and the reverse (`more`).
fn differences(read: &Holds, declared: &Holds, ownership: &Ownership) -> (Vec<String>, Vec<String>) {
    let (mut less, mut more) = (Vec::new(), Vec::new());
    let supplied = ownership.supplied_content(declared);
    let read_supplied = ownership.supplied_content(read);
    // A `Media` has no tool id: it is part of the root through the root's `media` list, so a
    // change of its bitmap is a change of the root's `media` (SDK `tool_ids`: an object without a
    // tool id is attributed to its holder; C8-doc-6, tests/c8 `a_picture_command_rests_on_the_
    // picture_by_its_number`: another program's new bitmap under the same name is refused as
    // `document` changed). A premise on the root's `media` holds every media object whole.
    let doc_media = declared.objects.get("doc").is_some_and(|f| f.contains("media") || f.contains("ALL"));
    for (object, fields) in &read.objects {
        if supplied.contains(object) {
            continue;
        }
        if doc_media && object.starts_with("media:") {
            continue;
        }
        match declared.objects.get(object) {
            None => less.push(format!("{object}{{{}}}", fields.iter().cloned().collect::<Vec<_>>().join(","))),
            Some(held) if held.contains("ALL") => {}
            Some(held) => {
                for field in
                    fields.iter().filter(|f| *f != "*" && !field_held(object, held, f) && !(*f == "@" && position_held(object, declared, &supplied)))
                {
                    less.push(format!("{object}.{field}"));
                }
            }
        }
    }
    for (object, fields) in &declared.objects {
        if read_supplied.contains(object) {
            continue;
        }
        match read.objects.get(object) {
            None => more.push(format!("{object}{{{}}}", fields.iter().cloned().collect::<Vec<_>>().join(","))),
            Some(want) => {
                if fields.contains("ALL") {
                    if !want.contains("ALL") {
                        more.push(format!("{object}.ALL"));
                    }
                    continue;
                }
                for field in
                    fields.iter().filter(|f| *f != "*" && !field_held(object, want, f) && !(*f == "@" && position_held(object, read, &read_supplied)))
                {
                    more.push(format!("{object}.{field}"));
                }
            }
        }
    }
    // A premise on a list field of the root holds its membership and order: the window on the
    // whole list is the same fact (C8-doc-5, `doc.revisions`).
    let root_list = |window: &str| {
        window.strip_prefix("doc.").is_some_and(|field| declared.objects.get("doc").is_some_and(|f| f.contains(field) || f.contains("ALL")))
    };
    let owns_window = |window: &str, holds: &Holds, supplied: &BTreeSet<String>| {
        window.rsplit_once('.').is_some_and(|(owner, field)| {
            supplied.contains(owner) || holds.objects.get(owner).is_some_and(|f| f.contains("ALL") || f.contains(field))
        })
    };
    for window in read.windows.difference(&declared.windows).filter(|w| !root_list(w) && !owns_window(w, declared, &supplied)) {
        less.push(format!("window {window}"));
    }
    for window in declared.windows.difference(&read.windows).filter(|w| !owns_window(w, read, &read_supplied)) {
        more.push(format!("window {window}"));
    }
    for state in read.reads.difference(&declared.reads) {
        less.push(format!("reads {state}"));
    }
    for state in declared.reads.difference(&read.reads) {
        more.push(format!("reads {state}"));
    }
    // The time a write reads (a revision's date, a field's result) is no other writer's to change.
    for fact in read.facts.iter().filter(|f| {
        !matches!(f.as_str(), "time" | "fonts")
            && !declared.facts.contains(*f)
            && !f.split_once(':').is_some_and(|(kind, holder)| {
                matches!(kind, "layout" | "frameValidity" | "inkCount" | "inkGeometry" | "comments") && supplied.contains(holder)
            })
    }) {
        less.push(format!("fact {fact}"));
    }
    for fact in declared.facts.difference(&read.facts) {
        more.push(format!("fact {fact}"));
    }
    (less, more)
}

fn position_held(object: &str, holds: &Holds, supplied: &BTreeSet<String>) -> bool {
    let list = parent_list(object);
    holds.windows.contains(&list)
        || list.rsplit_once('.').is_some_and(|(owner, field)| {
            supplied.contains(owner) || holds.objects.get(owner).is_some_and(|f| f.contains(field) || f.contains("ALL"))
        })
}

/// Editor state the kit saves and restores around a call (`selection`), the editing mode (`mode`),
/// and session state that is neither (the author, preferences, macro recording, the clipboard,
/// undo history): only the first two are another writer's to change between a basis and a write.
fn state_kind(name: &str) -> Option<&'static str> {
    let name = name.split(['(', ':', '.']).next().unwrap_or_default().trim();
    match name {
        "selection" | "also" | "pending" | "bodyReturn" | "goalX" | "page" | "view" => Some("selection"),
        "mode" | "math" | "mathLatex" | "mathNormalText" => Some("mode"),
        _ => None,
    }
}

fn prepare_variant(bytes: &[u8], variant: &str) -> DocDocument {
    let mut prepared = DocDocument::open("Report.docx", bytes).unwrap();
    if variant == "number" {
        run(&mut prepared, "para.text", json!({ "index": 5, "text": "1." }));
        select(&mut prepared, at(&[5], 0), at(&[5], 0));
        run(&mut prepared, "para.numbering", json!({ "off": true }));
    } else if variant == "empty" {
        run(&mut prepared, "para.text", json!({ "index": 5, "text": "" }));
    } else if variant == "border" {
        run(&mut prepared, "para.text", json!({ "index": 6, "text": "--" }));
        select(&mut prepared, at(&[6], 2), at(&[6], 2));
        run(&mut prepared, "text.insert", json!({ "text": "-" }));
    } else {
        select(&mut prepared, at(&[1], 0), if variant == "clipboardLine" { at(&[1], 5) } else { at(&[2], 3) });
        run(&mut prepared, "edit.copy", json!({}));
    }
    prepared
}

fn drawing_fixture(bytes: &[u8], line: &str) -> DocDocument {
    use kition_doc_doc::para::{Float, FloatAlign, GroupChild, InlineObject, ShapeKind, Wrap};
    use kition_doc_doc::{CharProps, Paragraph, Pos, StoryRef, para_block};
    let mut prepared = DocDocument::open("Report.docx", bytes).unwrap();
    select(&mut prepared, at(&[9], 0), at(&[9], 0));
    let chart = run(&mut prepared, "drawing.insert", json!({ "kind": "chart", "chart": { "kind": "pie" } }));
    let chart_id: u32 = chart["id"].as_str().unwrap().parse().unwrap();
    let mut doc = kition_doc_engine::io::open_bytes("Report.docx", &prepared.save(None).unwrap()).unwrap();
    let pos = doc.find_shape(chart_id).unwrap();
    doc.para_mut(pos.story, &pos.path).unwrap().object_at_mut(pos.off).unwrap().set_tool_uid(9);
    let pos = doc.find_shape(1).unwrap();
    let template = doc.para_at(&pos).unwrap().object_at(pos.off).unwrap().clone();
    let shape = |uid| {
        let mut object = template.clone();
        object.set_tool_uid(uid);
        if let Some(float) = object.float_mut() {
            *float = Float { wrap: Wrap::Square, ..Default::default() };
        }
        object
    };
    let mut connector = shape(7);
    if let InlineObject::Shape { kind, connection, .. } = &mut connector {
        *kind = ShapeKind::Line;
        *connection = Some(kition_doc_doc::graphic::Connection { start: Some((5, 0)), end: Some((6, 2)) });
    }
    let group = InlineObject::Group {
        uid: 10,
        w: 80.0,
        h: 30.0,
        ch_w: 80.0,
        ch_h: 30.0,
        float: Float {
            wrap: if line.contains("note=inline group") { Wrap::Inline } else { Wrap::Square },
            h_align: Some(FloatAlign::Center),
            ..Default::default()
        },
        children: vec![GroupChild { x: 0.0, y: 0.0, obj: shape(11) }, GroupChild { x: 40.0, y: 0.0, obj: shape(12) }],
    };
    for object in [shape(5), shape(6), connector, group] {
        let index = doc.body.len();
        doc.body.push(para_block(Paragraph::with_text("Drawing", Default::default())));
        doc.insert_object(&Pos::body(index, 0), object, &CharProps::default()).unwrap();
    }
    if line.starts_with("ROW arrange.group") && line.contains("note=both floating") {
        for uid in [1, 2] {
            let pos = doc.numbered().into_iter().find(|(_, o)| o.tool_uid() == Some(uid)).unwrap().0;
            doc.para_mut(pos.story, &pos.path).unwrap().object_at_mut(pos.off).unwrap().float_mut().unwrap().wrap = Wrap::Square;
        }
    }
    if line.starts_with("ROW arrange.nudge") && line.contains("note=floating") {
        let pos = doc.find_shape(1).unwrap();
        let float = doc.para_mut(pos.story, &pos.path).unwrap().object_at_mut(pos.off).unwrap().float_mut().unwrap();
        float.wrap = Wrap::Square;
        if line.contains("h_align/v_align") {
            float.h_align = Some(FloatAlign::Center);
        }
    }
    if line.contains("inline:X") {
        let part = doc.parts.iter().find(|(_, p)| p.kind == PartKind::Header).unwrap().0.to_owned();
        doc.insert_object(&Pos::new(StoryRef::Part(part), kition_doc_doc::Path(vec![0]), 4), shape(13), &CharProps::default()).unwrap();
    }
    DocDocument::open("Report.docx", &kition_doc_engine::io::save_bytes("Report.docx", &doc).unwrap()).unwrap()
}

fn drawing_row(doc: &mut DocDocument, line: &str) -> String {
    let native = kition_doc_engine::io::open_bytes("Report.docx", &doc.save(None).unwrap()).unwrap();
    let mut line = line.to_owned();
    for (alias, uid) in [("bA", 5), ("bB", 6), ("bG", 10)] {
        if let Some(pos) = native.find_shape(uid) {
            let path = format!("b{}", pos.path.0.iter().map(u32::to_string).collect::<Vec<_>>().join("/"));
            line = line.replace(&format!("{alias}:o-o+3"), &format!("{path}:{}-{}", pos.off, pos.off + 3));
            line = line.replace(alias, &path);
        }
    }
    if line.contains("bK")
        && ["ROW drawing.insert", "ROW drawing.transform", "ROW drawing.connect", "ROW drawing.ink", "ROW draw.stroke"]
            .iter()
            .any(|command| line.starts_with(command))
    {
        let (command, params, selection, _) = parse_row(&line, &[], &BTreeMap::new());
        let mut projected = kition_doc_engine::Session::new(native.clone());
        if let Some((anchor, focus)) = selection {
            projected.run("select.range", &json!({ "anchor": anchor, "focus": focus })).unwrap();
        }
        let result = projected.run(&command, &params).unwrap();
        let pos = if command == "draw.stroke" {
            serde_json::from_value(result["pos"].clone()).unwrap()
        } else {
            let uid = if matches!(command.as_str(), "drawing.insert" | "drawing.connect") {
                &result["id"]
            } else if command == "drawing.ink" {
                &result["ids"][0]
            } else {
                &params["ids"][0]
            };
            let uid = uid.as_u64().or_else(|| uid.as_str().and_then(|id| id.parse().ok())).unwrap() as u32;
            projected.doc.find_shape(uid).unwrap()
        };
        let id = projected.doc.para_at(&pos).unwrap().props.para_id;
        let path = native
            .para_paths(kition_doc_doc::StoryRef::Body)
            .into_iter()
            .find(|path| native.para(kition_doc_doc::StoryRef::Body, path).unwrap().props.para_id == id)
            .unwrap();
        line = line.replace("bK", &format!("b{}", path.0.iter().map(u32::to_string).collect::<Vec<_>>().join("/")));
    }
    if line.contains("bK") && line.starts_with("ROW arrange.") {
        let layout = kition_doc_engine::Session::new(native.clone()).layout();
        let (_, params, _, _) = parse_row(&line, &[], &BTreeMap::new());
        let destination = if line.starts_with("ROW arrange.bounds") {
            Some((params["page"].as_u64().unwrap_or(0) as usize, params["y"].as_f64().unwrap() as f32))
        } else if line.starts_with("ROW arrange.group") {
            let hits: Vec<_> = [1, 2]
                .into_iter()
                .filter_map(|uid| native.numbered().into_iter().find(|(_, o)| o.tool_uid() == Some(uid)))
                .filter_map(|(pos, _)| layout.object(&pos, 0))
                .collect();
            Some((hits.first().unwrap().page, hits.iter().map(|hit| hit.rect.y).fold(f32::MAX, f32::min)))
        } else {
            let pos = native.find_shape(10).unwrap();
            let hit = layout.object(&pos, 0).unwrap();
            Some((hit.page, hit.rect.y))
        };
        if let Some((page, y)) = destination {
            let path = kition_doc_engine::cmd::objects::anchor_paragraph(&layout, page, y).unwrap();
            line = line.replace("bK", &format!("b{}", path.0.iter().map(u32::to_string).collect::<Vec<_>>().join("/")));
        }
    }
    line
}

fn table_point_row(doc: &mut DocDocument, line: &str, parts: &BTreeMap<String, u32>) -> String {
    if !line.starts_with("ROW table.draw") && !line.starts_with("ROW table.eraser") {
        return line.to_owned();
    }
    let native = kition_doc_engine::io::open_bytes("Report.docx", &doc.save(None).unwrap()).unwrap();
    let layout = kition_doc_engine::Session::new(native.clone()).layout();
    let (_, mut params, _, _) = parse_row(line, &[], parts);
    let cell = |r, c| {
        layout.pages[0]
            .items
            .iter()
            .find_map(|item| match item {
                kition_doc_engine::layout::Placed::Cell { rect, table, row, cell, .. } if table.0 == [4] && *row == r && *cell == c => Some(*rect),
                _ => None,
            })
            .unwrap()
    };
    let mut line = line.to_owned();
    if line.contains("note=border between A1 and B1") {
        let r = cell(0, 0);
        params["x"] = json!(r.right());
        params["y"] = json!(r.y + r.h / 2.0);
    } else if line.contains("note=border between A1 and A2") {
        let r = cell(0, 0);
        params["x"] = json!(r.x + r.w / 2.0);
        params["y"] = json!(r.bottom());
    } else if line.contains("note=vertical line inside A1") {
        let (r0, r1) = (cell(0, 0), cell(1, 0));
        params["x0"] = json!(r0.x + r0.w / 2.0);
        params["x1"] = params["x0"].clone();
        params["y0"] = json!(r0.y + 1.0);
        params["y1"] = json!(r1.bottom() - 1.0);
    } else if line.contains("note=horizontal line across row 0") {
        let (r0, r1) = (cell(0, 0), cell(0, 1));
        params["x0"] = json!(r0.x + 1.0);
        params["x1"] = json!(r1.right() - 1.0);
        params["y0"] = json!(r0.y + r0.h / 2.0);
        params["y1"] = params["y0"].clone();
    } else if params.get("story").is_some() {
        params["story"] = json!({ "part": parts["header"] });
    } else {
        let (x, y) = layout.pages[0]
            .items
            .iter()
            .find_map(|item| match item {
                kition_doc_engine::layout::Placed::Lines { story: kition_doc_doc::StoryRef::Body, path, x, y, .. } if path.0 == [6] => Some((*x, *y)),
                _ => None,
            })
            .unwrap();
        params["x0"] = json!(x + 1.0);
        params["y0"] = json!(y + 1.0);
        params["x1"] = json!(x + 201.0);
        params["y1"] = json!(y + 61.0);
        let hit =
            layout.hit(0, params["x0"].as_f64().unwrap() as f32, params["y0"].as_f64().unwrap() as f32, kition_doc_doc::StoryRef::Body).unwrap();
        line = line.replace("bHit", &format!("b{}", hit.path.0.iter().map(u32::to_string).collect::<Vec<_>>().join("/")));
    }
    // Replace only the parsed parameter field, preserving the expected holds verbatim.
    let original = line.split(" | params=").nth(1).unwrap().split(" | holds=").next().unwrap().to_owned();
    line.replacen(&format!(" | params={original} | holds="), &format!(" | params={params} | holds="), 1)
}

#[test]
fn table_point_readings_reach_the_borders_they_name() {
    let mut doc = fixture();
    let native = kition_doc_engine::io::open_bytes("Report.docx", &doc.save(None).unwrap()).unwrap();
    let layout = kition_doc_engine::Session::new(native).layout();
    for source in READINGS.lines().filter(|line| line.starts_with("ROW table.eraser")) {
        let line = table_point_row(&mut doc, source, &BTreeMap::new());
        let (_, params, _, _) = parse_row(&line, &[], &BTreeMap::new());
        let target =
            kition_doc_engine::cmd::table_draw::erase_target(&layout, 0, params["x"].as_f64().unwrap() as f32, params["y"].as_f64().unwrap() as f32);
        let target = target.unwrap_or_else(|| panic!("the point must reach the named border: {line}"));
        assert_eq!(target.table().1.0, [4]);
        if source.contains("A1 and B1") {
            assert_eq!(target.from.0, target.to.0, "the A1/B1 border is vertical");
        } else {
            assert_eq!(target.from.1, target.to.1, "the A1/A2 border is horizontal");
        }
    }
}

#[test]
fn empty_paragraph_readings_reach_a_join_and_the_empty_mark() {
    let base = fixture();
    let bytes = base.save(None).unwrap();
    let mut prepared = prepare_variant(&bytes, "empty");
    let paragraph = run(&mut prepared, "document.paragraph", json!({ "path": [5] }));
    assert_eq!(paragraph["paragraph"]["text"], "", "the empty-paragraph row must not contain the fixture's equation");
    select(&mut prepared, at(&[5], 0), at(&[5], 0));
    let commands: BTreeMap<String, KitCommand> = prepared.commands().into_iter().map(|c| (c.id.clone(), c)).collect();
    let (names, _) = names(&mut prepared);
    for command in ["text.delete", "text.deleteWordForward"] {
        let holds = declared(&mut prepared, &names, &commands[command], &json!({}));
        assert!(holds.objects.get("b6").is_some_and(|fields| fields.contains("text")), "{command}: {holds:?}");
    }
    let holds = declared(&mut prepared, &names, &commands["insert.removeLink"], &json!({}));
    assert!(holds.objects.get("b5").is_some_and(|fields| fields.contains("para")), "{holds:?}");
}

#[test]
fn drawing_readings_resolve_real_ends_and_real_objects() {
    let parts = BTreeMap::new();
    assert!(selection("b0:0-b9:end", &parts).is_some(), "end must reach the last paragraph, not the default selection");
    let mut doc = drawing_fixture(&fixture().save(None).unwrap(), "ROW drawing.group");
    let (names, _) = names(&mut doc);
    for uid in ["5", "6", "7", "9", "10"] {
        assert!(names.contains_key(uid), "drawing variant must contain {uid}");
    }
}

#[test]
fn paragraph_insertion_names_the_table_kind_it_checks() {
    let mut doc = fixture();
    select(&mut doc, at(&[1], 0), at(&[1], 0));
    let (names, _) = names(&mut doc);
    let command = doc.commands().into_iter().find(|c| c.id == "para.insert").unwrap();
    let holds = declared(&mut doc, &names, &command, &json!({ "after": 4, "text": "x" }));
    assert!(holds.objects.get("t4").is_some_and(|fields| fields.contains("kind")), "{holds:?}");
    assert!(holds.windows.contains("doc.body"), "{holds:?}");
}

#[test]
fn every_write_command_is_declared_as_its_code_reads() {
    let mut doc = fixture();
    let (names, paragraphs) = names(&mut doc);
    // The fixture's parts by the numbers the session gives them (a row's `part:<kind>/…`
    // selection is a position in the session): by their texts (C8-doc-6: the numbers a replica's
    // read gives differ, which a command acting on the selection's story alone tells).
    let parts: BTreeMap<String, u32> = {
        let mut parts = BTreeMap::new();
        for k in 0..64u32 {
            let Ok(answer) = doc.execute("document.paragraph", json!({ "path": [0], "story": { "part": k } })) else { continue };
            let text = answer["paragraph"]["text"].as_str().unwrap_or_default().to_owned();
            for (kind, words) in [("header", "Head"), ("footer", "Foot"), ("footnote", "A note."), ("comment", "Check this")] {
                if text.contains(words) {
                    parts.entry(kind.to_owned()).or_insert(k);
                }
            }
        }
        parts
    };
    let commands: BTreeMap<String, KitCommand> = doc.commands().into_iter().map(|c| (c.id.clone(), c)).collect();
    let writes: BTreeSet<&String> = commands.values().filter(|c| c.mutates).map(|c| &c.id).collect();
    let anchored = anchors(&mut doc, &names);
    let roots: BTreeMap<String, String> = {
        let replica = Replica::load(DocModel::schema(), &doc.snapshot().unwrap(), ActorId::random()).unwrap();
        let (native, _) = kdm_model::read_document(&replica, &MemoryBlobs::default()).unwrap();
        let session = kition_doc_engine::Session::new(native.clone());
        native
            .comments
            .iter()
            .filter(|(id, _)| kition_doc_engine::cmd::review::thread_root(&session, **id) == **id)
            .filter_map(|(_, c)| {
                Some((names.get(&format!("comment:{}", c.durable_id))?.clone(), names.get(&format!("thread:{}", c.durable_id))?.clone()))
            })
            .collect()
    };
    let mut rows = Vec::new();
    for line in READINGS.lines().filter(|l| l.starts_with("ROW ")) {
        let (command, params, sel, mut holds) = parse_row(line, &paragraphs, &parts);
        table_block_names(&mut holds, &names);
        holds.objects.entry("doc".into()).or_default().insert("settings".into());
        inline_scans(&command, &mut holds, &names, &anchored);
        holds.reads = holds.reads.iter().filter_map(|r| state_kind(r)).map(str::to_owned).collect();
        tidy(&mut holds);
        // A row of the tracked variant (track changes on): `+trackChanges`, a note on what runs
        // when changes are tracked, or a row holding the root's revisions with the time as a fact
        // (the date of the revision the write makes, read only when changes are tracked: C8-doc-4).
        let tracked = line.contains("+trackChanges")
            || (holds.facts.contains("time") && holds.objects.get("doc").is_some_and(|f| f.contains("revisions")))
            || line
                .split("note=")
                .nth(1)
                .is_some_and(|note| note.contains("post-command") || note.contains("when settings.trackChanges") || note.starts_with("tracked"));
        let variant = if line.contains("note=list AutoFormat when") {
            Some("number")
        } else if line.contains("note=border AutoFormat when") {
            Some("border")
        } else if matches!(command.as_str(), "text.delete" | "text.deleteWordForward" | "insert.removeLink") && line.contains("sel=b5:0") {
            Some("empty")
        } else if matches!(command.as_str(), "edit.paste" | "edit.pasteAllClipboard" | "edit.pasteClipboardItem")
            || (command == "edit.pasteSpecial" && params.get("text").is_none() && params.get("html").is_none())
            || (matches!(command.as_str(), "edit.pasteMerge" | "edit.pasteText") && params.get("text").is_none())
        {
            Some("clipboard")
        } else {
            None
        };
        rows.push((command, params, sel, holds, tracked, line.contains("(column block)"), variant, line));
    }
    let read: BTreeSet<&String> = rows.iter().map(|row| &row.0).collect();
    let unread: Vec<&&String> = writes.difference(&read).collect();
    let unknown: Vec<&&String> = read.difference(&writes).collect();
    println!("write commands: {}; read: {}", writes.len(), writes.intersection(&read).count());
    assert!(unread.is_empty() && unknown.is_empty(), "unread {unread:?}, not commands {unknown:?}");
    let (mut less_commands, mut more_commands) = (BTreeSet::new(), BTreeSet::new());
    let mut lines = Vec::new();
    let mut facts = Vec::new();
    let variant_bytes = doc.save(None).unwrap();
    for (command, _, _, _, tracked, column, variant, source) in &rows {
        let drawings = source.contains("inline:a{")
            || source.contains("inline:b{")
            || source.contains("inline:conn{")
            || source.contains("inline:group{")
            || source.contains("inline:chart{")
            || source.contains("inline:X{")
            || (command == "arrange.group" && !source.contains("also empty"))
            || (command == "arrange.nudge" && source.contains("note=floating"));
        let mut prepared =
            if drawings { Some(drawing_fixture(&variant_bytes, source)) } else { variant.map(|variant| prepare_variant(&variant_bytes, variant)) };
        let doc = prepared.as_mut().unwrap_or(&mut doc);
        let row_parts = part_numbers(doc);
        let (row_names, row_paragraphs) = self::names(doc);
        let row_ownership = Ownership::fixture(doc, &row_names);
        let expanded = drawing_row(doc, source);
        let expanded = table_point_row(doc, &expanded, &row_parts);
        let (_, row_params, row_sel, mut row_holds) = parse_row(&expanded, &row_paragraphs, &row_parts);
        let params = &row_params;
        table_block_names(&mut row_holds, &row_names);
        row_holds.objects.entry("doc".into()).or_default().insert("settings".into());
        inline_scans(command, &mut row_holds, &row_names, &anchors(doc, &row_names));
        row_holds.reads = row_holds.reads.iter().filter_map(|r| state_kind(r)).map(str::to_owned).collect();
        tidy(&mut row_holds);
        let holds = &row_holds;
        let sel = &row_sel;
        let (anchor, focus) =
            sel.clone().unwrap_or_else(|| (json!({ "story": "body", "path": [1], "off": 6 }), json!({ "story": "body", "path": [1], "off": 10 })));
        let select_command = if *column { "select.column" } else { "select.range" };
        run(doc, select_command, json!({ "anchor": anchor, "focus": focus }));
        if command == "arrange.group" && !source.contains("also empty") {
            run(doc, "select.addObject", json!({ "pos": { "block": 8, "off": 10 } }));
        }
        let hidden_ink = source.contains("view.hide_ink true");
        if hidden_ink {
            run(doc, "review.hideInk", json!({ "value": true }));
        }
        if *tracked {
            run(doc, "review.trackChanges", json!({ "value": true }));
            run(doc, "select.range", json!({ "anchor": anchor, "focus": focus }));
        }
        // These readings explicitly describe an active equation editing session.
        let editing = command.starts_with("equation.") && holds.objects.contains_key("inline:eq") && params.get("all") != Some(&json!(true));
        if editing {
            select(doc, at(&[5], 0), at(&[5], 0));
            run(doc, "equation.edit", json!({}));
        }
        let mut scoped = holds.clone();
        reading_geometry(doc, &row_names, command, params, source, &mut scoped);
        reading_comment_anchors(doc, &row_names, command, params, &mut scoped);
        let holds = &scoped;
        let held = declared(doc, &row_names, &commands[command], params);
        if hidden_ink {
            run(doc, "review.hideInk", json!({ "value": false }));
        }
        if editing {
            run(doc, "equation.exit", json!({}));
        }
        if *tracked {
            run(doc, "review.trackChanges", json!({ "value": false }));
        }
        let absent: Vec<&String> =
            holds.objects.keys().filter(|o| !row_names.values().any(|n| n == *o) && !row_paragraphs.contains(o) && !is_fixture_object(o)).collect();
        // A paste of the session's clipboard is refused for an agent unless its own call copied
        // (`acts::reads_clipboard`, C8-doc-5): the clipboard is then the run's own, no other
        // writer's to change.
        let mut holds = holds.clone();
        if kition_doc_kit::acts::reads_clipboard(command, params) {
            holds.facts.remove("clipboard");
            // Clipboard history is session-local too; hosted agents use only their own
            // call's copies (lib.rs clipboard_gate/copied_by), not another writer's list.
            holds.windows.remove("other:clipHistory");
        }
        // A command that names a file by path is refused for every agent (`files::refuse_for_agent`,
        // KHP A29): the file it reads is no other writer's to change under an agent's write
        // (C8-doc-6).
        if kition_doc_kit::files::refuse_for_agent(command).is_err() {
            holds.facts.remove("file");
        }
        let mut held = held;
        comment_words(&mut holds, &roots);
        comment_words(&mut held, &roots);
        let (less, more) = differences(&holds, &held, &row_ownership);
        facts.push(json!({ "command": command, "params": params, "source": source, "reading": holds, "declared": held, "less": less, "more": more }));
        if !less.is_empty() {
            less_commands.insert(command.clone());
        }
        if !more.is_empty() {
            more_commands.insert(command.clone());
        }
        if !less.is_empty() || !more.is_empty() {
            lines.push(format!(
                "DIFFERS {command} {params}{}\n  less {less:?}\n  more {more:?}",
                if absent.is_empty() { String::new() } else { format!(" (not on the fixture: {absent:?})") }
            ));
        }
    }
    for line in &lines {
        println!("{line}");
    }
    if let Some(path) = std::env::var_os("KITION_DOC_FACTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&facts).unwrap()).unwrap();
    }
    let differing: BTreeSet<&String> = less_commands.union(&more_commands).collect();
    println!(
        "declarations that differ from the readings: {} of {} rows; {} of {} commands ({} hold less, {} hold more, {} both)",
        lines.len(),
        rows.len(),
        differing.len(),
        writes.len(),
        less_commands.len(),
        more_commands.len(),
        less_commands.intersection(&more_commands).count()
    );
    // After independently reviewing the readings and comparison, further repairs must lower
    // this floor. Correcting a comparison rule may expose previously hidden differences.
    let (rows_left, less_left, more_left) = LEFT;
    assert!(
        lines.len() <= rows_left && less_commands.len() <= less_left && more_commands.len() <= more_left,
        "the declarations moved away from the readings"
    );
}

/// Rows differing, commands holding less, commands holding more, at the last measurement.
const LEFT: (usize, usize, usize) = (428, 0, 234); // C8-doc-pin: selected objects are scoped; watermark consumes no caret text

#[test]
fn comparison_membership_is_not_cell_content() {
    let mut doc = fixture();
    let (names, _) = names(&mut doc);
    let ownership = Ownership::fixture(&mut doc, &names);
    let mut read = Holds::default();
    read.objects.insert("c4/0/0".into(), ["blocks".into()].into());
    let mut held = Holds::default();
    held.windows.insert("r4/0.cells".into());
    tidy(&mut read);
    tidy(&mut held);
    let (less, _) = differences(&read, &held, &ownership);
    assert!(!less.is_empty(), "editing a cell paragraph preserves membership but changes content");
}

#[test]
fn comparison_a_whole_object_fingerprint_is_more_than_its_fill() {
    let mut doc = fixture();
    let (names, _) = names(&mut doc);
    let ownership = Ownership::fixture(&mut doc, &names);
    let command = doc.commands().into_iter().find(|c| c.id == "drawing.format").unwrap();
    let mut held = declared(&mut doc, &names, &command, &json!({ "ids": [1], "fill": { "color": "#FF0000" } }));
    held.objects.retain(|name, _| name == "inline:shape");
    held.windows.clear();
    held.facts.clear();
    held.reads.clear();
    let mut read = Holds::default();
    read.objects.insert("inline:shape".into(), ["look".into()].into());
    let (_, more) = differences(&read, &held, &ownership);
    assert!(!more.is_empty(), "a frame edit changes the fingerprint even when fill is all that is requested");
}

#[test]
fn a_shape_frame_fingerprint_does_not_supply_appearance() {
    let mut doc = fixture();
    let (names, _) = names(&mut doc);
    let mut held = Holds::default();
    held.objects.insert("b8".into(), BTreeSet::from(["~shapeFrames".into()]));
    shape_frame_words(&doc, &mut held, &names);
    assert_eq!(held.objects.get("inline:shape"), Some(&BTreeSet::from(["frame".into()])));
    assert!(
        !held
            .objects
            .values()
            .any(|fields| fields.contains("~shapeFrames") || fields.contains("look") || fields.contains("value") || fields.contains("ALL"))
    );
}

#[test]
fn column_readings_name_both_real_segments_and_their_containers() {
    let line = READINGS.lines().find(|line| line.starts_with("ROW format.growFont |") && line.contains("column block")).unwrap();
    let (_, _, sel, holds) = parse_row(line, &[], &BTreeMap::new());
    let (a, b) = sel.unwrap();
    assert_eq!(a, at(&[4, 0, 0, 0], 0));
    assert_eq!(b, at(&[4, 1, 0, 0], 2));
    assert!(holds.objects.contains_key("b4/0/0/0"));
    assert!(holds.objects.contains_key("b4/1/0/0"));
    assert!(holds.windows.contains("r4/0.cells"));
    assert!(holds.windows.contains("r4/1.cells"));
    assert!(!holds.objects.get("r4/0").is_some_and(|f| f.contains("cells")));
    assert!(!holds.objects.get("r4/1").is_some_and(|f| f.contains("cells")));
    assert!(holds.objects["b4/0/0/0"].contains("@"));
    assert!(holds.objects["b4/1/0/0"].contains("@"));
    assert!(!holds.windows.contains("c4/0/0.blocks"));
    assert!(!holds.windows.contains("c4/1/0.blocks"));
    assert!(!holds.objects.keys().any(|key| key.contains('R') || key.contains('C')));
}

#[test]
fn owning_fields_cover_stored_descendants_but_never_text_mark_references() {
    let mut doc = fixture();
    let (names, _) = names(&mut doc);
    let ownership = Ownership::fixture(&mut doc, &names);
    let mut held = Holds::default();
    held.objects.entry("doc".into()).or_default().insert("body".into());
    let covered = ownership.supplied(&held);
    assert!(covered.contains("b1") && covered.contains("b4/1/1/0"), "{covered:?}");
    assert!(!covered.contains("inline:pic") && !covered.contains("inline:shape"), "{covered:?}");
    assert!(!covered.contains("part:header/0") && !covered.contains("thread:0"), "{covered:?}");
    held.objects.clear();
    held.windows.insert("doc.body".into());
    assert!(ownership.supplied(&held).is_empty(), "a membership window holds no descendant content");
    held.objects.entry("part:header".into()).or_default().insert("blocks".into());
    let covered = ownership.supplied(&held);
    assert!(covered.contains("part:header/0") && !covered.contains("part:footer/0"), "{covered:?}");
}

#[test]
fn review_comparison_boundaries() {
    let mut doc = fixture();
    let (names, _) = names(&mut doc);
    let commands: BTreeMap<_, _> = doc.commands().into_iter().map(|c| (c.id.clone(), c)).collect();
    select(&mut doc, at(&[3], 0), at(&[5], 0));
    let held = declared(&mut doc, &names, &commands["table.fromText"], &json!({}));
    assert!(held.objects["t4"].contains("ALL"), "{held:?}");
    let ownership = Ownership::fixture(&mut doc, &names);
    let mut read = Holds::default();
    let mut held = Holds::default();
    read.objects.insert("inline:shape".into(), ["uid".into()].into());
    held.facts.insert("drawingIds".into());
    assert!(!differences(&read, &held, &ownership).0.is_empty(), "swapping IDs preserves a sorted set but changes the named objects");
    read.objects.clear();
    read.facts.insert("layout:b1".into());
    held.facts = ["layout:b8".into()].into();
    let (less, more) = differences(&read, &held, &ownership);
    assert!(!less.is_empty() && !more.is_empty(), "geometry on another holder supplies neither direction");
    held.objects.insert("doc".into(), ["ALL".into()].into());
    read.facts = ["file".into()].into();
    assert!(!differences(&read, &held, &ownership).0.is_empty(), "document ownership cannot hold an external file");
    read.facts.clear();
    read.objects.insert("inline:eq".into(), ["value".into()].into());
    held.objects = [("doc".into(), ["body".into()].into())].into();
    assert!(differences(&read, &held, &ownership).0.is_empty(), "the owned paragraph's derived object change is its content change");
    held.objects.clear();
    held.windows.insert("doc.body".into());
    assert!(!differences(&read, &held, &ownership).0.is_empty(), "a list window has no paragraph content or derived values");
    read = Holds::default();
    held = Holds::default();
    read.objects.insert("comment:1".into(), ["parent".into()].into());
    held.objects.insert("comment:1".into(), ["props".into()].into());
    comment_words(&mut read, &BTreeMap::new());
    comment_words(&mut held, &BTreeMap::new());
    let (less, more) = differences(&read, &held, &ownership);
    assert!(less.is_empty() && !more.is_empty(), "parent-only reading cannot hide a hold on blocks and all other props");
}

/// The fixture's objects that have no tool id (and so no entry in [`names`]).
fn is_fixture_object(name: &str) -> bool {
    [
        "doc",
        "t4",
        "r4/0",
        "r4/1",
        "c4/0/0",
        "c4/0/1",
        "c4/1/0",
        "c4/1/1",
        "thread:0",
        "comment:0",
        "comment:1",
        "rev:0",
        "media:0",
        "inline:pic",
        "inline:eq",
    ]
    .contains(&name)
        || name.starts_with("part:")
}
