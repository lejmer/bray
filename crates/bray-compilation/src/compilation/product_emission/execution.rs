use bray_codegen::{CodegenMappings, CodegenOptions, CodegenTarget, CodegenUnit};
use bray_diagnostics::DiagnosticBag;
use bray_emitter::{
    ArtifactContribution, ArtifactKind, ArtifactProducer, BackendContributionSet, EmissionBackend,
    EmissionOutcome, EmissionPlan, EmissionPlanner, EmissionRequest, EmissionStatus, LinkStaging,
    LinkStagingError, OutputSinkResolver, ProductLinkInputs, PublicationValidator,
    construct_link_plan,
};
use bray_linker::Linker;
use bray_package_interface::encode_package_interface;
use bray_target::TargetOutputDescription;
use std::path::Path;
use std::sync::Arc;

use super::native::package_native_implementation;
use super::publishing::{
    package_implementation_contribution, publisher, test_catalog_contribution,
};
use super::{ProductEmissionError, ProductEmissionErrorKind};
use crate::compilation::{
    Compilation, EmissionCodegenError, EmissionCodegenErrorKind, NativeProductPlan,
    ProductDataKind, ProductQueryContext, ProductQueryFailure,
};
use crate::fact::{CancellationToken, FactQueryError};

/// Borrowed immutable producer and host inputs for one product emission operation.
#[derive(Clone, Copy)]
pub struct ProductEmissionInputs<'operation> {
    target_outputs: &'operation TargetOutputDescription,
    generation: ProductGenerationInputs<'operation>,
    linking: Option<ProductLinkingInputs<'operation>>,
    test_catalog: Option<&'operation [u8]>,
    sink_resolver: Option<&'operation dyn OutputSinkResolver>,
    publication_validation: Option<&'operation dyn PublicationValidator>,
    native_inspection: Option<NativeInspectionInputs<'operation>>,
}

impl<'operation> ProductEmissionInputs<'operation> {
    /// Creates product inputs from host-selected target output configuration.
    pub const fn new(target_outputs: &'operation TargetOutputDescription) -> Self {
        Self {
            target_outputs,
            generation: ProductGenerationInputs::None,
            linking: None,
            test_catalog: None,
            sink_resolver: None,
            publication_validation: None,
            native_inspection: None,
        }
    }

    /// Supplies an encoded immutable test catalog for transactional product publication.
    pub const fn with_test_catalog(mut self, catalog: &'operation [u8]) -> Self {
        self.test_catalog = Some(catalog);

        self
    }

    /// Supplies selected backend, validated mappings, target, and generation policy.
    pub const fn with_codegen(
        mut self,
        backend: &'operation EmissionBackend,
        mappings: &'operation [CodegenMappings],
        target: &'operation CodegenTarget,
        options: &'operation CodegenOptions,
    ) -> Self {
        self.generation = ProductGenerationInputs::Custom(ProductCodegenInputs {
            backend,
            mappings,
            target,
            options,
        });

        self
    }

    /// Supplies compilation-owned native product inputs and the linker invocation boundary.
    pub const fn with_native_product(
        mut self,
        inputs: &'operation NativeProductPlan,
        linker: &'operation Linker,
    ) -> Self {
        self.generation = ProductGenerationInputs::Native(inputs);

        self.linking = match inputs.link() {
            Some(inputs) => Some(ProductLinkingInputs { linker, inputs }),
            None => None,
        };

        self
    }

    /// Supplies compilation-owned native code generation without final linking.
    pub const fn with_native_codegen(mut self, inputs: &'operation NativeProductPlan) -> Self {
        self.generation = ProductGenerationInputs::Native(inputs);
        self.linking = None;

        self
    }

    /// Supplies selected native link inputs and the linker invocation boundary.
    pub const fn with_linking(
        mut self,
        linker: &'operation Linker,
        inputs: &'operation ProductLinkInputs,
    ) -> Self {
        self.linking = Some(ProductLinkingInputs { linker, inputs });

        self
    }

    /// Supplies the transactional resolver for planned indirect output sinks.
    pub const fn with_sink_resolver(
        mut self,
        resolver: &'operation dyn OutputSinkResolver,
    ) -> Self {
        self.sink_resolver = Some(resolver);

        self
    }

    /// Supplies the selected LLVM tools for inspecting published bitcode units.
    pub const fn with_native_inspection(
        mut self,
        symbols: &'operation Path,
        bitcode: &'operation Path,
    ) -> Self {
        self.native_inspection = Some(NativeInspectionInputs { symbols, bitcode });

        self
    }

