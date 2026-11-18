//! A face's OpenType `MATH` table (read by [`kition_office_core::math::table`], which the Office
//! kits share), glyph ink bounds, and which installed face sets math.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use kurbo::Shape;

use crate::{FaceRef, FontDb, FontFace};
use kition_office_core::math::table::{MATH_SUBSTITUTES, MathTable, parse};

/// The face's `MATH` table (parsed once per face).
pub fn table(face: &FontFace) -> Option<Arc<MathTable>> {
    static CACHE: OnceLock<Mutex<HashMap<u32, Option<Arc<MathTable>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(t) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&face.id()) {
        return t.clone();
    }
    let t = face
        .skrifa()
        .and_then(|f| f.table_data(skrifa::raw::types::Tag::new(b"MATH")).map(|d| d.as_bytes().to_vec()))
        .and_then(|bytes| parse(&bytes))
        .map(Arc::new);
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(face.id(), t.clone());
    t
}

/// Ink bounds of a glyph in font units, y up: (x_min, y_min, x_max, y_max). `None` for blank glyphs.
pub fn glyph_bounds(face: &FontFace, gid: u32) -> Option<(f32, f32, f32, f32)> {
    let p = FontDb::global().outline(face, gid);
    if p.elements().is_empty() {
        return None;
    }
    let r = p.bounding_box();
    // Outlines are cached y-down.
    let b = (r.x0 as f32, -r.y1 as f32, r.x1 as f32, -r.y0 as f32);
    (b.0.is_finite() && b.1.is_finite() && b.2.is_finite() && b.3.is_finite()).then_some(b)
}

/// The face to set math in: `family` when installed and it has a `MATH` table, else the first
/// installed substitute that has one, else `family` (or its text substitute) without one.
pub fn resolve_math(family: &str) -> (FaceRef, Option<Arc<MathTable>>) {
    static CACHE: OnceLock<Mutex<HashMap<String, (FaceRef, Option<Arc<MathTable>>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(family) {
        return v.clone();
    }
    let db = FontDb::global();
    let mut found = None;
    for fam in std::iter::once(family).chain(MATH_SUBSTITUTES.iter().copied()) {
        if !db.has_family(fam) {
            continue;
        }
        let face = db.face(fam, "Regular");
        if face.family.eq_ignore_ascii_case(fam)
            && let Some(t) = table(&face)
        {
            found = Some((FaceRef::of(&face), Some(t)));
            break;
        }
    }
    let v = found.unwrap_or_else(|| (crate::word::resolve(family, false, false).face, None));
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(family.to_string(), v.clone());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn system_math_font_has_a_table() {
        // Only meaningful where an open math font is installed (macOS ships STIX Two Math).
        let (face, t) = resolve_math("Cambria Math");
        eprintln!("math font: {} (MATH table: {})", face.family, t.is_some());
        if let Some(t) = t {
            assert!(t.get(kition_office_core::math::table::C::AxisHeight) > 0.0, "{}", face.family);
            assert!(t.get(kition_office_core::math::table::C::FractionRuleThickness) > 0.0);
            let paren = face.glyph_for('(');
            assert!(t.vert.get(&paren).is_some_and(|c| !c.variants.is_empty()), "paren variants in {}", face.family);
        }
    }
}
