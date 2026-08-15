use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use bray_diagnostics::{
    DiagnosticArgName, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticNoteKind,
};
use bray_project::{
    PackageRole, ProjectGraph, ProjectLoadError, ProjectManifestProblemKind, TargetPredicate,
    TargetPredicateValue, canonicalize_package_manifest, canonicalize_workspace_manifest,
    load_project_graph, load_standard_library_project_graph,
};
use bray_standard_library::PackageSourceAuthority;
use bray_symbols::PackageIdentity;
use bray_target::{TargetOutputKind, TargetPropertyKind};

static TEST_DIRECTORY_ORDINAL: AtomicUsize = AtomicUsize::new(0);

fn assert_manifest_problem(
    result: Result<ProjectGraph, ProjectLoadError>,
    expected: ProjectManifestProblemKind,
) {
    let Err(ProjectLoadError::InvalidManifest { problem, .. }) = result else {
        panic!("project graph must fail with a manifest validation problem");
    };

    assert_eq!(problem.kind(), expected);
}

fn manifest_diagnostic(result: Result<ProjectGraph, ProjectLoadError>) -> DiagnosticBag {
    let Err(error) = result else {
        panic!("project graph must fail with a manifest diagnostic");
    };

    DiagnosticBag::single(error.into_diagnostic(DiagnosticId::new(0)))
}

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

    assert_manifest_problem(
        Err(error),
        ProjectManifestProblemKind::UnknownDependencyProduct,
    );

    let non_test_workspace = TestWorkspace::new();
    write_valid_workspace(non_test_workspace.path(), false);

    replace(
        non_test_workspace
            .path()
            .join("app")
            .join("bray-package.json"),
        r#""kind": "executable","#,
        r#""kind": "executable", "tested_library": "application","#,
    );

    let diagnostics = manifest_diagnostic(load_project_graph(non_test_workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::ProjectManifestUnexpectedTestedLibrary,
    );
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

    bray_testing::assert_goal_state_diagnostic_kind(
        &DiagnosticBag::single(diagnostic),
        DiagnosticKind::ProjectPackageVersionInvalid,
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

    assert_manifest_problem(
        load_project_graph(workspace.path()),
        ProjectManifestProblemKind::InvalidPackageVersion,
    );
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

    assert_manifest_problem(
        load_project_graph(workspace.path()),
        ProjectManifestProblemKind::InvalidPackageVersion,
    );
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

    let diagnostics = manifest_diagnostic(load_project_graph(workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::ProjectPackageVersionMissingWorkspace,
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
            DiagnosticArgName::ProjectManifestField,
            DiagnosticArgName::ActualPackageIdentity,
        ]
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        &DiagnosticBag::single(diagnostic),
        DiagnosticKind::ProjectPackageIdentityReserved,
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

    assert_manifest_problem(
        load_project_graph(workspace.path()),
        ProjectManifestProblemKind::ReservedPackageIdentity,
    );
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
            DiagnosticArgName::ProjectManifestField,
            DiagnosticArgName::ActualPackageIdentity,
        ]
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        &DiagnosticBag::single(diagnostic),
        DiagnosticKind::ProjectStandardLibraryPackageIdentityRequired,
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

    let diagnostics = manifest_diagnostic(load_project_graph(workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::ProjectManifestUnsupportedFormat,
    );
}

#[test]
fn invalid_project_paths_and_names_preserve_their_manifest_fields() {
    let path_workspace = TestWorkspace::new();
    write_valid_workspace(path_workspace.path(), false);

    replace(
        path_workspace.path().join("bray-workspace.json"),
        r#""output_root": "build""#,
        r#""output_root": "../build""#,
    );

    let path_diagnostics = manifest_diagnostic(load_project_graph(path_workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &path_diagnostics,
        DiagnosticKind::ProjectManifestInvalidPath,
    );

    let name_workspace = TestWorkspace::new();
    write_valid_workspace(name_workspace.path(), false);

    replace(
        name_workspace.path().join("bray-workspace.json"),
        r#""name": "native""#,
        r#""name": "Native""#,
    );

    let name_diagnostics = manifest_diagnostic(load_project_graph(name_workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &name_diagnostics,
        DiagnosticKind::ProjectManifestInvalidName,
    );
}

#[test]
fn missing_output_selection_preserves_the_exact_manifest_field() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""outputs": ["executable", "dependency_metadata"]"#,
        r#""outputs": []"#,
    );

    let diagnostics = manifest_diagnostic(load_project_graph(workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::ProjectManifestMissingSelection,
    );
}

#[test]
fn unavailable_dependency_target_preserves_product_and_target_identity() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace
            .path()
            .join("vendor")
            .join("math")
            .join("bray-package.json"),
        r#""targets": ["native", "portable"]"#,
        r#""targets": ["native"]"#,
    );

    let diagnostics = manifest_diagnostic(load_project_graph(workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::ProjectDependencyProductTargetUnavailable,
    );
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
        [
            TargetPropertyKind::IdentitySystem,
            TargetPropertyKind::PointerBits
        ]
    );

    let Some(TargetPredicate::All(children)) = dependency.predicate() else {
        panic!("conditional dependency must retain one normalized all predicate");
    };

    assert_eq!(children.len(), 2);

    let values = children.iter().find_map(|child| match child {
        TargetPredicate::In(TargetPropertyKind::PointerBits, values) => Some(values.as_ref()),
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
    let unknown_workspace = TestWorkspace::new();
    write_valid_workspace(unknown_workspace.path(), false);

    replace(
        unknown_workspace
            .path()
            .join("app")
            .join("bray-package.json"),
        r#"{"package": "example.math", "product": "math"}"#,
        r#"{"package": "example.math", "product": "math", "when": {"property": "target.unknown", "equals": true}}"#,
    );

    let unknown = manifest_diagnostic(load_project_graph(unknown_workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &unknown,
        DiagnosticKind::ProjectManifestUnknownTargetPredicateProperty,
    );

    let mismatch_workspace = TestWorkspace::new();
    write_valid_workspace(mismatch_workspace.path(), false);

    replace(
        mismatch_workspace
            .path()
            .join("app")
            .join("bray-package.json"),
        r#"{"package": "example.math", "product": "math"}"#,
        r#"{"package": "example.math", "product": "math", "when": {"property": "target.pointer.BITS", "equals": "64"}}"#,
    );

    let mismatch = manifest_diagnostic(load_project_graph(mismatch_workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &mismatch,
        DiagnosticKind::ProjectManifestTargetPredicateValueKindMismatch,
    );
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
            DiagnosticArgName::ProjectManifestField,
            DiagnosticArgName::ActualPackageIdentity,
        ]
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        &DiagnosticBag::single(diagnostic),
        DiagnosticKind::ProjectStandardLibraryRootPackageRequired,
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

    let diagnostics = manifest_diagnostic(load_project_graph(workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::ProjectManifestUndeclaredFeature,
    );
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
        DiagnosticKind::ProjectDependencyProductUnknown
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        &DiagnosticBag::single(diagnostic),
        DiagnosticKind::ProjectDependencyProductUnknown,
    );
}

#[test]
fn dependency_edges_preserve_unknown_packages_and_non_library_products() {
    let package_workspace = TestWorkspace::new();
    write_valid_workspace(package_workspace.path(), false);

    replace(
        package_workspace
            .path()
            .join("app")
            .join("bray-package.json"),
        r#""package": "example.math""#,
        r#""package": "example.missing""#,
    );

    let package = manifest_diagnostic(load_project_graph(package_workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &package,
        DiagnosticKind::ProjectDependencyPackageUnknown,
    );

    let product_workspace = TestWorkspace::new();
    write_valid_workspace(product_workspace.path(), false);

    replace(
        product_workspace
            .path()
            .join("vendor")
            .join("math")
            .join("bray-package.json"),
        r#""kind": "library""#,
        r#""kind": "executable""#,
    );

    let product = manifest_diagnostic(load_project_graph(product_workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &product,
        DiagnosticKind::ProjectDependencyProductNotLibrary,
    );
}

#[test]
fn dependencies_are_isolated_to_the_declaring_product() {
    let workspace = TestWorkspace::new();
    write_valid_workspace(workspace.path(), false);

    replace(
        workspace.path().join("app").join("bray-package.json"),
        r#""outputs": ["executable", "dependency_metadata"]
                }]"#,
        r#""outputs": ["executable", "dependency_metadata"]
                }, {
                    "name": "independent",
                    "kind": "library",
                    "source_roots": ["main"],
                    "targets": ["native", "portable"],
                    "dependencies": [],
                    "outputs": ["package_interface"]
                }]"#,
    );

    let graph = load_project_graph(workspace.path())
        .unwrap_or_else(|error| panic!("product-scoped dependencies must load: {error:?}"));

    let package = &graph.packages()[0];
    let application = &package.products()[0];
    let independent = &package.products()[1];

    assert_eq!(application.identity().name(), "application");
    assert_eq!(application.dependencies().len(), 1);
    assert_eq!(independent.identity().name(), "independent");
    assert!(independent.dependencies().is_empty());
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

    let diagnostics = manifest_diagnostic(load_project_graph(workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::ProjectDependencyCycle,
    );
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

    let diagnostics = manifest_diagnostic(load_project_graph(workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::ProjectSourceRootInvalid,
    );
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

    assert_manifest_problem(
        load_project_graph(workspace.path()),
        ProjectManifestProblemKind::InvalidSourceRoot,
    );
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

    let diagnostics = manifest_diagnostic(load_project_graph(workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::ProjectManifestDuplicateSelection,
    );
}

#[test]
fn project_selections_preserve_missing_root_unknown_source_and_unknown_target() {
    let root_workspace = TestWorkspace::new();
    write_valid_workspace(root_workspace.path(), false);

    replace(
        root_workspace.path().join("bray-workspace.json"),
        r#""role": "root""#,
        r#""role": "vendored""#,
    );

    let root = manifest_diagnostic(load_project_graph(root_workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &root,
        DiagnosticKind::ProjectManifestMissingRootPackage,
    );

    let source_workspace = TestWorkspace::new();
    write_valid_workspace(source_workspace.path(), false);

    replace(
        source_workspace
            .path()
            .join("app")
            .join("bray-package.json"),
        r#""source_roots": ["main"]"#,
        r#""source_roots": ["missing"]"#,
    );

    let source = manifest_diagnostic(load_project_graph(source_workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &source,
        DiagnosticKind::ProjectManifestUnknownSourceRoot,
    );

    let target_workspace = TestWorkspace::new();
    write_valid_workspace(target_workspace.path(), false);

    replace(
        target_workspace
            .path()
            .join("app")
            .join("bray-package.json"),
        r#""targets": ["portable", "native"]"#,
        r#""targets": ["missing"]"#,
    );

    let target = manifest_diagnostic(load_project_graph(target_workspace.path()));

    bray_testing::assert_goal_state_diagnostic_kind(
        &target,
        DiagnosticKind::ProjectManifestUnknownTarget,
    );
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
