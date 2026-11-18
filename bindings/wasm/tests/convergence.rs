//! Kition Doc against the Kition platform convergence suite (L6), and its native and
//! WebAssembly builds against one change log.

mod common;

use std::path::Path;

use common::{DocKit, NAME};
use kition_convergence::wasm::{self, WasmEngine};
use kition_convergence::{Options, Outcome, run};

#[test]
fn converges() {
    let report = run(&DocKit, &Options::from_env());
    println!("{}", report.render());
    report.save_if_requested().unwrap();
    for check in &report.checks {
        match &check.outcome {
            Outcome::Failed(reason) => panic!("{} failed: {reason}\n{}", check.id, report.render()),
            Outcome::Skipped(reason) if check.required => panic!("{} is required for L6 but skipped: {reason}", check.id),
            _ => {}
        }
    }
}

/// Runs with `KITION_WASM_ENGINE=<folder with kition_doc_kit.js and its .wasm>`.
#[test]
fn wasm_parity() {
    let Some(engine) = WasmEngine::from_env("kition_doc_kit", "DocDocument", NAME) else {
        println!("SKIP wasm parity: set KITION_WASM_ENGINE to a built engine folder");
        return;
    };
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("word-convergence");
    for seed in [1, 2] {
        wasm::check_parity(&DocKit, &engine, &scratch, seed).unwrap_or_else(|error| panic!("seed {seed}: {error}"));
    }
}
