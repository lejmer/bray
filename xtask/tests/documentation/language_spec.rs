use bray_llvm_toolchain::workspace_root;

#[test]
fn language_specification_exists() {
    let root = workspace_root().expect("documentation tests must run in a Bray workspace");
    let specification = root.join("docs/language");

    assert!(specification.is_dir(), "language specification directory is missing");
    assert!(specification.join("index.md").is_file(), "language specification entry point is missing");
}
