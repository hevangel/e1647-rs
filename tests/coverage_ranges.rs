use std::{fs, path::Path, process::Command};

#[test]
fn real_ranges_reject_invalid_bounds_and_widths() {
    for (range, message) in [
        ("range([0.0..1e999]);", "finite numeric constants"),
        ("range([0.0..1.0 / 0.0]);", "finite numeric constants"),
        (
            "range([0.0..1.0], \"\", 0.0);",
            "bucket width must be positive",
        ),
        ("range([1.0..0.0]);", "lower bound exceeds upper bound"),
        (
            "range([1e16..1.0000000000000002e16], \"\", 0.25);",
            "width cannot advance",
        ),
        (
            "range([0.0..1.0], \"named\", 0.25);",
            "require an empty name",
        ),
    ] {
        let source = format!(
            "extend sys {{ event cg; cover cg is {{ item n: real using ranges = {{ {range} }}; }}; }};"
        );
        let parsed = e1647::parser::parse(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let elaborated = e1647::elab::elaborate(&parsed.declarations);
        assert!(
            elaborated.diagnostics.iter().any(
                |diagnostic| diagnostic.code == "E0224" && diagnostic.message.contains(message)
            )
        );
    }
}

#[test]
fn string_aliases_cannot_declare_coverage_ranges() {
    let parsed = e1647::parser::parse(
        "type text_t: string; type alias_t: text_t; extend sys { event cg; \
         !text_value: alias_t; cover cg is { item text_value using ranges = { range([0..1]); }; }; };",
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let elaborated = e1647::elab::elaborate(&parsed.declarations);
    assert!(elaborated.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "E0224"
            && diagnostic
                .message
                .contains("not permitted for string items")
    }));
}

#[test]
fn coverage_range_fixtures() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coverage-ranges");
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
                        "{} buckets={} grade={}\n",
                        item.name,
                        item.buckets.len(),
                        grade
                    ));
                    for (index, bucket) in item.buckets.iter().enumerate() {
                        if item.buckets.len() > 8 && index >= 2 && index < item.buckets.len() - 2 {
                            continue;
                        }
                        let (lo, hi) = bucket.bounds_text();
                        let end = if bucket.upper_inclusive() { ']' } else { ')' };
                        report.push_str(&format!(
                            "{} [{lo}..{hi}{end} goal={} hits={}\n",
                            bucket.label, bucket.goal, bucket.hits
                        ));
                    }
                }
                for item in &group.derived {
                    report.push_str(&format!(
                        "{} samples={} combinations={}\n",
                        item.name,
                        item.samples,
                        item.combos.len()
                    ));
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
fn cli_reports_zero_goals_without_false_holes() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coverage-ranges/zero-goals.e");
    let result = Command::new(env!("CARGO_BIN_EXE_e1647"))
        .args(["run", "--coverage", "--seed", "1"])
        .arg(path)
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("no grade"), "{stdout}");
    assert!(stdout.contains("no goal"), "{stdout}");
    assert!(!stdout.contains("[hole]"), "{stdout}");
}
