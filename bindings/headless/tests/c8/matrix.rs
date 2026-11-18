//! C8-doc-2 item 8: the guard's matrix, as kition-sdk's call-context adoption measures it ("What
//! the guard is proven against"): address form × colleague change × same server / outside.
//!
//! The document: [`TEXTS`] (blocks 0–4), a 2×2 table at block 5 (cells `Cell rc`), an empty
//! paragraph at block 6, a rectangle anchored at the start of block 4, a comment thread on
//! `Delta` (block 3). The target is block 2, `Gamma five six`; the range is `[6, 10)` (`five`).
//!
//! **Expectations, written before the first run** (never changed to fit a result; a change after
//! the fact is listed in the card's report with its reason). `A`: the write must apply. `R`: it
//! must be refused, `R:<reason>` with that reason among the conflicts. The rule per form:
//! - F1 `on` the target's id, italic: the paragraph exists and the formats its characters carry.
//! - F2 a caret at block 2 by index, centre: block 2 is the same paragraph, its properties.
//! - F3 `{para: id}` `[6,10)`, bold: characters `[0,10)`, their marks, the paragraph's style.
//! - F4 a range by index from block 1 offset 5 to block 2 offset 10, bold: block 1's place and
//!   whole text, block 2's place, characters `[0,10)`, marks, style.
//! - F5 a cell path `[5,0,1,0]` `[0,4)`, bold: the table's place, row 0's place, cells 0–1.
//! - F6 no position (the call's caret, block 0 offset 0), typing: block 0 is the same paragraph.
//! - F7 a caret at block 2 by index, apply style Heading1: block 2's place and its style (the
//!   colleague's restyle would be overwritten unseen).
//! - F8 the rectangle by its number, fill: the rectangle.
//! - F9 the comment thread by its id, reply: the thread.
//!
//! Changed after the first run: F1 × restyle, `A` → `R:changed`. Written before the run as `A`,
//! it contradicted F3's rule (an italic toggle reads the paragraph's style as a bold toggle does).
//!
//! C8-doc-3 adds C15 and C16, accept and reject a tracked change overlapping the range: their
//! fixture has a tracked insertion `Z` at block 2 offset 7 (inside `five`), made with tracking
//! on and then off; the other program or agent accepts or rejects every change. Written before
//! their first run: accepting changes what the characters inside the range are (proposed to
//! accepted, a mark of theirs), so the forms resting on those characters' marks refuse
//! (F1, F3, F4: `changed`); rejecting takes `Z` out of the range (F3, F4: `changed`; F1 holds
//! which formats the characters carry, which a removal leaves as they were: applies); the forms
//! resting on block 2's place, its properties or other objects apply.
use super::outside::{bold, by_id, by_index};
use super::*;

const CHANGES: [&str; 16] = [
    "type before",
    "type inside",
    "type after",
    "Enter before",
    "Enter inside",
    "Enter at 0",
    "Enter at end",
    "delete paragraph",
    "move paragraph",
    "join with previous",
    "format range",
    "restyle",
    "row inserted above",
    "table moved",
    "accept tracked inside",
    "reject tracked inside",
];

const FORMS: [&str; 9] = [
    "F1 on id",
    "F2 paragraph index",
    "F3 {para,off}",
    "F4 cross-paragraph",
    "F5 cell path",
    "F6 selection",
    "F7 style name",
    "F8 drawing id",
    "F9 comment id",
];

/// Rows: forms; columns: changes (see the module docs).
const EXPECT: [[&str; 16]; 9] = [
    ["A", "A", "A", "A", "A", "A", "A", "R:deleted", "A", "R:deleted", "R:changed", "R:changed", "A", "A", "R:changed", "A"],
    ["A", "A", "A", "R:relocated", "A", "R:relocated", "A", "R", "R:relocated", "R", "A", "R:changed", "A", "A", "A", "A"],
    [
        "R:relocated",
        "R:changed",
        "A",
        "A",
        "R:changed",
        "A",
        "A",
        "R:deleted",
        "A",
        "R:deleted",
        "R:changed",
        "R:changed",
        "A",
        "A",
        "R:changed",
        "R:changed",
    ],
    [
        "R:changed",
        "R:changed",
        "A",
        "R:relocated",
        "R:changed",
        "R",
        "A",
        "R",
        "R",
        "R",
        "R:changed",
        "R:changed",
        "A",
        "A",
        "R:changed",
        "R:changed",
    ],
    [
        "A",
        "A",
        "A",
        "R:relocated",
        "R:relocated",
        "R:relocated",
        "R:relocated",
        "R:relocated",
        "A",
        "R:relocated",
        "A",
        "A",
        "R:relocated",
        "R:relocated",
        "A",
        "A",
    ],
    ["A", "A", "A", "A", "A", "A", "A", "A", "R:relocated", "A", "A", "A", "A", "A", "A", "A"],
    ["A", "A", "A", "R:relocated", "A", "R:relocated", "A", "R", "R:relocated", "R", "A", "R:changed", "A", "A", "A", "A"],
    ["A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A"],
    ["A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A"],
];

