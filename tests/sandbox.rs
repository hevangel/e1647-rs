use e1647::sandbox::{Operation, evaluate};
use std::{fs, path::Path};

#[test]
fn browser_sources_share_native_pipeline_results() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wasm-api");
    for name in [
        "run",
        "coverage",
        "derived-unbucketed",
        "type-error",
        "dut-error",
        "import",
    ] {
        let path = root.join(format!("{name}.e"));
        let result = evaluate(
            &fs::read_to_string(&path).unwrap(),
            Operation::Run,
            u64::MAX,
        );
        let expected = if let Some((code, _)) = result.error {
            format!("{code}\n")
        } else if !result.diagnostics.is_empty() {
            result
                .diagnostics
                .iter()
                .map(|d| format!("{}\n", d.render()))
                .collect()
        } else if result.dut_errors != 0 {
            format!("DUT errors: {}\n", result.dut_errors)
        } else {
            "OK\n".to_owned()
        };
        assert_eq!(
            expected,
            fs::read_to_string(path.with_extension("expected")).unwrap(),
            "{name}"
        );
        if result.ok() {
            assert_eq!(
                result.output,
                fs::read_to_string(path.with_extension("out")).unwrap(),
                "{name}"
            );
        } else {
            assert!(!result.to_json().contains("\"ok\":true"));
        }
        if name == "coverage" {
            let group = result
                .coverage
                .iter()
                .flat_map(|t| &t.groups)
                .find(|g| g.name == "cg")
                .unwrap();
            assert_eq!(group.samples, 2);
            assert_eq!(
                group.items[0]
                    .buckets
                    .iter()
                    .map(|b| b.hits)
                    .collect::<Vec<_>>(),
                [1, 1]
            );
        }
    }
}

#[test]
fn derived_coverage_json_preserves_native_report_data() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for name in [
        "coverage-graphs/cross-transition",
        "coverage-graphs/duplicate-labels",
        "coverage-graphs/empty-unbounded",
        "coverage-graphs/all-masked",
        "wasm-api/derived-unbucketed",
    ] {
        let path = root.join(name);
        let response = evaluate(
            &fs::read_to_string(path.with_extension("e")).unwrap(),
            Operation::Run,
            u64::MAX,
        );
        assert!(response.ok(), "{name}: {:?}", response.diagnostics);
        let coverage = fs::read_to_string(path.with_extension("browser.json")).unwrap();
        let expected = format!(
            "{{\"version\":1,\"operation\":\"run\",\"seed\":\"18446744073709551615\",\"ok\":true,\"output\":\"\",\"diagnostics\":[],\"dutErrors\":0,\"coverage\":{},\"error\":null}}",
            coverage.trim()
        );
        assert_eq!(response.to_json(), expected, "{name}");
    }
}
