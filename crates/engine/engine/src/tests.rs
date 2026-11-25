use kition_doc_doc::props::BorderStyle;
use kition_doc_doc::{Pos, StoryRef};
use serde_json::json;

use crate::{Session, cmd};

fn s() -> Session {
    Session::new(kition_doc_doc::Document::new())
}

fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn text(s: &Session) -> String {
    s.doc.plain_text(StoryRef::Body)
}

#[test]
fn doc_extension_dispatches_to_docbin() {
    // Garbage bytes with a .doc name must produce an error through the Word 97-2003
    // reader — never a panic and never a silently empty document.
    for name in ["x.doc", "x.dot"] {
        assert!(crate::io::open_bytes(name, &[0u8; 64]).is_err(), "{name} should fail");
    }
}

#[test]
fn every_command_has_unique_id() {
    let reg = cmd::registry();
    let mut ids: Vec<&str> = reg.all().iter().map(|c| c.id).collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate command ids");
    assert!(n > 150, "{n} commands");
}

#[test]
fn every_shortcut_runs_one_command() {
    // `by_shortcut` takes the first match, so a shared key silently shadows the later command.
    let reg = cmd::registry();
    let mut seen = std::collections::HashMap::new();
    for c in reg.all() {
        for k in c.shortcut.split(" / ").filter(|k| !k.is_empty()) {
            if let Some(other) = seen.insert(k.to_ascii_lowercase(), c.id) {
                panic!("{k} is bound to both {other} and {}", c.id);
            }
        }
    }
    assert_eq!(reg.by_shortcut("Shift+F3").map(|c| c.id), Some("format.changeCase"));
}

#[test]
fn typing_enter_undo() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    run(&mut s, "text.insert", json!({"text": " world"}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "Second"}));
    assert_eq!(text(&s), "Hello world\nSecond");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s), "Hello world\n");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s), "Hello world");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s), "");
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(text(&s), "Hello world");
}

#[test]
fn joined_commands_are_one_undo_step() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    run(&mut s, "paragraph.indents", json!({"left": 9.0}));
    run(&mut s, "paragraph.indents", json!({"left": 18.0}));
    for x in [27.0, 36.0] {
        s.join_next_undo();
        run(&mut s, "paragraph.indents", json!({"left": x}));
    }
    let indent = |s: &Session| s.doc.para_at(&s.sel.focus).and_then(|p| p.props.indent_left);
    assert_eq!(indent(&s), Some(36.0));
    // One undo reverts the whole second drag, not just its last frame.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(indent(&s), Some(9.0));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(indent(&s), None);
    assert_eq!(text(&s), "Hello");
    run(&mut s, "edit.redo", json!({}));
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(indent(&s), Some(36.0));
}

#[test]
fn restore_takes_a_command_back_exactly() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "alpha beta"}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "gamma"}));
    run(&mut s, "edit.undo", json!({}));
    assert!(s.can_redo());
    let (doc, sel, labels, depth, rev) = (s.doc.clone(), s.sel.clone(), s.undo_labels(), s.undo_depth(), s.rev());
    s.dirty = false;
    let snap = s.edit_snapshot();
    assert_eq!(snap.doc(), &doc);
    assert_eq!(snap.undo_depth(), depth);
    assert!(!snap.dirty());
    run(&mut s, "edit.selectAll", json!({}));
    run(&mut s, "format.bold", json!({}));
    assert_ne!(s.doc, doc);
    assert_eq!(s.undo_depth(), depth + 1);
    assert!(!s.can_redo(), "a new command clears redo");
    s.restore(snap);
    assert_eq!(s.doc, doc);
    assert_eq!(s.sel, sel);
    assert_eq!(s.undo_labels(), labels, "the command's undo step is gone");
    assert!(s.can_redo(), "redo is back");
    assert!(!s.dirty);
    assert!(s.rev() > rev, "the layout is recomputed");
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(text(&s), "alpha beta\ngamma");
}

#[test]
fn restore_is_exact_at_the_undo_limit() {
    // 510 steps, each with its own indent: the stack holds the newest 500.
    let build = || {
        let mut s = s();
        run(&mut s, "text.insert", json!({"text": "alpha"}));
        for i in 0..510 {
            run(&mut s, "paragraph.indents", json!({"left": f64::from(i)}));
        }
        s
    };
    // What undo walks through: (label, indent) for every step, newest first.
    let walk = |s: &mut Session| {
        let mut steps = Vec::new();
        while let Some(label) = s.undo_label().map(str::to_string) {
            s.undo();
            steps.push((label, s.doc.para_at(&Pos::body(0, 0)).and_then(|p| p.props.indent_left)));
        }
        steps
    };
    let expected = walk(&mut build());
    assert_eq!(expected.len(), 500);
    // One command, and several (each pushes the oldest step out of the full stack).
    for n in [1, 2, 4] {
        let mut s = build();
        let snap = s.edit_snapshot();
        for _ in 0..n {
            run(&mut s, "table.insert", json!({"rows": 1, "cols": 1}));
        }
        assert_eq!(s.undo_label(), Some("Table"));
        assert_eq!(s.undo_depth(), 500);
        s.restore(snap);
        assert_eq!(walk(&mut s), expected, "{n} command(s): the oldest steps are back, the commands' steps are gone");
    }
    // A command that fails at the limit puts its eviction back with the stacks, so a later
    // restore doesn't bring back a step that never left.
    let mut s = build();
    let snap = s.edit_snapshot();
    assert!(s.run("paragraph.align", &json!({"value": "bogus"})).is_err());
    run(&mut s, "table.insert", json!({"rows": 1, "cols": 1}));
    s.restore(snap);
    assert_eq!(walk(&mut s), expected, "a failed command between snapshot and restore");
    // An automatic change recorded after the fact (`push_undo`) at the limit counts its eviction.
    let mut s = build();
    let snap = s.edit_snapshot();
    let (doc, sel) = (s.doc.clone(), s.sel.clone());
    s.push_undo("AutoCorrect", doc, sel);
    s.restore(snap);
    assert_eq!(walk(&mut s), expected, "push_undo between snapshot and restore");
}

#[test]
fn restore_keeps_the_open_undo_step() {
    // Typing in progress: text typed after the restore joins the same undo step.
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "alpha"}));
    let depth = s.undo_depth();
    let snap = s.edit_snapshot();
    run(&mut s, "format.alignCenter", json!({}));
    s.restore(snap);
    run(&mut s, "text.insert", json!({"text": " beta"}));
    assert_eq!(s.undo_depth(), depth, "still one typing step");
    // A drag in progress (`join_next_undo`): its next frame joins the same undo step.
    run(&mut s, "paragraph.indents", json!({"left": 9.0}));
    s.join_next_undo();
    let depth = s.undo_depth();
    let snap = s.edit_snapshot();
    run(&mut s, "paragraph.indents", json!({"left": 18.0}));
    s.restore(snap);
    run(&mut s, "paragraph.indents", json!({"left": 27.0}));
    assert_eq!(s.undo_depth(), depth, "still one drag step");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).and_then(|p| p.props.indent_left), None);
}

#[test]
fn failed_command_keeps_undo_and_redo() {
    // A command that fails after its undo checkpoint leaves the history exactly as it was:
    // the redo stack survives, and no empty undo step is added.
    for (id, params) in [("text.insert", json!({})), ("paragraph.align", json!({"value": "bogus"}))] {
        let mut s = s();
        run(&mut s, "text.insert", json!({"text": "Hello"}));
        run(&mut s, "edit.undo", json!({}));
        assert!(s.can_redo());
        assert!(s.run(id, &params).is_err(), "{id} should fail");
        assert!(s.can_redo(), "{id}: redo lost");
        assert!(!s.can_undo(), "{id}: failed command left an undo step");
        run(&mut s, "edit.redo", json!({}));
        assert_eq!(text(&s), "Hello", "{id}");
    }

    // At the undo limit, the oldest step is not evicted by a command that fails.
    let mut s = s();
    let steps = 600;
    for i in 1..=steps {
        run(&mut s, "paragraph.indents", json!({"left": i as f32}));
    }
    let kept = s.undo_labels().len();
    assert!(kept < steps, "the history should be at its limit");
    assert!(s.run("paragraph.align", &json!({"value": "bogus"})).is_err());
    assert_eq!(s.undo_labels().len(), kept);
    while s.can_undo() {
        run(&mut s, "edit.undo", json!({}));
    }
    let indent = s.doc.para_at(&s.sel.focus).and_then(|p| p.props.indent_left);
    assert_eq!(indent, Some((steps - kept) as f32));
}

#[test]
fn failed_command_restores_history_changed_by_nested_commands() {
    // `file.removeHiddenContent` runs `comment.deleteCurrent` (allowed in a comments-only document, and it
    // checkpoints), then `change.acceptAll` (refused). The refusal must leave the undo and
    // redo stacks exactly as they were, including at and next to the history limit, where the
    // outer and nested checkpoints each evict the oldest step.
    fn timeline(s: &mut Session) -> Vec<(Vec<String>, kition_doc_doc::Document)> {
        let mut out = Vec::new();
        while s.can_redo() {
            run(s, "edit.redo", json!({}));
        }
        loop {
            // Comments are stamped with the time to the second, and the two sessions compared
            // are built a moment apart, so leave the stamp out of the comparison.
            let mut doc = s.doc.clone();
            doc.comments.values_mut().for_each(|c| c.date.clear());
            out.push((s.undo_labels(), doc));
            if !s.can_undo() {
                return out;
            }
            run(s, "edit.undo", json!({}));
        }
    }
    let kept = {
        let mut s = s();
        for i in 1..=600 {
            run(&mut s, "paragraph.indents", json!({"left": i as f32}));
        }
        s.undo_labels().len()
    };
    // Below the limit there is room for a redo step too; at the limit an Undo would leave one free.
    for (len, redo) in [(kept - 1, Some("New Comment")), (kept, None)] {
        let session = || {
            let mut s = s();
            run(&mut s, "text.insert", json!({"text": "note"}));
            for i in 1..=len + usize::from(redo.is_some()) - 4 {
                run(&mut s, "paragraph.indents", json!({"left": i as f32}));
            }
            run(&mut s, "file.restrictEditing", json!({"mode": "comments"}));
            run(&mut s, "comment.new", json!({"text": "one"}));
            run(&mut s, "comment.new", json!({"text": "two"}));
            if redo.is_some() {
                run(&mut s, "edit.undo", json!({}));
            }
            assert_eq!(s.undo_labels().len(), len);
            assert_eq!(s.redo_label(), redo);
            s
        };
        let mut failed = session();
        let err = failed.run("file.removeHiddenContent", &json!({"remove": ["comments", "revisions"]}));
        assert!(matches!(err, Err(crate::CmdError::Disabled(_))), "{len}: {err:?}");
        assert_eq!(failed.undo_labels().len(), len, "{len}: undo steps");
        assert_eq!(failed.redo_label(), redo, "{len}: redo step");
        assert!(timeline(&mut failed) == timeline(&mut session()), "{len}: undo history changed");
    }
}

#[test]
fn backspace_and_delete() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "abc"}));
    run(&mut s, "text.backspace", json!({}));
    assert_eq!(text(&s), "ab");
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "cd"}));
    run(&mut s, "caret.home", json!({}));
    run(&mut s, "text.backspace", json!({}));
    // Enter capitalised the first word, as Word's AutoCorrect does.
    assert_eq!(text(&s), "Abcd");
    run(&mut s, "caret.docStart", json!({}));
    run(&mut s, "text.delete", json!({}));
    assert_eq!(text(&s), "bcd");
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "text.deleteWordBack", json!({}));
    assert_eq!(text(&s), "");
}

#[test]
fn bold_toggles_selection_and_caret_word() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "make this bold"}));
    run(&mut s, "select.text", json!({"text": "this"}));
    run(&mut s, "format.bold", json!({}));
    let p = s.doc.para_at(&Pos::body(0, 0)).unwrap();
    assert_eq!(p.props_of_char(5).bold, Some(true));
    assert_eq!(p.props_of_char(0).bold, None);
    run(&mut s, "format.bold", json!({}));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().props_of_char(5).bold, Some(false));
    // Caret inside a word formats the word.
    run(&mut s, "caret.set", json!({"pos": {"story": "body", "path": [0], "off": 11}}));
    run(&mut s, "format.italic", json!({}));
    let p = s.doc.para_at(&Pos::body(0, 0)).unwrap();
    assert_eq!(p.props_of_char(10).italic, Some(true));
    assert_eq!(p.props_of_char(13).italic, Some(true));
    assert_eq!(p.props_of_char(9).italic, None);
}

#[test]
fn border_toggles_and_saves() {
    use kition_doc_doc::props::Border;
    let border = |s: &Session| s.doc.para_at(&Pos::body(0, 0)).unwrap().props_of_char(5).border;
    // Saved .docx carries the border iff it is on (read back: the reader maps only `w:bdr` to `border`).
    let docx_has_bdr = |s: &Session| {
        let back = kition_doc_docx::read(&kition_doc_docx::write(&s.doc).unwrap()).unwrap();
        back.para_at(&Pos::body(0, 0)).unwrap().props_of_char(5).border.is_some()
    };
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "make this boxed"}));
    run(&mut s, "select.text", json!({"text": "this"}));
    run(&mut s, "format.border", json!({}));
    assert_eq!(border(&s), Some(Border::single(0.5)));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().props_of_char(0).border, None);
    assert!(docx_has_bdr(&s));
    run(&mut s, "format.border", json!({}));
    assert_eq!(border(&s), None);
    run(&mut s, "format.border", json!({"value": true}));
    run(&mut s, "format.border", json!({"value": true}));
    assert_eq!(border(&s), Some(Border::single(0.5)));
    run(&mut s, "format.border", json!({"value": false}));
    assert_eq!(border(&s), None);
    assert!(!docx_has_bdr(&s));
}

#[test]
fn pending_format_applies_to_typing() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "a "}));
    run(&mut s, "format.bold", json!({}));
    run(&mut s, "text.insert", json!({"text": "b"}));
    let p = s.doc.para_at(&Pos::body(0, 0)).unwrap();
    assert_eq!(p.props_of_char(2).bold, Some(true));
    assert_eq!(p.props_of_char(0).bold, None);
}

#[test]
fn styles_and_lists() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Title"}));
    run(&mut s, "paragraph.heading1", json!({}));
    run(&mut s, "text.newParagraph", json!({}));
    // Heading's next style is Normal.
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().props.style.as_deref(), Some("Normal"));
    run(&mut s, "paragraph.bullets", json!({}));
    run(&mut s, "text.insert", json!({"text": "one"}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "two"}));
    let n1 = s.doc.para_at(&Pos::body(1, 0)).unwrap().props.numbering.unwrap();
    let n2 = s.doc.para_at(&Pos::body(2, 0)).unwrap().props.numbering.unwrap();
    assert_eq!(n1.num, n2.num);
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.newParagraph", json!({})); // empty item ends the list
    assert!(s.doc.para_at(&s.sel.focus).unwrap().props.numbering.is_none_or(|n| n.num == 0));
    // "1. " autoformat.
    run(&mut s, "text.insert", json!({"text": "1."}));
    run(&mut s, "text.insert", json!({"text": " "}));
    assert!(s.doc.para_at(&s.sel.focus).unwrap().props.numbering.is_some_and(|n| n.num != 0));
}

#[test]
fn applying_a_heading_clears_list_numbering() {
    // Applying a heading (or Title/Normal) to a numbered paragraph removes its direct list
    // numbering. A dead `&& id == "Normal"` term used to let only Normal clear it, so headings
    // kept the numbering.
    for style_cmd in ["paragraph.heading1", "paragraph.heading2", "paragraph.normal"] {
        let mut s = s();
        run(&mut s, "text.insert", json!({"text": "item"}));
        run(&mut s, "paragraph.bullets", json!({}));
        assert!(
            s.doc.para_at(&Pos::body(0, 0)).unwrap().props.numbering.is_some_and(|n| n.num != 0),
            "{style_cmd}: expected list numbering before applying the style"
        );
        run(&mut s, style_cmd, json!({}));
        assert!(
            s.doc.para_at(&Pos::body(0, 0)).unwrap().props.numbering.is_none_or(|n| n.num == 0),
            "{style_cmd}: should clear direct list numbering"
        );
    }
}

#[test]
fn find_replace() {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "cat dog cat\nCat bird"}));
    let r = run(&mut s, "edit.find", json!({"text": "cat"}));
    assert_eq!(r["count"], 3);
    let r = run(&mut s, "edit.replaceAll", json!({"text": "cat", "with": "fox", "matchCase": true}));
    assert_eq!(r["replaced"], 2);
    assert_eq!(text(&s), "fox dog fox\nCat bird");
    let r = run(&mut s, "edit.find", json!({"text": "\\b\\w{3}\\b", "regex": true, "matchCase": false}));
    assert_eq!(r["count"], 4);
}

#[test]
fn replace_under_track_changes_skips_deleted_text() {
    // Replace marks the match deleted and leaves it in the text; the next
    // step must not find it again, or Replace never advances.
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "one cat two cat"}));
    run(&mut s, "change.track", json!({"value": true}));
    run(&mut s, "caret.docStart", json!({}));
    assert_eq!(run(&mut s, "edit.find", json!({"text": "cat"}))["count"], 2);
    let mut remaining = Vec::new();
    for _ in 0..4 {
        remaining.push(run(&mut s, "edit.replace", json!({"text": "cat", "with": "dog"}))["remaining"].clone());
    }
    assert_eq!(remaining, [json!(1), json!(0), json!(0), json!(0)]);
    assert_eq!(run(&mut s, "edit.find", json!({"text": "cat"}))["count"], 0);
    run(&mut s, "change.acceptAll", json!({}));
    assert_eq!(text(&s), "one dog two dog");

    // A match that runs across tracked-deleted text is not in the document.
    let mut s = self::s();
    run(&mut s, "file.setText", json!({"text": "cat"}));
    run(&mut s, "change.track", json!({"value": true}));
    run(&mut s, "select.range", json!({"anchor": {"block": 0, "off": 1}, "focus": {"block": 0, "off": 2}}));
    run(&mut s, "text.delete", json!({}));
    assert_eq!(text(&s), "cat", "the deletion is tracked, not applied");
    assert_eq!(run(&mut s, "edit.find", json!({"text": "cat"}))["count"], 0);
    assert_eq!(run(&mut s, "edit.find", json!({"text": "t"}))["count"], 1);

    // Without Track Changes, Replace steps through every match as before.
    let mut s = self::s();
    run(&mut s, "file.setText", json!({"text": "one cat two cat"}));
    run(&mut s, "caret.docStart", json!({}));
    run(&mut s, "edit.find", json!({"text": "cat"}));
    run(&mut s, "edit.replace", json!({"text": "cat", "with": "dog"}));
    let r = run(&mut s, "edit.replace", json!({"text": "cat", "with": "dog"}));
    assert_eq!(r["remaining"], 0);
    assert_eq!(text(&s), "one dog two dog");
}

