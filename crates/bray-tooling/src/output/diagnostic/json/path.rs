use std::path::Path;

use serde::Serialize;

/// Exact platform path plus a display-only rendering for human inspection.
#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticPathJson {
    display: String,
    encoding: &'static str,
    units: Vec<u32>,
}

impl DiagnosticPathJson {
    pub(in crate::output::diagnostic::json) fn from_path(path: &Path) -> Self {
        let (encoding, units) = encoded_path(path);

        Self {
            display: path.to_string_lossy().into_owned(),
            encoding,
            units,
        }
    }
}

#[cfg(unix)]
fn encoded_path(path: &Path) -> (&'static str, Vec<u32>) {
    use std::os::unix::ffi::OsStrExt;

    (
        "unix_bytes",
        path.as_os_str()
            .as_bytes()
            .iter()
            .copied()
            .map(u32::from)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::DiagnosticPathJson;

    #[cfg(windows)]
    #[test]
    fn windows_paths_preserve_non_unicode_wide_units() {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        let path = std::path::PathBuf::from(OsString::from_wide(&[0x0061, 0xd800, 0x0062]));

        let json = serde_json::to_value(DiagnosticPathJson::from_path(&path))
            .unwrap_or_else(|error| panic!("path should serialize: {error:?}"));

        assert_eq!(json["encoding"], "windows_wide");
        assert_eq!(json["units"], serde_json::json!([0x0061, 0xd800, 0x0062]));
    }

    #[cfg(unix)]
    #[test]
    fn unix_paths_preserve_non_unicode_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let path = std::path::PathBuf::from(OsString::from_vec(vec![0x61, 0xff, 0x62]));

        let json = serde_json::to_value(DiagnosticPathJson::from_path(&path))
            .unwrap_or_else(|error| panic!("path should serialize: {error:?}"));

        assert_eq!(json["encoding"], "unix_bytes");
        assert_eq!(json["units"], serde_json::json!([0x61, 0xff, 0x62]));
    }
}

#[cfg(windows)]
fn encoded_path(path: &Path) -> (&'static str, Vec<u32>) {
    use std::os::windows::ffi::OsStrExt;

    (
        "windows_wide",
        path.as_os_str().encode_wide().map(u32::from).collect(),
    )
}

#[cfg(not(any(unix, windows)))]
fn encoded_path(path: &Path) -> (&'static str, Vec<u32>) {
    (
        "platform_bytes",
        path.as_os_str()
            .as_encoded_bytes()
            .iter()
            .copied()
            .map(u32::from)
            .collect(),
    )
}
