mod common;

use common::{SharedBlobs, new_replica};
use kdm_model::read_document;
use kition_doc_doc::{
    CharProps, Document, Path, Run, StoryRef,
    props::{CombineBrackets, EastAsianLayout},
};

#[test]
fn two_lines_replica_preserves_settings_and_signed_group_identifier() {
    let mut doc = Document::from_text("Shared group");
    let props = CharProps {
        east_asian_layout: Some(EastAsianLayout {
            id: Some(-42),
            combine: Some(true),
            combine_brackets: Some(CombineBrackets::None),
            vertical: Some(false),
            vertical_compress: Some(false),
        }),
        ..Default::default()
    };
    doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap().runs = vec![Run { len: 12, props: props.clone() }];
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&doc, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap();
    assert_eq!(read.para(StoryRef::Body, &Path::top(0)).unwrap().runs[0].props, props);
}
