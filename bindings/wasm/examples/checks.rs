//! Writes the conformance fixtures (KHP `spec/conformance.md`): `basic.docx`, which `doc_check`
//! finds clean, and one `check-<code>.docx` per code of `agent/checks.json`, each a document on
//! which the check reports that code. `cargo run -p kition-doc-kit --example checks -- conformance`.
//! The documents are built on the model and written by the engine's own DOCX writer, so they are
//! the same on every run.

use std::path::Path;
use std::sync::Arc;

use kition_doc_doc::para::{InlineObject, NoteKind, OBJ, Paragraph, Run};
use kition_doc_doc::props::{CharProps, ParaProps};
use kition_doc_doc::table::Table;
use kition_doc_doc::{Block, Document, Part, PartKind};

/// The serif the engine carries itself, so no machine draws it with a substitute and only the
/// fixture's own finding shows.
const FONT: &str = "Source Serif 4";

fn props() -> CharProps {
    CharProps { font: Some(FONT.into()), ..CharProps::default() }
}

fn para(text: &str) -> Arc<Block> {
    Arc::new(Block::Para(Paragraph::with_text(text, props())))
}

/// A paragraph whose U+FFFC characters are `objects`.
fn with_objects(text: &str, objects: Vec<InlineObject>) -> Arc<Block> {
    let mut p = Paragraph::new();
    p.text = text.into();
    p.runs = vec![Run { len: text.len(), props: props() }];
    p.objects = objects;
    Arc::new(Block::Para(p))
}

fn document(body: Vec<Arc<Block>>) -> Document {
    let mut doc = Document::new();
    doc.body = body;
    doc
}

fn linked(text: &str, target: &str) -> Arc<Block> {
    let mut p = Paragraph::with_text(text, props());
    p.runs[0].props.link = Some(target.into());
    Arc::new(Block::Para(p))
}

fn image(media: &str, alt: &str) -> InlineObject {
    InlineObject::Image {
        edge: Default::default(),
        geom: None,
        media: media.into(),
        w: 40.0,
        h: 30.0,
        alt: alt.into(),
        float: Default::default(),
        crop: [0.0; 4],
        ole: None,
        adjust: Default::default(),
        uid: 0,
    }
}

/// A 1 × 1 PNG, for pictures whose image the document holds.
const PIXEL: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn fixtures() -> Vec<(&'static str, Document)> {
    let heading = |text: &str| {
        let mut p = Paragraph::with_text(text, props());
        p.props = ParaProps { style: Some("Heading1".into()), ..ParaProps::default() };
        Arc::new(Block::Para(p))
    };
    let basic = document(vec![heading("Quarterly report"), para("Sales rose in every region."), para("The team thanks everyone involved.")]);

    let mut empty_heading = Paragraph::new();
    empty_heading.props = ParaProps { style: Some("Heading1".into()), ..ParaProps::default() };
    let empty_heading = document(vec![Arc::new(Block::Para(empty_heading)), para("Text under a heading with no title.")]);

    let broken_link = document(vec![para("Introduction"), linked("See the summary", "#summary")]);

    let marks = format!("{OBJ}Intro");
    let field = format!("See page {OBJ}.");
    let broken_reference = document(vec![
        with_objects(&marks, vec![InlineObject::BookmarkStart { name: "intro".into() }]),
        with_objects(
            &field,
            vec![InlineObject::Field {
                instr: " PAGEREF results \\h ".into(),
                result: "2".into(),
                locked: false,
                code_runs: Vec::new(),
                code_fields: Vec::new(),
            }],
        ),
    ]);

    let errored = format!("Figure {OBJ}: the chart.");
    let field_error = document(vec![with_objects(
        &errored,
        vec![InlineObject::Field {
            instr: "SEQ Figure".into(),
            result: "Error! No sequence specified.".into(),
            locked: false,
            code_runs: Vec::new(),
            code_fields: Vec::new(),
        }],
    )]);

    let picture = format!("{OBJ}");
    // Written with its image, which `strip` then takes out of the package: the writer leaves out a
    // picture whose image the document does not hold.
    let mut missing_media = document(vec![with_objects(&picture, vec![image("img1", "A chart of sales")])]);
    missing_media.media.insert("img1".into(), Arc::new(PIXEL.to_vec()));

    let mut missing_alt = document(vec![with_objects(&picture, vec![image("img1", "")])]);
    missing_alt.media.insert("img1".into(), Arc::new(PIXEL.to_vec()));

    let noted = format!("A claim{OBJ}.");
    // Written with its footnote, whose text `drop_footnotes` then takes out of the package.
    let mut missing_note =
        document(vec![with_objects(&noted, vec![InlineObject::NoteRef { kind: NoteKind::Footnote, id: 9, custom: String::new() }])]);
    missing_note.parts.insert(9, Part { kind: PartKind::Footnote, blocks: vec![para("The source.")] });

    let mut table = Table::new(2, 3, 300.0);
    table.rows[1].cells.pop();
    let ragged_table = document(vec![para("A table whose second row is short."), Arc::new(Block::Table(table))]);

    let substituted = CharProps { font: Some("No Such Font Family 42".into()), ..CharProps::default() };
    let font_substituted = document(vec![Arc::new(Block::Para(Paragraph::with_text("Set in a font this machine lacks.", substituted)))]);

    let mut added = Paragraph::with_text("An added sentence.", props());
    added.runs[0].props.ins = Some(0);
    let mut tracked_changes = document(vec![para("Kept text."), Arc::new(Block::Para(added))]);
    tracked_changes.revisions.push(Default::default());

    let truncated = document((0..105).map(|i| linked(&format!("Link {i}"), "#missing")).collect());

    vec![
        ("basic.docx", basic),
        ("check-empty-heading.docx", empty_heading),
        ("check-broken-link.docx", broken_link),
        ("check-broken-reference.docx", broken_reference),
        ("check-field-error.docx", field_error),
        ("check-missing-media.docx", missing_media),
        ("check-missing-alt-text.docx", missing_alt),
        ("check-missing-note.docx", missing_note),
        ("check-ragged-table.docx", ragged_table),
        ("check-font-substituted.docx", font_substituted),
        ("check-tracked-changes.docx", tracked_changes),
        ("check-truncated.docx", truncated),
    ]
}

