use std::{fs, path::Path, process::Command};

#[test]
fn coverage_instance_fixtures() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coverage-instances");
    let mut paths: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "e"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty());
    for path in paths {
        let parsed = e1647::parser::parse(&fs::read_to_string(&path).unwrap());
        let mut diagnostics = parsed.diagnostics;
        let mut output = String::new();
        let mut coverage = Vec::new();
        if diagnostics.is_empty() {
            let elaborated = e1647::elab::elaborate(&parsed.declarations);
            diagnostics = elaborated.diagnostics.clone();
            if diagnostics.is_empty() {
                let result = e1647::interp::run(&elaborated, 1);
                diagnostics = result.diagnostics;
                output = result.output;
                coverage = result.coverage;
            }
        }
        let diagnostics = if diagnostics.is_empty() {
            "OK\n".to_owned()
        } else {
            diagnostics
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
            output,
            fs::read_to_string(path.with_extension("out")).unwrap(),
            "{path:?}"
        );
        let mut report = String::new();
        for entry in coverage {
            for group in entry.groups {
                let grade = group
                    .grade()
                    .map_or("none".to_owned(), |grade| format!("{grade:.3}"));
                report.push_str(&format!(
                    "{}.{}@{} samples={} grade={}\n",
                    entry.type_name,
                    group.name,
                    group.unit_instance.as_deref().unwrap_or("type"),
                    group.samples,
                    grade
                ));
                for item in &group.items {
                    let hits: Vec<_> = item
                        .buckets
                        .iter()
                        .map(|bucket| bucket.hits.to_string())
                        .collect();
                    report.push_str(&format!(
                        "{} samples={} hits=[{}]\n",
                        item.name,
                        item.samples,
                        hits.join(",")
                    ));
                }
                for item in &group.derived {
                    report.push_str(&format!(
                        "{} samples={} combinations={}\n",
                        item.name,
                        item.samples,
                        item.combos.len()
                    ));
                }
            }
        }
        assert_eq!(
            report,
            fs::read_to_string(path.with_extension("coverage")).unwrap(),
            "{path:?}"
        );
    }
}

#[test]
fn cli_reports_separate_unit_paths_and_rejects_bad_scopes() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coverage-instances");
    let result = Command::new(env!("CARGO_BIN_EXE_e1647"))
        .args(["run", "--coverage", "--seed", "1"])
        .arg(fixtures.join("units.e"))
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);
    let stdout = String::from_utf8(result.stdout).unwrap();
    for path in ["sys.a", "sys.b", "sys.c"] {
        assert!(
            stdout.contains(&format!("unit instance {path}:")),
            "{stdout}"
        );
    }
    let invalid = Command::new(env!("CARGO_BIN_EXE_e1647"))
        .arg("elab")
        .arg(fixtures.join("struct-scope.e"))
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("E0224"));
}