#[test]
fn find_reads_the_text_around_tracked_deletions() {
    // `edits` are (start, end) byte ranges deleted with Track Changes on, last first.
    fn tracked(text: &str, edits: &[(usize, usize)]) -> Session {
        let mut s = self::s();
        run(&mut s, "file.setText", json!({"text": text}));
        run(&mut s, "change.track", json!({"value": true}));
        for (a, b) in edits {
            run(&mut s, "select.range", json!({"anchor": {"block": 0, "off": a}, "focus": {"block": 0, "off": b}}));
            run(&mut s, "text.delete", json!({}));
        }
        assert_eq!(text, self::text(&s), "the deletions are tracked, not applied");
        s
    }
    let offs = |r: &serde_json::Value| -> Vec<(u64, u64)> {
        r["matches"].as_array().unwrap().iter().map(|m| (m["start"]["off"].as_u64().unwrap(), m["end"]["off"].as_u64().unwrap())).collect()
    };

    // "aaa" with the first "a" deleted reads "aa": the rejected raw match 0..2 must not hide
    // the live one at 1..3.
    let mut s = tracked("aaa", &[(0, 1)]);
    assert_eq!(offs(&run(&mut s, "edit.find", json!({"text": "aa"}))), [(1, 3)]);
    assert_eq!(offs(&run(&mut s, "edit.find", json!({"text": "^a", "regex": true}))), [(1, 2)]);
    assert_eq!(run(&mut s, "edit.replaceAll", json!({"text": "aa", "with": "b"}))["replaced"], 1);

    // Whole words follow the live text: deleting the space joins "cat" and "fish"...
    let mut s = tracked("cat fish", &[(3, 4)]);
    assert_eq!(run(&mut s, "edit.find", json!({"text": "fish", "wholeWord": true}))["count"], 0);
    assert_eq!(run(&mut s, "edit.find", json!({"text": "catfish", "wholeWord": true}))["count"], 0, "a match may not span a deletion");
    assert_eq!(run(&mut s, "edit.find", json!({"text": "fish", "wholeWord": false}))["count"], 1);
    // ...and deleting the "X" leaves "fish" a word of its own.
    let mut s = tracked("cat Xfish", &[(4, 5)]);
    assert_eq!(offs(&run(&mut s, "edit.find", json!({"text": "FISH", "wholeWord": true}))), [(5, 9)]);
    assert_eq!(run(&mut s, "edit.find", json!({"text": "FISH", "wholeWord": true, "matchCase": true}))["count"], 0);

    // Replace and the Find Next/Previous steps agree with Find.
    let mut s = tracked("aaa aaa", &[(4, 5), (0, 1)]);
    run(&mut s, "caret.docStart", json!({}));
    assert_eq!(run(&mut s, "edit.find", json!({"text": "aa"}))["count"], 2);
    assert_eq!(run(&mut s, "edit.findNext", json!({}))["index"], 1);
    assert_eq!(run(&mut s, "edit.findPrevious", json!({}))["index"], 0);
    let r = run(&mut s, "edit.replace", json!({"text": "aa", "with": "b"}));
    assert_eq!(r["remaining"], 1);
}

#[test]
fn clipboard_round_trip() {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "alpha beta\ngamma"}));
    run(&mut s, "select.range", json!({"anchor": {"block": 0, "off": 6}, "focus": {"block": 1, "off": 2}}));
    run(&mut s, "edit.copy", json!({}));
    assert_eq!(s.clipboard_text, "beta\nga");
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "edit.paste", json!({}));
    assert_eq!(text(&s), "alpha beta\ngammabeta\nga");
    run(&mut s, "edit.selectAll", json!({}));
    run(&mut s, "edit.cut", json!({}));
    assert_eq!(text(&s), "");
    run(&mut s, "edit.paste", json!({"text": "plain\ntext"}));
    assert_eq!(text(&s), "plain\ntext");
}

fn bold_at(s: &Session, block: usize, off: usize) -> bool {
    let pos = Pos { story: StoryRef::Body, path: kition_doc_doc::Path::top(block), off };
    s.doc.para_at(&pos).and_then(|p| p.props_of_char(off).bold).unwrap_or(false)
}

#[test]
fn paste_special_lists_formats_and_pastes_text_without_formatting() {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "Bold words"}));
    run(&mut s, "select.text", json!({"text": "Bold"}));
    run(&mut s, "format.bold", json!({}));
    run(&mut s, "edit.copy", json!({}));
    // No format: the formats are listed (and a front end is asked to show the dialog).
    let r = run(&mut s, "edit.pasteSpecial", json!({}));
    let ids: Vec<&str> = r["formats"].as_array().unwrap().iter().map(|f| f["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["formatted", "text"]);
    assert_eq!(s.ui_requests.last().unwrap()["open"], "pasteSpecial");
    run(&mut s, "caret.docEnd", json!({}));
    let r = run(&mut s, "edit.pasteSpecial", json!({"as": "text"}));
    assert_eq!(r["pastedAs"], "text");
    assert_eq!(text(&s), "Bold wordsBold");
    assert!(bold_at(&s, 0, 0));
    assert!(!bold_at(&s, 0, 11), "unformatted text drops the bold");
    // Formatted keeps it.
    run(&mut s, "edit.pasteSpecial", json!({"as": "formatted"}));
    assert_eq!(text(&s), "Bold wordsBoldBold");
    assert!(bold_at(&s, 0, 15));
    // Text that someone else copied since: our rich copy is no longer offered.
    let r = run(&mut s, "edit.pasteSpecial", json!({"text": "elsewhere"}));
    assert_eq!(r["formats"].as_array().unwrap().len(), 1);
    assert!(s.run("edit.pasteSpecial", &json!({"as": "formatted", "text": "elsewhere"})).is_err());
}

#[test]
fn paste_special_html_and_rtf_keep_bold() {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "x"}));
    run(&mut s, "caret.docEnd", json!({}));
    let html = "<p>plain <b>strong</b></p>";
    let r = run(&mut s, "edit.pasteSpecial", json!({"html": html, "text": "plain strong"}));
    let ids: Vec<&str> = r["formats"].as_array().unwrap().iter().map(|f| f["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["html", "text"]);
    run(&mut s, "edit.pasteSpecial", json!({"as": "html", "html": html}));
    assert_eq!(text(&s), "xplain strong");
    assert!(!bold_at(&s, 0, 2));
    assert!(bold_at(&s, 0, 8), "the <b> run stays bold");
    // HTML source copied as text can be pasted as HTML too.
    run(&mut s, "file.setText", json!({"text": ""}));
    run(&mut s, "edit.pasteSpecial", json!({"as": "html", "text": "<b>hi</b> there</p>"}));
    assert_eq!(text(&s), "hi there");
    assert!(bold_at(&s, 0, 0) && !bold_at(&s, 0, 4));
    // RTF.
    run(&mut s, "file.setText", json!({"text": ""}));
    run(&mut s, "edit.pasteSpecial", json!({"as": "rtf", "rtf": r"{\rtf1\ansi {\b bold}\b0  then}"}));
    assert!(text(&s).starts_with("bold"), "{}", text(&s));
    assert!(bold_at(&s, 0, 0));
    // Asking for a format the clipboard lacks is an error, not a panic.
    assert!(s.run("edit.pasteSpecial", &json!({"as": "rtf", "text": "plain"})).is_err());
    assert!(s.run("edit.pasteSpecial", &json!({"as": "bogus", "text": "plain"})).is_err());
    let huge = "a".repeat(crate::cmd::paste::MAX_PASTE_BYTES + 1);
    assert!(s.run("edit.pasteSpecial", &json!({"as": "text", "text": huge})).is_err());
}

#[test]
fn paste_special_respects_track_changes_and_brings_lists() {
    let mut s = s();
    run(&mut s, "change.track", json!({"value": true}));
    run(&mut s, "edit.pasteSpecial", json!({"as": "html", "html": "<ul><li>one</li><li>two</li></ul>"}));
    assert!(text(&s).starts_with("one\ntwo"), "{}", text(&s));
    let ch = run(&mut s, "change.list", json!({}));
    assert!(!ch.as_array().unwrap().is_empty(), "the paste is a tracked insertion");
    let first = s.doc.body.first().and_then(|b| b.as_para()).unwrap();
    let n = first.props.numbering.expect("list kept");
    assert!(s.doc.numbering.nums.iter().any(|x| x.id == n.num), "the list definition came along");
    run(&mut s, "change.acceptAll", json!({}));
    assert!(text(&s).starts_with("one\ntwo"));
}

#[test]
fn clipboard_pane_collects_copies_and_pastes_one_undoably() {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "one two three"}));
    for (a, b) in [(0, 3), (4, 7), (8, 13)] {
        run(&mut s, "select.range", json!({"anchor": {"block": 0, "off": a}, "focus": {"block": 0, "off": b}}));
        run(&mut s, "edit.copy", json!({}));
    }
    // Copying the same text again adds nothing.
    run(&mut s, "edit.copy", json!({}));
    let items = run(&mut s, "edit.clipboardItems", json!({}));
    let previews: Vec<&str> = items.as_array().unwrap().iter().map(|i| i["preview"].as_str().unwrap()).collect();
    assert_eq!(previews, ["three", "two", "one"], "newest first");
    assert_eq!(run(&mut s, "edit.clipboardPane", json!({}))["value"], true);
    assert!(s.view.clipboard_pane);

    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "edit.pasteClipboardItem", json!({"index": 1}));
    assert_eq!(text(&s), "one two threetwo");
    assert_eq!(s.clipboard_text, "three", "the clipboard itself is unchanged");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s), "one two three");

    run(&mut s, "edit.pasteAllClipboard", json!({}));
    assert_eq!(text(&s), "one two threeonetwothree", "Paste All goes in copy order");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s), "one two three", "Paste All is one undo step");

    let left = run(&mut s, "edit.deleteClipboardItem", json!({"index": 0}));
    assert_eq!(left[0]["preview"], "two");
    run(&mut s, "edit.clearClipboard", json!({}));
    assert!(s.clip_history.is_empty());
    assert!(s.run("edit.pasteClipboardItem", &json!({"index": 0})).is_err());
}

#[test]
fn clipboard_pane_is_capped_and_rejects_bad_indexes() {
    let mut s = s();
    let words: Vec<String> = (0..30).map(|i| format!("w{i}")).collect();
    run(&mut s, "file.setText", json!({"text": words.join(" ")}));
    // Cutting each word in turn leaves the spaces: word `off` starts at `off`.
    for (off, w) in words.iter().enumerate() {
        run(&mut s, "select.range", json!({"anchor": {"block": 0, "off": off}, "focus": {"block": 0, "off": off + w.len()}}));
        run(&mut s, "edit.cut", json!({}));
    }
    let items = run(&mut s, "edit.clipboardItems", json!({}));
    let items = items.as_array().unwrap();
    assert_eq!(items.len(), crate::cmd::edit::CLIP_MAX_ITEMS);
    assert_eq!(items[0]["preview"], "w29");
    assert_eq!(items[23]["preview"], "w6", "the oldest drop out");

    let before = text(&s);
    for bad in [json!({"index": 24}), json!({"index": -1}), json!({"index": 1e300}), json!({"index": "x"}), json!({})] {
        assert!(s.run("edit.pasteClipboardItem", &bad).is_err(), "{bad}");
        assert!(s.run("edit.deleteClipboardItem", &bad).is_err(), "{bad}");
    }
    assert_eq!(text(&s), before);
    assert_eq!(s.clip_history.len(), 24);

    // A huge copy still reaches the clipboard but is not collected.
    run(&mut s, "file.setText", json!({"text": "x".repeat(3 << 20)}));
    run(&mut s, "edit.selectAll", json!({}));
    run(&mut s, "edit.copy", json!({}));
    assert_eq!(s.clipboard_text.len(), 3 << 20);
    assert_eq!(s.clip_history.items()[0].text, "w29");
}

#[test]
fn tables_commands() {
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 2, "cols": 3}));
    assert!(s.sel.focus.path.cell().is_some());
    run(&mut s, "text.insert", json!({"text": "A1"}));
    run(&mut s, "text.tab", json!({}));
    run(&mut s, "text.insert", json!({"text": "B1"}));
    run(&mut s, "table.insertRowBelow", json!({}));
    run(&mut s, "table.insertColumnRight", json!({}));
    let t = s.doc.body.iter().find_map(|b| b.as_table()).unwrap();
    assert_eq!(t.rows.len(), 3);
    assert_eq!(t.cols(), 4);
    run(&mut s, "table.deleteTable", json!({}));
    assert!(s.doc.body.iter().all(|b| b.as_table().is_none()));
    assert!(s.run("table.merge", &json!({})).is_err());
}

/// AutoFit Contents measures the text: a column of short words narrows, a column with a
/// long sentence widens (wrapping within the page), and the widths are written to the grid.
#[test]
fn autofit_contents_sizes_columns_to_their_text() {
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 2, "cols": 3}));
    let grid = |s: &Session| s.doc.body.iter().find_map(|b| b.as_table()).unwrap().grid.clone();
    let before = grid(&s);
    run(&mut s, "text.insert", json!({"text": "A"}));
    run(&mut s, "text.tab", json!({}));
    run(
        &mut s,
        "text.insert",
        json!({"text": "This cell holds a long sentence that would wrap many times in a third of the page width, so it should get most of the room."}),
    );
    run(&mut s, "text.tab", json!({}));
    run(&mut s, "text.insert", json!({"text": "B"}));
    run(&mut s, "table.autofit", json!({"mode": "contents"}));
    let after = grid(&s);
    let tw = cmd::page::sect(&s).text_width();
    assert!(after[0] < before[0] / 2.0, "short column narrows: {before:?} → {after:?}");
    assert!(after[2] < before[2] / 2.0, "short column narrows: {before:?} → {after:?}");
    assert!(after[1] > before[1] * 1.5, "long column widens: {before:?} → {after:?}");
    let total: f32 = after.iter().sum();
    assert!(total <= tw + 0.5 && total > tw - 1.0, "the long text fills the page width: {total} vs {tw}");
    let t = s.doc.body.iter().find_map(|b| b.as_table()).unwrap();
    assert!(t.props.width_pct.is_none() && !t.props.fixed);
    assert_eq!(t.rows[0].cells[1].props.width, Some(after[1]), "cell widths follow the grid");
    // Short text only: every column gets just what its text needs.
    s.doc = kition_doc_doc::Document::new();
    s.sel = crate::Selection::caret(Pos::body(0, 0));
    run(&mut s, "table.insert", json!({"rows": 1, "cols": 2}));
    run(&mut s, "text.insert", json!({"text": "Name"}));
    run(&mut s, "text.tab", json!({}));
    run(&mut s, "text.insert", json!({"text": "Quantity"}));
    run(&mut s, "table.autofit", json!({"mode": "contents"}));
    let g = grid(&s);
    assert!(g[0] < g[1] && g[1] < 100.0, "{g:?}");
    // The layout uses the new widths.
    let w = s.layout().pages[0].items.iter().find_map(|it| match it {
        kition_doc_layout::Placed::Cell { rect, row: 0, cell: 0, .. } => Some(rect.w),
        _ => None,
    });
    assert!(w.is_some_and(|w| (w - g[0]).abs() < 0.5), "{w:?} vs {g:?}");
}

/// The Height and Width boxes (Table Layout › Cell Size) run `table.rowHeight` and
/// `table.columnWidth`; each edit is one Undo.
#[test]
fn cell_size_boxes_set_row_height_and_column_width() {
    use kition_doc_doc::props::HeightRule;
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 2, "cols": 2}));
    run(&mut s, "table.autofit", json!({"mode": "window"}));
    let t0 = s.doc.body.iter().find_map(|b| b.as_table()).unwrap().clone();
    run(&mut s, "table.columnWidth", json!({"width": 100.0}));
    run(&mut s, "table.rowHeight", json!({"height": 30.0, "rule": "exact"}));
    let t = s.doc.body.iter().find_map(|b| b.as_table()).unwrap();
    assert_eq!(t.grid[0], 100.0);
    assert_eq!(t.rows[0].cells[0].props.width, Some(100.0));
    assert!(t.props.width_pct.is_none(), "the column isn't scaled back to the window");
    assert_eq!((t.rows[0].props.height, t.rows[0].props.height_rule), (Some(30.0), HeightRule::Exact));
    let rect = s.layout().pages[0].items.iter().find_map(|it| match it {
        kition_doc_layout::Placed::Cell { rect, row: 0, cell: 0, .. } => Some(*rect),
        _ => None,
    });
    assert!(rect.is_some_and(|r| (r.w - 100.0).abs() < 0.5 && (r.h - 30.0).abs() < 0.5), "{rect:?}");
    assert!(s.run("table.rowHeight", &json!({"height": 30.0, "rule": "sideways"})).is_err());
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.body.iter().find_map(|b| b.as_table()).unwrap(), &t0);
}

/// Table Properties: table, row, column and cell settings land as one undo step, and
/// without settings the command reports the current ones (the dialog's source).
#[test]
fn table_properties_apply_in_one_step() {
    use kition_doc_doc::props::{Align, HeightRule, VAlign};
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 2, "cols": 2}));
    let t0 = s.doc.body.iter().find_map(|b| b.as_table()).unwrap().clone();
    run(
        &mut s,
        "table.properties",
        json!({"align": "center", "indent": 18.0, "width": 300.0, "rowHeight": 40.0, "rowHeightRule": "exact", "allowBreak": false,
               "headerRow": true, "columnWidth": 120.0, "cellWidth": 110.0, "valign": "bottom"}),
    );
    let t = s.doc.body.iter().find_map(|b| b.as_table()).unwrap();
    assert_eq!((t.props.align, t.props.indent, t.props.width), (Some(Align::Center), Some(18.0), Some(300.0)));
    let row = &t.rows[0];
    assert_eq!((row.props.height, row.props.height_rule, row.props.cant_split, row.props.header), (Some(40.0), HeightRule::Exact, true, true));
    assert_eq!(t.grid[0], 120.0);
    assert_eq!((row.cells[0].props.width, row.cells[0].props.valign), (Some(110.0), VAlign::Bottom));
    let applied = t.clone();
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.body.iter().find_map(|b| b.as_table()).unwrap(), &t0, "one undo step per apply");
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.doc.body.iter().find_map(|b| b.as_table()).unwrap(), &applied);
    let state = run(&mut s, "table.properties", json!({}));
    assert_eq!(state["align"], "center");
    assert_eq!(state["row"]["heightRule"], "exact");
    assert_eq!(state["columnWidth"], 120.0);
    assert_eq!(state["cell"]["valign"], "bottom");
    // Clearing the row height makes it automatic again; bad values are refused.
    run(&mut s, "table.properties", json!({"rowHeight": null}));
    assert_eq!(s.doc.body.iter().find_map(|b| b.as_table()).unwrap().rows[0].props.height_rule, HeightRule::Auto);
    assert!(s.run("table.properties", &json!({"valign": "middle"})).is_err());
}

/// Table Layout › Text Direction cycles the selected cells' text through horizontal,
/// top-to-bottom and bottom-to-top, and the layout turns the text.
#[test]
fn table_text_direction_cycles_and_turns_the_text() {
    use kition_doc_doc::props::TextDirection;
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 1, "cols": 3}));
    run(&mut s, "text.insert", json!({"text": "Turned"}));
    let dirs = |s: &Session| -> Vec<TextDirection> {
        s.doc.body.iter().find_map(|b| b.as_table()).unwrap().rows[0].cells.iter().map(|c| c.props.text_direction).collect()
    };
    let turned = |s: &mut Session| {
        s.layout().pages[0].items.iter().find_map(|it| match it {
            kition_doc_layout::Placed::Lines { para, turn, .. } if para.lines.first().is_some_and(|l| l.stop > 0) => Some(*turn),
            _ => None,
        })
    };
    assert_eq!(turned(&mut s), Some(TextDirection::Horizontal));
    run(&mut s, "table.textDirection", json!({}));
    assert_eq!(dirs(&s), [TextDirection::Down, TextDirection::Horizontal, TextDirection::Horizontal]);
    assert_eq!(turned(&mut s), Some(TextDirection::Down), "the text is drawn turned");
    run(&mut s, "table.textDirection", json!({}));
    assert_eq!(dirs(&s)[0], TextDirection::Up);
    assert_eq!(turned(&mut s), Some(TextDirection::Up));
    run(&mut s, "table.textDirection", json!({}));
    assert_eq!(dirs(&s)[0], TextDirection::Horizontal);
    // Every selected cell gets the caret cell's next direction (the issue's selection spans cells).
    let a = s.sel.focus.clone();
    let mut b = a.clone();
    if let Some(last) = b.path.0.get_mut(2) {
        *last = 1;
    }
    b.off = 0;
    s.sel = crate::Selection { anchor: a, focus: b };
    run(&mut s, "table.textDirection", json!({}));
    assert_eq!(dirs(&s), [TextDirection::Down, TextDirection::Down, TextDirection::Horizontal]);
    run(&mut s, "table.textDirection", json!({"value": "up"}));
    assert_eq!(dirs(&s), [TextDirection::Up, TextDirection::Up, TextDirection::Horizontal]);
    assert!(s.run("table.textDirection", &json!({"value": "sideways"})).is_err());
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(dirs(&s), [TextDirection::Down, TextDirection::Down, TextDirection::Horizontal]);
    // Outside a table it is disabled.
    s.sel = crate::Selection::caret(Pos::body(0, 0));
    if s.sel.focus.path.cell().is_none() {
        assert!(s.run("table.textDirection", &json!({})).is_err());
    }
}

