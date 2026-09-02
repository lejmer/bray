use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_compilation::{
    Compilation, CompilationOptions, CompilationRequest, DependencyInterfaceInput, WorkerBudget,
};
use bray_messages::LanguageServerMessage;
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
    encode_package_interface,
};
use bray_project::{ProjectGraph, ProjectProduct};
use bray_source::{
    LineIndex, LspPosition, SourceEdit, SourceId, SourceIdentity, SourceInput, SourceOrigin,
    SourceSnapshot, SourceVersion, SourceEditError, SourceUriError, TextSizeOverflow,
};
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;
use bray_tooling::{
    compilation_request_from_file_arguments, package_interface_export_request,
    project_interface_path, selected_target,
};

use crate::model::{ContentChange, Position};

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
    root: PathBuf,
    graph: Arc<ProjectGraph>,
    target: TargetIdentity,
    worker_budget: WorkerBudget,
    products: BTreeMap<ProductIdentity, ProductState>,
    documents: BTreeMap<String, DocumentOwner>,
    next_generation: u64,
}

struct ProductState {
    compilation: Arc<Compilation>,
    sources: Vec<WorkspaceSource>,
    generation: u64,
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

#[derive(Debug)]
pub(crate) enum WorkspaceError {
    SourceRead {
        path: PathBuf,
        cause: std::io::Error,
    },
    CompilationRequest(bray_diagnostics::DiagnosticBag),
    CompilationLoad(bray_compilation::CompilationLoadError),
    CompilationOrder {
        pending: Vec<ProductIdentity>,
    },
    GenerationExhausted {
        current: u64,
    },
    DocumentNotFound,
    InvalidDocumentUri { uri: String, cause: SourceUriError },
    InvalidSourceOrigin { origin: SourceOrigin, cause: SourceUriError },
    InvalidDocumentPath { path: PathBuf, cause: std::io::Error },
    MissingDocumentUri,
    InvalidEdit,
    SourceEdit(SourceEditError),
    InvalidVersion { actual: i64 },
    NonIncreasingVersion { current: u64, actual: u64 },
    VersionExhausted { current: u64 },
    SourceIdentityExhausted { current: u32 },
    ProductNotFound,
    SourceTooLarge(TextSizeOverflow),
    UnsupportedTarget,
    InvalidDependencyIdentity(ProductIdentity),
    MissingDependencyPath(ProductIdentity),
    DependencyNotLoaded(ProductIdentity),
    DependencyExportUnavailable(ProductIdentity),
    DependencyExport {
        product: ProductIdentity,
        cause: Box<bray_compilation::PackageInterfaceExportError>,
    },
    DependencyEncoding {
        product: ProductIdentity,
        cause: bray_package_interface::InterfaceValidationError,
    },
}

impl WorkspaceError {
    pub(crate) const fn message(&self) -> LanguageServerMessage {
        match self {
            Self::SourceRead { .. }
            | Self::CompilationRequest(_)
            | Self::CompilationLoad(_)
            | Self::CompilationOrder { .. }
            | Self::GenerationExhausted { .. } => LanguageServerMessage::CompilationFailed,
            Self::DocumentNotFound => LanguageServerMessage::DocumentNotFound,
            Self::InvalidDocumentUri { .. }
            | Self::InvalidSourceOrigin { .. }
            | Self::InvalidDocumentPath { .. }
            | Self::MissingDocumentUri => LanguageServerMessage::InvalidDocumentUri,
            Self::InvalidEdit | Self::SourceEdit(_) => LanguageServerMessage::InvalidDocumentEdit,
            Self::InvalidVersion { .. }
            | Self::NonIncreasingVersion { .. }
            | Self::VersionExhausted { .. } => LanguageServerMessage::InvalidDocumentVersion,
            Self::SourceIdentityExhausted { .. } => LanguageServerMessage::CompilationFailed,
            Self::ProductNotFound => LanguageServerMessage::ProductNotFound,
            Self::SourceTooLarge(_) => LanguageServerMessage::SourceTooLarge,
            Self::UnsupportedTarget => LanguageServerMessage::UnsupportedTarget,
            Self::InvalidDependencyIdentity(_)
            | Self::MissingDependencyPath(_)
            | Self::DependencyNotLoaded(_)
            | Self::DependencyExportUnavailable(_)
            | Self::DependencyExport { .. }
            | Self::DependencyEncoding { .. } => LanguageServerMessage::DependencyUnavailable,
        }
    }

