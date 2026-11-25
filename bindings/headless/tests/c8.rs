//! C8-land-14-doc: the kit as the agent tools serve it (kition-kits), with a separate writer
//! that really opens and rewrites the file between the agent's observation and its write.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use kition_doc_kit::DocDocument;
use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions, Guard};
use kition_kits::{KitDefinition, KitManager};
use serde_json::{Value, json};

#[path = "c8/holds.rs"]
mod holds;
#[path = "c8/ids.rs"]
mod ids;
#[path = "c8/independent.rs"]
mod independent;
#[path = "c8/matrix.rs"]
mod matrix;
#[path = "c8/outside.rs"]
mod outside;
#[path = "c8/saved.rs"]
mod saved;
#[path = "c8/sdk_adoption.rs"]
mod sdk_adoption;
#[path = "c8/sdk_precision.rs"]
mod sdk_precision;
#[path = "c8/split.rs"]
mod split;
#[path = "c8/tables.rs"]
mod tables;

const FILE: &str = "Outside.docx";
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// The body texts every fixture starts with.
pub const TEXTS: [&str; 5] = ["Alpha one two", "Beta three four", "Gamma five six", "Delta seven eight", "Epsilon nine ten"];

pub struct Fixture {
    pub kits: KitManager,
    pub root: PathBuf,
    pub options: CallOptions,
}

/// A document of [`TEXTS`], one paragraph each, as the kit makes and saves it.
pub fn document() -> DocDocument {
    let mut doc = DocDocument::create(FILE, 612, 792).unwrap();
    for (n, text) in TEXTS.iter().enumerate() {
        if n > 0 {
            doc.execute("text.newParagraph", json!({})).unwrap();
        }
        doc.execute("text.insert", json!({ "text": text })).unwrap();
    }
    doc
}

impl Fixture {
    pub fn new(guard: Guard) -> Self {
        Self::with(guard, &document().save(None).unwrap())
    }

    pub fn with(guard: Guard, bytes: &[u8]) -> Self {
        let root = std::env::temp_dir().join(format!("kition-doc-c8-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(root.join(FILE), bytes).unwrap();
        let kit = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(root.clone()), vec![(kit, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-doc"))))]);
        Self { kits, root, options: CallOptions { guard, ..Default::default() } }
    }

    /// The file as another program reads it.
    pub fn disk(&self) -> DocDocument {
        DocDocument::open(FILE, &std::fs::read(self.root.join(FILE)).unwrap()).unwrap()
    }

    /// The body texts of the file.
    pub fn texts(&self) -> Vec<String> {
        body_texts(&mut self.disk())
    }

    /// Another program opens the file, runs `steps` and saves it over the file.
    pub fn colleague(&self, steps: &[(&str, Value)]) {
        let mut doc = self.disk();
        for (command, params) in steps {
            doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("colleague {command}: {error}"));
        }
        self.rewrite(&doc.save(None).unwrap());
    }

    /// Writes `bytes` over the file, as another program's save.
    pub fn rewrite(&self, bytes: &[u8]) {
        let path = self.root.join(FILE);
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::fs::write(&path, bytes).unwrap();
        std::fs::File::options().write(true).open(path).unwrap().set_modified(modified - std::time::Duration::from_secs(3600)).unwrap();
    }

    pub async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!(FILE);
        match tools::call_as(&self.kits, &format!("doc_{action}"), &input, self.options, "c8-agent").await {
            Some(Ok(answer)) => Ok(answer.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("missing doc_{action}"),
        }
    }

    pub async fn basis(&self) -> Value {
        self.call("inspect", json!({})).await.unwrap()["revision"].clone()
    }

    /// The body's blocks as `doc_query` answers them: `(id, kind)`.
    pub async fn blocks(&self) -> Vec<(Value, Value)> {
        let answer = self.call("query", json!({ "follow": "body", "fields": ["id", "kind"] })).await.unwrap();
        answer["objects"].as_array().unwrap().iter().map(|o| (o["id"].clone(), o["fields"]["kind"].clone())).collect()
    }

    pub async fn run(&self, basis: Option<Value>, steps: Value) -> Result<Value, Value> {
        let mut input = json!({ "steps": steps });
        if let Some(basis) = basis {
            input["basis"] = basis;
        }
        self.call("run", input).await
    }

    pub async fn close(self) {
        self.kits.shutdown().await;
        std::fs::remove_dir_all(self.root).unwrap();
    }
}

/// The body paragraphs' texts of `doc`.
pub fn body_texts(doc: &mut DocDocument) -> Vec<String> {
    let text = doc.execute("file.text", json!({})).unwrap();
    text["text"].as_str().unwrap().split('\n').map(str::to_owned).collect()
}

pub fn conflict(error: &Value, reason: &str) {
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert!(error["conflicts"].as_array().unwrap().iter().any(|conflict| conflict["reason"] == reason), "{error}");
}