#[test]
fn table_border_presets_mask_the_sides_they_clear() {
    // On a default (TableGrid) table the "outside"/"inside" presets must write nil over the
    // sides they clear; without it those sides inherit the style's grid and every
    // preset renders like "all".
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 2, "cols": 2}));
    // Distinct vertical/horizontal rule positions on page 0.
    let rule_pos = |s: &mut Session| {
        let (mut vx, mut hy) = (Vec::new(), Vec::new());
        for it in &s.layout().pages[0].items {
            if let crate::layout::Placed::Rule { x0, y0, x1, y1, .. } = it {
                if (x0 - x1).abs() < 1e-3 && !vx.contains(x0) {
                    vx.push(*x0);
                }
                if (y0 - y1).abs() < 1e-3 && !hy.contains(y0) {
                    hy.push(*y0);
                }
            }
        }
        (vx, hy)
    };
    // Style default: a full 2x2 grid — 3 vertical + 3 horizontal lines.
    let (vx, hy) = rule_pos(&mut s);
    assert_eq!((vx.len(), hy.len()), (3, 3), "baseline grid: {vx:?} {hy:?}");
    run(&mut s, "table.borders", json!({"kind": "outside"}));
    let (vx, hy) = rule_pos(&mut s);
    assert_eq!((vx.len(), hy.len()), (2, 2), "outside keeps only the frame: {vx:?} {hy:?}");
    let b = s.doc.body.iter().find_map(|x| x.as_table()).unwrap().props.borders.unwrap();
    assert!(b.between.is_some_and(|x| x.style == BorderStyle::None));
    assert!(b.inside_v.is_some_and(|x| x.style == BorderStyle::None));
    run(&mut s, "table.borders", json!({"kind": "inside"}));
    let (vx, hy) = rule_pos(&mut s);
    assert_eq!((vx.len(), hy.len()), (1, 1), "inside keeps only the inner rules: {vx:?} {hy:?}");
    let b = s.doc.body.iter().find_map(|x| x.as_table()).unwrap().props.borders.unwrap();
    for side in [b.top, b.bottom, b.left, b.right] {
        assert!(side.is_some_and(|x| x.style == BorderStyle::None));
    }
}

#[test]
fn page_setup_and_breaks() {
    let mut s = s();
    run(&mut s, "page.orientation", json!({"value": "landscape"}));
    assert!(s.doc.last_section.landscape);
    run(&mut s, "page.size", json!({"name": "A4"}));
    assert!((s.doc.last_section.page_w - 841.89).abs() < 0.1);
    run(&mut s, "page.margins", json!({"preset": "narrow"}));
    assert_eq!(s.doc.last_section.margin_left, 36.0);
    run(&mut s, "text.insert", json!({"text": "one"}));
    run(&mut s, "page.break", json!({"kind": "nextPage"}));
    run(&mut s, "text.insert", json!({"text": "two"}));
    assert_eq!(s.doc.sections().len(), 2);
    assert_eq!(s.layout().pages.len(), 2);
    assert!(s.run("page.margins", &json!({"left": 1000.0})).is_err());
}

#[test]
fn track_changes_and_accept() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "original"}));
    run(&mut s, "change.track", json!({"value": true}));
    run(&mut s, "text.insert", json!({"text": " added"}));
    run(&mut s, "select.text", json!({"text": "orig"}));
    s.author = "Someone Else".into();
    run(&mut s, "text.delete", json!({}));
    // Deleted text stays (marked) until accepted.
    assert_eq!(text(&s), "original added");
    let ch = run(&mut s, "change.list", json!({}));
    assert_eq!(ch.as_array().unwrap().len(), 2);
    run(&mut s, "change.acceptAll", json!({}));
    assert_eq!(text(&s), "inal added");
}

/// The author of the revision `rid` points to.
fn author_of(s: &Session, rid: Option<u32>) -> Option<String> {
    rid.and_then(|r| s.doc.revisions.get(r as usize)).map(|r| r.author.clone())
}

#[test]
fn tracked_delete_keeps_another_authors_deletion() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "alpha beta gamma"}));
    run(&mut s, "change.track", json!({"value": true}));
    s.author = "Ana".into();
    run(&mut s, "select.text", json!({"text": "beta"}));
    run(&mut s, "text.delete", json!({}));
    // A second author deletes the whole line, including the text Ana already deleted.
    s.author = "Ben".into();
    run(&mut s, "select.text", json!({"text": "alpha beta gamma"}));
    run(&mut s, "text.delete", json!({}));
    let p = s.doc.para_at(&Pos::body(0, 0)).unwrap();
    let by: Vec<(String, Option<String>)> = p.run_ranges().map(|(r, c)| (p.text[r].to_string(), author_of(&s, c.del))).collect();
    assert!(by.iter().any(|(t, a)| t == "beta" && a.as_deref() == Some("Ana")), "{by:?}");
    assert!(by.iter().any(|(t, a)| t.contains("alpha") && a.as_deref() == Some("Ben")), "{by:?}");
    assert!(by.iter().any(|(t, a)| t.contains("gamma") && a.as_deref() == Some("Ben")), "{by:?}");
}

#[test]
fn tracked_split_gives_the_new_paragraph_mark_to_its_author() {
    // Ben presses Enter at the end of Ana's insertion, and in the middle of it.
    for off in [None, Some(8)] {
        let mut s = s();
        run(&mut s, "text.insert", json!({"text": "alpha"}));
        run(&mut s, "change.track", json!({"value": true}));
        s.author = "Ana".into();
        run(&mut s, "text.insert", json!({"text": " beta"}));
        s.author = "Ben".into();
        if let Some(off) = off {
            run(&mut s, "caret.set", json!({"pos": {"story": "body", "path": [0], "off": off}}));
        }
        run(&mut s, "text.newParagraph", json!({}));
        let head = s.doc.para(StoryRef::Body, &kition_doc_doc::Path::top(0)).unwrap();
        let tail = s.doc.para(StoryRef::Body, &kition_doc_doc::Path::top(1)).unwrap();
        assert_eq!(author_of(&s, head.mark.ins).as_deref(), Some("Ben"), "split at {off:?}: the new paragraph mark is Ben's");
        // The second paragraph ends with the original mark, which nobody inserted (not Ana).
        assert_eq!(tail.mark.ins, None, "split at {off:?}");
        assert_eq!(tail.mark.del, None, "split at {off:?}");
    }
}

fn body_texts(s: &Session) -> Vec<String> {
    s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| p.text.clone()).collect()
}

/// A paragraph split while tracking changes.
fn tracked_split(text: &str) -> Session {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": text}));
    run(&mut s, "file.setAuthor", json!({"name": "Owned Bob"}));
    run(&mut s, "change.track", json!({"value": true}));
    run(&mut s, "caret.set", json!({"pos": {"block": 0, "off": 12}}));
    run(&mut s, "text.newParagraph", json!({}));
    s
}

#[test]
fn reject_all_removes_a_tracked_paragraph_break() {
    // Reject All left the paragraph break typed with Track Changes on.
    for (text, head) in [("Owned ALPHA Owned BETA", "Owned ALPHA "), ("Owned GAMMA Owned DELTA", "Owned GAMMA ")] {
        let mut s = tracked_split(text);
        assert_eq!(body_texts(&s), [head, &text[12..]]);
        // The break is listed as a change, by its author.
        let ch = run(&mut s, "change.list", json!({}));
        assert_eq!(ch.as_array().map(Vec::len), Some(1), "{ch}");
        assert_eq!((ch[0]["kind"].as_str(), ch[0]["author"].as_str()), (Some("insert"), Some("Owned Bob")));
        run(&mut s, "change.rejectAll", json!({}));
        assert_eq!(body_texts(&s), [text]);
        assert!(s.doc.revisions.is_empty());
        let p = s.doc.para_at(&Pos::body(0, 0)).unwrap();
        assert_eq!((p.mark.ins, p.mark.del), (None, None));
        // Undo brings the tracked split back; redo rejects it again.
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(body_texts(&s), [head, &text[12..]]);
        assert!(s.doc.para_at(&Pos::body(0, 0)).unwrap().mark.ins.is_some());
        run(&mut s, "edit.redo", json!({}));
        assert_eq!(body_texts(&s), [text]);
    }
    // Split → Undo → Redo → Reject All (the issue's other variant).
    let mut s = tracked_split("Owned ALPHA Owned BETA");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(body_texts(&s), ["Owned ALPHA Owned BETA"]);
    run(&mut s, "edit.redo", json!({}));
    run(&mut s, "change.rejectAll", json!({}));
    assert_eq!(body_texts(&s), ["Owned ALPHA Owned BETA"]);
}

#[test]
fn accept_all_keeps_a_tracked_paragraph_break() {
    let mut s = tracked_split("Owned ALPHA Owned BETA");
    run(&mut s, "change.acceptAll", json!({}));
    assert_eq!(body_texts(&s), ["Owned ALPHA ", "Owned BETA"]);
    assert!(s.doc.revisions.is_empty());
    assert!(s.doc.body.iter().filter_map(|b| b.as_para()).all(|p| p.mark.ins.is_none() && p.mark.del.is_none()));
    assert_eq!(run(&mut s, "change.list", json!({})), json!([]));
}

#[test]
fn reject_one_paragraph_break_and_rejecting_a_typed_paragraph() {
    // Reject with the caret on the break (Next Change selects it), or at the end of its paragraph.
    let mut s = tracked_split("Owned ALPHA Owned BETA");
    run(&mut s, "caret.set", json!({"pos": {"block": 0, "off": 0}}));
    run(&mut s, "change.next", json!({}));
    run(&mut s, "change.reject", json!({}));
    assert_eq!(body_texts(&s), ["Owned ALPHA Owned BETA"]);
    let mut s = tracked_split("Owned ALPHA Owned BETA");
    run(&mut s, "caret.set", json!({"pos": {"block": 0, "off": 12}}));
    run(&mut s, "change.accept", json!({}));
    assert_eq!(body_texts(&s), ["Owned ALPHA ", "Owned BETA"]);
    assert_eq!(run(&mut s, "change.list", json!({})), json!([]));

    // Enter at the end of a heading, then a typed paragraph: rejecting everything restores
    // the heading alone, with its style.
    let mut s = self::s();
    run(&mut s, "text.insert", json!({"text": "Title"}));
    run(&mut s, "paragraph.style", json!({"style": "Heading1"}));
    run(&mut s, "change.track", json!({"value": true}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "Body"}));
    run(&mut s, "text.newParagraph", json!({}));
    assert_eq!(body_texts(&s), ["Title", "Body", ""]);
    run(&mut s, "change.rejectAll", json!({}));
    assert_eq!(body_texts(&s), ["Title"]);
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().props.style.as_deref(), Some("Heading1"));
}

#[test]
fn tracked_backspace_and_delete_mark_a_paragraph_break_deleted() {
    let two = || {
        let mut s = s();
        run(&mut s, "text.insert", json!({"text": "One"}));
        run(&mut s, "text.newParagraph", json!({}));
        run(&mut s, "text.insert", json!({"text": "Two"}));
        run(&mut s, "change.track", json!({"value": true}));
        s.author = "Ana".into();
        s
    };
    // Backspace at the start of "Two": the mark above is deleted, the caret moves before it.
    for accept in [false, true] {
        let mut s = two();
        run(&mut s, "caret.set", json!({"pos": {"block": 1, "off": 0}}));
        run(&mut s, "text.backspace", json!({}));
        assert_eq!(body_texts(&s), ["One", "Two"], "a tracked deletion stays in the text");
        assert_eq!(s.sel.focus, Pos::body(0, 3));
        assert_eq!(author_of(&s, s.doc.para_at(&Pos::body(0, 0)).unwrap().mark.del).as_deref(), Some("Ana"));
        let ch = run(&mut s, "change.list", json!({}));
        assert_eq!(ch[0]["kind"], "delete", "{ch}");
        if accept {
            run(&mut s, "change.acceptAll", json!({}));
            assert_eq!(body_texts(&s), ["OneTwo"]);
        } else {
            run(&mut s, "change.rejectAll", json!({}));
            assert_eq!(body_texts(&s), ["One", "Two"]);
            assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().mark.del, None);
        }
    }

    // Delete at the end of "One": the same, and the caret steps over the deleted mark.
    let mut s = two();
    run(&mut s, "caret.set", json!({"pos": {"block": 0, "off": 3}}));
    run(&mut s, "text.delete", json!({}));
    assert_eq!(body_texts(&s), ["One", "Two"]);
    assert_eq!(s.sel.focus, Pos::body(1, 0));
    assert!(s.doc.para_at(&Pos::body(0, 0)).unwrap().mark.del.is_some());
    run(&mut s, "change.acceptAll", json!({}));
    assert_eq!(body_texts(&s), ["OneTwo"]);

    // A break the same author inserted is simply removed again.
    let mut s = two();
    run(&mut s, "caret.set", json!({"pos": {"block": 1, "off": 3}}));
    run(&mut s, "text.newParagraph", json!({}));
    assert_eq!(body_texts(&s), ["One", "Two", ""]);
    run(&mut s, "text.backspace", json!({}));
    assert_eq!(body_texts(&s), ["One", "Two"]);
    assert!(s.doc.body.iter().filter_map(|b| b.as_para()).all(|p| p.mark.ins.is_none() && p.mark.del.is_none()));
}

#[test]
fn set_author_names_tracked_changes_headlessly() {
    // A script or agent labels its own edits without any dialog.
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "original"}));
    let r = run(&mut s, "file.setAuthor", json!({"name": "  Claude (copyedit)\n"}));
    assert_eq!(r["name"], "Claude (copyedit)");
    assert!(s.ui_requests.is_empty(), "programmatic calls never open dialogs");
    run(&mut s, "change.track", json!({"value": true}));
    run(&mut s, "text.insert", json!({"text": " added"}));
    run(&mut s, "select.text", json!({"text": "orig"}));
    run(&mut s, "text.delete", json!({}));
    let ch = run(&mut s, "change.list", json!({}));
    let ch = ch.as_array().unwrap();
    assert_eq!(ch.len(), 2);
    assert!(ch.iter().all(|c| c["author"] == "Claude (copyedit)"), "{ch:?}");
    // Comments use the same name.
    run(&mut s, "select.text", json!({"text": "added"}));
    run(&mut s, "comment.new", json!({"text": "Tightened."}));
    assert!(s.doc.comments.values().all(|c| c.author == "Claude (copyedit)" && c.initials == "C("));
}

#[test]
fn set_author_refuses_blank_names_and_clamps_long_ones() {
    let mut s = s();
    for bad in [json!(null), json!({}), json!({"name": 5}), json!({"name": ""}), json!({"name": " \t\n "}), json!({"name": "\u{0}\u{7}"})] {
        assert!(s.run("file.setAuthor", &bad).is_err(), "{bad} should be refused");
        assert_eq!(s.author, "Kition Doc User", "a refused name leaves the author alone");
    }
    run(&mut s, "file.setAuthor", json!({"name": "Ada\u{0}\u{1b} Lovelace"}));
    assert_eq!(s.author, "Ada Lovelace", "control characters can't reach the saved XML");
    run(&mut s, "file.setAuthor", json!({"name": "é".repeat(100_000)}));
    assert_eq!(s.author.chars().count(), cmd::file::MAX_AUTHOR_CHARS);
}

#[test]
fn track_changes_and_set_author_list_their_params() {
    // Scripts that only read the command list must find the explicit, idempotent form.
    let reg = cmd::registry();
    assert!(reg.get("change.track").unwrap().params.contains(r#""value"?: bool"#));
    assert!(reg.get("file.setAuthor").unwrap().params.contains(r#""name": string"#));
    let mut s = s();
    for _ in 0..2 {
        assert_eq!(run(&mut s, "change.track", json!({"value": true}))["value"], true);
    }
}

#[test]
fn no_markup_view_lays_out_the_final_text() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "original"}));
    run(&mut s, "change.track", json!({"value": true}));
    run(&mut s, "select.text", json!({"text": "orig"}));
    run(&mut s, "text.delete", json!({}));
    let gap = |s: &mut Session| {
        let l = s.layout();
        let x = |off| l.caret(&Pos::body(0, off)).map(|c| c.x).unwrap_or(f32::NAN);
        x(4) - x(0)
    };
    assert!(gap(&mut s) > 5.0, "markup shows the deletion");
    run(&mut s, "change.markup", json!({"value": "noMarkup"}));
    assert!(gap(&mut s).abs() < 0.01, "No Markup leaves it out");
    run(&mut s, "change.showMarkup", json!({"value": true}));
    assert!(gap(&mut s) > 5.0);
    run(&mut s, "change.showMarkup", json!({"value": false}));
    assert!(gap(&mut s).abs() < 0.01);
}

#[test]
fn replace_all_is_tracked() {
    let mut s = s();
    let original = "We walked towards the light, then towards home.";
    run(&mut s, "file.setText", json!({"text": original}));
    run(&mut s, "change.track", json!({"value": true}));
    let r = run(&mut s, "edit.replaceAll", json!({"text": "towards", "with": "toward", "matchCase": true}));
    assert_eq!(r["replaced"], 2);
    let ch = run(&mut s, "change.list", json!({}));
    assert!(!ch.as_array().unwrap().is_empty(), "replace all must leave tracked revisions");
    run(&mut s, "change.rejectAll", json!({}));
    assert_eq!(text(&s), original);
}

#[test]
fn comments() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Some text here"}));
    run(&mut s, "select.text", json!({"text": "text"}));
    let r = run(&mut s, "comment.new", json!({"text": "Nice"}));
    let id = r["id"].as_u64().unwrap();
    let l = run(&mut s, "comment.pane", json!({}));
    assert_eq!(l[0]["text"], "Nice");
    assert_eq!(text(&s), "Some text here");
    // Editing the text is one undo step; replies hang off their comment.
    run(&mut s, "comment.edit", json!({"id": id, "text": "Nicer\nTwo lines"}));
    assert_eq!(run(&mut s, "comment.pane", json!({}))[0]["text"], "Nicer\nTwo lines");
    assert!(s.undo());
    assert_eq!(run(&mut s, "comment.pane", json!({}))[0]["text"], "Nice");
    assert!(s.run("comment.edit", &json!({"id": 999, "text": "x"})).is_err());
    let r = run(&mut s, "comment.replyCurrent", json!({"id": id, "text": "Agreed"}));
    assert_eq!(s.doc.comments.get(&(r["id"].as_u64().unwrap() as u32)).unwrap().parent, Some(id as u32));
    run(&mut s, "comment.deleteCurrent", json!({"id": id}));
    assert!(s.doc.comments.is_empty());
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().objects.len(), 0);
}

#[test]
fn toc_and_fields() {
    let mut s = Session::new(crate::sample::report());
    run(&mut s, "caret.docStart", json!({}));
    run(&mut s, "toc.insert", json!({}));
    let t = text(&s);
    assert!(t.contains("Contents"), "{t}");
    assert!(t.contains("Summary\t1"), "{t}");
    run(&mut s, "toc.update", json!({}));
    assert_eq!(s.doc.plain_text(StoryRef::Body).matches("Summary\t").count(), 1, "one TOC entry after update");
}

