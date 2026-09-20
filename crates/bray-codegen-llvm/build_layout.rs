use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub(crate) fn artifact_profile_directory(
    workspace_directory: &Path,
    configured_target_directory: Option<OsString>,
    build_profile_directory: &Path,
    target: &OsStr,
    profile: &OsStr,
) -> PathBuf {
    let mut target_directory = match configured_target_directory
        .filter(|directory| !directory.is_empty())
        .map(PathBuf::from)
    {
        Some(directory) if directory.is_absolute() => directory,
        Some(directory) => workspace_directory.join(directory),
        None => workspace_directory.join("target"),
    };

    if build_profile_directory.parent().and_then(Path::file_name) == Some(target) {
        target_directory.push(target);
    }

    target_directory.push(profile);

    target_directory
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use super::artifact_profile_directory;

    #[test]
    fn final_profile_directory_follows_cargo_target_layouts() {
        let workspace = Path::new("workspace");
        let target = "x86_64-pc-windows-msvc";
        let absolute_target = std::env::temp_dir().join("bray-cargo-target");

        let cases = [
            (
                None,
                PathBuf::from("workspace/target/cargo/release"),
                PathBuf::from("workspace/target/release"),
            ),
            (
                Some(OsString::from("artifacts")),
                PathBuf::from("workspace/target/cargo/release"),
                PathBuf::from("workspace/artifacts/release"),
            ),
            (
                Some(absolute_target.clone().into_os_string()),
                PathBuf::from("workspace/target/cargo/release"),
                absolute_target.join("release"),
            ),
            (
                None,
                PathBuf::from(format!("workspace/target/cargo/{target}/release")),
                PathBuf::from(format!("workspace/target/{target}/release")),
            ),
        ];

        for (configured, build_profile, expected) in cases {
            assert_eq!(
                artifact_profile_directory(
                    workspace,
                    configured,
                    &build_profile,
                    target.as_ref(),
                    "release".as_ref(),
                ),
                expected
            );
        }
    }
}
