//! Item 6: the fixed-seed tables. Another program really rewrites the file between the agent's
//! observation (`basis`) and its write.
//!
//! **When a refusal is necessary** (written before the tables were first run; never changed to
//! fit a result). The target is body block 2 at basis, `Gamma five six`; the range is characters
//! `[6, 10)` (`five`). "Its text" is compared in characters; "marks" are each character's
//! formatting as the saved file reads it. Computed from the file before and after the other
//! program's save:
//! - W1 by id (`{para: <id of block 2>}`, `[6,10)`, then bold): necessary when no paragraph has
//!   that id any more, or its characters `[0,10)` or the marks of `[5,10)` differ.
//! - W2 by index (`{block: 2}`, `[6,10)`, then bold): necessary when block 2 is another paragraph
//!   (another id) or none, or as W1 for the paragraph at block 2.
//! - W3 at a point in the text (a caret at block 2, offset 6, then typing `+`): necessary when
//!   block 2 is another paragraph or none, or its characters `[0,6)` or the mark of character 5
//!   differ.
//! - W4 structural (`paragraph.insert` after block 2): necessary when block 2 is another paragraph or
//!   none, or its paragraph properties differ (the new paragraph takes them).
//! - W5 replace by index (`paragraph.text` of block 2): necessary when block 2 is another paragraph or
//!   none, or any of its characters differ (the write replaces them all).
//!
//! A write is then **applied**, **refused necessary**, **refused false** (refused, not
//! necessary), **applied wrongly** (applied, necessary) or **failed at kit** (an error other than
//! a conflict). Wanted: no write applied wrongly and none refused falsely.
use super::outside::{bold, by_id, by_index, type_at};
use super::*;

const SEED: u64 = 0x9e37_79b9_7f4a_7c15;
const ROUNDS: usize = 50;
const RANGE: (usize, usize) = (6, 10);

/// splitmix64.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// The other program's changes.
const KINDS: [&str; 12] = [
    "type in another paragraph",
    "type before the range",
    "type inside the range",
    "type after the range",
    "paragraph inserted before the target",
    "paragraph inserted after the target",
    "target deleted",
    "another paragraph bolded",
    "the range bolded",
    "paragraph above deleted",
    "target formatted before the range",
    "target centred",
];

fn change(kind: usize) -> Vec<(&'static str, Value)> {
    let sel = |a: (usize, usize), b: (usize, usize)| {
        ("select.range", json!({ "anchor": { "block": a.0, "off": a.1 }, "focus": { "block": b.0, "off": b.1 } }))
    };
    match kind {
        0 => type_at(0, TEXTS[0].len(), "!"),
        1 => type_at(2, 0, "Z"),
        2 => type_at(2, 7, "Z"),
        3 => type_at(2, TEXTS[2].len(), "Z"),
        4 => vec![("paragraph.insert", json!({ "before": 2, "text": "Inserted" }))],
        5 => vec![("paragraph.insert", json!({ "after": 3, "text": "After" }))],
        6 => vec![sel((1, TEXTS[1].len()), (2, TEXTS[2].len())), ("text.delete", json!({}))],
        7 => vec![sel((4, 0), (4, 5)), ("format.bold", json!({}))],
        8 => vec![sel((2, 6), (2, 10)), ("format.bold", json!({}))],
        9 => vec![sel((0, 0), (1, 0)), ("text.delete", json!({}))],
        10 => vec![sel((2, 0), (2, 2)), ("format.italic", json!({}))],
        _ => vec![sel((2, 0), (2, 0)), ("format.alignCenter", json!({}))],
    }
}

const FORMS: [&str; 5] = ["W1 by id", "W2 by index", "W3 at a point", "W4 structural", "W5 replace by index"];

/// One body paragraph as the file reads it: its id, characters, each character's marks, and its
/// paragraph properties.
#[derive(Clone, PartialEq, Debug)]
struct Para {
    id: String,
    chars: Vec<char>,
    marks: Vec<String>,
    props: String,
}