    /// Requires one final host validation after generation and before publication.
    pub const fn with_publication_validation(
        mut self,
        validation: &'operation dyn PublicationValidator,
    ) -> Self {
        self.publication_validation = Some(validation);

        self
    }
}

#[derive(Clone, Copy)]
pub(super) struct NativeInspectionInputs<'operation> {
    pub(super) symbols: &'operation Path,
    pub(super) bitcode: &'operation Path,
}

#[derive(Clone, Copy)]
enum ProductGenerationInputs<'operation> {
    None,
    Custom(ProductCodegenInputs<'operation>),
    Native(&'operation NativeProductPlan),
}

impl<'operation> ProductGenerationInputs<'operation> {
    fn codegen(self) -> Option<ProductCodegenInputs<'operation>> {
        match self {
            Self::None => None,
            Self::Custom(codegen) => Some(codegen),
            Self::Native(inputs) => Some(ProductCodegenInputs {
                backend: inputs.backend(),
                mappings: inputs.mappings(),
                target: inputs.target(),
                options: inputs.options(),
            }),
        }
    }

    const fn native(self) -> Option<&'operation NativeProductPlan> {
        match self {
            Self::Native(inputs) => Some(inputs),
            Self::None | Self::Custom(_) => None,
        }
    }
}

#[derive(Clone, Copy)]
struct ProductCodegenInputs<'operation> {
    backend: &'operation EmissionBackend,
    mappings: &'operation [CodegenMappings],
    target: &'operation CodegenTarget,
    options: &'operation CodegenOptions,
}

#[derive(Clone, Copy)]
struct ProductLinkingInputs<'operation> {
    linker: &'operation Linker,
    inputs: &'operation ProductLinkInputs,
}

impl Compilation {
    /// Plans, realizes, links, and publishes one compiler product.
    pub fn emit_product(
        &self,
        request: EmissionRequest,
        inputs: ProductEmissionInputs<'_>,
    ) -> Result<EmissionOutcome, ProductEmissionError> {
        self.emit_product_with_cancellation(request, inputs, &self.state.cancellation)
    }

    /// Emits one product while observing caller cancellation at every effectful boundary.
    pub fn emit_product_with_cancellation(
        &self,
        request: EmissionRequest,
        inputs: ProductEmissionInputs<'_>,
        cancellation: &CancellationToken,
    ) -> Result<EmissionOutcome, ProductEmissionError> {
        let span = self
            .state
            .fact_runtime
            .profile()
            .map(|profile| profile.start(crate::profile::ProfileOperation::Emission, None));

        let result = self.emit_product_with_cancellation_inner(request, inputs, cancellation);

        if let Some(span) = span {
            span.finish(match &result {
                Ok(outcome) => match outcome.status() {
                    EmissionStatus::Complete => crate::CompilationProfileOutcome::Completed,
                    EmissionStatus::Failed(_) => crate::CompilationProfileOutcome::Failed,
                    EmissionStatus::Cancelled => crate::CompilationProfileOutcome::Cancelled,
                },
                Err(error) if matches!(error.kind(), ProductEmissionErrorKind::Cancelled) => {
                    crate::CompilationProfileOutcome::Cancelled
                }
                Err(_) => crate::CompilationProfileOutcome::Failed,
            });
        }

        if let Ok(outcome) = &result
            && let Some(profile) = self.state.fact_runtime.profile()
        {
            let artifacts = outcome.artifacts().artifacts();

            let bytes = artifacts.iter().fold(0_u64, |total, artifact| {
                total.saturating_add(artifact.byte_len())
            });

            profile.record_metric(
                crate::profile::ProfileMetricKind::EmittedArtifacts,
                u64::try_from(artifacts.len()).unwrap_or(u64::MAX),
            );

            profile.record_metric(crate::profile::ProfileMetricKind::EmittedBytes, bytes);
        }

        result
    }

