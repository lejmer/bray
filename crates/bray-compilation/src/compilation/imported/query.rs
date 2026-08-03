use std::sync::Arc;

use bray_diagnostics::{DiagnosticBag, DiagnosticKind, DiagnosticResult};
use bray_package_interface::{
    ImportedInterfaceSymbolResolver, ImportedSemanticFact, ImportedSemanticFacts,
    LoadedInterfaceSurface, PackageInterfaceSurface, ValidatedPackageInterface,
    construct_imported_symbol_skeletons,
};
use bray_symbols::{ImportedInterfaceId, ImportedSymbolSkeleton, PackageIdentity, SymbolId};

use super::diagnostic::{
    interface_diagnostics, standard_library_diagnostics, unlocated_interface_diagnostics,
    validation_diagnostics,
};
use super::model::LoadedDependencyInterface;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, ImportedSemanticFactKey};
use crate::request::DependencyInterfaceInput;

impl super::super::Compilation {
    /// Returns structured diagnostics for one failed standard library artifact request.
    pub fn standard_library_load_diagnostics(
        &self,
        error: &bray_standard_library::StandardLibraryLoadError,
    ) -> DiagnosticBag {
        let input = self.state.dependency_interfaces.iter().find(|input| {
            input.package().as_str()
                == bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY
        });

        input.map_or_else(
            || unlocated_interface_diagnostics(DiagnosticKind::StandardLibraryManifestInvalid),
            |input| standard_library_diagnostics(error.clone(), input),
        )
    }

    /// Resolves the canonical compilation-local handle for one selected dependency interface.
    pub fn dependency_interface_id(
        &self,
        package: &PackageIdentity,
        product: &bray_package_interface::InterfaceProductIdentity,
    ) -> Option<ImportedInterfaceId> {
        self.state
            .dependency_interfaces
            .binary_search_by(|input| (input.package(), input.product()).cmp(&(package, product)))
            .ok()
            .and_then(ImportedInterfaceId::try_from_index)
    }

    /// Returns a dependency's validated identity surface and diagnostics.
    pub fn dependency_interface_result(
        &self,
        interface: ImportedInterfaceId,
    ) -> Option<&DiagnosticResult<Option<PackageInterfaceSurface>>> {
        self.loaded_dependency_interface(interface)
            .map(LoadedDependencyInterface::result)
    }

    /// Returns the deterministic imported identity skeleton.
    pub fn imported_symbol_skeleton_result(
        &self,
    ) -> Result<&DiagnosticResult<Option<Arc<ImportedSymbolSkeleton>>>, FactQueryError> {
        self.imported_symbol_skeleton_result_with_cancellation(&self.state.cancellation)
    }

    /// Returns one exact imported symbol-owned fact category.
    pub fn imported_semantic_fact_result(
        &self,
        key: ImportedSemanticFactKey,
    ) -> Result<Arc<DiagnosticResult<Arc<[ImportedSemanticFact]>>>, FactQueryError> {
        self.imported_semantic_fact_result_with_cancellation(key, &self.state.cancellation)
    }

