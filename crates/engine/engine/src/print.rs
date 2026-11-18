//! The document as it prints: WPS for Mac updates page references when it prints or exports, so a
//! table of contents shows the pages its headings land on in that layout, not the numbers saved
//! with the file.

use std::collections::HashMap;

use kition_doc_doc::para::InlineObject;
use kition_doc_doc::{Document, StoryRef};
use kition_doc_layout::{LayoutCache, LayoutOptions, ViewMode};

/// `doc` with its body's page references (`PAGEREF`, unlocked, without `\p`) showing the page
/// their bookmark lands on, numbered as that page's section numbers its pages; `None` when every
/// one already does (or there are none).
pub fn printed(doc: &Document) -> Option<Document> {
    let references = |doc: &Document| {
        let mut found = Vec::new();
        for path in doc.para_paths(StoryRef::Body) {
            let Some(p) = doc.para(StoryRef::Body, &path) else { continue };
            for (k, object) in p.objects.iter().enumerate() {
                if let InlineObject::Field { instr, result, locked: false, .. } = object
                    && kition_doc_layout::fields::field_name(instr) == "PAGEREF"
                    && !instr.split_whitespace().any(|word| word.eq_ignore_ascii_case("\\p"))
                    && let Some(target) = instr.split_whitespace().nth(1)
                {
                    found.push((path.clone(), k, target.to_string(), result.clone()));
                }
            }
        }
        found
    };
    let found = references(doc);
    if found.is_empty() {
        return None;
    }
    let opts = LayoutOptions { view: ViewMode::Print, web_width: 0.0, show_hidden: false, hide_deleted: false, proofing: false };
    let layout = kition_doc_layout::layout(doc, &mut LayoutCache::new(), &opts);
    let sections = doc.sections();
    let mut pages: HashMap<String, String> = HashMap::new();
    for (name, pos) in doc.bookmarks() {
        let Some(page) = layout.caret(&pos).and_then(|caret| layout.pages.get(caret.page)) else { continue };
        let format = sections.get(page.section).map(|(_, section)| section.page_num_format).unwrap_or_default();
        pages.entry(name).or_insert_with(|| format.format(page.number));
    }
    let mut out = doc.clone();
    let mut changed = false;
    for (path, k, target, result) in found {
        let Some(page) = pages.get(&target).filter(|page| **page != result) else { continue };
        let Ok(para) = out.para_mut(StoryRef::Body, &path) else { continue };
        if let Some(InlineObject::Field { result, .. }) = para.objects.get_mut(k) {
            result.clone_from(page);
            para.touch();
            changed = true;
        }
    }
    changed.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_doc_doc::Path;

    #[test]
    fn printing_shows_the_page_a_reference_lands_on() {
        // WPS for Mac's PDF of luan: a contents entry saved as page 16 prints 17, the page its
        // heading lands on.
        let mut doc = Document::from_text("Contents\nHeading");
        let field = |result: &str, locked: bool| InlineObject::Field {
            instr: " PAGEREF _Toc1 \\h ".into(),
            result: result.into(),
            locked,
            code_runs: Vec::new(),
            code_fields: Vec::new(),
        };
        let contents = doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
        contents.insert_object(contents.len(), field("1", false), &Default::default()).unwrap();
        contents.insert_object(contents.len(), field("1", true), &Default::default()).unwrap();
        let heading = doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap();
        heading.props.page_break_before = Some(true);
        heading.insert_object(0, InlineObject::BookmarkStart { name: "_Toc1".into() }, &Default::default()).unwrap();
        let printed = printed(&doc).expect("the reference changes");
        let results: Vec<_> = printed
            .para(StoryRef::Body, &Path::top(0))
            .unwrap()
            .objects
            .iter()
            .filter_map(|o| match o {
                InlineObject::Field { result, .. } => Some(result.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(results, ["2", "1"], "the locked reference keeps its saved page");
        assert!(super::printed(&printed).is_none(), "nothing left to update");
    }
}