    fn emit_product_with_cancellation_inner(
        &self,
        request: EmissionRequest,
        inputs: ProductEmissionInputs<'_>,
        cancellation: &CancellationToken,
    ) -> Result<EmissionOutcome, ProductEmissionError> {
        let product = request.product().clone();
        let target = request.target().clone();

        self.validate_product_request(&request, inputs.target_outputs)?;

        cancellation
            .check()
            .map_err(|_| ProductEmissionError::cancelled())?;

        let ProductEmissionPreparation {
            package_interface,
            package_implementation,
            diagnostics: planning_diagnostics,
        } = self.prepare_product_emission(&request, cancellation)?;

        // The immutable plan owns the selected target and backend inputs past this operation input.
        let planner = EmissionPlanner::new(
            inputs.target_outputs.clone(),
            inputs
                .generation
                .codegen()
                .map(|codegen| codegen.backend.clone()),
            package_interface,
        );

        let plan = planner.plan(request).map_err(|error| {
            ProductEmissionError::new(
                ProductEmissionErrorKind::Planning(error),
                planning_diagnostics.clone(),
                &product,
                &target,
            )
        })?;

        let test_catalog = inputs
            .test_catalog
            .map(|catalog| test_catalog_contribution(&plan, catalog))
            .transpose()
            .map_err(|kind| {
                ProductEmissionError::new(kind, planning_diagnostics.clone(), &product, &target)
            })?;

        let units = match inputs.generation.native() {
            Some(native) => native.units().to_vec(),
            None => self
                .codegen_units_for_plan(
                    plan.backend_requests(),
                    plan.request().executable_host(),
                    cancellation,
                )
                .map_err(|(unit, error)| {
                    ProductEmissionError::new(
                        ProductEmissionErrorKind::Codegen(EmissionCodegenError::new(
                            EmissionCodegenErrorKind::Request {
                                unit,
                                error: Box::new(error),
                            },
                            DiagnosticBag::new(),
                        )),
                        planning_diagnostics.clone(),
                        &product,
                        &target,
                    )
                })?,
        };

        let codegen = self
            .product_emission_contributions(&plan, &units, inputs, cancellation)
            .map_err(|error| {
                let diagnostics = planning_diagnostics.merged(error.diagnostics());

                ProductEmissionError::new(
                    ProductEmissionErrorKind::Codegen(error),
                    diagnostics,
                    &product,
                    &target,
                )
            })?;

        let diagnostics = planning_diagnostics.merged(&codegen.diagnostics);

        if diagnostics.has_errors() {
            return Err(ProductEmissionError::new(
                // rust-style: allow(context-erasing-failure-conversion, reason = "the emission diagnostic bag retains the exact causes")
                ProductEmissionErrorKind::InvalidCompilation,
                diagnostics,
                &product,
                &target,
            ));
        }

        let outcome = self
            .publish_product(
                &plan,
                codegen.backend,
                package_implementation,
                test_catalog,
                inputs.linking,
                inputs.generation.native(),
                inputs.native_inspection,
                inputs.sink_resolver,
                inputs.publication_validation,
                cancellation,
            )
            .map_err(|kind| {
                crate::compilation::linking::product_emission_error(kind, &diagnostics, &plan)
            })?;

        Ok(outcome.with_prior_diagnostics(&diagnostics))
    }

    fn validate_product_request(
        &self,
        request: &EmissionRequest,
        target_outputs: &TargetOutputDescription,
    ) -> Result<(), ProductEmissionError> {
        if request.product_kind() != self.product_kind()
            || (request.product().package() != self.package_identity()
                && request.product_kind() != bray_symbols::ProductKind::Test)
        {
            return Err(ProductEmissionError::new(
                ProductEmissionErrorKind::ProductMismatch {
                    requested: request.product().clone(),
                    selected_package: self.package_identity().clone(),
                    selected_kind: self.product_kind(),
                },
                DiagnosticBag::new(),
                request.product(),
                request.target(),
            ));
        }

        let selected = self.selected_target().target().profile().identity();

        if request.target() != selected || target_outputs.profile().identity() != selected {
            // The boundary error owns both Arc-backed identities after the compilation borrow ends.
            return Err(ProductEmissionError::new(
                ProductEmissionErrorKind::TargetMismatch {
                    requested: request.target().clone(),
                    selected: selected.clone(),
                },
                DiagnosticBag::new(),
                request.product(),
                request.target(),
            ));
        }

        Ok(())
    }

