use std::fs;
use std::path::Path;

/// Golden diagnostic fixtures: every `tests/fixtures/*.e` must produce
/// exactly the diagnostics listed in its `.expected` file (empty input
/// expected is written as `OK`). This is the byte-exact contract for the
/// parser's diagnostic output.
#[test]
fn fixture_diagnostics_match_expected() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .expect("fixtures directory exists")
        .collect::<Result<_, _>>()
        .expect("fixtures directory is readable");
    entries.sort_by_key(|entry| entry.file_name());
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("e") {
            continue;
        }
        let expected_path = path.with_extension("expected");
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|_| panic!("missing expected file for {}", path.display()));
        let source = fs::read_to_string(&path).expect("fixture is readable");
        let result = e1647::parser::parse(&source);
        let mut actual = String::new();
        if result.diagnostics.is_empty() {
            actual.push_str("OK\n");
        } else {
            for diagnostic in &result.diagnostics {
                actual.push_str(&diagnostic.render());
                actual.push('\n');
            }
        }
        checked += 1;
        if actual != expected {
            failures.push(format!(
                "--- {} (relative: tests/fixtures/{})\nexpected:\n{}actual:\n{}",
                path.display(),
                path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                expected,
                actual
            ));
        }
    }
    assert!(checked > 0, "no fixtures were found");
    assert!(
        failures.is_empty(),
        "{} fixture(s) mismatched:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
