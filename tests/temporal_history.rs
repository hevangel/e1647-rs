use std::{fs, path::Path};

#[test]
fn temporal_history_fixtures() {
    check_fixture_directory("temporal-history");
}

#[test]
fn temporal_edge_fixtures() {
    check_fixture_directory("temporal-edges");
}

fn check_fixture_directory(name: &str) {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
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
        let actual = if result.diagnostics.is_empty() {
            "OK\n".to_owned()
        } else {
            result
                .diagnostics
                .iter()
                .map(|diagnostic| format!("{}\n", diagnostic.render()))
                .collect()
        };
        assert_eq!(
            actual,
            fs::read_to_string(path.with_extension("expected")).unwrap(),
            "{path:?}"
        );
        assert_eq!(
            result.output,
            fs::read_to_string(path.with_extension("out")).unwrap(),
            "{path:?}"
        );
    }
}
