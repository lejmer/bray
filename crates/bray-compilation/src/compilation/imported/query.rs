// rust-style: allow(module-too-large, reason = "imported interface loading, decoding, and diagnostic conversion form one demand-driven query pipeline")

use std::sync::Arc;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticBag, DiagnosticCheckedTemplateProblem, DiagnosticId,
    DiagnosticInterfaceDeclarationIdentity, DiagnosticInterfaceLimit,
    DiagnosticInterfaceRelationshipKind, DiagnosticInterfaceSemanticProblem,
    DiagnosticInterfaceSymbolGraphProblem, DiagnosticInterfaceSymbolIdentity,
    DiagnosticInterfaceSymbolReference, DiagnosticInterfaceSynthesizedIdentity, DiagnosticKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticResult,
    DiagnosticSemanticContentProblem, DiagnosticSemanticValueKind, SeverityKind,
};
use bray_package_interface::{
    ImportedInterfaceSymbolResolver, ImportedSemanticRecord, ImportedSemantics,
    ImportedSymbolConstructionError, InterfaceSemanticInternError, InterfaceSymbolReference,
    LoadedInterfaceSurface, PackageInterfaceSurface, ValidatedPackageInterface,
    construct_imported_symbol_skeletons,
};
use bray_symbols::{
    ImportedInterfaceId, ImportedSymbolSkeleton, PackageIdentity, SemanticValueKind,
    SemanticValueStoreError, SymbolId, SymbolRootKey, diagnostic_symbol_kind,
};

use super::diagnostic::{
    contextual_interface_diagnostic, standard_library_diagnostics, unlocated_interface_diagnostics,
    validation_diagnostics,
};
use super::model::LoadedDependencyInterface;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, ImportedSemanticRecordKey};
use crate::request::DependencyInterfaceInput;

