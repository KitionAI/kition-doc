//! Every command says what it does and every read command an agent is offered says what it answers
//! (kition_kit::testing::check_commands, KHP A1, W1-46 P2-1).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::Path;

use kition_doc_kit::DocDocument;
use kition_kit::KitDocument;

fn basic() -> DocDocument {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.docx"));
    DocDocument::open("basic.docx", &std::fs::read(path).unwrap()).unwrap()
}

/// Runs a read command on `basic.docx` with its sample parameters (`command_samples.json`, `{}` when
/// it has none), after the sample's setup steps.
fn sample(id: &str) -> Result<serde_json::Value, String> {
    let samples: serde_json::Value = serde_json::from_str(include_str!("command_samples.json")).unwrap();
    let tmp = std::env::temp_dir().join(format!("kition-doc-samples-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let fill = |value: &serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&value.to_string().replace("$tmp", &tmp.to_string_lossy())).unwrap()
    };
    let mut doc = basic();
    let entry = &samples[id];
    if let Some(reason) = entry["skip"].as_str() {
        return Err(format!("skip: {reason}"));
    }
    for step in entry["setup"].as_array().into_iter().flatten() {
        doc.execute(step[0].as_str().unwrap(), fill(&step[1]))?;
    }
    doc.execute(id, entry.get("params").map_or(serde_json::json!({}), fill))
}

/// What each read command answers matches the output schema it declares (KHP A1).
#[test]
fn every_read_command_answers_what_its_output_schema_says() {
    let mut failures = Vec::new();
    for command in basic().commands().into_iter().filter(|c| c.schema.is_some() && !c.mutates) {
        let Some(schema) = command.output_schema else { continue };
        if let Err(error) = kition_kit::schema::validate_output(&schema) {
            failures.push(format!("{}: {error}", command.id));
            continue;
        }
        match sample(&command.id) {
            Ok(value) => {
                let wrapped = serde_json::json!({ "answer": schema });
                let params = serde_json::Map::from_iter([("answer".to_owned(), value)]);
                if let Err(error) = kition_kit::schema::check_params(&wrapped, &params) {
                    failures.push(format!("{}: {error}", command.id));
                }
            }
            Err(error) if error.starts_with("skip: ") => {}
            Err(error) => failures.push(format!("{}: the sample call failed: {error}", command.id)),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn every_command_is_described_and_every_read_command_declares_its_output() {
    if let Err(problems) = kition_kit::testing::check_commands(&basic().commands(), kition_doc_kit::NO_OUTPUT) {
        panic!("{} problems:\n{}", problems.len(), problems.join("\n"));
    }
}

/// `KITION_DUMP_COMMANDS=<file>`: writes the command list and what each read command answers on
/// `basic.docx` with no parameters, for writing descriptions and output schemas.
#[test]
fn dump_commands() {
    let Ok(out) = std::env::var("KITION_DUMP_COMMANDS") else { return };
    let doc = basic();
    let mut list = Vec::new();
    for command in doc.commands() {
        let mut entry = serde_json::to_value(&command).unwrap();
        if command.schema.is_some() && !command.mutates {
            entry["sample"] = match sample(&command.id) {
                Ok(value) => value,
                Err(error) => serde_json::json!({ "error": error }),
            };
        }
        list.push(entry);
    }
    std::fs::write(out, serde_json::to_string_pretty(&list).unwrap()).unwrap();
}

/// At least 95% of the parameter keys agents are offered say their type (kit requirement AX14): an
/// untyped key leaves an agent guessing between a number, a string and a list. Every schema is a
/// well-formed `ParameterSchemaSpec`, so the headless kit's parameter check reads it as written.
#[test]
fn parameter_keys_say_their_type() {
    // `"type": "json"` is the notation's "any value": an agent sees no type.
    let typed = |node: &serde_json::Value| {
        node.get("type").is_some_and(|kind| kind != "json")
            || ["oneOf", "anyOf", "allOf", "enum", "const", "$ref"].iter().any(|key| node.get(key).is_some())
    };
    let mut total = 0usize;
    let mut untyped = Vec::new();
    let mut malformed = Vec::new();
    for command in basic().commands() {
        let Some(whole @ serde_json::Value::Object(schema)) = &command.schema else { continue };
        if let Err(error) = kition_kit::schema::validate(whole) {
            malformed.push(format!("{}: {error}", command.id));
        }
        for (name, node) in schema {
            total += 1;
            if !typed(node) {
                untyped.push(format!("{}.{name}", command.id));
            }
        }
    }
    assert!(malformed.is_empty(), "{malformed:#?}");
    let share = (total - untyped.len()) as f64 / total as f64;
    assert!(share >= 0.95, "{:.1}% of {total} parameter keys typed; untyped: {untyped:#?}", share * 100.0);
}
