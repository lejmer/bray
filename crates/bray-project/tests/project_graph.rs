use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use bray_diagnostics::{DiagnosticId, DiagnosticKind};
use bray_project::{
    PackageRole, ProjectGraph, ProjectLoadError, ProjectManifestProblem, load_project_graph,
};
use bray_symbols::PackageIdentity;
use bray_target::TargetOutputKind;

static TEST_DIRECTORY_ORDINAL: AtomicUsize = AtomicUsize::new(0);

#[test]
fn project_graphs_are_safe_to_share_between_workers() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<ProjectGraph>();
}

#[test]
fn manifests_load_an_exact_dependency_first_project_graph() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    let graph = load_project_graph(workspace.path())
        .unwrap_or_else(|error| panic!("valid workspace must load: {error:?}"));

    assert_eq!(graph.output_root().as_str(), "build");

    assert_eq!(
        graph
            .targets()
            .iter()
            .map(|target| (target.name(), target.identity().as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("native", "x86_64-unknown-linux-gnu"),
            ("portable", "wasm32-unknown-unknown")
        ]
    );

    assert_eq!(
        graph
            .packages()
            .iter()
            .map(|package| package.identity().as_str())
            .collect::<Vec<_>>(),
        vec!["example.math", "example.application"]
    );

    let application = graph
        .packages()
        .get(1)
        .unwrap_or_else(|| panic!("application package must be present"));

    assert_eq!(application.role(), PackageRole::Root);

    assert_eq!(
        application
            .enabled_features()
            .iter()
            .map(|feature| feature.as_str())
            .collect::<Vec<_>>(),
        vec!["logging", "tracing"]
    );

    assert_eq!(
        application.products()[0]
            .sources()
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>(),
        vec!["app/src/main.bray", "app/src/nested/value.bray"]
    );

    assert_eq!(
        application.products()[0].outputs(),
        &[
            TargetOutputKind::DependencyMetadata,
            TargetOutputKind::Executable
        ]
    );

    assert_eq!(
        application.dependencies()[0].product().package().as_str(),
        "example.math"
    );

    assert_eq!(application.dependencies()[0].product().name(), "math");

    let identity = PackageIdentity::try_new("example.math")
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    assert_eq!(
        graph.package(&identity).map(|package| package.role()),
        Some(PackageRole::Vendored)
    );
}

#[test]
fn manifest_array_order_does_not_change_the_graph() {
    let first = TestWorkspace::new();
    let second = TestWorkspace::new();
    write_valid_workspace(first.path(), false);
    write_valid_workspace(second.path(), true);

    let first = load_project_graph(first.path())
        .unwrap_or_else(|error| panic!("first workspace must load: {error:?}"));

    let second = load_project_graph(second.path())
        .unwrap_or_else(|error| panic!("second workspace must load: {error:?}"));

    assert_eq!(first, second);
}

#[test]
fn unknown_manifest_fields_are_rejected_by_the_narrow_schema() {
    let workspace = TestWorkspace::new();

    write_file(
        workspace.path().join("bray-workspace.json"),
        r#"{
            "format": 1,
            "output_root": "build",
            "targets": [],
            "packages": [],
            "registry": "https://example.invalid"
        }"#,
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::ParseManifest { .. })
    ));
}

#[test]
fn workspace_features_must_be_declared_by_the_selected_package() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("bray-workspace.json"),
        r#""features": ["tracing", "logging"]"#,
        r#""features": ["network"]"#,
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::InvalidManifest {
            problem: ProjectManifestProblem::UndeclaredFeature,
            ..
        })
    ));
}

#[test]
fn dependency_edges_must_select_declared_library_products() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""product": "math""#,
        r#""product": "missing""#,
    );

    let Err(error) = load_project_graph(workspace.path()) else {
        panic!("unknown dependency product must reject the graph");
    };

    let diagnostic = error.into_diagnostic(DiagnosticId::new(7));

    assert_eq!(
        diagnostic.kind(),
        DiagnosticKind::ProjectDependencyProductInvalid
    );
}

#[test]
fn dependency_cycles_are_reported_deterministically() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace
            .path()
            .join("vendor")
            .join("math")
            .join("bray-package.json"),
        r#""dependencies": []"#,
        r#""dependencies": [{"package": "example.application", "product": "application"}]"#,
    );

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""kind": "executable""#,
        r#""kind": "library""#,
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::InvalidManifest {
            problem: ProjectManifestProblem::DependencyCycle,
            value: package,
            ..
        }) if package == "example.application"
    ));
}

#[test]
fn output_and_source_roots_cannot_overlap() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("bray-workspace.json"),
        r#""output_root": "build""#,
        r#""output_root": "app/src/generated""#,
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::InvalidManifest {
            problem: ProjectManifestProblem::InvalidSourceRoot,
            ..
        })
    ));
}

