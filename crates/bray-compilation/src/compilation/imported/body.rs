use std::sync::Arc;

use bray_bound_tree::CheckedTemplate;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_package_interface::InterfaceSymbolResolver;
use bray_symbols::{AnySymbolId, ImportedSemanticAddress};

use super::diagnostic::{
    dependency_artifact_diagnostics, executable_template_decode_diagnostics,
    executable_template_diagnostics, implementation_body_diagnostics,
    implementation_validation_diagnostics,
};
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, ImportedExecutableTemplateAddress,
};

pub(in crate::compilation) type LoadedImplementation = Result<
    Arc<bray_package_interface::PackageImplementationArtifact>,
    bray_package_interface::InterfaceValidationError,
>;

impl super::super::Compilation {
    pub(in crate::compilation) fn loaded_dependency_implementation_with_cancellation(
        &self,
        interface: bray_symbols::ImportedInterfaceId,
        cancellation: &CancellationToken,
    ) -> Result<Option<&DiagnosticResult<Option<LoadedImplementation>>>, FactQueryError> {
        let Some(index) = interface.to_index() else {
            return Ok(None);
        };

        if self.state.dependency_interfaces.get(index).is_none() {
            return Ok(None);
        }

        let Some(cache) = self.state.loaded_dependency_implementations.get(index) else {
            return Ok(None);
        };

        self.query_with_cancellation(
            CompilationFactKey::DependencyImplementation(interface),
            cache,
            cancellation,
            |_| {
                self.record_dependency_implementation(interface);

                let input = self.dependency_interface(interface).unwrap_or_else(|| {
                    panic!(
                        "imported publication invariant MissingDependencyInput: {:?}",
                        interface
                    )
                });

                let artifact = match input.shared_implementation_artifact() {
                    Ok(Some(artifact)) => artifact,
                    Ok(None) => return Ok(DiagnosticResult::without_diagnostics(None)),
                    Err(error) => {
                        return Ok(DiagnosticResult::new(
                            None,
                            dependency_artifact_diagnostics(
                                error,
                                input,
                                input.implementation_artifact_path().expect(
                                    "implementation load failure requires a selected input",
                                ),
                            ),
                        ));
                    }
                };

                let loaded = self
                    .loaded_dependency_interface_with_cancellation(interface, cancellation)?
                    .expect("implementation input must have a loaded dependency interface");

                let (Some(validated), Some(surface)) = (loaded.validated(), loaded.surface())
                else {
                    return Ok(DiagnosticResult::without_diagnostics(None));
                };

                let configuration = self
                    .package_implementation_configuration(None)
                    .map_err(FactQueryError::CodegenTarget)?;

                let artifact = artifact
                    .validate_interface(validated, surface)
                    .and_then(|()| artifact.validate_configuration(&configuration))
                    .map(|()| Arc::new(artifact));

                Ok(DiagnosticResult::without_diagnostics(Some(artifact)))
            },
        )
        .map(Some)
    }

