//! Every local id stored as data must be written as an object id and translated back
//! (`kdm_model`'s `refs` module and its explicit cases). Kition Doc's ids are plain integers (and
//! media keys strings), so this lists every model field typed with an integer or naming media: a
//! new one fails here until it is classified (an id the bridge handles, or not an id).

const SOURCES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../../engine/doc/src/lib.rs")),
    ("para.rs", include_str!("../../../engine/doc/src/para.rs")),
    ("props.rs", include_str!("../../../engine/doc/src/props.rs")),
    ("section.rs", include_str!("../../../engine/doc/src/section.rs")),
    ("numbering.rs", include_str!("../../../engine/doc/src/numbering.rs")),
    ("graphic.rs", include_str!("../../../engine/doc/src/graphic.rs")),
    ("table.rs", include_str!("../../../engine/doc/src/table.rs")),
    ("styles.rs", include_str!("../../../engine/doc/src/styles.rs")),
    ("freeform.rs", include_str!("../../../engine/doc/src/freeform.rs")),
    ("effects.rs", include_str!("../../../engine/doc/src/effects.rs")),
];

/// `file: field` for every integer or media field of the stored model, and how the bridge handles it.
const HANDLED: &[&str] = &[
    // Definitions: each is an object's own local id, minted by the reader (never replicated).
    "lib.rs: parts: BTreeMap<u32, Part>",
    "lib.rs: comments: BTreeMap<u32, Comment>",
    "lib.rs: media: BTreeMap<String, Arc<Vec<u8>>>",
    // `Num::id`: the UUID key of the list's `lists` entry; `AbstractNum::id` and
    // `Num::abstract_id`: the abstract list's UUID (`Writer::write_lists`, `Reader::lists`).
    "numbering.rs: id: u32",
    "numbering.rs: abstract_id: u32",
    // Comment.parent and Comment.part: the Comment object's `parent` and `part` ref fields.
    "lib.rs: parent: Option<u32>",
    "lib.rs: part: u32",
    // refs::walk: `ins`/`del`, `rev` of `fmtChange`/`numChange`, `num` of `numbering`,
    // `default`/`first`/`even` of `headers`/`footers`, `story` of a shape, `id` of a note
    // reference or comment anchor, `media` of a picture (inline, or in a chart or diagram).
    "props.rs: ins: Option<u32>",
    "props.rs: del: Option<u32>",
    "props.rs: rev: u32",
    "props.rs: num: u32",
    "section.rs: default: Option<u32>",
    "section.rs: first: Option<u32>",
    "section.rs: even: Option<u32>",
    "para.rs: story: Option<u32>",
    "para.rs: id: u32",
    "para.rs: media: String",
    "graphic.rs: Image { rect: [f32; 4], media: String }",
    // Not ids: counts, sizes, numbers shown, settings, layout caches.
    "lib.rs: open: Vec<u32>",
    "lib.rs: nested: usize",
    "lib.rs: total: usize",
    "lib.rs: off: usize",
    "lib.rs: revision: u32",
    "lib.rs: compat_mode: u32",
    "para.rs: len: usize",
    // `CodeField::start`: a byte offset into the enclosing field's code, not an object id.
    "para.rs: start: usize",
    "para.rs: rev: u64",
    "props.rs: span: u32",
    // Paragraph spacing in hundredths of a line, not object references.
    "props.rs: space_before_lines: Option<u32>",
    "props.rs: space_after_lines: Option<u32>",
    // Manual-width and East Asian group IDs only link adjacent runs, never replica objects.
    "props.rs: id: Option<i32>",
    // `w14:paraId` of a paragraph or row: the file's id, kept as a value (the writer keeps the
    // package's ids unique), not a reference.
    "props.rs: para_id: Option<u32>",
    // A shape's or chart's id in the document (KitDrawing DR2, the drawing's `wp:docPr/@id`): the
    // same on every replica, a value, not a reference to another object.
    "para.rs: uid: u32",
    "graphic.rs: uid: u32",
    // A floating object's place in the stack (`wp:anchor/@relativeHeight`): a value in the
    // object's frame, not a reference.
    "para.rs: z: Option<i64>",
    // A connector's glued ends: the shapes' document ids (`uid`, KitDrawing DR2, the same on every
    // replica) and their connection sites, values like `uid` itself.
    "graphic.rs: start: Option<(u32, u32)>",
    "graphic.rs: end: Option<(u32, u32)>",
    "section.rs: count: u32",
    "section.rs: count_by: u32",
    "section.rs: start: u32",
    "section.rs: page_num_start: Option<u32>",
    // Document-grid measurements, in twips and signed 1/4096-point units, not object ids.
    "section.rs: line_pitch: Option<i32>",
    "section.rs: char_space: Option<i32>",
    "numbering.rs: start: u32",
    "numbering.rs: start_overrides: Vec<(u8, u32)>",
    "numbering.rs: state: HashMap<u32, [u32; 9]>",
    "numbering.rs: seen: HashMap<u32, bool>",
    "styles.rs: priority: Option<u32>",
    "styles.rs: band_size: Option<u32>",
];

fn names_integer(line: &str) -> bool {
    line.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').any(|t| matches!(t, "u32" | "u64" | "usize" | "i32" | "i64" | "u16"))
}

#[test]
fn every_id_field_is_handled() {
    let mut found = Vec::new();
    for (file, source) in SOURCES {
        for line in source.lines() {
            let line = line.trim();
            if line.starts_with("//")
                || line.starts_with('#')
                || line.contains("fn ")
                || line.contains("let ")
                || line.contains("=>")
                || line.contains(" = ")
                || line.contains("::new()")
                || line.contains("Some(")
            {
                continue;
            }
            let line = line.trim_start_matches("pub ").trim_end_matches(',');
            let Some((name, _)) = line.split_once(": ") else { continue };
            let field_like = name.chars().all(|c| c.is_ascii_lowercase() || c == '_') || name.contains(" { ");
            if field_like && (names_integer(line) || name == "media" || line.contains(" media: ")) {
                found.push(format!("{file}: {line}"));
            }
        }
    }
    found.sort();
    found.dedup();
    let unhandled: Vec<_> = found.iter().filter(|f| !HANDLED.contains(&f.as_str())).collect();
    assert!(unhandled.is_empty(), "model fields holding integers or media that kdm-model does not classify: {unhandled:#?}");
    let stale: Vec<_> = HANDLED.iter().filter(|h| !found.iter().any(|f| f == *h)).collect();
    assert!(stale.is_empty(), "listed as handled but no longer in the model (update this list): {stale:#?}");
}
