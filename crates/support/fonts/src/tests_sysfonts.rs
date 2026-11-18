//! Installed system fonts: found by family name by every lookup, whatever ran before it.

use std::path::{Path, PathBuf};

use super::*;
use crate::word;

const FAMILY: &str = "Sysfont Sans3";

/// A bundled Source Sans 3 file renamed [`FAMILY`] (as long as the original name).
fn renamed(file: &str) -> Vec<u8> {
    let mut data = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/fonts").join(file)).unwrap();
    let utf16 = |s: &str| s.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>();
    for (from, to) in [(utf16("Source Sans 3"), utf16(FAMILY)), (b"Source Sans 3".to_vec(), FAMILY.as_bytes().to_vec())] {
        let mut i = 0;
        while let Some(at) = data[i..].windows(from.len()).position(|w| w == from) {
            data[i + at..i + at + to.len()].copy_from_slice(&to);
            i += at + to.len();
        }
    }
    data
}

/// The fonts in `faces` as one collection (TTC) file.
fn collection(faces: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"ttcf".to_vec();
    out.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
    out.extend_from_slice(&(faces.len() as u32).to_be_bytes());
    let mut base = 12 + 4 * faces.len();
    for f in faces {
        out.extend_from_slice(&(base as u32).to_be_bytes());
        base += f.len();
    }
    for f in faces {
        // Table offsets count from the start of the collection.
        let start = out.len() as u32;
        let mut f = f.clone();
        let tables = u16::from_be_bytes([f[4], f[5]]) as usize;
        for r in 0..tables {
            let at = 12 + r * 16 + 8;
            let off = u32::from_be_bytes(f[at..at + 4].try_into().unwrap()) + start;
            f[at..at + 4].copy_from_slice(&off.to_be_bytes());
        }
        out.extend_from_slice(&f);
    }
    out
}

/// A font folder (with a subfolder, a damaged font and a file that isn't a font) holding
/// [`FAMILY`] Regular and Bold.
fn font_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dc-sysfonts-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Sub")).unwrap();
    std::fs::write(dir.join("Sysfont-Regular.ttf"), renamed("SourceSans3-Regular.ttf")).unwrap();
    std::fs::write(dir.join("Sub/Sysfont-Bold.TTF"), renamed("SourceSans3-Bold.ttf")).unwrap();
    std::fs::write(dir.join("damaged.ttf"), b"ttcf\0\x01\0\0\xff\xff\xff\xff").unwrap();
    std::fs::write(dir.join("readme.txt"), FAMILY).unwrap();
    dir
}

fn has(list: &[String], family: &str) -> bool {
    list.iter().any(|f| f == family)
}

fn synthetic_han_font() -> Vec<u8> {
    let original = make_face(FontBytes::Static(BUNDLED[0]), 0, FAMILY.into(), "Regular".into(), FAMILY.into(), Vec::new()).unwrap();
    let mut data = BUNDLED[0].to_vec();
    // A format-12 cmap maps one Han scalar to an owned outline. No system CJK font is needed.
    let mut cmap = Vec::new();
    for value in [0_u16, 1, 3, 10] {
        cmap.extend_from_slice(&value.to_be_bytes());
    }
    cmap.extend_from_slice(&12_u32.to_be_bytes());
    for value in [12_u16, 0] {
        cmap.extend_from_slice(&value.to_be_bytes());
    }
    for value in [28_u32, 0, 1, 0x4e2d, 0x4e2d, original.glyph_for('A')] {
        cmap.extend_from_slice(&value.to_be_bytes());
    }
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    let offset = data.len() as u32;
    let count = u16::from_be_bytes([data[4], data[5]]) as usize;
    for index in 0..count {
        let record = &mut data[12 + index * 16..28 + index * 16];
        if record.starts_with(b"cmap") {
            record[8..12].copy_from_slice(&offset.to_be_bytes());
            record[12..16].copy_from_slice(&(cmap.len() as u32).to_be_bytes());
        }
    }
    data.extend(cmap);
    data
}