#[test]
fn toc_page_numbers_follow_headings() {
    let mut s = s();
    run(&mut s, "toc.insert", json!({}));
    run(&mut s, "caret.docEnd", json!({}));
    for name in ["Alpha", "Bravo", "Charlie"] {
        run(&mut s, "text.pageBreak", json!({}));
        run(&mut s, "text.insert", json!({"text": name}));
        run(&mut s, "paragraph.heading1", json!({}));
        run(&mut s, "text.newParagraph", json!({}));
    }
    run(&mut s, "toc.update", json!({}));
    let t = text(&s);
    for (name, page) in [("Alpha", 2), ("Bravo", 3), ("Charlie", 4)] {
        assert!(t.contains(&format!("{name}\t{page}")), "{name} should be on page {page}: {t}");
    }
}

/// A heading above the TOC keeps its index when the entries go in below it, so its page is read
/// where it is, not `entries` blocks further down.
#[test]
fn toc_page_number_for_heading_above_toc() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Preface"}));
    run(&mut s, "paragraph.heading1", json!({}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.pageBreak", json!({}));
    run(&mut s, "toc.insert", json!({}));
    run(&mut s, "caret.docEnd", json!({}));
    for name in ["Alpha", "Bravo"] {
        run(&mut s, "text.pageBreak", json!({}));
        run(&mut s, "text.insert", json!({"text": name}));
        run(&mut s, "paragraph.heading1", json!({}));
        run(&mut s, "text.newParagraph", json!({}));
    }
    run(&mut s, "toc.update", json!({}));
    let t = text(&s);
    for (name, page) in [("Preface", 1), ("Alpha", 3), ("Bravo", 4)] {
        assert!(t.contains(&format!("{name}\t{page}")), "{name} should be on page {page}: {t:?}");
    }
}

#[test]
fn failed_command_leaves_document_unchanged() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "keep"}));
    let before = text(&s);
    assert!(s.run("format.fontSize", &json!({"size": -3})).is_err());
    assert!(s.run("no.such", &json!({})).is_err());
    assert!(s.run("text.insert", &json!({})).is_err());
    assert_eq!(text(&s), before);
}

#[test]
fn hostile_params_never_panic() {
    let reg = cmd::registry();
    let junk = [
        json!(null),
        json!({}),
        json!({"text": 5, "size": "x", "path": [], "pos": {"block": 9999, "off": 99999}}),
        json!({"value": -1e308, "rows": 1e9}),
    ];
    for spec in reg.all() {
        // These reach outside the session (files). Read Aloud (`readAloud.open`, `readAloud.*`)
        // is fuzzed too: under `cfg(test)` its backend is the silent `Hold`, asserted below.
        if spec.id.starts_with("file.") || spec.id == "image.insert" || spec.id == "text.insertFromFile" {
            continue;
        }
        for j in &junk {
            let mut s = Session::new(crate::sample::sample_document());
            assert_ne!(s.read_aloud.backend, crate::speech::Backend::System, "tests must never start real speech");
            let _ = s.run(spec.id, j);
            s.clamp_selection();
            let _ = s.layout();
        }
    }
}

#[test]
fn inspect_reports_structure() {
    let mut s = Session::new(crate::sample::sample_document());
    let r = run(&mut s, "file.inspect", json!({}));
    assert!(r["pages"].as_u64().unwrap() >= 1);
    assert!(r["blocks"].as_array().unwrap().iter().any(|b| b["type"] == "table"));
    let f = run(&mut s, "format.state", json!({}));
    assert_eq!(f["styleName"], "Title");
}

#[test]
fn caret_navigation() {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "first line\nsecond line"}));
    run(&mut s, "caret.down", json!({}));
    assert_eq!(s.sel.focus.path.last(), 1);
    run(&mut s, "caret.end", json!({}));
    assert_eq!(s.sel.focus.off, 11);
    run(&mut s, "caret.up", json!({"extend": true}));
    assert_eq!(s.sel.focus.path.last(), 0);
    assert!(!s.sel.is_collapsed());
    run(&mut s, "caret.wordLeft", json!({}));
    run(&mut s, "caret.docEnd", json!({}));
    assert_eq!(s.sel.focus, Pos::body(1, 11));
    run(&mut s, "caret.left", json!({}));
    assert_eq!(s.sel.focus.off, 10);
}

#[test]
fn text_box_insert_puts_the_caret_inside() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Before "}));
    let id = run(&mut s, "textBox.insert", json!({}))["story"].as_u64().unwrap() as u32;
    let boxed = StoryRef::Part(id);
    assert_eq!(s.sel.focus.story, boxed);
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    assert_eq!(s.doc.plain_text(boxed), "Hello");
    assert!(!text(&s).contains("Hello"), "{}", text(&s));
    // Leaving the box lands just after it in the body.
    let anchor = s.doc.text_box_anchor(id).unwrap();
    assert_eq!(anchor, Pos::body(0, "Before \u{FFFC}".len()));
    run(&mut s, "caret.set", json!({"pos": anchor}));
    run(&mut s, "text.insert", json!({"text": " after"}));
    assert_eq!(s.doc.para(StoryRef::Body, &kition_doc_doc::Path::top(0)).map(|p| p.text.as_str()), Some("Before \u{FFFC} after"));
    // Undo goes back through the typing to before the box, in the body.
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.sel.focus.story, StoryRef::Body);
    assert_eq!(text(&s), "Before ");
    // Given text: the caret ends after it.
    let id = run(&mut s, "textBox.insert", json!({"text": "Note"}))["story"].as_u64().unwrap() as u32;
    assert_eq!(s.sel.focus, Pos { story: StoryRef::Part(id), path: kition_doc_doc::Path::top(0), off: 4 });
}

#[test]
fn text_box_in_a_table_cell_shows_its_text() {
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 1, "cols": 1}));
    assert!(s.sel.focus.path.0.len() > 1);
    let id = run(&mut s, "textBox.insert", json!({}))["story"].as_u64().unwrap() as u32;
    // The caret goes into the box, and typing shows there (it has a caret position).
    assert_eq!(s.sel.focus.story, StoryRef::Part(id));
    run(&mut s, "text.insert", json!({"text": "In a cell"}));
    let caret = s.layout().caret(&s.sel.focus).expect("laid out");
    let cell = s.layout().find_object(0, |o| o.text_box == Some(id)).unwrap();
    assert!(cell.rect.contains(kition_doc_geom::Point::new(caret.x, caret.top)), "{caret:?} in {:?}", cell.rect);
    // No text box inside a text box.
    assert!(s.run("textBox.insert", &json!({})).is_err());
}

#[test]
fn copy_paste_text_box_makes_an_independent_copy() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "A"}));
    let id = run(&mut s, "textBox.insert", json!({"text": "Original"}))["story"].as_u64().unwrap() as u32;
    // Select "A" and the box in the body, copy, paste at the end.
    run(&mut s, "select.range", json!({"anchor": Pos::body(0, 0), "focus": Pos::body(0, 1 + 3)}));
    run(&mut s, "edit.copy", json!({}));
    run(&mut s, "caret.set", json!({"pos": Pos::body(0, 4)}));
    let parts = s.doc.parts.len();
    run(&mut s, "edit.paste", json!({}));
    assert_eq!(s.doc.parts.len(), parts + 1);
    let stories: Vec<u32> = s
        .doc
        .para(StoryRef::Body, &kition_doc_doc::Path::top(0))
        .unwrap()
        .objects
        .iter()
        .filter_map(|o| if let kition_doc_doc::InlineObject::Shape { story, .. } = o { *story } else { None })
        .collect();
    assert_eq!(stories.len(), 2);
    assert_eq!(stories[0], id);
    let copy = stories[1];
    assert_ne!(copy, id);
    // Typing in the copy leaves the original alone.
    run(&mut s, "caret.set", json!({"pos": Pos { story: StoryRef::Part(copy), path: kition_doc_doc::Path::top(0), off: 0 }}));
    run(&mut s, "text.insert", json!({"text": "Copy of "}));
    assert_eq!(s.doc.plain_text(StoryRef::Part(copy)), "Copy of Original");
    assert_eq!(s.doc.plain_text(StoryRef::Part(id)), "Original");
    // Undo the typing and the paste: the copy's story goes too.
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.parts.len(), parts);
}

#[test]
fn word_count_includes_text_boxes_by_default() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "one two "}));
    run(&mut s, "textBox.insert", json!({"text": "three four five"}));
    run(&mut s, "caret.set", json!({"pos": Pos::body(0, 0)}));
    assert_eq!(s.word_count(), 5);
    let v = run(&mut s, "file.wordCount", json!({}));
    assert_eq!((v["words"].as_u64(), v["includeTextBoxes"].as_bool()), (Some(5), Some(true)));
    assert_eq!(v["paragraphs"].as_u64(), Some(2));
    // Turned off: body only, and it sticks.
    let v = run(&mut s, "file.wordCount", json!({"includeTextBoxes": false}));
    assert_eq!(v["words"].as_u64(), Some(2));
    assert_eq!(s.word_count(), 2);
    assert_eq!(run(&mut s, "file.wordCount", json!({}))["words"].as_u64(), Some(2));
    // Deleting the box drops its words (its story stays behind, unused).
    run(&mut s, "file.wordCount", json!({"includeTextBoxes": true}));
    run(&mut s, "select.range", json!({"anchor": Pos::body(0, 8), "focus": Pos::body(0, 8 + 3)}));
    run(&mut s, "text.delete", json!({}));
    assert_eq!(s.word_count(), 2);
}

/// The laid-out area of the only text box.
fn box_area(s: &mut Session) -> kition_doc_layout::hit::ObjectHit {
    let l = s.layout();
    l.find_object(0, |o| o.text_box.is_some()).unwrap()
}

fn select_box(s: &mut Session) {
    let o = box_area(s);
    let end = Pos { off: o.off + 3, ..o.pos() };
    run(s, "select.range", json!({"anchor": o.pos(), "focus": end}));
}

#[test]
fn arrange_bounds_resizes_and_moves_objects() {
    use kition_doc_doc::para::{Anchor, Wrap};
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Some text "}));
    run(&mut s, "textBox.insert", json!({"text": "Box"}));
    select_box(&mut s);
    // Resize: stays inline, keeps a text box's minimum.
    run(&mut s, "arrange.bounds", json!({"width": 200, "height": 5}));
    let o = box_area(&mut s);
    assert_eq!((o.rect.w, o.rect.h, o.wrap), (200.0, 18.0, Wrap::Inline));
    assert!(s.run("arrange.nudge", &json!({"dx": 5})).is_err(), "inline objects don't nudge");
    // Move: floats exactly where it was dropped, still selected.
    let r = run(&mut s, "arrange.bounds", json!({"page": 0, "x": 300, "y": 400}));
    assert_eq!(r["object"]["float"]["wrap"], "square");
    let o = box_area(&mut s);
    assert_eq!((o.page, o.rect.x, o.rect.y), (0, 300.0, 400.0));
    assert_eq!(s.doc.plain_text(StoryRef::Part(o.text_box.unwrap())), "Box");
    // Moving it again anchors it to the paragraph under its top edge, relative to that
    // paragraph (it moves with the text), landing exactly where dropped.
    let before = box_area(&mut s).rect;
    select_box(&mut s);
    run(&mut s, "arrange.bounds", json!({"x": before.x + 30.0, "y": before.y - 5.0}));
    let o = box_area(&mut s);
    assert!((o.rect.x - before.x - 30.0).abs() < 0.01 && (o.rect.y - before.y + 5.0).abs() < 0.01, "{:?} → {:?}", before, o.rect);
    let p = s.doc.para(StoryRef::Body, &o.path).unwrap();
    let Some(kition_doc_doc::InlineObject::Shape { float, .. }) = p.object_at(o.off) else { panic!() };
    assert_eq!((float.h_rel, float.v_rel), (Anchor::Column, Anchor::Paragraph));
    assert!((float.x - (o.rect.x - o.origin.x)).abs() < 0.01 && (float.y - (o.rect.y - o.origin.y)).abs() < 0.01);
    // Nudge.
    run(&mut s, "arrange.nudge", json!({"dx": 6, "dy": -1}));
    let n = box_area(&mut s).rect;
    assert!((n.x - o.rect.x - 6.0).abs() < 0.01 && (n.y - o.rect.y + 1.0).abs() < 0.01);
    // Hostile values stay on the page.
    run(&mut s, "arrange.bounds", json!({"x": -1e9, "y": 1e9, "width": 1e9}));
    let o = box_area(&mut s);
    assert!(o.rect.right() >= 12.0 && o.rect.y <= 792.0 && o.rect.w <= 4000.0, "{:?}", o.rect);
    // One undo per change.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(box_area(&mut s).rect, n);
}

#[test]
fn nudging_an_aligned_object_moves_it_from_where_it_is() {
    use kition_doc_doc::para::{Anchor, FloatAlign};
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Some text "}));
    run(&mut s, "textBox.insert", json!({"text": "Box"}));
    select_box(&mut s);
    run(&mut s, "arrange.position", json!({"preset": "topLeft"}));
    // As a .docx can place it: centred on the page, at the bottom margin.
    let o = box_area(&mut s);
    let p = s.doc.para_mut(StoryRef::Body, &o.path).unwrap();
    if let Some(kition_doc_doc::InlineObject::Shape { float, .. }) = p.object_at_mut(o.off) {
        (float.h_rel, float.h_align, float.v_rel, float.v_align) =
            (Anchor::Page, Some(FloatAlign::Center), Anchor::BottomMargin, Some(FloatAlign::Start));
    }
    p.touch();
    s.touch();
    let before = box_area(&mut s).rect;
    select_box(&mut s);
    run(&mut s, "arrange.nudge", json!({"dx": 6, "dy": -2}));
    let o = box_area(&mut s);
    assert!((o.rect.x - before.x - 6.0).abs() < 0.01 && (o.rect.y - before.y + 2.0).abs() < 0.01, "{before:?} → {:?}", o.rect);
    let p = s.doc.para(StoryRef::Body, &o.path).unwrap();
    let Some(kition_doc_doc::InlineObject::Shape { float, .. }) = p.object_at(o.off) else { panic!() };
    assert_eq!((float.h_align, float.v_align), (None, None));
    // Dragging it clears alignment too, and it lands where dropped.
    select_box(&mut s);
    run(&mut s, "arrange.bounds", json!({"x": 100, "y": 150}));
    let o = box_area(&mut s);
    assert_eq!((o.rect.x, o.rect.y), (100.0, 150.0));
}

#[test]
fn arrange_bounds_moves_an_object_to_another_page() {
    let mut s = s();
    for i in 0..60 {
        run(&mut s, "text.insert", json!({"text": format!("Paragraph {i} of filler text.")}));
        run(&mut s, "text.newParagraph", json!({}));
    }
    run(&mut s, "caret.set", json!({"pos": Pos::body(0, 0)}));
    run(&mut s, "textBox.insert", json!({"text": "Traveller"}));
    select_box(&mut s);
    assert!(s.layout().pages.len() >= 2);
    run(&mut s, "arrange.bounds", json!({"page": 1, "x": 100, "y": 200}));
    let o = box_area(&mut s);
    assert_eq!((o.page, o.rect.x, o.rect.y), (1, 100.0, 200.0));
    assert!(o.path.0[0] > 0, "anchored to text on page 2: {:?}", o.path);
    assert_eq!(s.doc.plain_text(StoryRef::Part(o.text_box.unwrap())), "Traveller");
    // The selection follows the object.
    assert_eq!(s.sel.anchor, o.pos());
    assert!(s.run("arrange.bounds", &json!({"page": 99, "x": 1, "y": 1})).is_err());
}

#[test]
fn moved_text_box_makes_the_text_it_lands_on_wrap() {
    let mut s = s();
    for i in 0..6 {
        run(
            &mut s,
            "text.insert",
            json!({"text": format!("Paragraph {i}: the quick brown fox jumps over the lazy dog, again and again, across the page.")}),
        );
        run(&mut s, "text.newParagraph", json!({}));
    }
    run(&mut s, "caret.set", json!({"pos": Pos::body(4, 0)}));
    run(&mut s, "textBox.insert", json!({"text": "Box"}));
    select_box(&mut s);
    // Drop it over paragraph 1, right half of the page: well above its own paragraph (4).
    let l = s.layout();
    let p1_top = kition_doc_layout::hit::page_lines(&l.pages[0], StoryRef::Body).iter().find(|ln| ln.path.0 == [1] && ln.li == 0).unwrap().top;
    run(&mut s, "arrange.bounds", json!({"x": 300, "y": p1_top + 2.0}));
    let o = box_area(&mut s);
    assert_eq!(o.path.0, vec![1], "anchored to the paragraph it was dropped on");
    assert_eq!((o.rect.x, o.rect.y), (300.0, p1_top + 2.0));
    // Lines beside the box stop short of it; lines below it use the full width again.
    let l = s.layout();
    let lines = kition_doc_layout::hit::page_lines(&l.pages[0], StoryRef::Body);
    let beside: Vec<_> = lines.iter().filter(|ln| ln.bottom > o.rect.y && ln.top < o.rect.bottom()).collect();
    assert!(!beside.is_empty());
    assert!(beside.iter().all(|ln| ln.right <= o.rect.x + 0.5 || ln.left >= o.rect.right() - 0.5), "text overlaps the box");
    assert!(lines.iter().any(|ln| ln.top > o.rect.bottom() + 10.0 && ln.right > o.rect.right()));
    // Its old paragraph no longer holds it.
    assert!(s.doc.para(StoryRef::Body, &kition_doc_doc::Path::top(4)).unwrap().objects.is_empty());
}

#[test]
fn deleted_text_box_takes_its_text_with_it_until_undo() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "A"}));
    let id = run(&mut s, "textBox.insert", json!({"text": "Inside"}))["story"].as_u64().unwrap() as u32;
    let box_sel = json!({"anchor": Pos::body(0, 1), "focus": Pos::body(0, 1 + 3)});
    run(&mut s, "select.range", box_sel.clone());
    run(&mut s, "text.delete", json!({}));
    assert!(!s.doc.parts.contains_key(&id), "the deleted box's text is gone");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.plain_text(StoryRef::Part(id)), "Inside", "undo brings it back");
    // Cut and paste: one box, one story, same text.
    run(&mut s, "select.range", box_sel);
    run(&mut s, "edit.cut", json!({}));
    assert!(s.doc.parts.is_empty());
    run(&mut s, "edit.paste", json!({}));
    assert_eq!(s.doc.parts.len(), 1);
    let (_, part) = s.doc.parts.iter().next().unwrap();
    assert_eq!(part.kind, kition_doc_doc::PartKind::TextBox);
    assert_eq!(s.doc.word_count_including_notes(), 2);
}

#[test]
fn word_count_with_an_object_selected_counts_the_document() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "one two "}));
    run(&mut s, "textBox.insert", json!({"text": "three"}));
    run(&mut s, "select.range", json!({"anchor": Pos::body(0, 8), "focus": Pos::body(0, 8 + 3)}));
    assert_eq!(run(&mut s, "file.wordCount", json!({}))["words"].as_u64(), Some(3));
    // A real text selection still counts just itself.
    run(&mut s, "select.range", json!({"anchor": Pos::body(0, 0), "focus": Pos::body(0, 3)}));
    assert_eq!(run(&mut s, "file.wordCount", json!({}))["words"].as_u64(), Some(1));
}

#[test]
fn custom_properties_set_read_remove_and_undo() {
    let mut s = s();
    let r = run(&mut s, "file.properties", json!({"custom": {"ZOTERO_PREF_1": "<data/>", "Status": "draft"}}));
    let custom = r["custom"].as_array().cloned().unwrap_or_default();
    assert!(custom.iter().any(|p| p["name"] == "Status" && p["value"] == "draft" && p["kind"] == "lpwstr"));
    assert_eq!(custom.len(), 2);
    assert_eq!(s.doc.custom_prop("zotero_pref_1"), Some("<data/>"));
    let r = run(&mut s, "file.properties", json!({"custom": {"status": null}}));
    assert_eq!(r["custom"].as_array().map(|a| a.len()), Some(1));
    assert!(s.run("file.properties", &json!({"custom": "x"})).is_err());
    assert!(s.run("file.properties", &json!({"custom": {"": "x"}})).is_err());
    assert!(s.run("file.properties", &json!({"custom": {"n": 3}})).is_err());
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.custom_prop("Status"), Some("draft"));
}

