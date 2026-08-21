pub(crate) fn output_detail(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = stderr.trim();

    if detail.is_empty() {
        stdout.trim().to_owned()
    } else {
        detail.to_owned()
    }
}
