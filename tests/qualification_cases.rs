use std::{fs, path::Path};

#[test]
fn qualification_candidates_have_independent_native_goldens() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/qualification");
    let mut paths: Vec<_> = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "e"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 3);
    for path in paths {
        let parsed = e1647::parser::parse(&fs::read_to_string(&path).unwrap());
        let elaborated = e1647::elab::elaborate(&parsed.declarations);
        let mut diagnostics = parsed.diagnostics;
        diagnostics.extend(elaborated.diagnostics.clone());
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
            "{path:?}"
        );
        let result = e1647::interp::run(&elaborated, 1);
        assert!(
            result.diagnostics.is_empty(),
            "{path:?}: {:?}",
            result.diagnostics
        );
        assert_eq!(
            result.output,
            fs::read_to_string(path.with_extension("out")).unwrap(),
            "{path:?}"
        );
    }
}