impl super::super::Compilation {
    /// Returns structured diagnostics for one failed standard library artifact request.
    pub fn standard_library_load_diagnostics(
        &self,
        error: &bray_standard_library::StandardLibraryLoadError,
    ) -> DiagnosticBag {
        let input = self.dependency_interfaces().iter().find(|input| {
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
        self.dependency_interfaces()
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

    /// Returns one exact imported symbol-owned semantic record category.
    pub fn imported_semantics(
        &self,
        key: ImportedSemanticRecordKey,
    ) -> Result<Arc<DiagnosticResult<Arc<[ImportedSemanticRecord]>>>, FactQueryError> {
        self.imported_semantics_with_cancellation(key, &self.state.cancellation)
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
    ) -> Result<Option<&DiagnosticResult<Option<Arc<ImportedSemantics>>>>, FactQueryError> {
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

    pub(in crate::compilation) fn imported_semantics_with_cancellation(
        &self,
        key: ImportedSemanticRecordKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Arc<[ImportedSemanticRecord]>>>, FactQueryError> {
        let cell = self.state.imported_semantics.cell(key)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::ImportedSemanticRecord(key),
            cancellation,
            || {
                self.compute_imported_semantics(key, cancellation)
                    .map(Arc::new)
            },
        )?;

        // Exact query results are Arc-backed so callers do not retain the cache-cell map lock.
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
                panic!("semantic unit context failed: {error:?}")
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

        if self.state.dependency_interfaces.get(index).is_none() {
            return Ok(None);
        }

        let Some(cache) = self.state.loaded_dependency_interfaces.get(index) else {
            return Ok(None);
        };

        self.query_fact_with_cancellation(
            CompilationFactKey::DependencyInterface(interface),
            cache,
            cancellation,
            |cancellation| {
                let input = self
                    .dependency_interface(interface)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                load_dependency_interface(input, cancellation)
            },
        )
        .map(Some)
    }

    pub(in crate::compilation) fn dependency_interface_input(
        &self,
        interface: ImportedInterfaceId,
    ) -> Option<&DependencyInterfaceInput> {
        self.dependency_interface(interface)
    }

    fn compute_imported_symbol_skeleton(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<Arc<ImportedSymbolSkeleton>>>, FactQueryError> {
        let dependency_count = self.dependency_interface_count();
        let mut loaded = Vec::with_capacity(dependency_count);
        let mut diagnostics = Vec::new();

        for index in 0..dependency_count {
            cancellation.check()?;

            let Some(interface) = ImportedInterfaceId::try_from_index(index) else {
                return Ok(DiagnosticResult::new(
                    None,
                    dependency_graph_capacity_diagnostics(
                        self,
                        DiagnosticInterfaceLimit::LoadedInterfaceCount,
                        stable_count(dependency_count),
                        ImportedInterfaceId::CAPACITY,
                    ),
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

        let imported_symbol_count = loaded.iter().fold(0_u64, |count, interface| {
            count.saturating_add(stable_count(interface.surface().symbols().symbols().len()))
        });

        let existing_symbol_count = stable_count(graph.next_symbol_index());

        let required_symbol_count = existing_symbol_count.saturating_add(imported_symbol_count);

        if required_symbol_count > SymbolId::CAPACITY {
            return Ok(DiagnosticResult::new(
                None,
                dependency_graph_capacity_diagnostics(
                    self,
                    DiagnosticInterfaceLimit::CompilationSymbolCount,
                    required_symbol_count,
                    SymbolId::CAPACITY,
                ),
            ));
        }

        let first_symbol = u32::try_from(graph.next_symbol_index())
            .map(SymbolId::new)
            .unwrap_or_else(|_| SymbolId::new(u32::MAX));

        cancellation.check()?;

        match construct_imported_symbol_skeletons(first_symbol, loaded) {
            Ok(symbols) => Ok(DiagnosticResult::new(Some(Arc::new(symbols)), diagnostics)),
            Err(error) => Ok(DiagnosticResult::new(
                None,
                dependency_graph_diagnostics(self, error),
            )),
        }
    }

    fn compute_imported_semantic_graph(
        &self,
        interface: ImportedInterfaceId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<Arc<ImportedSemantics>>>, FactQueryError> {
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

        let decoded = match validated.decode_semantics(surface) {
            Ok(decoded) => decoded,
            Err(error) => {
                return Ok(DiagnosticResult::new(
                    None,
                    validation_diagnostics(error, input),
                ));
            }
        };

        self.intern_imported_semantics(interface, input, decoded, skeleton, cancellation)
    }

    fn compute_imported_semantics(
        &self,
        key: ImportedSemanticRecordKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Arc<[ImportedSemanticRecord]>>, FactQueryError> {
        if let Some(graph) = self.compute_selected_imported_semantic_graph(key, cancellation)? {
            return self.imported_semantics_from_graph(key, &graph, cancellation);
        }

        let graph = self
            .imported_semantic_graph_result_with_cancellation(key.interface(), cancellation)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        self.imported_semantics_from_graph(key, graph, cancellation)
    }

    fn imported_semantics_from_graph(
        &self,
        key: ImportedSemanticRecordKey,
        graph: &DiagnosticResult<Option<Arc<ImportedSemantics>>>,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Arc<[ImportedSemanticRecord]>>, FactQueryError> {
        let Some(graph) = graph.value() else {
            // The exact query owns diagnostics so it can publish independently of the graph cache.
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
            graph.symbol_semantics(owner, key.kind()).into(),
        ))
    }

    fn compute_selected_imported_semantic_graph(
        &self,
        key: ImportedSemanticRecordKey,
        cancellation: &CancellationToken,
    ) -> Result<Option<DiagnosticResult<Option<Arc<ImportedSemantics>>>>, FactQueryError> {
        let loaded = self
            .loaded_dependency_interface_with_cancellation(key.interface(), cancellation)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let input = self
            .dependency_interface_input(key.interface())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let (Some(validated), Some(surface)) = (loaded.validated(), loaded.surface()) else {
            return Ok(Some(DiagnosticResult::without_diagnostics(None)));
        };

        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(skeleton) = skeleton.value() else {
            return Ok(Some(DiagnosticResult::without_diagnostics(None)));
        };

        cancellation.check()?;

        let decoded =
            match validated.decode_selected_semantic_graph(surface, key.owner(), key.kind()) {
                Ok(Some(decoded)) => decoded,
                Ok(None) => return Ok(None),
                Err(error) => {
                    return Ok(Some(DiagnosticResult::new(
                        None,
                        validation_diagnostics(error, input),
                    )));
                }
            };

        self.intern_imported_semantics(key.interface(), input, decoded, skeleton, cancellation)
            .map(Some)
    }

    fn intern_imported_semantics(
        &self,
        interface: ImportedInterfaceId,
        input: &DependencyInterfaceInput,
        decoded: bray_package_interface::InterfaceSemantics,
        skeleton: &ImportedSymbolSkeleton,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<Arc<ImportedSemantics>>>, FactQueryError> {
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
            Ok(semantics) => Ok(DiagnosticResult::without_diagnostics(Some(Arc::new(semantics)))),
            Err(error) => Ok(DiagnosticResult::new(
                None,
                semantic_content_diagnostics(error, input),
            )),
        }
    }

    pub(in crate::compilation) fn loaded_interface_views(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<Option<Vec<LoadedInterfaceSurface<'_>>>, FactQueryError> {
        let dependency_count = self.dependency_interface_count();
        let mut interfaces = Vec::with_capacity(dependency_count);

        for index in 0..dependency_count {
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
            for index in 0..self.dependency_interface_count() {
                let Some(interface) = ImportedInterfaceId::try_from_index(index) else {
                    return dependency_graph_capacity_diagnostics(
                        self,
                        DiagnosticInterfaceLimit::LoadedInterfaceCount,
                        stable_count(self.dependency_interface_count()),
                        ImportedInterfaceId::CAPACITY,
                    );
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
        let diagnostics = contextual_interface_diagnostic(
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::InterfacePackageIdentityMismatch,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::expected_package_identity(
                input.package().as_str(),
            ))
            .with_arg(DiagnosticArg::actual_package_identity(
                surface.identity().package().as_str(),
            )),
            input,
        );

        return Ok(LoadedDependencyInterface::invalid(diagnostics));
    }

    if surface.identity().product() != input.product() {
        let diagnostics = contextual_interface_diagnostic(
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::InterfaceProductIdentityMismatch,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::expected_product_identity(
                input.product().as_str(),
            ))
            .with_arg(DiagnosticArg::actual_product_identity(
                surface.identity().product().as_str(),
            )),
            input,
        );

        return Ok(LoadedDependencyInterface::invalid(diagnostics));
    }

    Ok(LoadedDependencyInterface::new(validated, surface))
}

fn dependency_graph_capacity_diagnostics(
    compilation: &super::super::Compilation,
    limit: DiagnosticInterfaceLimit,
    actual: u64,
    maximum: u64,
) -> DiagnosticBag {
    contextual_dependency_diagnostic(
        compilation,
        None,
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::InterfaceSymbolCapacityExceeded,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::actual_package_identity(
            compilation.package_identity().as_str(),
        ))
        .with_arg(DiagnosticArg::interface_limit(limit))
        .with_arg(DiagnosticArg::actual_count(actual))
        .with_arg(DiagnosticArg::maximum_count(maximum)),
    )
}

fn stable_count(count: usize) -> u64 {
    u64::try_from(count).unwrap_or(u64::MAX)
}

fn dependency_graph_diagnostics(
    compilation: &super::super::Compilation,
    error: ImportedSymbolConstructionError,
) -> DiagnosticBag {
    let (mut diagnostic, primary, related) =
        imported_symbol_diagnostic(error, compilation.package_identity());

    if let Some(related) = related
        .and_then(|interface| compilation.dependency_interface_input(interface))
        .and_then(DependencyInterfaceInput::dependency_span)
    {
        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::ConflictingDependency,
            related,
        ));
    }

    contextual_dependency_diagnostic(compilation, primary, diagnostic)
}

fn contextual_dependency_diagnostic(
    compilation: &super::super::Compilation,
    interface: Option<ImportedInterfaceId>,
    diagnostic: Diagnostic,
) -> DiagnosticBag {
    if let Some(input) = interface.and_then(|id| compilation.dependency_interface_input(id)) {
        contextual_interface_diagnostic(diagnostic, input)
    } else {
        DiagnosticBag::single(diagnostic)
    }
}

fn imported_symbol_diagnostic(
    error: ImportedSymbolConstructionError,
    package: &PackageIdentity,
) -> (
    Diagnostic,
    Option<ImportedInterfaceId>,
    Option<ImportedInterfaceId>,
) {
    use bray_symbols::ImportedSymbolSkeletonBuildError;

    let (kind, args, primary, related) = match error {
        ImportedSymbolConstructionError::DuplicatePackage {
            package,
            first,
            duplicate,
        } => (
            DiagnosticKind::InterfaceDuplicatePackage,
            vec![DiagnosticArg::actual_package_identity(package.as_str())],
            Some(duplicate),
            Some(first),
        ),
        ImportedSymbolConstructionError::MissingDependency { importing, package } => (
            DiagnosticKind::InterfaceMissingDependency,
            vec![DiagnosticArg::expected_package_identity(package.as_str())],
            Some(importing),
            None,
        ),
        ImportedSymbolConstructionError::DependencyProductMismatch {
            importing,
            expected,
            actual,
            ..
        } => (
            DiagnosticKind::InterfaceDependencyProductMismatch,
            vec![
                DiagnosticArg::expected_product_identity(expected.as_str()),
                DiagnosticArg::actual_product_identity(actual.as_str()),
            ],
            Some(importing),
            None,
        ),
        ImportedSymbolConstructionError::DependencyContentMismatch {
            importing,
            expected,
            actual,
            ..
        } => (
            DiagnosticKind::InterfaceDependencyContentMismatch,
            vec![
                DiagnosticArg::expected_artifact_digest(DiagnosticArtifactDigest::new(
                    DiagnosticArtifactDigestAlgorithm::Blake3,
                    *expected.as_bytes(),
                )),
                DiagnosticArg::actual_artifact_digest(DiagnosticArtifactDigest::new(
                    DiagnosticArtifactDigestAlgorithm::Blake3,
                    *actual.as_bytes(),
                )),
            ],
            Some(importing),
            None,
        ),
        ImportedSymbolConstructionError::SymbolOutOfBounds { interface, symbol } => (
            DiagnosticKind::InterfaceSymbolReferenceInvalid,
            vec![DiagnosticArg::interface_record_index(symbol.raw())],
            Some(interface),
            None,
        ),
        ImportedSymbolConstructionError::DependencyOutOfBounds {
            interface,
            dependency,
        } => (
            DiagnosticKind::InterfaceDependencyReferenceInvalid,
            vec![DiagnosticArg::interface_record_index(dependency.raw())],
            Some(interface),
            None,
        ),
        ImportedSymbolConstructionError::MissingDependencySymbol {
            importing,
            package,
            key,
        } => (
            DiagnosticKind::InterfaceDependencySymbolMissing,
            vec![
                DiagnosticArg::expected_package_identity(package.as_str()),
                DiagnosticArg::interface_symbol_identity(external_symbol_identity(&key)),
            ],
            Some(importing),
            None,
        ),
        ImportedSymbolConstructionError::CompilerKnownExportTarget { importing, key } => (
            DiagnosticKind::InterfaceCompilerDeclarationExported,
            vec![DiagnosticArg::interface_symbol_identity(symbol_identity(
                &key,
            ))],
            Some(importing),
            None,
        ),
        ImportedSymbolConstructionError::Symbols(
            ImportedSymbolSkeletonBuildError::SymbolCapacityExceeded { actual, maximum },
        ) => (
            DiagnosticKind::InterfaceSymbolCapacityExceeded,
            vec![
                DiagnosticArg::actual_package_identity(package.as_str()),
                DiagnosticArg::interface_limit(DiagnosticInterfaceLimit::CompilationSymbolCount),
                DiagnosticArg::actual_count(actual),
                DiagnosticArg::maximum_count(maximum),
            ],
            None,
            None,
        ),
        ImportedSymbolConstructionError::Symbols(error) => {
            let (primary, related) = interface_symbol_graph_context(&error);

            (
                DiagnosticKind::InterfaceSymbolGraphInvalid,
                vec![DiagnosticArg::interface_symbol_graph_problem(
                    interface_symbol_graph_problem(error),
                )],
                primary,
                related,
            )
        }
    };

    let diagnostic = args.into_iter().fold(
        Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error),
        Diagnostic::with_arg,
    );

    (diagnostic, primary, related)
}

fn symbol_identity(key: &bray_symbols::SymbolKey) -> DiagnosticInterfaceSymbolIdentity {
    use bray_symbols::SymbolKeyData;

    match key.data() {
        SymbolKeyData::Root(root) => symbol_root_identity(root),
        SymbolKeyData::Module { owner, path } => DiagnosticInterfaceSymbolIdentity::Module {
            owner: Box::new(symbol_root_identity(owner)),
            path: module_path(path),
        },
        SymbolKeyData::CompilerKnownDeclaration { key, kind } => {
            DiagnosticInterfaceSymbolIdentity::CompilerKnownDeclaration {
                key: key.as_str().to_owned(),
                kind: diagnostic_symbol_kind(*kind),
            }
        }
        SymbolKeyData::SourceDeclaration {
            owner,
            kind,
            declaration,
        } => DiagnosticInterfaceSymbolIdentity::SourceDeclaration {
            owner: Box::new(symbol_identity(owner)),
            kind: diagnostic_symbol_kind(*kind),
            declaration: declaration.raw(),
        },
        SymbolKeyData::Synthesized(synthesized) => DiagnosticInterfaceSymbolIdentity::Synthesized {
            owner: Box::new(symbol_identity(synthesized.subject())),
            identity: synthesized_identity(synthesized.role(), synthesized.ordinal()),
        },
        SymbolKeyData::External(external) => external_symbol_identity(external),
    }
}

fn symbol_root_identity(root: &SymbolRootKey) -> DiagnosticInterfaceSymbolIdentity {
    match root {
        SymbolRootKey::CompilerKnownEnvironment => {
            DiagnosticInterfaceSymbolIdentity::CompilerKnownEnvironment
        }
        SymbolRootKey::Package(package) => {
            DiagnosticInterfaceSymbolIdentity::Package(package.as_str().to_owned())
        }
    }
}

fn interface_symbol_graph_context(
    error: &bray_symbols::ImportedSymbolSkeletonBuildError,
) -> (Option<ImportedInterfaceId>, Option<ImportedInterfaceId>) {
    use bray_symbols::ImportedSymbolSkeletonBuildError as Error;

    match error {
        Error::DuplicateInterface(interface)
        | Error::RelationshipSymbolOutOfBounds { interface, .. }
        | Error::LookupOwnerOutOfBounds { interface, .. } => (Some(*interface), None),
        Error::DuplicateExternalKey {
            first, duplicate, ..
        } => (Some(*duplicate), Some(*first)),
        _ => (None, None),
    }
}

fn interface_symbol_graph_problem(
    error: bray_symbols::ImportedSymbolSkeletonBuildError,
) -> DiagnosticInterfaceSymbolGraphProblem {
    use bray_symbols::ImportedSymbolSkeletonBuildError as Error;

    match error {
        Error::DuplicateInterface(interface) => {
            DiagnosticInterfaceSymbolGraphProblem::DuplicateInterface(interface.raw())
        }
        Error::DuplicatePackage(package) => {
            DiagnosticInterfaceSymbolGraphProblem::DuplicatePackage(package.as_str().to_owned())
        }
        Error::SymbolCapacityExceeded { actual, maximum } => {
            DiagnosticInterfaceSymbolGraphProblem::SymbolCapacityExceeded { actual, maximum }
        }
        Error::DuplicateExternalKey { key, .. } => {
            DiagnosticInterfaceSymbolGraphProblem::DuplicateExternalIdentity(
                external_symbol_identity(&key),
            )
        }
        Error::RelationshipSymbolOutOfBounds { interface, symbol } => {
            DiagnosticInterfaceSymbolGraphProblem::RelationshipSymbolOutOfBounds {
                interface: interface.raw(),
                symbol: symbol.raw(),
            }
        }
        Error::InvalidRelationshipKinds {
            relationship,
            owner,
            member,
        } => DiagnosticInterfaceSymbolGraphProblem::InvalidRelationshipKinds {
            relationship: diagnostic_relationship_kind(relationship),
            owner: diagnostic_symbol_kind(owner),
            member: diagnostic_symbol_kind(member),
        },
        Error::RelationshipContainmentMismatch { owner, member } => {
            DiagnosticInterfaceSymbolGraphProblem::RelationshipContainmentMismatch {
                owner: owner.symbol_id().raw(),
                member: member.symbol_id().raw(),
            }
        }
        Error::NonCanonicalRelationshipOrdinal {
            relationship,
            owner,
            expected,
            actual,
        } => DiagnosticInterfaceSymbolGraphProblem::NonCanonicalRelationshipOrdinal {
            relationship: diagnostic_relationship_kind(relationship),
            owner: owner.symbol_id().raw(),
            expected,
            actual,
        },
        Error::MissingContainment(symbol) => {
            DiagnosticInterfaceSymbolGraphProblem::MissingContainment {
                symbol: symbol.symbol_id().raw(),
                kind: diagnostic_symbol_kind(symbol.kind()),
            }
        }
        Error::DuplicateContainment(symbol) => {
            DiagnosticInterfaceSymbolGraphProblem::DuplicateContainment {
                symbol: symbol.symbol_id().raw(),
                kind: diagnostic_symbol_kind(symbol.kind()),
            }
        }
        Error::LookupOwnerOutOfBounds { interface, owner } => {
            DiagnosticInterfaceSymbolGraphProblem::LookupOwnerOutOfBounds {
                interface: interface.raw(),
                owner: owner.raw(),
            }
        }
        Error::InvalidLookupOwner(owner) => {
            DiagnosticInterfaceSymbolGraphProblem::InvalidLookupOwner {
                owner: owner.symbol_id().raw(),
                kind: diagnostic_symbol_kind(owner.kind()),
            }
        }
        Error::MissingLookupTarget(key) => {
            DiagnosticInterfaceSymbolGraphProblem::MissingLookupTarget(external_symbol_identity(
                &key,
            ))
        }
        Error::DuplicateLookupName { owner, name } => {
            DiagnosticInterfaceSymbolGraphProblem::DuplicateLookupName {
                owner: owner.symbol_id().raw(),
                name: name.as_str().to_owned(),
            }
        }
        Error::UnsupportedSymbolKind(kind) => {
            DiagnosticInterfaceSymbolGraphProblem::UnsupportedSymbolKind(diagnostic_symbol_kind(
                kind,
            ))
        }
        Error::InvalidRecordRelationships(symbol) => {
            DiagnosticInterfaceSymbolGraphProblem::InvalidRecordRelationships {
                symbol: symbol.symbol_id().raw(),
                kind: diagnostic_symbol_kind(symbol.kind()),
            }
        }
    }
}

fn external_symbol_identity(
    key: &bray_symbols::ExternalSymbolKey,
) -> DiagnosticInterfaceSymbolIdentity {
    use bray_symbols::{ExternalDeclarationIdentity, ExternalSymbolKeyData};

    match key.data() {
        ExternalSymbolKeyData::Package(package) => {
            DiagnosticInterfaceSymbolIdentity::Package(package.as_str().to_owned())
        }
        ExternalSymbolKeyData::Module { package, path } => {
            DiagnosticInterfaceSymbolIdentity::Module {
                owner: Box::new(external_symbol_identity(package)),
                path: module_path(path),
            }
        }
        ExternalSymbolKeyData::Declaration {
            owner,
            kind,
            identity,
        } => DiagnosticInterfaceSymbolIdentity::Declaration {
            owner: Box::new(external_symbol_identity(owner)),
            kind: diagnostic_symbol_kind(*kind),
            identity: match identity {
                ExternalDeclarationIdentity::Name(name) => {
                    DiagnosticInterfaceDeclarationIdentity::Name(name.as_str().to_owned())
                }
                ExternalDeclarationIdentity::Ordinal(ordinal) => {
                    DiagnosticInterfaceDeclarationIdentity::Ordinal(ordinal.raw())
                }
            },
        },
        ExternalSymbolKeyData::Synthesized {
            owner,
            role,
            ordinal,
        } => DiagnosticInterfaceSymbolIdentity::Synthesized {
            owner: Box::new(external_symbol_identity(owner)),
            identity: synthesized_identity(*role, *ordinal),
        },
    }
}

fn module_path(path: &bray_symbols::ModulePathKey) -> Box<[String]> {
    path.segments()
        .map(str::to_owned)
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn synthesized_identity(
    role: bray_symbols::SynthesizedSymbolRole,
    ordinal: Option<bray_symbols::SymbolOrdinal>,
) -> DiagnosticInterfaceSynthesizedIdentity {
    use bray_symbols::SynthesizedSymbolRole as Role;

    match (role, ordinal) {
        (Role::ReceiverParameter, None) => {
            DiagnosticInterfaceSynthesizedIdentity::ReceiverParameter
        }
        (Role::DeclaredGenericTypeParameter, Some(ordinal)) => {
            DiagnosticInterfaceSynthesizedIdentity::DeclaredGenericTypeParameter(ordinal.raw())
        }
        (Role::DeclaredGenericConstParameter, Some(ordinal)) => {
            DiagnosticInterfaceSynthesizedIdentity::DeclaredGenericConstParameter(ordinal.raw())
        }
        (Role::CallableParameter, Some(ordinal)) => {
            DiagnosticInterfaceSynthesizedIdentity::CallableParameter(ordinal.raw())
        }
        (Role::PredicateParameter, Some(ordinal)) => {
            DiagnosticInterfaceSynthesizedIdentity::PredicateParameter(ordinal.raw())
        }
        (Role::InferredImplementationTypeParameter, Some(ordinal)) => {
            DiagnosticInterfaceSynthesizedIdentity::InferredImplementationTypeParameter(
                ordinal.raw(),
            )
        }
        (Role::InferredImplementationConstParameter, Some(ordinal)) => {
            DiagnosticInterfaceSynthesizedIdentity::InferredImplementationConstParameter(
                ordinal.raw(),
            )
        }
        (Role::CallableParameterDefaultProvider, None) => {
            DiagnosticInterfaceSynthesizedIdentity::CallableParameterDefaultProvider
        }
        (Role::StructFieldDefaultProvider, None) => {
            DiagnosticInterfaceSynthesizedIdentity::StructFieldDefaultProvider
        }
        (Role::UnionPayloadDefaultProvider, None) => {
            DiagnosticInterfaceSynthesizedIdentity::UnionPayloadDefaultProvider
        }
        _ => unreachable!("symbol keys enforce synthesized-role ordinal shape"),
    }
}

fn interface_symbol_reference(
    reference: InterfaceSymbolReference,
) -> DiagnosticInterfaceSymbolReference {
    match reference {
        InterfaceSymbolReference::Local(symbol) => {
            DiagnosticInterfaceSymbolReference::Local(symbol.raw())
        }
        InterfaceSymbolReference::Dependency { dependency, key } => {
            DiagnosticInterfaceSymbolReference::Dependency {
                dependency: dependency.raw(),
                identity: external_symbol_identity(&key),
            }
        }
        InterfaceSymbolReference::CompilerKnown(reference) => {
            DiagnosticInterfaceSymbolReference::CompilerKnown(symbol_identity(reference.key()))
        }
    }
}

fn semantic_content_problem(error: SemanticValueStoreError) -> DiagnosticSemanticContentProblem {
    match error {
        SemanticValueStoreError::ForeignId { expected, actual } => {
            DiagnosticSemanticContentProblem::ForeignId {
                expected: expected.raw(),
                actual: actual.raw(),
            }
        }
        SemanticValueStoreError::UnknownId { kind } => {
            DiagnosticSemanticContentProblem::UnknownId {
                value_kind: diagnostic_semantic_value_kind(kind),
            }
        }
        SemanticValueStoreError::CapacityExhausted { kind } => {
            DiagnosticSemanticContentProblem::CapacityExhausted {
                value_kind: diagnostic_semantic_value_kind(kind),
            }
        }
        SemanticValueStoreError::GenericOwnerMismatch { expected, actual } => {
            DiagnosticSemanticContentProblem::GenericOwnerMismatch {
                expected: expected.symbol().symbol_id().raw(),
                actual: actual.symbol().symbol_id().raw(),
            }
        }
        SemanticValueStoreError::OpenSubstitution => {
            DiagnosticSemanticContentProblem::OpenSubstitution
        }
    }
}

fn diagnostic_relationship_kind(
    kind: bray_symbols::SymbolRelationshipKind,
) -> DiagnosticInterfaceRelationshipKind {
    use DiagnosticInterfaceRelationshipKind as Diagnostic;
    use bray_symbols::SymbolRelationshipKind as Relationship;

    match kind {
        Relationship::PackageModule => Diagnostic::PackageModule,
        Relationship::ModuleMember => Diagnostic::ModuleMember,
        Relationship::TypeMember => Diagnostic::TypeMember,
        Relationship::TraitMember => Diagnostic::TraitMember,
        Relationship::ImplementationMember => Diagnostic::ImplementationMember,
        Relationship::StructField => Diagnostic::StructField,
        Relationship::UnionVariant => Diagnostic::UnionVariant,
        Relationship::UnionPayloadField => Diagnostic::UnionPayloadField,
        Relationship::GenericParameter => Diagnostic::GenericParameter,
        Relationship::CallableParameter => Diagnostic::CallableParameter,
        Relationship::PredicateParameter => Diagnostic::PredicateParameter,
        Relationship::OverloadArm => Diagnostic::OverloadArm,
        Relationship::ImplementationFulfillment => Diagnostic::ImplementationFulfillment,
        Relationship::DefaultProvider => Diagnostic::DefaultProvider,
    }
}

fn diagnostic_semantic_value_kind(kind: SemanticValueKind) -> DiagnosticSemanticValueKind {
    match kind {
        SemanticValueKind::Type => DiagnosticSemanticValueKind::Type,
        SemanticValueKind::ConstantValue => DiagnosticSemanticValueKind::ConstantValue,
        SemanticValueKind::ConstantTerm => DiagnosticSemanticValueKind::ConstantTerm,
        SemanticValueKind::GenericSubstitution => DiagnosticSemanticValueKind::GenericSubstitution,
        SemanticValueKind::TraitApplication => DiagnosticSemanticValueKind::TraitApplication,
        SemanticValueKind::CallableInstance => DiagnosticSemanticValueKind::CallableInstance,
        SemanticValueKind::ImplementationInstance => {
            DiagnosticSemanticValueKind::ImplementationInstance
        }
        SemanticValueKind::DependencyContractTemplate => {
            DiagnosticSemanticValueKind::DependencyContractTemplate
        }
    }
}

fn checked_template_problem(
    error: bray_bound_tree::CheckedTemplateBuildError,
) -> DiagnosticCheckedTemplateProblem {
    use bray_bound_tree::CheckedTemplateBuildError as Error;

    match error {
        Error::CapacityExceeded => DiagnosticCheckedTemplateProblem::CapacityExceeded,
        Error::RecoveredTemplate => DiagnosticCheckedTemplateProblem::RecoveredTemplate,
        Error::MissingInput(input) => DiagnosticCheckedTemplateProblem::MissingInput(input.raw()),
        Error::DuplicateInput { first, duplicate } => {
            DiagnosticCheckedTemplateProblem::DuplicateInput {
                first: first.raw(),
                duplicate: duplicate.raw(),
            }
        }
        Error::InputTypeMismatch {
            node,
            input,
            expected,
            actual,
        } => DiagnosticCheckedTemplateProblem::InputTypeMismatch {
            node: node.raw(),
            input: input.raw(),
            expected_type: expected.slot(),
            actual_type: actual.slot(),
        },
        Error::MissingNode(node) => DiagnosticCheckedTemplateProblem::MissingNode(node.raw()),
        Error::ForwardNodeReference { node, referenced } => {
            DiagnosticCheckedTemplateProblem::ForwardNodeReference {
                node: node.raw(),
                referenced: referenced.raw(),
            }
        }
        Error::MissingTemporary(temporary) => {
            DiagnosticCheckedTemplateProblem::MissingTemporary(temporary.raw())
        }
        Error::UninitializedTemporary { node, temporary } => {
            DiagnosticCheckedTemplateProblem::UninitializedTemporary {
                node: node.raw(),
                temporary: temporary.raw(),
            }
        }
        Error::TemporaryInitializerTypeMismatch {
            initializer,
            expected,
            actual,
        } => DiagnosticCheckedTemplateProblem::TemporaryInitializerTypeMismatch {
            initializer: initializer.raw(),
            expected_type: expected.slot(),
            actual_type: actual.slot(),
        },
        Error::TemporaryTypeMismatch {
            node,
            temporary,
            expected,
            actual,
        } => DiagnosticCheckedTemplateProblem::TemporaryTypeMismatch {
            node: node.raw(),
            temporary: temporary.raw(),
            expected_type: expected.slot(),
            actual_type: actual.slot(),
        },
        Error::ConversionTypeMismatch {
            node,
            expected,
            actual,
        } => DiagnosticCheckedTemplateProblem::ConversionTypeMismatch {
            node: node.raw(),
            expected_type: expected.slot(),
            actual_type: actual.slot(),
        },
        Error::ConditionalBranchTypeMismatch {
            node,
            when_true,
            when_false,
        } => DiagnosticCheckedTemplateProblem::ConditionalBranchTypeMismatch {
            node: node.raw(),
            when_true_type: when_true.slot(),
            when_false_type: when_false.slot(),
        },
        Error::ConditionalResultTypeMismatch {
            node,
            expected,
            actual,
        } => DiagnosticCheckedTemplateProblem::ConditionalResultTypeMismatch {
            node: node.raw(),
            expected_type: expected.slot(),
            actual_type: actual.slot(),
        },
        Error::ShortCircuitOperandTypeMismatch { node, left, right } => {
            DiagnosticCheckedTemplateProblem::ShortCircuitOperandTypeMismatch {
                node: node.raw(),
                left_type: left.slot(),
                right_type: right.slot(),
            }
        }
        Error::ShortCircuitResultTypeMismatch {
            node,
            expected,
            actual,
        } => DiagnosticCheckedTemplateProblem::ShortCircuitResultTypeMismatch {
            node: node.raw(),
            expected_type: expected.slot(),
            actual_type: actual.slot(),
        },
        Error::ArrayElementTypeMismatch {
            node,
            element,
            expected,
            actual,
        } => DiagnosticCheckedTemplateProblem::ArrayElementTypeMismatch {
            node: node.raw(),
            element: element.raw(),
            expected_type: expected.slot(),
            actual_type: actual.slot(),
        },
    }
}

fn semantic_content_diagnostics(
    error: InterfaceSemanticInternError,
    input: &DependencyInterfaceInput,
) -> DiagnosticBag {
    let (kind, problem) = match error {
        InterfaceSemanticInternError::UnresolvedSymbol(reference) => (
            DiagnosticKind::InterfaceSemanticSymbolUnresolved,
            DiagnosticInterfaceSemanticProblem::UnresolvedSymbol(interface_symbol_reference(
                reference,
            )),
        ),
        InterfaceSemanticInternError::InvalidSymbolKind(reference) => (
            DiagnosticKind::InterfaceSemanticSymbolKindInvalid,
            DiagnosticInterfaceSemanticProblem::InvalidSymbolKind(interface_symbol_reference(
                reference,
            )),
        ),
        InterfaceSemanticInternError::UnresolvedValueGraph => (
            DiagnosticKind::InterfaceSemanticValueGraphInvalid,
            DiagnosticInterfaceSemanticProblem::UnresolvedValueGraph,
        ),
        InterfaceSemanticInternError::SemanticStore(error) => (
            DiagnosticKind::InterfaceSemanticValueInvalid,
            DiagnosticInterfaceSemanticProblem::SemanticContent(semantic_content_problem(error)),
        ),
        InterfaceSemanticInternError::InvalidTemplate(error) => (
            DiagnosticKind::InterfaceExecutableTemplateInvalid,
            DiagnosticInterfaceSemanticProblem::InvalidTemplate(checked_template_problem(error)),
        ),
        InterfaceSemanticInternError::InvalidSupportEntity(entity) => (
            DiagnosticKind::InterfaceSupportEntityInvalid,
            DiagnosticInterfaceSemanticProblem::InvalidSupportEntity(entity.raw()),
        ),
    };

    contextual_interface_diagnostic(
        Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
            .with_arg(DiagnosticArg::interface_semantic_problem(problem)),
        input,
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use bray_bound_tree::CheckedTemplateKind;
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticInterfaceLimit,
        DiagnosticKind,
    };
    use bray_package_interface::{
        DependencyInterfaceId, ImportedSemanticRecord, ImportedSymbolConstructionError,
        InterfaceContentHash, InterfaceLanguageRevision, InterfacePredicateDefinitionState,
        InterfaceProductIdentity, InterfaceSemanticRecordKind, InterfaceSemanticInternError,
        InterfaceSymbolReference, InterfaceValidationPolicy,
        test_support::encoded_semantic_test_interface,
    };
    use bray_source::{SourceId, SourceIdentity, SourceInput, SourceSpan, SourceVersion, TextSize};
    use bray_standard_library::{
        STANDARD_LIBRARY_MANIFEST_FILE_NAME, StandardLibraryArtifact, StandardLibraryArtifactKind,
        StandardLibraryBundleManifest, StandardLibraryRoot, StandardLibraryTargetArtifacts,
        encode_standard_library_manifest, standard_library_target_artifact_directory,
    };
    use bray_symbols::{
        ExternalSymbolKey, ImportedInterfaceId, ImportedSymbolSkeletonBuildError,
        InterfaceSupportEntityId, InterfaceSymbolId, PackageIdentity, SemanticValueKind,
        SemanticValueStoreError, SymbolId, SymbolKey, SymbolKind,
    };
    use bray_target::TargetIdentity;

    use crate::test_support::diagnostic_kinds;
    use crate::{
        CancellationToken, Compilation, CompilationRequest, DependencyInterfaceInput,
        FactQueryError, ImportedSemanticRecordKey,
    };

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

        bray_testing::assert_goal_state_diagnostics(first.diagnostics());

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
    fn standard_library_bundle_failures_publish_exact_structured_diagnostics() {
        let malformed_directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary root must exist: {error}"));

        fs::write(
            malformed_directory
                .path()
                .join(STANDARD_LIBRARY_MANIFEST_FILE_NAME),
            b"{}",
        )
        .unwrap_or_else(|error| panic!("malformed manifest must be written: {error}"));

        let malformed = compilation_with_standard_library_root(
            StandardLibraryRoot::try_new(malformed_directory.path())
                .unwrap_or_else(|| panic!("temporary root must be absolute")),
        );

        let malformed_result = malformed
            .dependency_interface_result(interface_id(&malformed, "std", "library"))
            .unwrap_or_else(|| panic!("synthetic standard library dependency must exist"));

        assert_eq!(
            diagnostic_kinds(malformed_result.diagnostics()),
            [DiagnosticKind::StandardLibraryManifestInvalid]
        );

        let [malformed_diagnostic] = malformed_result.diagnostics().diagnostics() else {
            panic!("malformed manifest must produce one diagnostic");
        };

        assert_eq!(
            malformed_diagnostic.args(),
            [
                DiagnosticArg::file_path(
                    malformed_directory
                        .path()
                        .join(STANDARD_LIBRARY_MANIFEST_FILE_NAME),
                ),
                DiagnosticArg::standard_library_manifest_problem(
                    bray_diagnostics::DiagnosticStandardLibraryManifestProblem::Malformed,
                ),
            ]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            malformed_result.diagnostics(),
            DiagnosticKind::StandardLibraryManifestInvalid,
        );

        let unavailable_directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary root must exist: {error}"));

        let selected_target_name = "x86_64-unknown-linux-gnu";
        let unavailable_target_name = "aarch64-unknown-linux-gnu";

        let unavailable_target = TargetIdentity::try_new(unavailable_target_name)
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        let unavailable_root = write_standard_library_fixture(
            unavailable_directory.path(),
            unavailable_target,
            b"interface",
            b"interface",
        );

        let unavailable = compilation_with_standard_library_root(unavailable_root);

        let unavailable_result = unavailable
            .dependency_interface_result(interface_id(&unavailable, "std", "library"))
            .unwrap_or_else(|| panic!("synthetic standard library dependency must exist"));

        let [unavailable_diagnostic] = unavailable_result.diagnostics().diagnostics() else {
            panic!("unavailable target must produce one diagnostic");
        };

        assert_eq!(
            unavailable_diagnostic.kind(),
            DiagnosticKind::StandardLibraryTargetUnavailable
        );

        assert_eq!(
            unavailable_diagnostic.args(),
            [DiagnosticArg::target_triple(selected_target_name)]
        );

        bray_testing::assert_goal_state_diagnostic(unavailable_diagnostic);

        let mismatch_directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary root must exist: {error}"));

        let selected_target = TargetIdentity::try_new(selected_target_name)
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        let mismatch_root = write_standard_library_fixture(
            mismatch_directory.path(),
            selected_target,
            b"interface",
            b"different",
        );

        let mismatch = compilation_with_standard_library_root(mismatch_root);

        let mismatch_result = mismatch
            .dependency_interface_result(interface_id(&mismatch, "std", "library"))
            .unwrap_or_else(|| panic!("synthetic standard library dependency must exist"));

        let [mismatch_diagnostic] = mismatch_result.diagnostics().diagnostics() else {
            panic!("digest mismatch must produce one diagnostic");
        };

        assert_eq!(
            mismatch_diagnostic.kind(),
            DiagnosticKind::StandardLibraryArtifactDigestMismatch
        );

        assert_eq!(
            mismatch_diagnostic
                .args()
                .iter()
                .map(|argument| argument.name())
                .collect::<Vec<_>>(),
            [
                DiagnosticArgName::FilePath,
                DiagnosticArgName::ExpectedArtifactDigest,
                DiagnosticArgName::ActualArtifactDigest,
            ]
        );

        bray_testing::assert_goal_state_diagnostic(mismatch_diagnostic);
    }

    #[test]
    fn unavailable_standard_library_imports_publish_diagnostics_without_panicking() {
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
                "module application;\nusing std.io;\n",
            )],
        )
        .with_standard_library_root(root);

        let compilation = Compilation::load(request)
            .unwrap_or_else(|error| panic!("I/O-free compilation load must succeed: {error:?}"));

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::StandardLibraryArtifactReadFailed)
        );
    }

    #[test]
    fn dependency_interface_identity_mismatches_publish_exact_diagnostics() {
        let fixture = encoded_semantic_test_interface();

        let expected_package = package("example.other");

        let package_dependency = DependencyInterfaceInput::new(
            expected_package.clone(),
            fixture.product.clone(),
            "package-identity-mismatch.brayi",
            Arc::<[u8]>::from(fixture.bytes.clone()),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let package_compilation = compilation([package_dependency]);

        let package_interface = package_compilation
            .dependency_interface_id(&expected_package, &fixture.product)
            .unwrap_or_else(|| panic!("test dependency interface must have an ID"));

        let package_result = package_compilation
            .dependency_interface_result(package_interface)
            .unwrap_or_else(|| panic!("selected dependency interface must have a result"));

        assert_eq!(
            diagnostic_kinds(package_result.diagnostics()),
            [DiagnosticKind::InterfacePackageIdentityMismatch]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            package_result.diagnostics(),
            DiagnosticKind::InterfacePackageIdentityMismatch,
        );

        let expected_product = product("other");

        let product_dependency = DependencyInterfaceInput::new(
            fixture.package.clone(),
            expected_product.clone(),
            "product-identity-mismatch.brayi",
            Arc::<[u8]>::from(fixture.bytes),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let product_compilation = compilation([product_dependency]);

        let product_interface = product_compilation
            .dependency_interface_id(&fixture.package, &expected_product)
            .unwrap_or_else(|| panic!("test dependency interface must have an ID"));

        let product_result = product_compilation
            .dependency_interface_result(product_interface)
            .unwrap_or_else(|| panic!("selected dependency interface must have a result"));

        assert_eq!(
            diagnostic_kinds(product_result.diagnostics()),
            [DiagnosticKind::InterfaceProductIdentityMismatch]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            product_result.diagnostics(),
            DiagnosticKind::InterfaceProductIdentityMismatch,
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
    fn valid_dependencies_publish_cached_skeleton_and_exact_template_semantics() {
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

        let key = ImportedSemanticRecordKey::new(
            interface,
            fixture.template_owner,
            InterfaceSemanticRecordKind::DeclarationTemplate,
        );

        let first = compilation
            .imported_semantics(key)
            .unwrap_or_else(|error| panic!("semantic query must complete: {error:?}"));

        let second = compilation
            .imported_semantics(key)
            .unwrap_or_else(|error| panic!("semantic query must complete: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());

        let [ImportedSemanticRecord::DeclarationTemplate(template)] = first.value().as_ref() else {
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

        let signature_key = ImportedSemanticRecordKey::new(
            interface,
            fixture.template_owner,
            InterfaceSemanticRecordKind::CallableSignature,
        );

        let signatures = std::thread::scope(|scope| {
            let first = scope.spawn(|| compilation.imported_semantics(signature_key));
            let second = scope.spawn(|| compilation.imported_semantics(signature_key));

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
            [ImportedSemanticRecord::CallableSignature(_)]
        ));

        let interface_index = interface
            .to_index()
            .unwrap_or_else(|| panic!("test interface ID must fit the host index"));

        assert!(
            compilation.state.imported_semantic_graphs[interface_index]
                .get()
                .is_some()
        );
    }

    #[test]
    fn predicate_definition_states_decode_without_unrelated_semantics() {
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
            let key = ImportedSemanticRecordKey::new(
                interface,
                owner,
                InterfaceSemanticRecordKind::PredicateDefinition,
            );

            let result = compilation
                .imported_semantics(key)
                .unwrap_or_else(|error| panic!("predicate query must complete: {error:?}"));

            let [ImportedSemanticRecord::PredicateDefinition(definition)] = result.value().as_ref()
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
    fn implementation_header_semantics_are_narrow_cached_and_complete() {
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

        let implementation_key = ImportedSemanticRecordKey::new(
            interface,
            fixture.implementation_owner,
            InterfaceSemanticRecordKind::Implementation,
        );

        let results = std::thread::scope(|scope| {
            let first =
                scope.spawn(|| compilation.imported_semantics(implementation_key));

            let second =
                scope.spawn(|| compilation.imported_semantics(implementation_key));

            [first, second].map(|thread| {
                thread
                    .join()
                    .unwrap_or_else(|_| panic!("implementation query thread must not panic"))
                    .unwrap_or_else(|error| {
                        panic!("implementation query must complete: {error:?}")
                    })
            })
        });

        assert!(Arc::ptr_eq(&results[0], &results[1]));
        assert!(results[0].diagnostics().is_empty());

        let [ImportedSemanticRecord::Implementation(implementation)] = results[0].value().as_ref()
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
    fn cancelled_exact_semantic_queries_do_not_publish_results() {
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

        let key = ImportedSemanticRecordKey::new(
            interface,
            fixture.template_owner,
            InterfaceSemanticRecordKind::DeclarationTemplate,
        );

        let cancellation = CancellationToken::new();

        cancellation.cancel();

        assert_eq!(
            compilation.imported_semantics_with_cancellation(key, &cancellation),
            Err(FactQueryError::Cancelled)
        );

        assert_eq!(
            compilation.state.imported_semantics.is_published(&key),
            Ok(false)
        );
    }

    #[test]
    fn compiler_identity_capacity_diagnostics_preserve_owner_category_and_bounds() {
        let compilation = compilation([]);

        let interface_capacity = super::dependency_graph_capacity_diagnostics(
            &compilation,
            DiagnosticInterfaceLimit::LoadedInterfaceCount,
            ImportedInterfaceId::CAPACITY + 1,
            ImportedInterfaceId::CAPACITY,
        );

        let [interface_diagnostic] = interface_capacity.diagnostics() else {
            panic!("interface capacity must produce one diagnostic");
        };

        assert_eq!(
            interface_diagnostic.args(),
            [
                DiagnosticArg::actual_package_identity("example.current"),
                DiagnosticArg::interface_limit(DiagnosticInterfaceLimit::LoadedInterfaceCount),
                DiagnosticArg::actual_count(ImportedInterfaceId::CAPACITY + 1),
                DiagnosticArg::maximum_count(ImportedInterfaceId::CAPACITY),
            ]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &interface_capacity,
            DiagnosticKind::InterfaceSymbolCapacityExceeded,
        );

        let symbol_capacity = super::dependency_graph_capacity_diagnostics(
            &compilation,
            DiagnosticInterfaceLimit::CompilationSymbolCount,
            SymbolId::CAPACITY + 7,
            SymbolId::CAPACITY,
        );

        let [symbol_diagnostic] = symbol_capacity.diagnostics() else {
            panic!("symbol capacity must produce one diagnostic");
        };

        assert_eq!(
            symbol_diagnostic.args(),
            [
                DiagnosticArg::actual_package_identity("example.current"),
                DiagnosticArg::interface_limit(DiagnosticInterfaceLimit::CompilationSymbolCount,),
                DiagnosticArg::actual_count(SymbolId::CAPACITY + 7),
                DiagnosticArg::maximum_count(SymbolId::CAPACITY),
            ]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &symbol_capacity,
            DiagnosticKind::InterfaceSymbolCapacityExceeded,
        );
    }

    #[test]
    fn imported_symbol_construction_failures_preserve_owners_and_exact_causes() {
        let alpha = dependency("example.alpha", "main")
            .with_dependency_span(SourceSpan::empty(SourceId::new(1), TextSize::new(1)));

        let beta = dependency("example.beta", "main")
            .with_dependency_span(SourceSpan::empty(SourceId::new(1), TextSize::new(2)));

        let compilation = compilation([alpha, beta]);
        let first = ImportedInterfaceId::new(0);
        let duplicate = ImportedInterfaceId::new(1);

        let duplicate_package = super::dependency_graph_diagnostics(
            &compilation,
            ImportedSymbolConstructionError::DuplicatePackage {
                package: package("example.shared"),
                first,
                duplicate,
            },
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &duplicate_package,
            DiagnosticKind::InterfaceDuplicatePackage,
        );

        assert_eq!(
            duplicate_package.diagnostics()[0].related_locations().len(),
            1
        );

        let missing_dependency = super::dependency_graph_diagnostics(
            &compilation,
            ImportedSymbolConstructionError::MissingDependency {
                importing: first,
                package: package("example.missing"),
            },
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &missing_dependency,
            DiagnosticKind::InterfaceMissingDependency,
        );

        let product_mismatch = super::dependency_graph_diagnostics(
            &compilation,
            ImportedSymbolConstructionError::DependencyProductMismatch {
                package: package("example.beta"),
                importing: first,
                expected: product("library"),
                actual: product("main"),
            },
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &product_mismatch,
            DiagnosticKind::InterfaceDependencyProductMismatch,
        );

        let content_mismatch = super::dependency_graph_diagnostics(
            &compilation,
            ImportedSymbolConstructionError::DependencyContentMismatch {
                package: package("example.beta"),
                importing: first,
                expected: InterfaceContentHash::from_bytes([1; 32]),
                actual: InterfaceContentHash::from_bytes([2; 32]),
            },
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &content_mismatch,
            DiagnosticKind::InterfaceDependencyContentMismatch,
        );

        let symbol_reference = super::dependency_graph_diagnostics(
            &compilation,
            ImportedSymbolConstructionError::SymbolOutOfBounds {
                interface: first,
                symbol: InterfaceSymbolId::new(27),
            },
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &symbol_reference,
            DiagnosticKind::InterfaceSymbolReferenceInvalid,
        );

        let dependency_reference = super::dependency_graph_diagnostics(
            &compilation,
            ImportedSymbolConstructionError::DependencyOutOfBounds {
                interface: first,
                dependency: DependencyInterfaceId::new(8),
            },
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &dependency_reference,
            DiagnosticKind::InterfaceDependencyReferenceInvalid,
        );

        let external = ExternalSymbolKey::package(package("example.missing"));

        let missing_symbol = super::dependency_graph_diagnostics(
            &compilation,
            ImportedSymbolConstructionError::MissingDependencySymbol {
                importing: first,
                package: package("example.missing"),
                key: external.clone(),
            },
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &missing_symbol,
            DiagnosticKind::InterfaceDependencySymbolMissing,
        );

        let compiler_key =
            bray_compiler_known::CompilerKnownDeclarationKey::try_new("CompilerProvidedFunction")
                .unwrap_or_else(|| panic!("test compiler-known key must be valid"));

        let compiler_symbol =
            SymbolKey::compiler_known_declaration(compiler_key, SymbolKind::Function)
                .unwrap_or_else(|| panic!("test compiler-known function key must be valid"));

        let compiler_export = super::dependency_graph_diagnostics(
            &compilation,
            ImportedSymbolConstructionError::CompilerKnownExportTarget {
                importing: first,
                key: compiler_symbol,
            },
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &compiler_export,
            DiagnosticKind::InterfaceCompilerDeclarationExported,
        );

        let invalid_graph = super::dependency_graph_diagnostics(
            &compilation,
            ImportedSymbolConstructionError::Symbols(
                ImportedSymbolSkeletonBuildError::DuplicateExternalKey {
                    key: external,
                    first,
                    duplicate,
                },
            ),
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &invalid_graph,
            DiagnosticKind::InterfaceSymbolGraphInvalid,
        );

        assert_eq!(invalid_graph.diagnostics()[0].related_locations().len(), 1);
    }

    #[test]
    fn imported_semantic_failures_preserve_artifact_owner_and_nested_cause() {
        let input = dependency("example.alpha", "main")
            .with_dependency_span(SourceSpan::empty(SourceId::new(1), TextSize::new(3)));

        let unresolved = super::semantic_content_diagnostics(
            InterfaceSemanticInternError::UnresolvedSymbol(InterfaceSymbolReference::Local(
                InterfaceSymbolId::new(7),
            )),
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &unresolved,
            DiagnosticKind::InterfaceSemanticSymbolUnresolved,
        );

        let invalid_kind = super::semantic_content_diagnostics(
            InterfaceSemanticInternError::InvalidSymbolKind(InterfaceSymbolReference::Local(
                InterfaceSymbolId::new(8),
            )),
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &invalid_kind,
            DiagnosticKind::InterfaceSemanticSymbolKindInvalid,
        );

        let value_graph = super::semantic_content_diagnostics(
            InterfaceSemanticInternError::UnresolvedValueGraph,
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &value_graph,
            DiagnosticKind::InterfaceSemanticValueGraphInvalid,
        );

        let semantic_value = super::semantic_content_diagnostics(
            InterfaceSemanticInternError::SemanticStore(SemanticValueStoreError::UnknownId {
                kind: SemanticValueKind::Type,
            }),
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &semantic_value,
            DiagnosticKind::InterfaceSemanticValueInvalid,
        );

        let template = super::semantic_content_diagnostics(
            InterfaceSemanticInternError::InvalidTemplate(
                bray_bound_tree::CheckedTemplateBuildError::CapacityExceeded,
            ),
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &template,
            DiagnosticKind::InterfaceExecutableTemplateInvalid,
        );

        let support = super::semantic_content_diagnostics(
            InterfaceSemanticInternError::InvalidSupportEntity(InterfaceSupportEntityId::new(5)),
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &support,
            DiagnosticKind::InterfaceSupportEntityInvalid,
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

    fn compilation_with_standard_library_root(root: StandardLibraryRoot) -> Compilation {
        let request = CompilationRequest::new(
            package("example.current"),
            vec![SourceInput::virtual_text(
                SourceIdentity::new(1),
                "main.bray",
                SourceVersion::new(1),
                "module example.current;",
            )],
        )
        .with_standard_library_root(root);

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn write_standard_library_fixture(
        root: &Path,
        target: TargetIdentity,
        expected_interface: &[u8],
        actual_interface: &[u8],
    ) -> StandardLibraryRoot {
        let runtime_abi = bray_runtime_interface::RuntimeAbiVersion::new(1, 0);
        let prefix = standard_library_target_artifact_directory(&target, runtime_abi);

        let interface = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PackageInterface,
            format!("{prefix}/std.brayi"),
            expected_interface,
        )
        .unwrap_or_else(|error| panic!("interface metadata must be valid: {error:?}"));

        let implementation = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PackageImplementation,
            format!("{prefix}/std.brayimpl"),
            b"implementation",
        )
        .unwrap_or_else(|error| panic!("implementation metadata must be valid: {error:?}"));

        let inventory = StandardLibraryTargetArtifacts::try_new(
            target,
            runtime_abi,
            [interface.clone(), implementation.clone()],
        )
        .unwrap_or_else(|error| panic!("target inventory must be valid: {error:?}"));

        let manifest = StandardLibraryBundleManifest::try_new([inventory])
            .unwrap_or_else(|error| panic!("manifest must be valid: {error:?}"));

        let interface_path = interface.beneath(root);

        fs::create_dir_all(
            interface_path
                .parent()
                .unwrap_or_else(|| panic!("interface must have a parent")),
        )
        .unwrap_or_else(|error| panic!("target directory must be created: {error}"));

        fs::write(interface_path, actual_interface)
            .unwrap_or_else(|error| panic!("interface must be written: {error}"));

        fs::write(implementation.beneath(root), b"implementation")
            .unwrap_or_else(|error| panic!("implementation must be written: {error}"));

        let manifest = encode_standard_library_manifest(&manifest)
            .unwrap_or_else(|error| panic!("manifest must encode: {error:?}"));

        fs::write(root.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME), manifest)
            .unwrap_or_else(|error| panic!("manifest must be written: {error}"));

        StandardLibraryRoot::try_new(root)
            .unwrap_or_else(|| panic!("temporary root must be absolute"))
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
