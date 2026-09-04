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
