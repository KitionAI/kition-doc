//! A hosted session (`Session::for_host`, KHP A29, A42, A45, section 10.6) reaches files only
//! through its host and starts no program; anyone but a person passes the session's gate at every
//! step; and the engine's code (and the code it links) reaches files, programs and connections
//! only through its door (`crate::disk`) or behind a refusal of hosted sessions.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use kition_doc_doc::{Document, StoryRef};
use serde_json::json;

use kdm_model::kition_kit::kdm::OriginKind;

use crate::disk::HostFiles;
use crate::{Origin, Session};

const SECRET: &str = "SENTINEL-41c9";
const LENT_TEXT: &str = "Lent from the workspace";

/// A 2 x 2 PNG.
fn picture() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([17, 233, 71, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// What the host lends: a text file and a picture in the workspace, by their paths there.
fn lent_read(path: &str) -> Result<Vec<u8>, String> {
    match path {
        "notes/inside.txt" => Ok(LENT_TEXT.as_bytes().to_vec()),
        "inside.png" => Ok(picture()),
        _ => Err(format!("{path}: not in the workspace")),
    }
}

fn lent_size(path: &str) -> Result<u64, String> {
    lent_read(path).map(|bytes| bytes.len() as u64)
}

const HOST: HostFiles = HostFiles { size: lent_size, read: lent_read };

/// A fresh folder outside any workspace, holding the sentinels.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-doc-hosted-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("secret.txt"), SECRET).unwrap();
    std::fs::write(dir.join("secret.csv"), format!("name\n{SECRET}\n")).unwrap();
    std::fs::write(dir.join("secret.png"), picture()).unwrap();
    std::fs::write(dir.join("page.html"), format!("<p>{SECRET}</p><img src=\"secret.png\">")).unwrap();
    dir
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

fn hosted() -> Session {
    let mut s = Session::for_host(Document::from_text("Hosted document"), HOST);
    // A document opened from a replica is named after its file: saving it with no path must not
    // write that name into the process's current folder.
    s.path = Some(PathBuf::from("draft.docx"));
    s
}

#[test]
fn a_hosted_session_writes_reads_and_starts_nothing_by_path() {
    let dir = folder("by-path");
    let at = |name: &str| dir.join(name).to_string_lossy().into_owned();
    let mut s = hosted();
    assert!(s.host_files_only());
    s.run("mailMerge.recipients", &json!({"csv": "name\nAda\n"})).unwrap();
    for (command, params) in [
        ("file.save", json!({})),
        ("file.save", json!({"path": at("made.docx")})),
        ("file.saveAs", json!({"path": at("made-as.docx")})),
        ("file.exportPdf", json!({"path": at("made.pdf")})),
        ("file.exportPng", json!({"path": at("made.png")})),
        ("file.saveTemplate", json!({"path": at("made")})),
        ("mailMerge.finish", json!({"path": at("merged.docx")})),
        ("file.open", json!({"path": at("page.html")})),
        ("file.open", json!({"path": at("secret.txt")})),
        ("file.newFromTemplate", json!({"path": at("secret.txt")})),
        ("text.insertFromFile", json!({"path": at("secret.txt")})),
        ("text.insertFromFile", json!({"path": "../secret.txt"})),
        ("text.insertFromFile", json!({"path": format!("file://{}", at("secret.txt"))})),
        ("object.insert", json!({"path": at("secret.txt")})),
        ("file.compare", json!({"path": at("secret.txt")})),
        ("file.combine", json!({"path": at("secret.txt")})),
        ("mailMerge.recipients", json!({"path": at("secret.csv")})),
        ("image.insert", json!({"path": at("secret.png")})),
        ("image.insert", json!({"path": "http://127.0.0.1:9/secret.png"})),
    ] {
        assert!(s.run(command, &params).is_err(), "{command} {params}");
    }
    assert_eq!(files(&dir), ["page.html", "secret.csv", "secret.png", "secret.txt"], "nothing was written outside the workspace");
    assert!(!Path::new("draft.docx").exists(), "nothing was written into the current folder");
    assert!(!s.doc.plain_text(StoryRef::Body).contains(SECRET), "nothing outside the workspace was read");
    assert!(s.doc.media.is_empty(), "no picture outside the workspace was read");
    assert!(s.merge.rows.iter().flatten().all(|cell| !cell.contains(SECRET)), "no data source outside the workspace was read");

    // What the host lends from the workspace is read through it.
    s.run("text.insertFromFile", &json!({"path": "notes/inside.txt"})).unwrap();
    assert!(s.doc.plain_text(StoryRef::Body).contains(LENT_TEXT));
    s.run("image.insert", &json!({"path": "inside.png"})).unwrap();
    assert_eq!(s.doc.media.len(), 1);

    // Read Aloud's speech program is not started.
    s.read_aloud.backend = crate::speech::Backend::System;
    assert!(s.run("readAloud.open", &json!({})).is_err());
    assert!(s.run("readAloud.playPause", &json!({})).is_err());
    assert_eq!(s.read_aloud.state(), crate::speech::State::Stopped);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The kit's gate in this test: the commands that name files.
fn gate(command: &str) -> Result<(), String> {
    if crate::cmd::family::family(command) == "file" || command == "text.insertFromFile" { Err(format!("{command}: not for agents")) } else { Ok(()) }
}

#[test]
fn every_step_an_agent_runs_passes_the_gate() {
    let dir = folder("gate");
    let copy = dir.join("copy.docx").to_string_lossy().into_owned();
    let mut s = Session::new(Document::from_text("Gated"));
    s.agent_gate = Some(gate);
    let agent = Origin::agent("agent:test");
    let save = json!({"command": "file.save", "params": {"path": copy}});
    let wrapped = json!({"define": {"name": "m", "steps": [save]}, "run": "m"});
    // Directly, in a macro, nested in a macro's macro, and the repeat of a person's command.
    assert!(s.execute_as("file.save", &json!({"path": copy}), &agent).is_err());
    assert!(s.execute_as("macro.list", &wrapped, &agent).is_err());
    let nested = json!({"define": {"name": "outer", "steps": [{"command": "macro.list", "params": wrapped}]}, "run": "outer"});
    let _ = s.execute_as("macro.list", &nested, &agent);
    s.run("text.insertFromFile", &json!({"path": dir.join("secret.txt").to_string_lossy()})).unwrap();
    assert!(s.execute_as("edit.repeat", &json!({}), &agent).is_err(), "a person's command repeated by an agent is gated");
    for kind in [OriginKind::Extension, OriginKind::System, OriginKind::Import] {
        let origin = Origin::new(kind, "test");
        assert!(s.execute_as("file.save", &json!({"path": copy}), &origin).is_err(), "{kind:?}");
    }
    assert_eq!(files(&dir), ["page.html", "secret.csv", "secret.png", "secret.txt"]);
    // A person runs what the gate keeps from agents (here, with no host, on the file system).
    s.execute_as("macro.list", &json!({"run": "m"}), &Origin::person("person:local")).unwrap();
    assert!(Path::new(&copy).is_file());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Strips `#[cfg(test)]` / `#[cfg(all(test, …))]` items from Rust source: what is compiled into
/// the program.
fn without_tests(source: &str) -> String {
    let mut out = String::new();
    let mut rest = source;
    while let Some(at) = ["#[cfg(test)]", "#[cfg(all(test"].iter().filter_map(|marker| rest.find(marker)).min() {
        out.push_str(&rest[..at]);
        let item = &rest[at..];
        // Skip to the item's closing brace, ignoring braces in strings, chars and comments.
        let (mut depth, mut started, mut end) = (0i32, false, item.len());
        let bytes = item.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    i = item[i..].find('\n').map_or(bytes.len(), |n| i + n);
                }
                b'r' if (!bytes.get(i.wrapping_sub(1)).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                    || (bytes.get(i.wrapping_sub(1)) == Some(&b'b')
                        && !bytes.get(i.wrapping_sub(2)).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')))
                    && matches!(bytes.get(i + 1), Some(b'#' | b'"')) =>
                {
                    // A raw string, including byte strings: `r"…"`, `br#"…"#`.
                    let hashes = item[i + 1..].bytes().take_while(|b| *b == b'#').count();
                    if bytes.get(i + 1 + hashes) == Some(&b'"') {
                        let close = format!("\"{}", "#".repeat(hashes));
                        let from = i + 2 + hashes;
                        i = item[from..].find(&close).map_or(bytes.len(), |n| from + n + close.len() - 1);
                    }
                }
                b'"' => {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'"' {
                        i += if bytes[i] == b'\\' { 2 } else { 1 };
                    }
                }
                b'\'' if bytes.get(i + 2) == Some(&b'\'') || (bytes.get(i + 1) == Some(&b'\\') && bytes.get(i + 3) == Some(&b'\'')) => {
                    i += if bytes.get(i + 1) == Some(&b'\\') { 3 } else { 2 };
                }
                b'{' => {
                    depth += 1;
                    started = true;
                }
                b'}' => {
                    depth -= 1;
                    if started && depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                b';' if !started => {
                    end = i + 1;
                    break;
                }
                _ => {}
            }
            i += 1;
        }
        rest = &item[end.min(item.len())..];
    }
    out.push_str(rest);
    out
}

#[test]
fn byte_raw_strings_stay_inside_test_items() {
    // RTF has quotes inside a raw byte string. Its braces must not close the
    // Rust test module early, and real code after that module must remain scanned.
    for literal in [
        r##"br#"{\field{\*\fldinst{HYPERLINK "http://a.b"}}{\fldrslt{link}}}"#"##,
        r##"r#"{\field{\*\fldinst{HYPERLINK "http://a.b"}}{\fldrslt{link}}}"#"##,
        r##"br"{}""##,
    ] {
        let source = format!(
            "fn before() {{}}\n#[cfg(test)]\nmod tests {{\n#[test]\nfn first() {{ let _ = {literal}; }}\n#[test]\nfn second() {{}}\n}}\nfn after() {{ std::fs::read(\"kept\"); }}"
        );
        let scanned = without_tests(&source);
        assert_eq!(scanned, "fn before() {}\n\nfn after() { std::fs::read(\"kept\"); }", "{literal}");
        assert!(names(&scanned, "std::fs::"), "production access after the tests stays visible");
    }
}

/// Whether `code` names `reach` as a path of its own (`CompoundFile::open` is not `File::open`).
fn names(code: &str, reach: &str) -> bool {
    code.match_indices(reach).any(|(at, _)| !code[..at].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// The engine and the crates it links reach the file system, start programs or open connections
/// only in [`crate::disk`], which refuses all of it in a hosted session, and in code that refuses
/// a hosted session or a served process first: Read Aloud's speech program (`disk::need` on the
/// command's thread) and the machine's font folders (none after `no_system_fonts`).
#[test]
fn the_engine_reaches_files_and_programs_only_through_its_door() {
    /// (crate-relative file, what it must hold): files that touch the file system or start
    /// programs only behind that refusal.
    const GUARDED: &[(&str, &str)] = &[
        ("engine/engine/src/disk.rs", "fn hosted()"),
        ("engine/engine/src/speech.rs", "crate::disk::need(\"Read Aloud\")"),
        ("support/fonts/src/fontdb.rs", "!SYSTEM_FONTS.load(Ordering::Relaxed)"),
        ("support/fonts/src/ui_fallback.rs", "crate::system_font_dirs()"),
    ];
    const REACH: &[&str] = &[
        "std::fs::",
        "std::process::",
        "Command::new",
        "tempfile::",
        "std::net::",
        "TcpStream",
        "UdpSocket",
        "File::open",
        "File::create",
        "OpenOptions",
        "reqwest::",
        "ureq::",
    ];
    // Every crate the kit links: the engine and what it builds on (crates/*/*/src).
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let crates = std::fs::canonicalize(crates).unwrap();
    let mut stack: Vec<PathBuf> = std::fs::read_dir(&crates)
        .unwrap()
        .flatten()
        .flat_map(|group| std::fs::read_dir(group.path()).into_iter().flatten().flatten().map(|krate| krate.path().join("src")))
        .filter(|src| src.is_dir())
        .collect();
    assert!(stack.len() >= 10, "the scan found the kit's crates ({})", stack.len());
    let mut found = Vec::new();
    let mut guarded = 0;
    let mut lent_reads = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if !name.ends_with(".rs") || name.starts_with("tests") || name.ends_with("_tests.rs") {
                continue;
            }
            let relative = path.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/");
            let source = without_tests(&std::fs::read_to_string(&path).unwrap());
            lent_reads += source.matches("crate::disk::read_lent(").count();
            assert!(!source.contains("#[test]"), "{relative}: a test was left in the scanned code");
            if let Some((_, must)) = GUARDED.iter().find(|(file, _)| *file == relative) {
                assert!(source.contains(must), "{relative} refuses a hosted session first ({must})");
                guarded += 1;
                continue;
            }
            for (n, line) in source.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                if let Some(reach) = REACH.iter().find(|reach| names(code, reach)) {
                    found.push(format!("{relative}:{}: {reach}", n + 1));
                }
            }
        }
    }
    assert_eq!(guarded, GUARDED.len(), "the scan saw every guarded file");
    assert!(lent_reads >= 4, "the scan saw the engine's reads through its host ({lent_reads})");
    assert!(found.is_empty(), "file system, program or network access outside the engine's door: {found:#?}");
}
