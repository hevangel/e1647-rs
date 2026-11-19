use std::{fs, path::Path};

#[test]
fn domain_proof_fixtures() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/domain-proofs");
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
fn interval_masks_match_exhaustive_small_domain_sampling() {
    type Predicate = fn(i128, i128) -> bool;
    let cases: [(&str, Predicate); 14] = [
        ("n < m", |n, m| n < m),
        ("n <= m", |n, m| n <= m),
        ("n == m", |n, m| n == m),
        ("n != m", |n, m| n != m),
        ("n > m", |n, m| n > m),
        ("n >= m", |n, m| n >= m),
        ("n + m <= 0", |n, m| n + m <= 0),
        ("n - m > 0", |n, m| n - m > 0),
        ("n * m >= 0", |n, m| n * m >= 0),
        ("m == 0 or n / m > 1", |n, m| m == 0 || n / m > 1),
        ("(n < 0) => (m >= 0)", |n, m| n >= 0 || m >= 0),
        ("(n == 0 ? TRUE : m / n >= 0)", |n, m| n == 0 || m / n >= 0),
        ("n in [m..m+1]", |n, m| n >= m && n <= m + 1),
        ("n in [0, n, 1 / 0]", |_, _| true),
    ];
    for (expression, ignored) in cases {
        let source = format!(
            r#"extend sys {{
            event cg;
            !n: int = 0; !m: int = 0;
            cover cg is {{
                item n: int using no_collect, ranges = {{ range([-2..-1], "neg"); range([0..2], "nonneg"); }};
                item m: int using no_collect, ranges = {{ range([-2..-1], "neg"); range([0..2], "nonneg"); }};
                cross n, m using ignore = {expression};
            }};
            setup() is also {{ set_config(cover, mode, normal); }};
            run() is also {{ for i from -2 to 2 {{ for j from -2 to 2 {{ n = i; m = j; emit cg; }}; }}; }};
        }};"#
        );
        let parsed = e1647::parser::parse(&source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{expression}: {:?}",
            parsed.diagnostics
        );
        let elaborated = e1647::elab::elaborate(&parsed.declarations);
        assert!(
            elaborated.diagnostics.is_empty(),
            "{expression}: {:?}",
            elaborated.diagnostics
        );
        let result = e1647::interp::run(&elaborated, 1);
        assert!(
            result.diagnostics.is_empty(),
            "{expression}: {:?}",
            result.diagnostics
        );
        let mut expected = std::collections::BTreeMap::<String, u64>::new();
        for n in -2..=2 {
            for m in -2..=2 {
                if !ignored(n, m) {
                    let left = if n < 0 { "neg" } else { "nonneg" };
                    let right = if m < 0 { "neg" } else { "nonneg" };
                    *expected.entry(format!("({left}, {right})")).or_default() += 1;
                }
            }
        }
        let item = &result
            .coverage
            .iter()
            .find(|data| data.type_name == "sys")
            .unwrap()
            .groups[0]
            .derived[0];
        let observed: std::collections::BTreeMap<_, _> = item.combos.iter().cloned().collect();
        assert_eq!(observed, expected, "{expression}");
        assert_eq!(
            item.total_combos,
            Some(expected.len() as u64),
            "{expression}"
        );
        assert_eq!(item.samples, expected.values().sum(), "{expression}");
        assert_eq!(
            item.grade(),
            if expected.is_empty() { None } else { Some(1.0) },
            "{expression}"
        );
    }
}
