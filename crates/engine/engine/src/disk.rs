//! The engine's door to files named by path, and the host-files-only mode (KHP A29, A42, A45,
//! section 10.6).
//!
//! A session made for a host ([`crate::Session::for_host`]: a kit's document, served headless or
//! in the page) works only on the files its host owns. While such a session runs a command
//! ([`enter`], from [`crate::Session::run`]), the engine
//!
//! - reads a file a command names (a picture, a text file, a document to compare, a mail merge
//!   data source) only through the host ([`read_lent`]), by a plain path inside the workspace:
//!   absolute paths, URLs (`file:`, `http:`, …), drives, `..` and hidden names are refused here,
//!   and the host refuses anything else outside the workspace (a link that leads out);
//! - reaches nothing else by path: no read, write, rename, file status or folder ([`read`],
//!   [`write`], [`metadata`], …), and starts no program ([`need`] before Read Aloud's speech
//!   program).
//!
//! Outside such a session (the engine's own tests and tools) these are the file system's. Links a
//! document holds (external pictures, templates, data sources, `INCLUDETEXT` and the other
//! fields) are never resolved anywhere: they stay as data.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use crate::CmdError;

/// How a hosted session reads the files its host lends, by the path a command names
/// (`kition_kit::host::size` and `read` for a kit).
#[derive(Clone, Copy)]
pub struct HostFiles {
    pub size: fn(&str) -> Result<u64, String>,
    pub read: fn(&str) -> Result<Vec<u8>, String>,
}

impl std::fmt::Debug for HostFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostFiles")
    }
}

thread_local! {
    /// The host of the hosted session running a command on this thread, if any.
    static HOST: Cell<Option<HostFiles>> = const { Cell::new(None) };
}

/// While a session runs: hosted when `host` is `Some`, and a hosted scope stays hosted (a command
/// run inside one reaches no more than its caller).
pub(crate) struct Scope(Option<HostFiles>);

pub(crate) fn enter(host: Option<HostFiles>) -> Scope {
    let previous = HOST.get();
    if host.is_some() {
        HOST.set(host);
    }
    Scope(previous)
}

impl Drop for Scope {
    fn drop(&mut self) {
        HOST.set(self.0);
    }
}

fn host() -> Option<HostFiles> {
    HOST.get()
}

/// Whether the command running now belongs to a hosted session.
pub fn hosted() -> bool {
    host().is_some()
}

/// The error for `what` in a hosted session: it would reach files by path, or start a program,
/// and the host owns every file (KHP A29, A45; the message says what to do, A48).
pub fn refusal(what: &str) -> CmdError {
    CmdError::Disabled(format!(
        "{what} is not available here: this kit works only on files of its workspace, through its host, and starts no program; \
         open, save and export documents through the host, and give pictures, text and mail merge lists as data"
    ))
}

/// Refuses `what` in a hosted session.
pub fn need(what: &str) -> Result<(), CmdError> {
    if hosted() { Err(refusal(what)) } else { Ok(()) }
}

fn denied(path: &Path) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{}: outside this kit's workspace", path.display()))
}

/// Why `path` may not be lent: it is not a plain path inside the workspace (an absolute path, a
/// URL or a drive, a hidden name, a step up with `..`, an empty or overlong name). A granted
/// file's handle passes.
pub fn not_lent(path: &str) -> Option<String> {
    const MAX: usize = 4096;
    let refuse = |why: &str| Some(format!("{path}: {why}; name a file in the workspace by its path"));
    if path.trim().is_empty() || path.len() > MAX || path.contains('\0') {
        return refuse("not a file name");
    }
    if path.starts_with(['/', '\\']) {
        return refuse("an absolute path is outside the workspace");
    }
    // `file:`, `http:`, `C:`: a scheme or a drive, never fetched.
    if let Some((scheme, _)) = path.split_once(':')
        && !scheme.is_empty()
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return refuse("a URL or a drive is not a file of the workspace");
    }
    let parts: Vec<&str> = path.split(['/', '\\']).collect();
    if parts.contains(&"..") {
        return refuse("a path that steps up with `..` may leave the workspace");
    }
    if parts.iter().any(|part| part.starts_with('.') && *part != ".") {
        return refuse("hidden files are not lent");
    }
    None
}

