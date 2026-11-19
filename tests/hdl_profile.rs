use std::{fs, path::Path};

/// The frontend half is portable. Real backend/profile evidence comes from
/// scripts/qualify_hdl.py; these declarations alone do not prove simulation.
#[test]
fn hdl_profile_frontend_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hdl-profile");
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "e"))
        .collect();
    files.sort();
    assert_eq!(files.len(), 6);
    for path in files {
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
    }
}
