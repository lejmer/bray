use super::WorkspaceError;
use super::sources::{
    WorkspaceSource, append_open_source, apply_changes, source_index_for_uri, source_version,
};
use crate::model::ContentChange;
use bray_compilation::{Compilation, WorkerBudget};
use bray_project::ProjectGraph;
use bray_source::{SourceId, SourceIdentity, SourceSnapshot};
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct DocumentSnapshot {
    pub(crate) compilation: Arc<Compilation>,
    pub(crate) source_id: SourceId,
    pub(crate) uri: String,
    pub(crate) revision: u64,
    pub(crate) compilation_revision: CompilationRevision,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CompilationRevision {
    pub(crate) product: ProductIdentity,
    pub(crate) target: TargetIdentity,
    pub(crate) generation: u64,
}

pub(crate) struct WorkspaceUpdate {
    pub(crate) current: DocumentSnapshot,
    pub(crate) affected: Vec<DocumentSnapshot>,
}

pub(crate) struct Workspace {
    pub(super) root: PathBuf,
    pub(super) graph: Arc<ProjectGraph>,
    pub(super) target: TargetIdentity,
    pub(super) worker_budget: WorkerBudget,
    pub(super) products: BTreeMap<ProductIdentity, ProductState>,
    documents: BTreeMap<String, DocumentOwner>,
    next_generation: u64,
}

pub(super) struct ProductState {
    pub(super) compilation: Arc<Compilation>,
    pub(super) sources: Vec<WorkspaceSource>,
    pub(super) generation: u64,
}

#[derive(Clone, Debug)]
struct DocumentOwner {
    product: ProductIdentity,
    identity: SourceIdentity,
}

impl Workspace {
    pub(crate) fn new(
        root: impl Into<PathBuf>,
        graph: Arc<ProjectGraph>,
        target: TargetIdentity,
        worker_budget: WorkerBudget,
    ) -> Self {
        Self {
            root: root.into(),
            graph,
            target,
            worker_budget,
            products: BTreeMap::new(),
            documents: BTreeMap::new(),
            next_generation: 0,
        }
    }

    pub(crate) fn open_document(
        &mut self,
        uri: String,
        version: i64,
        text: String,
    ) -> Result<WorkspaceUpdate, WorkspaceError> {
        let version = source_version(version)?;
        let product = self.product_for_uri(&uri)?.clone();
        let identity = product.identity().clone();

        self.ensure_product_loaded(&identity)?;

        let state = self
            .products
            .get_mut(&identity)
            .ok_or(WorkspaceError::ProductNotFound)?;

        let source_index = match source_index_for_uri(&state.sources, &uri) {
            Some(index) => index,
            None => append_open_source(&mut state.sources, &uri)?,
        };

        let source = state
            .sources
            .get_mut(source_index)
            .ok_or(WorkspaceError::DocumentNotFound)?;

        source.uri.clone_from(&uri);

        source.version = version;
        source.text = text;
        source.is_open = true;

        let source_identity = source.identity;

        self.documents.insert(
            uri.clone(),
            DocumentOwner {
                product: identity.clone(),
                identity: source_identity,
            },
        );

        let affected = self.rebuild_products_from(&identity)?;

        let current = self
            .document(&uri)
            .ok_or(WorkspaceError::DocumentNotFound)?;

        Ok(WorkspaceUpdate {
            current,
            affected: self.documents_for_products(&affected),
        })
    }

    pub(crate) fn change_document(
        &mut self,
        uri: &str,
        version: i64,
        changes: &[ContentChange],
    ) -> Result<WorkspaceUpdate, WorkspaceError> {
        let version = source_version(version)?;

        let owner = self
            .documents
            .get(uri)
            .cloned()
            .ok_or(WorkspaceError::DocumentNotFound)?;

        let state = self
            .products
            .get_mut(&owner.product)
            .ok_or(WorkspaceError::ProductNotFound)?;

        let source = state
            .sources
            .iter_mut()
            .find(|source| source.identity == owner.identity)
            .ok_or(WorkspaceError::DocumentNotFound)?;

        if version <= source.version {
            return Err(WorkspaceError::NonIncreasingVersion {
                current: source.version.raw(),
                actual: version.raw(),
            });
        }

        apply_changes(source, version, changes)?;

        let affected = self.rebuild_products_from(&owner.product)?;
        let current = self.document(uri).ok_or(WorkspaceError::DocumentNotFound)?;

        Ok(WorkspaceUpdate {
            current,
            affected: self.documents_for_products(&affected),
        })
    }

    pub(crate) fn close_document(
        &mut self,
        uri: &str,
    ) -> Result<Vec<DocumentSnapshot>, WorkspaceError> {
        let owner = self
            .documents
            .remove(uri)
            .ok_or(WorkspaceError::DocumentNotFound)?;

        let state = self
            .products
            .get_mut(&owner.product)
            .ok_or(WorkspaceError::ProductNotFound)?;

        let source = state
            .sources
            .iter_mut()
            .find(|source| source.identity == owner.identity)
            .ok_or(WorkspaceError::DocumentNotFound)?;

        let Some(path) = source.path.as_ref() else {
            source.is_open = false;

            let affected = self.rebuild_products_from(&owner.product)?;

            return Ok(self.documents_for_products(&affected));
        };

        let text = std::fs::read_to_string(path).map_err(|cause| WorkspaceError::SourceRead {
            path: path.clone(),
            cause,
        })?;

        let version = source
            .version
            .checked_next()
            .ok_or(WorkspaceError::VersionExhausted {
                current: source.version.raw(),
            })?;

        source.text = text;
        source.version = version;
        source.is_open = false;

        let affected = self.rebuild_products_from(&owner.product)?;

        Ok(self.documents_for_products(&affected))
    }

    pub(crate) fn document(&self, uri: &str) -> Option<DocumentSnapshot> {
        let owner = self.documents.get(uri)?;
        let state = self.products.get(&owner.product)?;

        let source = state
            .sources
            .iter()
            .find(|source| source.identity == owner.identity)?;

        document_snapshot(
            state,
            owner.identity,
            uri.to_owned(),
            source.version.raw(),
            CompilationRevision {
                product: owner.product.clone(),
                target: self.target.clone(),
                generation: state.generation,
            },
        )
        .ok()
    }

    pub(crate) fn is_current(&self, revision: &CompilationRevision) -> bool {
        self.target == revision.target
            && self
                .products
                .get(&revision.product)
                .is_some_and(|state| state.generation == revision.generation)
    }
    fn documents_for_products(
        &self,
        products: &BTreeSet<ProductIdentity>,
    ) -> Vec<DocumentSnapshot> {
        self.documents
            .iter()
            .filter(|(_, owner)| products.contains(&owner.product))
            .filter_map(|(uri, _)| self.document(uri))
            .collect()
    }

    pub(super) fn next_generation(&mut self) -> Result<u64, WorkspaceError> {
        self.next_generation =
            self.next_generation
                .checked_add(1)
                .ok_or(WorkspaceError::GenerationExhausted {
                    current: self.next_generation,
                })?;

        Ok(self.next_generation)
    }
}

fn document_snapshot(
    state: &ProductState,
    identity: SourceIdentity,
    uri: String,
    revision: u64,
    compilation_revision: CompilationRevision,
) -> Result<DocumentSnapshot, WorkspaceError> {
    let source_id = state
        .compilation
        .sources()
        .latest_for_identity(identity)
        .map(SourceSnapshot::source_id)
        .ok_or(WorkspaceError::DocumentNotFound)?;

    Ok(DocumentSnapshot {
        compilation: Arc::clone(&state.compilation),
        source_id,
        uri,
        revision,
        compilation_revision,
    })
}

#[cfg(test)]
mod tests {
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
}