    pub(in crate::compilation) fn imported_native_boundary_with_cancellation(
        &self,
        symbol: AnySymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Option<bray_package_interface::InterfaceNativeBoundary>, FactQueryError> {
        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(skeleton) = skeleton.value() else {
            return Ok(None);
        };

        let Some(address) = skeleton.imported_semantic_address(symbol) else {
            return Ok(None);
        };

        let loaded = self
            .loaded_dependency_interface_with_cancellation(address.interface(), cancellation)?
            .unwrap_or_else(|| {
                panic!(
                    "imported publication invariant MissingLoadedInterface: {:?}",
                    address.interface()
                )
            });

        if loaded.validated().is_none() {
            return Ok(None);
        }

        let implementation = self
            .loaded_dependency_implementation_with_cancellation(address.interface(), cancellation)?
            .unwrap_or_else(|| {
                panic!(
                    "imported publication invariant MissingLoadedImplementation: {:?}",
                    address.interface()
                )
            });

        let Some(implementation) = implementation.value() else {
            return Ok(None);
        };

        let implementation = implementation
            .as_ref()
            .map_err(|error| FactQueryError::from(error.clone()))?;

        implementation
            .native_boundary(address.symbol())
            .map_err(FactQueryError::from)
    }

    pub(in crate::compilation) fn imported_executable_template_key_with_cancellation(
        &self,
        address: ImportedExecutableTemplateAddress,
        owner: AnySymbolId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<bray_ir::MirImportedExecutableKey>>, FactQueryError> {
        let symbol = address.symbol();

        let loaded = self
            .loaded_dependency_interface_with_cancellation(symbol.interface(), cancellation)?
            .expect("imported executable identity must have a loaded dependency interface");

        if loaded.validated().is_none() {
            return Ok(DiagnosticResult::without_diagnostics(None));
        }

        let artifact = self.imported_body_artifact(
            symbol.interface(),
            executable_template_diagnostics,
            cancellation,
        )?;

        let Some(artifact) = artifact.value() else {
            return Ok(DiagnosticResult::new(None, artifact.diagnostics().clone()));
        };

        if let Some(key) =
            artifact.executable_template_key(symbol.symbol(), owner, address.template())
        {
            return Ok(DiagnosticResult::without_diagnostics(Some(key)));
        }

        let input = self
            .dependency_interface_input(symbol.interface())
            .expect("imported executable identity must have a dependency input");

        Ok(DiagnosticResult::new(
            None,
            executable_template_diagnostics(input),
        ))
    }

    pub(in crate::compilation) fn imported_executable_template_with_cancellation(
        &self,
        address: ImportedExecutableTemplateAddress,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<Arc<bray_ir::MirUnit>>>>, FactQueryError> {
        let cell = self.state.imported_executable_templates.cell(address)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::ImportedExecutableTemplate(address),
            cancellation,
            || {
                self.compute_imported_executable_template(address, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(result))
    }

    fn imported_body_artifact(
        &self,
        interface: bray_symbols::ImportedInterfaceId,
        unavailable: fn(&crate::request::DependencyInterfaceInput) -> DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<Option<&bray_package_interface::PackageImplementationArtifact>>,
        FactQueryError,
    > {
        let input = self
            .dependency_interface_input(interface)
            .expect("imported body must have a dependency input");

        let loaded = self
            .loaded_dependency_implementation_with_cancellation(interface, cancellation)?
            .expect("imported body must have an implementation query");

        Ok(match loaded.value() {
            Some(Ok(artifact)) => DiagnosticResult::without_diagnostics(Some(artifact.as_ref())),
            Some(Err(error)) => DiagnosticResult::new(
                None,
                // The immutable query cache retains the typed cause for non-diagnostic consumers.
                implementation_validation_diagnostics(error.clone(), input),
            ),
            None => DiagnosticResult::new(
                None,
                DiagnosticBag::merged_all([loaded.diagnostics(), &unavailable(input)]),
            ),
        })
    }

    fn compute_imported_executable_template(
        &self,
        address: ImportedExecutableTemplateAddress,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<Arc<bray_ir::MirUnit>>>, FactQueryError> {
        let symbol_address = address.symbol();

        let input = self
            .dependency_interface_input(symbol_address.interface())
            .unwrap_or_else(|| {
                panic!(
                    "imported publication invariant MissingDependencyInput: {:?}",
                    symbol_address.interface()
                )
            });

        let loaded = self
            .loaded_dependency_interface_with_cancellation(
                symbol_address.interface(),
                cancellation,
            )?
            .unwrap_or_else(|| {
                panic!(
                    "imported publication invariant MissingLoadedInterface: {:?}",
                    symbol_address.interface()
                )
            });

        if loaded.validated().is_none() {
            return Ok(DiagnosticResult::without_diagnostics(None));
        }

        let artifact = self.imported_body_artifact(
            symbol_address.interface(),
            executable_template_diagnostics,
            cancellation,
        )?;

        let Some(artifact) = artifact.value() else {
            return Ok(DiagnosticResult::new(None, artifact.diagnostics().clone()));
        };

        let template =
            match artifact.executable_template(symbol_address.symbol(), address.template()) {
                Ok(Some(template)) => template,
                Ok(None) => {
                    return Ok(DiagnosticResult::new(
                        None,
                        executable_template_diagnostics(input),
                    ));
                }
                Err(error) => {
                    return Ok(DiagnosticResult::new(
                        None,
                        implementation_validation_diagnostics(error, input),
                    ));
                }
            };

        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(skeleton) = skeleton.value() else {
            return Ok(DiagnosticResult::new(None, skeleton.diagnostics().clone()));
        };

        let graph = self
            .imported_semantic_graph_result_with_cancellation(
                symbol_address.interface(),
                cancellation,
            )?
            .unwrap_or_else(|| {
                panic!(
                    "imported publication invariant MissingLoadedSemanticGraph: {:?}",
                    symbol_address.interface()
                )
            });

        let Some(graph) = graph.value() else {
            return Ok(DiagnosticResult::new(None, graph.diagnostics().clone()));
        };

        let resolver =
            self.imported_symbol_resolver(symbol_address.interface(), skeleton, cancellation)?;

        let owner = resolver
            .resolve(&bray_package_interface::InterfaceSymbolReference::Local(
                symbol_address.symbol(),
            ))
            .unwrap_or_else(|| {
                panic!(
                    "imported publication invariant MissingResolvedSymbol: {:?}",
                    symbol_address
                )
            });

        let selected_target = self.selected_target().target();

        let target = bray_ir::MirTargetContract::new(
            selected_target.profile().clone(),
            selected_target.runtime_abi(),
        );

        let template = match bray_package_interface::decode_executable_template(
            &template,
            owner,
            bray_ir::MirUnitId::new(0),
            target,
            graph,
            &resolver,
            input.validation_policy().limits(),
        ) {
            Ok(template) => template,
            Err(error) => {
                return Ok(DiagnosticResult::new(
                    None,
                    executable_template_decode_diagnostics(input, error),
                ));
            }
        };

        Ok(DiagnosticResult::without_diagnostics(Some(Arc::new(
            template,
        ))))
    }

    pub(in crate::compilation) fn imported_executable_mir(
        &self,
        key: bray_ir::MirImportedExecutableKey,
        unit: bray_ir::MirUnitId,
        target: bray_ir::MirTargetContract,
        cancellation: &CancellationToken,
    ) -> Result<Option<bray_ir::MirUnit>, crate::CodegenPreparationError> {
        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(skeleton) = skeleton.value() else {
            return Ok(None);
        };

        let Some(address) = skeleton.imported_semantic_address(key.owner()) else {
            return Ok(None);
        };

        let address = ImportedExecutableTemplateAddress::new(address, key.template());

        let template =
            self.imported_executable_template_with_cancellation(address, cancellation)?;

        let Some(template) = template.value() else {
            if template.diagnostics().has_errors() {
                return Err(crate::CodegenPreparationError::Diagnostics(
                    template.diagnostics().clone(),
                ));
            }

            return Ok(None);
        };

        assert_eq!(
            template.unit(),
            unit,
            "imported template unit at {address:?}"
        );

        assert_eq!(
            template.key(),
            &bray_ir::MirUnitKey::ImportedExecutable(key),
            "imported template key at {address:?}"
        );

        assert_eq!(
            template.target(),
            &target,
            "imported template target at {address:?}"
        );

        Ok(Some((**template).clone()))
    }

    pub(in crate::compilation) fn imported_constant_callable_body_with_cancellation(
        &self,
        address: ImportedSemanticAddress,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<Arc<CheckedTemplate>>>>, FactQueryError> {
        let cell = self.state.imported_constant_callable_bodies.cell(address)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::ImportedConstantCallableBody(address),
            cancellation,
            || {
                self.compute_imported_constant_callable_body(address, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(result))
    }

    fn compute_imported_constant_callable_body(
        &self,
        address: ImportedSemanticAddress,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<Arc<CheckedTemplate>>>, FactQueryError> {
        let input = self
            .dependency_interface_input(address.interface())
            .unwrap_or_else(|| {
                panic!(
                    "imported publication invariant MissingDependencyInput: {:?}",
                    address.interface()
                )
            });

        let loaded = self
            .loaded_dependency_interface_with_cancellation(address.interface(), cancellation)?
            .unwrap_or_else(|| {
                panic!(
                    "imported publication invariant MissingLoadedInterface: {:?}",
                    address.interface()
                )
            });

        let Some(surface) = loaded.surface() else {
            return Ok(DiagnosticResult::without_diagnostics(None));
        };

        let artifact = self.imported_body_artifact(
            address.interface(),
            implementation_body_diagnostics,
            cancellation,
        )?;

        let Some(artifact) = artifact.value() else {
            return Ok(DiagnosticResult::new(None, artifact.diagnostics().clone()));
        };

        let graph_result = self
            .imported_semantic_graph_result_with_cancellation(address.interface(), cancellation)?
            .unwrap_or_else(|| {
                panic!(
                    "imported publication invariant MissingLoadedSemanticGraph: {:?}",
                    address.interface()
                )
            });

        let Some(graph) = graph_result.value() else {
            return Ok(DiagnosticResult::new(
                None,
                graph_result.diagnostics().clone(),
            ));
        };

        let body = match artifact.constant_callable_body(address.symbol(), surface) {
            Ok(Some(body)) => body,
            Ok(None) => {
                return Ok(DiagnosticResult::new(
                    None,
                    implementation_body_diagnostics(input),
                ));
            }
            Err(error) => {
                return Ok(DiagnosticResult::new(
                    None,
                    implementation_validation_diagnostics(error, input),
                ));
            }
        };

        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(skeleton) = skeleton.value() else {
            return Ok(DiagnosticResult::new(None, skeleton.diagnostics().clone()));
        };

        cancellation.check()?;

        let resolver =
            self.imported_symbol_resolver(address.interface(), skeleton, cancellation)?;

        let template = match graph.intern_checked_template(body.template(), &resolver) {
            Ok(template) => template,
            Err(_) => {
                return Ok(DiagnosticResult::new(
                    None,
                    DiagnosticBag::merged_all([
                        graph_result.diagnostics(),
                        &implementation_body_diagnostics(input),
                    ]),
                ));
            }
        };

        Ok(DiagnosticResult::new(
            Some(Arc::new(template)),
            graph_result.diagnostics().clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_bound_tree::CheckedTemplateKind;
    use bray_diagnostics::DiagnosticKind;
    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceValidationLimits, InterfaceValidationPolicy,
        PackageImplementationArtifact, ValidatedPackageInterface, encode_package_interface,
        test_support::package_interface_export_bundle,
    };
    use bray_symbols::ImportedSemanticAddress;

    use crate::fact::ImportedExecutableTemplateAddress;
    use crate::test_support::compilation_with_dependencies;
    use crate::{Compilation, DependencyInterfaceInput};

    #[test]
    fn imported_constant_bodies_are_loaded_once_by_exact_symbol_address() {
        let (compilation, owner) = imported_body_compilation(true);

        let address = constant_body_address(&compilation, owner);

        let first = compilation
            .imported_constant_callable_body_with_cancellation(
                address,
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("imported constant body must load: {error:?}"));

        let second = compilation
            .imported_constant_callable_body_with_cancellation(
                address,
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("imported constant body must be cached: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());

        let Some(body) = first.value() else {
            panic!("matching implementation artifact must publish the requested body");
        };

        assert_eq!(body.kind(), CheckedTemplateKind::ConstantCallableBody);
    }

    #[test]
    fn missing_imported_constant_bodies_publish_structured_diagnostics() {
        let (compilation, owner) = imported_body_compilation(false);

        let address = constant_body_address(&compilation, owner);

        let result = compilation
            .imported_constant_callable_body_with_cancellation(
                address,
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("missing imported body must recover: {error:?}"));

        assert!(result.value().is_none());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::InterfaceConstantCallableBodyUnavailable)
                .count(),
            1
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::InterfaceConstantCallableBodyUnavailable,
        );
    }

    #[test]
    fn missing_imported_executable_templates_publish_structured_diagnostics() {
        let (compilation, owner) = imported_body_compilation(true);

        let address =
            ImportedExecutableTemplateAddress::root(constant_body_address(&compilation, owner));

        let result = compilation
            .imported_executable_template_with_cancellation(
                address,
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("missing executable template must recover: {error:?}"));

        assert!(result.value().is_none());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::InterfaceExecutableTemplateUnavailable)
                .count(),
            1
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::InterfaceExecutableTemplateUnavailable,
        );
    }

    fn imported_body_compilation(
        include_artifact: bool,
    ) -> (Compilation, bray_symbols::InterfaceSymbolId) {
        let bundle = package_interface_export_bundle();

        let encoded = encode_package_interface(&bundle)
            .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

        let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

        let interface = ValidatedPackageInterface::try_new(encoded.bytes(), policy)
            .unwrap_or_else(|error| panic!("test interface must validate: {error:?}"));

        let body = bray_package_interface::test_support::constant_callable_body(&bundle);
        let owner = body.owner();

        let artifact = PackageImplementationArtifact::try_new(
            &interface,
            bundle.surface(),
            bundle.semantics(),
            bundle.implementation_configuration().clone(),
            [body],
            [],
            [],
            [],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("test implementation artifact must validate: {error:?}"));

        let mut dependency = DependencyInterfaceInput::new(
            bundle.surface().identity().package().clone(),
            bundle.surface().identity().product().clone(),
            "dependency.brayi",
            Arc::<[u8]>::from(encoded.bytes()),
            policy,
        );

        if include_artifact {
            dependency =
                dependency.with_implementation_artifact("dependency.brayimpl", Arc::new(artifact));
        }

        (
            compilation_with_dependencies("module example.current;", [dependency]),
            owner,
        )
    }

    fn constant_body_address(
        compilation: &Compilation,
        owner: bray_symbols::InterfaceSymbolId,
    ) -> ImportedSemanticAddress {
        let skeleton = compilation
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("imported skeleton must load: {error:?}"));

        let skeleton = skeleton
            .value()
            .as_deref()
            .unwrap_or_else(|| panic!("valid dependency must publish a symbol skeleton"));

        let function = skeleton
            .functions()
            .iter()
            .find(|function| {
                function
                    .imported_semantic_key()
                    .is_some_and(|key| key.symbol() == owner)
            })
            .unwrap_or_else(|| panic!("imported function must retain its interface address"));

        skeleton
            .imported_semantic_address(function.id().into())
            .unwrap_or_else(|| panic!("imported function must publish a semantic address"))
    }
}