#[test]
fn regional_cjk_fallback_prefers_source_han_to_legacy_faces() {
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    for family in ["Heiti SC", "Heiti TC", "Source Han Sans CN", "Source Han Sans TC", "Source Han Sans JP", "Source Han Sans KR"] {
        let face =
            make_face(FontBytes::Owned(Arc::new(synthetic_han_font())), 0, family.into(), "Regular".into(), family.into(), Vec::new()).unwrap();
        db.faces.write().unwrap().push(Arc::new(face));
    }
    // An earlier candidate with no Han coverage must not prevent a regional fallback.
    let latin =
        make_face(FontBytes::Static(BUNDLED[0]), 0, "Noto Sans CJK SC".into(), "Regular".into(), "Noto Sans CJK SC".into(), Vec::new()).unwrap();
    db.faces.write().unwrap().push(Arc::new(latin));
    for (language, expected) in
        [("zh-CN", "Source Han Sans CN"), ("zh-TW", "Source Han Sans TC"), ("ja", "Source Han Sans JP"), ("ko", "Source Han Sans KR")]
    {
        let face = word::east_asian_fallback(&db, Some(language), '\u{4e2d}', false, false).unwrap();
        assert_eq!(face.face.family, expected);
        assert!(face.substituted);
        assert!(face.face.covers('\u{4e2d}'));
    }
}

#[test]
fn song_families_keep_their_installed_song_face_before_the_regional_fallback() {
    // WPS draws 宋体 in a Song face: the installed STSong (华文宋体), not the sans regional fallback.
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    for family in ["PingFang SC", "STSong"] {
        let face =
            make_face(FontBytes::Owned(Arc::new(synthetic_han_font())), 0, family.into(), "Regular".into(), family.into(), Vec::new()).unwrap();
        db.faces.write().unwrap().push(Arc::new(face));
    }
    for family in ["宋体", "SimSun", "华文中宋"] {
        let face = word::regional_substitute(&db, family, '\u{4e2d}', false, false).unwrap();
        assert_eq!(face.face.family, "STSong", "{family}");
        assert!(face.substituted);
    }
    assert!(word::regional_substitute(&db, "微软雅黑", '\u{4e2d}', false, false).is_none());
    assert_eq!(word::east_asian_fallback(&db, Some("zh-CN"), '\u{4e2d}', false, false).unwrap().face.family, "PingFang SC");
}

#[test]
fn hei_kai_and_fangsong_families_keep_an_installed_face_of_their_style() {
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    for family in ["PingFang SC", "Heiti SC", "STKaiti", "STSong"] {
        let face =
            make_face(FontBytes::Owned(Arc::new(synthetic_han_font())), 0, family.into(), "Regular".into(), family.into(), Vec::new()).unwrap();
        db.faces.write().unwrap().push(Arc::new(face));
    }
    let face =
        make_face(FontBytes::Owned(Arc::new(synthetic_han_font())), 0, "Songti TC".into(), "Regular".into(), "Songti TC".into(), Vec::new()).unwrap();
    db.faces.write().unwrap().push(Arc::new(face));
    for (family, expected) in [("黑体", "Heiti SC"), ("楷体_GB2312", "STKaiti"), ("仿宋", "STSong"), ("PMingLiU", "Songti TC")] {
        assert_eq!(word::regional_substitute(&db, family, '\u{4e2d}', false, false).unwrap().face.family, expected, "{family}");
    }
}

#[test]
fn a_symbol_the_run_font_lacks_falls_back_to_song_then_pingfang() {
    // WPS for Mac draws arrows and enclosed numbers a run's font lacks in its Song face, then
    // PingFang SC (corpus references); the one-em STSong stands in for its Song face.
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    let add = |family: &str| {
        let face =
            make_face(FontBytes::Owned(Arc::new(synthetic_han_font())), 0, family.into(), "Regular".into(), family.into(), Vec::new()).unwrap();
        db.faces.write().unwrap().push(Arc::new(face));
    };
    add("PingFang SC");
    assert_eq!(word::symbol_fallback_in(&db, '\u{4e2d}', false, false).unwrap().face.family, "PingFang SC");
    add("STSong");
    assert_eq!(word::symbol_fallback_in(&db, '\u{4e2d}', false, false).unwrap().face.family, "STSong");
    assert!(word::symbol_fallback_in(&db, '\u{1F600}', false, false).is_none(), "neither covers it: the general fallback decides");
}

