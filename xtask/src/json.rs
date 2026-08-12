use std::fs;
use std::path::Path;

use serde::Serialize;

pub(crate) fn write_pretty(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("could not encode {}: {error}", path.display()))?;

    bytes.push(b'\n');

    fs::write(path, bytes).map_err(|error| format!("could not write {}: {error}", path.display()))
}
