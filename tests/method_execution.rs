use std::{fs, path::Path};

#[test]
fn composed_method_execution_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/method-execution");
    let mut paths: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "e"))
        .collect();
    paths.sort();
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        let parsed = e1647::parser::parse(&source);
        let elaborated = e1647::elab::elaborate(&parsed.declarations);
        let mut diagnostics = parsed.diagnostics;
        diagnostics.extend(elaborated.diagnostics.iter().cloned());
        diagnostics.extend(e1647::typing::check_types(&elaborated));
        let actual = if diagnostics.is_empty() {
            "OK\n".to_owned()
        } else {
            diagnostics
                .iter()
                .map(|d| format!("{}\n", d.render()))
                .collect()
        };
        assert_eq!(
            actual,
            fs::read_to_string(path.with_extension("expected")).unwrap(),
            "{}",
            path.display()
        );
        if diagnostics.is_empty() {
            let run = e1647::interp::run(&elaborated, 1);
            assert!(
                run.diagnostics.is_empty(),
                "{}: {:?}",
                path.display(),
                run.diagnostics
            );
            assert_eq!(run.dut_errors, 0, "{}: {}", path.display(), run.output);
            assert_eq!(
                run.output,
                fs::read_to_string(path.with_extension("out")).unwrap(),
                "{}",
                path.display()
            );
        }
    }
}

#[test]
fn deeply_extended_methods_use_an_iterative_plan() {
    let mut source =
        String::from("extend sys { !counter: int = 0; run() is { out(counter); }; };\n");
    for _ in 0..1200 {
        source.push_str("extend sys { run() is first { counter = counter + 1; }; };\n");
    }
    let parsed = e1647::parser::parse(&source);
    assert!(parsed.diagnostics.is_empty());
    let elaborated = e1647::elab::elaborate(&parsed.declarations);
    assert!(
        elaborated.diagnostics.is_empty(),
        "{:?}",
        elaborated.diagnostics
    );
    let run = e1647::interp::run(&elaborated, 1);
    assert!(run.diagnostics.is_empty(), "{:?}", run.diagnostics);
    assert_eq!(run.output, "1200");
}