    pub(in crate::compilation) fn imported_symbol_skeleton_result_with_cancellation(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticResult<Option<Arc<ImportedSymbolSkeleton>>>, FactQueryError> {
        self.query_fact_with_cancellation(
            CompilationFactKey::ImportedSymbolSkeleton,
            &self.state.imported_symbol_skeleton,
            cancellation,
            |cancellation| self.compute_imported_symbol_skeleton(cancellation),
        )
    }

    pub(super) fn imported_semantic_graph_result_with_cancellation(
        &self,
        interface: ImportedInterfaceId,
        cancellation: &CancellationToken,
    ) -> Result<Option<&DiagnosticResult<Option<Arc<ImportedSemanticFacts>>>>, FactQueryError> {
        let Some(index) = interface.to_index() else {
            return Ok(None);
        };

        let Some(cache) = self.state.imported_semantic_graphs.get(index) else {
            return Ok(None);
        };

        self.query_fact_with_cancellation(
            CompilationFactKey::ImportedSemanticGraph(interface),
            cache,
            cancellation,
            |cancellation| self.compute_imported_semantic_graph(interface, cancellation),
        )
        .map(Some)
    }

    pub(in crate::compilation) fn imported_semantic_fact_result_with_cancellation(
        &self,
        key: ImportedSemanticFactKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Arc<[ImportedSemanticFact]>>>, FactQueryError> {
        let cell = self.state.imported_semantic_facts.cell(key)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::ImportedSemanticFact(key),
            cancellation,
            || {
                self.compute_imported_semantic_fact(key, cancellation)
                    .map(Arc::new)
            },
        )?;

        // Exact fact results are Arc-backed so callers do not retain the cache-cell map lock.
        Ok(Arc::clone(result))
    }

    /// Returns diagnostics owned by all selected compiled dependency interfaces.
    pub fn imported_diagnostics(&self) -> &DiagnosticBag {
        self.fact(
            CompilationFactKey::ImportedDiagnostics,
            &self.state.imported_diagnostics,
            || self.compute_imported_diagnostics(),
        )
    }

    fn loaded_dependency_interface(
        &self,
        interface: ImportedInterfaceId,
    ) -> Option<&LoadedDependencyInterface> {
        match self
            .loaded_dependency_interface_with_cancellation(interface, &self.state.cancellation)
        {
            Ok(loaded) => loaded,
            Err(FactQueryError::Cancelled) => {
                panic!("uncancellable dependency-interface query was cancelled")
            }
            Err(FactQueryError::Cycle(cycle)) => {
                panic!("dependency-interface query formed a cycle: {cycle:?}")
            }
            Err(FactQueryError::InfrastructureFailure) => {
                panic!("dependency-interface query infrastructure failed")
            }
            Err(FactQueryError::SemanticUnitContext(error)) => {
                panic!("semantic semantic unit context failed: {error:?}")
            }
            Err(FactQueryError::CheckerInfrastructure(error)) => {
                panic!("semantic checker infrastructure failed: {error:?}")
            }
        }
    }

    pub(super) fn loaded_dependency_interface_with_cancellation(
        &self,
        interface: ImportedInterfaceId,
        cancellation: &CancellationToken,
    ) -> Result<Option<&LoadedDependencyInterface>, FactQueryError> {
        let Some(index) = interface.to_index() else {
            return Ok(None);
        };

        let Some(input) = self.state.dependency_interfaces.get(index) else {
            return Ok(None);
        };

        let Some(cache) = self.state.loaded_dependency_interfaces.get(index) else {
            return Ok(None);
        };

        self.query_fact_with_cancellation(
            CompilationFactKey::DependencyInterface(interface),
            cache,
            cancellation,
            |cancellation| load_dependency_interface(input, cancellation),
        )
        .map(Some)
    }

    pub(in crate::compilation) fn dependency_interface_input(
        &self,
        interface: ImportedInterfaceId,
    ) -> Option<&DependencyInterfaceInput> {
        interface
            .to_index()
            .and_then(|index| self.state.dependency_interfaces.get(index))
    }

    fn compute_imported_symbol_skeleton(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<Arc<ImportedSymbolSkeleton>>>, FactQueryError> {
        let mut loaded = Vec::with_capacity(self.state.dependency_interfaces.len());
        let mut diagnostics = Vec::new();

        for index in 0..self.state.dependency_interfaces.len() {
            cancellation.check()?;

            let Some(interface) = ImportedInterfaceId::try_from_index(index) else {
                return Ok(DiagnosticResult::new(
                    None,
                    dependency_graph_diagnostics(self),
                ));
            };

            let Some(fact) =
                self.loaded_dependency_interface_with_cancellation(interface, cancellation)?
            else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            diagnostics.push(fact.result().diagnostics());

            let (Some(validated), Some(surface)) = (fact.validated(), fact.surface()) else {
                continue;
            };

            loaded.push(LoadedInterfaceSurface::new(
                interface,
                validated.header().content_hash(),
                surface,
            ));
        }

        let diagnostics = DiagnosticBag::merged_all(diagnostics);

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(None, diagnostics));
        }

        let graph = self.symbol_graph()?;

        let Some(first_symbol) = u32::try_from(graph.next_symbol_index())
            .ok()
            .map(SymbolId::new)
        else {
            return Ok(DiagnosticResult::new(
                None,
                dependency_graph_diagnostics(self),
            ));
        };

        cancellation.check()?;

        match construct_imported_symbol_skeletons(first_symbol, loaded) {
            Ok(symbols) => Ok(DiagnosticResult::new(Some(Arc::new(symbols)), diagnostics)),
            Err(_) => Ok(DiagnosticResult::new(
                None,
                dependency_graph_diagnostics(self),
            )),
        }
    }

