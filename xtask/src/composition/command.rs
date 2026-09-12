use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use bray_target::NativeTarget;

use super::report;

pub(super) const CASES: &[&str] = &[
    "compiler-request",
    "synchronous",
    "standard-library",
    "runtime-role",
    "cleanup",
    "selected-entry",
    "typed-error",
];
const USAGE: &str =
    "usage: cargo xtask composition [--case <name>] [--no-build] [--output <directory>]";

pub(crate) fn run(arguments: impl Iterator<Item = String>) -> ExitCode {
    match Options::parse(arguments).and_then(execute) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");

            ExitCode::FAILURE
        }
    }
}

struct Options {
    case: Option<String>,
    no_build: bool,
    output: Option<PathBuf>,
}

impl Options {
    fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            case: None,
            no_build: false,
            output: None,
        };

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--case" if options.case.is_none() => {
                    let case = arguments.next().ok_or("--case requires a name")?;

                    if !CASES.contains(&case.as_str()) {
                        return Err(format!(
                            "unknown composition case {case}. Available: {}",
                            CASES.join(", ")
                        ));
                    }

                    options.case = Some(case);
                }
                "--no-build" if !options.no_build => options.no_build = true,
                "--output" if options.output.is_none() => {
                    options.output = Some(
                        arguments
                            .next()
                            .ok_or("--output requires a directory")?
                            .into(),
                    )
                }
                _ => return Err(format!("unexpected composition argument {argument}")),
            }
        }

        Ok(options)
    }
}

fn execute(options: Options) -> Result<(), String> {
    let root = crate::workspace::root()?;
    let target = NativeTarget::current().ok_or("composition requires a supported native host")?;

    let directory = options
        .output
        .unwrap_or_else(|| crate::workspace::cargo_target(&root).join("composition"));

    let directory = std::path::absolute(&directory)
        .map_err(|error| crate::workspace::io_error("resolve", &directory, error))?;

    let workspace = directory.join("workspace");
    let toolchain = directory.join("toolchain");

    fs::create_dir_all(&directory)
        .map_err(|error| crate::workspace::io_error("create", &directory, error))?;

    let lock_path = directory.join("composition.lock");

    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| crate::workspace::io_error("open", &lock_path, error))?;

    lock.try_lock().map_err(|error| {
        format!(
            "composition output is in use at {}: {error}",
            directory.display()
        )
    })?;

    crate::progress::message(&format!(
        "Retaining composition products in {}",
        directory.display()
    ));

    let result = (|| {
        if !options.no_build {
            crate::native_toolchain::build_compiler(&root)?;

            let runtime = crate::progress::run("Preparing composition runtime", || {
                crate::runtime_artifact::build_for_readiness(target, &directory.join("runtime"))
            })?;

            crate::native_toolchain::assemble(&root, target, &runtime, &toolchain)?;
        }

        prepare_workspace(&root, &workspace, target)?;

        for case in CASES.iter().copied().filter(|case| {
            options
                .case
                .as_deref()
                .is_none_or(|selected| selected == *case)
        }) {
            crate::progress::run(&format!("Composition {case}"), || {
                run_case(
                    &root,
                    &workspace,
                    &toolchain,
                    &directory,
                    case,
                    options.no_build,
                )
            })?;
        }

        Ok(())
    })();

    result.map_err(|error: String| {
        format!(
            "{error}\n{}",
            report::evidence(&root, &workspace, &toolchain)
        )
    })
}

fn prepare_workspace(root: &Path, workspace: &Path, target: NativeTarget) -> Result<(), String> {
    let fixtures = root.join("xtask/fixtures/composition");
    crate::native_toolchain::copy_directory(&fixtures, workspace)?;

    for source_root in std::iter::once("library").chain(CASES.iter().copied()) {
        remove_obsolete_sources(&fixtures.join(source_root), &workspace.join(source_root))?;
    }

    let manifest = workspace.join("bray-workspace.json");

    let bytes = fs::read(&manifest)
        .map_err(|error| crate::workspace::io_error("read", &manifest, error))?;

    let mut value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: {error}", manifest.display()))?;

    value["targets"][0]["identity"] = target.as_str().into();
    crate::json::write_pretty(&manifest, &value)?;

    let fragments = workspace.join("compiler-request/source-files-in-a-deliberately-long-workspace-path-to-exercise-native-request-transport");

    fs::create_dir_all(&fragments)
        .map_err(|error| crate::workspace::io_error("create", &fragments, error))?;

    for index in 0..256 {
        let path = fragments.join(format!("ordered-source-contribution-{index:03}.bray"));

        fs::write(&path, "module composition.compiler_request;\n")
            .map_err(|error| crate::workspace::io_error("write", &path, error))?;
    }

    Ok(())
}

