use std::{fs, path::Path, process::Command};

#[test]
fn integrated_execution_fixtures_pass_typing_and_native_cli() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/execution-integration");
    for name in [
        "qualified-when-fields",
        "live-when-types",
        "live-when-methods",
        "dynamic-when-generation-events",
        "dynamic-when-joint-rgf",
        "nested-edges",
        "entry-alias",
        "coverage-owners",
        "root-events",
    ] {
        let path = directory.join(format!("{name}.e"));
        let source = fs::read_to_string(&path).unwrap();
        let parsed = e1647::parser::parse(&source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{name}: {:?}",
            parsed.diagnostics
        );
        let elaborated = e1647::elab::elaborate(&parsed.declarations);
        let mut diagnostics = elaborated.diagnostics.clone();
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
            "{name}"
        );
        let result = Command::new(env!("CARGO_BIN_EXE_e1647"))
            .arg("run")
            .arg(&path)
            .output()
            .expect("CLI starts");
        assert!(
            result.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            fs::read_to_string(path.with_extension("out")).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn dynamic_subtype_port_drives_coverage_and_rule_lifetimes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/execution-integration/dynamic-when-registrations.e");
    let parsed = e1647::parser::parse(&fs::read_to_string(path).unwrap());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let elaborated = e1647::elab::elaborate(&parsed.declarations);
    assert!(
        elaborated.diagnostics.is_empty(),
        "{:?}",
        elaborated.diagnostics
    );
    let typing = e1647::typing::check_types(&elaborated);
    assert!(typing.is_empty(), "{typing:?}");
    let mut signals = e1647::hdl::InjectedSignals::new();
    signals
        .at("top.clock", 0, 0)
        .at("top.clock", 2, 1)
        .at("top.clock", 3, 0)
        .at("top.clock", 4, 1)
        .at("top.clock", 6, 0)
        .at("top.clock", 7, 1);
    let result = e1647::interp::run_with_signals(&elaborated, 1, signals);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.output, "edge 2\nrule 2\nedge 2\nrule 2\n");
    let group = |ty: &str, group: &str| {
        result
            .coverage
            .iter()
            .find(|entry| entry.type_name == ty)
            .unwrap()
            .groups
            .iter()
            .find(|entry| entry.name == group)
            .unwrap()
    };
    assert_eq!(group("probe_u", "base_tick").samples, 2);
    let covered = group("active probe_u", "edge$");
    assert_eq!(covered.unit_instance.as_deref(), Some("sys.probe"));
    assert_eq!(covered.samples, 2);
    assert_eq!(
        covered.items[0]
            .buckets
            .iter()
            .find(|bucket| bucket.lo == 2)
            .unwrap()
            .hits,
        2
    );
}

#[test]
fn subtype_hdl_trigger_restarts_its_edge_baseline() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/execution-integration/dynamic-when-hdl.e");
    let parsed = e1647::parser::parse(&fs::read_to_string(&path).unwrap());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let elaborated = e1647::elab::elaborate(&parsed.declarations);
    assert!(
        elaborated.diagnostics.is_empty(),
        "{:?}",
        elaborated.diagnostics
    );
    let mut signals = e1647::hdl::InjectedSignals::new();
    signals
        .at("top.clock", 0, 0)
        .at("top.clock", 1, 1)
        .at("top.clock", 2, 0)
        .at("top.clock", 3, 1)
        .at("top.clock", 5, 0)
        .at("top.clock", 6, 1);
    let result = e1647::interp::run_with_signals(&elaborated, 1, signals);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.output,
        fs::read_to_string(path.with_extension("out")).unwrap()
    );
}

#[test]
fn failed_subtype_generation_preserves_the_previous_value() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/execution-integration/dynamic-when-rollback.e");
    let parsed = e1647::parser::parse(&fs::read_to_string(path).unwrap());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let elaborated = e1647::elab::elaborate(&parsed.declarations);
    let result = e1647::interp::run(&elaborated, 1);
    assert_eq!(result.output, "13\n");
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "E0407");
    assert!(result.diagnostics[0].message.contains("alpha packet_s"));
    assert_eq!(result.diagnostics[1].code, "E0413");
    assert!(result.diagnostics[1].message.contains("alpha_only"));
}

