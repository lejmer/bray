use std::sync::Arc;

use bray_compilation::WorkerBudget;
use bray_project::load_project_graph;

use super::Workspace;
use crate::model::{ContentChange, Position, Range};
use crate::test_support::ProjectFixture;

#[test]
fn document_edits_create_revisioned_immutable_compilation_snapshots() {
    let fixture = ProjectFixture::new();

    let graph = load_project_graph(fixture.path())
        .unwrap_or_else(|error| panic!("test project should load: {error:?}"));

    let target = graph
        .targets()
        .first()
        .unwrap_or_else(|| panic!("test target should exist"))
        .identity()
        .clone();

    let uri = bray_source::SourceOrigin::file_uri_from_path(&fixture.source)
        .unwrap_or_else(|error| panic!("test source URI should form: {error:?}"));

    let mut workspace = Workspace::new(
        fixture.path(),
        Arc::new(graph),
        target,
        WorkerBudget::new(2)
            .unwrap_or_else(|error| panic!("test worker budget should form: {error:?}")),
    );

    let opened = workspace
        .open_document(uri.clone(), 1, fixture.source_text.to_owned())
        .unwrap_or_else(|error| panic!("test document should open: {error:?}"))
        .current;

    let changed = workspace
        .change_document(
            &uri,
            2,
            &[ContentChange {
                range: Some(Range {
                    start: Position {
                        line: 0,
                        character: 7,
                    },
                    end: Position {
                        line: 0,
                        character: 10,
                    },
                }),
                text: String::from("new"),
            }],
        )
        .unwrap_or_else(|error| panic!("test document should change: {error:?}"))
        .current;

    assert_eq!(opened.revision, 1);
    assert_eq!(changed.revision, 2);

    assert!(!Arc::ptr_eq(&opened.compilation, &changed.compilation));

    assert_eq!(
        opened
            .compilation
            .source(opened.source_id)
            .map(bray_source::SourceSnapshot::text),
        Some(fixture.source_text)
    );

    assert!(
        changed
            .compilation
            .source(changed.source_id)
            .is_some_and(|source| source.text().starts_with("module new;"))
    );

    assert!(!workspace.is_current(&opened.compilation_revision));
    assert!(workspace.is_current(&changed.compilation_revision));
}

#[test]
fn edits_invalidate_other_open_documents_in_the_same_compilation() {
    let fixture = ProjectFixture::new();
    let other = fixture.path().join("app").join("src").join("other.bray");

    std::fs::write(&other, "module app;\n")
        .unwrap_or_else(|error| panic!("second test source should write: {error}"));

    let graph = load_project_graph(fixture.path())
        .unwrap_or_else(|error| panic!("test project should load: {error:?}"));

    let target = graph
        .targets()
        .first()
        .unwrap_or_else(|| panic!("test target should exist"))
        .identity()
        .clone();

    let main_uri = bray_source::SourceOrigin::file_uri_from_path(&fixture.source)
        .unwrap_or_else(|error| panic!("main source URI should form: {error:?}"));

    let other_uri = bray_source::SourceOrigin::file_uri_from_path(&other)
        .unwrap_or_else(|error| panic!("second source URI should form: {error:?}"));

    let mut workspace = Workspace::new(
        fixture.path(),
        Arc::new(graph),
        target,
        WorkerBudget::serial(),
    );

    workspace
        .open_document(main_uri.clone(), 1, fixture.source_text.to_owned())
        .unwrap_or_else(|error| panic!("main document should open: {error:?}"));

    let other_before = workspace
        .open_document(other_uri.clone(), 1, String::from("module app;\n"))
        .unwrap_or_else(|error| panic!("second document should open: {error:?}"))
        .current;

    let update = workspace
        .change_document(
            &main_uri,
            2,
            &[ContentChange {
                range: None,
                text: fixture
                    .source_text
                    .replace("module app;", "module changed;"),
            }],
        )
        .unwrap_or_else(|error| panic!("main document should change: {error:?}"));

    let other_after = update
        .affected
        .iter()
        .find(|document| document.uri == other_uri)
        .unwrap_or_else(|| panic!("second document should be affected"));

    assert!(!workspace.is_current(&other_before.compilation_revision));
    assert!(workspace.is_current(&other_after.compilation_revision));

    assert_eq!(other_before.revision, other_after.revision);
}