    pub(crate) fn data(&self) -> serde_json::Value {
        use serde_json::json;

        match self {
            Self::SourceRead { path, cause } => json!({
                "reason": "source_read",
                "path": format!("{path:?}"),
                "cause": cause.to_string(),
            }),
            Self::CompilationRequest(diagnostics) => json!({
                "reason": "compilation_request",
                "cause": format!("{diagnostics:?}"),
            }),
            Self::CompilationLoad(cause) => json!({
                "reason": "compilation_load",
                "cause": format!("{cause:?}"),
            }),
            Self::CompilationOrder { pending } => json!({
                "reason": "compilation_order",
                "pending": pending.iter().map(|value| format!("{value:?}")).collect::<Vec<_>>(),
            }),
            Self::GenerationExhausted { current } => json!({
                "reason": "generation_exhausted",
                "current": current,
            }),
            Self::DocumentNotFound => json!({ "reason": "document_not_found" }),
            Self::InvalidDocumentUri { uri, cause } => json!({
                "reason": "invalid_document_uri",
                "uri": uri,
                "cause": format!("{cause:?}"),
            }),
            Self::InvalidSourceOrigin { origin, cause } => json!({
                "reason": "invalid_source_origin",
                "origin": format!("{origin:?}"),
                "cause": format!("{cause:?}"),
            }),
            Self::InvalidDocumentPath { path, cause } => json!({
                "reason": "invalid_document_path",
                "path": format!("{path:?}"),
                "cause": cause.to_string(),
            }),
            Self::MissingDocumentUri => json!({ "reason": "missing_document_uri" }),
            Self::InvalidEdit => json!({ "reason": "invalid_edit" }),
            Self::SourceEdit(cause) => json!({
                "reason": "source_edit",
                "cause": format!("{cause:?}"),
            }),
            Self::InvalidVersion { actual } => json!({
                "reason": "invalid_version",
                "actual": actual,
            }),
            Self::NonIncreasingVersion { current, actual } => json!({
                "reason": "non_increasing_version",
                "current": current,
                "actual": actual,
            }),
            Self::VersionExhausted { current } => json!({
                "reason": "version_exhausted",
                "current": current,
            }),
            Self::SourceIdentityExhausted { current } => json!({
                "reason": "source_identity_exhausted",
                "current": current,
            }),
            Self::ProductNotFound => json!({ "reason": "product_not_found" }),
            Self::SourceTooLarge(cause) => json!({
                "reason": "source_too_large",
                "actual_bytes": cause.bytes(),
            }),
            Self::UnsupportedTarget => json!({ "reason": "unsupported_target" }),
            Self::InvalidDependencyIdentity(product) => json!({
                "reason": "invalid_dependency_identity",
                "product": format!("{product:?}"),
            }),
            Self::MissingDependencyPath(product) => json!({
                "reason": "missing_dependency_path",
                "product": format!("{product:?}"),
            }),
            Self::DependencyNotLoaded(product) => json!({
                "reason": "dependency_not_loaded",
                "product": format!("{product:?}"),
            }),
            Self::DependencyExportUnavailable(product) => json!({
                "reason": "dependency_export_unavailable",
                "product": format!("{product:?}"),
            }),
            Self::DependencyExport { product, cause } => json!({
                "reason": "dependency_export",
                "product": format!("{product:?}"),
                "cause": format!("{cause:?}"),
            }),
            Self::DependencyEncoding { product, cause } => json!({
                "reason": "dependency_encoding",
                "product": format!("{product:?}"),
                "cause": format!("{cause:?}"),
            }),
        }
    }
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

