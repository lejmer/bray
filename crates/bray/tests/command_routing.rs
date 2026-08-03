use std::process::Command;

#[test]
fn bray_binary_routes_to_the_build_tool() {
    let output = Command::new(env!("CARGO_BIN_EXE_bray"))
        .arg("--help")
        .output()
        .unwrap_or_else(|error| panic!("bray should run: {error:?}"));

    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("vendor"));
}
