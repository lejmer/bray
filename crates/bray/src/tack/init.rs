use std::io::{self, Write};
use std::path::Path;

use bray_base::{FileReplacementMode, StagedFile};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_project::{
    PACKAGE_MANIFEST_FILE_NAME, ProjectLoadError, WORKSPACE_MANIFEST_FILE_NAME,
    canonicalize_package_manifest, canonicalize_workspace_manifest,
    is_valid_ordinary_package_identity,
};
use bray_target::NativeTarget;
use serde::Serialize;

const SOURCE_DIRECTORY_NAME: &str = "src";
const ENTRYPOINT_FILE_NAME: &str = "main.bray";
const STARTER_SOURCE: &str = "module app;\n\nfunc main()\n{\n}\n";

#[derive(Serialize)]
struct WorkspaceManifest<'a> {
    format: u32,
    package: WorkspacePackageMetadata,
    output_root: &'static str,
    targets: [WorkspaceTarget<'a>; 1],
    packages: [WorkspacePackage; 1],
}

#[derive(Serialize)]
struct WorkspacePackageMetadata {
    version: &'static str,
}

#[derive(Serialize)]
struct WorkspaceTarget<'a> {
    name: &'static str,
    identity: &'a str,
}

#[derive(Serialize)]
struct WorkspacePackage {
    path: &'static str,
    role: &'static str,
    features: [&'static str; 0],
}

#[derive(Serialize)]
struct PackageManifest<'a> {
    format: u32,
    identity: &'a str,
    version: InheritedPackageVersion,
    features: [&'static str; 0],
    source_roots: [SourceRoot; 1],
    products: [Product; 1],
}

#[derive(Serialize)]
struct InheritedPackageVersion {
    workspace: bool,
}

#[derive(Serialize)]
struct SourceRoot {
    name: &'static str,
    path: &'static str,
}

#[derive(Serialize)]
struct Product {
    name: &'static str,
    kind: &'static str,
    source_roots: [&'static str; 1],
    targets: [&'static str; 1],
    dependencies: [&'static str; 0],
    outputs: [&'static str; 1],
}

pub(super) fn initialize_project(
    workspace_root: &Path,
    package: Option<&str>,
) -> Result<(), DiagnosticBag> {
    initialize_project_for_target(workspace_root, package, NativeTarget::current())
}

fn initialize_project_for_target(
    workspace_root: &Path,
    package: Option<&str>,
    target: Option<NativeTarget>,
) -> Result<(), DiagnosticBag> {
    let package = package
        .map(str::to_owned)
        .or_else(|| derive_package_identity(workspace_root))
        .ok_or_else(|| invalid_identity(workspace_root.display().to_string()))?;

    if !is_valid_ordinary_package_identity(&package) {
        return Err(invalid_identity(package));
    }

    let Some(target) = target else {
        return Err(target_unsupported());
    };

    let workspace_manifest_path = workspace_root.join(WORKSPACE_MANIFEST_FILE_NAME);
    let package_manifest_path = workspace_root.join(PACKAGE_MANIFEST_FILE_NAME);
    let source_directory = workspace_root.join(SOURCE_DIRECTORY_NAME);
    let entrypoint_path = source_directory.join(ENTRYPOINT_FILE_NAME);

    require_directory_or_absent(workspace_root)?;
    require_directory_or_absent(&source_directory)?;

    for path in [
        &workspace_manifest_path,
        &package_manifest_path,
        &entrypoint_path,
    ] {
        require_absent(path)?;
    }

    let workspace_manifest = WorkspaceManifest {
        format: 1,
        package: WorkspacePackageMetadata { version: "0.1.0" },
        output_root: "build",
        targets: [WorkspaceTarget {
            name: "native",
            identity: target.as_str(),
        }],
        packages: [WorkspacePackage {
            path: ".",
            role: "root",
            features: [],
        }],
    };

    let package_manifest = PackageManifest {
        format: 1,
        identity: &package,
        version: InheritedPackageVersion { workspace: true },
        features: [],
        source_roots: [SourceRoot {
            name: "main",
            path: SOURCE_DIRECTORY_NAME,
        }],
        products: [Product {
            name: "application",
            kind: "executable",
            source_roots: ["main"],
            targets: ["native"],
            dependencies: [],
            outputs: ["executable"],
        }],
    };

    let workspace_bytes = serialize_manifest(
        &workspace_manifest,
        &workspace_manifest_path,
        canonicalize_workspace_manifest,
    )?;

    let package_bytes = serialize_manifest(
        &package_manifest,
        &package_manifest_path,
        canonicalize_package_manifest,
    )?;

    create_directory(workspace_root)?;
    create_directory(&source_directory)?;

    write_new_file(&workspace_manifest_path, &workspace_bytes)?;
    write_new_file(&package_manifest_path, &package_bytes)?;

    write_new_file(&entrypoint_path, STARTER_SOURCE.as_bytes())
}

fn derive_package_identity(workspace_root: &Path) -> Option<String> {
    let directory_name = workspace_root.file_name()?.to_string_lossy();

    Some(directory_name.into_owned())
}

fn serialize_manifest(
    manifest: &impl Serialize,
    path: &Path,
    canonicalize: fn(&str, &Path) -> Result<String, ProjectLoadError>,
) -> Result<Vec<u8>, DiagnosticBag> {
    let source = serde_json::to_string(manifest)
        .map_err(|_| write_failed(path, io::ErrorKind::InvalidData))?;

    let source =
        canonicalize(&source, path).map_err(|_| write_failed(path, io::ErrorKind::InvalidData))?;

    Ok(source.into_bytes())
}

fn require_directory_or_absent(path: &Path) -> Result<(), DiagnosticBag> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(path_conflict(path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(write_failed(path, error.kind())),
    }
}

fn require_absent(path: &Path) -> Result<(), DiagnosticBag> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(path_conflict(path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(write_failed(path, error.kind())),
    }
}

fn create_directory(path: &Path) -> Result<(), DiagnosticBag> {
    std::fs::create_dir_all(path).map_err(|error| write_failed(path, error.kind()))
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), DiagnosticBag> {
    let mut staging = StagedFile::create(path, FileReplacementMode::RequireAbsent, None)
        .map_err(|error| publication_failed(path, error.kind()))?;

    staging
        .write_all(bytes)
        .map_err(|error| write_failed(path, error.kind()))?;

    staging
        .finish()
        .and_then(|staged| staged.promote(path))
        .map_err(|error| publication_failed(path, error.kind()))
}

fn publication_failed(path: &Path, kind: io::ErrorKind) -> DiagnosticBag {
    if kind == io::ErrorKind::AlreadyExists {
        path_conflict(path)
    } else {
        write_failed(path, kind)
    }
}

fn invalid_identity(identity: impl Into<String>) -> DiagnosticBag {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::ProjectInitializationIdentityInvalid,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::referenced_name(identity))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::PackageIdentityMustBeValid,
    ));

    DiagnosticBag::single(diagnostic)
}

fn path_conflict(path: &Path) -> DiagnosticBag {
    diagnostic(
        DiagnosticKind::ProjectInitializationPathConflict,
        [DiagnosticArg::file_path(path)],
    )
}

fn write_failed(path: &Path, kind: io::ErrorKind) -> DiagnosticBag {
    diagnostic(
        DiagnosticKind::ProjectInitializationWriteFailed,
        [
            DiagnosticArg::file_path(path),
            DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(kind)),
        ],
    )
}

fn target_unsupported() -> DiagnosticBag {
    diagnostic(DiagnosticKind::ProjectInitializationTargetUnsupported, [])
}

fn diagnostic<const ARGUMENT_COUNT: usize>(
    kind: DiagnosticKind,
    arguments: [DiagnosticArg; ARGUMENT_COUNT],
) -> DiagnosticBag {
    let diagnostic = arguments.into_iter().fold(
        Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error),
        Diagnostic::with_arg,
    );