struct Base {
    bytes: Vec<u8>,
    uid: u64,
    thread: String,
}

fn base(tracked: bool) -> Base {
    let mut doc = document();
    if tracked {
        // A tracked insertion `Z` inside `five` (block 2, offset 7); tracking off again.
        doc.execute("select.range", json!({ "anchor": { "block": 2, "off": 7 }, "focus": { "block": 2, "off": 7 } })).unwrap();
        doc.execute("review.trackChanges", json!({ "value": true })).unwrap();
        doc.execute("text.insert", json!({ "text": "Z" })).unwrap();
        doc.execute("review.trackChanges", json!({ "value": false })).unwrap();
    }
    let caret = |b: usize, off: usize| json!({ "anchor": { "block": b, "off": off }, "focus": { "block": b, "off": off } });
    doc.execute("select.range", caret(4, TEXTS[4].len())).unwrap();
    doc.execute("text.newParagraph", json!({})).unwrap();
    doc.execute("insert.table", json!({ "rows": 2, "cols": 2 })).unwrap();
    for (r, c) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
        let at = json!({ "story": "body", "path": [5, r, c, 0], "off": 0 });
        doc.execute("select.range", json!({ "anchor": at, "focus": at })).unwrap();
        doc.execute("text.insert", json!({ "text": format!("Cell {r}{c}") })).unwrap();
    }
    doc.execute("select.range", caret(4, 0)).unwrap();
    let shape = doc.execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "w": 50, "h": 20 } })).unwrap();
    let uid = shape["id"].as_str().and_then(|id| id.parse().ok()).or_else(|| shape["id"].as_u64()).unwrap();
    doc.execute("select.range", json!({ "anchor": { "block": 3, "off": 0 }, "focus": { "block": 3, "off": 5 } })).unwrap();
    let added = doc.execute("comment.add", json!({ "text": "Look" })).unwrap();
    let thread = added["thread"].as_str().unwrap().to_owned();
    Base { bytes: doc.save(None).unwrap(), uid, thread }
}

fn change(kind: usize) -> Vec<(&'static str, Value)> {
    let sel = |a: (usize, usize), b: (usize, usize)| {
        ("select.range", json!({ "anchor": { "block": a.0, "off": a.1 }, "focus": { "block": b.0, "off": b.1 } }))
    };
    let caret = |b: usize, off: usize| sel((b, off), (b, off));
    let enter = |b: usize, off: usize| vec![caret(b, off), ("text.newParagraph", json!({}))];
    let (l1, l2) = (TEXTS[1].len(), TEXTS[2].len());
    match kind {
        0 => vec![caret(2, 0), ("text.insert", json!({ "text": "Z" }))],
        1 => vec![caret(2, 7), ("text.insert", json!({ "text": "Z" }))],
        2 => vec![caret(2, l2), ("text.insert", json!({ "text": "Z" }))],
        3 => enter(1, l1),
        4 => enter(2, 8),
        5 => enter(2, 0),
        6 => enter(2, l2),
        7 => vec![sel((1, l1), (2, l2)), ("text.delete", json!({}))],
        8 => vec![sel((2, 0), (3, 0)), ("edit.cut", json!({})), caret(0, 0), ("edit.paste", json!({}))],
        9 => vec![caret(2, 0), ("text.backspace", json!({}))],
        10 => vec![sel((2, 6), (2, 10)), ("format.bold", json!({}))],
        11 => vec![caret(2, 0), ("para.style", json!({ "style": "Heading2" }))],
        12 => {
            let at = json!({ "story": "body", "path": [5, 0, 0, 0], "off": 0 });
            vec![("select.range", json!({ "anchor": at, "focus": at })), ("table.insertRowAbove", json!({}))]
        }
        13 => vec![caret(4, TEXTS[4].len() + 3), ("text.newParagraph", json!({}))],
        14 => vec![("review.acceptAll", json!({}))],
        _ => vec![("review.rejectAll", json!({}))],
    }
}

