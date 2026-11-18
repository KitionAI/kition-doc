//! Deterministic page images for external layout comparison; originals are opened read-only.
use std::path::Path;

use kition_doc_engine::{
    Session, io,
    layout::{
        self, Placed,
        display::{self, DisplayOptions, Draw},
    },
};
use serde_json::json;

/// The affine map `m` after `n` (x' = a·x + c·y + e, y' = b·x + d·y + f).
fn then(m: [f32; 6], n: [f32; 6]) -> [f32; 6] {
    let [a, b, c, d, e, f] = m;
    let [p, q, r, s, t, u] = n;
    [a * p + c * q, b * p + d * q, a * r + c * s, b * r + d * s, a * t + c * u + e, b * t + d * u + f]
}

/// The text runs drawn, with their first glyph's origin on the page. `m` maps the draws' frame
/// to the page: runs it turns are marked `rotated` (their letters lie on their side).
fn text_spans(draws: &[Draw], m: [f32; 6], spans: &mut Vec<serde_json::Value>) {
    for draw in draws {
        match draw {
            Draw::Glyphs { face, size, text, glyphs, alpha, .. } => {
                let origin = glyphs.first().map(|(_, x, y)| [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]);
                let rotated = m[1].abs() > 1e-3 || m[2].abs() > 1e-3 || m[0] < 0.0;
                spans.push(json!({
                    "text": text, "font": face.family, "style": face.style, "size_pt": size,
                    "origin": origin, "alpha": alpha, "rotated": rotated
                }))
            }
            Draw::Figure { draws, .. } => text_spans(draws, m, spans),
            Draw::Turned { x, y, turn, items } => text_spans(items, then(m, Draw::turn_matrix(*turn, *x, *y)), spans),
            Draw::Rotated { cx, cy, spin, items } => {
                let turned = then(m, spin.matrix(*cx, *cy));
                let upright = turned[1].abs() < 1e-3 && turned[2].abs() < 1e-3 && turned[0] > 0.0;
                // One letter set upright in vertical text (a run of several reads across the line).
                let single = items.iter().all(|item| matches!(item, Draw::Glyphs { glyphs, .. } if glyphs.len() == 1));
                if upright && single && (m[1].abs() > 1e-3 || m[2].abs() > 1e-3) {
                    // An East Asian letter set upright in turned text: its origin is where vertical
                    // writing puts it (as a PDF's vertical text has it), the top of its cell on
                    // the column's centre line.
                    for item in items {
                        let Draw::Glyphs { face, size, text, glyphs, alpha, horizontal_scale, .. } = item else { continue };
                        let Some(&(gid, _, _)) = glyphs.first() else { continue };
                        let advance = face.advance(gid) as f32 * size / face.upem.max(1.0) as f32 * horizontal_scale;
                        let (u, v) = (cx - advance / 2.0, *cy);
                        spans.push(json!({
                            "text": text, "font": face.family, "style": face.style, "size_pt": size,
                            "origin": [m[0] * u + m[2] * v + m[4], m[1] * u + m[3] * v + m[5]], "alpha": alpha, "rotated": false
                        }));
                    }
                } else {
                    text_spans(items, turned, spans);
                }
            }
            Draw::Clipped { items, .. } => text_spans(items, m, spans),
            _ => {}
        }
    }
}

const UPRIGHT: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn page_description(session: &Session, page: &layout::Page, options: &DisplayOptions) -> serde_json::Value {
    let mut spans = Vec::new();
    text_spans(&display::page_display(&session.doc, page, options), UPRIGHT, &mut spans);
    let mut footer = page.clone();
    footer.items.clear();
    footer.header.clear();
    let mut footer_spans = Vec::new();
    text_spans(&display::page_display(&session.doc, &footer, options), UPRIGHT, &mut footer_spans);
    let mut paragraphs = Vec::new();
    for item in &page.items {
        if let Placed::Lines { story, path, para, l0, l1, y, .. } = item {
            let source = session.doc.para(*story, path);
            let lines: Vec<_> = para
                .lines
                .get(*l0..*l1)
                .unwrap_or(&[])
                .iter()
                .map(|line| {
                    json!({
                        "top": y + line.top - para.lines.get(*l0).map_or(0.0, |l| l.top),
                        "height": line.height, "baseline": y + line.baseline - para.lines.get(*l0).map_or(0.0, |l| l.top),
                        "text": source.and_then(|p| p.text.get(line.start..line.stop)).unwrap_or("")
                    })
                })
                .collect();
            paragraphs.push(json!({"properties": para.rp, "lines": lines}));
        }
    }
    json!({"width_pt": page.w, "height_pt": page.h, "number": page.number,
        "section": page.section, "text_spans": spans, "footer_spans": footer_spans, "paragraphs": paragraphs})
}

pub fn run(args: &[&str]) -> Result<(), String> {
    let [input, output, limit, scale] = args else {
        return Err("usage: cargo xtask render-pages <input.docx> <output-dir> <page-limit> <scale>".into());
    };
    let limit: usize = limit.parse().map_err(|_| "page-limit must be an integer")?;
    let scale: f32 = scale.parse().map_err(|_| "scale must be a number")?;
    if !scale.is_finite() || !(0.1..=2.0).contains(&scale) {
        return Err("scale must be between 0.1 and 2".into());
    }
    let bytes = std::fs::read(input).map_err(|e| e.to_string())?;
    let doc = io::open_bytes(input, &bytes)?;
    // As printed: page references show the pages their bookmarks land on.
    let doc = kition_doc_engine::print::printed(&doc).unwrap_or(doc);
    let session = Session::new(doc);
    let layout = session.export_layout();
    // External page exports show header/footer colors at full opacity.
    let options = DisplayOptions { dim_header: false, ..Default::default() };
    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let mut pages = Vec::new();
    for (index, page) in layout.pages.iter().enumerate() {
        let mut description = page_description(&session, page, &options);
        description["page"] = json!(index + 1);
        if index < limit {
            let name = format!("page-{:04}.png", index + 1);
            let path = Path::new(output).join(&name);
            let image = kition_doc_render::render_page(
                &session.doc,
                page,
                scale,
                &kition_doc_render::RenderOptions { display: options.clone(), ..Default::default() },
            );
            std::fs::write(path, image.to_png()).map_err(|e| e.to_string())?;
            description["image"] = json!(name);
        }
        pages.push(description);
    }
    let grids: Vec<_> = session.doc.sections().iter().map(|(_, section)| &section.grid).collect();
    let report = json!({"pages": pages, "page_count": layout.pages.len(), "scale": scale, "section_grids": grids});
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(Path::new(output).join("layout.json"), text).map_err(|e| e.to_string())?;
    println!("{}", json!({"page_count": layout.pages.len(), "rendered": limit.min(layout.pages.len())}));
    Ok(())
}