#[test]
fn font_charset_selects_a_regional_face_without_replacing_available_fonts() {
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    for family in ["Source Han Sans CN", "Source Han Sans TC"] {
        let face =
            make_face(FontBytes::Owned(Arc::new(synthetic_han_font())), 0, family.into(), "Regular".into(), family.into(), Vec::new()).unwrap();
        db.faces.write().unwrap().push(Arc::new(face));
    }
    assert_eq!(word::charset_fallback(&db, 0x86, false, false).unwrap().face.family, "Source Han Sans CN");
    assert_eq!(word::charset_fallback(&db, 0x88, false, false).unwrap().face.family, "Source Han Sans TC");
    for charset in [0x00, 0x01, 0x02] {
        assert!(word::charset_fallback(&db, charset, false, false).is_none());
    }
    let available = word::resolve_with_charset("Source Sans 3", Some(0x86), false, false);
    assert!(!available.substituted);
    assert_eq!(available.face.family, "Source Sans 3");
}

#[test]
fn late_font_registration_replaces_memoized_word_substitution() {
    let before = word::resolve(FAMILY, false, false);
    assert!(before.substituted);
    let db = FontDb::global();
    assert_eq!(db.add_font(renamed("SourceSans3-Regular.ttf")), 1);
    let after = word::resolve(FAMILY, false, false);
    assert!(!after.substituted, "a previously missing font is now registered");
    assert_eq!(after.face.family, FAMILY);
    assert_ne!(before.face.id(), after.face.id());
}