fn write(form: usize, ids: &[(Value, Value)], base: &Base) -> Value {
    let id = &ids[2].0;
    match form {
        0 => json!([{ "command": "format.italic", "params": {}, "on": [id] }]),
        1 => json!([by_index(2, 0, 0), { "command": "para.alignCenter", "params": {} }]),
        2 => json!([by_id(id, 6, 10), bold()]),
        3 => json!([{ "command": "select.range", "params": { "anchor": { "block": 1, "off": 5 }, "focus": { "block": 2, "off": 10 } } }, bold()]),
        4 => {
            let at = |off: usize| json!({ "story": "body", "path": [5, 0, 1, 0], "off": off });
            json!([{ "command": "select.range", "params": { "anchor": at(0), "focus": at(4) } }, bold()])
        }
        5 => json!([{ "command": "text.insert", "params": { "text": "X" } }]),
        6 => json!([by_index(2, 0, 0), { "command": "para.style", "params": { "style": "Heading1" } }]),
        7 => json!([{ "command": "drawing.format", "params": { "ids": [base.uid], "fill": "#FF0000" } }]),
        _ => json!([{ "command": "comment.reply", "params": { "thread": base.thread, "text": "Reply" } }]),
    }
}

/// One cell's outcome: `A`, `R:<reasons>`, or `F:<error>` (failed at the kit).
async fn cell(base: &Base, form: usize, kind: usize, outside: bool) -> String {
    let f = Fixture::with(Guard::Premise, &base.bytes);
    let ids = f.blocks().await;
    let basis = f.basis().await;
    if outside {
        f.colleague(&change(kind));
    } else {
        let steps: Vec<Value> = change(kind).into_iter().map(|(command, params)| json!({ "command": command, "params": params })).collect();
        let answer = tools::call_as(&f.kits, "doc_run", &json!({ "path": FILE, "steps": steps }), f.options, "c8-colleague").await;
        assert!(matches!(answer, Some(Ok(_))), "colleague {}: {answer:?}", CHANGES[kind]);
    }
    let result = f.run(Some(basis), write(form, &ids, base)).await;
    if std::env::var("C8_CELL").is_ok_and(|cell| cell == format!("{form},{kind},{outside}")) {
        println!("CELL {result:?}");
    }
    f.close().await;
    match result {
        Ok(_) => "A".to_owned(),
        Err(error) if error["error"]["code"] == "conflict" => {
            let mut reasons: Vec<String> =
                error["conflicts"].as_array().into_iter().flatten().filter_map(|c| c["reason"].as_str()).map(str::to_owned).collect();
            reasons.sort();
            reasons.dedup();
            format!("R:{}", reasons.join(","))
        }
        Err(error) => format!("F:{}", error["error"]["message"]),
    }
}

#[tokio::test]
async fn the_matrix() {
    let (plain, tracked) = (base(false), base(true));
    let (mut wrong, mut false_, mut wrong_reason, mut failed, mut cells) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0usize);
    let only = std::env::var("C8_CELL").ok();
    for outside in [false, true] {
        for (form, form_name) in FORMS.iter().enumerate() {
            for (kind, change_name) in CHANGES.iter().enumerate() {
                if only.as_ref().is_some_and(|cell| *cell != format!("{form},{kind},{outside}")) {
                    continue;
                }
                let base = if kind >= 14 { &tracked } else { &plain };
                let got = cell(base, form, kind, outside).await;
                cells += 1;
                let want = EXPECT[form][kind];
                let at = format!("{form_name} × {change_name} × {}: want {want}, got {got}", if outside { "outside" } else { "same server" });
                match (want, got.as_str()) {
                    (_, got) if got.starts_with("F:") => failed.push(at),
                    ("A", "A") => {}
                    ("A", _) => false_.push(at),
                    (_, "A") => wrong.push(at),
                    (want, got) => {
                        if let Some(reason) = want.strip_prefix("R:")
                            && !got.trim_start_matches("R:").split(',').any(|r| r == reason)
                        {
                            wrong_reason.push(at);
                        }
                    }
                }
            }
        }
    }
    println!("matrix: {cells} cells; wrong {}, false {}, wrong reason {}, failed {}", wrong.len(), false_.len(), wrong_reason.len(), failed.len());
    for (name, list) in [("WRONG", &wrong), ("FALSE", &false_), ("WRONG-REASON", &wrong_reason), ("FAILED", &failed)] {
        for at in list.iter() {
            println!("{name} {at}");
        }
    }
    // Measured at C8-doc-2's end (at e2439937, before it: wrong 16, false 19, wrong reason 24;
    // C8-doc-3: wrong 0, the two join cells refused; the 36 cells of C15 and C16 add two false
    // refusals, F1 × reject both ways, and none else; C8-doc-4: false 12, the commands that read no
    // selection no longer hold it;
    // C8-doc-refusals: false 2, wrong reason 3, all waiting on the SDK):
    // what is left only falls. The cells left are in the card's report and the agent guide.
    if only.is_none() {
        assert!(wrong.is_empty() && false_.len() <= 2 && wrong_reason.len() <= 3 && failed.is_empty(), "the matrix got worse");
    }
}