#[test]
fn cli_reproduces_complete_generated_objects_for_the_same_seed() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/execution-integration/seeded-objects.e");
    let run = |seed: u64| {
        let result = Command::new(env!("CARGO_BIN_EXE_e1647"))
            .arg("run")
            .arg(&path)
            .args(["--seed", &seed.to_string()])
            .output()
            .expect("CLI starts");
        assert!(result.status.success(), "seed {seed}: {result:?}");
        assert!(result.stderr.is_empty(), "seed {seed}: {result:?}");
        String::from_utf8(result.stdout).unwrap()
    };

    use std::collections::BTreeSet;
    let mut snapshots = BTreeSet::new();
    for seed in [0, 1, 42, u64::MAX] {
        let output = run(seed);
        assert_eq!(output, run(seed), "seed {seed}");
        let objects: Vec<_> = output.lines().collect();
        assert_eq!(objects.len(), 3, "seed {seed}: {output}");
        assert_eq!(
            objects.iter().copied().collect::<BTreeSet<_>>().len(),
            3,
            "seed {seed}: {output}"
        );
        for object in objects {
            assert!(
                object.starts_with("seeded_sample_s{serial_number: "),
                "{object}"
            );
            assert!(object.contains("nested: seeded_nested_s{"), "{object}");
            let (_, digits) = object
                .split_once("digits: [")
                .expect("generated scalar list");
            let (digits, _) = digits.split_once(']').expect("complete scalar list");
            assert_eq!(digits.split(", ").count(), 3, "{object}");
            let (_, batch) = object
                .split_once("batch: [")
                .expect("generated struct list");
            assert_eq!(batch.matches("seeded_leaf_s{").count(), 2, "{object}");
            let codes: Vec<_> = object.split("seeded_leaf_s{code: ").skip(1).collect();
            assert_eq!(codes.len(), 3, "{object}");
            for leaf in codes {
                let (code, _) = leaf.split_once('}').expect("complete leaf struct");
                assert!((12..=19).contains(&code.parse::<u8>().unwrap()), "{object}");
            }
        }
        snapshots.insert(output);
    }
    assert_eq!(
        snapshots.len(),
        4,
        "each tested seed should vary the generated data"
    );
}

#[test]
fn suspended_helpers_preserve_coverage_unit_owners() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/execution-integration/coverage-owners.e");
    let parsed = e1647::parser::parse(&fs::read_to_string(path).unwrap());
    assert!(parsed.diagnostics.is_empty());
    let elaborated = e1647::elab::elaborate(&parsed.declarations);
    assert!(elaborated.diagnostics.is_empty());
    let result = e1647::interp::run(&elaborated, 1);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let mut owners = Vec::new();
    for coverage in &result.coverage {
        for group in &coverage.groups {
            if coverage.type_name != "channel_u" {
                continue;
            }
            owners.push(group.unit_instance.clone().expect("per-unit owner"));
            assert_eq!(group.samples, 2);
            assert_eq!(group.grade(), Some(1.0));
            assert_eq!(group.items.len(), 1);
            let hits: Vec<_> = group.items[0]
                .buckets
                .iter()
                .map(|bucket| bucket.hits)
                .collect();
            assert_eq!(hits, [1, 1]);
        }
    }
    owners.sort();
    assert_eq!(owners, ["sys.a", "sys.b"]);
}

#[test]
fn root_event_aliases_sample_coverage_once_per_emit() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/execution-integration/root-events.e");
    let parsed = e1647::parser::parse(&fs::read_to_string(path).unwrap());
    assert!(parsed.diagnostics.is_empty());
    let elaborated = e1647::elab::elaborate(&parsed.declarations);
    assert!(elaborated.diagnostics.is_empty());
    let result = e1647::interp::run(&elaborated, 1);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let groups: Vec<_> = result
        .coverage
        .iter()
        .filter(|entry| entry.type_name == "sys")
        .flat_map(|entry| &entry.groups)
        .collect();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].samples, 4);
    let hits: Vec<_> = groups[0].items[0]
        .buckets
        .iter()
        .map(|bucket| bucket.hits)
        .collect();
    assert_eq!(hits, [4, 0]);
}