/// The package `bytes` without the entries `keep` refuses.
fn strip(bytes: &[u8], keep: impl Fn(&str) -> bool) -> Result<Vec<u8>, String> {
    use std::io::{Read, Write};
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|error| error.to_string())?;
        let name = entry.name().to_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|error| error.to_string())?;
        if keep(&name) {
            out.start_file(name, options).map_err(|error| error.to_string())?;
            out.write_all(&data).map_err(|error| error.to_string())?;
        }
    }
    Ok(out.finish().map_err(|error| error.to_string())?.into_inner())
}

/// The package `bytes` with the notes of `word/footnotes.xml` taken out (the separators, which
/// carry a `w:type`, stay), while the references in the body stay.
fn drop_footnotes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::{Read, Write};
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut dropped = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|error| error.to_string())?;
        let name = entry.name().to_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|error| error.to_string())?;
        if name == "word/footnotes.xml" {
            let mut text = String::from_utf8(data).map_err(|error| error.to_string())?;
            while let Some(start) = text.match_indices("<w:footnote ").map(|(at, _)| at).find(|&at| {
                let head = &text[at..text[at..].find('>').map_or(text.len(), |end| at + end)];
                !head.contains("w:type=")
            }) {
                let end = text[start..].find("</w:footnote>").ok_or("an unclosed footnote")? + start + "</w:footnote>".len();
                text.replace_range(start..end, "");
                dropped += 1;
            }
            data = text.into_bytes();
        }
        out.start_file(name, options).map_err(|error| error.to_string())?;
        out.write_all(&data).map_err(|error| error.to_string())?;
    }
    if dropped == 0 {
        return Err("no footnote to take out".into());
    }
    Ok(out.finish().map_err(|error| error.to_string())?.into_inner())
}

fn main() -> Result<(), String> {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "conformance".into());
    let only: Vec<String> = std::env::args().skip(2).collect();
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    for (name, doc) in fixtures() {
        if !only.is_empty() && !only.iter().any(|wanted| wanted == name) {
            continue;
        }
        let mut bytes = kition_doc_engine::io::save_bytes(name, &doc)?;
        match name {
            "check-missing-media.docx" => bytes = strip(&bytes, |entry| !entry.starts_with("word/media/"))?,
            "check-missing-note.docx" => bytes = drop_footnotes(&bytes)?,
            _ => {}
        }
        std::fs::write(Path::new(&dir).join(name), &bytes).map_err(|error| format!("{name}: {error}"))?;
        println!("wrote {name}");
        if name == "basic.docx" {
            golden(Path::new(&dir), &bytes)?;
        }
    }
    Ok(())
}

/// `golden/basic.png` and `golden/basic-scale-2.png`: the engine's own render of `basic.docx`'s
/// first page, at 1 and 2 pixels per point, which `<prefix>_inspect { preview }` must match.
fn golden(dir: &Path, bytes: &[u8]) -> Result<(), String> {
    use kition_kit::KitDocument;
    let doc = kition_doc_kit::DocDocument::open("basic.docx", bytes)?;
    let golden = dir.join("golden");
    std::fs::create_dir_all(&golden).map_err(|error| error.to_string())?;
    for (file, scale) in [("basic.png", 1.0), ("basic-scale-2.png", 2.0)] {
        let (width, height) = ((f64::from(doc.width()) * scale) as u32, (f64::from(doc.height()) * scale) as u32);
        let rgba = doc.render(0.0, 0.0, scale, width, height)?;
        let picture = image::RgbaImage::from_raw(width, height, rgba).ok_or("a render of the wrong size")?;
        picture.save(golden.join(file)).map_err(|error| error.to_string())?;
        println!("wrote golden/{file}");
    }
    Ok(())
}