/// C8-doc-refusals: removing a proposed insertion leaves the surviving formats
/// intact. Accepting it, or changing those formats, must still protect the peer.
#[tokio::test]
async fn character_formats_survive_rejected_insertion() {
    let base = base(true);
    for outside in [false, true] {
        for kind in [10, 14, 15] {
            let f = Fixture::with(Guard::Premise, &base.bytes);
            let ids = f.blocks().await;
            let basis = f.basis().await;
            peer_change(&f, kind, outside).await;
            let before = std::fs::read(f.root.join(FILE)).unwrap();
            let result = f.run(Some(basis), write(0, &ids, &base)).await;
            if kind == 15 {
                result.unwrap_or_else(|error| panic!("rejected insertion, outside {outside}: {error}"));
                let p = f.disk().execute("document.paragraph", json!({ "path": [2] })).unwrap();
                assert_eq!(p["paragraph"]["text"], TEXTS[2]);
                assert!(p["paragraph"]["runs"].as_array().unwrap().iter().all(|run| run["props"]["italic"] == true), "{p}");
            } else {
                conflict(&result.unwrap_err(), "changed");
                assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
            }
            f.close().await;
        }
    }
}

/// A joined paragraph no longer exists in the editor, even while its replica
/// piece remains. A range by its id reports deletion; edits after a live range merge.
#[tokio::test]
async fn missing_id_ranges_report_deletion() {
    let base = base(false);
    for outside in [false, true] {
        for kind in [7, 9, 2] {
            let f = Fixture::with(Guard::Premise, &base.bytes);
            let ids = f.blocks().await;
            let basis = f.basis().await;
            peer_change(&f, kind, outside).await;
            let before = std::fs::read(f.root.join(FILE)).unwrap();
            let result = f.run(Some(basis), write(2, &ids, &base)).await;
            if kind == 2 {
                result.unwrap();
                let p = f.disk().execute("document.paragraph", json!({ "path": [2] })).unwrap();
                assert_eq!(p["paragraph"]["text"], format!("{}Z", TEXTS[2]));
                assert_eq!(bold_spans(&p), [(6, 10)], "{p}");
            } else {
                conflict(&result.unwrap_err(), "deleted");
                assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
            }
            f.close().await;
        }
    }
}

async fn peer_change(f: &Fixture, kind: usize, outside: bool) {
    if outside {
        f.colleague(&change(kind));
    } else {
        let steps: Vec<Value> = change(kind).into_iter().map(|(command, params)| json!({ "command": command, "params": params })).collect();
        let result = tools::call_as(&f.kits, "doc_run", &json!({ "path": FILE, "steps": steps }), f.options, "c8-colleague").await;
        assert!(matches!(result, Some(Ok(_))), "{result:?}");
    }
}

#[tokio::test]
async fn cell_paths_report_join_relocation() {
    let base = base(false);
    for outside in [false, true] {
        for kind in [3, 9, 2, 8] {
            let f = Fixture::with(Guard::Premise, &base.bytes);
            let ids = f.blocks().await;
            let basis = f.basis().await;
            peer_change(&f, kind, outside).await;
            let before = std::fs::read(f.root.join(FILE)).unwrap();
            let result = f.run(Some(basis), write(4, &ids, &base)).await;
            if kind == 2 || kind == 8 {
                result.unwrap();
                let p = f.disk().execute("document.paragraph", json!({ "path": [5, 0, 1, 0] })).unwrap();
                assert_eq!(bold_spans(&p), [(0, 4)], "{p}");
                if kind == 2 {
                    assert_eq!(f.texts()[2], format!("{}Z", TEXTS[2]));
                } else {
                    assert_eq!(f.texts()[0], TEXTS[2]);
                }
            } else {
                conflict(&result.unwrap_err(), "relocated");
                assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), before);
            }
            f.close().await;
        }
    }
}

fn bold_spans(paragraph: &Value) -> Vec<(usize, usize)> {
    let mut offset = 0;
    paragraph["paragraph"]["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|run| {
            let start = offset;
            offset += run["len"].as_u64().unwrap() as usize;
            (run["props"]["bold"] == true).then_some((start, offset))
        })
        .collect()
}
