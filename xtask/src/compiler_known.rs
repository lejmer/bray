use std::path::PathBuf;
use std::process::ExitCode;

use bray_compiler_known::generate_catalog_output;

use crate::{command, workspace};

const USAGE: &str = "usage: cargo xtask compiler-known <generate [--check] | check>";
const GENERATED_SOURCE_PATH: &str =
    "crates/bray-compiler-known/src/catalog/generated/compiler_known.rs";
const GENERATED_DIGEST_PATH: &str =
    "crates/bray-compiler-known/src/catalog/generated/catalog.sha256";

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let Some(action) = arguments.next() else {
        eprintln!("{USAGE}");

        return ExitCode::FAILURE;
    };

    let result = match action.as_str() {
        "generate" => generate_command(arguments),
        "check" => check_command(arguments),
        _ => Err(format!("unexpected compiler-known command: {action}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");

            ExitCode::FAILURE
        }
    }
}

fn generate_command(mut arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let check_only = match arguments.next().as_deref() {
        None => false,
        Some("--check") => true,
        Some(argument) => return Err(format!("unexpected argument: {argument}")),
    };

    command::reject_trailing_argument(arguments)?;

    generate(check_only)
}

fn check_command(arguments: impl Iterator<Item = String>) -> Result<(), String> {
    command::reject_trailing_argument(arguments)?;
    generate(true)?;

    crate::progress::run("Validating compiler-known semantics", || {
        bray_compilation::check_compiler_known_catalog()
            .map(|_| ())
            .map_err(|error| format!("compiler-known semantic validation failed: {error}"))
    })
}

fn generate(check: bool) -> Result<(), String> {
    let output = crate::progress::run("Generating the compiler-known catalog", || {
        generate_catalog_output()
            .map_err(|error| format!("compiler-known catalog generation failed: {error}"))
    })?;

    let root = workspace::root()?;
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

fn check_files(files: &[(PathBuf, &[u8])]) -> Result<(), String> {
    let mut stale = Vec::new();

    for (path, expected) in files {
        let actual =
            std::fs::read(path).map_err(|error| workspace::io_error("read", path, error))?;

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

        std::fs::write(path, contents)
            .map_err(|error| workspace::io_error("write", path, error))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_files, run};

    #[test]
    fn missing_actions_fail_without_side_effects() {
        assert_eq!(run(std::iter::empty()), std::process::ExitCode::FAILURE);
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

    #[test]
    fn check_command_runs_generated_and_semantic_validation() {
        assert_eq!(
            run(["check".to_owned()].into_iter()),
            std::process::ExitCode::SUCCESS
        );
    }

    #[test]
    fn check_command_rejects_arguments() {
        assert_eq!(
            run(["check".to_owned(), "--check".to_owned(),].into_iter()),
            std::process::ExitCode::FAILURE
        );
    }
}
