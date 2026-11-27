//! `file.identity` and `file.fork` (Kit Process Protocol section 5.6, KitModel M7a)
//! through the real `kition-doc` process: a `.docx` is written from the engine's document
//! (`kition_doc_engine::io::save_bytes`) and embeds no replica, so it carries no document id;
//! opening it makes a new replica each time. A host learns that without parsing the file, and a
//! fork of a document is the same bytes, with no id.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use kition_doc_kit::DocDocument;
use kition_kit::KitDocument;
use kition_kit_host::{HeadlessKit, LaunchOptions};
use serde_json::json;

const LIMIT: Duration = Duration::from_secs(30);

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-doc-identity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

async fn launch(dir: &Path) -> HeadlessKit {
    let mut options = LaunchOptions::new(env!("CARGO_BIN_EXE_kition-doc"));
    options.workspace = Some(dir.to_path_buf());
    HeadlessKit::launch(options).await.unwrap()
}

/// The conformance document, edited and saved by the kit.
fn document() -> Vec<u8> {
    let basic = std::fs::read(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.docx"))).unwrap();
    let mut doc = DocDocument::open("Plan.docx", &basic).unwrap();
    doc.execute("text.insert", json!({ "text": "Typed before the fork. " })).unwrap();
    doc.save(Some("Plan.docx")).unwrap()
}

#[tokio::test]
async fn a_word_file_embeds_no_replica_so_it_carries_no_identity_and_forks_unchanged() {
    let dir = workspace("plain");
    let bytes = document();
    std::fs::write(dir.join("Plan.docx"), &bytes).unwrap();
    let kit = launch(&dir).await;
    assert!(kit.serves("file.identity"), "{}", kit.capabilities());

    // A document, a byte copy of it and an empty file (made for the kit before it saved one)
    // carry no identity: the file holds the document, not its replica.
    std::fs::copy(dir.join("Plan.docx"), dir.join("Copy.docx")).unwrap();
    std::fs::write(dir.join("New.docx"), b"").unwrap();
    for path in ["Plan.docx", "Copy.docx", "New.docx"] {
        assert_eq!(kit.identity(path, LIMIT).await.unwrap().doc_id, None, "{path}");
    }

    // A fork is the same bytes, with no id, and leaves the source as it was.
    let fork = kit.fork("Plan.docx", "Fork.docx", LIMIT).await.unwrap();
    assert_eq!(fork.doc_id, None);
    assert_eq!(fork.name, "Fork.docx");
    assert!(!fork.version.is_empty());
    assert_eq!(std::fs::read(dir.join("Fork.docx")).unwrap(), bytes, "a file without a replica forks unchanged");
    assert_eq!(std::fs::read(dir.join("Plan.docx")).unwrap(), bytes, "the source is unchanged");
    DocDocument::open("Fork.docx", &std::fs::read(dir.join("Fork.docx")).unwrap()).unwrap();
    let empty = kit.fork("New.docx", "NewCopy.docx", LIMIT).await.unwrap();
    assert_eq!(empty.doc_id, None);
    assert_eq!(std::fs::read(dir.join("NewCopy.docx")).unwrap(), b"");

    // A fork never replaces a file.
    std::fs::write(dir.join("Taken.docx"), b"someone else's file").unwrap();
    let taken = kit.fork("Plan.docx", "Taken.docx", LIMIT).await.unwrap_err();
    assert_eq!(taken.reason(), Some("file_conflict"), "{taken}");
    assert_eq!(std::fs::read(dir.join("Taken.docx")).unwrap(), b"someone else's file", "the existing file is kept");
    kit.shutdown(Duration::from_secs(5)).await;
    let _ = std::fs::remove_dir_all(dir);
}