/// `document_id` tells an edit from a replacement (the UI drops a "Save changes?" prompt
/// about a document that has been replaced).
#[test]
fn document_id_changes_only_when_the_document_is_replaced() {
    let mut s = s();
    let first = s.document_id();
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    run(&mut s, "format.bold", json!({}));
    assert_eq!(s.document_id(), first);
    run(&mut s, "file.new", json!({"template": "letter"}));
    let second = s.document_id();
    assert_ne!(second, first);
    run(&mut s, "file.new", json!({}));
    assert_ne!(s.document_id(), second);
}

/// Envelopes, Labels and Finish & Merge make a new, untitled document, as Word does, and like New
/// its undo history starts afresh. Undo/Redo across the swap put the wrong document under a file
/// (Undo, Save As, Redo, Save wrote the envelope over the saved file), so it isn't offered; the UI
/// asks to save the replaced document first.
#[test]
fn mailings_results_are_new_untitled_documents() {
    let dir = std::env::temp_dir().join(format!("kition-doc-engine-mailings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let original = dir.join("letter.docx");
    let saved_as = dir.join("saved-as.docx");
    let on_file = |p: &std::path::Path| crate::io::open_path(p).unwrap().plain_text(StoryRef::Body);
    for id in ["mailMerge.envelopes", "mailMerge.labels", "mailMerge.finish"] {
        let mut s = s();
        run(&mut s, "mailMerge.recipients", json!({"csv": "First Name\nAda\nAlan"}));
        run(&mut s, "text.insert", json!({"text": "Dear "}));
        run(&mut s, "mailMerge.insertField", json!({"field": "First Name"}));
        run(&mut s, "file.save", json!({"path": original.to_string_lossy()}));
        let on_disk = std::fs::read(&original).unwrap();
        run(&mut s, "text.insert", json!({"text": ", unsaved"}));
        let before = text(&s);
        let document = s.document_id();

        run(&mut s, id, json!({}));
        let result = text(&s);
        assert_ne!(result, before, "{id}: the result replaced the document");
        assert_ne!(s.document_id(), document, "{id}: a different document");
        assert_eq!(s.path, None, "{id}: the result is untitled");
        assert_eq!(run(&mut s, "file.save", json!({}))["saved"], false, "{id}: Save asks where (Save As)");
        assert_eq!(std::fs::read(&original).unwrap(), on_disk, "{id}: the original file is untouched");

        // Undo doesn't bring the old document back under the new one's identity; Redo can't
        // put the result back under a file saved in between.
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(text(&s), result, "{id}: history starts afresh, like New");
        run(&mut s, "file.saveAs", json!({"path": saved_as.to_string_lossy()}));
        let written = on_file(&saved_as);
        run(&mut s, "edit.redo", json!({}));
        assert_eq!(text(&s), result, "{id}: nothing to redo");
        run(&mut s, "file.save", json!({}));
        assert_eq!(on_file(&saved_as), written, "{id}: the saved file still holds what was saved");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn html_pictures_load_relative_to_the_file() {
    // `<img src="logo.png">` beside an HTML file is embedded when it is opened.
    let dir = std::env::temp_dir().join(format!("kition-doc-html-img-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("img")).unwrap();
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(4, 2, image::Rgba([10, 20, 30, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    std::fs::write(dir.join("img/my logo.png"), &png).unwrap();
    std::fs::write(dir.join("secret.txt"), b"not a picture").unwrap();
    let html = r#"<p><img src="img/my%20logo.png" alt="Logo"></p><p><img src="secret.txt" alt="T"></p><p><img src="http://example.com/x.png" alt="Web"></p>"#;
    std::fs::write(dir.join("page.html"), html).unwrap();
    let doc = crate::io::open_path(&dir.join("page.html"));
    let _ = std::fs::remove_dir_all(&dir);
    let doc = doc.unwrap();
    assert_eq!(doc.media.len(), 1);
    let text = doc.plain_text(StoryRef::Body);
    assert!(!text.contains("Logo") && text.contains('T') && text.contains("Web"), "{text:?}");
}

/// A fresh, empty scratch folder for one test.
#[cfg(not(target_arch = "wasm32"))]
fn scratch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-doc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(not(target_arch = "wasm32"))]
fn tiny_png() -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([200, 30, 30, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn html_pictures_outside_the_folder_are_not_loaded() {
    // `..` must not reach files outside the HTML file's folder.
    let root = scratch_dir("html-img-escape");
    std::fs::create_dir_all(root.join("site/img")).unwrap();
    std::fs::write(root.join("outside.png"), tiny_png()).unwrap();
    std::fs::write(root.join("site/inside.png"), tiny_png()).unwrap();
    let html = r#"<p><img src="../outside.png" alt="Up"></p><p><img src="img/../../outside.png" alt="Sneak"></p><p><img src="img/../inside.png" alt="In"></p>"#;
    std::fs::write(root.join("site/page.html"), html).unwrap();
    let doc = crate::io::open_path(&root.join("site/page.html"));
    let _ = std::fs::remove_dir_all(&root);
    let doc = doc.unwrap();
    assert_eq!(doc.media.len(), 1, "only the picture inside the folder loads");
    let text = doc.plain_text(StoryRef::Body);
    assert!(text.contains("Up") && text.contains("Sneak") && !text.contains("In"), "{text:?}");
}

#[cfg(unix)]
#[test]
fn html_pictures_through_a_symlink_out_of_the_folder_are_not_loaded() {
    // A symlink inside the folder must not lead outside it.
    let root = scratch_dir("html-img-symlink");
    std::fs::create_dir_all(root.join("site")).unwrap();
    std::fs::write(root.join("outside.png"), tiny_png()).unwrap();
    std::os::unix::fs::symlink(root.join("outside.png"), root.join("site/link.png")).unwrap();
    std::fs::write(root.join("site/page.html"), r#"<p><img src="link.png" alt="Link"></p>"#).unwrap();
    let doc = crate::io::open_path(&root.join("site/page.html"));
    let _ = std::fs::remove_dir_all(&root);
    let doc = doc.unwrap();
    assert!(doc.media.is_empty());
    assert!(doc.plain_text(StoryRef::Body).contains("Link"));
}

#[cfg(unix)]
#[test]
fn html_picture_that_is_a_named_pipe_is_skipped_without_blocking() {
    // Opening a FIFO blocks until a writer appears; it must never be opened.
    let dir = scratch_dir("html-img-fifo");
    let fifo = dir.join("pipe.png");
    let made = std::process::Command::new("mkfifo").arg(&fifo).status();
    if !made.is_ok_and(|s| s.success()) {
        let _ = std::fs::remove_dir_all(&dir);
        eprintln!("mkfifo unavailable; skipping");
        return;
    }
    std::fs::write(dir.join("page.html"), r#"<p><img src="pipe.png" alt="Pipe"></p>"#).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let page = dir.join("page.html");
    std::thread::spawn(move || {
        let _ = tx.send(crate::io::open_path(&page));
    });
    let got = rx.recv_timeout(std::time::Duration::from_secs(10));
    if got.is_err() {
        // Unblock the stuck reader so the thread ends, then fail.
        let _ = std::fs::OpenOptions::new().write(true).open(&fifo);
    }
    let _ = std::fs::remove_dir_all(&dir);
    let doc = got.expect("opening the HTML file blocked on a named pipe").unwrap();
    assert!(doc.media.is_empty());
    assert!(doc.plain_text(StoryRef::Body).contains("Pipe"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn html_picture_size_limit_holds_whatever_length_the_file_reports() {
    use crate::io::LocalImages;
    let dir = scratch_dir("html-img-limit");
    std::fs::write(dir.join("ten.bin"), [7u8; 10]).unwrap();
    std::fs::write(dir.join("eleven.bin"), [7u8; 11]).unwrap();
    let images = LocalImages::with_limits(&dir, 10, 1000);
    let ten = images.load("ten.bin").map(|d| d.len());
    let eleven = images.load("eleven.bin");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ten, Some(10));
    assert!(eleven.is_none());
    #[cfg(unix)]
    {
        // A device reports a length of 0 and never ends: it is not a regular file, and the read is
        // bounded anyway.
        let dev = LocalImages::with_limits(std::path::Path::new("/dev"), 1 << 20, 1 << 20);
        assert!(dev.load("zero").is_none());
        assert!(dev.load("null").is_none());
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn html_pictures_share_one_read_and_a_per_document_budget() {
    // A picture referenced many times is read once, and every reference counts
    // against the document's budget so a large picture can't be multiplied without bound.
    use crate::io::LocalImages;
    let dir = scratch_dir("html-img-budget");
    let png = tiny_png();
    let n = png.len() as u64;
    std::fs::write(dir.join("a.png"), &png).unwrap();
    let images = LocalImages::with_limits(&dir, 1 << 20, 3 * n);
    let first = images.load("a.png").unwrap();
    std::fs::write(dir.join("a.png"), b"changed on disk").unwrap();
    let second = images.load("./a.png").unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &second), "the second reference reuses the first read");
    assert_eq!(*second, png);
    assert!(images.load("a.png").is_some());
    assert!(images.load("a.png").is_none(), "budget spent");
    // Files over the per-picture limit still cost what was read.
    std::fs::write(dir.join("big.bin"), vec![1u8; 100]).unwrap();
    std::fs::write(dir.join("small.bin"), [1u8; 5]).unwrap();
    let images = LocalImages::with_limits(&dir, 10, 25);
    assert!(images.load("big.bin").is_none());
    assert!(images.load("big.bin").is_none());
    let small = images.load("small.bin");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(small.is_none(), "two oversized reads used up the budget");
}

fn para_style(s: &Session, i: usize) -> Option<String> {
    s.doc.para_at(&Pos::body(i, 0)).and_then(|p| p.props.style.clone())
}

#[test]
fn styles_apply_by_name() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Title text"}));
    run(&mut s, "style.apply", json!({"style": "Heading 1"}));
    assert_eq!(para_style(&s, 0).as_deref(), Some("Heading1"));
    assert_eq!(s.undo_label(), Some("Apply Style"));
    assert!(s.dirty);
}

#[test]
fn styles_apply_pane_is_not_an_edit() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    s.dirty = false;
    let undo = s.undo_labels();
    s.view.styles_pane = false;
    assert_eq!(run(&mut s, "style.apply", json!({})), json!({"pane": true}));
    assert!(s.view.styles_pane);
    // Shows, never toggles.
    run(&mut s, "style.apply", json!({}));
    assert!(s.view.styles_pane);
    assert_eq!(s.undo_labels(), undo);
    assert!(!s.dirty);
    // Works on a read-only document.
    s.doc.settings.protection = Some("readOnly".into());
    s.view.styles_pane = false;
    run(&mut s, "style.apply", json!({}));
    assert!(s.view.styles_pane);
}

#[test]
fn styles_apply_rejects_bad_style_params() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    let undo = s.undo_labels();
    let before = text(&s);
    for bad in [json!(5), json!(null), json!([]), json!({}), json!(""), json!("   "), json!(true)] {
        let r = s.run("style.apply", &json!({"style": bad}));
        assert!(matches!(r, Err(crate::CmdError::Params(_))), "{bad}: {r:?}");
    }
    assert_eq!(s.undo_labels(), undo);
    assert_eq!(text(&s), before);
    assert_eq!(para_style(&s, 0), None);
}

#[test]
fn styles_apply_unknown_style_changes_nothing() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    let undo = s.undo_labels();
    let doc = s.doc.clone();
    assert!(s.run("style.apply", &json!({"style": "No Such Style"})).is_err());
    assert_eq!(s.undo_labels(), undo);
    assert_eq!(s.doc, doc);
    assert!(!s.can_redo());
}

#[test]
fn styles_apply_undo_redo() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    run(&mut s, "style.apply", json!({"style": "Heading 2"}));
    assert_eq!(para_style(&s, 0).as_deref(), Some("Heading2"));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(para_style(&s, 0), None);
    assert_eq!(text(&s), "Hello");
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(para_style(&s, 0).as_deref(), Some("Heading2"));
}

#[test]
fn styles_apply_character_style() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "make this strong"}));
    run(&mut s, "select.text", json!({"text": "this"}));
    run(&mut s, "style.apply", json!({"style": "Strong"}));
    let p = s.doc.para_at(&Pos::body(0, 0)).unwrap();
    assert_eq!(p.props_of_char(6).style.as_deref(), Some("Strong"));
    assert_eq!(p.props_of_char(0).style, None);
    // A character style leaves the paragraph style alone.
    assert_eq!(p.props.style, None);
}

#[test]
fn styles_apply_multi_paragraph_selection() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "One"}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "Two"}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "Three"}));
    run(&mut s, "edit.selectAll", json!({}));
    run(&mut s, "style.apply", json!({"style": "Quote"}));
    for i in 0..3 {
        assert_eq!(para_style(&s, i).as_deref(), Some("Quote"), "paragraph {i}");
    }
    // One undo step for the whole selection.
    run(&mut s, "edit.undo", json!({}));
    for i in 0..3 {
        assert_eq!(para_style(&s, i), None, "paragraph {i}");
    }
}

#[test]
fn styles_apply_rejected_on_protected_document() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    s.doc.settings.protection = Some("readOnly".into());
    let undo = s.undo_labels();
    let r = s.run("style.apply", &json!({"style": "Heading 1"}));
    assert!(matches!(r, Err(crate::CmdError::Disabled(_))), "{r:?}");
    assert_eq!(para_style(&s, 0), None);
    assert_eq!(s.undo_labels(), undo);
}

fn column_doc() -> Session {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "abcdef\nabcdef\nabcdef"}));
    s
}

fn block(s: &mut Session) {
    let a = serde_json::to_value(Pos::body(0, 2)).unwrap();
    let f = serde_json::to_value(Pos::body(2, 4)).unwrap();
    let r = run(s, "select.column", json!({"anchor": a, "focus": f}));
    assert_eq!(r["rows"], json!(["cd", "cd", "cd"]));
}

#[test]
fn column_selection_copies_the_block() {
    let mut s = column_doc();
    block(&mut s);
    let r = run(&mut s, "edit.copy", json!({}));
    assert_eq!(r["text"], "cd\ncd\ncd");
    assert_eq!(s.clipboard.as_ref().map(|f| f.blocks.len()), Some(3));
    // A caret move drops the block; pasting the copy gives three paragraphs.
    run(&mut s, "caret.docEnd", json!({}));
    assert!(s.column_segments().is_none());
    s.autocorrect_on = false;
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "edit.paste", json!({}));
    assert_eq!(text(&s), "abcdef\nabcdef\nabcdef\ncd\ncd\ncd");
}

#[test]
fn column_selection_deletes_only_the_block() {
    let mut s = column_doc();
    block(&mut s);
    run(&mut s, "text.delete", json!({}));
    assert_eq!(text(&s), "abef\nabef\nabef");
    assert_eq!(s.sel, crate::Selection::caret(Pos::body(0, 2)));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s), "abcdef\nabcdef\nabcdef");
    // Cut copies and deletes; typing replaces the block.
    block(&mut s);
    let r = run(&mut s, "edit.cut", json!({}));
    assert_eq!(r["text"], "cd\ncd\ncd");
    assert_eq!(text(&s), "abef\nabef\nabef");
    run(&mut s, "edit.undo", json!({}));
    block(&mut s);
    run(&mut s, "text.insert", json!({"text": "X"}));
    assert_eq!(text(&s), "abXef\nabef\nabef");
}

#[test]
fn column_selection_formats_each_row() {
    let mut s = column_doc();
    block(&mut s);
    run(&mut s, "format.bold", json!({}));
    for i in 0..3 {
        let p = s.doc.para_at(&Pos::body(i, 0)).unwrap();
        assert_ne!(p.props_of_char(1).bold, Some(true));
        assert_eq!(p.props_of_char(2).bold, Some(true));
        assert_eq!(p.props_of_char(3).bold, Some(true));
        assert_ne!(p.props_of_char(4).bold, Some(true));
    }
    // The block survives formatting, and a second Bold turns it off again.
    assert!(s.column_segments().is_some());
    run(&mut s, "format.bold", json!({}));
    assert_ne!(s.doc.para_at(&Pos::body(1, 0)).unwrap().props_of_char(2).bold, Some(true));
    run(&mut s, "format.changeCase", json!({"mode": "upper"}));
    assert_eq!(text(&s), "abCDef\nabCDef\nabCDef");
}

#[test]
fn column_mode_extends_with_the_arrow_keys() {
    let mut s = column_doc();
    run(&mut s, "caret.set", json!({"pos": {"block": 0, "off": 1}}));
    let r = run(&mut s, "select.column", json!({}));
    assert_eq!(r["columnMode"], true);
    run(&mut s, "caret.right", json!({}));
    run(&mut s, "caret.right", json!({}));
    run(&mut s, "caret.down", json!({}));
    let r = run(&mut s, "caret.down", json!({}));
    assert_eq!(r["rows"], json!(["bc", "bc", "bc"]));
    run(&mut s, "text.backspace", json!({}));
    assert_eq!(text(&s), "adef\nadef\nadef");
    assert!(!s.column_mode);
    // Escape leaves the mode.
    run(&mut s, "select.column", json!({}));
    run(&mut s, "select.collapse", json!({}));
    assert!(!s.column_mode);
}

#[test]
fn column_selection_from_points() {
    let mut s = column_doc();
    let l = s.layout();
    let a = l.caret(&Pos::body(0, 1)).unwrap();
    let b = l.caret(&Pos::body(1, 5)).unwrap();
    let r = run(
        &mut s,
        "select.column",
        json!({"from": {"page": a.page, "x": a.x, "y": a.top + 2.0}, "to": {"page": b.page, "x": b.x, "y": b.top + 2.0}}),
    );
    assert_eq!(r["rows"], json!(["bcde", "bcde"]));
    for bad in [
        json!({"from": {"page": 99, "x": 1, "y": 1}, "to": {"page": 0, "x": 1, "y": 1}}),
        json!({"from": 1, "to": "x"}),
        json!({"anchor": 1, "focus": {}}),
    ] {
        assert!(s.run("select.column", &bad).is_err());
    }
}

#[test]
fn style_inspector_reports_and_clears_levels() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Chapter one"}));
    run(&mut s, "paragraph.heading1", json!({}));
    run(&mut s, "edit.selectAll", json!({}));
    run(&mut s, "format.bold", json!({"value": true}));
    run(&mut s, "format.alignCenter", json!({}));

    let r = run(&mut s, "style.inspect", json!({}));
    assert_eq!(r["paragraph"]["style"], "Heading1");
    assert_eq!(r["paragraph"]["styleName"], "Heading 1");
    let pdirect = r["paragraph"]["direct"].as_array().unwrap();
    assert!(pdirect.iter().any(|d| d["prop"] == "align" && d["value"] == "center"), "{pdirect:?}");
    assert!(r["character"]["style"].is_null());
    let cdirect = r["character"]["direct"].as_array().unwrap();
    assert_eq!(cdirect, &vec![json!({"prop": "bold", "value": true})]);

    // Formatting that matches the style isn't a difference (Heading 1 is 20 pt).
    run(&mut s, "format.fontSize", json!({"size": 20}));
    let r = run(&mut s, "style.inspect", json!({}));
    assert_eq!(r["character"]["direct"].as_array().unwrap().len(), 1, "{r}");

    // Character style level.
    run(&mut s, "format.charStyle", json!({"style": "Emphasis"}));
    let r = run(&mut s, "style.inspect", json!({}));
    assert_eq!(r["character"]["style"], "Emphasis");
    assert_eq!(r["character"]["styleName"], "Emphasis");
    run(&mut s, "style.inspectorClear", json!({"level": "characterStyle"}));
    assert!(run(&mut s, "style.inspect", json!({}))["character"]["style"].is_null());

    // Clearing character formatting removes the bold and keeps the paragraph levels.
    run(&mut s, "style.inspectorClear", json!({"level": "characterFormatting"}));
    let r = run(&mut s, "style.inspect", json!({}));
    assert!(r["character"]["direct"].as_array().unwrap().is_empty(), "{r}");
    assert!(!run(&mut s, "format.state", json!({}))["bold"].as_bool().unwrap());
    assert_eq!(r["paragraph"]["style"], "Heading1");
    assert!(!r["paragraph"]["direct"].as_array().unwrap().is_empty());

    run(&mut s, "style.inspectorClear", json!({"level": "paragraphFormatting"}));
    let r = run(&mut s, "style.inspect", json!({}));
    assert!(r["paragraph"]["direct"].as_array().unwrap().is_empty(), "{r}");
    assert_eq!(r["paragraph"]["style"], "Heading1");

    run(&mut s, "style.inspectorClear", json!({"level": "paragraphStyle"}));
    assert_eq!(run(&mut s, "style.inspect", json!({}))["paragraph"]["style"], "Normal");

    // Each clear is one undo step.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(run(&mut s, "style.inspect", json!({}))["paragraph"]["style"], "Heading1");
    assert_eq!(text(&s), "Chapter one");
}

