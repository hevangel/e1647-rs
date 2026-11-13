use std::{fs, path::Path, process::Command};

#[test]
fn runtime_configuration_fixtures() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coverage-config");
    let mut paths: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "e"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty());
    for path in paths {
        let parsed = e1647::parser::parse(&fs::read_to_string(&path).unwrap());
        assert!(
            parsed.diagnostics.is_empty(),
            "{path:?}: {:?}",
            parsed.diagnostics
        );
        let elaborated = e1647::elab::elaborate(&parsed.declarations);
        assert!(
            elaborated.diagnostics.is_empty(),
            "{path:?}: {:?}",
            elaborated.diagnostics
        );
        let result = e1647::interp::run(&elaborated, 1);
        let diagnostics = if result.diagnostics.is_empty() {
            "OK\n".to_owned()
        } else {
            result
                .diagnostics
                .iter()
                .map(|diagnostic| format!("{}\n", diagnostic.render()))
                .collect()
        };
        assert_eq!(
            diagnostics,
            fs::read_to_string(path.with_extension("expected")).unwrap(),
            "{path:?}"
        );
        assert_eq!(
            result.output,
            fs::read_to_string(path.with_extension("out")).unwrap(),
            "{path:?}"
        );
        let mut coverage = String::new();
        for entry in &result.coverage {
            for group in &entry.groups {
                coverage.push_str(&format!(
                    "{}.{} samples={}\n",
                    entry.type_name, group.name, group.samples
                ));
                for item in &group.items {
                    let hits: Vec<_> = item
                        .buckets
                        .iter()
                        .map(|bucket| bucket.hits.to_string())
                        .collect();
                    coverage.push_str(&format!(
                        "{} hits=[{}] ungradeable={}\n",
                        item.name,
                        hits.join(","),
                        item.ungradeable_hits
                    ));
                }
            }
        }
        assert_eq!(
            coverage,
            fs::read_to_string(path.with_extension("coverage")).unwrap(),
            "{path:?}"
        );
    }
}

#[test]
fn cli_requires_explicit_coverage_enablement() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coverage-config");
    let disabled = Command::new(env!("CARGO_BIN_EXE_e1647"))
        .args(["run", "--coverage", "--seed", "1"])
        .arg(fixtures.join("default-off.e"))
        .output()
        .unwrap();
    assert!(disabled.status.success(), "{:?}", disabled);
    assert!(!String::from_utf8_lossy(&disabled.stdout).contains("coverage for sys:"));
    let enabled = Command::new(env!("CARGO_BIN_EXE_e1647"))
        .args(["run", "--coverage", "--seed", "1"])
        .arg(fixtures.join("pause.e"))
        .output()
        .unwrap();
    assert!(enabled.status.success(), "{:?}", enabled);
    assert!(String::from_utf8_lossy(&enabled.stdout).contains("coverage for sys:"));
}