#[test]
fn nondrawable_font_metadata_does_not_claim_usable_coverage() {
    let mut data = renamed("SourceSans3-Regular.ttf");
    let count = u16::from_be_bytes([data[4], data[5]]) as usize;
    for i in 0..count {
        let tag = &mut data[12 + i * 16..16 + i * 16];
        if tag == b"glyf" {
            tag.copy_from_slice(b"hvgl");
        }
    }
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    assert_eq!(db.add_font(data.clone()), 0, "cmap coverage alone cannot draw a glyph");
    assert!(!db.has_family(FAMILY));
    let fallback = db.face(FAMILY, "Regular");
    assert!(!db.outline(&fallback, fallback.glyph_for('A')).elements().is_empty());
    let dir = std::env::temp_dir().join(format!("kition-nondrawable-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("unsupported.ttf"), data).unwrap();
    let catalog = FontDb::with_font_dirs(vec![dir.clone()]);
    assert!(!catalog.has_family(FAMILY), "the lazy catalog must apply the same renderability rule");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(target_os = "macos")]
fn macos_font_discovery_includes_reserved_system_faces() {
    let reserved = Path::new("/System/Library/PrivateFrameworks/FontServices.framework/Versions/A/Resources/Reserved");
    assert!(system_font_dirs().iter().any(|dir| dir == reserved));
}

#[test]
fn every_lookup_by_name_finds_installed_fonts_in_a_fresh_database() {
    let dir = font_dir("fresh");
    // Each lookup is the first thing a new database is asked. A folder listed twice is read once.
    let db = || FontDb::with_font_dirs(vec![dir.clone(), dir.join("Sub/..")]);
    let f = db().face("sysfont sans3", "Bold");
    assert_eq!((f.family.as_str(), f.style.as_str()), (FAMILY, "Bold"));
    assert!(db().has_family(FAMILY));
    assert!(has(&db().families(), FAMILY));
    assert_eq!(db().styles(FAMILY), ["Regular", "Bold"]);
    assert_eq!(db().load_system_fonts(), 2);
    // Missing fonts stay missing.
    let db = db();
    assert!(!db.has_family("No Such Font"));
    assert_eq!(db.face("No Such Font", "Regular").family, FALLBACK_FAMILY);
}

#[test]
fn the_scan_reads_collections_and_loads_fonts_only_when_used() {
    let dir = font_dir("collection");
    std::fs::remove_file(dir.join("Sysfont-Regular.ttf")).unwrap();
    std::fs::remove_file(dir.join("Sub/Sysfont-Bold.TTF")).unwrap();
    std::fs::write(dir.join("Sysfont.ttc"), collection(&[renamed("SourceSans3-Regular.ttf"), renamed("SourceSans3-Bold.ttf")])).unwrap();
    let db = FontDb::with_font_dirs(vec![dir.clone()]);
    assert_eq!(db.styles(FAMILY), ["Regular", "Bold"]);
    // Cataloged, not loaded.
    assert!(!db.is_loaded(FAMILY));
    let bold = db.face(FAMILY, "Bold");
    assert_eq!((bold.style.as_str(), bold.index()), ("Bold", 1));
    assert_eq!(db.face(FAMILY, "Regular").index(), 0, "the collection's faces load together");
}

#[test]
fn concurrent_first_lookups_all_find_the_installed_font() {
    // Threads racing to load the same installed family used to lose: the one whose load added
    // nothing (another thread had just added it) fell back to the default font.
    let dir = font_dir("race");
    for _ in 0..20 {
        let db = FontDb::with_font_dirs(vec![dir.clone()]);
        assert!(db.has_family(FAMILY));
        std::thread::scope(|s| {
            let found: Vec<_> = (0..8).map(|_| s.spawn(|| db.face(FAMILY, "Regular").family.clone())).collect();
            for f in found {
                assert_eq!(f.join().unwrap(), FAMILY);
            }
        });
    }
}

#[test]
fn rescanning_finds_fonts_installed_since() {
    let dir = font_dir("rescan");
    let later = dir.join("Later");
    let db = FontDb::with_font_dirs(vec![later.clone()]);
    assert!(!db.has_family(FAMILY));
    std::fs::create_dir_all(&later).unwrap();
    std::fs::copy(dir.join("Sysfont-Regular.ttf"), later.join("Sysfont-Regular.ttf")).unwrap();
    assert!(!db.has_family(FAMILY), "the folders are scanned once, not on every miss");
    assert_eq!(db.load_system_fonts(), 1);
    assert!(db.has_family(FAMILY) && has(&db.families(), FAMILY));
}

#[test]
fn a_background_scan_serves_the_first_lookup() {
    let dir = font_dir("background");
    let db: &'static FontDb = Box::leak(Box::new(FontDb::with_font_dirs(vec![dir])));
    db.scan_in_background();
    // Waits for the scan when it is still running.
    assert!(has(&db.families(), FAMILY));
    assert_eq!(db.load_system_fonts(), 2, "a rescan catalogs the two faces again");
}

#[cfg(unix)]
#[test]
fn a_link_back_to_a_parent_folder_ends_the_scan() {
    let dir = font_dir("loop");
    std::os::unix::fs::symlink(&dir, dir.join("Sub/loop")).unwrap();
    let db = FontDb::with_font_dirs(vec![dir]);
    assert_eq!(db.load_system_fonts(), 2);
}

#[test]
fn fallback_lookups_are_remembered_until_fonts_load() {
    // Layout asks for a fallback once per character the font lacks; each search sorts every
    // loaded face, which froze Thai text once many fonts were loaded.
    let dir = font_dir("fallbacks");
    let db = FontDb::with_font_dirs(vec![dir]);
    db.set_system_fallback(false);
    let latin = db.face(FALLBACK_FAMILY, "Regular");
    let thai = 'ก';
    assert!(db.fallback_for(thai, latin.id()).is_none(), "no bundled Thai font");
    let a = db.fallback_for('A', latin.id()).unwrap();
    assert_eq!(db.fallback_for('A', latin.id()).unwrap().id(), a.id(), "the same answer from the cache");
    {
        let cache = db.fallbacks.read().unwrap();
        assert_eq!(cache.faces, db.read_faces().len());
        assert!(cache.map.contains_key(&(thai, latin.id())) && cache.map.contains_key(&('A', latin.id())));
    }
    // Loading fonts (here, the installed family) makes the remembered answers stale.
    assert_eq!(db.face(FAMILY, "Regular").family, FAMILY);
    assert_eq!(db.fallback_for('A', latin.id()).unwrap().id(), a.id());
    let cache = db.fallbacks.read().unwrap();
    assert_eq!(cache.faces, db.read_faces().len());
    assert!(!cache.map.contains_key(&(thai, latin.id())), "entries from before the fonts loaded are dropped");
}

#[test]
fn the_scan_reads_names_without_loading_the_font() {
    let dir = font_dir("names");
    assert_eq!(file_face_names(&dir.join("Sysfont-Regular.ttf")), [(FAMILY.to_string(), "Regular".to_string(), FAMILY.to_string())]);
    assert!(file_face_names(&dir.join("damaged.ttf")).is_empty());
    assert!(file_face_names(&dir.join("readme.txt")).is_empty());
    assert!(file_face_names(&dir.join("missing.ttf")).is_empty());
}

#[test]
fn installed_weights_are_found_by_their_legacy_family_name() {
    // Semibold is not one of Regular, Bold and Italic: the font names it "Sysfont Sans3 Semibold"
    // (name ID 1), the way Word lists and stores it.
    let dir = font_dir("legacy");
    std::fs::write(dir.join("Sysfont-Semibold.ttf"), renamed("SourceSans3-Semibold.ttf")).unwrap();
    let semibold = format!("{FAMILY} Semibold");
    assert_eq!(file_face_names(&dir.join("Sysfont-Semibold.ttf")), [(FAMILY.to_string(), "Semibold".to_string(), semibold.clone())]);
    let db = || FontDb::with_font_dirs(vec![dir.clone()]);
    assert!(has(&db().families(), FAMILY) && has(&db().families(), &semibold));
    assert!(db().has_family(&semibold.to_uppercase()));
    assert_eq!(db().styles(&semibold), ["Semibold"]);
    assert_eq!(db().styles(FAMILY), ["Regular", "Semibold", "Bold"]);
    // The first lookup, by the legacy name, finds the face and loads the whole family with it.
    let db = db();
    let f = db.face(&semibold, "Regular");
    assert_eq!((f.family.as_str(), f.style.as_str(), f.legacy_family.as_str()), (FAMILY, "Semibold", semibold.as_str()));
    assert_eq!(db.face(FAMILY, "Regular").style, "Regular");
    assert_eq!(db.face(FAMILY, "Bold").style, "Bold");
}

#[test]
fn the_scan_lists_a_variable_fonts_named_instances_as_loading_it_does() {
    // Uses an installed variable font when there is one.
    let Some(path) = [
        "/System/Library/Fonts/Supplemental/Skia.ttf",
        "/System/Library/Fonts/NewYork.ttf",
        "/usr/share/fonts/cantarell/Cantarell-VF.otf",
        "/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf",
        "C:\\Windows\\Fonts\\bahnschrift.ttf",
    ]
    .iter()
    .map(Path::new)
    .find(|p| p.exists()) else {
        eprintln!("no variable font installed: nothing to check");
        return;
    };
    let data = std::fs::read(path).unwrap();
    let loaded: Vec<(String, String, String)> = enumerate_faces(&data).into_iter().map(|f| (f.family, f.style, f.legacy)).collect();
    assert_eq!(file_face_names(path), loaded);
    if loaded.len() > 1 {
        // Instances other than Regular, Bold and Italic have names of their own.
        assert!(loaded.iter().any(|(family, _, legacy)| family != legacy), "{loaded:?}");
    }
}

#[test]
fn fallback_cache_is_dropped_when_a_face_loads_by_its_legacy_family_name() {
    // A lookup by a legacy family name ("Sysfont Sans3 Semibold") loads faces,
    // so fallbacks remembered before it are searched again.
    let dir = font_dir("fallback-legacy");
    std::fs::write(dir.join("Sysfont-Semibold.ttf"), renamed("SourceSans3-Semibold.ttf")).unwrap();
    let db = FontDb::with_font_dirs(vec![dir]);
    db.set_system_fallback(false);
    let latin = db.face(FALLBACK_FAMILY, "Regular");
    let thai = 'ก';
    assert!(db.fallback_for(thai, latin.id()).is_none());
    let before = db.read_faces().len();
    let semibold = format!("{FAMILY} Semibold");
    let f = db.face(&semibold, "Regular");
    assert_eq!((f.style.as_str(), f.legacy_family.as_str()), ("Semibold", semibold.as_str()));
    assert!(db.read_faces().len() > before, "the installed family loaded");
    assert!(db.fallback_for('A', f.id()).is_some_and(|g| g.id() != f.id()));
    let cache = db.fallbacks.read().unwrap();
    assert_eq!(cache.faces, db.read_faces().len());
    assert!(!cache.map.contains_key(&(thai, latin.id())), "answers from before the load are dropped");
}

/// `font` with its `cmap` replaced by one mapping `c` (a BMP character) to the glyph of "A", so
/// the face covers a character no bundled font has.
fn covering(font: &[u8], c: char) -> Vec<u8> {
    let be16 = |at: usize| u16::from_be_bytes([font[at], font[at + 1]]);
    let be32 = |at: usize| u32::from_be_bytes(font[at..at + 4].try_into().unwrap()) as usize;
    let gid = skrifa::MetadataProvider::charmap(&skrifa::FontRef::new(font).unwrap()).map('A').unwrap().to_u32() as u16;
    let c = c as u16;
    // One format 4 subtable of two segments: `c`, then the closing 0xFFFF.
    let cmap: Vec<u8> = [0u16, 1, 3, 1, 0, 12, 4, 32, 0, 4, 4, 1, 0, c, 0xFFFF, 0, c, 0xFFFF, gid.wrapping_sub(c), 1, 0, 0]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    let n = be16(4) as usize;
    let tables: Vec<([u8; 4], Vec<u8>)> = (0..n)
        .map(|i| {
            let r = 12 + 16 * i;
            let tag: [u8; 4] = font[r..r + 4].try_into().unwrap();
            let data = if &tag == b"cmap" { cmap.clone() } else { font[be32(r + 8)..be32(r + 8) + be32(r + 12)].to_vec() };
            (tag, data)
        })
        .collect();
    let mut out = font[..12].to_vec();
    let mut body = Vec::new();
    let mut offset = 12 + 16 * n;
    for (tag, data) in &tables {
        out.extend(tag);
        out.extend([0u8; 4]);
        out.extend((offset as u32).to_be_bytes());
        out.extend((data.len() as u32).to_be_bytes());
        let padded = data.len().div_ceil(4) * 4;
        body.extend(data);
        body.resize(body.len() + padded - data.len(), 0);
        offset += padded;
    }
    out.extend(body);
    out
}

/// A font installed after a fallback search for a character found none is used for it once a
/// rescan catalogs it (W3-59): the scan forgets the characters no font covered.
#[test]
fn a_rescan_forgets_characters_no_installed_font_covered() {
    let c = '\u{e9a1}';
    let dir = std::env::temp_dir().join(format!("word-sysfonts-{}-late-cover", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = FontDb::with_font_dirs(vec![dir.clone()]);
    let latin = db.face(FALLBACK_FAMILY, "Regular");
    assert!(db.fallback_for(c, latin.id()).is_none(), "nothing covers {c:?} yet");
    std::fs::write(dir.join("Sysfont-Regular.ttf"), covering(&renamed("SourceSans3-Regular.ttf"), c)).unwrap();
    assert_eq!(db.load_system_fonts(), 1);
    assert_eq!(db.fallback_for(c, latin.id()).map(|f| f.family.clone()).as_deref(), Some(FAMILY));
}
