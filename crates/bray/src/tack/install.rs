use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_diagnostics::DiagnosticBag;
use bray_platform::{NativeExitStatus, NativeProcessCommand};
use bray_project::ProjectPath;

use crate::tack::error::{operation_diagnostics, selection_diagnostics};

pub(crate) fn install_git_repository(
    workspace_root: &Path,
    name: &str,
    repository: &str,
) -> Result<(), DiagnosticBag> {
    let plan = GitInstallPlan::new(workspace_root, name, repository)?;

    prepare_install_destination(&plan)?;

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

fn prepare_install_destination(plan: &GitInstallPlan) -> Result<(), DiagnosticBag> {
    require_owned_directory(plan.vendor_directory())?;

    require_missing_path(plan.destination(), plan.name())
}

fn require_owned_directory(path: &Path) -> Result<(), DiagnosticBag> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(operation_diagnostics("vendor_directory")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            std::fs::create_dir(path)
                .map_err(|_| operation_diagnostics("create_vendor_directory"))?;

            let metadata = std::fs::symlink_metadata(path)
                .map_err(|_| operation_diagnostics("vendor_directory"))?;

            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                Ok(())
            } else {
                Err(operation_diagnostics("vendor_directory"))
            }
        }
        Err(_) => Err(operation_diagnostics("vendor_directory")),
    }
}

fn require_missing_path(path: &Path, name: &str) -> Result<(), DiagnosticBag> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) | Err(_) => Err(selection_diagnostics(name)),
    }
}

pub(crate) fn run_project_process(
    program: &Path,
    arguments: &[OsString],
    workspace_root: &Path,
) -> Result<ExitCode, DiagnosticBag> {
    let arguments: Vec<_> = arguments.iter().map(OsString::as_os_str).collect();

    let status = native_process_status(
        program.as_os_str(),
        &arguments,
        workspace_root,
        "project_process",
    )?;

    if status.success() {
        return Ok(ExitCode::SUCCESS);
    }

    let code = status.code().and_then(|code| u8::try_from(code).ok()).unwrap_or_else(|| 1);

    Ok(ExitCode::from(code))
}

fn native_process_status(
    program: &OsStr,
    arguments: &[&OsStr],
    working_directory: &Path,
    operation: &'static str,
) -> Result<NativeExitStatus, DiagnosticBag> {
    let mut command =
        NativeProcessCommand::new(program).map_err(|_| operation_diagnostics(operation))?;

    for argument in arguments {
        command.arg(argument);
    }

    command.current_dir(working_directory);

    let mut child = command
        .spawn()
        .map_err(|_| operation_diagnostics(operation))?;

    child.wait().map_err(|_| operation_diagnostics(operation))
}

#[derive(Debug, Eq, PartialEq)]
struct GitInstallPlan {
    name: String,
    repository: OsString,
    vendor_directory: PathBuf,
    destination: PathBuf,
}

