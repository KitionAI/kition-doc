// Local scratch (never committed): dumps lost and added leaves per changed class for files whose
// name contains KITION_W4_MATCH.
#![allow(dead_code, unused)]
include!("w4/fid.rs");

#[test]
fn w4_dump() {
    let Ok(dir) = std::env::var(CORPUS_VAR) else { return };
    let pat = std::env::var("KITION_W4_MATCH").unwrap_or_default();
    let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
    names.sort();
    for path in names {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.contains(&pat) || !EXTENSIONS.iter().any(|x| name.ends_with(x)) {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let first = import(&bytes).unwrap();
        let written = export(&first).unwrap();
        if let Ok(out) = std::env::var("KITION_W4_WRITE") {
            std::fs::write(format!("{out}/{}", name.replace(' ', "_")), &written).unwrap();
        }
        let second = import(&written).unwrap();
        let (a, b) = (leaves(&model_value(&first)), leaves(&model_value(&second)));
        println!("=== {name}");
        let empty = BTreeMap::new();
        for (class, left) in &a {
            let right = b.get(class).unwrap_or(&empty);
            for (leaf, n) in left {
                let m = right.get(leaf).copied().unwrap_or(0);
                if m < *n {
                    println!("  - [{class}] {}x {}", n - m, clip(leaf));
                }
            }
            for (leaf, m) in right {
                let n = left.get(leaf).copied().unwrap_or(0);
                if *m > n {
                    println!("  + [{class}] {}x {}", m - n, clip(leaf));
                }
            }
        }
    }
}

#[test]
fn w4_objects() {
    let Ok(dir) = std::env::var(CORPUS_VAR) else { return };
    let pat = std::env::var("KITION_W4_MATCH").unwrap_or_default();
    let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
    names.sort();
    for path in names {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.contains(&pat) || !EXTENSIONS.iter().any(|x| name.ends_with(x)) {
            continue;
        }
        let first = import(&std::fs::read(&path).unwrap()).unwrap();
        println!("=== {name}");
        for b in &first.body {
            if let Some(p) = b.as_para() {
                for o in &p.objects {
                    if let kition_doc_doc::InlineObject::Shape { kind, fill, stroke, stroke_width, vml, .. } = o {
                        println!("  shape {kind:?} fill={fill:?} stroke={stroke:?} w={stroke_width} vml={}", vml.is_some());
                    }
                }
            }
        }
    }
}

#[test]
fn w4_manifest() {
    let Ok(dir) = std::env::var(CORPUS_VAR) else { return };
    let pat = std::env::var("KITION_W4_MATCH").unwrap_or_default();
    let out = std::env::var("KITION_W4_OUT").unwrap_or_default();
    let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
    names.sort();
    for path in names {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.contains(&pat) || !EXTENSIONS.iter().any(|x| name.ends_with(x)) {
            continue;
        }
        let first = import(&std::fs::read(&path).unwrap()).unwrap();
        let second = import(&export(&first).unwrap()).unwrap();
        for (i, d) in [&first, &second].iter().enumerate() {
            for (k, v) in &d.passthrough {
                if k.contains("embedded.parts") {
                    std::fs::write(format!("{out}/m{i}.txt"), &**v).unwrap();
                }
            }
            for (k, v) in &d.media {
                println!("media{i} {k} {}", digest(v));
            }
        }
    }
}
