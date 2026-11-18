//! Round trips: a document written to a replica reads back with the same content (ids aside),
//! for the web editor's sample, a document exercising every replicated reference (directly and
//! through `.docx`), and (when `KITION_DOC_CORPUS=<dir>` is set) up to 40 `.docx` files of a
//! local corpus.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, assert_same, new_replica, rich};
use kdm_model::{DocModel, decode, encode, read_document, write_changes};
use kition_doc_doc::{Block, Document};
use kition_kit::kdm::ActorId;
use kition_kit::session::{KitModel, KitSession};

/// A copy of `blocks` sharing no `Arc`, so every block is compared.
fn deep(blocks: &[Arc<Block>]) -> Vec<Arc<Block>> {
    blocks.iter().map(|b| Arc::new((**b).clone())).collect()
}

/// `doc` through `import`'s path (write_document in a creating transaction) and back. Also checks
/// that diffing the read view against a deep copy of itself writes nothing.
fn round_trip(doc: &Document) -> Document {
    let blobs = SharedBlobs::default();
    let (mut replica, _) = new_replica(doc, &blobs);
    replica.take_changed();
    let (read, mut ids) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    let mut copy = read.clone();
    copy.body = deep(&copy.body);
    for part in copy.parts.values_mut() {
        part.blocks = deep(&part.blocks);
    }
    let mut sink = blobs.clone();
    let wrote = replica
        .transact(&common::person("t"), |tx| write_changes(tx, &read, &copy, &mut ids, &mut sink))
        .unwrap_or_else(|error| panic!("write_changes: {error}"));
    let changed: Vec<String> = replica.take_changed().iter().map(|o| format!("{} {}", replica.object_type(o).unwrap_or_default(), o)).collect();
    assert!(!wrote, "an unchanged view wrote changes: {changed:?}");
    read
}

#[test]
fn document_grid_metadata_survives_the_replica() {
    let mut document = Document::from_text("Grid settings");
    document.last_section.grid =
        Some(kition_doc_doc::section::DocumentGrid { kind: Some("linesAndChars".into()), line_pitch: Some(312), char_space: Some(-12) });
    let read = round_trip(&document);
    assert_eq!(read.last_section.grid, document.last_section.grid);
}

#[test]
fn rich_document_round_trips() {
    let doc = rich();
    let read = round_trip(&doc);
    assert_same("rich", &doc, &read);
    // What serde skips came back too.
    let ole = read
        .body
        .iter()
        .filter_map(|b| b.as_para())
        .flat_map(|p| &p.objects)
        .any(|o| matches!(o, kition_doc_doc::InlineObject::Image { ole: Some(e), .. } if e.refs.len() == 1 && e.xml == "<w:object/>"));
    assert!(ole, "the picture's OLE object survived");
    assert_eq!(read.passthrough.get("word/embeddings/x.bin").map(|b| b.as_slice()), Some(&[1u8, 2, 3][..]));
}

#[test]
fn rich_document_round_trips_through_docx() {
    let bytes = encode("Rich.docx", &rich()).unwrap_or_else(|error| panic!("encode: {error}"));
    let original = decode("Rich.docx", &bytes).unwrap_or_else(|error| panic!("decode: {error}"));
    assert!(!original.parts.is_empty() && !original.comments.is_empty() && !original.revisions.is_empty());
    assert_same("rich.docx", &original, &round_trip(&original));
}

#[test]
fn sample_round_trips() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../web/public/sample/Sample.docx"))
        .unwrap_or_else(|error| panic!("sample: {error}"));
    let original = decode("Sample.docx", &bytes).unwrap_or_else(|error| panic!("decode: {error}"));
    assert!(original.body.len() > 1);
    assert_same("Sample.docx", &original, &round_trip(&original));
}

#[test]
fn import_and_export_through_a_session() {
    let bytes = encode("Rich.docx", &rich()).unwrap_or_else(|error| panic!("encode: {error}"));
    let session: KitSession<DocModel> =
        KitSession::import("Rich.docx", &bytes, Box::new(SharedBlobs::default()), ActorId::from_u64(7).unwrap_or_else(|_| panic!("actor")))
            .unwrap_or_else(|error| panic!("import: {error}"));
    let exported = session.export("Rich.docx").unwrap_or_else(|error| panic!("export: {error}"));
    let again = decode("Rich.docx", &exported).unwrap_or_else(|error| panic!("decode: {error}"));
    assert_same("export", &decode("Rich.docx", &bytes).unwrap_or_else(|error| panic!("decode: {error}")), &again);
    let text = session.export("Rich.txt").unwrap_or_else(|error| panic!("text export: {error}"));
    assert!(String::from_utf8_lossy(&text).contains("Paragraph 5"));
}

#[test]
fn other_formats_import() {
    let doc = decode("Notes.md", b"# Title\n\nSome *text*.\n").unwrap_or_else(|error| panic!("markdown: {error}"));
    assert!(doc.plain_text(kition_doc_doc::StoryRef::Body).contains("Title"));
    let doc = decode("plain.txt", b"one\ntwo").unwrap_or_else(|error| panic!("text: {error}"));
    assert_eq!(doc.body.len(), 2);
    assert!(decode("x.unknownformat", b"zzz").is_err());
}

#[test]
fn schema_parses() {
    assert_eq!(DocModel::schema().kit, "kit.doc");
}

/// `KITION_DOC_CORPUS=<dir>`: up to 40 `.docx` files there, each imported, written, read back and
/// compared. Skipped when unset.
#[test]
fn docx_corpus_round_trips() {
    let Ok(dir) = std::env::var("KITION_DOC_CORPUS") else { return };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("docx")))
        .collect();
    files.sort();
    files.truncate(40);
    let mut checked = 0;
    for path in &files {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{name}: {error}"));
        let Ok(original) = decode(&name, &bytes) else {
            eprintln!("{name}: not importable, skipped");
            continue;
        };
        assert_same(&name, &original, &round_trip(&original));
        checked += 1;
    }
    eprintln!("corpus: {checked} of {} files round-tripped", files.len());
}
