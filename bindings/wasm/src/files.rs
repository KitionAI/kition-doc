//! The parameters of this kit's commands that name files or folders on disk (KHP A28), declared
//! as typed files in the commands' schemas (`kition_kit::files`): a host lends them, and a
//! headless kit keeps those commands from agents (the host opens, saves and exports documents).
//!
//! A document the kit serves is hosted ([`hosted`]): its engine session reaches files only
//! through the host (`kition_doc_engine::Session::for_host`), and starts no program. The kit's own
//! check ([`refuse_for_agent`]) keeps the commands that name files or start programs from anyone
//! but a person however the call arrives: directly, as a step of the object model's
//! `model.batch`, as a step of a macro (`macro.list`) or as a repeat (`edit.repeat`): the engine
//! asks it before every command it runs.

use std::sync::atomic::{AtomicBool, Ordering};

use kition_doc_engine::Session;
use kition_doc_engine::disk::HostFiles;
use kition_doc_engine::doc::Document;
use kition_kit::files::FileParam;

/// Every file or folder parameter of the kit's commands.
pub const FILES: &[FileParam] = &[
    FileParam::read("mailMerge.editRecipients", "path", &["csv"]),
    FileParam::read("image.insert", "path", &[]),
    FileParam::write("file.save", "path", &[]),
    FileParam::read("mailMerge.recipients", "path", &[]),
    FileParam::create("mailMerge.finish", "path", &[]),
    FileParam::read("image.change", "path", &[]),
    FileParam::read("file.compare", "path", &[]),
    FileParam::read("file.combine", "path", &[]),
    FileParam::read("text.insertFromFile", "path", &[]),
    FileParam::read("file.open", "path", &[]),
    FileParam::create("file.saveAs", "path", &[]),
    FileParam::create("file.exportPdf", "path", &[]),
    FileParam::create("file.exportPng", "path", &[]),
    FileParam::read("file.newFromTemplate", "path", &[]),
    FileParam::create("file.saveTemplate", "path", &[]),
    FileParam::read("object.insert", "path", &[]),
];

/// Commands that name no file but start a program (Read Aloud's speech program): a hosted kit
/// refuses them, a served one does not list them, and the manifest's `agent.exclude` lists them
/// too, for the hosts that read it (KHP A45).
pub const AGENT_EXCLUDED: &[&str] = &["readAloud.open", "readAloud.playPause"];

/// Refuses `command` for anyone but a person (KHP A29, A45, A48): it names files or folders by
/// path or starts a program, and this kit works only on its workspace's files, through its host.
pub fn refuse_for_agent(command: &str) -> Result<(), String> {
    let names_files = FILES.iter().any(|file| file.command == command);
    if names_files || AGENT_EXCLUDED.contains(&command) {
        return Err(format!(
            "`{command}` reaches files by path or starts a program, and this kit works only on files of its workspace, through its \
             host: it is not available to the agent. The host opens, saves and exports the document (`file.export` answers PDF or \
             PNG as data); give pictures and mail merge lists as data (`image.insert {{\"data\"}}`, `mailMerge.recipients {{\"rows\"}}`)"
        ));
    }
    Ok(())
}

/// Whether this process serves its documents for a host ([`serve_hosted`]).
static SERVED: AtomicBool = AtomicBool::new(false);

/// From now on, every document this process makes is hosted: its host owns every file (KHP A29,
/// A45, section 10.6), and the process reads no font folder of the machine. `kition-doc` calls it
/// before it serves anything. (In the page, documents are always hosted.)
pub fn serve_hosted() {
    SERVED.store(true, Ordering::Relaxed);
    kition_doc_fonts::no_system_fonts();
}

/// Whether a host owns every file of the documents this process makes: in the page, and once
/// [`serve_hosted`] ran.
pub fn hosted() -> bool {
    cfg!(target_arch = "wasm32") || served()
}

/// Whether this native process serves its documents for a host ([`serve_hosted`]).
pub(crate) fn served() -> bool {
    SERVED.load(Ordering::Relaxed)
}

/// The files a hosted session reads: what the host lends (`kition_kit::host`).
const HOST_FILES: HostFiles = HostFiles { size: kition_kit::host::size, read: kition_kit::host::read };

/// `doc` as its save would read back (C8-doc-2 item 10), when served for a host's agents: what a
/// `.docx` reader keeps that the writer does not write (a picture whose media the file lacks, a
/// paragraph mark's link to a bookmark the file lacks) is dropped, and what the writer adds (an
/// empty note for a note reference whose note the file lacks, ids for paragraphs without one)
/// is there, so the document in memory is the one its next save holds and a reread of that save
/// finds nothing the agent was not told. Kept as it was when the round trip fails.
pub(crate) fn as_saved(name: &str, mut doc: Document) -> Document {
    let docx = name.rsplit_once('.').is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("docx"));
    if !served() || !docx {
        return doc;
    }
    doc.ensure_tool_ids();
    // Twice: what the first save adds (an empty note) reads back as the writer then keeps it.
    for _ in 0..2 {
        match kition_doc_engine::io::save_bytes(name, &doc).and_then(|bytes| kition_doc_engine::io::open_bytes(name, &bytes)) {
            Ok(saved) => doc = saved,
            Err(_) => break,
        }
    }
    doc
}

