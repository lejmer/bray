use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use bray_diagnostics::{DiagnosticArgName, DiagnosticId, DiagnosticKind, DiagnosticNoteKind};
use bray_project::{
    PackageRole, ProjectGraph, ProjectLoadError, ProjectManifestProblem, TargetPredicate,
    TargetPredicateValue, canonicalize_package_manifest, canonicalize_workspace_manifest,
    load_project_graph, load_standard_library_project_graph,
};
use bray_standard_library::PackageSourceAuthority;
use bray_symbols::PackageIdentity;
use bray_target::{TargetFactKind, TargetOutputKind};

static TEST_DIRECTORY_ORDINAL: AtomicUsize = AtomicUsize::new(0);

#[test]
fn project_graphs_are_safe_to_share_between_workers() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<ProjectGraph>();
}

#[test]
fn repository_standard_library_workspace_uses_the_reserved_source_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("standard-library");

    let graph = load_standard_library_project_graph(&root)
        .unwrap_or_else(|error| panic!("repository standard library must load: {error:?}"));

    assert_eq!(graph.packages().len(), 1);

    assert_eq!(
        graph.source_authority(),
        PackageSourceAuthority::StandardLibrary
    );

    let package = &graph.packages()[0];

    assert_eq!(package.identity().as_str(), "std");
    assert_eq!(package.version().to_string(), "0.1.0");

    assert_eq!(
        package
            .products()
            .iter()
            .map(|product| product.identity().name())
            .collect::<Vec<_>>(),
        vec!["api", "library", "outcomes"]
    );

    let api = &package.products()[0];
    let library = &package.products()[1];

    assert_eq!(api.tested_library(), Some(library.identity()));
    assert_eq!(library.targets().len(), 6);
}

#[test]
fn tested_libraries_must_name_sibling_library_products() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    let manifest = workspace.path().join("app").join("bray-package.json");

    replace(
        manifest.clone(),
        r#""kind": "executable","#,
        r#""kind": "test", "tested_library": "missing","#,
    );

    let Err(error) = load_project_graph(workspace.path()) else {
        panic!("a missing tested library must reject the graph");
    };

    assert!(matches!(
        error,
        ProjectLoadError::InvalidManifest {
            problem: ProjectManifestProblem::UnknownDependencyProduct,
            ..
        }
    ));
}

#[test]
fn manifests_load_an_exact_canonical_project_graph() {
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
            ("portable", "aarch64-pc-windows-msvc")
        ]
    );

    assert_eq!(
        graph
            .packages()
            .iter()
            .map(|package| package.identity().as_str())
            .collect::<Vec<_>>(),
        vec!["example.application", "example.math"]
    );

    let application = graph
        .packages()
        .first()
        .unwrap_or_else(|| panic!("application package must be present"));

    assert_eq!(application.role(), PackageRole::Root);
    assert_eq!(application.version().to_string(), "1.2.3");

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
        application.products()[0].dependencies()[0]
            .product()
            .package()
            .as_str(),
        "example.math"
    );

    assert_eq!(
        application.products()[0].dependencies()[0].product().name(),
        "math"
    );

    let native_plan = graph
        .build_plans()
        .iter()
        .find(|plan| plan.target().as_str() == "x86_64-unknown-linux-gnu")
        .unwrap_or_else(|| panic!("native build plan must be present"));

    assert_eq!(
        native_plan
            .products()
            .iter()
            .map(|product| product.package().as_str())
            .collect::<Vec<_>>(),
        ["example.math", "example.application"]
    );

    let identity = PackageIdentity::try_new("example.math")
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    assert_eq!(
        graph.package(&identity).map(|package| package.role()),
        Some(PackageRole::Vendored)
    );

    assert_eq!(
        graph
            .package(&identity)
            .map(|package| package.version().to_string()),
        Some(String::from("2.0.0-beta.1"))
    );
}

