use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_compilation::{
    Compilation, CompilationOptions, DependencyInterfaceInput, SelectedTarget, WorkerBudget,
};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
};
use bray_project::{ProjectGraph, ProjectProduct};
use bray_source::{
    LineIndex, LspPosition, SourceEdit, SourceId, SourceIdentity, SourceInput, SourceOrigin,
    SourceSnapshot, SourceVersion,
};
use bray_symbols::ProductIdentity;
use bray_tooling::{
    compilation_request_from_file_arguments, project_interface_path,
};

use crate::model::{ContentChange, Position};

#[derive(Clone)]
pub(crate) struct DocumentSnapshot {
    pub(crate) compilation: Arc<Compilation>,
    pub(crate) source_id: SourceId,
    pub(crate) uri: String,
    pub(crate) revision: u64,
}

pub(crate) struct Workspace {
    root: PathBuf,
    graph: Arc<ProjectGraph>,
    worker_budget: WorkerBudget,
    products: BTreeMap<ProductIdentity, ProductState>,
    documents: BTreeMap<String, DocumentOwner>,
}

struct ProductState {
    compilation: Arc<Compilation>,
    sources: Vec<WorkspaceSource>,
}

#[derive(Clone, Debug)]
struct WorkspaceSource {
    identity: SourceIdentity,
    path: Option<PathBuf>,
    uri: String,
    version: SourceVersion,
    text: String,
    is_open: bool,
}

#[derive(Clone, Debug)]
struct DocumentOwner {
    product: ProductIdentity,
    identity: SourceIdentity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceError {
    Compilation,
    DocumentNotFound,
    InvalidDocumentUri,
    InvalidEdit,
    InvalidVersion,
    ProductNotFound,
    SourceTooLarge,
    UnsupportedTarget,
}

impl WorkspaceError {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Compilation => "language_server_compilation_failed",
            Self::DocumentNotFound => "language_server_document_not_found",
            Self::InvalidDocumentUri => "language_server_invalid_document_uri",
            Self::InvalidEdit => "language_server_invalid_document_edit",
            Self::InvalidVersion => "language_server_invalid_document_version",
            Self::ProductNotFound => "language_server_product_not_found",
            Self::SourceTooLarge => "language_server_source_too_large",
            Self::UnsupportedTarget => "language_server_unsupported_target",
        }
    }
}

impl Workspace {
    pub(crate) fn new(
        root: impl Into<PathBuf>,
        graph: Arc<ProjectGraph>,
        worker_budget: WorkerBudget,
    ) -> Self {
        Self {
            root: root.into(),
            graph,
            worker_budget,
            products: BTreeMap::new(),
            documents: BTreeMap::new(),
        }
    }

    pub(crate) fn open_document(
        &mut self,
        uri: String,
        version: i64,
        text: String,
    ) -> Result<DocumentSnapshot, WorkspaceError> {
        let version = source_version(version)?;
        let product = self.product_for_uri(&uri)?.clone();
        let identity = product.identity().clone();

        if !self.products.contains_key(&identity) {
            let state = self.load_product(product)?;

            self.products.insert(identity.clone(), state);
        }

        let state = self
            .products
            .get_mut(&identity)
            .ok_or(WorkspaceError::ProductNotFound)?;

        let source_index = match source_index_for_uri(&state.sources, &uri) {
            Some(index) => index,
            None => append_open_source(&mut state.sources, &uri)
                .ok_or(WorkspaceError::SourceTooLarge)?,
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

        revise_compilation(state)?;

        self.documents.insert(
            uri.clone(),
            DocumentOwner {
                product: identity,
                identity: source_identity,
            },
        );

        document_snapshot(state, source_identity, uri, version.raw())
    }

    pub(crate) fn change_document(
        &mut self,
        uri: &str,
        version: i64,
        changes: &[ContentChange],
    ) -> Result<DocumentSnapshot, WorkspaceError> {
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
            return Err(WorkspaceError::InvalidVersion);
        }

        apply_changes(source, version, changes)?;

        revise_compilation(state)?;

        document_snapshot(state, owner.identity, uri.to_owned(), version.raw())
    }

    pub(crate) fn close_document(&mut self, uri: &str) -> Result<(), WorkspaceError> {
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

            return Ok(());
        };

        let text = std::fs::read_to_string(path).map_err(|_| WorkspaceError::Compilation)?;