    fn prepare_product_emission(
        &self,
        request: &EmissionRequest,
        cancellation: &CancellationToken,
    ) -> Result<ProductEmissionPreparation, ProductEmissionError> {
        let requires_interface = request.artifact(ArtifactKind::PackageInterface).is_some();

        let requires_implementation = request
            .artifact(ArtifactKind::PackageImplementation)
            .is_some();

        let inputs = self
            .state
            .fact_runtime
            .map_indexed(2, |index| match index {
                0 => ProductEmissionInput::PackageInterface(self.product_interface_artifacts(
                    requires_interface,
                    requires_implementation,
                    cancellation,
                )),
                1 => ProductEmissionInput::Diagnostics(
                    cancellation
                        .check()
                        .and_then(|()| self.check_diagnostics_with_cancellation(cancellation))
                        .cloned()
                        .map_err(product_query_error),
                ),
                _ => unreachable!("product planning input index must be in range"),
            })
            .map_err(product_query_error)
            .map_err(|kind| {
                ProductEmissionError::new(
                    kind,
                    DiagnosticBag::new(),
                    request.product(),
                    request.target(),
                )
            })?;

        let mut package_interface = None;
        let mut diagnostics = None;

        for input in inputs {
            match input {
                ProductEmissionInput::PackageInterface(artifact) => {
                    package_interface = Some(artifact);
                }
                ProductEmissionInput::Diagnostics(query_diagnostics) => {
                    diagnostics = Some(query_diagnostics);
                }
            }
        }

        let diagnostics = diagnostics
            .unwrap_or(Err(ProductEmissionErrorKind::Query(
                ProductQueryFailure::missing(
                    ProductQueryContext::Product(request.product_kind()),
                    ProductDataKind::CompilationDiagnostics,
                )
                .into(),
            )))
            .map_err(|kind| {
                ProductEmissionError::new(
                    kind,
                    DiagnosticBag::new(),
                    request.product(),
                    request.target(),
                )
            })?;

        let package_interface = package_interface
            .unwrap_or(Err(ProductEmissionErrorKind::Query(
                ProductQueryFailure::missing(
                    ProductQueryContext::Product(request.product_kind()),
                    ProductDataKind::PackageInterfaceContribution,
                )
                .into(),
            )))
            .map_err(|kind| {
                ProductEmissionError::new(
                    kind,
                    diagnostics.clone(),
                    request.product(),
                    request.target(),
                )
            })?;

        Ok(ProductEmissionPreparation {
            package_interface: package_interface.interface,
            package_implementation: package_interface.implementation,
            diagnostics,
        })
    }

    fn product_interface_artifacts(
        &self,
        interface_required: bool,
        implementation_required: bool,
        cancellation: &CancellationToken,
    ) -> Result<ProductInterfaceArtifacts, ProductEmissionErrorKind> {
        if !interface_required && !implementation_required {
            return Ok(ProductInterfaceArtifacts {
                interface: None,
                implementation: None,
            });
        }

        cancellation
            .check()
            .map_err(|_| ProductEmissionErrorKind::Cancelled)?;

        let Some(bundle) = self.package_interface_export_bundle() else {
            return Err(ProductEmissionErrorKind::PackageInterfaceUnavailable);
        };

        // The error crosses the cached input borrow and therefore retains its Arc-backed identity.
        let bundle = bundle
            .as_ref()
            .map_err(|error| ProductEmissionErrorKind::PackageInterface(error.clone()))?;

        let artifact = encode_package_interface(bundle)
            .map_err(ProductEmissionErrorKind::PackageInterfaceEncoding)?;

        if let Some(profile) = self.state.fact_runtime.profile() {
            profile.record_metric(
                crate::profile::ProfileMetricKind::InterfaceSections,
                artifact.section_count(),
            );

            profile.record_metric(
                crate::profile::ProfileMetricKind::InterfaceBytes,
                artifact.byte_len(),
            );
        }

        let implementation = implementation_required
            .then(|| (artifact.clone(), Arc::clone(bundle)));

        cancellation
            .check()
            .map_err(|_| ProductEmissionErrorKind::Cancelled)?;

        Ok(ProductInterfaceArtifacts {
            interface: interface_required.then_some(artifact),
            implementation,
        })
    }

    fn product_emission_contributions(
        &self,
        plan: &EmissionPlan,
        units: &[CodegenUnit],
        inputs: ProductEmissionInputs<'_>,
        cancellation: &CancellationToken,
    ) -> Result<ProductEmissionContributions, EmissionCodegenError> {
        if plan.backend_requests().is_empty() {
            return Ok(ProductEmissionContributions {
                backend: None,
                diagnostics: DiagnosticBag::new(),
            });
        }

        // The planner receives its backend only from this same codegen configuration.
        let codegen = inputs
            .generation
            .codegen()
            .unwrap_or_else(|| panic!("backend emission plans must retain codegen configuration"));

        let result = self.emission_backend_contributions_with_cancellation(
            plan,
            units,
            codegen.mappings,
            codegen.target,
            codegen.options,
            cancellation,
        )?;

        let (contributions, diagnostics) = result.into_parts();

        if let Some(profile) = self.state.fact_runtime.profile() {
            profile.record_optimization_inputs(plan.backend_requests(), &contributions);
        }

        Ok(ProductEmissionContributions {
            backend: Some(contributions),
            diagnostics,
        })
    }

