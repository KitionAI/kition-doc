//! `kition-doc` reads and writes only inside its workspace, and only through its host (KHP A29,
//! A37, A42, A45, section 10.6): whatever an agent runs, nothing appears in the process's current
//! folder, its home folder or its temporary folder, nothing outside the workspace is read into an
//! answer, a picture or an export, changed or deleted, no program is started, no connection is
//! opened, and the workspace holds only what the test put there.
//!
//! It covers the ways an agent could otherwise reach files and programs: `model.batch` command
//! steps and the kit's own macros (`macro.list` with `file.save`, `file.exportPng`, `file.open`,
//! `text.insertFromFile`, `image.insert`, `mailMerge.recipients`, …), `file.save {}` of a
//! document opened from its replica (the process's current folder), and `readAloud.open`
//! called directly (`say`, `espeak` or `powershell`).
//!
//! The test runs itself again as a child process whose current folder, `HOME` and `TMPDIR` are
//! fresh empty folders and whose `PATH` starts with fake speech, print and opener programs that
//! only record that they ran, so every by-path write lands where it can be seen and no real
//! program is ever started. A folder beside them (`outside/`) holds this test's own sentinel
//! files, which the agent names by path (directly, inside `model.batch` as a script, as the agent
//! and as a person it claims to be, and inside a `macro.list` macro) and which a document in the
//! workspace links to (external pictures, an attached template, a mail merge data source, an
//! `altChunk`, a subdocument, `INCLUDETEXT`, `INCLUDEPICTURE`, `LINK` and `DDEAUTO` fields, an
//! external XML entity, and a remote URL served by a listener the test opens and checks was never
//! contacted). A font only the home folder's font folders hold must not be used. The child drives
//! the backend the executable serves ([`kition_doc::backend`]).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::io::Write;
use std::path::{Path, PathBuf};

use kition_doc_kit::DocDocument;
use kition_headless::{Headless, HostFolder};
use kition_kit::KitDocument;
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use serde_json::{Map, Value, json};

/// Set in the child: the folder holding `workspace/` and `outside/`.
const CHILD: &str = "KITION_DOC_TEST_SANDBOX";
/// Set in the child: the port of the listener the hostile document names as a remote server.
const PORT: &str = "KITION_DOC_TEST_PORT";
/// Text only files outside the workspace hold: it must reach no answer.
const SECRET: &str = "SENTINEL-7d2a";
/// A file outside the workspace that no call names: only a listing could find it.
const LISTED_ONLY: &str = "listed-only-7d2a";
/// A font family only the home folder's font folders hold, as long as "JetBrains Mono" (the
/// bundled monospaced face it is renamed from).
const HOME_FONT: &str = "Plant Home7d2a";
/// The colour of every picture outside the workspace.
const OUTSIDE_RGB: [u8; 3] = [17, 233, 71];
/// The programs the kit may try to start, faked first on `PATH`.
const FAKES: &[&str] = &["say", "espeak", "espeak-ng", "powershell", "lpr", "lpstat", "lp", "open", "xdg-open"];
/// What the workspace holds: what the test put there, nothing the kit wrote.
const WORKSPACE: &[&str] = &[".hidden.png", "basic.docx", "hostile.docx", "link.png", "page.html"];

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode(text: &str) -> Option<Vec<u8>> {
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0u32);
    for byte in text.trim_end_matches('=').bytes() {
        let value = B64.iter().position(|known| *known == byte)?;
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

/// Every file and folder under `dir`, relative to it, with a file's bytes (a link's target).
fn tree(dir: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().into_owned();
            let link = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink());
            if link {
                out.push((name, std::fs::read_link(&path).ok().map(|target| target.to_string_lossy().into_owned().into_bytes())));
            } else if path.is_dir() {
                stack.push(path.clone());
                out.push((name, None));
            } else {
                out.push((name, std::fs::read(&path).ok()));
            }
        }
    }
    out.sort();
    out
}

fn names(dir: &Path) -> Vec<String> {
    tree(dir).into_iter().map(|(name, _)| name).collect()
}