    DiagnosticBag::single(diagnostic)
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticArgName, DiagnosticKind, DiagnosticNote, DiagnosticNoteKind};
    use bray_project::load_project_graph;
    use bray_target::NativeTarget;

    use super::{initialize_project, initialize_project_for_target};
    use crate::test_support::unique_temporary_directory;

    #[test]
    fn initialization_produces_a_loadable_root_package() {
        let parent = unique_temporary_directory();
        let workspace = parent.join("hello_world");

        initialize_project(&workspace, None)
            .unwrap_or_else(|diagnostics| panic!("project should initialize: {diagnostics:?}"));

        let graph = load_project_graph(&workspace)
            .unwrap_or_else(|error| panic!("generated project should load: {error:?}"));

        let [package] = graph.packages() else {
            panic!("generated project should have one package");
        };

        assert_eq!(package.identity().as_str(), "hello_world");
        assert_eq!(package.version().to_string(), "0.1.0");
        assert_eq!(package.path().as_str(), ".");
        assert_eq!(package.products().len(), 1);

        let source = std::fs::read_to_string(workspace.join("src/main.bray"))
            .unwrap_or_else(|error| panic!("starter source should be readable: {error:?}"));

        assert_eq!(source, "module app;\n\nfunc main()\n{\n}\n");

        let _ = std::fs::remove_dir_all(parent);
    }

    #[test]
    fn initialization_refuses_to_replace_existing_source() {
        let parent = unique_temporary_directory();
        let workspace = parent.join("sample-project");

        initialize_project(&workspace, Some("example.application"))
            .unwrap_or_else(|diagnostics| panic!("project should initialize: {diagnostics:?}"));

        let entrypoint = workspace.join("src/main.bray");

        std::fs::write(&entrypoint, "preserve me")
            .unwrap_or_else(|error| panic!("test source should be writable: {error:?}"));

        let Err(diagnostics) = initialize_project(&workspace, Some("example.application")) else {
            panic!("existing project files should be rejected");
        };

        assert_eq!(
            diagnostics
                .by_kind(DiagnosticKind::ProjectInitializationPathConflict)
                .count(),
            1
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::ProjectInitializationPathConflict,
        );

        assert_eq!(
            std::fs::read_to_string(entrypoint)
                .unwrap_or_else(|error| panic!("test source should be readable: {error:?}")),
            "preserve me"
        );

        let _ = std::fs::remove_dir_all(parent);
    }

    #[test]
    fn invalid_explicit_identity_does_not_create_a_workspace() {
        let parent = unique_temporary_directory();
        let workspace = parent.join("sample-project");

        let Err(diagnostics) = initialize_project(&workspace, Some("Invalid Package")) else {
            panic!("invalid package identity should be rejected");
        };

        let Some(diagnostic) = diagnostics
            .by_kind(DiagnosticKind::ProjectInitializationIdentityInvalid)
            .next()
        else {
            panic!("invalid package identity should produce a diagnostic: {diagnostics:?}");
        };

        assert_eq!(
            diagnostic.notes(),
            &[DiagnosticNote::new(
                DiagnosticNoteKind::PackageIdentityMustBeValid
            )]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::ProjectInitializationIdentityInvalid,
        );

        assert!(!workspace.exists());
    }

    #[test]
    fn initialization_reports_unsupported_hosts_through_the_target_boundary() {
        let workspace = unique_temporary_directory().join("unsupported-project");

        let Err(diagnostics) =
            initialize_project_for_target(&workspace, Some("example.application"), None)
        else {
            panic!("missing native target must reject initialization");
        };

        let diagnostic = bray_testing::single_diagnostic(&diagnostics);

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::ProjectInitializationTargetUnsupported
        );

        assert!(diagnostic.args().is_empty());

        bray_testing::assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::ProjectInitializationTargetUnsupported,
        );

        assert!(!workspace.exists());
    }

    #[test]
    fn initialization_reports_filesystem_failures_with_path_and_error_kind() {
        let parent = unique_temporary_directory();
        let blocking_file = parent.join("blocking-file");

        std::fs::create_dir_all(&parent)
            .unwrap_or_else(|error| panic!("test parent must be created: {error:?}"));

        std::fs::write(&blocking_file, b"not a directory")
            .unwrap_or_else(|error| panic!("blocking file must be written: {error:?}"));

        let workspace = blocking_file.join("project");

        let Err(diagnostics) = initialize_project_for_target(
            &workspace,
            Some("example.application"),
            Some(NativeTarget::X86_64LinuxGnu),
        ) else {
            panic!("filesystem failure must reject initialization");
        };

        let diagnostic = bray_testing::single_diagnostic(&diagnostics);

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::ProjectInitializationWriteFailed
        );

        assert_eq!(
            diagnostic
                .args()
                .iter()
                .map(|argument| argument.name())
                .collect::<Vec<_>>(),
            [DiagnosticArgName::FilePath, DiagnosticArgName::IoErrorKind]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::ProjectInitializationWriteFailed,
        );

        let _ = std::fs::remove_dir_all(parent);
    }
}
