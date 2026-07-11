use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_compiler_known::generate_catalog_output;

const GENERATED_SOURCE_PATH: &str =
    "crates/bray-compiler-known/src/catalog/generated/compiler_known.rs";
const GENERATED_DIGEST_PATH: &str =
    "crates/bray-compiler-known/src/catalog/generated/catalog.sha256";

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let Some(command) = arguments.next() else {
        eprintln!("usage: cargo xtask compiler-known generate [--check]");
        return ExitCode::FAILURE;
    };

    if command != "compiler-known" || arguments.next().as_deref() != Some("generate") {
        eprintln!("usage: cargo xtask compiler-known generate [--check]");
        return ExitCode::FAILURE;
    }

    let check = match arguments.next().as_deref() {
        None => false,
        Some("--check") => true,
        Some(argument) => {
            eprintln!("unexpected argument: {argument}");
            return ExitCode::FAILURE;
        }
    };

    if let Some(argument) = arguments.next() {
        eprintln!("unexpected argument: {argument}");
        return ExitCode::FAILURE;
    }

    match generate(check) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn generate(check: bool) -> Result<(), String> {
    let output = generate_catalog_output()
        .map_err(|error| format!("compiler-known catalog generation failed: {error}"))?;
    let root = workspace_root()?;
    let digest = format!("{}\n", output.source_digest());
    let files = [
        (
            root.join(GENERATED_SOURCE_PATH),
            output.rust_source().as_bytes(),
        ),
        (root.join(GENERATED_DIGEST_PATH), digest.as_bytes()),
    ];

    if check {
        check_files(&files)
    } else {
        write_files(&files)
    }
}

fn workspace_root() -> Result<PathBuf, String> {
    let xtask = Path::new(env!("CARGO_MANIFEST_DIR"));

    xtask
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask manifest directory has no workspace parent".to_owned())
}

fn check_files(files: &[(PathBuf, &[u8])]) -> Result<(), String> {
    let mut stale = Vec::new();

    for (path, expected) in files {
        let actual = std::fs::read(path).map_err(|error| io_error("read", path, error))?;

        if actual != *expected {
            stale.push(path.display().to_string());
        }
    }

    if stale.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "compiler-known generated output is stale: {}",
            stale.join(", ")
        ))
    }
}

fn write_files(files: &[(PathBuf, &[u8])]) -> Result<(), String> {
    for (path, contents) in files {
        if std::fs::read(path).ok().as_deref() == Some(*contents) {
            continue;
        }

        std::fs::write(path, contents).map_err(|error| io_error("write", path, error))?;
    }

    Ok(())
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> String {
    format!("failed to {action} {}: {error}", path.display())
}

#[cfg(test)]
mod tests {
    use super::{check_files, run};

    #[test]
    fn unrelated_commands_fail_without_side_effects() {
        assert_eq!(
            run(["other".to_owned()].into_iter()),
            std::process::ExitCode::FAILURE
        );
    }

    #[test]
    fn freshness_check_rejects_stale_output() {
        let path =
            std::env::temp_dir().join(format!("bray-compiler-known-check-{}", std::process::id()));
        let files = [(path.clone(), b"current".as_slice())];

        if let Err(error) = std::fs::write(&path, b"stale") {
            panic!("test output should be writable: {error}");
        }

        assert!(check_files(&files).is_err());

        if let Err(error) = std::fs::write(&path, b"current") {
            panic!("test output should be writable: {error}");
        }

        assert_eq!(check_files(&files), Ok(()));

        if let Err(error) = std::fs::remove_file(&path) {
            panic!("test output should be removable: {error}");
        }
    }
}