#[test]
fn package_source_roots_cannot_overlap_each_other() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""source_roots": [{"name": "main", "path": "src"}]"#,
        r#""source_roots": [
            {"name": "main", "path": "src"},
            {"name": "nested", "path": "src/nested"}
        ]"#,
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::InvalidManifest {
            problem: ProjectManifestProblem::InvalidSourceRoot,
            ..
        })
    ));
}

#[test]
fn target_names_cannot_alias_one_target_identity() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("bray-workspace.json"),
        r#""identity": "wasm32-unknown-unknown""#,
        r#""identity": "x86_64-unknown-linux-gnu""#,
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::InvalidManifest {
            problem: ProjectManifestProblem::DuplicateSelection,
            ..
        })
    ));
}

fn write_valid_workspace(root: &Path, reversed: bool) {
    let targets = if reversed {
        r#"[
            {"name": "native", "identity": "x86_64-unknown-linux-gnu"},
            {"name": "portable", "identity": "wasm32-unknown-unknown"}
        ]"#
    } else {
        r#"[
            {"name": "portable", "identity": "wasm32-unknown-unknown"},
            {"name": "native", "identity": "x86_64-unknown-linux-gnu"}
        ]"#
    };

    let packages = if reversed {
        r#"[
            {"path": "vendor/math", "role": "vendored", "features": ["simd"]},
            {"path": "app", "role": "root", "features": ["logging", "tracing"]}
        ]"#
    } else {
        r#"[
            {"path": "app", "role": "root", "features": ["tracing", "logging"]},
            {"path": "vendor/math", "role": "vendored", "features": ["simd"]}
        ]"#
    };

    let app_features = if reversed {
        r#"["logging", "tracing"]"#
    } else {
        r#"["tracing", "logging"]"#
    };

    let app_targets = if reversed {
        r#"["native", "portable"]"#
    } else {
        r#"["portable", "native"]"#
    };

    let app_outputs = if reversed {
        r#"["dependency_metadata", "executable"]"#
    } else {
        r#"["executable", "dependency_metadata"]"#
    };

    write_file(
        root.join("bray-workspace.json"),
        &format!(
            r#"{{
                "format": 1,
                "output_root": "build",
                "targets": {targets},
                "packages": {packages}
            }}"#
        ),
    );

    write_file(
        root.join("app").join("bray-package.json"),
        &format!(
            r#"{{
                "format": 1,
                "identity": "example.application",
                "features": {app_features},
                "source_roots": [{{"name": "main", "path": "src"}}],
                "dependencies": [{{"package": "example.math", "product": "math"}}],
                "products": [{{
                    "name": "application",
                    "kind": "executable",
                    "source_roots": ["main"],
                    "targets": {app_targets},
                    "outputs": {app_outputs}
                }}]
            }}"#
        ),
    );

    write_file(
        root.join("vendor")
            .join("math")
            .join("bray-package.json"),
        r#"{
            "format": 1,
            "identity": "example.math",
            "features": ["simd"],
            "source_roots": [{"name": "library", "path": "source"}],
            "dependencies": [],
            "products": [{
                "name": "math",
                "kind": "library",
                "source_roots": ["library"],
                "targets": ["native", "portable"],
                "outputs": ["package_interface"]
            }]
        }"#,
    );

    write_file(root.join("app").join("src").join("main.bray"), "module app");

    write_file(
        root.join("app")
            .join("src")
            .join("nested")
            .join("value.bray"),
        "module app.value",
    );

    write_file(
        root.join("app").join("src").join("ignored.txt"),
        "not Bray source",
    );

    write_file(
        root.join("vendor")
            .join("math")
            .join("source")
            .join("math.bray"),
        "module math",
    );
}

fn write_file(path: PathBuf, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("test directory must be created: {error}"));
    }

    fs::write(path, contents).unwrap_or_else(|error| panic!("test file must be written: {error}"));
}

fn replace(path: PathBuf, from: &str, to: &str) {
    let contents =
        fs::read_to_string(&path).unwrap_or_else(|error| panic!("test file must be read: {error}"));

    let updated = contents.replace(from, to);

    assert_ne!(contents, updated, "test replacement must change the fixture");

    fs::write(path, updated)
        .unwrap_or_else(|error| panic!("test file must be replaced: {error}"));
}

struct TestWorkspace {
    path: PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let ordinal = TEST_DIRECTORY_ORDINAL.fetch_add(1, Ordering::Relaxed);

        let path = std::env::temp_dir().join(format!(
            "bray-project-{}-{ordinal}",
            std::process::id()
        ));

        if path.exists() {
            fs::remove_dir_all(&path)
                .unwrap_or_else(|error| panic!("stale test directory must be removed: {error}"));
        }

        fs::create_dir_all(&path)
            .unwrap_or_else(|error| panic!("test workspace must be created: {error}"));

        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.path) {
            panic!("test workspace must be removed: {error}");
        }
    }
}
