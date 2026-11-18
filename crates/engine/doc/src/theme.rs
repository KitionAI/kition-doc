//! Theme references kept next to the colours and fonts they stand for ([`ThemeColor`],
//! [`ThemeFonts`]) and their re-resolution when the document's theme changes.
//!
//! The model keeps resolved values (layout reads them) and, beside each one, the theme reference
//! it came from with the value it stood for. A reference holds while its value is unchanged, so
//! editing a colour or font needs no bookkeeping: the stale reference is neither written nor
//! re-resolved. [`Document::retheme`] walks every place a reference can sit by serde name, which
//! keeps it in step with the model without a typed visitor per structure.

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::props::{Rgb, TextColor, ThemeColor, ThemeFonts};
use crate::{Block, Blocks, Document};

/// Fields holding a [`ThemeColor`] and the colour field it describes, by serde name.
const COLOR_PAIRS: &[(&str, &str)] = &[("colorTheme", "color"), ("shadingTheme", "shading"), ("theme", "color")];
/// The table style field mapping fill fields to their theme colours.
const FILL_THEMES: &str = "fillThemes";
/// The run property naming theme fonts.
const FONT_THEME: &str = "fontTheme";

struct Theme<'a> {
    colors: &'a [Rgb],
    major: &'a str,
    minor: &'a str,
}

impl Document {
    /// Re-resolve every theme reference that still holds against the current theme
    /// (`settings.theme_colors`, `major_font`, `minor_font`): call after changing them. Returns
    /// whether anything changed.
    pub fn retheme(&mut self) -> bool {
        let settings = &self.settings;
        let theme = Theme { colors: &settings.theme_colors, major: &settings.major_font, minor: &settings.minor_font };
        let mut changed = false;
        if let Ok(mut v) = serde_json::to_value(&self.styles)
            && walk(&mut v, &theme, 0)
            && let Ok(styles) = serde_json::from_value(v)
        {
            self.styles = styles;
            changed = true;
        }
        if let Ok(mut v) = serde_json::to_value(&self.last_section)
            && walk(&mut v, &theme, 0)
            && let Ok(section) = serde_json::from_value(v)
        {
            self.last_section = section;
            changed = true;
        }
        changed |= retheme_blocks(&mut self.body, &theme);
        for part in self.parts.values_mut() {
            changed |= retheme_blocks(&mut part.blocks, &theme);
        }
        changed
    }
}

/// Re-resolve the blocks of one story; only blocks holding a reference that moved are replaced.
fn retheme_blocks(blocks: &mut Blocks, theme: &Theme) -> bool {
    let mut changed = false;
    for b in blocks.iter_mut() {
        let Ok(mut v) = serde_json::to_value(&**b) else { continue };
        if walk(&mut v, theme, 0)
            && let Ok(block) = serde_json::from_value::<Block>(v)
        {
            *b = Arc::new(block);
            changed = true;
        }
    }
    changed
}

/// Nesting deeper than any document the reader accepts.
const MAX_DEPTH: usize = 256;

fn walk(v: &mut Value, theme: &Theme, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    match v {
        Value::Array(items) => items.iter_mut().fold(false, |acc, item| walk(item, theme, depth + 1) | acc),
        Value::Object(map) => {
            let mut changed = false;
            for (reference, field) in COLOR_PAIRS {
                changed |= retheme_pair(map, reference, field, theme);
            }
            if let Some(Value::Object(fills)) = map.get(FILL_THEMES).cloned() {
                let mut fills = fills;
                for (field, reference) in fills.iter_mut() {
                    changed |= retheme_color(reference, map.get_mut(field), theme);
                }
                map.insert(FILL_THEMES.into(), Value::Object(fills));
            }
            changed |= retheme_fonts(map, theme);
            for (key, child) in map.iter_mut() {
                if key != FILL_THEMES && !COLOR_PAIRS.iter().any(|(r, _)| r == key) && key != FONT_THEME {
                    changed |= walk(child, theme, depth + 1);
                }
            }
            changed
        }
        _ => false,
    }
}

fn retheme_pair(map: &mut Map<String, Value>, reference: &str, field: &str, theme: &Theme) -> bool {
    let Some(mut r) = map.get(reference).cloned() else { return false };
    let changed = retheme_color(&mut r, map.get_mut(field), theme);
    if changed {
        map.insert(reference.into(), r);
    }
    changed
}

/// Re-resolve one colour value (a bare [`Rgb`] or a [`TextColor`]) from its reference.
fn retheme_color(reference: &mut Value, value: Option<&mut Value>, theme: &Theme) -> bool {
    let (Some(value), Ok(mut tc)) = (value, serde_json::from_value::<ThemeColor>(reference.clone())) else { return false };
    let text = serde_json::from_value::<TextColor>(value.clone()).ok();
    let mut rgb = match text {
        Some(TextColor::Rgb(rgb)) => rgb,
        Some(TextColor::Auto) => return false,
        None => match serde_json::from_value::<Rgb>(value.clone()) {
            Ok(rgb) => rgb,
            Err(_) => return false,
        },
    };
    let before = tc;
    tc.retheme(&mut rgb, theme.colors);
    if tc == before {
        return false;
    }
    let new_value = if text.is_some() { serde_json::to_value(TextColor::Rgb(rgb)) } else { serde_json::to_value(rgb) };
    let (Ok(new_value), Ok(new_reference)) = (new_value, serde_json::to_value(tc)) else { return false };
    *value = new_value;
    *reference = new_reference;
    true
}

/// Re-resolve a run's Latin font from its theme font reference.
fn retheme_fonts(map: &mut Map<String, Value>, theme: &Theme) -> bool {
    let Some(Ok(mut ft)) = map.get(FONT_THEME).map(|v| serde_json::from_value::<ThemeFonts>(v.clone())) else { return false };
    let Some(slot) = ft.ascii.clone().or_else(|| ft.h_ansi.clone()) else { return false };
    let font = map.get("font").and_then(Value::as_str).map(str::to_string);
    if font != ft.font {
        return false;
    }
    let new = ThemeFonts::latin(&slot, theme.major, theme.minor);
    if new == ft.font {
        return false;
    }
    match &new {
        Some(f) => map.insert("font".into(), Value::String(f.clone())),
        None => map.remove("font"),
    };
    ft.font = new;
    match serde_json::to_value(&ft) {
        Ok(v) => {
            map.insert(FONT_THEME.into(), v);
            true
        }
        Err(_) => false,
    }
}