/// A 64 x 64 PNG of one colour.
fn png(rgb: [u8; 3]) -> Vec<u8> {
    let picture = image::RgbaImage::from_pixel(64, 64, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Whether some pixel of the PNG `bytes` is within 8 of `rgb` in every channel.
fn shows(bytes: &[u8], rgb: [u8; 3]) -> bool {
    let Ok(picture) = image::load_from_memory(bytes) else { return false };
    picture.to_rgba8().pixels().any(|px| (0..3).all(|c| px.0[c].abs_diff(rgb[c]) <= 8))
}

/// The rightmost dark pixel in `rows` of the PNG `bytes` (0 when there is none).
fn ink_right(bytes: &[u8], rows: std::ops::Range<u32>) -> u32 {
    let Ok(picture) = image::load_from_memory(bytes) else { return 0 };
    let picture = picture.to_rgba8();
    let mut right = 0;
    for y in rows.filter(|y| *y < picture.height()) {
        for x in 0..picture.width() {
            let px = picture.get_pixel(x, y).0;
            if px[3] > 0 && px[0] < 110 && px[1] < 110 && px[2] < 110 {
                right = right.max(x);
            }
        }
    }
    right
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn at(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// `file:///…` for `path`.
fn file_url(path: &str) -> String {
    format!("file://{path}")
}

/// A document saved by the kit's own engine whose text is `text`.
fn docx_with_text(text: &str) -> Vec<u8> {
    let mut doc = DocDocument::create("planted.docx", 612, 792).unwrap();
    doc.execute("text.insert", json!({ "text": text })).unwrap();
    doc.save(Some("planted.docx")).unwrap()
}

/// The bundled JetBrains Mono renamed [`HOME_FONT`] (as long as the original name).
fn home_font() -> Vec<u8> {
    let mut data = std::fs::read(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/JetBrainsMono-Regular.ttf"))).unwrap();
    let utf16 = |s: &str| s.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>();
    let mut renamed = 0;
    for (from, to) in [(utf16("JetBrains Mono"), utf16(HOME_FONT)), (b"JetBrains Mono".to_vec(), HOME_FONT.as_bytes().to_vec())] {
        assert_eq!(from.len(), to.len());
        let mut i = 0;
        while let Some(found) = data[i..].windows(from.len()).position(|w| w == from.as_slice()) {
            data[i + found..i + found + to.len()].copy_from_slice(&to);
            i += found + to.len();
            renamed += 1;
        }
    }
    assert!(renamed > 0, "the bundled face names itself JetBrains Mono");
    data
}

/// A Word package (the test's own XML) whose content names files outside the workspace in every
/// way the format allows, and a remote server at `port`.
fn hostile_docx(outside: &Path, port: u16) -> Vec<u8> {
    let secret_txt = at(outside, "secret.txt");
    let secret_png = at(outside, "secret.png");
    let planted = at(outside, "planted.docx");
    let remote = format!("http://127.0.0.1:{port}");
    let mono: String = "i".repeat(40);
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#;
    let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let rel = |id: &str, kind: &str, target: &str| {
        format!(
            r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{target}" TargetMode="External"/>"#
        )
    };
    let document_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdSettings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>{}{}{}{}{}{}{}{}</Relationships>"#,
        rel("rIdImg1", "image", &file_url(&secret_png)),
        rel("rIdImg2", "image", &secret_png),
        rel("rIdImg3", "image", "../outside/secret.png"),
        rel("rIdImg4", "image", &format!("{remote}/secret.png")),
        rel("rIdLink", "hyperlink", &format!("{remote}/page")),
        rel("rIdChunk", "aFChunk", &file_url(&secret_txt)),
        rel("rIdSub", "subDocument", &file_url(&planted)),
        rel("rIdOle", "oleObject", &file_url(&planted)),
    );
    let settings_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{}{}</Relationships>"#,
        rel("rIdTpl", "attachedTemplate", &file_url(&at(outside, "evil.dotm"))),
        rel("rIdSource", "mailMergeSource", &file_url(&at(outside, "source.csv"))),
    );
    let source = at(outside, "source.csv");
    let settings = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:attachedTemplate r:id="rIdTpl"/><w:linkStyles/><w:mailMerge><w:mainDocumentType w:val="formLetters"/><w:linkToQuery/><w:dataType w:val="textFile"/><w:connectString w:val="Data Source={source}"/><w:query w:val="SELECT * FROM {source}"/><w:dataSource r:id="rIdSource"/></w:mailMerge></w:settings>"#
    );
    let picture = |attr: &str, id: u32| {
        format!(
            r#"<w:p><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="914400" cy="914400"/><wp:docPr id="{id}" name="Picture {id}" descr="outside"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{id}" name="Picture {id}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip {attr}/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
        )
    };
    let field = |instr: &str, result: &str| {
        format!(
            r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {instr} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{result}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#
        )
    };
    let quoted = |text: &str| text.replace('\\', "\\\\");
    let simple = format!(
        r#"<w:p><w:fldSimple w:instr=" INCLUDEPICTURE &quot;{remote}/secret.png&quot; \d "><w:r><w:t>Remote picture</w:t></w:r></w:fldSimple></w:p>"#
    );
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE w:document [<!ENTITY outside SYSTEM "{entity}">]>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body><w:p><w:r><w:rPr><w:rFonts w:ascii="{HOME_FONT}" w:hAnsi="{HOME_FONT}" w:cs="{HOME_FONT}"/><w:sz w:val="24"/></w:rPr><w:t>{mono}</w:t></w:r></w:p><w:p><w:r><w:t xml:space="preserve">Entity here &outside;</w:t></w:r></w:p>{include_text}{include_picture}{link}{dde}{simple}<w:p><w:hyperlink r:id="rIdLink"><w:r><w:t>Remote page</w:t></w:r></w:hyperlink></w:p>{img1}{img2}{img3}{img4}<w:altChunk r:id="rIdChunk"/><w:p><w:subDoc r:id="rIdSub"/></w:p><w:p><w:r><w:object><o:OLEObject xmlns:o="urn:schemas-microsoft-com:office:office" Type="Link" ProgID="Word.Document.12" r:id="rIdOle" UpdateMode="Always"/></w:object></w:r></w:p><w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#,
        entity = file_url(&secret_txt),
        include_text = field(&format!(r#"INCLUDETEXT "{}""#, quoted(&secret_txt)), "Included text"),
        include_picture = field(&format!(r#"INCLUDEPICTURE "{}" \d"#, quoted(&secret_png)), "Included picture"),
        link = field(&format!(r#"LINK Word.Document.12 "{}" "" \a \t"#, quoted(&planted)), "Linked text"),
        dde = field(&format!(r#"DDEAUTO Word.Document.12 "{}" "" \a"#, quoted(&planted)), "Dde text"),
        img1 = picture(r#"r:embed="rIdImg1""#, 11),
        img2 = picture(r#"r:link="rIdImg2""#, 12),
        img3 = picture(r#"r:embed="rIdImg3""#, 13),
        img4 = picture(r#"r:link="rIdImg4""#, 14),
    );
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, text) in [
        ("[Content_Types].xml", content_types.to_owned()),
        ("_rels/.rels", root_rels.to_owned()),
        ("word/document.xml", document),
        ("word/_rels/document.xml.rels", document_rels),
        ("word/settings.xml", settings),
        ("word/_rels/settings.xml.rels", settings_rels),
    ] {
        out.start_file(name, zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)).unwrap();
        out.write_all(text.as_bytes()).unwrap();
    }
    out.finish().unwrap().into_inner()
}

/// The workspace's files and the sentinels outside it, made by the parent before the child runs.
fn plant(root: &Path, port: u16) {
    let workspace = root.join("workspace");
    let outside = root.join("outside");
    std::fs::copy(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.docx")), workspace.join("basic.docx")).unwrap();
    std::fs::write(workspace.join(".hidden.png"), png(OUTSIDE_RGB)).unwrap();
    for name in ["secret.png", "secret2.png"] {
        std::fs::write(outside.join(name), png(OUTSIDE_RGB)).unwrap();
    }
    // A symbolic link in the workspace that leads out of it.
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.join("secret.png"), workspace.join("link.png")).unwrap();
    #[cfg(not(unix))]
    std::fs::write(workspace.join("link.png"), png([200, 200, 200])).unwrap();
    std::fs::write(outside.join("secret.txt"), format!("{SECRET}\n")).unwrap();
    std::fs::write(outside.join("secret"), format!("{SECRET}\n")).unwrap();
    std::fs::write(outside.join(format!("{LISTED_ONLY}.txt")), SECRET).unwrap();
    std::fs::write(outside.join("sub/keep.txt"), SECRET).unwrap();
    std::fs::write(outside.join("source.csv"), format!("name,city\n{SECRET},{SECRET}\n")).unwrap();
    std::fs::write(outside.join("planted.docx"), docx_with_text(SECRET)).unwrap();
    std::fs::write(outside.join("evil.dotm"), docx_with_text(SECRET)).unwrap();
    std::fs::write(outside.join("page.html"), format!("<html><body><p>{SECRET}</p><img src=\"secret.png\"></body></html>")).unwrap();
    // An HTML page in the workspace whose pictures are outside it.
    std::fs::write(
        workspace.join("page.html"),
        format!(
            "<html><body><p>Workspace page</p><img src=\"../outside/secret.png\"><img src=\"{}\"><img src=\"{}\"></body></html>",
            at(&outside, "secret2.png"),
            file_url(&at(&outside, "secret.png"))
        ),
    )
    .unwrap();
    std::fs::write(workspace.join("hostile.docx"), hostile_docx(&outside, port)).unwrap();

    // A font only the home folder's font folders hold.
    let font = home_font();
    for folder in ["home/Library/Fonts", "home/.fonts", "home/.local/share/fonts"] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
        std::fs::write(root.join(folder).join("planted.ttf"), &font).unwrap();
    }

    // The programs the kit might start: each records that it ran, and does nothing else.
    for fake in FAKES {
        let script = root.join("bin").join(fake);
        std::fs::write(&script, format!("#!/bin/sh\necho \"$0 $*\" >> \"{}\"\n", at(&root.join("marks"), fake))).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
}

/// The agent's session: the backend the executable serves, over the workspace, and every answer
/// it gave.
struct Agent {
    kit: Headless<DocDocument>,
    folder: HostFolder,
    answers: Vec<(String, Value)>,
}

impl Agent {
    fn open(&mut self, name: &str) -> String {
        self.kit.open(OpenRequest { file: &self.folder.file(name).unwrap(), read_only: false, create: None }).unwrap().id
    }

    fn keep(&mut self, what: String, answer: Result<Value, String>) -> Result<Value, String> {
        self.answers.push((what, answer.clone().unwrap_or_else(Value::String)));
        answer
    }

    /// `command` as the agent calls it directly.
    fn run(&mut self, id: &str, command: &str, value: Value) -> Result<Value, String> {
        let answer =
            self.kit.execute(id, command, params(value), Origin::Agent).map(|done| done.result.unwrap_or(Value::Null)).map_err(|e| format!("{e:?}"));
        self.keep(command.to_owned(), answer)
    }

    /// `command` as one `command` operation of `model.batch`, with the batch's `origin` (none: the
    /// object model's default, a script; or one the caller claims).
    fn batch(&mut self, id: &str, command: &str, value: Value, origin: &Value) -> Result<Value, String> {
        let mut request = json!({ "documentId": id, "ops": [{ "op": "command", "id": command, "params": value }] });
        if !origin.is_null() {
            request["origin"] = origin.clone();
        }
        let answer = self.kit.model("model.batch", request).map_err(|e| format!("{e:?}"));
        self.keep(format!("model.batch {command} as {origin}"), answer)
    }

    /// `command` as the one step of a macro of the kit's own (`macro.list`), defined and run in
    /// one call.
    fn macro_step(&mut self, id: &str, command: &str, value: &Value) -> Result<Value, String> {
        let answer = self.run(id, "macro.list", json!({ "define": { "name": "m", "steps": [{ "command": command, "params": value }] }, "run": "m" }));
        if let Some(last) = self.answers.last_mut() {
            last.0 = format!("macro.list {command}");
        }
        answer
    }

    /// `command` every way an agent reaches it: directly, inside `model.batch` (as a script, as
    /// the agent, as a person it claims to be) and inside a macro.
    fn every_way(&mut self, id: &str, command: &str, value: &Value) {
        let _ = self.run(id, command, value.clone());
        for origin in [Value::Null, json!({ "kind": "agent", "principal": "agent:test" }), json!({ "kind": "person", "principal": "person:local" })] {
            let _ = self.batch(id, command, value.clone(), &origin);
        }
        let _ = self.macro_step(id, command, value);
    }

    /// Page `page` of document `id` as the kit exports it (PNG bytes), kept as an answer.
    fn page(&mut self, id: &str, page: u32) -> Vec<u8> {
        let answer = self.run(id, "file.export", json!({ "format": "png", "page": page, "scale": 1 })).unwrap_or_default();
        answer["data"].as_str().and_then(decode).unwrap_or_default()
    }

    /// The canvas of document `id`, as PNG bytes.
    fn screenshot(&mut self, id: &str) -> Vec<u8> {
        self.kit
            .screenshot(
                id,
                &kition_kit_server::ScreenshotRequest {
                    scope: kition_kit_server::protocol::ScreenshotScope::Canvas,
                    max_size: 768,
                    scale: 1.0,
                    target: None,
                },
            )
            .map(|shot| shot.png)
            .unwrap_or_default()
    }

    fn check(&mut self, id: &str) -> Value {
        let found = self.kit.check(id).unwrap_or(Value::Null);
        self.answers.push(("file.check".into(), found.clone()));
        found
    }

    fn model(&mut self, method: &str, request: Value) -> Value {
        let answer = self.kit.model(method, request).unwrap_or(Value::Null);
        self.answers.push((method.into(), answer.clone()));
        answer
    }
}

/// The value a command's file parameter gets: a sentinel outside the workspace for what it
/// reads, a new name outside it for what it writes.
fn target(command: &str, node: &Value, outside: &Path) -> String {
    if node["access"] != "read" {
        return at(outside, &format!("made-{command}.out"));
    }
    let name = match command {
        "file.open" => "page.html",
        "file.newFromTemplate" => "evil.dotm",
        "file.compare" | "file.combine" => "planted.docx",
        "mailMerge.recipients" | "mailMerge.editRecipients" => "source.csv",
        "image.insert" | "image.change" => "secret.png",
        "text.insertFromFile" | "object.insert" => "secret",
        _ => "secret.txt",
    };
    at(outside, name)
}

/// What the agent does in the child. Errors are fine; reaching files outside the workspace is not.
/// Returns what went wrong that the child can see (answers and pictures); the parent checks the
/// folders, the fake programs and the listener.
fn act(root: &Path, port: u16) -> Vec<String> {
    let workspace = root.join("workspace");
    let outside = root.join("outside");
    let mut wrong = Vec::new();

    let kit = kition_doc::kit().unwrap();
    let mut agent = Agent { kit: kition_doc::backend(&kit), folder: HostFolder::new(&workspace).unwrap(), answers: Vec::new() };
    agent.kit.set_host(agent.folder.host());
    let id = agent.open("basic.docx");
    if agent.run(&id, "text.insert", json!({ "text": "Typed by the agent. " })).is_err() {
        wrong.push("the agent cannot type in a document of its workspace".to_owned());
    }

    // Pictures by path: outside the workspace every way it can be named; none is inserted.
    let hostile_paths = [
        at(&outside, "secret.png"),
        "../outside/secret2.png".to_owned(),
        file_url(&at(&outside, "secret.png")),
        "sub/../../outside/secret.png".to_owned(),
        ".hidden.png".to_owned(),
        "link.png".to_owned(),
        format!("http://127.0.0.1:{port}/secret.png"),
    ];
    for path in &hostile_paths {
        agent.every_way(&id, "image.insert", &json!({ "path": path, "width": 60 }));
        agent.every_way(&id, "text.insertFromFile", &json!({ "path": path }));
    }
    if shows(&agent.page(&id, 1), OUTSIDE_RGB) {
        wrong.push("a picture outside the workspace was inserted (image.insert by path)".to_owned());
    }

    // Read Aloud starts no speech program, however it is asked for.
    for command in ["readAloud.open", "readAloud.playPause", "readAloud.next"] {
        agent.every_way(&id, command, &json!({}));
    }

    // Mail merge: a list given as data, merged into a file outside the workspace (never written),
    // and a data source by path (never read).
    let _ = agent.batch(&id, "mailMerge.recipients", json!({ "csv": "name\nAda\n" }), &json!({ "kind": "person", "principal": "person:local" }));
    agent.every_way(&id, "mailMerge.finish", &json!({ "path": at(&outside, "merged.docx") }));

    // Every command that names a file, with that parameter outside the workspace, every way the
    // agent reaches it, and climbing out of the workspace from its folder.
    let commands = DocDocument::create("sweep.docx", 64, 64).unwrap().commands();
    for command in &commands {
        let Some(schema) = command.schema.as_ref().and_then(Value::as_object) else { continue };
        let files: Vec<(&String, &Value)> = schema.iter().filter(|(_, node)| node["type"] == "file").collect();
        for (name, node) in files {
            agent.every_way(&id, &command.id, &json!({ name.as_str(): target(&command.id, node, &outside) }));
            agent.every_way(&id, &command.id, &json!({ name.as_str(): format!("../outside/climb-{}", command.id) }));
        }
    }
    // Saving with no path: the document's own name, never a file of this process's folders.
    for command in ["file.save", "file.saveAs", "edit.repeat"] {
        agent.every_way(&id, command, &json!({}));
    }

    // A document opened from its replica (an agent's draft) has no file: saving it with no path
    // writes nothing (it once wrote its bare name into the current folder).
    let exported = agent.model("model.export", json!({ "documentId": id }));
    let opened = agent.model("model.open", json!({ "name": "draft.docx", "snapshot": exported["changes"], "blobs": exported["blobs"] }));
    if let Some(draft) = opened["documentId"].as_str().map(str::to_owned) {
        agent.every_way(&draft, "file.save", &json!({}));
        agent.every_way(&draft, "file.saveAs", &json!({ "path": "draft-copy.docx" }));
    } else {
        wrong.push(format!("model.open of the agent's draft failed: {opened}"));
    }

    // An HTML page in the workspace whose pictures are outside it: opened through the host, and by
    // its workspace path.
    let page = agent.open("page.html");
    agent.every_way(&page, "file.open", &json!({ "path": "page.html" }));
    if shows(&agent.page(&page, 1), OUTSIDE_RGB) || shows(&agent.screenshot(&page), OUTSIDE_RGB) {
        wrong.push("page.html: a picture outside the workspace was read".to_owned());
    }

    // A document whose content names files outside the workspace and a remote server: nothing it
    // names is read, fetched or updated, and its own text is there.
    let hostile = agent.open("hostile.docx");
    let text = agent.run(&hostile, "file.text", json!({})).unwrap_or_default();
    if !text.to_string().contains("Entity here") {
        wrong.push(format!("hostile.docx: its own text is not there: {text}"));
    }
    for (command, value) in [
        ("file.inspect", json!({ "text": true })),
        ("field.updateAll", json!({})),
        ("edit.selectAll", json!({})),
        ("field.updateAll", json!({})),
        ("mailMerge.preview", json!({})),
        ("mailMerge.finish", json!({})),
        ("file.info", json!({})),
        ("file.compatibility", json!({})),
        ("file.removeHiddenContent", json!({})),
        ("file.export", json!({ "format": "pdf" })),
    ] {
        let _ = agent.run(&hostile, command, value);
    }
    let _ = agent.check(&hostile);
    let shown = agent.page(&hostile, 1);
    if shown.is_empty() {
        wrong.push("hostile.docx: the kit exported no page".to_owned());
    }
    if shows(&shown, OUTSIDE_RGB) || shows(&agent.screenshot(&hostile), OUTSIDE_RGB) {
        wrong.push("hostile.docx: a picture outside the workspace was drawn".to_owned());
    }
    // Its first line asks for a family only the home folder's font folders hold: drawn with a
    // face of the kit's own, the forty letters i take about 115 points from the left margin (72);
    // in the planted monospaced face, about 290.
    let right = ink_right(&shown, 64..100);
    if right > 260 {
        wrong.push(format!("hostile.docx: the first line reaches x = {right}: it was drawn with a font from the home folder"));
    }
    let _ = agent.model("model.export", json!({ "documentId": hostile }));
    let _ = agent.model("model.save", json!({ "documentId": hostile }));

    // Every command the kit offers the agent, with no parameters, on both documents.
    for doc in [&id, &hostile] {
        let offered: Vec<String> = agent.kit.commands(Some(doc.as_str())).unwrap().into_iter().map(|command| command.id).collect();
        for command in offered {
            if command == "file.close" {
                continue;
            }
            let _ = agent.run(doc, &command, json!({}));
        }
        let _ = agent.run(doc, "file.text", json!({}));
        let _ = agent.run(doc, "file.export", json!({ "format": "pdf" }));
        let _ = agent.model("model.export", json!({ "documentId": doc }));
    }

    // Nothing outside the workspace reached an answer: its files' text, a listing, a folder of this
    // process, the bytes of a picture outside.
    let home = root.join("home").to_string_lossy().into_owned();
    let tmp = root.join("tmp").to_string_lossy().into_owned();
    let outside_png = png(OUTSIDE_RGB);
    let leaks: [&[u8]; 4] = [SECRET.as_bytes(), LISTED_ONLY.as_bytes(), home.as_bytes(), tmp.as_bytes()];
    for (what, answer) in &agent.answers {
        let mut texts = Vec::new();
        strings(answer, &mut texts);
        for text in texts {
            for leak in leaks {
                if text.windows(leak.len()).any(|window| window == leak) {
                    wrong.push(format!("{what}: an answer holds `{}` from outside the workspace", String::from_utf8_lossy(leak)));
                }
            }
            if text.windows(outside_png.len()).any(|window| window == outside_png.as_slice()) {
                wrong.push(format!("{what}: an answer holds the bytes of a picture outside the workspace"));
            }
        }
    }
    wrong.sort();
    wrong.dedup();
    wrong
}

/// Every string in `value`, and the bytes of each that reads as base64.
fn strings(value: &Value, out: &mut Vec<Vec<u8>>) {
    match value {
        Value::String(text) => {
            out.push(text.as_bytes().to_vec());
            if text.len() >= 16
                && let Some(bytes) = decode(text)
            {
                out.push(bytes);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(fields) => fields.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}

#[test]
fn agent_commands_reach_no_file_outside_the_workspace() {
    if let Some(root) = std::env::var_os(CHILD) {
        let port = std::env::var(PORT).ok().and_then(|port| port.parse().ok()).unwrap_or(9);
        let wrong = act(&PathBuf::from(root), port);
        assert!(wrong.is_empty(), "{wrong:#?}");
        return;
    }
    let root = std::env::temp_dir().join(format!("kition-doc-workspace-only-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["workspace", "cwd", "home", "tmp", "outside/sub", "bin", "marks"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    let root = std::fs::canonicalize(root).unwrap();
    // The remote server the hostile document names: it must never be contacted.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    plant(&root, port);
    let outside = root.join("outside");
    let planted = tree(&outside);
    let home = tree(&root.join("home"));
    let workspace = tree(&root.join("workspace"));

    let path = format!("{}:/usr/bin:/bin", root.join("bin").display());
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "agent_commands_reach_no_file_outside_the_workspace", "--nocapture", "--test-threads=1"])
        .current_dir(root.join("cwd"))
        .env(CHILD, &root)
        .env(PORT, port.to_string())
        .env("HOME", root.join("home"))
        .env("TMPDIR", root.join("tmp"))
        .env("XDG_CONFIG_HOME", root.join("home/.config"))
        .env("XDG_DATA_HOME", root.join("home/.local/share"))
        .env("XDG_CACHE_HOME", root.join("home/.cache"))
        .env("PATH", path)
        // Read Aloud speaks unless this says otherwise: the child must be free to try.
        .env_remove("KITION_DOC_SPEECH")
        .status()
        .unwrap();
    // A program the child started may still be recording that it ran.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let mut wrong = Vec::new();
    if !status.success() {
        wrong.push(format!("the agent's session found something outside the workspace (see its output above): {status}"));
    }
    for dir in ["cwd", "tmp"] {
        let found = names(&root.join(dir));
        if !found.is_empty() {
            wrong.push(format!("the kit wrote into its {dir} folder: {found:?}"));
        }
    }
    if tree(&root.join("home")) != home {
        wrong.push(format!("the kit wrote into its home folder: {:?}", names(&root.join("home"))));
    }
    let started = names(&root.join("marks"));
    if !started.is_empty() {
        wrong.push(format!("the kit started programs: {started:?}"));
    }
    match listener.accept() {
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
        other => wrong.push(format!("the kit connected to the remote server the document names: {other:?}")),
    }
    let now = tree(&outside);
    if now != planted {
        let before: Vec<&String> = planted.iter().map(|(name, _)| name).collect();
        let after: Vec<&String> = now.iter().map(|(name, _)| name).collect();
        wrong.push(format!("the folder outside the workspace changed: before {before:?}, after {after:?}"));
    }
    let found = names(&root.join("workspace"));
    if found != WORKSPACE {
        wrong.push(format!("the workspace holds {found:?}, not only what the test put there ({WORKSPACE:?})"));
    } else if tree(&root.join("workspace")) != workspace {
        wrong.push("a file in the workspace changed".to_owned());
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    let _ = std::fs::remove_dir_all(&root);
}
