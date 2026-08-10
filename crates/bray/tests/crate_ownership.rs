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

#[test]
fn target_specific_regular_dependencies_are_part_of_the_ownership_contract() {
    let regular = "[target.'cfg(windows)'.dependencies]\nbray-driver = { path = '../bray-driver' }";
    let table = "[target.x86_64-pc-windows-msvc.dependencies.bray-driver]\npath = '../bray-driver'";

    let development =
        "[target.'cfg(windows)'.dev-dependencies]\nbray-driver = { path = '../bray-driver' }";

    assert!(has_regular_dependency(regular, "bray-driver"));
    assert!(has_regular_dependency(table, "bray-driver"));
    assert!(!has_regular_dependency(development, "bray-driver"));
}

fn assert_no_regular_dependency(manifest: &str, dependency: &str) {
    assert!(
        !has_regular_dependency(manifest, dependency),
        "unexpected regular dependency {dependency}"
    );
}

fn has_regular_dependency(manifest: &str, dependency: &str) -> bool {
    let mut section = "";
    let dependency_table = format!("[dependencies.{dependency}]");
    let target_dependency_table_suffix = format!(".dependencies.{dependency}]");

    for line in manifest.lines() {
        if line.starts_with('[') && line.ends_with(']') {
            section = line;

            if section == dependency_table
                || (section.starts_with("[target.")
                    && section.ends_with(&target_dependency_table_suffix))
            {
                return true;
            }

            continue;
        }

        let regular_dependencies = section == "[dependencies]"
            || (section.starts_with("[target.") && section.ends_with(".dependencies]"));

        if regular_dependencies && line.starts_with(&format!("{dependency} = ")) {
            return true;
        }
    }

    false
}