fn paragraphs(doc: &mut DocDocument) -> Vec<Para> {
    let blocks = doc.inspect().unwrap()["blocks"].clone();
    let mut out = Vec::new();
    for (i, block) in blocks.as_array().unwrap().iter().enumerate() {
        let answer = doc.execute("file.paragraph", json!({ "path": [i] })).unwrap();
        let p = &answer["paragraph"];
        let text = p["text"].as_str().unwrap_or_default().to_owned();
        let mut marks = Vec::new();
        let mut at = 0usize;
        for run in p["runs"].as_array().into_iter().flatten() {
            let len = run["len"].as_u64().unwrap_or(0) as usize;
            let end = (at + len).min(text.len());
            let count = text.get(at..end).map_or(0, |s| s.chars().count());
            marks.extend(std::iter::repeat_n(run["props"].to_string(), count));
            at = end;
        }
        let mut props = p["props"].clone();
        if let Some(map) = props.as_object_mut() {
            map.remove("paraId");
        }
        out.push(Para { id: block["id"].as_str().unwrap_or_default().to_owned(), chars: text.chars().collect(), marks, props: props.to_string() });
    }
    out
}

fn necessary(form: usize, before: &[Para], after: &[Para]) -> bool {
    let target = &before[2];
    let (s, e) = RANGE;
    let held = |p: &Para, chars: usize, marks: std::ops::Range<usize>| {
        p.chars.get(..chars) != target.chars.get(..chars) || marks.into_iter().any(|i| p.marks.get(i) != target.marks.get(i))
    };
    let at_index = after.get(2).filter(|p| p.id == target.id);
    match form {
        0 => after.iter().find(|p| p.id == target.id).is_none_or(|p| held(p, e, s - 1..e)),
        1 => at_index.is_none_or(|p| held(p, e, s - 1..e)),
        2 => at_index.is_none_or(|p| held(p, s, s - 1..s)),
        3 => at_index.is_none_or(|p| p.props != target.props),
        _ => at_index.is_none_or(|p| p.chars != target.chars),
    }
}

fn write(form: usize, id: &Value) -> Value {
    let (s, e) = RANGE;
    match form {
        0 => json!([by_id(id, s, e), bold()]),
        1 => json!([by_index(2, s, e), bold()]),
        2 => json!([by_index(2, s, s), { "command": "text.insert", "params": { "text": "+" } }]),
        3 => json!([{ "command": "paragraph.insert", "params": { "after": 2, "text": "Mine" } }]),
        _ => json!([{ "command": "paragraph.text", "params": { "index": 2, "text": "Replaced" } }]),
    }
}

#[derive(Default, Clone, Copy)]
struct Counts {
    applied: usize,
    necessary: usize,
    false_refusal: usize,
    wrong: usize,
    failed: usize,
}

#[tokio::test]
async fn the_mixed_table_applies_nothing_wrongly_and_refuses_nothing_falsely() {
    let mut rng = Rng(SEED);
    let mut table = vec![vec![Counts::default(); FORMS.len()]; KINDS.len()];
    let mut notes = Vec::new();
    for round in 0..ROUNDS {
        let (kind, form) = (rng.below(KINDS.len()), rng.below(FORMS.len()));
        let f = Fixture::new(Guard::Premise);
        let before = paragraphs(&mut f.disk());
        let id = json!(before[2].id);
        let basis = f.basis().await;
        f.colleague(&change(kind));
        let after = paragraphs(&mut f.disk());
        let needed = necessary(form, &before, &after);
        let result = f.run(Some(basis), write(form, &id)).await;
        let cell = &mut table[kind][form];
        match (&result, needed) {
            (Ok(_), false) => cell.applied += 1,
            (Ok(_), true) => {
                cell.wrong += 1;
                notes.push(format!("round {round}: {} / {}: applied wrongly", KINDS[kind], FORMS[form]));
            }
            (Err(error), needed) if error["error"]["code"] == "conflict" => {
                if needed {
                    cell.necessary += 1;
                } else {
                    cell.false_refusal += 1;
                    notes.push(format!("round {round}: {} / {}: refused falsely: {}", KINDS[kind], FORMS[form], error["conflicts"]));
                }
            }
            (Err(error), _) => {
                cell.failed += 1;
                notes.push(format!("round {round}: {} / {}: failed at kit: {}", KINDS[kind], FORMS[form], error["error"]));
            }
        }
        f.close().await;
    }
    println!("| colleague change | {} |", FORMS.join(" | "));
    println!("|---|{}", "---|".repeat(FORMS.len()));
    let mut total = Counts::default();
    for (k, row) in table.iter().enumerate() {
        let cells: Vec<String> = row
            .iter()
            .map(|c| {
                total.applied += c.applied;
                total.necessary += c.necessary;
                total.false_refusal += c.false_refusal;
                total.wrong += c.wrong;
                total.failed += c.failed;
                if c.applied + c.necessary + c.false_refusal + c.wrong + c.failed == 0 {
                    "-".to_owned()
                } else {
                    format!("A{} R{} F{} W{} X{}", c.applied, c.necessary, c.false_refusal, c.wrong, c.failed)
                }
            })
            .collect();
        println!("| {} | {} |", KINDS[k], cells.join(" | "));
    }
    println!(
        "total: applied {}, refused necessary {}, refused false {}, applied wrongly {}, failed at kit {}",
        total.applied, total.necessary, total.false_refusal, total.wrong, total.failed
    );
    for note in &notes {
        println!("{note}");
    }
    assert_eq!((total.wrong, total.false_refusal), (0, 0), "{}", notes.join("\n"));
}

