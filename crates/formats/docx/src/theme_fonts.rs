//! Resolve theme font slots from document languages before reading run properties.

use std::collections::HashMap;

use kition_doc_doc::ThemeFontLang;

use crate::xml::El;

fn face(element: &El) -> Option<&str> {
    element.attr("typeface").filter(|name| !name.is_empty() && name.len() < 256)
}

/// Explicit ISO 15924 subtags take precedence over language/region defaults.
/// Unlisted languages leave the theme's regional default active.
fn script(language: &str) -> Option<String> {
    let mut parts = language.split('-');
    let language = parts.next()?.to_ascii_lowercase();
    let subtags: Vec<_> = parts.take_while(|part| part.len() != 1).collect();
    if let Some(tag) = subtags.iter().find(|part| part.len() == 4 && part.bytes().all(|byte| byte.is_ascii_alphabetic())) {
        let mut tag = tag.to_ascii_lowercase();
        if let Some(first) = tag.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        return Some(tag);
    }
    let result = match language.as_str() {
        "zh" if subtags.iter().any(|part| ["TW", "HK", "MO"].iter().any(|region| part.eq_ignore_ascii_case(region))) => "Hant",
        "zh" => "Hans",
        "ja" => "Jpan",
        "ko" => "Hang",
        "ar" | "fa" | "ur" | "ps" | "sd" | "ug" => "Arab",
        "he" | "yi" => "Hebr",
        "hi" | "mr" | "ne" => "Deva",
        "bn" => "Beng",
        "th" => "Thai",
        "el" => "Grek",
        "ru" | "uk" | "bg" | "mk" | "sr" | "be" => "Cyrl",
        "hy" => "Armn",
        "ka" => "Geor",
        "ta" => "Taml",
        "te" => "Telu",
        "ml" => "Mlym",
        "kn" => "Knda",
        "gu" => "Gujr",
        "or" => "Orya",
        "si" => "Sinh",
        "km" => "Khmr",
        "lo" => "Laoo",
        "bo" => "Tibt",
        "am" => "Ethi",
        "my" => "Mymr",
        "en" | "fr" | "de" | "es" | "it" | "pt" | "nl" | "da" | "sv" | "nb" | "nn" | "fi" | "is" | "pl" | "cs" | "sk" | "sl" | "hr" | "hu" | "ro"
        | "vi" | "id" | "ms" | "tr" | "sw" => "Latn",
        _ => return None,
    };
    Some(result.to_string())
}

pub(crate) fn read(theme: &El, language: Option<&ThemeFontLang>) -> HashMap<String, String> {
    let mut slots = HashMap::new();
    let Some(scheme) = theme.child("a:themeElements").and_then(|elements| elements.child("a:fontScheme")) else { return slots };
    for (element, prefix) in [("a:majorFont", "major"), ("a:minorFont", "minor")] {
        let Some(collection) = scheme.child(element) else { continue };
        let mut scripts = HashMap::new();
        for font in collection.children("a:font").take(256) {
            if let Some(tag) = font.attr("script").filter(|tag| tag.len() == 4 && tag.bytes().all(|byte| byte.is_ascii_alphabetic()))
                && let Some(name) = face(font)
            {
                scripts.entry(tag.to_ascii_lowercase()).or_insert(name);
            }
        }
        for (suffix, default, locale) in [
            ("Ascii", "a:latin", language.and_then(|language| language.latin.as_deref())),
            ("HAnsi", "a:latin", language.and_then(|language| language.latin.as_deref())),
            ("EastAsia", "a:ea", language.and_then(|language| language.east_asia.as_deref())),
            ("Bidi", "a:cs", language.and_then(|language| language.bidi.as_deref())),
        ] {
            let selected = locale.and_then(script).and_then(|script| scripts.get(&script.to_ascii_lowercase()).copied());
            if let Some(name) = selected.or_else(|| collection.child(default).and_then(face)) {
                slots.insert(format!("{prefix}{suffix}"), name.to_string());
            }
        }
    }
    slots
}
