//! `kition-doc`: the Doc kit for Kition's agent. The same document the in-page editor runs
//! (`kition-doc-kit`), served headless over the Kit Protocol by `kition-headless`, as the kit's
//! `kit.json` declares it, with the object model (`model.describe`, `model.batch`) and the replica
//! methods (`model.open`, `model.import`, `model.export`, `model.save`) over the document's change
//! exchange, and the identity a file carries (`file.identity`, `file.fork`): a `.docx`
//! is written from the engine's document and embeds no replica, so it carries none.
//!
//! The executable's `main` serves [`backend`]; tests drive the same backend in-process.
//!
//! Its documents reach files only through the host (KHP A29, A45, section 10.6): every document
//! it makes is hosted (`kition_doc_kit::files::serve_hosted`), so its engine reads a file a
//! command names only through the host, by its path in the workspace, reaches nothing else by
//! path, starts no program, and the process reads no font folder of the machine.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use kition_doc_kit::DocDocument;
use kition_headless::{FileIdentity, Headless, KitInfo};

/// The kit's manifest.
pub const MANIFEST: &str = include_str!("../../../kit.json");
/// Commands with file parameters an agent may run: none (the host opens, saves and exports).
const SANDBOXED: &[&str] = &[];

/// The kit as `kit.json` declares it.
pub fn kit() -> Result<KitInfo, String> {
    KitInfo::from_manifest(MANIFEST, SANDBOXED, kition_doc_kit::context::EDITOR_STATE)
}

/// The backend the executable serves: from now on every document this process makes is hosted.
pub fn backend(kit: &KitInfo) -> Headless<DocDocument> {
    kition_doc_kit::files::serve_hosted();
    Headless::<DocDocument>::with_replica_port(kit.clone()).with_file_identity(FileIdentity::NotEmbedded)
}
