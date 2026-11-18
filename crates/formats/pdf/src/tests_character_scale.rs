use crate::*;
use hayro_interpret::font::Glyph;
use hayro_interpret::{
    BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache, InterpreterSettings, Paint, PathDrawMode, SoftMask, interpret_page,
};

fn transforms(scale: f32) -> (Vec<[f64; 6]>, Vec<String>) {
    let mut doc = Document::from_text("HH");
    doc.styles.default_chr.font = Some("Source Sans 3".into());
    doc.styles.default_chr.size = Some(48.0);
    doc.styles.default_chr.scale = Some(scale);
    let bytes = export(&doc, &Default::default()).unwrap();
    let pdf = hayro_syntax::Pdf::new(bytes).unwrap();
    struct Capture(Vec<[f64; 6]>, String);
    impl Device<'_> for Capture {
        fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
        fn set_blend_mode(&mut self, _: BlendMode) {}
        fn draw_path(&mut self, _: &kurbo::BezPath, _: kurbo::Affine, _: &Paint<'_>, _: &PathDrawMode) {}
        fn push_clip_path(&mut self, _: &ClipPath) {}
        fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
        fn draw_glyph(&mut self, g: &Glyph<'_>, transform: kurbo::Affine, glyph: kurbo::Affine, _: &Paint<'_>, mode: &GlyphDrawMode) {
            if !matches!(mode, GlyphDrawMode::Stroke(_)) {
                self.0.push((transform * glyph).as_coeffs());
                match g.as_unicode() {
                    Some(hayro_cmap::BfString::Char(c)) => self.1.push(c),
                    Some(hayro_cmap::BfString::String(s)) => self.1.push_str(&s),
                    None => {}
                }
            }
        }
        fn draw_image(&mut self, _: Image<'_, '_>, _: kurbo::Affine) {}
        fn pop_clip_path(&mut self) {}
        fn pop_transparency_group(&mut self) {}
    }
    let cache = InterpreterCache::new();
    let mut capture = Capture(Vec::new(), String::new());
    for page in pdf.pages().iter() {
        let mut context =
            Context::new(kurbo::Affine::IDENTITY, kurbo::Rect::new(0.0, 0.0, 1000.0, 1000.0), &cache, pdf.xref(), InterpreterSettings::default());
        interpret_page(page, &mut context, &mut capture);
    }
    (capture.0, vec![capture.1])
}

#[test]
fn character_scale_pdf_transforms_glyph_width_and_preserves_origins_and_text() {
    let (normal, text) = transforms(100.0);
    assert_eq!(normal.len(), 2);
    for scale in [50.0, 80.0, 200.0] {
        let (scaled, actual_text) = transforms(scale);
        assert_eq!(actual_text, text);
        assert_eq!(scaled.len(), normal.len());
        let ratio = f64::from(scale) / 100.0;
        assert!((scaled[0][0] / normal[0][0] - ratio).abs() < 0.001, "scale={scale}, normal={normal:?}, scaled={scaled:?}");
        assert!((scaled[0][3] - normal[0][3]).abs() < 0.001);
        assert!((scaled[0][4] - normal[0][4]).abs() < 0.001);
        assert!((scaled[0][5] - normal[0][5]).abs() < 0.001);
        assert!((scaled[1][4] - scaled[0][4] - (normal[1][4] - normal[0][4]) * ratio).abs() < 0.01);
    }
}