    fn publish_product(
        &self,
        plan: &EmissionPlan,
        backend: Option<BackendContributionSet>,
        package_implementation: Option<(bray_package_interface::InterfaceArtifact, Arc<bray_package_interface::PackageInterfaceExportBundle>)>,
        test_catalog: Option<ArtifactContribution>,
        linking: Option<ProductLinkingInputs<'_>>,
        native: Option<&NativeProductPlan>,
        inspection: Option<NativeInspectionInputs<'_>>,
        resolver: Option<&dyn OutputSinkResolver>,
        validation: Option<&dyn PublicationValidator>,
        cancellation: &CancellationToken,
    ) -> Result<EmissionOutcome, ProductEmissionErrorKind> {
        let requires_linking = plan
            .artifacts()
            .iter()
            .any(|artifact| matches!(artifact.producer(), ArtifactProducer::Linker(_)));

        match (requires_linking, linking) {
            (false, None) => {
                let package_implementation = package_implementation
                    .map(|(interface, bundle)| {
                        bray_package_interface::PackageImplementationArtifact::try_from_export_bundle(
                            &interface,
                            &bundle,
                            bray_package_interface::InterfaceValidationLimits::default(),
                        )
                        .map_err(ProductEmissionErrorKind::PackageImplementation)
                        .and_then(|artifact| package_implementation_contribution(plan, artifact))
                    })
                    .transpose()?;

                let contributions = backend
                    .iter()
                    .flat_map(|contributions| contributions.published(plan))
                    .cloned()
                    .chain(package_implementation)
                    .chain(test_catalog);

                let publisher = publisher(cancellation, resolver, validation);

                Ok(publisher.publish(plan, contributions))
            }
            (false, Some(_)) => Err(ProductEmissionErrorKind::UnexpectedLinker),
            (true, None) => Err(ProductEmissionErrorKind::MissingLinker),
            (true, Some(linking)) => {
                let staged = backend
                    .iter()
                    .flat_map(|contributions| contributions.staged(plan))
                    .cloned();

                let staging = crate::profile::profile_operation(
                    self.state.fact_runtime.profile(),
                    crate::profile::ProfileOperation::LinkInputStaging,
                    || LinkStaging::prepare(plan, staged, cancellation),
                    crate::profile::result_outcome,
                )
                .map_err(product_staging_error)?;

                let package_implementation = package_implementation
                    .map(|(interface, bundle)| {
                        let artifact = match (native, inspection) {
                            (Some(native), Some(inspection)) => crate::profile::profile_operation(
                                self.state.fact_runtime.profile(),
                                crate::profile::ProfileOperation::NativeUnitInspection,
                                || package_native_implementation(
                                    self, plan, &staging, native, inspection, &interface, &bundle,
                                ),
                                crate::profile::result_outcome,
                            )?,
                            (Some(_), None) => return Err(ProductEmissionErrorKind::MissingNativeInspector),
                            (None, _) => bray_package_interface::PackageImplementationArtifact::try_from_export_bundle(
                                &interface,
                                &bundle,
                                bray_package_interface::InterfaceValidationLimits::default(),
                            ).map_err(ProductEmissionErrorKind::PackageImplementation)?,
                        };

                        package_implementation_contribution(plan, artifact)
                    })
                    .transpose()?;

                let published = backend
                    .iter()
                    .flat_map(|contributions| contributions.published(plan))
                    .cloned()
                    .chain(package_implementation)
                    .chain(test_catalog);

                let link_plan = construct_link_plan(
                    plan,
                    staging.inputs().iter().cloned(),
                    staging.outputs().iter().cloned(),
                    linking.inputs,
                    linking.linker,
                )
                .map_err(ProductEmissionErrorKind::LinkPlan)?;

                crate::profile::profile_operation(
                    self.state.fact_runtime.profile(),
                    crate::profile::ProfileOperation::EmissionLinking,
                    || {
                        self.emit_linked_product_with_publication_validation(
                            linking.linker,
                            plan,
                            &link_plan,
                            published,
                            resolver,
                            validation,
                            cancellation,
                        )
                    },
                    crate::profile::result_outcome,
                )
                .map_err(product_linked_emission_error)
            }
        }
    }
}

struct ProductEmissionPreparation {
    package_interface: Option<bray_package_interface::InterfaceArtifact>,
    package_implementation: Option<(bray_package_interface::InterfaceArtifact, Arc<bray_package_interface::PackageInterfaceExportBundle>)>,
    diagnostics: DiagnosticBag,
}

struct ProductInterfaceArtifacts {
    interface: Option<bray_package_interface::InterfaceArtifact>,
    implementation: Option<(bray_package_interface::InterfaceArtifact, Arc<bray_package_interface::PackageInterfaceExportBundle>)>,
}

