use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn copy_dynamic_library(source: &Path, destination_directory: &Path) -> io::Result<()> {
    fs::create_dir_all(destination_directory)?;

    let destination = destination_directory.join("LLVM-C.dll");

    // Cargo can run another LLVM consumer while staging this backend's DLL.
    // Preserve an identical library that Windows may already have loaded.
    match bray_base::sha256_file(&destination) {
        Ok(identity) if identity == bray_base::sha256_file(source)? => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    fs::copy(source, destination)?;

    Ok(())
}

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

    use super::{artifact_profile_directory, copy_dynamic_library};

    #[test]
    fn staged_library_is_created_and_replaced_when_content_changes() {
        let directory = tempfile::tempdir().expect("LLVM staging");
        let source = directory.path().join("source.dll");
        let destination_directory = directory.path().join("release/deps");
        let destination = destination_directory.join("LLVM-C.dll");

        std::fs::write(&source, b"first library").expect("source library");
        copy_dynamic_library(&source, &destination_directory).expect("initial staging");

        assert_eq!(
            std::fs::read(&destination).expect("staged library"),
            b"first library"
        );

        std::fs::write(&source, b"other library").expect("replacement library");
        copy_dynamic_library(&source, &destination_directory).expect("replacement staging");

        assert_eq!(
            std::fs::read(&destination).expect("replaced library"),
            b"other library"
        );
    }

    #[cfg(windows)]
    #[test]
    fn identical_staged_library_can_remain_locked_against_writes() {
        use std::os::windows::fs::OpenOptionsExt as _;

        let directory = tempfile::tempdir().expect("locked LLVM staging");
        let source = directory.path().join("source.dll");
        let destination_directory = directory.path().join("release");
        let destination = destination_directory.join("LLVM-C.dll");

        std::fs::write(&source, b"LLVM library").expect("source library");
        copy_dynamic_library(&source, &destination_directory).expect("initial staging");

        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&destination)
            .expect("lock staged library against writes");

        assert!(
            std::fs::copy(&source, &destination).is_err(),
            "the destination must be locked"
        );

        copy_dynamic_library(&source, &destination_directory).expect("reuse locked library");

        std::fs::write(&source, b"new library!").expect("changed source library");

        assert!(copy_dynamic_library(&source, &destination_directory).is_err());

        drop(locked);
    }

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
