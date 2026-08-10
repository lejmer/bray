use std::path::{Path, PathBuf};

#[test]
fn command_packages_depend_only_on_their_owned_driver() {
    let bray = read_manifest("bray");
    let brayc = read_manifest("brayc");
    let brayfmt = read_manifest("brayfmt");
    let loose_file_driver = read_manifest("bray-driver");
    let tooling = read_manifest("bray-tooling");

    assert_dependency(&bray, "bray-tooling");
    assert_no_regular_dependency(&bray, "bray-driver");
    assert_no_dependency(&bray, "bray-compilation");
    assert_no_dependency(&bray, "bray-formatter");
    assert_no_dependency(&bray, "bray-lsp");

    assert_dependency(&brayc, "bray-driver");
    assert_no_dependency(&brayc, "bray-tooling");

    assert_dependency(&brayfmt, "bray-formatter");
    assert_no_dependency(&brayfmt, "bray-compilation");
    assert_no_dependency(&brayfmt, "bray-driver");
    assert_no_dependency(&brayfmt, "bray");

    assert_dependency(&loose_file_driver, "bray-tooling");
    assert_no_dependency(&loose_file_driver, "bray");

    assert_no_dependency(&tooling, "bray");
    assert_no_dependency(&tooling, "bray-driver");
}

fn read_manifest(package: &str) -> String {
    let path = workspace_root()
        .join("crates")
        .join(package)
        .join("Cargo.toml");

    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("could not read {}: {error:?}", path.display()))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("bray package must be inside the workspace"))
        .to_path_buf()
}

fn assert_dependency(manifest: &str, dependency: &str) {
    assert!(
        manifest
            .lines()
            .any(|line| line.starts_with(&format!("{dependency} = "))),
        "expected dependency {dependency}"
    );
}

fn assert_no_dependency(manifest: &str, dependency: &str) {
    assert!(
        manifest
            .lines()
            .all(|line| !line.starts_with(&format!("{dependency} = "))),
        "unexpected dependency {dependency}"
    );
}

fn assert_no_regular_dependency(manifest: &str, dependency: &str) {
    let mut section = "";

    for line in manifest.lines() {
        if line.starts_with('[') && line.ends_with(']') {
            section = line;

            continue;
        }

        assert!(
            section != "[dependencies]" || !line.starts_with(&format!("{dependency} = ")),
            "unexpected regular dependency {dependency}"
        );
    }
}
