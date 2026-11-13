use std::{fs, path::Path, process::Command};

#[test]
fn coverage_effect_fixtures() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coverage-effects");
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
                let suffix = group
                    .unit_instance
                    .as_ref()
                    .map_or(String::new(), |path| format!("@{path}"));
                report.push_str(&format!(
                    "{}.{}{suffix} samples={} grade={}\n",
                    entry.type_name, group.name, group.samples, grade
                ));
                for item in &group.items {
                    let grade = item
                        .grade()
                        .map_or("none".to_owned(), |grade| format!("{grade:.3}"));
                    report.push_str(&format!(
                        "{} buckets={} grade={} samples={}\n",
                        item.name,
                        item.buckets.len(),
                        grade,
                        item.samples
                    ));
                    for (index, bucket) in item.buckets.iter().enumerate() {
                        if item.buckets.len() > 8 && index >= 2 && index < item.buckets.len() - 2 {
                            continue;
                        }
                        report.push_str(&format!(
                            "{} [{}..{}] goal={} hits={}\n",
                            bucket.label, bucket.lo, bucket.hi, bucket.goal, bucket.hits
                        ));
                    }
                }
                for item in &group.derived {
                    report.push_str(&format!(
                        "{} samples={} combinations={} total={:?}\n",
                        item.name,
                        item.samples,
                        item.combos.len(),
                        item.total_combos
                    ));
                }
                for item in &group.derived {
                    for (label, hits) in &item.combos {
                        report.push_str(&format!("{label} hits={hits}\n"));
                    }
                }
                for instance in group.instances() {
                    report.push_str(&format!(
                        "instance {} goal={} hole={}\n",
                        instance.name,
                        instance.goal,
                        instance.is_hole()
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
fn cli_excludes_ignored_holes_and_reports_illegal_errors() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coverage-effects");
    let output = Command::new(env!("CARGO_BIN_EXE_e1647"))
        .args(["run", "--coverage", "--seed", "1"])
        .arg(directory.join("unsampled.e"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(
        report.contains("item n: 0/1 buckets, 0 samples"),
        "{report}"
    );
    assert!(report.contains("3 \"3\": 0/1 hits"), "{report}");
    for excluded in ["0 \"0\":", "1 \"1\":", "2 \"2\":"] {
        assert!(!report.contains(excluded), "{report}");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_e1647"))
        .args(["run", "--coverage", "--seed", "1"])
        .arg(directory.join("illegal-immediate.e"))
        .output()
        .unwrap();
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("1 DUT error(s)"), "{report}");
    assert!(
        report.find("dut_error:").unwrap() < report.find("after").unwrap(),
        "{report}"
    );
}
