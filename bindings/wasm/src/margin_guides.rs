//! Non-printing corner guides at each page's actual text area.
use super::{PageArea, ToolOverlay};
use kition_doc_engine::layout::DocLayout;

pub(super) fn append(layout: &DocLayout, areas: &[PageArea], overlays: &mut Vec<ToolOverlay>) {
    for (page, area) in layout.pages.iter().zip(areas) {
        let body = page.body;
        for (x, y, dx, dy) in [
            (body.x, body.y, -1.0, -1.0),
            (body.right(), body.y, 1.0, -1.0),
            (body.x, body.bottom(), -1.0, 1.0),
            (body.right(), body.bottom(), 1.0, 1.0),
        ] {
            // Short margins shorten the arms so the guides stay on the paper.
            let horizontal = if dx < 0.0 { x } else { page.w - x }.clamp(0.0, 18.0);
            let vertical = if dy < 0.0 { y } else { page.h - y }.clamp(0.0, 18.0);
            let bx = area.x + f64::from(x);
            let by = area.y + f64::from(y);
            for (ax, ay, length) in [(bx + f64::from(dx * horizontal), by, horizontal), (bx, by + f64::from(dy * vertical), vertical)] {
                if length > 0.0 {
                    overlays.push(ToolOverlay::Line { ax, ay, bx, by, color: [200, 196, 190, 255], dashed: false });
                }
            }
        }
    }
}
