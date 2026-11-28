//! Kition Doc engine: the editing session and its commands.
//!
//! **Everything is a command.** Each user-visible action is a [`CommandSpec`] with a stable id
//! (`format.bold`, `table.insert`), a label, the ribbon/menu location, a default shortcut, a
//! params description and a `run` function. Menus, ribbon, shortcuts, the command palette, the
//! CLI, the JSON control channel and the MCP server all call [`Session::run`].
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod catalog;
pub mod cmd;
pub mod disk;
pub mod io;
mod io_ext;
pub mod kdm_bridge;
pub mod math_gallery;
pub mod print;
pub mod sample;
mod session;
pub mod speech;

use std::collections::HashMap;

use serde_json::Value;

pub use io::Password;
pub use kdm_model;
pub use kdm_model::kition_kit::kdm::{Origin, Version};
pub use kdm_model::kition_kit::session::{Damage, MemoryBlobs};
pub use kition_doc_doc as doc;
pub use kition_doc_layout as layout;
pub use kition_doc_render as render;
pub use session::{ColumnBlock, EditSnapshot, FindState, MathEdit, Prefs, Selection, Session, ViewState};

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum CmdError {
    #[error("unknown command `{0}`")]
    Unknown(String),
    #[error("bad parameters: {0}")]
    Params(String),
    #[error("not available: {0}")]
    Disabled(String),
    #[error("{0}")]
    Failed(String),
}

impl From<kition_doc_doc::DocError> for CmdError {
    fn from(e: kition_doc_doc::DocError) -> Self {
        CmdError::Failed(e.to_string())
    }
}

pub type CmdResult = Result<Value, CmdError>;

/// A registered command.
#[derive(Clone)]
pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Where it lives in the UI: `Home › Font`, `Insert › Tables`, `File`…
    pub location: &'static str,
    /// Default shortcut in a portable form (`Mod+B`, `Mod+Shift+>`, `F7`). `Mod` = Ctrl, or ⌘ on macOS.
    /// Alternatives are ` / ` apart, the first one the one menus show.
    pub shortcut: &'static str,
    /// The shortcut on a Mac where it differs from [`Self::shortcut`] (empty: the same): ⌘← goes
    /// to the start of the line there, ⌥← a word left. See [`Platform`].
    pub mac_shortcut: &'static str,
    /// Parameter description for agents (`{"size": number}`).
    pub params: &'static str,
    pub mutates: bool,
    /// `None` = enabled; `Some(reason)` = disabled.
    pub enabled: fn(&Session) -> Option<&'static str>,
    /// When a mutating command's run only moves the selection (see [`CommandSpec::moves_only_when`]).
    pub moves_only: Option<fn(&Session, &Value) -> bool>,
    pub run: fn(&mut Session, &Value) -> CmdResult,
}

/// Which keyboard the shortcuts are for. The engine has no platform of its own (it runs in
/// WebAssembly too); the editor that sends the keys says which (`kition-doc-kit`'s `set_mac`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Platform {
    /// Windows and Linux: `Mod` is Ctrl.
    #[default]
    Default,
    /// macOS: `Mod` is ⌘, and the keys follow the Mac's habits ([`CommandSpec::mac_shortcut`]).
    Mac,
}

fn always(_: &Session) -> Option<&'static str> {
    None
}

