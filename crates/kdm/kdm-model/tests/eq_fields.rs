mod common;

use common::{SharedBlobs, new_replica};
use kdm_model::read_document;
use kition_doc_doc::{CharProps, Document, InlineObject, Path, Run, StoryRef};

#[test]
fn eq_fields_replica_preserves_code_run_formatting() {
    let mut doc = Document::from_text("");
    let small = CharProps { size: Some(6.0), ..Default::default() };
    let field = InlineObject::Field {
        instr: r"EQ \o(AB,CD)".into(),
        result: String::new(),
        locked: false,
        code_runs: vec![
            Run { len: 6, props: CharProps::default() },
            Run { len: 2, props: small.clone() },
            Run { len: 1, props: CharProps::default() },
            Run { len: 2, props: small },
            Run { len: 1, props: CharProps::default() },
        ],
        code_fields: Vec::new(),
    };
    doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().insert_object(0, field.clone(), &CharProps::default()).unwrap();
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&doc, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap();
    let para = read.para(StoryRef::Body, &Path::top(0)).unwrap();
    assert_eq!(serde_json::to_value(&para.objects[0]).unwrap(), serde_json::to_value(&field).unwrap());
}
