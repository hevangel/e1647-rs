use std::{fs, path::Path};

#[test]
fn list_extrema_fixtures() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/list-extrema");
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
        let diagnostics = if result.diagnostics.is_empty() {
            let typed = e1647::typing::check_types(&elaborated);
            assert!(typed.is_empty(), "{path:?}: {typed:?}");
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
fn list_extrema_reject_extra_arguments_and_unorderable_singleton_keys() {
    for method in ["min", "max", "min_index", "max_index"] {
        for (receiver, arguments, message) in [
            ("{}", "1, 2", "expects at most one item expression"),
            ("{}", "it", "of an empty list"),
            ("{1}", "TRUE", "key is not orderable"),
        ] {
            let parsed = e1647::parser::parse(&format!(
                "extend sys {{ run() is also {{ out({receiver}.{method}({arguments})); }}; }};"
            ));
            assert!(parsed.diagnostics.is_empty());
            let elaborated = e1647::elab::elaborate(&parsed.declarations);
            let result = e1647::interp::run(&elaborated, 1);
            assert_eq!(result.diagnostics.len(), 1, "{method}: {message}");
            assert_eq!(result.diagnostics[0].code, "E0402");
            assert!(result.diagnostics[0].message.contains(message));
        }
    }
}
