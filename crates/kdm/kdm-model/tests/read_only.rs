//! The package parts Kition Doc keeps verbatim (`Document.passthrough`) are read-only through
//! the object model (Kition platform, OM8). Reads still answer them, the kit's own transactions
//! still write them, and the mark changes nothing stored, so a reader of the schema without it
//! loads the document.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, new_replica, para};
use kdm_model::{DocModel, read_document};
use kition_doc_doc::Document;
use kition_kit::kdm::batch::{self, BatchErrorCode, ReplicaTarget};
use kition_kit::kdm::{ActorId, Replica, Schema};
use kition_kit::session::KitModel;
use serde_json::{Value, json};

const SCHEMA: &str = include_str!("../model/schema.json");
const PEOPLE: &[u8] = b"<w15:people xmlns:w15=\"urn:w15\"/>";

fn kept_document() -> Document {
    let mut d = Document::new();
    d.body = vec![para("Kept parts")];
    d.passthrough.insert("word/people.xml".into(), Arc::new(PEOPLE.to_vec()));
    d
}

#[test]
fn kept_parts_are_read_only_through_the_object_model() {
    let blobs = SharedBlobs::default();
    let (mut replica, _) = new_replica(&kept_document(), &blobs);

    let read = batch::run(&mut ReplicaTarget(&mut replica), &json!({ "load": [{ "target": "root", "fields": ["passthrough"] }] }))
        .unwrap_or_else(|error| panic!("load: {error}"));
    assert!(read["loaded"][0]["fields"]["passthrough"].get("word/people.xml").is_some(), "{read}");

    let before = replica.state_hash().unwrap_or_else(|error| panic!("{error}"));
    for value in [json!({}), Value::Null, json!({ "word/people.xml": { "hash": "00", "mime": "application/xml", "size": 1 } })] {
        let error = batch::run(
            &mut ReplicaTarget(&mut replica),
            &json!({ "ops": [{ "op": "set", "target": "root", "field": "passthrough", "value": value }] }),
        )
        .expect_err("passthrough is read-only");
        assert_eq!((error.code, error.index), (BatchErrorCode::ReadOnly, Some(0)));
    }
    assert_eq!(replica.state_hash().unwrap_or_else(|error| panic!("{error}")), before);

    // The modelled fields stay writable.
    batch::run(&mut ReplicaTarget(&mut replica), &json!({ "ops": [{ "op": "set", "target": "root", "field": "settings", "value": null }] }))
        .unwrap_or_else(|error| panic!("settings: {error}"));
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(read.passthrough.get("word/people.xml").map(|bytes| bytes.as_slice()), Some(PEOPLE));
}

#[test]
fn the_schema_marks_the_kept_parts() {
    let schema = DocModel::schema();
    assert!(schema.field("Document", "passthrough").unwrap_or_else(|error| panic!("{error}")).read_only);
    assert!(!schema.field("Document", "settings").unwrap_or_else(|error| panic!("{error}")).read_only);
}

#[test]
fn a_reader_of_the_schema_without_the_mark_loads_the_document() {
    let mut plain: Value = serde_json::from_str(SCHEMA).unwrap_or_else(|error| panic!("{error}"));
    let types = plain["types"].as_object_mut().unwrap_or_else(|| panic!("types"));
    for def in types.values_mut() {
        // A type's tool id names a read-only field (C8-land-14-doc); the reader without the mark
        // is older than tool ids too, and the SDK refuses a `toolId` field that is not read-only.
        def.as_object_mut().unwrap_or_else(|| panic!("type")).remove("toolId");
        for field in def["fields"].as_object_mut().unwrap_or_else(|| panic!("fields")).values_mut() {
            field.as_object_mut().unwrap_or_else(|| panic!("field")).remove("readOnly");
        }
    }
    let plain = Arc::new(Schema::parse(&plain.to_string()).unwrap_or_else(|error| panic!("{error}")));
    assert_eq!(plain.schema_version, DocModel::schema().schema_version);
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&kept_document(), &blobs);
    let actor = ActorId::from_u64(9).unwrap_or_else(|error| panic!("{error}"));
    let snapshot = replica.snapshot().unwrap_or_else(|error| panic!("{error}"));
    let old = Replica::load(plain, &snapshot, actor).unwrap_or_else(|error| panic!("the previous schema loads: {error}"));
    assert_eq!(old.state_json().ok(), replica.state_json().ok());
    let back =
        Replica::load(DocModel::schema(), &old.snapshot().unwrap_or_else(|error| panic!("{error}")), actor).unwrap_or_else(|error| panic!("{error}"));
    let (read, _) = read_document(&back, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(read.passthrough.get("word/people.xml").map(|bytes| bytes.as_slice()), Some(PEOPLE));
}