#[test]
fn package_versions_must_be_semantic_versions() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""version": {"workspace": true}"#,
        r#""version": "1.2""#,
    );

    let Err(error) = load_project_graph(workspace.path()) else {
        panic!("invalid package version must reject the graph");
    };

    let diagnostic = error.into_diagnostic(DiagnosticId::new(9));

    assert_eq!(
        diagnostic.kind(),
        DiagnosticKind::ProjectPackageVersionInvalid
    );

    assert_eq!(diagnostic.notes().len(), 1);

    assert_eq!(
        diagnostic.notes()[0].kind(),
        DiagnosticNoteKind::PackageVersionMustBeValid
    );
}

#[test]
fn workspace_package_versions_must_be_semantic_versions() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("bray-workspace.json"),
        r#""package": {"version": "1.2.3"}"#,
        r#""package": {"version": "1.2"}"#,
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::InvalidManifest {
            problem: ProjectManifestProblem::InvalidPackageVersion,
            ..
        })
    ));
}

#[test]
fn package_version_inheritance_must_be_enabled() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""version": {"workspace": true}"#,
        r#""version": {"workspace": false}"#,
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::InvalidManifest {
            problem: ProjectManifestProblem::InvalidPackageVersion,
            ..
        })
    ));
}

#[test]
fn inherited_package_versions_require_workspace_metadata() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("bray-workspace.json"),
        r#""package": {"version": "1.2.3"},"#,
        "",
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::InvalidManifest {
            problem: ProjectManifestProblem::MissingWorkspacePackageVersion,
            ..
        })
    ));
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
fn ordinary_projects_cannot_claim_standard_library_package_identities() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""identity": "example.application""#,
        r#""identity": "std""#,
    );

    let Err(error) = load_project_graph(workspace.path()) else {
        panic!("reserved package identity must reject the graph");
    };

    let diagnostic = error.into_diagnostic(DiagnosticId::new(10));

    assert_eq!(
        diagnostic.kind(),
        DiagnosticKind::ProjectPackageIdentityReserved
    );

    assert_eq!(
        diagnostic
            .args()
            .iter()
            .map(|argument| argument.name())
            .collect::<Vec<_>>(),
        [
            DiagnosticArgName::FilePath,
            DiagnosticArgName::ReferencedName
        ]
    );
}

#[test]
fn ordinary_projects_cannot_claim_private_standard_library_package_identities() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""identity": "example.application""#,
        r#""identity": "std.application""#,
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::InvalidManifest {
            problem: ProjectManifestProblem::ReservedPackageIdentity,
            ..
        })
    ));
}

#[test]
fn standard_library_projects_require_and_accept_reserved_package_identities() {
    let ordinary = TestWorkspace::new();
    write_valid_workspace(ordinary.path(), false);

    let Err(error) = load_standard_library_project_graph(ordinary.path()) else {
        panic!("ordinary package identity must reject a standard library graph");
    };

    let diagnostic = error.into_diagnostic(DiagnosticId::new(11));

    assert_eq!(
        diagnostic.kind(),
        DiagnosticKind::ProjectStandardLibraryPackageIdentityRequired
    );

    assert_eq!(
        diagnostic
            .args()
            .iter()
            .map(|argument| argument.name())
            .collect::<Vec<_>>(),
        [
            DiagnosticArgName::FilePath,
            DiagnosticArgName::ReferencedName
        ]
    );

    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""identity": "example.application""#,
        r#""identity": "std""#,
    );

    replace(
        workspace
            .path()
            .join("vendor")
            .join("math")
            .join("bray-package.json"),
        r#""identity": "example.math""#,
        r#""identity": "std.runtime""#,
    );

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""package": "example.math""#,
        r#""package": "std.runtime""#,
    );

    let graph = load_standard_library_project_graph(workspace.path())
        .unwrap_or_else(|error| panic!("standard library workspace must load: {error:?}"));

    assert_eq!(
        graph
            .packages()
            .iter()
            .map(|package| package.identity().as_str())
            .collect::<Vec<_>>(),
        vec!["std", "std.runtime"]
    );
}