    fn product_for_uri(&self, uri: &str) -> Result<&ProjectProduct, WorkspaceError> {
        let document_path = SourceOrigin::path_from_document_uri(uri).map_err(|cause| {
            WorkspaceError::InvalidDocumentUri {
                uri: uri.to_owned(),
                cause,
            }
        })?;

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
            .filter(|package| package.role() == bray_project::PackageRole::Root)
            .flat_map(|package| package.products())
            .next()
            .ok_or(WorkspaceError::ProductNotFound)
    }

    fn ensure_product_loaded(&mut self, identity: &ProductIdentity) -> Result<(), WorkspaceError> {
        if self.products.contains_key(identity) {
            return Ok(());
        }

        let product = self
            .project_product(identity)
            .cloned()
            .ok_or(WorkspaceError::ProductNotFound)?;

        for dependency in self.product_dependencies(&product)? {
            self.ensure_product_loaded(&dependency)?;
        }

        let state = self.load_product(product)?;

        self.products.insert(identity.clone(), state);

        Ok(())
    }

    fn load_product(&mut self, product: ProjectProduct) -> Result<ProductState, WorkspaceError> {
        let selected_target =
            selected_target(&self.target).ok_or(WorkspaceError::UnsupportedTarget)?;

        if !product.targets().contains(&self.target) {
            return Err(WorkspaceError::UnsupportedTarget);
        }

        let files = product
            .sources()
            .iter()
            .map(|source| source.beneath(&self.root))
            .collect::<Vec<_>>();

        let options =
            CompilationOptions::new(self.worker_budget, product.kind(), selected_target.clone());

        let mut request = compilation_request_from_file_arguments(
            product.identity().package().clone(),
            files,
            options,
        )
        .map_err(WorkspaceError::CompilationRequest)?;

        request = self.configure_request(request, &product)?;

        let compilation = Compilation::load(request).map_err(WorkspaceError::CompilationLoad)?;

        let sources = compilation
            .sources()
            .iter()
            .map(workspace_source)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ProductState {
            compilation: Arc::new(compilation),
            sources,
            generation: self.next_generation()?,
        })
    }

