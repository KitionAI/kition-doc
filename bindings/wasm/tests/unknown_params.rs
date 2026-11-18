//! A command refuses a parameter its schema does not name, saying which, instead of running
//! without it (engine rule E7a, kit requirement AX15).
#![allow(clippy::unwrap_used, clippy::panic)]

use kition_doc_kit::DocDocument;
use kition_kit::KitDocument;
use serde_json::json;

fn document() -> DocDocument {
    DocDocument::create("Document.docx", 64, 36).unwrap()
}

#[test]
fn every_command_refuses_a_parameter_it_does_not_take() {
    if let Err(problems) = kition_kit::testing::check_unknown_params(document, &[]) {
        panic!("{} commands:\n{}", problems.len(), problems.join("\n"));
    }
}

#[test]
fn a_misspelled_parameter_names_the_one_it_resembles_and_changes_nothing() {
    let mut doc = document();
    doc.execute("text.insert", json!({ "text": "Hello" })).unwrap();
    let before = doc.save(None).unwrap();
    let error = doc.execute("text.insert", json!({ "txet": "!" })).unwrap_err();
    assert!(error.contains("`txet`") && error.contains("did you mean `text`"), "{error}");
    assert_eq!(doc.save(None).unwrap(), before);
}
