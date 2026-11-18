//! Reproducible saves: with `SOURCE_DATE_EPOCH` set, a headless build
//! stamps saved documents with that time instead of the clock, so saving the same document twice
//! writes the same bytes.
//!
//! A test cannot set an environment variable in its own process (that is `unsafe`), so the
//! outer test runs this test binary again with the variable set and only the inner test selected.

use std::process::Command;

use kition_doc_engine::{Session, doc::Document, io};
use serde_json::json;

const EPOCH: &str = "1700000000";
const EPOCH_ISO: &str = "2023-11-14T22:13:20Z";

/// A new document with some text, saved the way the kit saves it (`KitDocument::save`).
fn saved_new_document() -> Vec<u8> {
    let mut session = Session::new(Document::new());
    session.run("text.insert", &json!({"text": "Quarterly report"})).unwrap();
    let doc = session.save_stamped();
    io::save_bytes("report.docx", &doc).unwrap()
}

fn core_xml(bytes: &[u8]) -> String {
    let doc = io::open_bytes("report.docx", bytes).unwrap();
    format!("{} {}", doc.core.created, doc.core.modified)
}

#[test]
fn saves_with_source_date_epoch_are_the_same_bytes() {
    if std::env::var("SOURCE_DATE_EPOCH").as_deref() == Ok(EPOCH) {
        let first = saved_new_document();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let second = saved_new_document();
        assert_eq!(core_xml(&first), format!("{EPOCH_ISO} {EPOCH_ISO}"), "created and modified are SOURCE_DATE_EPOCH");
        assert!(first == second, "two saves of the same new document a second apart are the same bytes");
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let out = Command::new(exe)
        .args(["--exact", "saves_with_source_date_epoch_are_the_same_bytes", "--nocapture", "--test-threads=1"])
        .env("SOURCE_DATE_EPOCH", EPOCH)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"), "the inner test ran: {}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn source_date_epoch_must_be_whole_seconds() {
    use kition_doc_engine::doc::parse_source_date_epoch;
    assert_eq!(parse_source_date_epoch("1700000000"), Some(1_700_000_000));
    assert_eq!(parse_source_date_epoch(" 0 "), Some(0));
    for junk in ["", "-5", "1.5", "soon", "99999999999999999999999"] {
        assert_eq!(parse_source_date_epoch(junk), None, "{junk}");
    }
}
