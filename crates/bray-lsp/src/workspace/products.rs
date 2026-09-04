use super::WorkspaceError;
use super::sources::{WorkspaceSource, absolute_path, workspace_source};
use super::state::{ProductState, Workspace};
use bray_compilation::{
    Compilation, CompilationOptions, CompilationRequest, DependencyInterfaceInput,
};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
    encode_package_interface,
};
use bray_project::ProjectProduct;
use bray_source::SourceOrigin;
use bray_symbols::ProductIdentity;
use bray_tooling::{
    compilation_request_from_file_arguments, package_interface_export_request,
    project_interface_path, selected_target,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

impl Workspace {
    pub(super) fn product_for_uri(&self, uri: &str) -> Result<&ProjectProduct, WorkspaceError> {
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

    pub(super) fn ensure_product_loaded(
        &mut self,
        identity: &ProductIdentity,
    ) -> Result<(), WorkspaceError> {
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

    pub(super) fn rebuild_products_from(
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
                    .ok_or_else(|| WorkspaceError::DependencyExportUnavailable(identity.clone()))?
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
}