impl GitInstallPlan {
    fn new(workspace_root: &Path, name: &str, repository: &str) -> Result<Self, DiagnosticBag> {
        if !workspace_root.is_absolute() || name.contains('/') || repository.is_empty() {
            return Err(selection_diagnostics(name));
        }

        let portable = ProjectPath::try_relative(format!("vendor/{name}"))
            .ok_or_else(|| selection_diagnostics(name))?;

        let destination = portable.beneath(workspace_root);
        let vendor_directory = workspace_root.join("vendor");

        Ok(Self {
            name: name.to_owned(),
            repository: repository.into(),
            vendor_directory,
            destination,
        })
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn vendor_directory(&self) -> &Path {
        &self.vendor_directory
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
    use std::path::{Path, PathBuf};

    use super::{GitInstallPlan, prepare_install_destination};
    use crate::test_support::unique_temporary_directory;

    #[cfg(unix)]
    fn create_vendor_symlink(workspace: &Path, external: &Path) -> bool {
        std::os::unix::fs::symlink(external, workspace.join("vendor"))
            .unwrap_or_else(|error| panic!("test vendor symlink should be created: {error:?}"));

        true
    }

    #[cfg(windows)]
    fn create_vendor_symlink(workspace: &Path, external: &Path) -> bool {
        match std::os::windows::fs::symlink_dir(external, workspace.join("vendor")) {
            Ok(()) => true,
            Err(_) => false,
        }
    }

    #[cfg(unix)]
    fn remove_vendor_symlink(path: PathBuf) {
        std::fs::remove_file(path)
            .unwrap_or_else(|error| panic!("test vendor symlink should be removed: {error:?}"));
    }

    #[cfg(windows)]
    fn remove_vendor_symlink(path: PathBuf) {
        std::fs::remove_dir(path)
            .unwrap_or_else(|error| panic!("test vendor symlink should be removed: {error:?}"));
    }

    #[test]
    fn explicit_install_disables_recursive_repository_acquisition() {
        let workspace = unique_temporary_directory();

        let plan = GitInstallPlan::new(&workspace, "math", "https://example.invalid/math.git")
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

        assert!(GitInstallPlan::new(Path::new("project"), "math", "repository").is_err());
        assert!(GitInstallPlan::new(&workspace, "../math", "repository").is_err());
        assert!(GitInstallPlan::new(&workspace, "math/tools", "repository").is_err());
        assert!(GitInstallPlan::new(&workspace, "math", "").is_err());

        std::fs::create_dir(&workspace)
            .unwrap_or_else(|error| panic!("test workspace should be created: {error:?}"));

        std::fs::create_dir(workspace.join("vendor"))
            .unwrap_or_else(|error| panic!("test vendor directory should be created: {error:?}"));

        std::fs::create_dir(workspace.join("vendor").join("math"))
            .unwrap_or_else(|error| panic!("test destination should be created: {error:?}"));

        let plan = GitInstallPlan::new(&workspace, "math", "repository")
            .unwrap_or_else(|error| panic!("install plan should be valid: {error:?}"));

        assert!(prepare_install_destination(&plan).is_err());

        std::fs::remove_dir(workspace.join("vendor").join("math"))
            .unwrap_or_else(|error| panic!("test destination should be removed: {error:?}"));

        std::fs::remove_dir(workspace.join("vendor"))
            .unwrap_or_else(|error| panic!("test vendor directory should be removed: {error:?}"));

        std::fs::remove_dir(&workspace)
            .unwrap_or_else(|error| panic!("test workspace should be removed: {error:?}"));
    }

    #[test]
    fn explicit_install_rejects_symlinked_vendor_ancestors() {
        let workspace = unique_temporary_directory();
        let external = workspace.with_extension("external");

        std::fs::create_dir(&workspace)
            .unwrap_or_else(|error| panic!("test workspace should be created: {error:?}"));

        std::fs::create_dir(&external)
            .unwrap_or_else(|error| panic!("external directory should be created: {error:?}"));

        if !create_vendor_symlink(&workspace, &external) {
            std::fs::remove_dir(&external)
                .unwrap_or_else(|error| panic!("external directory should be removed: {error:?}"));

            std::fs::remove_dir(&workspace)
                .unwrap_or_else(|error| panic!("test workspace should be removed: {error:?}"));

            return;
        }

        let plan = GitInstallPlan::new(&workspace, "math", "repository")
            .unwrap_or_else(|error| panic!("install plan should be valid: {error:?}"));

        assert!(prepare_install_destination(&plan).is_err());
        assert!(!external.join("math").exists());

        remove_vendor_symlink(workspace.join("vendor"));

        std::fs::remove_dir(&external)
            .unwrap_or_else(|error| panic!("external directory should be removed: {error:?}"));

        std::fs::remove_dir(&workspace)
            .unwrap_or_else(|error| panic!("test workspace should be removed: {error:?}"));
    }
}