#[tokio::test]
async fn fifty_unrelated_changes_premise_refuses_none_epoch_refuses_all() {
    // Changes no write by index to block 2's `[6,10)` rests on.
    let unrelated = [0usize, 3, 5, 7, 10];
    for (guard, refused_wanted) in [(Guard::Premise, 0usize), (Guard::Epoch, ROUNDS)] {
        let mut rng = Rng(SEED);
        let mut refused = 0usize;
        let mut failed = Vec::new();
        for round in 0..ROUNDS {
            let kind = unrelated[rng.below(unrelated.len())];
            let f = Fixture::new(guard);
            let basis = f.basis().await;
            f.colleague(&change(kind));
            match f.run(Some(basis), json!([by_index(2, RANGE.0, RANGE.1), bold()])).await {
                Ok(_) => {}
                Err(error) if error["error"]["code"] == "conflict" => refused += 1,
                Err(error) => failed.push(format!("round {round} {}: {}", KINDS[kind], error["error"])),
            }
            f.close().await;
        }
        println!("{guard:?}: refused {refused}/{ROUNDS}; failed {failed:?}");
        assert_eq!(refused, refused_wanted, "{guard:?}");
        assert!(failed.is_empty(), "{failed:?}");
    }
}

/// Every colleague change against every write form once (the seeded table above samples 50 of
/// these 60 cells), with the same rule.
#[tokio::test]
async fn every_change_against_every_write_form() {
    let mut notes = Vec::new();
    let mut rows = Vec::new();
    for (kind, name) in KINDS.iter().enumerate() {
        let mut cells = Vec::new();
        for (form, form_name) in FORMS.iter().enumerate() {
            let f = Fixture::new(Guard::Premise);
            let before = paragraphs(&mut f.disk());
            let id = json!(before[2].id);
            let basis = f.basis().await;
            f.colleague(&change(kind));
            let after = paragraphs(&mut f.disk());
            let needed = necessary(form, &before, &after);
            let result = f.run(Some(basis), write(form, &id)).await;
            let mark = match (&result, needed) {
                (Ok(_), false) => "A",
                (Ok(_), true) => "W",
                (Err(error), true) if error["error"]["code"] == "conflict" => "R",
                (Err(error), false) if error["error"]["code"] == "conflict" => "F",
                (Err(_), _) => "X",
            };
            if matches!(mark, "W" | "F" | "X") {
                notes.push(format!("{name} / {form_name}: {mark} {:?}", result.err().map(|e| e["conflicts"].clone())));
            }
            cells.push(mark);
            f.close().await;
        }
        rows.push(format!("| {name} | {} |", cells.join(" | ")));
    }
    println!("| colleague change | {} |\n|---|{}\n{}", FORMS.join(" | "), "---|".repeat(FORMS.len()), rows.join("\n"));
    assert!(notes.iter().all(|note| !note.contains(": W") && !note.contains(": F")), "{}", notes.join("\n"));
    println!("{}", notes.join("\n"));
}