#[test]
fn style_inspector_pane_and_hostile_params() {
    let mut s = Session::new(crate::sample::sample_document());
    let r = run(&mut s, "style.inspector", json!({}));
    assert_eq!(r["pane"], true);
    assert!(s.view.style_inspector);
    assert_eq!(r["paragraph"]["styleName"], "Title");
    let r = run(&mut s, "style.inspector", json!({"value": false}));
    assert_eq!(r["pane"], false);
    let before = text(&s);
    for junk in [json!(null), json!({}), json!({"level": 5}), json!({"level": "everything"}), json!({"level": ""})] {
        assert!(s.run("style.inspectorClear", &junk).is_err(), "{junk}");
    }
    assert_eq!(text(&s), before);
    assert!(s.run("style.inspector", &json!({"value": "yes"})).is_ok());
    // A selection reaching far outside the document.
    s.sel.focus = Pos::body(9999, 99999);
    s.sel.anchor = Pos::body(0, 0);
    assert!(s.run("style.inspect", &json!({})).is_ok());
    assert!(s.run("style.inspectorClear", &json!({"level": "characterFormatting"})).is_ok());
}

/// Home › Editing › Find › Advanced Find: every match, in the body or a selection, with Reading Highlight.
#[test]
fn advanced_find_lists_and_highlights_matches() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "cat dog cat"}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "Cat"}));
    // From the UI it opens the dialog.
    run(&mut s, "edit.advancedFind", json!({}));
    assert_eq!(s.ui_requests.last(), Some(&json!({"open": "find"})));
    let r = run(&mut s, "edit.advancedFind", json!({"text": "cat", "highlight": true}));
    assert_eq!(r["count"], 3);
    assert_eq!(s.find_highlights().len(), 3);
    assert_eq!(run(&mut s, "edit.advancedFind", json!({"text": "cat", "matchCase": true}))["count"], 2);
    // Highlights follow edits and the latest search.
    assert_eq!(s.find_highlights().len(), 2);
    run(&mut s, "text.insert", json!({"text": " cat"}));
    assert_eq!(s.find_highlights().len(), 3);
    // Only inside the selection.
    run(&mut s, "select.text", json!({"text": "cat dog"}));
    assert_eq!(run(&mut s, "edit.advancedFind", json!({"text": "cat", "in": "selection"}))["count"], 1);
    run(&mut s, "edit.advancedFind", json!({"highlight": false}));
    assert!(s.find_highlights().is_empty());
    // Hostile params.
    assert!(s.run("edit.advancedFind", &json!({"text": "x", "in": "elsewhere"})).is_err());
    assert!(s.run("edit.advancedFind", &json!({"text": "(", "regex": true})).is_err());
    run(&mut s, "edit.selectAll", json!({}));
    run(&mut s, "text.delete", json!({}));
    assert!(s.run("edit.advancedFind", &json!({"text": "x", "in": "selection"})).is_err());
}

/// A table style made by command styles the table at the caret (header row fill and bold
/// text, banded rows), can be modified, and saves as a table style the table refers to.
#[test]
fn custom_table_style_applies_modifies_and_saves() {
    use kition_doc_doc::{Rgb, StyleKind, TextColor};
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 3, "cols": 2}));
    run(&mut s, "text.insert", json!({"text": "Head"}));
    run(&mut s, "table.look", json!({"headerRow": true, "bandedRows": true}));
    let (tp, _, _) = s.sel.focus.path.cell().unwrap();
    let r = run(
        &mut s,
        "table.newStyle",
        json!({"name": "Thesis Table", "basedOn": "Table Grid",
            "wholeTable": {"size": 10},
            "headerRow": {"fill": "1F3864", "bold": true, "color": "FFFFFF", "borders": true},
            "bandedRows": {"fill": "EEEEEE", "italic": true}}),
    );
    let id = r["id"].as_str().unwrap().to_string();
    let table_style = |s: &Session| s.doc.table(StoryRef::Body, &tp).unwrap().props.style.clone();
    assert_eq!(table_style(&s).as_deref(), Some(id.as_str()), "applied to the current table");
    assert!(s.run("table.newStyle", &json!({"name": "thesis table"})).is_err(), "names are unique");
    assert!(s.run("table.newStyle", &json!({"name": "X", "basedOn": "Heading 1"})).is_err(), "bases are table styles");
    assert!(s.run("table.style", &json!({"style": "Normal"})).is_err(), "only table styles apply to tables");

    let fills = |s: &mut Session, c: Rgb| {
        s.layout().pages[0].items.iter().filter(|i| matches!(i, crate::layout::Placed::Fill { color, .. } if *color == c)).count()
    };
    let head = |s: &mut Session| {
        let mut p = tp.0.clone();
        p.extend([0, 0, 0]);
        s.layout().pages[0]
            .items
            .iter()
            .find_map(
                |i| if let crate::layout::Placed::Lines { path, para, .. } = i { (path.0 == p).then(|| para.styles[0].rc.clone()) } else { None },
            )
            .unwrap()
    };
    assert_eq!(fills(&mut s, Rgb(0x1F, 0x38, 0x64)), 2, "header cells");
    assert_eq!(fills(&mut s, Rgb(0xEE, 0xEE, 0xEE)), 2, "first band");
    let rc = head(&mut s);
    assert!(rc.bold && rc.size == 10.0, "{rc:?}");
    assert_eq!(rc.color, TextColor::Rgb(Rgb::WHITE));

    // Modify the current table's style: the table follows.
    run(&mut s, "table.setStyle", json!({"headerRow": {"fill": "C00000", "bold": false}, "bandSize": 2}));
    assert_eq!(fills(&mut s, Rgb(0x1F, 0x38, 0x64)), 0);
    assert_eq!(fills(&mut s, Rgb(0xC0, 0, 0)), 2);
    assert_eq!(fills(&mut s, Rgb(0xEE, 0xEE, 0xEE)), 4, "two rows per band");
    assert!(!head(&mut s).bold);

    let back = kition_doc_docx::read(&kition_doc_docx::write(&s.doc).unwrap()).unwrap();
    let st = back.styles.get(&id).unwrap();
    assert_eq!((st.kind, st.name.as_str(), st.based_on.as_deref()), (StyleKind::Table, "Thesis Table", Some("TableGrid")));
    let parts = st.table.as_ref().unwrap();
    assert_eq!((parts.header_fill, parts.band_fill, parts.band_size), (Some(Rgb(0xC0, 0, 0)), Some(Rgb(0xEE, 0xEE, 0xEE)), Some(2)));
    assert_eq!((parts.header_chr.bold, parts.band_chr.italic), (Some(false), Some(true)));
    assert!(parts.header_borders.is_some_and(|b| b.any_visible()));
    let t = back.body.iter().find_map(|b| b.as_table()).unwrap();
    assert_eq!(t.props.style.as_deref(), Some(id.as_str()), "w:tblStyle");
}

#[test]
fn page_and_table_gridlines_toggle_independently() {
    // View › Gridlines (the page drawing grid) and Table Layout › View Gridlines (table cell
    // outlines) are separate view switches; neither touches the document or the undo stack.
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    let undo = s.undo_labels();
    // Table cell outlines are on by default, as in Word; the page grid is off.
    assert!(!s.view.gridlines && s.view.table_gridlines);
    assert_eq!(run(&mut s, "table.viewGridlines", json!({"value": false}))["value"], false);

    assert_eq!(run(&mut s, "view.gridlines", json!({}))["value"], true);
    assert!(s.view.gridlines && !s.view.table_gridlines);

    // Works with the caret outside a table.
    assert_eq!(run(&mut s, "table.viewGridlines", json!({}))["value"], true);
    assert!(s.view.gridlines && s.view.table_gridlines);

    assert_eq!(run(&mut s, "view.gridlines", json!({"value": false}))["value"], false);
    assert!(!s.view.gridlines && s.view.table_gridlines);
    assert_eq!(run(&mut s, "view.gridlines", json!({"value": false}))["value"], false);
    assert!(!s.view.gridlines);

    assert_eq!(run(&mut s, "table.viewGridlines", json!({}))["value"], false);
    assert!(!s.view.gridlines && !s.view.table_gridlines);

    assert_eq!(text(&s), "Hello");
    assert_eq!(s.undo_labels(), undo);
    let state = run(&mut s, "view.state", json!({}));
    assert_eq!(state["gridlines"], false);
    assert_eq!(state["tableGridlines"], false);
}

/// View › Zoom steps. Zoom In/Out leave a fit mode and step 10% from the current zoom
/// (the UI keeps `view.zoom` equal to the shown zoom while a fit mode is on), and never get stuck
/// short of the 10%–500% limits.
#[test]
fn zoom_in_and_out_step_from_current_zoom_and_leave_fit_modes() {
    let mut s = s();
    let pct = |s: &Session| (s.view.zoom * 100.0).round() as i32;
    run(&mut s, "view.zoomIn", json!({}));
    assert_eq!(pct(&s), 110);
    run(&mut s, "view.zoomOut", json!({}));
    run(&mut s, "view.zoomOut", json!({}));
    assert_eq!(pct(&s), 90);
    for fit in ["view.pageWidth", "view.onePage", "view.multiplePages"] {
        run(&mut s, fit, json!({}));
        assert!(!s.view.fit.is_empty());
        // What the canvas reports while a fit mode shows the page at 163%.
        s.view.zoom = 1.63;
        run(&mut s, "view.zoomIn", json!({}));
        assert_eq!((pct(&s), s.view.fit.as_str(), s.view.multi_page), (170, "", false), "{fit}");
        run(&mut s, fit, json!({}));
        s.view.zoom = 1.63;
        run(&mut s, "view.zoomOut", json!({}));
        assert_eq!((pct(&s), s.view.fit.as_str()), (150, ""), "{fit}");
    }
    let mut last = pct(&s);
    for _ in 0..60 {
        run(&mut s, "view.zoomIn", json!({}));
        assert!(pct(&s) > last || pct(&s) == 500);
        last = pct(&s);
    }
    assert_eq!(last, 500);
    for _ in 0..60 {
        run(&mut s, "view.zoomOut", json!({}));
        assert!(pct(&s) < last || pct(&s) == 10);
        last = pct(&s);
    }
    assert_eq!(last, 10);
    run(&mut s, "view.zoom100", json!({}));
    assert_eq!(pct(&s), 100);
}

/// Comments, revisions and dates read the real clock, not a fixed date.
#[test]
fn timestamps_come_from_the_clock() {
    let before = cmd::now_unix();
    assert!(before > 1_760_000_000, "{before}");
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Dated"}));
    run(&mut s, "select.text", json!({"text": "Dated"}));
    run(&mut s, "comment.new", json!({"text": "When?"}));
    let date = run(&mut s, "comment.pane", json!({}))[0]["date"].as_str().unwrap_or_default().to_string();
    // Fixed-width ISO strings sort by time.
    assert!(date >= cmd::iso_from_unix_secs(before) && date <= cmd::iso_from_unix_secs(cmd::now_unix()), "{date}");
}

#[test]
fn diagrams_are_selected_and_deleted_but_not_moved_and_charts_resize() {
    use kition_doc_doc::graphic::{Graphic, GraphicKind};
    use kition_doc_doc::para::InlineObject;
    use std::sync::Arc;
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Before "}));
    let diagram = Graphic { kind: GraphicKind::Diagram, ..Default::default() };
    let obj = InlineObject::Graphic { w: 200.0, h: 100.0, alt: String::new(), float: Default::default(), graphic: Arc::new(diagram) };
    s.doc.insert_object(&Pos::body(0, 7), obj, &Default::default()).unwrap();
    let end = 7 + kition_doc_doc::para::OBJ.len_utf8();
    run(&mut s, "select.range", json!({"anchor": Pos::body(0, 7), "focus": Pos::body(0, end)}));
    assert!(crate::cmd::objects::object_selection(&s).is_some(), "the diagram is selected");
    for (id, v) in [
        ("arrange.bounds", json!({"width": 50})),
        ("image.size", json!({"width": 50})),
        ("arrange.wrap", json!({"wrap": "square"})),
        ("arrange.position", json!({"preset": "topLeft"})),
        ("arrange.align", json!({"value": "left"})),
        ("arrange.bringForward", json!({})),
    ] {
        assert!(s.run(id, &v).is_err(), "{id} must not change a diagram");
    }
    run(&mut s, "text.delete", json!({}));
    assert!(s.doc.para_at(&Pos::body(0, 0)).is_some_and(|p| p.objects.is_empty()), "the diagram is deleted");
    // A chart is drawn from its part at any size, and its frame is saved: it resizes.
    let chart = InlineObject::Graphic { w: 200.0, h: 100.0, alt: String::new(), float: Default::default(), graphic: Arc::new(Graphic::default()) };
    s.doc.insert_object(&Pos::body(0, 7), chart, &Default::default()).unwrap();
    run(&mut s, "select.range", json!({"anchor": Pos::body(0, 7), "focus": Pos::body(0, end)}));
    run(&mut s, "arrange.bounds", json!({"width": 150, "height": 80}));
    assert!(
        matches!(s.doc.para_at(&Pos::body(0, 7)).and_then(|p| p.object_at(7)), Some(InlineObject::Graphic { w, .. }) if *w == 150.0),
        "the chart resizes"
    );
}

#[test]
fn accessibility_reports_a_chart_without_alt_text() {
    use kition_doc_doc::para::InlineObject;
    use std::sync::Arc;
    let mut s = s();
    let chart = InlineObject::Graphic { w: 200.0, h: 100.0, alt: " ".into(), float: Default::default(), graphic: Arc::new(Default::default()) };
    s.doc.insert_object(&Pos::body(0, 0), chart, &Default::default()).unwrap();
    let v = run(&mut s, "file.accessibility", json!({}));
    let issues = v["issues"].as_array().expect("issues");
    assert!(issues.iter().any(|i| i["issue"] == "Chart or diagram has no alternative text"), "{issues:?}");
}

/// The Tabs dialog's command sets and clears stops (inherited ones too), changes the
/// default interval, and is one undo step.
#[test]
fn para_tabs_set_clear_and_undo() {
    use kition_doc_doc::props::{TabAlign, TabLeader};
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Name\tPrice"}));
    run(&mut s, "paragraph.style", json!({"style": "Header"}));
    let stops = |s: &Session| {
        let p = s.doc.para_at(&s.sel.focus).map(|p| p.props.clone()).unwrap_or_default();
        s.doc.styles.resolve_para(&p).tabs.iter().map(|t| (t.pos, t.align, t.leader)).collect::<Vec<_>>()
    };
    assert_eq!(stops(&s).len(), 2, "Header's own stops");
    run(
        &mut s,
        "paragraph.tabs",
        json!({"set": [{"pos": 100.0, "align": "decimal", "leader": "dot"}, {"pos": 300.0}], "clear": [234.0], "default": 18.0}),
    );
    assert_eq!(
        stops(&s),
        vec![(100.0, TabAlign::Decimal, TabLeader::Dot), (300.0, TabAlign::Left, TabLeader::None), (468.0, TabAlign::Right, TabLeader::None)]
    );
    assert_eq!(s.doc.settings.default_tab, 18.0);
    // Setting a stop at an existing position replaces it.
    run(&mut s, "paragraph.tabs", json!({"set": [{"pos": 300.0, "align": "bar"}]}));
    assert!(stops(&s).contains(&(300.0, TabAlign::Bar, TabLeader::None)));
    run(&mut s, "paragraph.tabs", json!({"clearAll": true}));
    assert!(stops(&s).is_empty(), "Clear All removes inherited stops too");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(stops(&s).len(), 3);
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(stops(&s).len(), 2);
    assert_eq!(s.doc.settings.default_tab, 36.0, "one undo step restores stops and default together");
    assert!(s.run("paragraph.tabs", &json!({"set": [{"pos": f64::MAX}]})).is_err());
    assert!(s.run("paragraph.tabs", &json!({"clear": "x"})).is_err());
}

/// Changing only the default tab interval (a document setting) is its own undo step,
/// and redo puts it back.
#[test]
fn para_tabs_default_only_undoes() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "a\tb"}));
    let before = s.doc.settings.default_tab;
    run(&mut s, "paragraph.tabs", json!({"default": 72.0}));
    assert_eq!(s.doc.settings.default_tab, 72.0);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.settings.default_tab, before, "undo restores the default interval");
    assert!(text(&s).contains("a\tb"), "only the setting was undone");
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.doc.settings.default_tab, 72.0, "redo sets it again");
}

/// Exact border sides with a style, for paragraphs and page borders.
#[test]
fn border_sides_apply_exactly() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Boxed"}));
    let side = json!({"style": "double", "width": 1.5, "color": "C00000"});
    run(&mut s, "paragraph.borders", json!({"sides": {"top": side, "bottom": side, "left": side, "right": side}}));
    let b = s.doc.para_at(&s.sel.focus).and_then(|p| p.props.borders).unwrap();
    for x in [b.top, b.bottom, b.left, b.right] {
        let x = x.unwrap();
        assert_eq!((x.style, x.width, x.color), (BorderStyle::Double, 1.5, Some(kition_doc_doc::props::Rgb(0xC0, 0, 0))));
    }
    assert!(b.between.is_none());
    run(&mut s, "paragraph.borders", json!({"sides": {}}));
    assert!(!s.doc.para_at(&s.sel.focus).and_then(|p| p.props.borders).unwrap().any_visible());
    run(&mut s, "page.borders", json!({"sides": {"top": {"style": "dashed", "width": 3.0}}, "applyTo": "document"}));
    let pb = s.doc.last_section.page_borders.unwrap();
    assert_eq!(pb.top.map(|t| (t.style, t.width)), Some((BorderStyle::Dashed, 3.0)));
    assert!(pb.left.is_none());
    run(&mut s, "page.borders", json!({"kind": "box", "style": "thick"}));
    assert_eq!(s.doc.last_section.page_borders.unwrap().left.map(|t| t.style), Some(BorderStyle::Thick));
    run(&mut s, "page.borders", json!({"sides": {}}));
    assert!(s.doc.last_section.page_borders.is_none());
}

