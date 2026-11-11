use std::fs;
use std::path::Path;

/// Golden diagnostic fixtures for the conservative type check: every
/// `tests/fixtures-type/*.e` must produce exactly the diagnostics
/// listed in its `.expected` file (empty input expected is written as
/// `OK`) through the full `parse → elaborate → typing::check_types`
/// pipeline — the same stages the CLI's `run`/`elab` commands apply.
/// This is the byte-exact contract for the E0220–E0223 family; the
/// `tests/fixtures/` harness above covers the parser's own contract.
#[test]
fn type_fixture_diagnostics_match_expected() {
    check_fixture_directory("fixtures-type");
}

#[test]
fn coverage_type_fixture_diagnostics_match_expected() {
    check_fixture_directory("fixtures/typing-coverage");
}

#[test]
fn inferred_local_fixture_diagnostics_match_expected() {
    check_fixture_directory("fixtures/typing-inference");
}

#[test]
fn inferred_locals_are_checked_by_the_cli_and_valid_programs_execute() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/typing-inference");
    for command in ["elab", "run"] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_e1647"))
            .arg(command)
            .arg(directory.join("locals.e"))
            .output()
            .expect("CLI starts");
        assert!(
            !result.status.success(),
            "{command} accepted definite mismatches"
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("error[E0220]: assignment"));
    }
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_e1647"))
        .arg("run")
        .arg(directory.join("runtime.e"))
        .output()
        .expect("CLI starts");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        fs::read_to_string(directory.join("runtime.out")).unwrap()
    );
}

#[test]
fn list_extrema_type_fixture_diagnostics_match_expected() {
    check_fixture_directory("fixtures/typing-list-extrema");
}

#[test]
fn list_query_type_fixture_diagnostics_match_expected() {
    check_fixture_directory("fixtures/typing-list-queries");
}

fn check_fixture_directory(directory: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(directory);
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .expect("type fixtures directory exists")
        .collect::<Result<_, _>>()
        .expect("type fixtures directory is readable");
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
        let parsed = e1647::parser::parse(&source);
        let elaborated = e1647::elab::elaborate(&parsed.declarations);
        let mut diagnostics = parsed.diagnostics;
        diagnostics.extend(e1647::typing::check_types(&elaborated));
        diagnostics.extend(elaborated.diagnostics);
        let mut actual = String::new();
        if diagnostics.is_empty() {
            actual.push_str("OK\n");
        } else {
            for diagnostic in &diagnostics {
                actual.push_str(&diagnostic.render());
                actual.push('\n');
            }
        }
        checked += 1;
        if actual != expected {
            failures.push(format!(
                "--- {} (relative: tests/{directory}/{})\nexpected:\n{}actual:\n{}",
                path.display(),
                path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                expected,
                actual
            ));
        }
    }
    assert!(checked > 0, "no type fixtures were found");
    assert!(
        failures.is_empty(),
        "{} type fixture(s) mismatched:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The fail-open contract: a body the checker cannot pin down must not
/// produce diagnostics. Unknown locals, template types, casts, `table`
/// receivers and `when`-guarded members all stay silent — and the
/// newly-walked surfaces (constraints, when views, on bodies) accept
/// their legitimate shapes: when-view siblings, determinant references,
/// the predefined any-instance members.
#[test]
fn uncertain_constructs_stay_silent() {
    let source = "\
struct base_s {\n    common: uint;\n};\n\
struct derived_s like base_s {\n    extra: list of uint;\n};\n\
interface shape_i {\n    area(): real;\n};\n\
struct impl_s implementing shape_i {\n    area(): real is { return 1.0; };\n};\n\
type colors: [red, green, blue];\n\
template box_t of (T: type) {\n    item: T;\n};\n\
struct whened_s {\n    kind: colors;\n    tone: uint;\n    when red'kind {\n        depth: uint;\n        show() is {\n            // The when view's own fields and the base's fields resolve.\n            tone = depth + 1;\n        };\n    };\n    on ev_reset {\n        tone = 0;\n    };\n};\n\
extend sys {\n    ev_reset: event;\n    run() is also {\n        // Enumerals coerce to ints.\n        var c: colors = red;\n        var n: int = c + 1;\n\
        // NULL unifies with object kinds.\n        var d: derived_s;\n        d = NULL;\n\
        // The like chain accepts the derived value in the base slot.\n        var b: base_s = new derived_s;\n\
        // An interface-typed slot takes the implementing struct.\n        var s: shape_i = new impl_s;\n\
        // A when-only member is conditionally present: silent.\n        var w: whened_s;\n        var t: uint = w.tone;\n\
        // The predefined any-instance members are not declared per type.\n        me.get_children();\n        var p: string = me.hdl_path();\n\
        // Template instances, untyped vars and unknown routines.\n        var boxed: box_t of (uint) = new box_t of (uint);\n        var mystery = unknown_thing(\"arg\");\n\
        print n, t, p, boxed, mystery;\n    };\n};\n";
    let parsed = e1647::parser::parse(source);
    let elaborated = e1647::elab::elaborate(&parsed.declarations);
    let type_diagnostics = e1647::typing::check_types(&elaborated);
    assert!(
        type_diagnostics.is_empty(),
        "the checker must stay silent on the uncertain subset: {:?}",
        type_diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}
