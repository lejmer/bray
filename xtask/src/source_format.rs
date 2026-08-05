use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_formatter::{FormatFileOutcome, FormatMode, FormatterConfiguration, format_files};
use unicode_width::UnicodeWidthStr;

use crate::{command, workspace};

const MAINTAINED_SOURCE_ROOTS: &[&str] = &[
    "standard-library",
    "examples",
    "xtask/fixtures",
    "fuzz/corpus",
];

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = match arguments.next().as_deref() {
        None => workspace::root().and_then(|root| write_workspace(&root)),
        Some("check") => command::reject_trailing_argument(arguments)
            .and_then(|()| workspace::root())
            .and_then(|root| check_workspace(&root)),
        Some(action) => Err(format!("unexpected format command: {action}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");

            ExitCode::FAILURE
        }
    }
}

pub(crate) fn write_workspace(root: &Path) -> Result<(), String> {
    format_workspace(root, FormatMode::Write)
}

pub(crate) fn check_workspace(root: &Path) -> Result<(), String> {
    format_workspace(root, FormatMode::Check)
}

fn format_workspace(root: &Path, mode: FormatMode) -> Result<(), String> {
    let paths = maintained_sources(root)?;
    let configuration = FormatterConfiguration::default();
    let worker_count = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
    let results = format_files(&paths, mode, &configuration, worker_count);
    let maximum_width = configuration.maximum_line_width().into();
    let mut failures = Vec::new();

    for (path, result) in paths.iter().zip(results) {
        match result {
            Ok(FormatFileOutcome::WouldChange) => {
                failures.push(format!("source needs formatting: {}", path.display()));
            }
            Ok(FormatFileOutcome::Unchanged | FormatFileOutcome::Written) => {
                collect_width_failures(path, maximum_width, &mut failures)?;
            }
            Err(error) => failures.push(format!(
                "could not format {}: {:?}",
                error.path().display(),
                error.kind()
            )),
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

fn maintained_sources(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();

    for relative_root in MAINTAINED_SOURCE_ROOTS {
        collect_sources(&root.join(relative_root), &mut paths)?;
    }

    paths.sort_unstable();

    Ok(paths)
}

fn collect_sources(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }

    let entries = fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?;

    for entry in entries {
        let entry =
            entry.map_err(|error| format!("could not read {}: {error}", directory.display()))?;

        let file_type = entry
            .file_type()
            .map_err(|error| format!("could not inspect {}: {error}", entry.path().display()))?;

        let path = entry.path();

        if file_type.is_dir() {
            if !is_generated_directory(&path) {
                collect_sources(&path, paths)?;
            }
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "bray")
        {
            paths.push(path);
        }
    }

    Ok(())
}

fn is_generated_directory(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| matches!(name.to_str(), Some("build" | "target")))
}

fn collect_width_failures(
    path: &Path,
    maximum_width: usize,
    failures: &mut Vec<String>,
) -> Result<(), String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;

    for (index, line) in source.lines().enumerate() {
        let width = UnicodeWidthStr::width(line);

        if width > maximum_width {
            failures.push(format!(
                "formatted source exceeds {maximum_width} columns at {}:{} ({width} columns)",
                path.display(),
                index + 1
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use super::{check_workspace, maintained_sources, write_workspace};

    #[test]
    fn write_and_check_share_the_maintained_source_selection() {
        let workspace = tempdir()
            .unwrap_or_else(|error| panic!("temporary workspace must be created: {error}"));

        let example = workspace.path().join("examples/demo/src/main.bray");
        let recovery = workspace.path().join("fuzz/corpus/recovery/broken.bray");
        let ignored = workspace.path().join("other/ignored.bray");

        write_source(&example, "module demo;func main(){return;}");
        write_source(&recovery, "module broken;func main(){let value=@;}\n");
        write_source(&ignored, "module ignored;");

        let recovery_before = fs::read(&recovery)
            .unwrap_or_else(|error| panic!("recovery fixture must be readable: {error}"));

        assert!(check_workspace(workspace.path()).is_err());

        write_workspace(workspace.path())
            .unwrap_or_else(|error| panic!("maintained source must format: {error}"));

        check_workspace(workspace.path())
            .unwrap_or_else(|error| panic!("formatted source must pass check mode: {error}"));

        assert_eq!(
            fs::read(&recovery)
                .unwrap_or_else(|error| panic!("recovery fixture must remain readable: {error}")),
            recovery_before
        );

        let maintained = maintained_sources(workspace.path())
            .unwrap_or_else(|error| panic!("maintained sources must be discoverable: {error}"));

        assert_eq!(maintained, vec![example, recovery]);

        assert_eq!(
            fs::read_to_string(&ignored)
                .unwrap_or_else(|error| panic!("ignored source must remain readable: {error}")),
            "module ignored;"
        );
    }

    fn write_source(path: &Path, source: &str) {
        let parent = path
            .parent()
            .unwrap_or_else(|| panic!("test source must have a parent"));

        fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("test source parent must be created: {error}"));

        fs::write(path, source)
            .unwrap_or_else(|error| panic!("test source must be written: {error}"));
    }
}
