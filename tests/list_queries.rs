use std::{fs, path::Path};

#[test]
fn list_query_fixtures() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/list-queries");
    let mut paths: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "e"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty());
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        let parsed = e1647::parser::parse(&source);
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
        let diagnostics = if result.diagnostics.is_empty() {
            // Valid runtime programs must also pass the CLI's type check.
            assert!(
                e1647::typing::check_types(&elaborated).is_empty(),
                "{path:?}"
            );
            "OK\n".to_owned()
        } else {
            result
                .diagnostics
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
            result.output,
            fs::read_to_string(path.with_extension("out")).unwrap(),
            "{path:?}"
        );
    }
}

#[test]
fn list_queries_validate_arity_before_traversing_empty_lists() {
    for method in ["has", "count", "all", "all_indices"] {
        for arguments in ["", "TRUE, TRUE"] {
            let source =
                format!("extend sys {{ run() is also {{ out({{}}.{method}({arguments})); }}; }};");
            let parsed = e1647::parser::parse(&source);
            assert!(parsed.diagnostics.is_empty());
            let elaborated = e1647::elab::elaborate(&parsed.declarations);
            let result = e1647::interp::run(&elaborated, 1);
            assert_eq!(result.diagnostics.len(), 1, "{method}({arguments})");
            assert_eq!(result.diagnostics[0].code, "E0402");
            assert!(
                result.diagnostics[0]
                    .message
                    .contains("expects one boolean item expression")
            );
        }
    }
}