    fn rebuild_products_from(
        &mut self,
        changed: &ProductIdentity,
    ) -> Result<BTreeSet<ProductIdentity>, WorkspaceError> {
        let loaded = self.products.keys().cloned().collect::<BTreeSet<_>>();

        let products = self
            .graph
            .packages()
            .iter()
            .flat_map(|package| package.products())
            .filter(|product| loaded.contains(product.identity()))
            .map(|product| (product.identity().clone(), product.clone()))
            .collect::<BTreeMap<_, _>>();

        let dependencies = products
            .iter()
            .map(|(identity, product)| {
                self.product_dependencies(product)
                    .map(|dependencies| (identity.clone(), dependencies))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        let mut affected = BTreeSet::from([changed.clone()]);

        loop {
            let next = dependencies
                .iter()
                .filter(|(identity, _)| !affected.contains(*identity))
                .filter(|(_, dependencies)| {
                    dependencies
                        .iter()
                        .any(|dependency| affected.contains(dependency))
                })
                .map(|(identity, _)| identity.clone())
                .collect::<Vec<_>>();

            if next.is_empty() {
                break;
            }

            affected.extend(next);
        }

        let mut pending = affected.clone();
        let mut rebuilt = BTreeSet::new();

        while !pending.is_empty() {
            let identity = pending
                .iter()
                .find(|identity| {
                    dependencies
                        .get(*identity)
                        .into_iter()
                        .flatten()
                        .filter(|dependency| affected.contains(*dependency))
                        .all(|dependency| rebuilt.contains(dependency))
                })
                .cloned()
                .ok_or_else(|| WorkspaceError::CompilationOrder {
                    pending: pending.iter().cloned().collect(),
                })?;

            let product = products
                .get(&identity)
                .ok_or(WorkspaceError::ProductNotFound)?;

            self.rebuild_product(product)?;

            pending.remove(&identity);
            rebuilt.insert(identity);
        }

        Ok(affected)
    }

    fn rebuild_product(&mut self, product: &ProjectProduct) -> Result<(), WorkspaceError> {
        let identity = product.identity().clone();

        let state = self
            .products
            .get(&identity)
            .ok_or(WorkspaceError::ProductNotFound)?;

        let sources = state
            .sources
            .iter()
            .map(WorkspaceSource::input)
            .collect::<Vec<_>>();

        let options = CompilationOptions::new(
            self.worker_budget,
            product.kind(),
            selected_target(&self.target).ok_or(WorkspaceError::UnsupportedTarget)?,
        );

        let request =
            CompilationRequest::with_options(identity.package().clone(), sources, options);

        let request = self.configure_request(request, product)?;

        let previous = self
            .products
            .get(&identity)
            .ok_or(WorkspaceError::ProductNotFound)?
            .compilation
            .as_ref();

        let compilation = previous
            .updated(request)
            .map_err(WorkspaceError::CompilationLoad)?;

        let generation = self.next_generation()?;

        let state = self
            .products
            .get_mut(&identity)
            .ok_or(WorkspaceError::ProductNotFound)?;

        state.compilation = Arc::new(compilation);
        state.generation = generation;

        Ok(())
    }

    fn configure_request(
        &self,
        mut request: CompilationRequest,
        product: &ProjectProduct,
    ) -> Result<CompilationRequest, WorkspaceError> {
        request = request.with_dependency_interfaces(self.dependency_interfaces(product)?);

        if product.kind() == bray_symbols::ProductKind::Library {
            let Some(package) = self.graph.package(product.identity().package()) else {
                return Err(WorkspaceError::ProductNotFound);
            };

            request = request.with_package_interface_export(package_interface_export_request(
                product.identity().clone(),
                package.version(),
            ));
        }

        Ok(request)
    }

    fn dependency_interfaces(
        &self,
        product: &ProjectProduct,
    ) -> Result<Vec<DependencyInterfaceInput>, WorkspaceError> {
        product
            .dependencies()
            .iter()
            .filter(|dependency| dependency.is_active_for(&self.target))
            .map(|dependency| {
                let identity = dependency.product();

                let interface_product = InterfaceProductIdentity::try_new(identity.name())
                    .ok_or_else(|| WorkspaceError::InvalidDependencyIdentity(identity.clone()))?;

                let path = project_interface_path(&self.graph, &self.root, identity, &self.target)
                    .ok_or_else(|| WorkspaceError::MissingDependencyPath(identity.clone()))?;

                let dependency = self
                    .products
                    .get(identity)
                    .ok_or_else(|| WorkspaceError::DependencyNotLoaded(identity.clone()))?;

                let bundle = dependency
                    .compilation
                    .package_interface_export_bundle()
                    .ok_or_else(|| {
                        WorkspaceError::DependencyExportUnavailable(identity.clone())
                    })?
                    .as_ref()
                    .map_err(|cause| WorkspaceError::DependencyExport {
                        product: identity.clone(),
                        cause: Box::new(cause.clone()),
                    })?;

                let bytes = encode_package_interface(bundle)
                    .map_err(|cause| WorkspaceError::DependencyEncoding {
                        product: identity.clone(),
                        cause,
                    })?
                    .shared_bytes();

                Ok(DependencyInterfaceInput::new(
                    identity.package().clone(),
                    interface_product,
                    path,
                    bytes,
                    InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
                ))
            })
            .collect()
    }

    fn project_product(&self, identity: &ProductIdentity) -> Option<&ProjectProduct> {
        self.graph
            .packages()
            .iter()
            .flat_map(|package| package.products())
            .find(|product| product.identity() == identity)
    }

    fn product_dependencies(
        &self,
        product: &ProjectProduct,
    ) -> Result<Vec<ProductIdentity>, WorkspaceError> {
        Ok(product
            .dependencies()
            .iter()
            .filter(|dependency| dependency.is_active_for(&self.target))
            .map(|dependency| dependency.product().clone())
            .collect())
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

    fn next_generation(&mut self) -> Result<u64, WorkspaceError> {
        self.next_generation = self
            .next_generation
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

fn workspace_source(source: &SourceSnapshot) -> Result<WorkspaceSource, WorkspaceError> {
    let uri = source
        .origin()
        .document_uri()
        .map_err(|cause| WorkspaceError::InvalidSourceOrigin {
            origin: source.origin().clone(),
            cause,
        })?
        .ok_or(WorkspaceError::MissingDocumentUri)?;

    let path = source
        .origin()
        .document_file_path()
        .map_err(|cause| WorkspaceError::InvalidSourceOrigin {
            origin: source.origin().clone(),
            cause,
        })?;

    Ok(WorkspaceSource {
        identity: source.identity(),
        path,
        uri,
        version: source.version(),
        text: source.text().to_owned(),
        is_open: false,
    })
}

fn source_index_for_uri(sources: &[WorkspaceSource], uri: &str) -> Option<usize> {
    let path = SourceOrigin::path_from_document_uri(uri).ok().flatten();

    sources.iter().position(|source| {
        source.uri == uri
            || path
                .as_ref()
                .zip(source.path.as_ref())
                .is_some_and(|(left, right)| absolute_path(left).ok() == absolute_path(right).ok())
    })
}

fn append_open_source(
    sources: &mut Vec<WorkspaceSource>,
    uri: &str,
) -> Result<usize, WorkspaceError> {
    let next_identity = sources
        .iter()
        .map(|source| source.identity.raw())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(WorkspaceError::SourceIdentityExhausted { current: u32::MAX })?;

    if next_identity >= 1 << 31 {
        return Err(WorkspaceError::SourceIdentityExhausted {
            current: next_identity,
        });
    }

    sources.push(WorkspaceSource {
        identity: SourceIdentity::new(next_identity),
        path: SourceOrigin::path_from_document_uri(uri).ok().flatten(),
        uri: uri.to_owned(),
        version: SourceVersion::new(0),
        text: String::new(),
        is_open: true,
    });

    sources
        .len()
        .checked_sub(1)
        .ok_or(WorkspaceError::SourceIdentityExhausted {
            current: next_identity,
        })
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

        let index = LineIndex::new(&source.text).map_err(WorkspaceError::SourceTooLarge)?;

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
        .map_err(WorkspaceError::SourceTooLarge)?;

        let edit = SourceEdit::new(range, change.text.as_str());

        source.text = snapshot
            .apply_edit(SourceId::new(0), version, &edit)
            .map_err(WorkspaceError::SourceEdit)?
            .text()
            .to_owned();
    }

    source.version = version;

    Ok(())
}

fn source_version(version: i64) -> Result<SourceVersion, WorkspaceError> {
    let version = u64::try_from(version)
        .map_err(|_| WorkspaceError::InvalidVersion { actual: version })?;

    Ok(SourceVersion::new(version))
}

fn absolute_path(path: &Path) -> Result<PathBuf, WorkspaceError> {
    std::path::absolute(path).map_err(|cause| WorkspaceError::InvalidDocumentPath {
        path: path.to_path_buf(),
        cause,
    })
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
            Some(path) => {
                SourceInput::file(self.identity, path.clone(), self.version, self.text.clone())
            }
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
        .map_err(WorkspaceError::SourceTooLarge)?
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
