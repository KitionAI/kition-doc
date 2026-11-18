//! Single line spacing of East Asian fonts: Word gives a font that declares an East Asian code
//! page 1.3 times its Windows ascent plus descent, the extra split evenly above and below.

use super::*;
use crate::word;
use skrifa::raw::TableProvider;

/// The bundled Source Sans 3 with its OS/2 code pages and Windows metrics replaced.
fn patched(code_pages: u32, win: (u16, u16)) -> FontFace {
    let mut data = BUNDLED[0].to_vec();
    let count = u16::from_be_bytes([data[4], data[5]]) as usize;
    let os2 = (0..count)
        .map(|i| 12 + i * 16)
        .find(|&r| &data[r..r + 4] == b"OS/2")
        .map(|r| u32::from_be_bytes(data[r + 8..r + 12].try_into().unwrap()) as usize)
        .unwrap();
    data[os2 + 74..os2 + 76].copy_from_slice(&win.0.to_be_bytes());
    data[os2 + 76..os2 + 78].copy_from_slice(&win.1.to_be_bytes());
    data[os2 + 78..os2 + 82].copy_from_slice(&code_pages.to_be_bytes());
    make_face(FontBytes::Owned(std::sync::Arc::new(data)), 0, "Metric Probe".into(), "Regular".into(), "Metric Probe".into(), Vec::new()).unwrap()
}

#[test]
fn east_asian_code_pages_give_word_east_asian_line_height() {
    // Chinese Simplified (code page 936, bit 18): Windows metrics, times 1.3, split evenly.
    let face = patched(1 | 1 << 18, (900, 300));
    let (a, d) = word::line_metrics(&face);
    assert!((a - 1080.0).abs() < 1e-6 && (d - 480.0).abs() < 1e-6, "{a} {d}");
    for bit in [17, 19, 20, 21] {
        let (a, d) = word::line_metrics(&patched(1 << bit, (900, 300)));
        assert!((a + d - 1560.0).abs() < 1e-6, "bit {bit}: {a} {d}");
    }
}

#[test]
fn latin_code_pages_keep_their_own_line_height() {
    let plain = patched(1, (900, 300));
    let (a, d) = word::line_metrics(&plain);
    let hhea = plain.skrifa().unwrap().hhea().unwrap();
    let gap = f64::from(hhea.line_gap().to_i16().max(0));
    assert!((a - (f64::from(hhea.ascender().to_i16()) + gap / 2.0)).abs() < 1e-6, "{a}");
    assert!((d - (f64::from(hhea.descender().to_i16()).abs() + gap / 2.0)).abs() < 1e-6, "{d}");
}
