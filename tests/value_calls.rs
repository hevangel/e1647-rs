use std::{fs, path::Path, process::Command};

#[test]
fn value_call_fixtures_pass_typing_and_native_execution() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/value-calls");
    let mut paths: Vec<_> = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "e"))
        .collect();
    paths.sort();
    let mut failures = Vec::new();
    for path in paths {
        let parsed = e1647::parser::parse(&fs::read_to_string(&path).unwrap());
        let elaborated = e1647::elab::elaborate(&parsed.declarations);
        let mut diagnostics = parsed.diagnostics;
        diagnostics.extend(elaborated.diagnostics.clone());
        diagnostics.extend(e1647::typing::check_types(&elaborated));
        let actual = if diagnostics.is_empty() {
            "OK\n".to_owned()
        } else {
            diagnostics
                .iter()
                .map(|d| format!("{}\n", d.render()))
                .collect()
        };
        let expected = fs::read_to_string(path.with_extension("expected")).unwrap();
        if actual != expected {
            failures.push(format!("{path:?}: {actual}"));
            continue;
        }
        if path.with_extension("run.expected").exists() {
            let native = e1647::interp::run(&elaborated, 1);
            let actual = native
                .diagnostics
                .iter()
                .map(|d| format!("{}\n", d.render()))
                .collect::<String>();
            assert_eq!(
                actual,
                fs::read_to_string(path.with_extension("run.expected")).unwrap()
            );
            assert_eq!(
                native.output,
                fs::read_to_string(path.with_extension("out")).unwrap()
            );
            continue;
        }
        let result = Command::new(env!("CARGO_BIN_EXE_e1647"))
            .arg("run")
            .arg(&path)
            .output()
            .expect("CLI starts");
        let output = String::from_utf8(result.stdout).unwrap();
        if !result.status.success()
            || output != fs::read_to_string(path.with_extension("out")).unwrap()
        {
            failures.push(format!(
                "{path:?}: {output} {}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
