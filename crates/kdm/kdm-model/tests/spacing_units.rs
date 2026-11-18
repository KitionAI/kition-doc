mod common;

use common::{SharedBlobs, new_replica};
use kdm_model::read_document;
use kition_doc_doc::{Document, Path, StoryRef};
use serde_json::json;

#[test]
fn paragraph_line_units_survive_the_replica_with_explicit_false_and_zero() {
    let expected = json!({"spaceBefore":12.0,"spaceAfter":8.0,"spaceBeforeLines":150,"spaceAfterLines":0,
        "spaceBeforeAuto":false,"spaceAfterAuto":true});
    let mut doc = Document::from_text("Paragraph spacing");
    doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props = serde_json::from_value(expected.clone()).unwrap();
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&doc, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap();
    assert_eq!(serde_json::to_value(&read.para(StoryRef::Body, &Path::top(0)).unwrap().props).unwrap(), expected);
}
