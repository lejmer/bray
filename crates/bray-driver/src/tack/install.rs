use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_diagnostics::DiagnosticBag;
use bray_platform::{
    NativeExitStatus, NativeProcessCommand,
};
use bray_project::ProjectPath;

use crate::tack::error::{
    operation_diagnostics, selection_diagnostics,
};

pub(crate) fn install_git_repository(
    workspace_root: &Path,
    name: &str,
    repository: &str,
) -> Result<(), DiagnosticBag> {
    let plan = GitInstallPlan::new(workspace_root, name, repository)?;

    let parent = plan
        .destination()
        .parent()
        .ok_or_else(|| operation_diagnostics("vendor_destination_parent"))?;

    std::fs::create_dir_all(parent)
        .map_err(|_| operation_diagnostics("create_vendor_directory"))?;

    let status = native_process_status(
        OsStr::new("git"),
        &plan.arguments(),
        workspace_root,
        "git_clone",
    )?;

    if !status.success() {
        return Err(operation_diagnostics("git_clone_status"));
    }

    Ok(())
}

pub(crate) fn run_project_process(
    program: &Path,
    arguments: &[OsString],
    workspace_root: &Path,
) -> Result<ExitCode, DiagnosticBag> {
    let arguments: Vec<_> = arguments
        .iter()
        .map(OsString::as_os_str)
        .collect();

    let status = native_process_status(
        program.as_os_str(),
        &arguments,
        workspace_root,
        "project_process",
    )?;

    if status.success() {
        return Ok(ExitCode::SUCCESS);
    }

    let code = match status.code().and_then(|code| u8::try_from(code).ok()) {
        Some(code) => code,
        None => 1,
    };

    Ok(ExitCode::from(code))
}

fn native_process_status(
    program: &OsStr,
    arguments: &[&OsStr],
    working_directory: &Path,
    operation: &'static str,
) -> Result<NativeExitStatus, DiagnosticBag> {
    let mut command = NativeProcessCommand::new(program)
        .map_err(|_| operation_diagnostics(operation))?;

    for argument in arguments {
        command.arg(argument);
    }

    command.current_dir(working_directory);

    let mut child = command
        .spawn()
        .map_err(|_| operation_diagnostics(operation))?;

    child
        .wait()
        .map_err(|_| operation_diagnostics(operation))
}

#[derive(Debug, Eq, PartialEq)]
struct GitInstallPlan {
    repository: OsString,
    destination: PathBuf,
}

impl GitInstallPlan {
    fn new(
        workspace_root: &Path,
        name: &str,
        repository: &str,
    ) -> Result<Self, DiagnosticBag> {
        if name.contains('/') || repository.is_empty() {
            return Err(selection_diagnostics(name));
        }

        let portable = ProjectPath::try_relative(format!("vendor/{name}"))
            .ok_or_else(|| selection_diagnostics(name))?;

        let destination = portable.beneath(workspace_root);

        if destination.exists() {
            return Err(selection_diagnostics(name));
        }

        Ok(Self {
            repository: repository.into(),
            destination,
        })
    }

    fn destination(&self) -> &Path {
        &self.destination
    }

    fn arguments(&self) -> [&OsStr; 5] {
        [
            OsStr::new("clone"),
            OsStr::new("--no-recurse-submodules"),
            OsStr::new("--"),
            &self.repository,
            self.destination.as_os_str(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::GitInstallPlan;
    use crate::test_support::unique_temporary_directory;

    #[test]
    fn explicit_install_disables_recursive_repository_acquisition() {
        let workspace = unique_temporary_directory();

        let plan = GitInstallPlan::new(
            &workspace,
            "math",
            "https://example.invalid/math.git",
        )
        .unwrap_or_else(|error| panic!("install plan should be valid: {error:?}"));

        assert_eq!(
            plan.arguments(),
            [
                OsStr::new("clone"),
                OsStr::new("--no-recurse-submodules"),
                OsStr::new("--"),
                OsStr::new("https://example.invalid/math.git"),
                workspace.join("vendor").join("math").as_os_str(),
            ]
        );
    }

    #[test]
    fn explicit_install_rejects_ambient_or_existing_destinations() {
        let workspace = unique_temporary_directory();

        assert!(GitInstallPlan::new(&workspace, "../math", "repository").is_err());
        assert!(GitInstallPlan::new(&workspace, "math/tools", "repository").is_err());
        assert!(GitInstallPlan::new(&workspace, "math", "").is_err());
    }
}