enum ProductEmissionInput {
    PackageInterface(Result<ProductInterfaceArtifacts, ProductEmissionErrorKind>),
    Diagnostics(Result<DiagnosticBag, ProductEmissionErrorKind>),
}

struct ProductEmissionContributions {
    backend: Option<BackendContributionSet>,
    diagnostics: DiagnosticBag,
}

fn product_staging_error(error: LinkStagingError) -> ProductEmissionErrorKind {
    if error == LinkStagingError::Cancelled {
        ProductEmissionErrorKind::Cancelled
    } else {
        ProductEmissionErrorKind::Staging(error)
    }
}

fn product_query_error(error: FactQueryError) -> ProductEmissionErrorKind {
    if error == FactQueryError::Cancelled {
        ProductEmissionErrorKind::Cancelled
    } else {
        ProductEmissionErrorKind::Query(error)
    }
}

fn product_linked_emission_error(
    error: crate::LinkedProductEmissionError,
) -> ProductEmissionErrorKind {
    match error {
        crate::LinkedProductEmissionError::Query(error) => product_query_error(error),
    }
}

#[cfg(test)]
mod tests {
    use bray_codegen::CodegenPartitionPolicy;
    use bray_codegen::test_support::codegen_partition_compatibility;
    use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
    use bray_emitter::{
        ArtifactKind, ArtifactRequirement, BackendEmissionPolicy, EmissionBackend, EmissionPlanner,
        EmissionRequest, EmissionStatus, LinkStagingError, ReplacementPolicy, RequestedArtifact,
        RequestedArtifactDestination,
    };
    use bray_ir::{MirUnit, MirUnitId};
    use bray_lowering::{ExecutableHostLoweringInput, lower_executable_host};
    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
        PackageInterfaceIdentity,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
    use bray_target::test_support::test_target_profile;
    use bray_target::{TargetOutputDescription, TargetOutputKind, TargetOutputName};
    use bray_testing::{
        TemporaryFile, assert_goal_state_diagnostic_kind, test_bound_unit,
        test_executable_host_contract_for, test_mir_target,
    };

    use super::{ProductEmissionError, ProductEmissionErrorKind, ProductEmissionInputs};
    use crate::test_support::{compilation_with_product, package_version};
    use crate::{
        CancellationToken, Compilation, CompilationOptions, CompilationRequest,
        PackageInterfaceExportRequest, SelectedTarget, WorkerBudget,
    };

    #[test]
    fn package_interface_emission_reuses_pure_inputs_across_publications() {
        let compilation = compilation();
        let target_outputs = target_outputs();

        let destination = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test output directory must exist: {error}"));

        let inputs = ProductEmissionInputs::new(&target_outputs);

        let first = compilation
            .emit_product(emission_request(destination.path()), inputs)
            .unwrap_or_else(|error| panic!("first interface emission must complete: {error:?}"));

        let first_path = first
            .generation()
            .and_then(|generation| generation.artifact_path(first.artifacts().artifacts()[0].id()))
            .unwrap_or_else(|| panic!("first interface generation path must resolve"));

        let first_bytes = std::fs::read(first_path)
            .unwrap_or_else(|error| panic!("first interface output must be readable: {error:?}"));

        let second = compilation
            .emit_product(emission_request(destination.path()), inputs)
            .unwrap_or_else(|error| panic!("repeated interface emission must complete: {error:?}"));

        let second_path = second
            .generation()
            .and_then(|generation| generation.artifact_path(second.artifacts().artifacts()[0].id()))
            .unwrap_or_else(|| panic!("second interface generation path must resolve"));

        let second_bytes = std::fs::read(second_path)
            .unwrap_or_else(|error| panic!("second interface output must be readable: {error:?}"));

        assert!(matches!(first.status(), EmissionStatus::Complete));
        assert!(matches!(second.status(), EmissionStatus::Complete));
        assert_eq!(first_bytes, second_bytes);

        assert_eq!(
            first.artifacts().artifacts()[0].digest(),
            second.artifacts().artifacts()[0].digest(),
        );

        let first_bundle = compilation
            .package_interface_export_bundle()
            .and_then(|result| result.as_ref().ok())
            .unwrap_or_else(|| panic!("interface bundle input must remain published"));

        let second_bundle = compilation
            .package_interface_export_bundle()
            .and_then(|result| result.as_ref().ok())
            .unwrap_or_else(|| panic!("interface bundle input must remain reusable"));

        assert!(std::sync::Arc::ptr_eq(first_bundle, second_bundle));
    }