/// The engine session of a document the kit makes: for the host when [`hosted`], and keeping the
/// commands [`refuse_for_agent`] names from anyone but a person.
pub(crate) fn session(mut doc: Document) -> Session {
    // Every paragraph and table row has its stable tool id from the start (C8-land-14-doc).
    doc.ensure_tool_ids();
    let mut session = if hosted() { Session::for_host(doc, HOST_FILES) } else { Session::new(doc) };
    session.agent_gate = Some(refuse_for_agent);
    session.stable_ids = true;
    // Served for a host's agents, an edit carries its save stamp in its receipt (C8-doc-2).
    session.stamp_edits = served();
    session
}

#[cfg(test)]
mod tests {
    use kition_doc_engine::kdm_model::kition_kit::kdm::OriginKind;
    use kition_kit::KitDocument;

    use super::*;

    /// The manifest keeps from agents (`agent.exclude`) what the kit refuses them although it
    /// names no file; the kit refuses agents every command that names a file, and not the rest.
    #[test]
    fn the_manifest_excludes_what_the_kit_refuses_agents() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../kit.json")).unwrap_or_default();
        let excluded: Vec<&str> = manifest["agent"]["exclude"].as_array().into_iter().flatten().filter_map(serde_json::Value::as_str).collect();
        assert_eq!(excluded, AGENT_EXCLUDED);
        for command in AGENT_EXCLUDED.iter().chain(&["file.save", "file.exportPng", "image.insert", "mailMerge.recipients", "file.open"]) {
            assert!(refuse_for_agent(command).is_err(), "{command}");
        }
        for command in ["text.insert", "macro.list", "edit.repeat", "file.export", "readAloud.stop"] {
            assert!(refuse_for_agent(command).is_ok(), "{command}");
        }
        let refused = refuse_for_agent("file.save").err().unwrap_or_default();
        assert!(refused.contains("workspace") && refused.contains("file.export"), "{refused}");
    }

    /// Every step an agent runs passes the gate: directly, in a macro, as a repeat, and nested.
    #[test]
    fn every_step_an_agent_runs_passes_the_gate() {
        let dir = std::env::temp_dir().join(format!("kition-doc-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("{error}"));
        let target = dir.join("made.png").to_string_lossy().into_owned();
        let mut doc = crate::DocDocument::create("Untitled.docx", 64, 64).unwrap_or_else(|error| panic!("{error}"));
        let agent = kition_doc_engine::Origin::agent("agent:test");
        let step = serde_json::json!({ "command": "file.exportPng", "params": { "path": target } });
        assert!(doc.execute_as("file.exportPng", serde_json::json!({ "path": target }), &agent).is_err());
        let wrapped = serde_json::json!({ "define": { "name": "m", "steps": [step] }, "run": "m" });
        assert!(doc.execute_as("macro.list", wrapped.clone(), &agent).is_err(), "a macro's step is gated");
        // A person's macro may export (here, with no host, on the file system); an agent may not
        // run it again.
        doc.execute("macro.list", wrapped).unwrap_or_else(|error| panic!("{error}"));
        assert!(std::path::Path::new(&target).is_file());
        std::fs::remove_file(&target).unwrap_or_else(|error| panic!("{error}"));
        assert!(doc.execute_as("macro.list", serde_json::json!({ "run": "m" }), &agent).is_err());
        assert!(!std::path::Path::new(&target).exists(), "the agent's run of a person's macro wrote nothing");
        for kind in [OriginKind::Agent, OriginKind::Extension, OriginKind::System] {
            let origin = kition_doc_engine::Origin::new(kind, "test");
            assert!(doc.execute_as("readAloud.open", serde_json::json!({}), &origin).is_err(), "{kind:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Parameters whose names read like files but are not: ids, property paths, directions,
    /// folders of the kit's own panels, in-memory data.
    const NOT_FILES: &[(&str, &str)] =
        &[("equation.move", "dir"), ("file.paragraph", "path"), ("paragraph.dialog", "path"), ("drawing.erase", "path")];

    #[test]
    fn every_file_parameter_is_declared() {
        let doc = crate::DocDocument::create("Untitled.docx", 64, 64).unwrap_or_else(|error| panic!("{error}"));
        let problems = kition_kit::files::check(&doc.commands(), FILES, NOT_FILES);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