/// A file a hosted session reads: through the host, by its path in the workspace. Elsewhere the
/// file at `path`.
pub fn read_lent(path: &str) -> Result<Vec<u8>, String> {
    match host() {
        Some(host) => match not_lent(path) {
            Some(why) => Err(why),
            None => (host.read)(path),
        },
        None => std::fs::read(path).map_err(|e| format!("{path}: {e}")),
    }
}

/// The size of the file [`read_lent`] reads.
pub fn lent_len(path: &str) -> Result<u64, String> {
    match host() {
        Some(host) => match not_lent(path) {
            Some(why) => Err(why),
            None => (host.size)(path),
        },
        None => std::fs::metadata(path).map(|meta| meta.len()).map_err(|e| format!("{path}: {e}")),
    }
}

/// The file at `path`, never through the host: refused in a hosted session.
pub fn read(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    let path = path.as_ref();
    if hosted() {
        return Err(denied(path));
    }
    std::fs::read(path)
}

/// Opens the file at `path` for reading: refused in a hosted session.
pub fn open(path: impl AsRef<Path>) -> std::io::Result<std::fs::File> {
    let path = path.as_ref();
    if hosted() {
        return Err(denied(path));
    }
    std::fs::File::open(path)
}

/// Writes the file at `path`: refused in a hosted session.
pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    let path = path.as_ref();
    if hosted() {
        return Err(denied(path));
    }
    std::fs::write(path, bytes)
}

/// Renames the file at `from` to `to`: refused in a hosted session.
pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    if hosted() {
        return Err(denied(to));
    }
    std::fs::rename(from, to)
}

/// The file status at `path`: refused in a hosted session.
pub fn metadata(path: impl AsRef<Path>) -> std::io::Result<std::fs::Metadata> {
    let path = path.as_ref();
    if hosted() {
        return Err(denied(path));
    }
    std::fs::metadata(path)
}

/// The canonical form of `path` (links and `..` resolved): refused in a hosted session.
pub fn canonicalize(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    let path = path.as_ref();
    if hosted() {
        return Err(denied(path));
    }
    std::fs::canonicalize(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lend(_: &str) -> Result<Vec<u8>, String> {
        Ok(b"lent".to_vec())
    }

    fn size(_: &str) -> Result<u64, String> {
        Ok(4)
    }

    const HOST: HostFiles = HostFiles { size, read: lend };

    #[test]
    fn a_hosted_scope_lends_plain_workspace_paths_only() {
        let _scope = enter(Some(HOST));
        assert_eq!(read_lent("pictures/a.png").unwrap(), b"lent");
        assert_eq!(read_lent("./a.png").unwrap(), b"lent");
        assert_eq!(read_lent("fh_123").unwrap(), b"lent");
        assert_eq!(lent_len("a.csv").unwrap(), 4);
        for path in [
            "/etc/passwd",
            "\\\\server\\share\\a.png",
            "file:///etc/passwd",
            "file:a.png",
            "http://127.0.0.1:9/a.png",
            "C:\\a.png",
            "../a.png",
            "sub/../../a.png",
            ".hidden.png",
            "a/.git/config",
            "",
            "a\0b",
        ] {
            assert!(read_lent(path).is_err(), "{path:?}");
            assert!(lent_len(path).is_err(), "{path:?}");
        }
        assert!(read("Cargo.toml").is_err() && open("Cargo.toml").is_err() && write("x", b"x").is_err());
        assert!(rename("Cargo.toml", "x").is_err() && metadata(".").is_err() && canonicalize(".").is_err());
        assert!(need("review.readAloud").is_err());
    }

    #[test]
    fn a_scope_stays_hosted_until_it_ends() {
        {
            let _hosted = enter(Some(HOST));
            let _inner = enter(None);
            assert!(hosted(), "a command run inside a hosted one reaches no more");
        }
        assert!(!hosted());
        assert!(need("review.readAloud").is_ok());
        assert!(metadata(".").is_ok());
    }
}
