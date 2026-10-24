use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::{command, workspace};

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = (|| {
        let check_only = match arguments.next().as_deref() {
            None => false,
            Some("check") => {
                command::reject_trailing_argument(arguments)?;

                true
            }
            Some(action) => return Err(format!("unexpected style command: {action}")),
        };

        let root = workspace::root()?;

        crate::progress::run("Checking compiled fixture source filenames", || {
            check_fixture_source_names(&root)
        })?;

        if check_only {
            crate::progress::run("Checking Rust structural style", || {
                rust_style::check_workspace(&root)
            })?;

            crate::source_format::check_workspace(&root)
        } else {
            crate::progress::run("Applying Rust structural style", || {
                rust_style::fix_workspace(&root)
            })?;

            crate::source_format::write_workspace(&root)
        }
    })();

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");

            ExitCode::FAILURE
        }
    }
}

fn check_fixture_source_names(root: &Path) -> Result<(), String> {
    let mut invalid = Vec::new();

    collect_invalid_fixture_names(&root.join("xtask/fixtures"), &mut invalid)?;
    invalid.sort_unstable();

    if invalid.is_empty() {
        return Ok(());
    }

    Err(invalid
        .iter()
        .map(|path| {
            format!(
                "compiled fixture source filename must use kebab-case: {}",
                path.display()
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

fn collect_invalid_fixture_names(
    directory: &Path,
    invalid: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| workspace::io_error("read directory", directory, error))?;

    for entry in entries {
        let entry =
            entry.map_err(|error| workspace::io_error("read directory entry", directory, error))?;

        let path = entry.path();

        let kind = entry
            .file_type()
            .map_err(|error| workspace::io_error("inspect", &path, error))?;

        if kind.is_dir() {
            if !matches!(entry.file_name().to_str(), Some("build" | "target")) {
                collect_invalid_fixture_names(&path, invalid)?;
            }
        } else if is_compiled_source(&path)
            && path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_none_or(|stem| {
                    !stem.split('-').all(|word| {
                        !word.is_empty()
                            && word
                                .bytes()
                                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                    })
                })
        {
            invalid.push(path);
        }
    }

    Ok(())
}

fn is_compiled_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "bray"
                    | "c"
                    | "cc"
                    | "cpp"
                    | "cxx"
                    | "c++"
                    | "h"
                    | "hh"
                    | "hpp"
                    | "hxx"
                    | "h++"
                    | "rs"
                    | "s"
                    | "asm"
                    | "ll"
            )
        })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::check_fixture_source_names;

    fn write_file(root: &Path, relative: &str) {
        let path = root.join(relative);

        fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
        fs::write(path, []).expect("fixture file");
    }

    #[test]
    fn accepts_compiled_scenarios_headers_and_structural_entry_names() {
        let root = tempfile::tempdir().expect("test workspace");

        for file in [
            "xtask/fixtures/main.bray",
            "xtask/fixtures/project/src/main.rs",
            "xtask/fixtures/project/src/lib.rs",
            "xtask/fixtures/project/build.rs",
            "xtask/fixtures/async-tasks.bray",
            "xtask/fixtures/static-storage-host.c",
            "xtask/fixtures/static-storage-host.h",
            "xtask/fixtures/native-peer.cpp",
            "xtask/fixtures/runtime-smoke.rs",
            "xtask/fixtures/entry-i32.bray",
            "xtask/fixtures/cxx/scenario-2.cxx",
            "xtask/fixtures/cxx/scenario-2.hpp",
            "xtask/fixtures/assembly/entry-point.S",
            "xtask/fixtures/llvm/native-input.ll",
        ] {
            write_file(root.path(), file);
        }

        assert_eq!(check_fixture_source_names(root.path()), Ok(()));
    }

    #[test]
    fn rejects_mixed_names_across_compiled_languages_in_path_order() {
        let root = tempfile::tempdir().expect("test workspace");

        let files = [
            "nested/invalid_name.bray",
            "upper/InvalidName.c",
            "double--hyphen.cpp",
            "trailing-.rs",
            "header_name.h",
            "-leading.cc",
            "space name.cxx",
            "Upper.CPP",
            "header_name.hpp",
            "unicode-\u{e9}.s",
            "llvm_ir.ll",
        ];

        for file in files {
            write_file(root.path(), &format!("xtask/fixtures/{file}"));
        }

        let error = check_fixture_source_names(root.path()).expect_err("invalid names must fail");
        let lines = error.lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), files.len());
        assert!(lines.windows(2).all(|pair| pair[0] <= pair[1]));

        for file in files {
            let name = Path::new(file)
                .file_name()
                .expect("fixture filename")
                .to_str()
                .expect("UTF-8 test filename");

            assert!(
                error.contains(name),
                "missing invalid filename {file}: {error}"
            );
        }
    }

    #[test]
    fn leaves_production_corpus_data_and_generated_output_names_outside_the_rule() {
        let root = tempfile::tempdir().expect("test workspace");

        for file in [
            "standard-library/std/src/some_module.bray",
            "crates/example/src/some_module.rs",
            "fuzz/corpus/parser/token_name.bray",
            "crates/bray-parser/tests/fixtures/token_name.bray",
            "xtask/fixtures/coverage_data.json",
            "xtask/fixtures/README.md",
            "xtask/fixtures/project/build/generated_input.c",
            "xtask/fixtures/project/target/generated_input.rs",
        ] {
            write_file(root.path(), file);
        }

        assert_eq!(check_fixture_source_names(root.path()), Ok(()));
    }

    #[test]
    fn preserves_the_directory_and_io_cause_when_fixtures_cannot_be_read() {
        let root = tempfile::tempdir().expect("test workspace");

        write_file(root.path(), "xtask/fixtures");

        let error = check_fixture_source_names(root.path()).expect_err("fixture root is a file");

        assert!(error.contains(&root.path().join("xtask/fixtures").display().to_string()));
        assert!(error.contains("read directory"));
    }
}
