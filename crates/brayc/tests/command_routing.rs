use std::process::Command;

#[test]
fn brayc_binary_routes_to_the_loose_file_driver() {
    let output = Command::new(env!("CARGO_BIN_EXE_brayc"))
        .arg("--help")
        .output()
        .unwrap_or_else(|error| panic!("brayc should run: {error:?}"));

    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Bray compiler"));
    assert!(stdout.contains("inspect"));
    assert!(!stdout.contains("vendor"));
}