    #[test]
    fn cancellation_before_planning_preserves_existing_output() {
        let compilation = compilation();
        let target_outputs = target_outputs();
        let destination = TemporaryFile::write("library.brayi", b"unchanged");
        let cancellation = CancellationToken::new();

        cancellation.cancel();

        let inputs = ProductEmissionInputs::new(&target_outputs);

        let result = compilation.emit_product_with_cancellation(
            emission_request(destination.path()),
            inputs,
            &cancellation,
        );

        assert!(matches!(
            result,
            Err(error) if error.kind() == &ProductEmissionErrorKind::Cancelled
        ));

        assert_eq!(
            std::fs::read(destination.path())
                .unwrap_or_else(|error| panic!("existing output must be readable: {error:?}")),
            b"unchanged",
        );
    }

    #[test]
    fn test_product_emission_can_use_a_public_package_identity() {
        let compilation = compilation_with_product("module tests;\n", ProductKind::Test);

        let public_package = PackageIdentity::try_new("public.package")
            .unwrap_or_else(|| panic!("public test package identity must be valid"));

        let product = ProductIdentity::try_new(public_package, "tests")
            .unwrap_or_else(|| panic!("public test product identity must be valid"));

        let selected = compilation.selected_target().target().profile().clone();

        let host = test_executable_host_contract_for(product.clone(), selected.identity().clone());

        let request = EmissionRequest::try_new(
            product,
            ProductKind::Test,
            Some(host),
            selected.identity().clone(),
            RequestedArtifactDestination::FilesystemFile("tests".into()),
            [RequestedArtifact::new(
                ArtifactKind::Executable,
                ArtifactRequirement::Required,
            )],
            ReplacementPolicy::RequireAbsent,
        )
        .unwrap_or_else(|error| panic!("test emission request must be valid: {error:?}"));

        let name = TargetOutputName::try_new(TargetOutputKind::Executable, "", "")
            .unwrap_or_else(|error| panic!("test executable name must be valid: {error:?}"));

        let outputs = TargetOutputDescription::try_new(selected, [name])
            .unwrap_or_else(|error| panic!("test target outputs must be valid: {error:?}"));

        assert_eq!(
            compilation.validate_product_request(&request, &outputs),
            Ok(())
        );
    }

    #[test]
    fn staging_io_failures_preserve_artifact_operation_and_io_cause() {
        let product = ProductIdentity::try_new(package_identity(), "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let host = test_executable_host_contract_for(
            product.clone(),
            test_mir_target().identity().clone(),
        );

        let host_mir = executable_host_mir(host.clone());

        let unit = bray_codegen::CodegenUnit::try_new(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            codegen_partition_compatibility(),
            [host_mir],
            bray_testing::test_mir_content_identity,
        )
        .unwrap_or_else(|error| panic!("test codegen unit must be valid: {error:?}"));

        let plan = async_plan(product.clone(), host, &unit);

        let artifact = plan
            .staged_artifacts()
            .next()
            .map(|planned| planned.id().clone())
            .unwrap_or_else(|| panic!("linked test plan must stage one backend artifact"));

        let target = plan.request().target();

        let create = ProductEmissionError::new(
            ProductEmissionErrorKind::Staging(LinkStagingError::Create {
                artifact: artifact.clone(),
                kind: std::io::ErrorKind::PermissionDenied,
            }),
            DiagnosticBag::new(),
            &product,
            target,
        );

        assert_goal_state_diagnostic_kind(
            create.diagnostics(),
            DiagnosticKind::EmissionArtifactIoFailed,
        );

        let read = ProductEmissionError::new(
            ProductEmissionErrorKind::Staging(LinkStagingError::Read {
                artifact: artifact.clone(),
                kind: std::io::ErrorKind::NotFound,
            }),
            DiagnosticBag::new(),
            &product,
            target,
        );

        assert_goal_state_diagnostic_kind(
            read.diagnostics(),
            DiagnosticKind::EmissionArtifactIoFailed,
        );

        let write = ProductEmissionError::new(
            ProductEmissionErrorKind::Staging(LinkStagingError::Write {
                artifact: artifact.clone(),
                kind: std::io::ErrorKind::WriteZero,
            }),
            DiagnosticBag::new(),
            &product,
            target,
        );

        assert_goal_state_diagnostic_kind(
            write.diagnostics(),
            DiagnosticKind::EmissionArtifactIoFailed,
        );

        let flush = ProductEmissionError::new(
            ProductEmissionErrorKind::Staging(LinkStagingError::Flush {
                artifact,
                kind: std::io::ErrorKind::BrokenPipe,
            }),
            DiagnosticBag::new(),
            &product,
            target,
        );

        assert_goal_state_diagnostic_kind(
            flush.diagnostics(),
            DiagnosticKind::EmissionArtifactIoFailed,
        );
    }

