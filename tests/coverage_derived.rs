use std::{fs, path::Path, process::Command};

#[test]
fn derived_coverage_fixtures() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coverage-derived");
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
                        "{} samples={} combinations={} total={:?} goal={} weight={} grade={:?}\n",
                        item.name,
                        item.samples,
                        item.combos.len(),
                        item.total_combos,
                        item.goal,
                        item.weight,
                        item.grade()
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
fn hidden_derived_items_keep_library_sampling_data() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coverage-derived/all-hidden.e");
    let parsed = e1647::parser::parse(&fs::read_to_string(path).unwrap());
    assert!(parsed.diagnostics.is_empty());
    let elaborated = e1647::elab::elaborate(&parsed.declarations);
    assert!(elaborated.diagnostics.is_empty());
    let mut data = e1647::coverage::build(&elaborated);
    let group = &mut data
        .iter_mut()
        .find(|entry| entry.type_name == "sys")
        .unwrap()
        .groups[0];
    for _ in 0..2 {
        group.begin_sample();
        group.sample("n", &e1647::interp::Value::Bool(false));
        group.sample("flag", &e1647::interp::Value::Bool(false));
        group.record_crosses();
    }
    assert_eq!(group.derived[0].samples, 2);
    assert_eq!(group.derived[1].samples, 1);
    assert_eq!(group.derived[0].grade(), Some(0.25));
    assert_eq!(group.derived[1].grade(), Some(0.25));
    assert_eq!(group.grade(), None);
}

#[test]
fn cli_reports_derived_goals_and_weights() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/coverage-derived/when-and-goals.e");
    let output = Command::new(env!("CARGO_BIN_EXE_e1647"))
        .args(["run", "--coverage", "--seed", "1"])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(
        report.contains("3 samples, 2 combinations, goal=2, weight=1, grade 25%"),
        "{report}"
    );
    assert!(!report.contains("item n:"), "{report}");
}