fn remove_obsolete_sources(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in fs::read_dir(destination)
        .map_err(|error| crate::workspace::io_error("read", destination, error))?
    {
        let entry =
            entry.map_err(|error| crate::workspace::io_error("read", destination, error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| crate::workspace::io_error("inspect", &entry.path(), error))?;
        let original = source.join(entry.file_name());
        let copied = entry.path();

        if file_type.is_symlink() {
            return Err(format!(
                "composition source cannot be a symbolic link: {}",
                copied.display()
            ));
        }

        if file_type.is_dir() {
            remove_obsolete_sources(&original, &copied)?;
        } else if copied
            .extension()
            .is_some_and(|extension| extension == "bray")
            && !original.is_file()
        {
            fs::remove_file(&copied).map_err(|error| {
                crate::workspace::io_error("remove obsolete fixture", &copied, error)
            })?;
        }
    }

    Ok(())
}

fn run_case(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    directory: &Path,
    case: &str,
    no_build: bool,
) -> Result<(), String> {
    let first = invoke(root, workspace, toolchain, directory, case, no_build)?;

    if !no_build {
        let rerun = invoke(root, workspace, toolchain, directory, case, true)?;
        report::same_generation(&first, &rerun)?;

        if case == "synchronous" {
            crate::progress::run("Checking retained identity rejection", || {
                super::identity::audit(root, workspace, toolchain, case)
            })?;

            crate::progress::run("Checking runtime ABI and role rejection", || {
                super::identity::audit_runtime(root, workspace, toolchain, case)
            })?;
            let restored = invoke(root, workspace, toolchain, directory, case, false)?;
            let restored_rerun = invoke(root, workspace, toolchain, directory, case, true)?;
            report::same_generation(&restored, &restored_rerun)?;
        }
    }

    Ok(())
}

fn invoke(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    directory: &Path,
    case: &str,
    no_build: bool,
) -> Result<crate::native_test_report::NativeTestReport, String> {
    let mut command = test_command(root, workspace, toolchain, case, no_build);

    let output = crate::command::output_with_streamed_stderr(&mut command)
        .map_err(|error| format!("composition {case}: compiler command launch: {error}"))?;

    let report_path = directory.join(format!(
        "{case}-{}.json",
        if no_build { "rerun" } else { "build" }
    ));

    fs::write(&report_path, &output.stdout)
        .map_err(|error| crate::workspace::io_error("write", &report_path, error))?;

    report::validate(&output, workspace, case, no_build)
}

pub(super) fn test_command(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    case: &str,
    no_build: bool,
) -> Command {
    let mut command = Command::new(crate::native_toolchain::compiler_executable(root, "bray"));

    command
        .current_dir(workspace)
        .arg("--workspace")
        .arg(workspace)
        .arg("--toolchain-root")
        .arg(toolchain)
        .args([
            "--format",
            "json",
            "test",
            "--release",
            "--target",
            "native",
            "--product",
            case,
            "run",
            "--jobs",
            "1",
            "--timeout-ms",
            "10000",
        ]);

    if no_build {
        command.arg("--no-build");
    }

    command
}

#[cfg(test)]
mod tests {
    use super::Options;

    #[test]
    fn selectors_reject_typos_and_duplicate_options() {
        for arguments in [
            vec!["--case", "typo"],
            vec!["--case"],
            vec!["--no-build", "--no-build"],
        ] {
            assert!(Options::parse(arguments.into_iter().map(str::to_owned)).is_err());
        }

        assert!(
            Options::parse(
                ["--case", "cleanup", "--no-build"]
                    .into_iter()
                    .map(str::to_owned)
            )
            .is_ok()
        );
    }
}