/// Encrypt with Password: a document saved with a password opens only with it, keeps it
/// for the next save, and saves unencrypted once the password is removed.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn password_protected_documents_round_trip() {
    let dir = scratch_dir("password");
    let path = dir.join("secret.docx");
    let path_str = path.to_string_lossy().to_string();
    let mut a = s();
    run(&mut a, "file.setText", json!({"text": "Quarterly numbers"}));
    let r = run(&mut a, "file.save", json!({"path": path_str, "password": "hunter2"}));
    assert_eq!(r["encrypted"], true);
    assert!(kition_doc_docx::is_encrypted(&std::fs::read(&path).unwrap()));
    assert!(a.run("file.save", &json!({"path": dir.join("x.txt").to_string_lossy(), "password": "pw"})).is_err(), "only Word packages encrypt");

    // No password, then a wrong one: clear errors, and the open document stays.
    let mut t = s();
    let e = t.run("file.open", &json!({"path": path_str})).unwrap_err().to_string();
    assert!(crate::io::needs_password(&e), "{e}");
    let e = t.run("file.open", &json!({"path": path_str, "password": "hunter3"})).unwrap_err().to_string();
    assert!(crate::io::wrong_password(&e), "{e}");
    assert!(t.path.is_none());

    let r = run(&mut t, "file.open", json!({"path": path_str, "password": "hunter2"}));
    assert_eq!(r["encrypted"], true);
    assert_eq!(text(&t), "Quarterly numbers");
    assert_eq!(run(&mut t, "file.info", json!({}))["encrypted"], true);
    // Saving again keeps the password, as Word does.
    run(&mut t, "text.insert", json!({"text": "Q3 "}));
    run(&mut t, "file.save", json!({}));
    let (doc, encrypted) = crate::io::open_path_with(&path, Some("hunter2")).unwrap();
    assert!(encrypted && doc.plain_text(StoryRef::Body).contains("Q3"));

    // Removing the password saves the document unencrypted.
    assert!(t.run("file.encrypt", &json!({})).is_err());
    assert!(t.run("file.encrypt", &json!({"password": "x".repeat(300)})).is_err());
    t.dirty = false;
    assert_eq!(run(&mut t, "file.encrypt", json!({"password": null}))["encrypted"], false);
    assert!(t.dirty, "removing the password is a change to save");
    run(&mut t, "file.save", json!({}));
    assert!(crate::io::open_path(&path).unwrap().plain_text(StoryRef::Body).contains("Quarterly"));
    // A password never shows in debug output.
    assert_eq!(format!("{:?}", crate::io::Password::new("hunter2")), "Password(***)");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Shape Format › Shape Effects: presets and custom values apply to the selected shape,
/// an omitted key is kept, hostile numbers are clamped and each change is one undo step.
#[test]
fn shape_effects_apply_and_undo() {
    use kition_doc_doc::effects::{MAX_BLUR, MAX_DISTANCE, Shadow};
    let mut s = s();
    run(&mut s, "shape.insert", json!({"kind": "ellipse"}));
    let effects = |s: &Session| match cmd::objects::selected(s) {
        Some((_, kition_doc_doc::InlineObject::Shape { effects, float, .. })) => (effects, float.effect),
        o => panic!("{o:?}"),
    };
    assert!(effects(&s).0.is_empty());
    run(&mut s, "shape.effects", json!({"shadow": "offsetBottomRight", "glow": {"color": "FF0000", "size": 8}, "softEdge": 5}));
    let (e, extent) = effects(&s);
    assert_eq!(e.shadow, Shadow::preset("offsetBottomRight"));
    assert_eq!(e.glow.map(|g| (g.color, g.size)), Some((kition_doc_doc::props::Rgb(255, 0, 0), 8.0)));
    assert_eq!(e.soft_edge, Some(5.0));
    assert!(extent.iter().all(|v| *v >= 8.0), "room for the glow: {extent:?}");
    // Clearing the glow leaves the shadow and soft edges.
    run(&mut s, "shape.effects", json!({"glow": null}));
    let (e, _) = effects(&s);
    assert!(e.glow.is_none() && e.shadow.is_some() && e.soft_edge == Some(5.0));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(effects(&s).0.glow.map(|g| g.size), Some(8.0));
    // Hostile numbers.
    run(&mut s, "shape.effects", json!({"shadow": {"blur": 1e30, "distance": -5, "angle": 1e20, "transparency": 400}, "softEdge": -3}));
    let (e, _) = effects(&s);
    let sh = e.shadow.unwrap();
    assert!(sh.blur == MAX_BLUR && sh.distance == 0.0 && (0.0..360.0).contains(&sh.angle) && sh.transparency == 100.0, "{sh:?}");
    assert!(e.soft_edge.is_none());
    run(&mut s, "shape.effects", json!({"shadow": {"distance": 1e9}}));
    assert_eq!(effects(&s).0.shadow.map(|s| s.distance), Some(MAX_DISTANCE));
    assert!(s.run("shape.effects", &json!({"shadow": "perspective"})).is_err());
    // Off a shape it's disabled.
    run(&mut s, "select.collapse", json!({"end": true}));
    run(&mut s, "text.insert", json!({"text": "x"}));
    assert!(s.run("shape.effects", &json!({"shadow": null})).is_err());
}

#[test]
fn asian_typography_sets_flags_on_selected_paragraphs_and_undoes() {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "一\n二\n三"}));
    run(&mut s, "edit.selectAll", json!({}));
    let r = run(&mut s, "paragraph.asianTypography", json!({"kinsoku": false, "wordWrap": false, "topLinePunct": true}));
    assert_eq!(r["asianTypography"]["kinsoku"], false);
    assert_eq!(r["asianTypography"]["autoSpaceDE"], true, "a flag left out keeps its default");
    for p in s.doc.body.iter().filter_map(|b| b.as_para()) {
        assert_eq!(
            (p.props.kinsoku, p.props.word_wrap, p.props.top_line_punct, p.props.overflow_punct),
            (Some(false), Some(false), Some(true), None)
        );
    }
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc.body.iter().filter_map(|b| b.as_para()).all(|p| p.props.kinsoku.is_none() && p.props.word_wrap.is_none()));
    // Without flags it only reports them.
    let r = run(&mut s, "paragraph.asianTypography", json!({}));
    assert_eq!(r["asianTypography"]["kinsoku"], true);
}

#[test]
fn manage_styles_lists_paragraph_character_linked_and_table_styles() {
    let mut s = s();
    let list = run(&mut s, "style.manage", json!({"sort": "alphabetical"}));
    let list = list.as_array().unwrap();
    let find = |name: &str| list.iter().find(|x| x["name"] == name).unwrap_or_else(|| panic!("{name} listed"));
    assert_eq!(find("Normal")["type"], "paragraph");
    assert_eq!(find("Normal")["builtIn"], true);
    assert_eq!(find("Normal")["inGallery"], true);
    assert_eq!(find("Heading 1")["type"], "linked");
    assert_eq!(find("Heading 1")["basedOn"], "Normal");
    assert!(find("Heading 1")["format"]["size"].is_number(), "description formatting");
    assert_eq!(find("Table Grid")["type"], "table");
    assert!(list.iter().any(|x| x["type"] == "character"));
    // The character half of a linked pair is listed once, as the linked style.
    assert!(!list.iter().any(|x| x["id"] == "Heading1Char"));
    let names: Vec<String> = list.iter().map(|x| x["name"].as_str().unwrap().to_lowercase()).collect();
    assert!(names.windows(2).all(|w| w[0] <= w[1]), "alphabetical");
    // Recommended order starts with Normal (priority 0); listing is not an edit.
    let rec = run(&mut s, "style.manage", json!({}));
    assert_eq!(rec[0]["name"], "Normal");
    assert!(s.run("style.manage", &json!({"sort": "sideways"})).is_err());
    assert!(!s.dirty);
}

#[test]
fn deleting_a_style_falls_back_to_its_base_and_undoes() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Styled"}));
    run(&mut s, "style.add", json!({"name": "Mine", "basedOn": "Heading 1"}));
    run(&mut s, "style.add", json!({"name": "Child", "basedOn": "Mine", "fromSelection": false}));
    run(&mut s, "style.apply", json!({"style": "Mine"}));
    assert_eq!(para_style(&s, 0).as_deref(), Some("Mine"));
    let r = run(&mut s, "style.delete", json!({"style": "Mine"}));
    assert_eq!(r["restyled"], 1);
    assert!(s.doc.styles.get("Mine").is_none());
    assert_eq!(para_style(&s, 0).as_deref(), Some("Heading1"), "falls back to its base");
    assert_eq!(s.doc.styles.get("Child").and_then(|c| c.based_on.clone()).as_deref(), Some("Heading1"), "rebased");
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc.styles.get("Mine").is_some());
    assert_eq!(para_style(&s, 0).as_deref(), Some("Mine"));
    // Built-in styles are refused, unknown ones are errors.
    assert!(s.run("style.delete", &json!({"style": "Heading 1"})).is_err());
    assert!(s.run("style.delete", &json!({"style": "No Such Style"})).is_err());
    assert!(s.doc.styles.get("Heading1").is_some());
}

/// Manage Styles' Delete on a table style is Table Design's Delete Table Style: its tables take
/// Table Grid, styles based on it keep their look, and Word's built-in table styles are refused.
#[test]
fn deleting_a_table_style_from_manage_styles_uses_table_style_deletion() {
    use kition_doc_doc::Rgb;
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 2, "cols": 2}));
    let (tp, _, _) = s.sel.focus.path.cell().unwrap();
    run(&mut s, "table.newStyle", json!({"name": "Base", "headerRow": {"fill": "C00000"}}));
    run(&mut s, "table.newStyle", json!({"name": "Child", "basedOn": "Base", "apply": false}));
    let r = run(&mut s, "style.delete", json!({"style": "Base"}));
    assert_eq!((r["deleted"][0].as_str(), r["restyled"].as_u64()), (Some("Base"), Some(1)));
    assert_eq!(s.doc.table(StoryRef::Body, &tp).unwrap().props.style.as_deref(), Some("TableGrid"));
    assert_eq!(s.doc.styles.table_style("Child").unwrap().parts.header_fill, Some(Rgb(0xC0, 0, 0)), "the child keeps its look");
    for st in s.doc.styles.styles.iter_mut() {
        st.builtin = false;
    }
    assert!(s.run("style.delete", &json!({"style": "Table Grid"})).is_err());
    let list = run(&mut s, "style.manage", json!({}));
    let grid = list.as_array().unwrap().iter().find(|x| x["name"] == "Table Grid").unwrap();
    assert_eq!(grid["builtIn"], true);
}

#[test]
fn style_visibility_round_trips_through_docx() {
    let mut s = s();
    run(&mut s, "style.add", json!({"name": "Mine", "fromSelection": false}));
    run(&mut s, "style.setVisibility", json!({"style": "Heading 1", "gallery": false}));
    run(&mut s, "style.setVisibility", json!({"style": "Mine", "hidden": true}));
    run(&mut s, "style.setVisibility", json!({"style": "Heading 4", "gallery": true}));
    assert!(s.run("style.setVisibility", &json!({"style": "Mine"})).is_err());
    let gallery: Vec<String> = s.doc.styles.gallery().iter().map(|x| x.id.clone()).collect();
    assert!(!gallery.contains(&"Heading1".to_string()) && !gallery.contains(&"Mine".to_string()) && gallery.contains(&"Heading4".to_string()));
    let bytes = crate::io::save_bytes("v.docx", &s.doc).unwrap();
    let doc = crate::io::open_bytes("v.docx", &bytes).unwrap();
    let st = |id: &str| doc.styles.get(id).cloned().unwrap_or_else(|| panic!("{id} kept"));
    assert!(!st("Heading1").quick && !st("Heading1").hidden);
    assert!(st("Mine").hidden && !st("Mine").builtin);
    assert!(st("Heading4").quick);
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc.styles.get("Heading4").is_some_and(|h| !h.quick), "undoable");
}

/// The first/last column, total row and banded column regions apply in layout (later
/// regions win: the total row's fill beats the first column's) and round-trip through .docx.
#[test]
fn custom_table_style_column_and_total_regions() {
    use kition_doc_doc::Rgb;
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 3, "cols": 4}));
    run(
        &mut s,
        "table.look",
        json!({"headerRow": false, "bandedRows": false, "firstColumn": true, "lastColumn": true, "totalRow": true, "bandedColumns": true}),
    );
    let r = run(
        &mut s,
        "table.newStyle",
        json!({"name": "Ledger",
            "firstColumn": {"fill": "112233", "bold": true},
            "lastColumn": {"fill": "445566"},
            "lastRow": {"fill": "778899", "borders": true},
            "bandedColumns": {"fill": "ABCDEF"}}),
    );
    let id = r["id"].as_str().unwrap().to_string();
    let fills = |s: &mut Session, c: Rgb| {
        s.layout().pages[0].items.iter().filter(|i| matches!(i, crate::layout::Placed::Fill { color, .. } if *color == c)).count()
    };
    // 3 rows x 4 columns: the total row takes all 4 of its cells, the first and last columns the
    // two cells above it, and the column band (column 2 of the inner columns 2 and 3) one each.
    assert_eq!(fills(&mut s, Rgb(0x77, 0x88, 0x99)), 4, "total row");
    assert_eq!(fills(&mut s, Rgb(0x11, 0x22, 0x33)), 2, "first column");
    assert_eq!(fills(&mut s, Rgb(0x44, 0x55, 0x66)), 2, "last column");
    assert_eq!(fills(&mut s, Rgb(0xAB, 0xCD, 0xEF)), 2, "first column band");

    let back = kition_doc_docx::read(&kition_doc_docx::write(&s.doc).unwrap()).unwrap();
    let p = back.styles.get(&id).unwrap().table.clone().unwrap();
    assert_eq!(
        (p.first_col_fill, p.first_col_chr.bold, p.last_col_fill, p.total_fill, p.col_band_fill),
        (Some(Rgb(0x11, 0x22, 0x33)), Some(true), Some(Rgb(0x44, 0x55, 0x66)), Some(Rgb(0x77, 0x88, 0x99)), Some(Rgb(0xAB, 0xCD, 0xEF)))
    );
    assert!(p.total_borders.is_some_and(|b| b.any_visible()));
}

/// Deleting a custom table style re-bases the styles based on it (keeping their look),
/// puts its tables on Table Grid, refuses built-in styles, and undoes.
#[test]
fn delete_table_style_falls_back_and_undoes() {
    use kition_doc_doc::Rgb;
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 2, "cols": 2}));
    let (tp, _, _) = s.sel.focus.path.cell().unwrap();
    run(&mut s, "table.newStyle", json!({"name": "Base", "headerRow": {"fill": "C00000"}}));
    run(&mut s, "table.newStyle", json!({"name": "Child", "basedOn": "Base", "apply": false, "wholeTable": {"bold": true}}));
    let style = |s: &Session| s.doc.table(StoryRef::Body, &tp).unwrap().props.style.clone();
    assert_eq!(style(&s).as_deref(), Some("Base"));
    assert!(s.run("table.deleteStyle", &json!({"style": "Table Grid"})).is_err(), "built-in");
    assert!(s.run("table.deleteStyle", &json!({"style": "Normal Table"})).is_err(), "built-in");

    let r = run(&mut s, "table.deleteStyle", json!({}));
    assert_eq!((r["deleted"].as_str(), r["tables"].as_u64()), (Some("Base"), Some(1)));
    assert!(s.doc.styles.get("Base").is_none());
    assert_eq!(style(&s).as_deref(), Some("TableGrid"), "tables fall back to Table Grid");
    let child = s.doc.styles.table_style("Child").unwrap();
    assert_eq!((child.parts.header_fill, child.chr.bold), (Some(Rgb(0xC0, 0, 0)), Some(true)), "the child keeps its look");
    assert_eq!(s.doc.styles.get("Child").unwrap().based_on.as_deref(), Some("TableGrid"));

    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc.styles.get("Base").is_some());
    assert_eq!(style(&s).as_deref(), Some("Base"));
    assert_eq!(s.doc.styles.get("Child").unwrap().based_on.as_deref(), Some("Base"));
}

/// A .docx may mark Word's built-in table styles as custom; they still can't be deleted, so
/// no table is left pointing at a style that's gone.
#[test]
fn builtin_table_styles_marked_custom_in_a_docx_cant_be_deleted() {
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 1, "cols": 1}));
    for st in s.doc.styles.styles.iter_mut() {
        st.builtin = false;
    }
    for name in ["Table Grid", "Normal Table", "TableGrid"] {
        assert!(s.run("table.deleteStyle", &json!({"style": name})).is_err(), "{name}");
    }
    let (tp, _, _) = s.sel.focus.path.cell().unwrap();
    let id = s.doc.table(StoryRef::Body, &tp).unwrap().props.style.clone().unwrap();
    assert!(s.doc.styles.get(&id).is_some(), "the table's style still exists");
}

/// Delete Cells can leave a row shorter than the table grid; Delete Columns then removed that row's
/// only cell and every row with it, leaving a table block with no rows.
#[test]
fn deleting_the_last_cell_column_never_leaves_a_table_without_rows() {
    let mut s = s();
    run(&mut s, "table.insert", json!({"rows": 1, "cols": 2}));
    run(&mut s, "table.deleteCells", json!({}));
    run(&mut s, "table.deleteColumn", json!({}));
    for b in s.doc.body.iter() {
        if let Some(t) = b.as_table() {
            assert!(!t.rows.is_empty(), "table without rows");
        }
    }
    assert!(s.doc.body.iter().all(|b| b.as_table().is_none()), "an emptied table is removed");
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc.body.iter().any(|b| b.as_table().is_some_and(|t| !t.rows.is_empty())));
}

#[test]
fn design_theme_re_resolves_theme_references_and_keeps_them() {
    use kition_doc_doc::props::{Rgb, TextColor, ThemeColor, ThemeFonts, ThemeSlot};
    let mut doc = kition_doc_doc::Document::new();
    // A style coloured by accent 2 darkened, in the theme's heading font, as read from a file;
    // another whose colour was edited after reading.
    let old = ThemeColor { slot: ThemeSlot::Accent2, tint: None, shade: Some(0xBF), rgb: Rgb(0, 0, 0) };
    let shaded = ThemeColor { rgb: old.resolve(&doc.settings.theme_colors).unwrap(), ..old };
    let major = doc.settings.major_font.clone();
    let quote = doc.styles.styles.iter_mut().find(|st| st.id == "Quote").unwrap();
    quote.chr.color = Some(TextColor::Rgb(shaded.rgb));
    quote.chr.color_theme = Some(shaded);
    quote.chr.font = Some(major.clone());
    quote.chr.font_theme =
        Some(Box::new(ThemeFonts { ascii: Some("majorHAnsi".into()), h_ansi: Some("majorHAnsi".into()), font: Some(major), ..Default::default() }));
    let title = doc.styles.styles.iter_mut().find(|st| st.id == "Subtitle").unwrap();
    title.chr.color = Some(TextColor::Rgb(Rgb(1, 2, 3)));
    title.chr.color_theme = Some(shaded);
    let mut s = Session::new(doc);
    run(&mut s, "theme.set", json!({"name": "Studio"}));
    let accent2 = s.doc.settings.theme_colors[5];
    let expect = ThemeColor { rgb: accent2, ..old }.resolve(&s.doc.settings.theme_colors).unwrap();
    let quote = s.doc.styles.get("Quote").unwrap();
    assert_eq!(quote.chr.color, Some(TextColor::Rgb(expect)));
    assert_eq!(quote.chr.color_theme.map(|t| t.rgb), Some(expect), "the reference still holds");
    assert_eq!(quote.chr.font.as_deref(), Some("Georgia"));
    assert_eq!(quote.chr.font_theme.as_ref().and_then(|t| t.font.as_deref()), Some("Georgia"));
    assert_eq!(s.doc.styles.get("Subtitle").unwrap().chr.color, Some(TextColor::Rgb(Rgb(1, 2, 3))), "an edited colour stays");
}

#[test]
fn a_new_document_reports_no_character_toggles_on() {
    // A new document, and the same saved and opened again (as Kition makes new files), has its
    // caret in plain Normal text: no Bold, Italic, Underline... pressed in the ribbon.
    let mut fresh = s();
    let bytes = crate::io::save_bytes("new.docx", &fresh.doc).unwrap();
    let mut reopened = Session::new(crate::io::open_bytes("new.docx", &bytes).unwrap());
    for s in [&mut fresh, &mut reopened] {
        let f = run(s, "format.state", json!({}));
        for toggle in ["bold", "italic", "underline", "strike", "border", "subscript", "superscript"] {
            assert_eq!(f[toggle], false, "{toggle}: {f}");
        }
        assert_eq!(f["styleName"], "Normal");
    }
}

