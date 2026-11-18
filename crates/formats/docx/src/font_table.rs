//! Character-set metadata for font substitution; font programs are never loaded here.

use std::collections::{BTreeMap, BTreeSet};

use kition_doc_doc::MAX_FONT_DECLARATIONS;

use crate::xml::{self, El, Node};

fn charset(font: &El) -> Option<u8> {
    u8::from_str_radix(font.child("w:charset")?.attr("w:val")?.trim(), 16).ok()
}

pub fn read(bytes: &[u8]) -> BTreeMap<String, u8> {
    let Ok(root) = xml::parse(bytes) else { return BTreeMap::new() };
    if root.name != "w:fonts" {
        return BTreeMap::new();
    }
    root.children("w:font")
        .take(MAX_FONT_DECLARATIONS)
        .filter_map(|font| {
            let name = font.attr("w:name")?;
            if name.is_empty() || name.len() > 512 {
                return None;
            }
            Some((name.to_string(), charset(font)?))
        })
        .collect()
}

fn declaration(value: u8) -> Node {
    Node::El(El { name: "w:charset".into(), attrs: vec![("w:val".into(), format!("{value:02X}"))], kids: Vec::new() })
}

pub fn write(charsets: Option<&BTreeMap<String, u8>>, original: Option<&[u8]>) -> Option<Vec<u8>> {
    let Some(charsets) = charsets else { return original.map(<[u8]>::to_vec) };
    if let Some(bytes) = original {
        if read(bytes) == *charsets {
            return Some(bytes.to_vec());
        }
    } else if charsets.is_empty() {
        return None;
    }
    let mut root = original
        .and_then(|bytes| xml::parse(bytes).ok())
        .filter(|root| root.name == "w:fonts")
        .unwrap_or_else(|| El { name: "w:fonts".into(), ..Default::default() });
    let mut seen = BTreeSet::new();
    for node in root
        .kids
        .iter_mut()
        .filter_map(|node| match node {
            Node::El(font) if font.name == "w:font" => Some(font),
            _ => None,
        })
        .take(MAX_FONT_DECLARATIONS)
    {
        let Some(name) = node.attr("w:name").map(str::to_owned) else { continue };
        seen.insert(name.clone());
        let next = charsets.get(&name).copied();
        if next == charset(node) {
            continue;
        }
        node.kids.retain(|child| !matches!(child, Node::El(child) if child.name == "w:charset"));
        if let Some(value) = next {
            let at = node.kids.iter().position(|child| matches!(child, Node::El(child) if matches!(child.name.as_str(), "w:family" | "w:notTrueType" | "w:pitch" | "w:sig" | "w:embedRegular" | "w:embedBold" | "w:embedItalic" | "w:embedBoldItalic"))).unwrap_or(node.kids.len());
            node.kids.insert(at, declaration(value));
        }
    }
    for (name, &value) in charsets.iter().take(MAX_FONT_DECLARATIONS) {
        if !seen.contains(name) && !name.is_empty() && name.len() <= 512 {
            root.kids.push(Node::El(El { name: "w:font".into(), attrs: vec![("w:name".into(), name.clone())], kids: vec![declaration(value)] }));
        }
    }
    for (prefix, uri) in xml::NAMESPACES {
        let key = format!("xmlns:{prefix}");
        if !root.attrs.iter().any(|(name, _)| *name == key) {
            root.attrs.push((key, (*uri).into()));
        }
    }
    Some(root.to_xml().into_bytes())
}