    fn compute_imported_semantic_graph(
        &self,
        interface: ImportedInterfaceId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<Arc<ImportedSemanticFacts>>>, FactQueryError> {
        let Some(loaded) =
            self.loaded_dependency_interface_with_cancellation(interface, cancellation)?
        else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let Some(input) = self.dependency_interface_input(interface) else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let (Some(validated), Some(surface)) = (loaded.validated(), loaded.surface()) else {
            return Ok(DiagnosticResult::without_diagnostics(None));
        };

        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(skeleton) = skeleton.value() else {
            return Ok(DiagnosticResult::without_diagnostics(None));
        };

        cancellation.check()?;

        let decoded = match validated.decode_semantic_facts(surface) {
            Ok(decoded) => decoded,
            Err(error) => {
                return Ok(DiagnosticResult::new(
                    None,
                    validation_diagnostics(error, input),
                ));
            }
        };

        self.intern_imported_semantic_facts(interface, input, decoded, skeleton, cancellation)
    }

    fn compute_imported_semantic_fact(
        &self,
        key: ImportedSemanticFactKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Arc<[ImportedSemanticFact]>>, FactQueryError> {
        let graph = self.compute_selected_imported_semantic_graph(key, cancellation)?;

        let Some(graph) = graph.value() else {
            // The exact fact owns diagnostics so it can publish independently of the graph cache.
            return Ok(DiagnosticResult::new(
                Arc::from([]),
                graph.diagnostics().clone(),
            ));
        };

        let loaded = self
            .loaded_dependency_interface_with_cancellation(key.interface(), cancellation)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let surface = loaded
            .surface()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let identity = surface
            .symbols()
            .symbol(key.owner())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let owner = skeleton
            .value()
            .as_ref()
            .and_then(|skeleton| skeleton.symbol_by_external_key(identity.key()))
            .ok_or(FactQueryError::InfrastructureFailure)?;

        cancellation.check()?;

        Ok(DiagnosticResult::without_diagnostics(
            graph.symbol_facts(owner, key.kind()).into(),
        ))
    }

    fn compute_selected_imported_semantic_graph(
        &self,
        key: ImportedSemanticFactKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<Arc<ImportedSemanticFacts>>>, FactQueryError> {
        let loaded = self
            .loaded_dependency_interface_with_cancellation(key.interface(), cancellation)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let input = self
            .dependency_interface_input(key.interface())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let (Some(validated), Some(surface)) = (loaded.validated(), loaded.surface()) else {
            return Ok(DiagnosticResult::without_diagnostics(None));
        };

        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(skeleton) = skeleton.value() else {
            return Ok(DiagnosticResult::without_diagnostics(None));
        };

        cancellation.check()?;

        let decoded = match validated.decode_semantic_fact_graph(surface, key.owner(), key.kind()) {
            Ok(decoded) => decoded,
            Err(error) => {
                return Ok(DiagnosticResult::new(
                    None,
                    validation_diagnostics(error, input),
                ));
            }
        };

        self.intern_imported_semantic_facts(key.interface(), input, decoded, skeleton, cancellation)
    }

    fn intern_imported_semantic_facts(
        &self,
        interface: ImportedInterfaceId,
        input: &DependencyInterfaceInput,
        decoded: bray_package_interface::InterfaceSemanticFacts,
        skeleton: &ImportedSymbolSkeleton,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<Arc<ImportedSemanticFacts>>>, FactQueryError> {
        cancellation.check()?;

        let interfaces = self
            .loaded_interface_views(cancellation)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let Some(current) = interfaces
            .iter()
            .copied()
            .find(|loaded| loaded.interface() == interface)
        else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let symbols = self.symbol_graph()?;

        let resolver = ImportedInterfaceSymbolResolver::try_new(
            current,
            interfaces,
            skeleton,
            symbols.compiler_known_provider().symbol_keys(),
        )
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let semantic_values = self.semantic_value_store()?;

        cancellation.check()?;

        match decoded.intern(semantic_values, &resolver) {
            Ok(facts) => Ok(DiagnosticResult::without_diagnostics(Some(Arc::new(facts)))),
            Err(_) => Ok(DiagnosticResult::new(
                None,
                semantic_facts_diagnostics(input),
            )),
        }
    }

    pub(in crate::compilation) fn loaded_interface_views(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<Option<Vec<LoadedInterfaceSurface<'_>>>, FactQueryError> {
        let mut interfaces = Vec::with_capacity(self.state.dependency_interfaces.len());

        for index in 0..self.state.dependency_interfaces.len() {
            cancellation.check()?;

            let interface = ImportedInterfaceId::try_from_index(index)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let loaded = self
                .loaded_dependency_interface_with_cancellation(interface, cancellation)?
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let (Some(validated), Some(surface)) = (loaded.validated(), loaded.surface()) else {
                return Ok(None);
            };

            interfaces.push(LoadedInterfaceSurface::new(
                interface,
                validated.header().content_hash(),
                surface,
            ));
        }

        Ok(Some(interfaces))
    }

    fn compute_imported_diagnostics(&self) -> DiagnosticBag {
        let skeleton = match self.imported_symbol_skeleton_result() {
            Ok(result) => result,
            Err(error) => panic!("imported symbol diagnostics failed: {error:?}"),
        };

        let mut diagnostics = vec![skeleton.diagnostics()];

        if skeleton.value().is_some() {
            for index in 0..self.state.dependency_interfaces.len() {
                let Some(interface) = ImportedInterfaceId::try_from_index(index) else {
                    return dependency_graph_diagnostics(self);
                };

                match self.imported_semantic_graph_result_with_cancellation(
                    interface,
                    &self.state.cancellation,
                ) {
                    Ok(Some(result)) => diagnostics.push(result.diagnostics()),
                    Ok(None) => {}
                    Err(error) => panic!("imported semantic diagnostics failed: {error:?}"),
                }
            }
        }

        DiagnosticBag::merged_all(diagnostics)
    }
}

fn load_dependency_interface(
    input: &DependencyInterfaceInput,
    cancellation: &CancellationToken,
) -> Result<LoadedDependencyInterface, FactQueryError> {
    cancellation.check()?;

    let bytes = match input.shared_bytes() {
        Ok(bytes) => bytes,
        Err(error) => {
            return Ok(LoadedDependencyInterface::invalid(
                standard_library_diagnostics(error, input),
            ));
        }
    };

    let validated = match ValidatedPackageInterface::try_new(bytes, input.validation_policy()) {
        Ok(validated) => validated,
        Err(error) => {
            return Ok(LoadedDependencyInterface::invalid(validation_diagnostics(
                error, input,
            )));
        }
    };

    cancellation.check()?;

    let surface = match validated.decode_identity_surface() {
        Ok(surface) => surface,
        Err(error) => {
            return Ok(LoadedDependencyInterface::invalid(validation_diagnostics(
                error, input,
            )));
        }
    };

    cancellation.check()?;

    if surface.identity().package() != input.package() {
        return Ok(LoadedDependencyInterface::invalid(interface_diagnostics(
            DiagnosticKind::InterfacePackageIdentityMismatch,
            input,
        )));
    }

    if surface.identity().product() != input.product() {
        return Ok(LoadedDependencyInterface::invalid(interface_diagnostics(
            DiagnosticKind::InterfaceProductIdentityMismatch,
            input,
        )));
    }

    Ok(LoadedDependencyInterface::new(validated, surface))
}

fn dependency_graph_diagnostics(compilation: &super::super::Compilation) -> DiagnosticBag {
    compilation.state.dependency_interfaces.first().map_or_else(
        || unlocated_interface_diagnostics(DiagnosticKind::InterfaceDependencyGraphInvalid),
        |input| interface_diagnostics(DiagnosticKind::InterfaceDependencyGraphInvalid, input),
    )
}

fn semantic_facts_diagnostics(input: &DependencyInterfaceInput) -> DiagnosticBag {
    interface_diagnostics(DiagnosticKind::InterfaceSemanticFactsInvalid, input)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use bray_bound_tree::CheckedTemplateKind;
    use bray_diagnostics::{DiagnosticArgName, DiagnosticArgValue, DiagnosticKind};
    use bray_package_interface::{
        ImportedSemanticFact, InterfaceLanguageRevision, InterfacePredicateDefinitionState,
        InterfaceProductIdentity, InterfaceSemanticFactKind, InterfaceValidationPolicy,
        test_support::encoded_semantic_test_interface,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::PackageIdentity;

    use crate::test_support::diagnostic_kinds;
    use crate::{
        CancellationToken, Compilation, CompilationRequest, DependencyInterfaceInput,
        FactQueryError, ImportedSemanticFactKey,
    };

    use super::{dependency_graph_diagnostics, semantic_facts_diagnostics};

    #[test]
    fn dependency_interface_validation_is_lazy_cached_and_diagnostic_backed() {
        let compilation = compilation([dependency("example.alpha", "main")]);

        assert!(
            compilation.state.loaded_dependency_interfaces[0]
                .get()
                .is_none()
        );

        assert!(compilation.state.imported_symbol_skeleton.get().is_none());

        let interface = interface_id(&compilation, "example.alpha", "main");

        let first = compilation
            .dependency_interface_result(interface)
            .unwrap_or_else(|| panic!("selected dependency interface must have a result"));

        let second = compilation
            .dependency_interface_result(interface)
            .unwrap_or_else(|| panic!("selected dependency interface must have a result"));

        assert!(std::ptr::eq(first, second));
        assert!(first.value().is_none());

        assert_eq!(
            diagnostic_kinds(first.diagnostics()),
            [DiagnosticKind::InterfaceInvalidMagic]
        );

        let skeleton = compilation
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("skeleton query must complete: {error:?}"));

        assert!(skeleton.value().is_none());

        assert_eq!(
            diagnostic_kinds(compilation.imported_diagnostics()),
            [DiagnosticKind::InterfaceInvalidMagic]
        );

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::InterfaceInvalidMagic)
        );
    }

    #[test]
    fn configured_standard_library_roots_are_loaded_only_when_imports_are_demanded() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary root must exist: {error}"));

        let root = bray_standard_library::StandardLibraryRoot::try_new(directory.path())
            .unwrap_or_else(|| panic!("temporary root must be absolute"));

        let package = PackageIdentity::try_new("example.application")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let request = CompilationRequest::new(
            package,
            vec![SourceInput::virtual_text(
                SourceIdentity::new(0),
                "source",
                SourceVersion::new(0),
                "module application;",
            )],
        )
        .with_standard_library_root(root);

        let compilation = Compilation::load(request)
            .unwrap_or_else(|error| panic!("I/O-free compilation load must succeed: {error:?}"));

        assert!(compilation.source_diagnostics().is_empty());

        let interface = interface_id(&compilation, "std", "library");

        let result = compilation
            .dependency_interface_result(interface)
            .unwrap_or_else(|| panic!("synthetic standard library dependency must exist"));

        assert_eq!(
            diagnostic_kinds(result.diagnostics()),
            [DiagnosticKind::StandardLibraryArtifactReadFailed]
        );
    }

    #[test]
    fn dependency_interface_identity_mismatches_publish_exact_diagnostics() {
        let fixture = encoded_semantic_test_interface();

        let cases = [
            (
                package("example.other"),
                fixture.product.clone(),
                DiagnosticKind::InterfacePackageIdentityMismatch,
            ),
            (
                fixture.package.clone(),
                product("other"),
                DiagnosticKind::InterfaceProductIdentityMismatch,
            ),
        ];

        for (expected_package, expected_product, expected_kind) in cases {
            let dependency = DependencyInterfaceInput::new(
                expected_package.clone(),
                expected_product.clone(),
                "identity-mismatch.brayi",
                Arc::<[u8]>::from(fixture.bytes.clone()),
                InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
            );

            let compilation = compilation([dependency]);

            let interface = compilation
                .dependency_interface_id(&expected_package, &expected_product)
                .unwrap_or_else(|| panic!("test dependency interface must have an ID"));

            let result = compilation
                .dependency_interface_result(interface)
                .unwrap_or_else(|| panic!("selected dependency interface must have a result"));

            assert_eq!(diagnostic_kinds(result.diagnostics()), [expected_kind]);
        }
    }

    #[test]
    fn imported_graph_and_semantic_failures_publish_exact_diagnostics() {
        let compilation = compilation([]);

        assert_eq!(
            diagnostic_kinds(&dependency_graph_diagnostics(&compilation)),
            [DiagnosticKind::InterfaceDependencyGraphInvalid]
        );

        let dependency = dependency("example.alpha", "main");

        assert_eq!(
            diagnostic_kinds(&semantic_facts_diagnostics(&dependency)),
            [DiagnosticKind::InterfaceSemanticFactsInvalid]
        );
    }

    #[test]
    fn interface_handles_ignore_dependency_input_order() {
        let forward = compilation([
            dependency("example.alpha", "main"),
            dependency("example.beta", "test"),
        ]);

        let reversed = compilation([
            dependency("example.beta", "test"),
            dependency("example.alpha", "main"),
        ]);

        assert_eq!(
            interface_id(&forward, "example.alpha", "main"),
            interface_id(&reversed, "example.alpha", "main")
        );

        assert_eq!(
            interface_id(&forward, "example.beta", "test"),
            interface_id(&reversed, "example.beta", "test")
        );
    }

    #[test]
    fn concurrent_imported_skeleton_requests_publish_one_result() {
        let compilation = compilation([dependency("example.alpha", "main")]);

        let results = std::thread::scope(|scope| {
            let first = scope.spawn(|| compilation.imported_symbol_skeleton_result());
            let second = scope.spawn(|| compilation.imported_symbol_skeleton_result());

            [first, second].map(|thread| {
                thread
                    .join()
                    .unwrap_or_else(|_| panic!("imported skeleton query thread must not panic"))
                    .unwrap_or_else(|error| {
                        panic!("imported skeleton query must complete: {error:?}")
                    })
            })
        });

        assert!(std::ptr::eq(results[0], results[1]));
    }

    #[test]
    fn valid_dependencies_publish_cached_skeleton_and_exact_template_facts() {
        let fixture = encoded_semantic_test_interface();

        let dependency = DependencyInterfaceInput::new(
            fixture.package.clone(),
            fixture.product.clone(),
            "example-dependency.brayi",
            Arc::<[u8]>::from(fixture.bytes),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let compilation = compilation([dependency]);

        let interface = compilation
            .dependency_interface_id(&fixture.package, &fixture.product)
            .unwrap_or_else(|| panic!("test dependency interface must have an ID"));

        let skeleton = compilation
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("skeleton query must complete: {error:?}"));

        let skeleton = skeleton.value().as_ref().unwrap_or_else(|| {
            panic!(
                "valid dependency must publish a symbol skeleton: {:?}",
                skeleton.diagnostics()
            )
        });

        assert!(!skeleton.packages().is_empty());

        let key = ImportedSemanticFactKey::new(
            interface,
            fixture.template_owner,
            InterfaceSemanticFactKind::DeclarationTemplate,
        );

        let first = compilation
            .imported_semantic_fact_result(key)
            .unwrap_or_else(|error| panic!("semantic fact query must complete: {error:?}"));

        let second = compilation
            .imported_semantic_fact_result(key)
            .unwrap_or_else(|error| panic!("semantic fact query must complete: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());

        let [ImportedSemanticFact::DeclarationTemplate(template)] = first.value().as_ref() else {
            panic!("exact declaration-template query must publish one template");
        };

        let source_symbol_end = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph query must complete: {error:?}"))
            .next_symbol_index();

        assert!(template.owner().symbol_id().to_index() >= Some(source_symbol_end));
        assert_eq!(template.kind(), CheckedTemplateKind::CallableContract);

        assert_eq!(
            template.template().kind(),
            CheckedTemplateKind::CallableContract
        );

        let signature_key = ImportedSemanticFactKey::new(
            interface,
            fixture.template_owner,
            InterfaceSemanticFactKind::CallableSignature,
        );

        let signatures = std::thread::scope(|scope| {
            let first = scope.spawn(|| compilation.imported_semantic_fact_result(signature_key));
            let second = scope.spawn(|| compilation.imported_semantic_fact_result(signature_key));

            [first, second].map(|thread| {
                thread
                    .join()
                    .unwrap_or_else(|_| panic!("imported signature query thread must not panic"))
                    .unwrap_or_else(|error| {
                        panic!("imported signature query must complete: {error:?}")
                    })
            })
        });

        assert!(Arc::ptr_eq(&signatures[0], &signatures[1]));

        assert!(matches!(
            signatures[0].value().as_ref(),
            [ImportedSemanticFact::CallableSignature(_)]
        ));

        let interface_index = interface
            .to_index()
            .unwrap_or_else(|| panic!("test interface ID must fit the host index"));

        assert!(
            compilation.state.imported_semantic_graphs[interface_index]
                .get()
                .is_none()
        );
    }

    #[test]
    fn predicate_definition_states_decode_without_unrelated_semantic_facts() {
        let fixture = encoded_semantic_test_interface();

        let dependency = DependencyInterfaceInput::new(
            fixture.package.clone(),
            fixture.product.clone(),
            "predicate-dependency.brayi",
            Arc::<[u8]>::from(fixture.bytes),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let compilation = compilation([dependency]);

        let interface = compilation
            .dependency_interface_id(&fixture.package, &fixture.product)
            .unwrap_or_else(|| panic!("test dependency interface must have an ID"));

        let cases = [
            (
                fixture.opaque_predicate_owner,
                InterfacePredicateDefinitionState::OpaqueTrusted,
            ),
            (
                fixture.defined_predicate_owner,
                InterfacePredicateDefinitionState::Defined,
            ),
        ];

        for (owner, expected) in cases {
            let key = ImportedSemanticFactKey::new(
                interface,
                owner,
                InterfaceSemanticFactKind::PredicateDefinition,
            );

            let result = compilation
                .imported_semantic_fact_result(key)
                .unwrap_or_else(|error| panic!("predicate fact query must complete: {error:?}"));

            let [ImportedSemanticFact::PredicateDefinition(definition)] = result.value().as_ref()
            else {
                panic!("exact predicate query must publish one definition state");
            };

            assert_eq!(definition.state(), expected);
        }

        let interface_index = interface
            .to_index()
            .unwrap_or_else(|| panic!("test interface ID must fit the host index"));

        assert!(
            compilation.state.imported_semantic_graphs[interface_index]
                .get()
                .is_none()
        );
    }

    #[test]
    fn implementation_header_facts_are_narrow_cached_and_complete() {
        let fixture = encoded_semantic_test_interface();

        let dependency = DependencyInterfaceInput::new(
            fixture.package.clone(),
            fixture.product.clone(),
            "implementation-dependency.brayi",
            Arc::<[u8]>::from(fixture.bytes),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let compilation = compilation([dependency]);

        let interface = compilation
            .dependency_interface_id(&fixture.package, &fixture.product)
            .unwrap_or_else(|| panic!("test dependency interface must have an ID"));

        let implementation_key = ImportedSemanticFactKey::new(
            interface,
            fixture.implementation_owner,
            InterfaceSemanticFactKind::Implementation,
        );

        let results = std::thread::scope(|scope| {
            let first =
                scope.spawn(|| compilation.imported_semantic_fact_result(implementation_key));

            let second =
                scope.spawn(|| compilation.imported_semantic_fact_result(implementation_key));

            [first, second].map(|thread| {
                thread
                    .join()
                    .unwrap_or_else(|_| panic!("implementation fact thread must not panic"))
                    .unwrap_or_else(|error| {
                        panic!("implementation fact query must complete: {error:?}")
                    })
            })
        });

        assert!(Arc::ptr_eq(&results[0], &results[1]));
        assert!(results[0].diagnostics().is_empty());

        let [ImportedSemanticFact::Implementation(implementation)] = results[0].value().as_ref()
        else {
            panic!("exact implementation query must publish one header");
        };

        assert!(implementation.coherence().is_some());
        assert_eq!(implementation.constraints().len(), 1);
        assert_eq!(implementation.target_dependencies().len(), 1);

        let interface_index = interface
            .to_index()
            .unwrap_or_else(|| panic!("test interface ID must fit the host index"));

        assert!(
            compilation.state.imported_semantic_graphs[interface_index]
                .get()
                .is_none()
        );
    }

    #[test]
    fn cancelled_exact_fact_queries_do_not_publish_results() {
        let fixture = encoded_semantic_test_interface();

        let dependency = DependencyInterfaceInput::new(
            fixture.package.clone(),
            fixture.product.clone(),
            "example-dependency.brayi",
            Arc::<[u8]>::from(fixture.bytes),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let compilation = compilation([dependency]);

        let interface = compilation
            .dependency_interface_id(&fixture.package, &fixture.product)
            .unwrap_or_else(|| panic!("test dependency interface must have an ID"));

        let key = ImportedSemanticFactKey::new(
            interface,
            fixture.template_owner,
            InterfaceSemanticFactKind::DeclarationTemplate,
        );

        let cancellation = CancellationToken::new();

        cancellation.cancel();

        assert_eq!(
            compilation.imported_semantic_fact_result_with_cancellation(key, &cancellation),
            Err(FactQueryError::Cancelled)
        );

        assert_eq!(
            compilation.state.imported_semantic_facts.is_published(&key),
            Ok(false)
        );
    }

    #[test]
    fn identical_interface_failures_retain_dependency_provenance() {
        let compilation = compilation([
            dependency("example.alpha", "main"),
            dependency("example.beta", "test"),
        ]);

        let diagnostics = compilation.imported_diagnostics();

        let paths = diagnostics
            .iter()
            .flat_map(|diagnostic| diagnostic.notes())
            .flat_map(|note| note.args())
            .filter(|argument| argument.name() == DiagnosticArgName::ArtifactPath)
            .filter_map(|argument| match argument.value() {
                DiagnosticArgValue::FilePath(path) => Some(path.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(diagnostics.len(), 2);

        assert_eq!(
            paths,
            [
                PathBuf::from("example.alpha-main.brayi"),
                PathBuf::from("example.beta-test.brayi"),
            ]
        );
    }

    fn compilation(
        dependencies: impl IntoIterator<Item = DependencyInterfaceInput>,
    ) -> Compilation {
        let request = CompilationRequest::new(
            package("example.current"),
            vec![SourceInput::virtual_text(
                SourceIdentity::new(1),
                "main.bray",
                SourceVersion::new(1),
                "module example.current;",
            )],
        )
        .with_dependency_interfaces(dependencies);

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn dependency(package_name: &str, product_name: &str) -> DependencyInterfaceInput {
        DependencyInterfaceInput::new(
            package(package_name),
            product(product_name),
            format!("{package_name}-{product_name}.brayi"),
            Arc::<[u8]>::from(vec![0; 112]),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
    }

    fn interface_id(
        compilation: &Compilation,
        package_name: &str,
        product_name: &str,
    ) -> bray_symbols::ImportedInterfaceId {
        compilation
            .dependency_interface_id(&package(package_name), &product(product_name))
            .unwrap_or_else(|| panic!("selected dependency interface must have an ID"))
    }

    fn package(value: &str) -> PackageIdentity {
        PackageIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test package identity must be valid"))
    }

    fn product(value: &str) -> InterfaceProductIdentity {
        InterfaceProductIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test product identity must be valid"))
    }
}
