mod common;

use common::{SharedBlobs, new_replica};
use kdm_model::read_document;
use kition_doc_doc::{CharProps, Document, Path, Run, StoryRef, props::FitText};

#[test]
fn fit_text_replica_preserves_width_and_signed_group_identifier() {
    let mut doc = Document::from_text("Shared group");
    let props = CharProps { fit_text: Some(FitText { width: 90.0, id: Some(-42) }), ..Default::default() };
    doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().runs = vec![Run { len: 12, props: props.clone() }];
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&doc, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap();
    assert_eq!(read.para(StoryRef::Body, &Path::top(0)).unwrap().runs[0].props, props);
}
