use std::fs;
use std::path::Path;

use serde::Serialize;

pub(crate) fn write_compact(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("could not encode {}: {error}", path.display()))?;

    write(path, bytes)
}

pub(crate) fn write_pretty(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("could not encode {}: {error}", path.display()))?;

    write(path, bytes)
}

fn write(path: &Path, mut bytes: Vec<u8>) -> Result<(), String> {
    bytes.push(b'\n');

    fs::write(path, bytes).map_err(|error| format!("could not write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn compact_json_is_single_line_and_newline_terminated() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory must exist: {error}"));

        let path = directory.path().join("report.json");

        super::write_compact(&path, &serde_json::json!({ "value": 42 }))
            .unwrap_or_else(|error| panic!("compact JSON must be written: {error}"));

        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("compact JSON must be readable: {error}"));

        assert_eq!(bytes, b"{\"value\":42}\n");
    }
}