#[test]
fn unsupported_manifest_revisions_are_rejected_before_schema_interpretation() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("bray-workspace.json"),
        r#""format": 1"#,
        r#""format": 7"#,
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::InvalidManifest {
            problem: ProjectManifestProblem::UnsupportedFormat,
            value,
            ..
        }) if value == "7"
    ));
}

#[test]
fn canonical_manifest_writers_are_idempotent_and_normalize_set_order() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    let workspace_path = workspace.path().join("bray-workspace.json");
    let package_path = workspace.path().join("app").join("bray-package.json");

    let workspace_source = fs::read_to_string(&workspace_path)
        .unwrap_or_else(|error| panic!("workspace manifest must be readable: {error:?}"));

    let package_source = fs::read_to_string(&package_path)
        .unwrap_or_else(|error| panic!("package manifest must be readable: {error:?}"));

    let workspace_canonical = canonicalize_workspace_manifest(&workspace_source, &workspace_path)
        .unwrap_or_else(|error| panic!("workspace manifest must canonicalize: {error:?}"));

    let package_canonical = canonicalize_package_manifest(&package_source, &package_path)
        .unwrap_or_else(|error| panic!("package manifest must canonicalize: {error:?}"));

    assert!(workspace_canonical.ends_with('\n'));
    assert!(package_canonical.ends_with('\n'));

    let native = workspace_canonical
        .find("\"native\"")
        .unwrap_or_else(|| panic!("canonical workspace must retain native target"));

    let portable = workspace_canonical
        .find("\"portable\"")
        .unwrap_or_else(|| panic!("canonical workspace must retain portable target"));

    let logging = package_canonical
        .find("\"logging\"")
        .unwrap_or_else(|| panic!("canonical package must retain logging feature"));

    let tracing = package_canonical
        .find("\"tracing\"")
        .unwrap_or_else(|| panic!("canonical package must retain tracing feature"));

    assert!(native < portable);
    assert!(logging < tracing);

    assert_eq!(
        canonicalize_workspace_manifest(&workspace_canonical, &workspace_path),
        Ok(workspace_canonical)
    );

    assert_eq!(
        canonicalize_package_manifest(&package_canonical, &package_path),
        Ok(package_canonical)
    );
}

#[test]
fn package_wide_dependency_syntax_is_rejected() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    let path = workspace.path().join("app").join("bray-package.json");

    replace(
        path,
        r#""source_roots": [{"name": "main", "path": "src"}],"#,
        r#""source_roots": [{"name": "main", "path": "src"}],
                "dependencies": [],"#,
    );

    assert!(matches!(
        load_project_graph(workspace.path()),
        Err(ProjectLoadError::ParseManifest { .. })
    ));
}

