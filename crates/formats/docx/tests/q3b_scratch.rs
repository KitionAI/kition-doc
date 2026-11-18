//! Scratch (untracked): round-trips the corpus into $HOME/.cache/q3b/out.
#[test]
fn dump() {
    let Ok(dir) = std::env::var("KITION_WORD_CORPUS") else { return };
    let out = format!("{}/.cache/q3b/out", std::env::var("HOME").unwrap());
    std::fs::create_dir_all(&out).unwrap();
    let only = std::env::var("Q3B_ONLY").ok();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".docx") {
            continue;
        }
        if let Some(o) = &only
            && !name.contains(o.as_str())
        {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        let doc = kition_doc_docx::read(&bytes).unwrap();
        let w = kition_doc_docx::write(&doc).unwrap();
        std::fs::write(format!("{out}/{name}"), w).unwrap();
    }
}

#[test]
fn parts() {
    let Ok(dir) = std::env::var("KITION_WORD_CORPUS") else { return };
    let Ok(only) = std::env::var("Q3B_ONLY") else { return };
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if !name.contains(only.as_str()) {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        let a = kition_doc_docx::read(&bytes).unwrap();
        let b = kition_doc_docx::read(&kition_doc_docx::write(&a).unwrap()).unwrap();
        for (label, d) in [("first", &a), ("second", &b)] {
            let kinds: Vec<String> = d
                .parts
                .iter()
                .map(|(id, p)| {
                    format!(
                        "{id}:{:?}:{}",
                        p.kind,
                        p.blocks
                            .iter()
                            .filter_map(|b| b.as_para())
                            .map(|p| p.text.clone())
                            .collect::<Vec<_>>()
                            .join("|")
                            .chars()
                            .take(30)
                            .collect::<String>()
                    )
                })
                .collect();
            eprintln!("{label}: {kinds:#?}");
        }
    }
}

#[test]
fn models() {
    let Ok(dir) = std::env::var("KITION_WORD_CORPUS") else { return };
    let Ok(only) = std::env::var("Q3B_ONLY") else { return };
    let out = format!("{}/.cache/q3b/models", std::env::var("HOME").unwrap());
    std::fs::create_dir_all(&out).unwrap();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if !name.contains(only.as_str()) {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        let a = kition_doc_docx::read(&bytes).unwrap();
        let b = kition_doc_docx::read(&kition_doc_docx::write(&a).unwrap()).unwrap();
        std::fs::write(format!("{out}/a.json"), serde_json::to_string_pretty(&a).unwrap()).unwrap();
        std::fs::write(format!("{out}/b.json"), serde_json::to_string_pretty(&b).unwrap()).unwrap();
    }
}