    fn compilation() -> Compilation {
        let package = package_identity();

        let product = InterfaceProductIdentity::try_new("library")
            .unwrap_or_else(|| panic!("test interface product identity must be valid"));

        let identity = PackageInterfaceIdentity::try_new(
            package.clone(),
            package_version(),
            product,
            InterfaceProductKind::Library,
            "public-v1",
        )
        .unwrap_or_else(|| panic!("test package interface identity must be valid"));

        let export =
            PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

        let source = SourceInput::virtual_text(
            SourceIdentity::new(0),
            "library.bray",
            SourceVersion::new(0),
            "module app;",
        );

        let options = CompilationOptions::new(
            WorkerBudget::default(),
            ProductKind::Library,
            SelectedTarget::default(),
        );

        let request = CompilationRequest::with_options(package, vec![source], options)
            .with_package_interface_export(export);

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn emission_request(destination: &std::path::Path) -> EmissionRequest {
        EmissionRequest::try_new(
            ProductIdentity::try_new(package_identity(), "library")
                .unwrap_or_else(|| panic!("test product identity must be valid")),
            ProductKind::Library,
            None,
            SelectedTarget::default().profile().identity().clone(),
            RequestedArtifactDestination::FilesystemDirectory(destination.to_path_buf().into()),
            [RequestedArtifact::new(
                ArtifactKind::PackageInterface,
                ArtifactRequirement::Required,
            )],
            ReplacementPolicy::ReplaceExisting,
        )
        .unwrap_or_else(|error| panic!("test emission request must be valid: {error:?}"))
    }

    fn target_outputs() -> TargetOutputDescription {
        let name = TargetOutputName::try_new(TargetOutputKind::PackageInterface, "", ".brayi")
            .unwrap_or_else(|error| panic!("test output name must be valid: {error:?}"));

        TargetOutputDescription::try_new(SelectedTarget::default().profile().clone(), [name])
            .unwrap_or_else(|error| panic!("test target outputs must be valid: {error:?}"))
    }

    fn package_identity() -> PackageIdentity {
        PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"))
    }

    fn executable_host_mir(host: bray_runtime_interface::ExecutableHostContract) -> MirUnit {
        let root = test_bound_unit(8).key().clone();

        lower_executable_host(ExecutableHostLoweringInput::new(
            MirUnitId::new(90),
            [root],
            host,
            test_mir_target(),
        ))
        .unwrap_or_else(|error| panic!("test executable host must lower: {error:?}"))
    }

    fn async_plan(
        product: ProductIdentity,
        host: bray_runtime_interface::ExecutableHostContract,
        unit: &bray_codegen::CodegenUnit,
    ) -> bray_emitter::EmissionPlan {
        let backend = EmissionBackend::new(
            test_backend_identity(),
            test_backend_capabilities(),
            [unit.key().clone()],
            BackendEmissionPolicy::new(
                bray_codegen::DebugInformationMode::None,
                bray_codegen::DebugInformationOutputMode::Omit,
                Some(bray_codegen::LinkableArtifactKind::RelocatableObject),
                bray_codegen::BackendSerializationOptions::new(
                    bray_codegen::AssemblySyntaxKind::TargetDefault,
                ),
            ),
        );

        let name = TargetOutputName::try_new(TargetOutputKind::Executable, "", "")
            .unwrap_or_else(|error| panic!("test executable name must be valid: {error:?}"));

        let target = TargetOutputDescription::try_new(test_target_profile(), [name])
            .unwrap_or_else(|error| panic!("test target outputs must be valid: {error:?}"));

        let request = EmissionRequest::try_new(
            product,
            ProductKind::Executable,
            Some(host),
            test_mir_target().identity().clone(),
            RequestedArtifactDestination::FilesystemDirectory("application".into()),
            [RequestedArtifact::new(
                ArtifactKind::Executable,
                ArtifactRequirement::Required,
            )],
            ReplacementPolicy::RequireAbsent,
        )
        .unwrap_or_else(|error| panic!("test emission request must be valid: {error:?}"));

        EmissionPlanner::new(target, Some(backend), None)
            .plan(request)
            .unwrap_or_else(|error| panic!("test emission plan must be valid: {error:?}"))
    }

    fn test_backend_identity() -> bray_codegen::BackendIdentity {
        bray_codegen::BackendIdentity::try_new("test", "bray-1", "toolchain-1")
            .unwrap_or_else(|| panic!("test backend identity must be valid"))
    }

    fn test_backend_capabilities() -> bray_codegen::BackendCapabilities {
        bray_codegen::test_support::codegen_backend_capabilities()
    }
}