        let version = source
            .version
            .checked_next()
            .ok_or(WorkspaceError::InvalidVersion)?;

        source.text = text;
        source.version = version;
        source.is_open = false;

        revise_compilation(state)
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
        )
        .ok()
    }

    pub(crate) fn is_current(&self, uri: &str, revision: u64) -> bool {
        self.document(uri)
            .is_some_and(|document| document.revision == revision)
    }

    fn product_for_uri(&self, uri: &str) -> Result<&ProjectProduct, WorkspaceError> {
        let document_path = SourceOrigin::path_from_document_uri(uri)
            .map_err(|_| WorkspaceError::InvalidDocumentUri)?;

        if let Some(document_path) = document_path {
            let document_path = absolute_path(&document_path)?;

            if let Some(product) = self.graph.packages().iter().find_map(|package| {
                package.products().iter().find(|product| {
                    product.sources().iter().any(|source| {
                        absolute_path(&source.beneath(&self.root))
                            .is_ok_and(|source| source == document_path)
                    })
                })
            }) {
                return Ok(product);
            }
        }

        self.graph
            .packages()
            .iter()
            .flat_map(|package| package.products())
            .next()
            .ok_or(WorkspaceError::ProductNotFound)
    }

    fn load_product(&self, product: ProjectProduct) -> Result<ProductState, WorkspaceError> {
        let selected_target = SelectedTarget::baseline();

        if !product.targets().contains(selected_target.profile().identity()) {
            return Err(WorkspaceError::UnsupportedTarget);
        }

        let files = product
            .sources()
            .iter()
            .map(|source| source.beneath(&self.root))
            .collect::<Vec<_>>();

        let options = CompilationOptions::new(
            self.worker_budget,
            product.kind(),
            selected_target.clone(),
        );

        let mut request = compilation_request_from_file_arguments(
            product.identity().package().clone(),
            files,
            options,
        )
        .map_err(|_| WorkspaceError::Compilation)?;

        request = request.with_dependency_interfaces(self.dependency_interfaces(
            &product,
            selected_target.profile().identity(),
        ));

        let compilation = Compilation::load(request).map_err(|_| WorkspaceError::Compilation)?;

        let sources = compilation
            .sources()
            .iter()
            .map(workspace_source)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ProductState {
            compilation: Arc::new(compilation),
            sources,
        })
    }

    fn dependency_interfaces(
        &self,
        product: &ProjectProduct,
        target: &bray_target::TargetIdentity,
    ) -> Vec<DependencyInterfaceInput> {
        let Some(package) = self.graph.package(product.identity().package()) else {
            return Vec::new();
        };

        package
            .dependencies()
            .iter()
            .filter_map(|dependency| {
                let identity = dependency.product();

                let interface_product = InterfaceProductIdentity::try_new(identity.name())?;

                let path = project_interface_path(
                    &self.graph,
                    &self.root,
                    identity,
                    target,
                )?;

                let bytes = std::fs::read(&path).ok()?;

                Some(DependencyInterfaceInput::new(
                    identity.package().clone(),
                    interface_product,
                    path,
                    bytes,
                    InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
                ))
            })
            .collect()
    }
}

fn revise_compilation(state: &mut ProductState) -> Result<(), WorkspaceError> {
    let inputs = state
        .sources
        .iter()
        .map(WorkspaceSource::input)
        .collect::<Vec<_>>();

    let compilation = state
        .compilation
        .updated_sources(inputs)
        .map_err(|_| WorkspaceError::Compilation)?;

    state.compilation = Arc::new(compilation);

    Ok(())
}

fn document_snapshot(
    state: &ProductState,
    identity: SourceIdentity,
    uri: String,
    revision: u64,
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
    })
}

fn workspace_source(source: &SourceSnapshot) -> Result<WorkspaceSource, WorkspaceError> {
    let uri = source
        .origin()
        .document_uri()
        .map_err(|_| WorkspaceError::InvalidDocumentUri)?
        .ok_or(WorkspaceError::InvalidDocumentUri)?;

    let path = source
        .origin()
        .document_file_path()
        .map_err(|_| WorkspaceError::InvalidDocumentUri)?;

    Ok(WorkspaceSource {
        identity: source.identity(),
        path,
        uri,
        version: source.version(),
        text: source.text().to_owned(),
        is_open: false,
    })
}

