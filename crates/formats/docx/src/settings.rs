//! The document settings Kition Doc does not model, kept for writing back.
//!
//! `word/settings.xml` holds much that Kition Doc neither shows nor edits: document variables,
//! revision save ids, the theme colour mapping, compatibility settings, view and proofing options,
//! Word 2010+ extensions. The reader keeps every such setting as its XML ([`kept`], passthrough
//! key [`crate::package::SETTINGS_KEPT`]); the writer puts each back in its place in the
//! schema's sequence (ECMA-376 §17.15.1.78, `CT_Settings`) among the settings it writes from the
//! model ([`merge`]). A setting the writer writes itself always wins, except that a protected
//! document's protection keeps its password hash while it protects the same way.

use crate::xml::{self, El, Node};

/// Most bytes of kept settings.
const MAX_KEPT: usize = 1 << 20;

/// `CT_Settings` in schema order (local names; `mathPr` is `m:mathPr`). Extensions in other
/// namespaces (`w14:docId`, `w15:docId`…) come after all of these.
const ORDER: &[&str] = &[
    "writeProtection",
    "view",
    "zoom",
    "removePersonalInformation",
    "removeDateAndTime",
    "doNotDisplayPageBoundaries",
    "displayBackgroundShape",
    "printPostScriptOverText",
    "printFractionalCharacterWidth",
    "printFormsData",
    "embedTrueTypeFonts",
    "embedSystemFonts",
    "saveSubsetFonts",
    "saveFormsData",
    "mirrorMargins",
    "alignBordersAndEdges",
    "bordersDoNotSurroundHeader",
    "bordersDoNotSurroundFooter",
    "gutterAtTop",
    "hideSpellingErrors",
    "hideGrammaticalErrors",
    "activeWritingStyle",
    "proofState",
    "formsDesign",
    "attachedTemplate",
    "linkStyles",
    "stylePaneFormatFilter",
    "stylePaneSortMethod",
    "documentType",
    "mailMerge",
    "revisionView",
    "trackRevisions",
    "doNotTrackMoves",
    "doNotTrackFormatting",
    "documentProtection",
    "autoFormatOverride",
    "styleLockTheme",
    "styleLockQFSet",
    "defaultTabStop",
    "autoHyphenation",
    "consecutiveHyphenLimit",
    "hyphenationZone",
    "doNotHyphenateCaps",
    "showEnvelope",
    "summaryLength",
    "clickAndTypeStyle",
    "defaultTableStyle",
    "evenAndOddHeaders",
    "bookFoldRevPrinting",
    "bookFoldPrinting",
    "bookFoldPrintingSheets",
    "drawingGridHorizontalSpacing",
    "drawingGridVerticalSpacing",
    "displayHorizontalDrawingGridEvery",
    "displayVerticalDrawingGridEvery",
    "doNotUseMarginsForDrawingGridOrigin",
    "drawingGridHorizontalOrigin",
    "drawingGridVerticalOrigin",
    "doNotShadeFormData",
    "noPunctuationKerning",
    "characterSpacingControl",
    "printTwoOnOne",
    "strictFirstAndLastChars",
    "noLineBreaksAfter",
    "noLineBreaksBefore",
    "savePreviewPicture",
    "doNotValidateAgainstSchema",
    "saveInvalidXml",
    "ignoreMixedContent",
    "alwaysShowPlaceholderText",
    "doNotDemarcateInvalidXml",
    "saveXmlDataOnly",
    "useXSLTWhenSaving",
    "saveThroughXslt",
    "showXMLTags",
    "alwaysMergeEmptyNamespace",
    "updateFields",
    "hdrShapeDefaults",
    "footnotePr",
    "endnotePr",
    "compat",
    "docVars",
    "rsids",
    "mathPr",
    "attachedSchema",
    "themeFontLang",
    "clrSchemeMapping",
    "doNotIncludeSubdocsInStats",
    "doNotAutoCompressPictures",
    "forceUpgrade",
    "captions",
    "readModeInkLockDown",
    "smartTagType",
    "schemaLibrary",
    "shapeDefaults",
    "doNotEmbedSmartTags",
    "decimalSymbol",
    "listSeparator",
];

