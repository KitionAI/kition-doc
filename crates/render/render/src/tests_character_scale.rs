use super::*;
use kition_doc_layout::{LayoutCache, layout};

fn bounds(scale: f32, spacing: f32) -> (u32, u32) {
    let mut doc = Document::from_text("H");
    doc.styles.default_chr.font = Some("Source Sans 3".into());
    doc.styles.default_chr.size = Some(80.0);
    doc.styles.default_chr.scale = Some(scale);
    doc.styles.default_chr.spacing = Some(spacing);
    let laid = layout(&doc, &mut LayoutCache::new(), &Default::default());
    let image = render_page(&doc, &laid.pages[0], 1.0, &Default::default());
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for y in 0..image.height {
        for x in 0..image.width {
            if image.pixel(x, y)[0] < 128 {
                xs.push(x);
                ys.push(y);
            }
        }
    }
    (xs.iter().max().unwrap() - xs.iter().min().unwrap() + 1, ys.iter().max().unwrap() - ys.iter().min().unwrap() + 1)
}

#[test]
fn character_scale_compresses_glyph_ink_without_compressing_height() {
    let normal = bounds(100.0, 0.0);
    let narrow = bounds(50.0, 0.0);
    assert!((narrow.0 as f32 / normal.0 as f32 - 0.5).abs() < 0.04, "normal={normal:?}, compressed={narrow:?}");
    assert_eq!(narrow.1, normal.1);
}

#[test]
fn character_scale_stretches_glyph_ink_without_stretching_height() {
    let normal = bounds(100.0, 0.0);
    let wide = bounds(200.0, 0.0);
    assert!((wide.0 as f32 / normal.0 as f32 - 2.0).abs() < 0.04, "normal={normal:?}, expanded={wide:?}");
    assert_eq!(wide.1, normal.1);
}

#[test]
fn character_scale_spacing_changes_pitch_without_distorting_glyph_ink() {
    assert_eq!(bounds(100.0, 0.0), bounds(100.0, 12.0));
    assert_eq!(bounds(50.0, 0.0), bounds(50.0, 12.0));
}