#[test]
fn workspace_dependencies_use_live_compilations_without_emitted_interfaces() {
    let fixture = ProjectFixture::new();
    let vendor = fixture.path().join("vendor").join("math");

    std::fs::create_dir_all(vendor.join("src"))
        .unwrap_or_else(|error| panic!("vendor source directory should form: {error}"));

    std::fs::write(
        fixture.path().join("bray-workspace.json"),
        r#"{
                "format": 1,
                "package": {"version": "0.1.0"},
                "output_root": "build",
                "targets": [
                    {
                        "name": "native",
                        "identity": "x86_64-unknown-linux-gnu"
                    }
                ],
                "packages": [
                    {
                        "path": "app",
                        "role": "root",
                        "features": []
                    },
                    {
                        "path": "vendor/math",
                        "role": "vendored",
                        "features": []
                    }
                ]
            }"#,
    )
    .unwrap_or_else(|error| panic!("workspace manifest should update: {error}"));

    std::fs::write(
        fixture.path().join("app").join("bray-package.json"),
        r#"{
                "format": 1,
                "identity": "example.application",
                "version": {"workspace": true},
                "features": [],
                "source_roots": [
                    {
                        "name": "main",
                        "path": "src"
                    }
                ],
                "products": [
                    {
                        "name": "application",
                        "kind": "executable",
                        "source_roots": ["main"],
                        "targets": ["native"],
                        "dependencies": [
                            {
                                "package": "example.math",
                                "product": "math"
                            }
                        ],
                        "outputs": ["executable"]
                    }
                ]
            }"#,
    )
    .unwrap_or_else(|error| panic!("application manifest should update: {error}"));

    std::fs::write(
        vendor.join("bray-package.json"),
        r#"{
                "format": 1,
                "identity": "example.math",
                "version": "1.0.0",
                "features": [],
                "source_roots": [
                    {
                        "name": "library",
                        "path": "src"
                    }
                ],
                "products": [
                    {
                        "name": "math",
                        "kind": "library",
                        "source_roots": ["library"],
                        "targets": ["native"],
                        "dependencies": [],
                        "outputs": ["package_interface"]
                    }
                ]
            }"#,
    )
    .unwrap_or_else(|error| panic!("dependency manifest should write: {error}"));

    std::fs::write(vendor.join("src").join("math.bray"), "module math;\n")
        .unwrap_or_else(|error| panic!("dependency source should write: {error}"));

    let graph = load_project_graph(fixture.path())
        .unwrap_or_else(|error| panic!("test project should load: {error:?}"));

    let target = graph
        .targets()
        .first()
        .unwrap_or_else(|| panic!("test target should exist"))
        .identity()
        .clone();

    let uri = bray_source::SourceOrigin::file_uri_from_path(&fixture.source)
        .unwrap_or_else(|error| panic!("test source URI should form: {error:?}"));

    let mut workspace = Workspace::new(
        fixture.path(),
        Arc::new(graph),
        target,
        WorkerBudget::serial(),
    );

    let opened = workspace
        .open_document(uri.clone(), 1, fixture.source_text.to_owned())
        .unwrap_or_else(|error| panic!("dependent document should open: {error:?}"))
        .current;

    assert!(!fixture.path().join("build").exists());

    let dependency_source = vendor.join("src").join("math.bray");

    let dependency_uri = bray_source::SourceOrigin::file_uri_from_path(&dependency_source)
        .unwrap_or_else(|error| panic!("dependency source URI should form: {error:?}"));

    let update = workspace
        .open_document(dependency_uri, 1, String::from("module math;\n"))
        .unwrap_or_else(|error| panic!("dependency document should open: {error:?}"));

    let dependent = update
        .affected
        .iter()
        .find(|document| document.uri == uri)
        .unwrap_or_else(|| panic!("dependent document should be invalidated"));

    assert!(!workspace.is_current(&opened.compilation_revision));
    assert!(workspace.is_current(&dependent.compilation_revision));

    assert_eq!(opened.revision, dependent.revision);
}