#[test]
fn target_conditioned_dependencies_retain_predicates_and_per_target_orders() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#"{"package": "example.math", "product": "math"}"#,
        r#"{
                        "package": "example.math",
                        "product": "math",
                        "when": {
                            "all": [
                                {
                                    "property": "target.pointer.BITS",
                                    "in": [64, 32, 64]
                                },
                                {
                                    "property": "target.identity.SYSTEM",
                                    "equals": "linux"
                                },
                                {
                                    "property": "target.identity.SYSTEM",
                                    "equals": "linux"
                                }
                            ]
                        }
                    }"#,
    );

    let graph = load_project_graph(workspace.path())
        .unwrap_or_else(|error| panic!("conditional workspace must load: {error:?}"));

    let dependency = &graph.packages()[0].products()[0].dependencies()[0];

    assert_eq!(
        dependency.property_dependencies(),
        [TargetFactKind::IdentitySystem, TargetFactKind::PointerBits]
    );

    let Some(TargetPredicate::All(children)) = dependency.predicate() else {
        panic!("conditional dependency must retain one normalized all predicate");
    };

    assert_eq!(children.len(), 2);

    let values = children.iter().find_map(|child| match child {
        TargetPredicate::In(TargetFactKind::PointerBits, values) => Some(values.as_ref()),
        _ => None,
    });

    assert_eq!(
        values,
        Some(
            [
                TargetPredicateValue::Usize(32),
                TargetPredicateValue::Usize(64)
            ]
            .as_slice()
        )
    );

    assert_eq!(
        dependency
            .active_targets()
            .iter()
            .map(|target| target.as_str())
            .collect::<Vec<_>>(),
        ["x86_64-unknown-linux-gnu"]
    );

    let orders = graph
        .build_plans()
        .iter()
        .map(|plan| {
            plan.products()
                .iter()
                .map(|product| product.package().as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        orders,
        [
            vec!["example.math", "example.application"],
            vec!["example.application", "example.math"],
        ]
    );
}

#[test]
fn target_predicates_reject_unknown_properties_and_mismatched_values() {
    for predicate in [
        r#"{"property": "target.unknown", "equals": true}"#,
        r#"{"property": "target.pointer.BITS", "equals": "64"}"#,
    ] {
        let workspace = TestWorkspace::new();
        write_valid_workspace(workspace.path(), false);

        replace(
            workspace.path().join("app").join("bray-package.json"),
            r#"{"package": "example.math", "product": "math"}"#,
            &format!(r#"{{"package": "example.math", "product": "math", "when": {predicate}}}"#),
        );

        assert!(matches!(
            load_project_graph(workspace.path()),
            Err(ProjectLoadError::InvalidManifest {
                problem: ProjectManifestProblem::InvalidTargetPredicate,
                ..
            })
        ));
    }
}

#[test]
fn standard_library_projects_require_the_public_std_root() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""identity": "example.application""#,
        r#""identity": "std.application""#,
    );

    replace(
        workspace
            .path()
            .join("vendor")
            .join("math")
            .join("bray-package.json"),
        r#""identity": "example.math""#,
        r#""identity": "std.runtime""#,
    );

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""package": "example.math""#,
        r#""package": "std.runtime""#,
    );

    let Err(error) = load_standard_library_project_graph(workspace.path()) else {
        panic!("missing public standard library root must reject the graph");
    };

    let diagnostic = error.into_diagnostic(DiagnosticId::new(12));

    assert_eq!(
        diagnostic.kind(),
        DiagnosticKind::ProjectStandardLibraryRootPackageRequired
    );

    assert_eq!(
        diagnostic
            .args()
            .iter()
            .map(|argument| argument.name())
            .collect::<Vec<_>>(),
        [
            DiagnosticArgName::FilePath,
            DiagnosticArgName::ReferencedName
        ]
    );
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
        r#""identity": "aarch64-pc-windows-msvc""#,
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
            {"name": "portable", "identity": "aarch64-pc-windows-msvc"}
        ]"#
    } else {
        r#"[
            {"name": "portable", "identity": "aarch64-pc-windows-msvc"},
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
                "package": {{"version": "1.2.3"}},
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
                "version": {{"workspace": true}},
                "features": {app_features},
                "source_roots": [{{"name": "main", "path": "src"}}],
                "products": [{{
                    "name": "application",
                    "kind": "executable",
                    "source_roots": ["main"],
                    "targets": {app_targets},
                    "dependencies": [{{"package": "example.math", "product": "math"}}],
                    "outputs": {app_outputs}
                }}]
            }}"#
        ),
    );

    write_file(
        root.join("vendor").join("math").join("bray-package.json"),
        r#"{
            "format": 1,
            "identity": "example.math",
            "version": "2.0.0-beta.1",
            "features": ["simd"],
            "source_roots": [{"name": "library", "path": "source"}],
            "products": [{
                "name": "math",
                "kind": "library",
                "source_roots": ["library"],
                "targets": ["native", "portable"],
                "dependencies": [],
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

    assert_ne!(
        contents, updated,
        "test replacement must change the fixture"
    );

    fs::write(path, updated).unwrap_or_else(|error| panic!("test file must be replaced: {error}"));
}

struct TestWorkspace {
    path: PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let ordinal = TEST_DIRECTORY_ORDINAL.fetch_add(1, Ordering::Relaxed);

        let path =
            std::env::temp_dir().join(format!("bray-project-{}-{ordinal}", std::process::id()));

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