impl CommandSpec {
    pub const fn new(id: &'static str, label: &'static str, location: &'static str, run: fn(&mut Session, &Value) -> CmdResult) -> Self {
        CommandSpec { id, label, location, shortcut: "", mac_shortcut: "", params: "{}", mutates: true, enabled: always, moves_only: None, run }
    }
    pub const fn key(mut self, s: &'static str) -> Self {
        self.shortcut = s;
        self
    }
    /// The shortcut on a Mac, where it differs from the default one (`key`).
    pub const fn mac(mut self, s: &'static str) -> Self {
        self.mac_shortcut = s;
        self
    }
    /// The shortcut on `platform`.
    pub fn shortcut_on(&self, platform: Platform) -> &'static str {
        match platform {
            Platform::Mac if !self.mac_shortcut.is_empty() => self.mac_shortcut,
            _ => self.shortcut,
        }
    }
    /// A run that, with these parameters in this session, only moves the caret or the selection
    /// (Tab to the next table cell) leaves the document and the history as they were: no undo step.
    pub const fn moves_only_when(mut self, f: fn(&Session, &Value) -> bool) -> Self {
        self.moves_only = Some(f);
        self
    }
    /// Whether this run, with `params` in `s`, changes nothing but the selection (see
    /// [`Self::moves_only_when`]).
    pub fn only_moves(&self, s: &Session, params: &Value) -> bool {
        self.mutates && self.moves_only.is_some_and(|f| f(s, params))
    }
    pub const fn params(mut self, p: &'static str) -> Self {
        self.params = p;
        self
    }
    /// Doesn't change the document (navigation, view, export).
    pub const fn pure(mut self) -> Self {
        self.mutates = false;
        self
    }
    pub const fn when(mut self, f: fn(&Session) -> Option<&'static str>) -> Self {
        self.enabled = f;
        self
    }
}

/// All commands by id.
pub struct Registry {
    specs: Vec<CommandSpec>,
    by_id: HashMap<&'static str, usize>,
}

impl Registry {
    pub fn new(specs: Vec<CommandSpec>) -> Self {
        let mut by_id = HashMap::new();
        for (i, s) in specs.iter().enumerate() {
            by_id.insert(s.id, i);
        }
        Registry { specs, by_id }
    }
    pub fn get(&self, id: &str) -> Option<&CommandSpec> {
        self.by_id.get(id).and_then(|i| self.specs.get(*i))
    }
    pub fn all(&self) -> &[CommandSpec] {
        &self.specs
    }
    /// Command list as JSON (for agents: `commands` / `list_commands`).
    pub fn describe(&self) -> Value {
        Value::Array(
            self.specs
                .iter()
                .map(|s| serde_json::json!({"id": s.id, "label": s.label, "location": s.location, "shortcut": s.shortcut, "params": s.params, "mutates": s.mutates}))
                .collect(),
        )
    }
    /// The command bound to a shortcut on the default keyboard, if any.
    pub fn by_shortcut(&self, key: &str) -> Option<&CommandSpec> {
        self.by_shortcut_on(key, Platform::Default)
    }
    /// The command bound to a shortcut on `platform`'s keyboard, if any.
    pub fn by_shortcut_on(&self, key: &str, platform: Platform) -> Option<&CommandSpec> {
        self.specs.iter().find(|s| {
            let keys = s.shortcut_on(platform);
            !keys.is_empty() && keys.split(" / ").any(|k| k.eq_ignore_ascii_case(key))
        })
    }
}

/// Parameter helpers.
pub mod p {
    use serde_json::Value;

    use crate::CmdError;

    pub fn str<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
        v.get(k).and_then(Value::as_str)
    }
    pub fn req_str<'a>(v: &'a Value, k: &str) -> Result<&'a str, CmdError> {
        str(v, k).ok_or_else(|| CmdError::Params(format!("`{k}` (string) is required")))
    }
    pub fn f32(v: &Value, k: &str) -> Option<f32> {
        v.get(k).and_then(Value::as_f64).map(|x| x as f32).filter(|x| x.is_finite())
    }
    pub fn req_f32(v: &Value, k: &str) -> Result<f32, CmdError> {
        f32(v, k).ok_or_else(|| CmdError::Params(format!("`{k}` (number) is required")))
    }
    pub fn u64(v: &Value, k: &str) -> Option<u64> {
        v.get(k).and_then(|x| x.as_u64().or_else(|| x.as_f64().filter(|f| *f >= 0.0 && f.is_finite()).map(|f| f as u64)))
    }
    pub fn bool(v: &Value, k: &str) -> Option<bool> {
        v.get(k).and_then(Value::as_bool)
    }
    /// The `value` param, or the first positional-ish string.
    pub fn value(v: &Value) -> Option<&Value> {
        v.get("value")
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_bidi;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests_hosted;
#[cfg(test)]
mod tests_typing;