/// Saving with nothing new writes the same bytes (Kition One saw a second Save make a version
/// differing only in `dcterms:modified`): the save stamp (modified time, last modified by,
/// revision) changes only when the document changed since it was last saved or opened.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn saving_with_nothing_new_writes_the_same_bytes() {
    let dir = scratch_dir("save-stamp");
    let (input, again) = (dir.join("input.docx"), dir.join("again.docx"));
    // A file saved before, long ago.
    let mut doc = crate::sample::letter();
    doc.core.modified = "2020-01-02T03:04:05Z".into();
    doc.core.created = "2020-01-01T00:00:00Z".into();
    doc.core.last_modified_by = "Ada".into();
    doc.core.revision = 4;
    crate::io::save_path_with(&input, &doc, None).unwrap();
    let original = std::fs::read(&input).unwrap();

    // Opened and saved untouched: the file it was.
    let mut a = s();
    run(&mut a, "file.open", json!({"path": input.to_string_lossy()}));
    let rev = a.rev();
    run(&mut a, "file.save", json!({}));
    assert_eq!(std::fs::read(&input).unwrap(), original, "an untouched file saves as it was");
    assert_eq!(a.rev(), rev, "a save with nothing new changes nothing");
    run(&mut a, "file.saveAs", json!({"path": again.to_string_lossy()}));
    assert_eq!(std::fs::read(&again).unwrap(), original);

    // An edit stamps the next save, once.
    run(&mut a, "text.insert", json!({"text": "New words. "}));
    run(&mut a, "file.save", json!({}));
    let edited = std::fs::read(&again).unwrap();
    assert_ne!(edited, original);
    let core = crate::io::open_path(&again).unwrap().core;
    assert_ne!(core.modified, "2020-01-02T03:04:05Z", "an edit's save has a new modified time");
    assert_eq!((core.created.as_str(), core.last_modified_by.as_str(), core.revision), ("2020-01-01T00:00:00Z", "Kition Doc User", 5));
    run(&mut a, "file.save", json!({}));
    assert_eq!(std::fs::read(&again).unwrap(), edited, "saving again with nothing new");

    // Reopening the saved file: the same bytes again.
    let mut b = s();
    run(&mut b, "file.open", json!({"path": again.to_string_lossy()}));
    run(&mut b, "file.save", json!({}));
    assert_eq!(std::fs::read(&again).unwrap(), edited, "the saved file reopens and saves as it was");

    // A new document is stamped on its first save, and then saves the same.
    let fresh = dir.join("fresh.docx");
    let mut c = s();
    run(&mut c, "file.saveAs", json!({"path": fresh.to_string_lossy()}));
    let first = std::fs::read(&fresh).unwrap();
    assert!(!crate::io::open_path(&fresh).unwrap().core.modified.is_empty());
    run(&mut c, "file.save", json!({}));
    assert_eq!(std::fs::read(&fresh).unwrap(), first);
}

/// `edit.goto {paragraph}` takes the index `file.inspect` gives a body block, from 0,
/// not from 1; a table's index goes to its first cell.
#[test]
fn goto_paragraph_takes_the_inspect_index() {
    let mut a = s();
    for text in ["Zero", "One"] {
        run(&mut a, "text.insert", json!({"text": text}));
        run(&mut a, "text.newParagraph", json!({}));
    }
    run(&mut a, "table.insert", json!({"rows": 2, "cols": 2}));
    run(&mut a, "caret.docEnd", json!({}));
    run(&mut a, "text.insert", json!({"text": "Last"}));
    let blocks = run(&mut a, "file.inspect", json!({}))["blocks"].clone();
    let index_of = |text: &str| blocks.as_array().unwrap().iter().find(|b| b["text"] == text).map(|b| b["index"].as_u64().unwrap()).unwrap();
    for text in ["Zero", "One", "Last"] {
        let i = index_of(text);
        run(&mut a, "edit.goto", json!({"paragraph": i}));
        assert_eq!(a.sel.focus.path.0, vec![i as u32], "{text}");
        assert_eq!(a.doc.para_at(&a.sel.focus).unwrap().plain_text(), text);
    }
    let table = blocks.as_array().unwrap().iter().find(|b| b["type"] == "table").map(|b| b["index"].as_u64().unwrap()).unwrap();
    run(&mut a, "edit.goto", json!({"paragraph": table}));
    assert!(a.sel.focus.path.cell().is_some(), "a table's index goes into its first cell");
    assert_eq!(a.sel.focus.path.0.first(), Some(&(table as u32)));
    assert!(a.run("edit.goto", &json!({"paragraph": 99})).is_err(), "an index past the body is an error");
}

/// `page.insertCover` puts one page of its own paragraphs before the
/// document, whatever the caret's paragraph is (here an empty Heading 1, which the cover took on
/// and spilled onto a second page); it writes no date unless given one, so it does not change
/// from day to day; and the caret stays where it was, as Word leaves it.
#[test]
fn cover_page_is_one_page_of_its_own_before_the_document() {
    for page in [None, Some(json!({"width": 420, "height": 595}))] {
        let mut a = s();
        if let Some(size) = page {
            run(&mut a, "page.size", size);
        }
        run(&mut a, "paragraph.style", json!({"style": "Heading 1"}));
        run(&mut a, "page.insertCover", json!({"title": "Annual Review", "subtitle": "Our year in numbers", "author": "Ada Byron"}));
        run(&mut a, "text.insert", json!({"text": "Introduction"}));
        let inspect = run(&mut a, "file.inspect", json!({}));
        let blocks = inspect["blocks"].as_array().unwrap();
        let last = blocks.last().unwrap();
        assert_eq!((last["text"].as_str(), last["style"].as_str()), (Some("Introduction"), Some("Heading1")), "typing continues where it was");
        for b in &blocks[..blocks.len() - 1] {
            assert_ne!(b["style"], "Heading1", "the cover's paragraphs keep their own styles: {b}");
        }
        let text = a.doc.plain_text(StoryRef::Body);
        for part in ["Annual Review", "Our year in numbers", "Ada Byron"] {
            assert!(text.contains(part), "{part}: {text}");
        }
        assert!(!text.contains(&cmd::now_iso()[..4]), "no date unless one is given: {text}");
        assert_eq!(inspect["pages"], 2, "the cover is one page, the document starts on the next");
        let caret = a.sel.focus.path.clone();
        let l = a.layout();
        let on_page_2 = l.pages.get(1).is_some_and(|p| {
            p.items.iter().any(|it| matches!(it, kition_doc_layout::Placed::Lines { story: StoryRef::Body, path, .. } if *path == caret))
        });
        assert!(on_page_2, "the document's first paragraph starts page 2");
    }
    let mut a = s();
    run(&mut a, "page.insertCover", json!({"title": "Plan", "date": "1 March 2026"}));
    assert!(a.doc.plain_text(StoryRef::Body).contains("1 March 2026"));
}

/// `pageNumber.insert {position: current}` honours `format` ("x of y")
/// and `align` as the header and footer positions do.
#[test]
fn page_number_at_the_caret_takes_format_and_align() {
    let mut a = s();
    run(&mut a, "footer.insert", json!({"text": ""}));
    run(&mut a, "pageNumber.insert", json!({"position": "current", "format": "x of y", "align": "right"}));
    let story = a.sel.focus.story;
    let p = a.doc.para_at(&a.sel.focus).unwrap().clone();
    assert_eq!(p.props.align, Some(kition_doc_doc::props::Align::Right));
    let fields: Vec<String> = p
        .objects
        .iter()
        .filter_map(|o| match o {
            kition_doc_doc::InlineObject::Field { instr, .. } => Some(instr.trim().to_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(fields.len(), 2, "{fields:?}");
    assert!(fields.iter().any(|f| f == "PAGE") && fields.iter().any(|f| f == "NUMPAGES"), "{fields:?}");
    assert!(a.doc.plain_text(story).starts_with("Page "), "{}", a.doc.plain_text(story));
}

/// The table of contents shows heading numbers: headings numbered by a
/// list linked to their styles list as "number, tab, text, tab, page", as Word writes them, and
/// the TOC Heading (based on Heading 1) does not take Heading 1's number.
#[test]
fn toc_entries_show_heading_numbers() {
    let mut a = s();
    run(&mut a, "toc.insert", json!({"levels": 2}));
    run(&mut a, "caret.docEnd", json!({}));
    run(&mut a, "paragraph.heading1", json!({}));
    run(
        &mut a,
        "list.define",
        json!({"name": "Headings", "levels": [
            {"format": "decimal", "text": "%1", "style": "Heading 1", "alignedAt": 0, "indentAt": 28},
            {"format": "decimal", "text": "%1.%2", "style": "Heading 2", "alignedAt": 0, "indentAt": 36}]}),
    );
    for (style, text) in [
        ("paragraph.heading1", "Introduction"),
        ("paragraph.heading2", "Background"),
        ("paragraph.heading2", "Scope"),
        ("paragraph.heading1", "Plan"),
    ] {
        run(&mut a, style, json!({}));
        run(&mut a, "text.insert", json!({"text": text}));
        run(&mut a, "text.newParagraph", json!({}));
    }
    run(&mut a, "toc.update", json!({}));
    let entries: Vec<String> = a
        .doc
        .body
        .iter()
        .filter_map(|b| b.as_para())
        .filter(|p| p.props.style.as_deref().is_some_and(|st| st.starts_with("TOC") && st != "TOCHeading"))
        .map(|p| p.plain_text())
        .collect();
    assert_eq!(entries, ["1\tIntroduction\t1", "1.1\tBackground\t1", "1.2\tScope\t1", "2\tPlan\t1"]);
    let head = a.doc.body.iter().filter_map(|b| b.as_para()).find(|p| p.props.style.as_deref() == Some("TOCHeading")).unwrap();
    let numbering = head.props.numbering.or_else(|| a.doc.styles.resolve_para(&head.props).numbering).filter(|n| n.num != 0);
    assert_eq!(numbering, None, "the TOC Heading is not numbered like Heading 1");
}

/// Table formulas take cell references, arithmetic and functions over
/// ranges, read "£360.50" as a number, format with a `\#` picture, and compute again on F9.
#[test]
fn table_formulas_take_references_arithmetic_and_pictures() {
    let mut a = s();
    run(&mut a, "table.insert", json!({"rows": 5, "cols": 2}));
    for text in ["Item", "Amount", "Repairs", "£360.50", "Delivery", "£1,200", "Subtotal"] {
        run(&mut a, "text.insert", json!({"text": text}));
        run(&mut a, "text.tab", json!({}));
    }
    let subtotal = run(&mut a, "table.formula", json!({"formula": "=SUM(ABOVE) \\# \"£#,##0.00\""}));
    assert_eq!(subtotal["result"], "£1,560.50");
    run(&mut a, "text.tab", json!({}));
    run(&mut a, "text.insert", json!({"text": "VAT"}));
    run(&mut a, "text.tab", json!({}));
    let vat = run(&mut a, "table.formula", json!({"formula": "=B4*20% \\# \"#,##0.00\""}));
    assert_eq!(vat["result"], "312.10");
    for (formula, want) in [
        ("=SUM(B2:B3)", "1560.50"),
        ("=B2+B3*2", "2760.50"),
        ("=MAX(B2,B3)-MIN(B2:B3)", "839.50"),
        ("=MIN(B:B)", "312.10"),
        ("=ROUND(AVERAGE(B2:B3),0)", "780"),
        ("=IF(B3>B2,1,0)", "1"),
        ("=COUNT(ABOVE)", "3"),
    ] {
        let table = a.doc.body.iter().find_map(|b| match &**b {
            kition_doc_doc::Block::Table(t) => Some(t.clone()),
            _ => None,
        });
        let got = cmd::table_formula::evaluate(formula, &table.unwrap(), 4, 1).unwrap();
        assert_eq!(got, want, "{formula}");
    }
    assert!(a.run("table.formula", &json!({"formula": "=SUM(ABOVE"})).is_err(), "a syntax error inserts nothing");

    // F9 computes formulas again from the cells as they are now.
    run(&mut a, "select.text", json!({"text": "£360.50"}));
    run(&mut a, "text.insert", json!({"text": "£400"}));
    run(&mut a, "field.updateAll", json!({}));
    let text = a.doc.plain_text(StoryRef::Body);
    assert!(text.contains("£1,600.00") && text.contains("320.00"), "{text}");
}

/// C8-doc-4's reading (C8-doc-5): a command that changes the document is no pure command: Edit
/// Header and Edit Footer make the part when there is none, Version History restores a version,
/// New from Template replaces the document. Marked pure, they changed it without an undo step or
/// a change an agent's receipt tells.
#[test]
fn commands_that_change_the_document_are_not_pure() {
    let reg = cmd::registry();
    for id in ["header.edit", "footer.edit", "file.versions", "file.newFromTemplate"] {
        assert!(reg.get(id).is_some_and(|spec| spec.mutates), "{id} is marked pure");
    }
    // Edit Header on a document without one makes it, and undo takes it away again.
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "body"}));
    let parts = s.doc.parts.len();
    run(&mut s, "header.edit", json!({}));
    assert_eq!(s.doc.parts.len(), parts + 1);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.parts.len(), parts);
}

/// C8-doc-5 (the C8-doc-3 reading's defects): a paragraph keeps its id (`w14:paraId`, its tool
/// id) when its formatting is cleared or matched to a style, and a paragraph whose formatting is
/// painted onto another keeps its own: Clear All Formatting, the Style Inspector's Clear, Update
/// Style to Match dropped it (the next save gave it a new one), Paste Formatting copied the
/// source paragraph's onto the target (two paragraphs with one id).
#[test]
fn a_paragraph_keeps_its_id_through_formatting_commands() {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "first\nsecond\nthird"}));
    for block in 0..3u32 {
        let path = kition_doc_doc::Path::top(block as usize);
        s.doc.para_mut(StoryRef::Body, &path).unwrap().props.para_id = Some(0x1000 + block);
    }
    let id = |s: &Session, block: usize| s.doc.para_at(&Pos::new(StoryRef::Body, kition_doc_doc::Path::top(block), 0)).and_then(|p| p.props.para_id);
    let ids: Vec<Option<u32>> = (0..3).map(|b| id(&s, b)).collect();
    assert!(ids.iter().all(Option::is_some), "{ids:?}");
    let select =
        |s: &mut Session, block: usize| run(s, "select.range", json!({"anchor": {"block": block, "off": 0}, "focus": {"block": block, "off": 3}}));
    for (block, command, params) in
        [(0, "format.clear", json!({})), (1, "style.inspectorClear", json!({"level": "paragraphFormatting"})), (2, "style.updateToMatch", json!({}))]
    {
        select(&mut s, block);
        run(&mut s, command, params);
        assert_eq!(id(&s, block), ids[block], "{command}");
    }
    select(&mut s, 0);
    run(&mut s, "edit.copyFormat", json!({}));
    run(&mut s, "select.range", json!({"anchor": {"block": 1, "off": 0}, "focus": {"block": 2, "off": 2}}));
    run(&mut s, "edit.pasteFormat", json!({}));
    assert_eq!((id(&s, 1), id(&s, 2)), (ids[1], ids[2]), "edit.pasteFormat");
}

/// C8-doc-5 (the C8-doc-3 reading's defects): a regular expression that does not name the object
/// character U+FFFC never matches across a picture's, shape's or equation's anchor: `.+` replaced
/// the paragraph's text and the equation in it with it.
#[test]
fn a_pattern_does_not_match_object_anchors_it_does_not_name() {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "before after"}));
    run(&mut s, "select.range", json!({"anchor": {"block": 0, "off": 7}, "focus": {"block": 0, "off": 7}}));
    run(&mut s, "equation.insert", json!({"linear": "x^2", "edit": false}));
    let objects = |s: &Session| s.doc.para_at(&Pos::new(StoryRef::Body, kition_doc_doc::Path::top(0), 0)).map_or(0, |p| p.object_offsets().len());
    assert_eq!(objects(&s), 1);
    run(&mut s, "edit.replaceAll", json!({"text": ".+", "with": "X", "regex": true}));
    assert_eq!(objects(&s), 1, "{}", text(&s));
    // A pattern naming the anchor matches it.
    run(&mut s, "edit.replaceAll", json!({"text": "\u{FFFC}", "with": "", "regex": true}));
    assert_eq!(objects(&s), 0);
}

/// C8-doc-5 (the C8-doc-3 reading's defects): a section command with the caret in a header acts
/// on the section the caret came from (`body_return`), not on the body block numbered as the
/// caret's paragraph in the header (`page.rs`: `sel.focus.path[0]` in any story); with no body
/// place to go by it is refused.
#[test]
fn a_section_command_in_a_header_acts_on_the_section_the_caret_came_from() {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "one\ntwo\nthree\nfour"}));
    run(&mut s, "select.range", json!({"anchor": {"block": 1, "off": 3}, "focus": {"block": 1, "off": 3}}));
    run(&mut s, "page.break", json!({"kind": "nextPage"}));
    let landscape = |s: &Session| -> Vec<bool> { s.doc.sections().iter().map(|(_, x)| x.page_w > x.page_h).collect() };
    assert_eq!(landscape(&s), [false, false]);
    let last = s.doc.body.len() - 1;
    run(&mut s, "select.range", json!({"anchor": {"block": last, "off": 0}, "focus": {"block": last, "off": 0}}));
    run(&mut s, "header.edit", json!({}));
    assert_ne!(s.sel.focus.story, StoryRef::Body);
    run(&mut s, "page.orientation", json!({"value": "landscape"}));
    assert_eq!(landscape(&s), [false, true]);
    // A caret put in a part with no body place to go by.
    s.body_return = None;
    assert!(s.run("page.orientation", &json!({"value": "portrait"})).is_err());
    assert_eq!(landscape(&s), [false, true]);
}

/// C8-doc-5 (the C8-doc-3 reading's defects): a selection from a cell of one table into another
/// acts on the cell the focus is in, in the focus's table (`table.rs` `selected_cells` took the
/// selection's later end's cell and applied it to the focus's table).
#[test]
fn a_cell_command_across_two_tables_acts_on_the_focus_cell() {
    let mut s = s();
    run(&mut s, "file.setText", json!({"text": "a\nb\nc"}));
    run(&mut s, "select.range", json!({"anchor": {"block": 2, "off": 0}, "focus": {"block": 2, "off": 0}}));
    run(&mut s, "table.insert", json!({"rows": 2, "cols": 2}));
    run(&mut s, "select.range", json!({"anchor": {"block": 0, "off": 0}, "focus": {"block": 0, "off": 0}}));
    run(&mut s, "table.insert", json!({"rows": 2, "cols": 2}));
    let tables: Vec<usize> =
        s.doc.body.iter().enumerate().filter(|(_, b)| matches!(&***b, kition_doc_doc::Block::Table(_))).map(|(i, _)| i).collect();
    assert_eq!(tables.len(), 2);
    let (first, second) = (tables[0], tables[1]);
    // Backward: the anchor in the second table's last cell, the focus in the first's first.
    run(
        &mut s,
        "select.range",
        json!({"anchor": {"story": "body", "path": [second, 1, 1, 0], "off": 0}, "focus": {"story": "body", "path": [first, 0, 0, 0], "off": 0}}),
    );
    run(&mut s, "table.shading", json!({"color": "FF0000"}));
    let shaded = |s: &Session, t: usize| -> Vec<bool> {
        let kition_doc_doc::Block::Table(table) = &*s.doc.body[t] else { return Vec::new() };
        table.rows.iter().flat_map(|r| r.cells.iter().map(|c| c.props.shading.is_some())).collect()
    };
    assert_eq!(shaded(&s, first), [true, false, false, false]);
    assert_eq!(shaded(&s, second), [false, false, false, false]);
}