/// Settings the reader turns into the model (written from it, so never kept): keeping them too
/// would bring back what an edit changed.
const MODELLED: &[&str] = &[
    "w:zoom",
    "w:trackRevisions",
    "w:defaultTabStop",
    "w:autoHyphenation",
    "w:evenAndOddHeaders",
    "w:mirrorMargins",
    "w:drawingGridHorizontalSpacing",
    "w:drawingGridVerticalSpacing",
    "w:characterSpacingControl",
    "w:footnotePr",
    "w:endnotePr",
    "w:themeFontLang",
    "m:mathPr",
];

/// Where `e` goes in the sequence: its index in [`ORDER`], extensions after.
fn rank(e: &El) -> usize {
    let (prefix, local) = e.name.split_once(':').unwrap_or(("", e.name.as_str()));
    match (prefix, ORDER.iter().position(|n| *n == local)) {
        ("w" | "m" | "sl", Some(i)) => i,
        ("w", None) => ORDER.len(),
        _ => ORDER.len() + 1,
    }
}

/// Uses a relationship (`attachedTemplate`, a mail merge's data source), which would not travel.
fn uses_rels(e: &El, depth: usize) -> bool {
    depth > xml::MAX_DEPTH || e.attrs.iter().any(|(k, _)| k.starts_with("r:")) || e.els().any(|c| uses_rels(c, depth + 1))
}

/// Whether `e` survives being written as XML: elements in unknown namespaces are dropped by
/// [`El::to_xml`], and alternate content would name their prefixes.
fn writable(e: &El) -> bool {
    !e.name.starts_with("?:") && e.name != "mc:AlternateContent" && !uses_rels(e, 0)
}

/// The settings of `root` (`w:settings`) Kition Doc does not model, as XML. Of `w:compat`, the
/// compatibility settings other than the mode (as `<w:compat>` holding them).
/// In schema order (some writers put settings out of it), so that saving and reading again keeps
/// the same.
pub(crate) fn kept(root: &El) -> String {
    let mut els: Vec<&El> = root.els().collect();
    els.sort_by_key(|e| rank(e));
    let mut out = String::new();
    for e in els {
        let piece = match e.name.as_str() {
            "w:compat" => {
                let settings: Vec<&El> = e
                    .children("w:compatSetting")
                    .filter(|c| c.attr("w:name").is_some_and(|n| n != "compatibilityMode") && c.attr("w:uri").is_some())
                    .collect();
                if settings.is_empty() {
                    continue;
                }
                let inner: String = settings.iter().map(|c| c.to_xml()).collect();
                format!("<w:compat>{inner}</w:compat>")
            }
            name if MODELLED.contains(&name) => continue,
            _ if writable(e) => e.to_xml(),
            _ => continue,
        };
        if out.len() + piece.len() > MAX_KEPT {
            break;
        }
        out.push_str(&piece);
    }
    out
}

/// Whether a `w:documentProtection` enforces its protection.
fn enforced(e: &El) -> bool {
    e.attr("w:enforcement").is_some_and(|v| !matches!(v, "0" | "false" | "off"))
}