fn source_index_for_uri(
    sources: &[WorkspaceSource],
    uri: &str,
) -> Option<usize> {
    let path = SourceOrigin::path_from_document_uri(uri).ok().flatten();

    sources.iter().position(|source| {
        source.uri == uri
            || path
                .as_ref()
                .zip(source.path.as_ref())
                .is_some_and(|(left, right)| {
                    absolute_path(left).ok() == absolute_path(right).ok()
                })
    })
}

fn append_open_source<'source>(
    sources: &mut Vec<WorkspaceSource>,
    uri: &str,
) -> Option<usize> {
    let next_identity = sources
        .iter()
        .map(|source| source.identity.raw())
        .max()
        .unwrap_or(0)
        .checked_add(1)?;

    if next_identity >= 1 << 31 {
        return None;
    }

    sources.push(WorkspaceSource {
        identity: SourceIdentity::new(next_identity),
        path: SourceOrigin::path_from_document_uri(uri).ok().flatten(),
        uri: uri.to_owned(),
        version: SourceVersion::new(0),
        text: String::new(),
        is_open: true,
    });

    sources.len().checked_sub(1)
}

fn apply_changes(
    source: &mut WorkspaceSource,
    version: SourceVersion,
    changes: &[ContentChange],
) -> Result<(), WorkspaceError> {
    for change in changes {
        let Some(range) = change.range else {
            source.text.clone_from(&change.text);

            continue;
        };

        let index = LineIndex::new(&source.text).map_err(|_| WorkspaceError::SourceTooLarge)?;

        let range = index
            .text_range_for_lsp_range(
                LspPosition::new(range.start.line, range.start.character),
                LspPosition::new(range.end.line, range.end.character),
            )
            .ok_or(WorkspaceError::InvalidEdit)?;

        let snapshot = SourceSnapshot::new(
            SourceId::new(0),
            source.identity,
            SourceOrigin::lsp_document(&source.uri),
            source.version,
            source.text.as_str(),
        )
        .map_err(|_| WorkspaceError::SourceTooLarge)?;

        let edit = SourceEdit::new(range, change.text.as_str());

        source.text = snapshot
            .apply_edit(SourceId::new(0), version, &edit)
            .map_err(|_| WorkspaceError::InvalidEdit)?
            .text()
            .to_owned();
    }

    source.version = version;

    Ok(())
}

fn source_version(version: i64) -> Result<SourceVersion, WorkspaceError> {
    let version = u64::try_from(version).map_err(|_| WorkspaceError::InvalidVersion)?;

    Ok(SourceVersion::new(version))
}

fn absolute_path(path: &Path) -> Result<PathBuf, WorkspaceError> {
    std::path::absolute(path).map_err(|_| WorkspaceError::InvalidDocumentUri)
}

impl WorkspaceSource {
    fn input(&self) -> SourceInput {
        if self.is_open {
            return SourceInput::lsp_open_document(
                self.identity,
                self.uri.clone(),
                self.version,
                self.text.clone(),
            );
        }

        match &self.path {
            Some(path) => SourceInput::file(
                self.identity,
                path.clone(),
                self.version,
                self.text.clone(),
            ),
            None => SourceInput::lsp_open_document(
                self.identity,
                self.uri.clone(),
                self.version,
                self.text.clone(),
            ),
        }
    }
}

pub(crate) fn offset_for_position(
    source: &SourceSnapshot,
    position: Position,
) -> Result<bray_source::TextSize, WorkspaceError> {
    LineIndex::new(source.text())
        .map_err(|_| WorkspaceError::SourceTooLarge)?
        .offset_for_lsp_position(LspPosition::new(position.line, position.character))
        .ok_or(WorkspaceError::InvalidEdit)
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

        let uri = bray_source::SourceOrigin::file_uri_from_path(&fixture.source)
            .unwrap_or_else(|error| panic!("test source URI should form: {error:?}"));

        let mut workspace = Workspace::new(
            fixture.path(),
            Arc::new(graph),
            WorkerBudget::new(2)
                .unwrap_or_else(|error| panic!("test worker budget should form: {error:?}")),
        );

        let opened = workspace
            .open_document(uri.clone(), 1, fixture.source_text.to_owned())
            .unwrap_or_else(|error| panic!("test document should open: {error:?}"));

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
            .unwrap_or_else(|error| panic!("test document should change: {error:?}"));

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

        assert!(!workspace.is_current(&uri, opened.revision));
        assert!(workspace.is_current(&uri, changed.revision));
    }
}