/// `written` (a whole `settings.xml`) with the `kept` settings put back in their places.
pub(crate) fn merge(written: Vec<u8>, kept: &[u8]) -> Vec<u8> {
    let Ok(mut root) = xml::parse(&written) else { return written };
    let ns: String = xml::body_ns().iter().filter(|(k, _)| k.starts_with("xmlns:")).map(|(k, v)| format!(" {k}=\"{}\"", xml::esc(v))).collect();
    let Ok(kept) = xml::parse(format!("<w:x{ns}>{}</w:x>", String::from_utf8_lossy(kept)).as_bytes()) else { return written };
    for e in kept.els().filter(|e| writable(e)) {
        let at = root.kids.iter().position(|k| matches!(k, Node::El(c) if c.name == e.name));
        match (e.name.as_str(), at) {
            // The compatibility settings beyond the mode, after the ones written.
            ("w:compat", Some(i)) => {
                if let Some(Node::El(compat)) = root.kids.get_mut(i) {
                    for s in e.children("w:compatSetting") {
                        let name = s.attr("w:name");
                        if !compat.children("w:compatSetting").any(|c| c.attr("w:name") == name) {
                            compat.kids.push(Node::El(s.clone()));
                        }
                    }
                }
            }
            // The same protection: keep its password hash and options.
            ("w:documentProtection", Some(i)) => {
                if let Some(Node::El(written)) = root.kids.get(i)
                    && enforced(e)
                    && written.attr("w:edit") == e.attr("w:edit")
                {
                    root.kids[i] = Node::El(e.clone());
                }
            }
            // Protection the document no longer has is not brought back.
            ("w:documentProtection", None) if enforced(e) => {}
            (_, Some(_)) => {}
            (_, None) => {
                let r = rank(e);
                let pos = root.kids.iter().position(|k| matches!(k, Node::El(c) if rank(c) > r)).unwrap_or(root.kids.len());
                root.kids.insert(pos, Node::El(e.clone()));
            }
        }
    }
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    let decls: Vec<(String, String)> = xml::body_ns();
    let mut head = root.clone();
    head.attrs.retain(|(k, _)| !k.starts_with("xmlns:") && k != "mc:Ignorable");
    head.attrs.splice(0..0, decls);
    out.push_str(&head.to_xml());
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(inner: &str) -> El {
        let ns: String = xml::body_ns().iter().filter(|(k, _)| k.starts_with("xmlns:")).map(|(k, v)| format!(" {k}=\"{v}\"")).collect();
        xml::parse(format!("<w:settings{ns}>{inner}</w:settings>").as_bytes()).unwrap()
    }

    #[test]
    fn unmodelled_settings_go_back_in_schema_order() {
        let read = settings(
            r#"<w:zoom w:percent="90"/><w:displayBackgroundShape/><w:proofState w:spelling="clean"/><w:attachedTemplate r:id="rId1"/><w:defaultTabStop w:val="420"/><w:compat><w:useFELayout/><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/><w:compatSetting w:name="overrideTableStyleFontSizeAndJustification" w:uri="http://schemas.microsoft.com/office/word" w:val="1"/></w:compat><w:docVars><w:docVar w:name="Client" w:val="Synthetic"/></w:docVars><w:rsids><w:rsidRoot w:val="00A1B2C3"/></w:rsids><w:clrSchemeMapping w:bg1="light1" w:t1="dark1"/><w15:docId w15:val="{00000000-0000-0000-0000-000000000001}"/>"#,
        );
        let k = kept(&read);
        assert!(!k.contains("w:zoom") && !k.contains("defaultTabStop") && !k.contains("attachedTemplate") && !k.contains("compatibilityMode"), "{k}");
        let written = br#"<?xml version="1.0"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:zoom w:percent="100"/><w:defaultTabStop w:val="720"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat><w:themeFontLang w:val="en-US"/></w:settings>"#;
        let out = String::from_utf8(merge(written.to_vec(), k.as_bytes())).unwrap();
        let names: Vec<String> = xml::parse(out.as_bytes()).unwrap().els().map(|e| e.name.clone()).collect();
        assert_eq!(
            names,
            [
                "w:zoom",
                "w:displayBackgroundShape",
                "w:proofState",
                "w:defaultTabStop",
                "w:compat",
                "w:docVars",
                "w:rsids",
                "w:themeFontLang",
                "w:clrSchemeMapping",
                "w15:docId"
            ]
        );
        assert!(out.contains(r#"<w:zoom w:percent="100"/>"#) && out.contains(r#"w:name="overrideTableStyleFontSizeAndJustification""#), "{out}");
        assert!(out.contains(r#"<w:docVar w:name="Client" w:val="Synthetic"/>"#), "{out}");
    }

    #[test]
    fn protection_keeps_its_hash_only_while_the_same() {
        let read = settings(
            r#"<w:documentProtection w:edit="readOnly" w:enforcement="1" w:cryptProviderType="rsaAES" w:hashValue="c3ludGhldGlj" w:saltValue="c2FsdA=="/>"#,
        );
        let k = kept(&read);
        let same = br#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:documentProtection w:edit="readOnly" w:enforcement="1"/></w:settings>"#;
        let out = String::from_utf8(merge(same.to_vec(), k.as_bytes())).unwrap();
        assert!(out.contains(r#"w:hashValue="c3ludGhldGlj""#), "{out}");
        let other = br#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:documentProtection w:edit="comments" w:enforcement="1"/></w:settings>"#;
        let out = String::from_utf8(merge(other.to_vec(), k.as_bytes())).unwrap();
        assert!(!out.contains("hashValue") && out.contains(r#"w:edit="comments""#), "{out}");
        let none = br#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:zoom w:percent="100"/></w:settings>"#;
        let out = String::from_utf8(merge(none.to_vec(), k.as_bytes())).unwrap();
        assert!(!out.contains("documentProtection"), "protection removed in Kition stays removed: {out}");
    }
}
