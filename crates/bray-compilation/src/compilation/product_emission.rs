use bray_codegen::{CodegenMappings, CodegenOptions, CodegenTarget, CodegenUnit};
use bray_diagnostics::DiagnosticBag;
use bray_emitter::{
    ArtifactKind, ArtifactProducer, ArtifactPublisher, BackendContributionSet, EmissionBackend,
    EmissionOutcome, EmissionPlan, EmissionPlanner, EmissionPlanningError, EmissionRequest,
    LinkPlanConstructionError, LinkStaging, LinkStagingError, OutputSinkResolver, ProductLinkFacts,
    construct_link_plan,
};
use bray_linker::Linker;
use bray_package_interface::{InterfaceValidationError, encode_package_interface};
use bray_runtime_interface::{ProtectedAsyncFrameId, RootExecution};
use bray_target::{TargetIdentity, TargetOutputDescription};

use super::{
    Compilation, EmissionCodegenError, EmissionCodegenErrorKind, NativeProductFacts,
    PackageInterfaceExportError,
};
use crate::fact::{CancellationToken, FactQueryError};

/// Borrowed immutable producer and host inputs for one product emission operation.
#[derive(Clone, Copy)]
pub struct ProductEmissionInputs<'operation> {
    target_outputs: &'operation TargetOutputDescription,
    generation: ProductGenerationInputs<'operation>,
    sink_resolver: Option<&'operation dyn OutputSinkResolver>,
}

impl<'operation> ProductEmissionInputs<'operation> {
    /// Creates product inputs from host-selected target output configuration.
    pub const fn new(target_outputs: &'operation TargetOutputDescription) -> Self {
        Self {
            target_outputs,
            generation: ProductGenerationInputs::None,
            sink_resolver: None,
        }
    }

    /// Supplies selected backend, validated mappings, target, and generation policy.
    pub const fn with_codegen(
        mut self,
        backend: &'operation EmissionBackend,
        mappings: &'operation [CodegenMappings],
        target: &'operation CodegenTarget,
        options: &'operation CodegenOptions,
    ) -> Self {
        let linking = match self.generation {
            ProductGenerationInputs::Custom { linking, .. } => linking,
            ProductGenerationInputs::None | ProductGenerationInputs::Native { .. } => None,
        };

        self.generation = ProductGenerationInputs::Custom {
            codegen: Some(ProductCodegenInputs {
                backend,
                mappings,
                target,
                options,
            }),
            linking,
        };

        self
    }

    /// Supplies compilation-owned native product facts and the linker invocation boundary.
    pub const fn with_native_product(
        mut self,
        facts: &'operation NativeProductFacts,
        linker: &'operation Linker,
    ) -> Self {
        self.generation = ProductGenerationInputs::Native { facts, linker };

        self
    }

    /// Supplies compilation-owned native code generation without final linking.
    pub fn with_native_codegen(
        mut self,
        facts: &'operation NativeProductFacts,
    ) -> Self {
        self.generation = ProductGenerationInputs::Custom {
            codegen: Some(ProductCodegenInputs {
                backend: facts.backend(),
                mappings: facts.mappings(),
                target: facts.target(),
                options: facts.options(),
            }),
            linking: None,
        };

        self
    }

    /// Supplies selected native link facts and the linker invocation boundary.
    pub const fn with_linking(
        mut self,
        linker: &'operation Linker,
        facts: &'operation ProductLinkFacts,
    ) -> Self {
        let codegen = match self.generation {
            ProductGenerationInputs::Custom { codegen, .. } => codegen,
            ProductGenerationInputs::None | ProductGenerationInputs::Native { .. } => None,
        };

        self.generation = ProductGenerationInputs::Custom {
            codegen,
            linking: Some(ProductLinkingInputs { linker, facts }),
        };

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
}

#[derive(Clone, Copy)]
enum ProductGenerationInputs<'operation> {
    None,
    Custom {
        codegen: Option<ProductCodegenInputs<'operation>>,
        linking: Option<ProductLinkingInputs<'operation>>,
    },
    Native {
        facts: &'operation NativeProductFacts,
        linker: &'operation Linker,
    },
}

impl<'operation> ProductGenerationInputs<'operation> {
    fn codegen(self) -> Option<ProductCodegenInputs<'operation>> {
        match self {
            Self::None => None,
            Self::Custom { codegen, .. } => codegen,
            Self::Native { facts, .. } => Some(ProductCodegenInputs {
                backend: facts.backend(),
                mappings: facts.mappings(),
                target: facts.target(),
                options: facts.options(),
            }),
        }
    }

    const fn linking(self) -> Option<ProductLinkingInputs<'operation>> {
        match self {
            Self::None => None,
            Self::Custom { linking, .. } => linking,
            Self::Native { facts, linker } => Some(ProductLinkingInputs {
                linker,
                facts: facts.link(),
            }),
        }
    }

    const fn native(self) -> Option<&'operation NativeProductFacts> {
        match self {
            Self::Native { facts, .. } => Some(facts),
            Self::None | Self::Custom { .. } => None,
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
    facts: &'operation ProductLinkFacts,
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
        self.validate_product_request(&request, inputs.target_outputs)?;

        cancellation
            .check()
            .map_err(|_| ProductEmissionError::cancelled())?;

        let ProductEmissionPlanningFacts {
            package_interface,
            diagnostics: planning_diagnostics,
        } = self.product_emission_planning_facts(&request, cancellation)?;

        // The immutable plan owns the selected target and backend facts past this operation input.
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
            )
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
                    )
                })?,
        };

        validate_executable_units(&plan, &units)
            .map_err(|kind| ProductEmissionError::new(kind, planning_diagnostics.clone()))?;

        let codegen = self
            .product_emission_contributions(&plan, &units, inputs, cancellation)
            .map_err(|error| {
                let diagnostics = planning_diagnostics.merged(error.diagnostics());

                ProductEmissionError::new(ProductEmissionErrorKind::Codegen(error), diagnostics)
            })?;

        let diagnostics = planning_diagnostics.merged(&codegen.diagnostics);

        if diagnostics.has_errors() {
            return Err(ProductEmissionError::new(
                ProductEmissionErrorKind::InvalidCompilation,
                diagnostics,
            ));
        }

        let outcome = self
            .publish_product(
                &plan,
                codegen.backend,
                inputs.generation.linking(),
                inputs.sink_resolver,
                cancellation,
            )
            .map_err(|kind| ProductEmissionError::new(kind, diagnostics.clone()))?;

        Ok(outcome.with_prior_diagnostics(&diagnostics))
    }

    fn validate_product_request(
        &self,
        request: &EmissionRequest,
        target_outputs: &TargetOutputDescription,
    ) -> Result<(), ProductEmissionError> {
        if request.product().package() != self.package_identity()
            || request.product_kind() != self.options().product_kind()
        {
            return Err(ProductEmissionError::new(
                ProductEmissionErrorKind::ProductMismatch,
                DiagnosticBag::new(),
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
            ));
        }

        Ok(())
    }

    fn product_emission_planning_facts(
        &self,
        request: &EmissionRequest,
        cancellation: &CancellationToken,
    ) -> Result<ProductEmissionPlanningFacts, ProductEmissionError> {
        let requires_interface = request.artifact(ArtifactKind::PackageInterface).is_some();

        let facts = self
            .state
            .fact_runtime
            .map_indexed(2, |index| match index {
                0 => ProductEmissionPlanningFact::PackageInterface(
                    self.product_interface_artifact(requires_interface, cancellation),
                ),
                1 => ProductEmissionPlanningFact::Diagnostics(
                    cancellation
                        .check()
                        .and_then(|()| self.check_diagnostics_with_cancellation(cancellation))
                        .cloned()
                        .map_err(product_query_error),
                ),
                _ => unreachable!("product planning fact index must be in range"),
            })
            .map_err(product_query_error)
            .map_err(|kind| ProductEmissionError::new(kind, DiagnosticBag::new()))?;

        let mut package_interface = None;
        let mut diagnostics = None;

        for fact in facts {
            match fact {
                ProductEmissionPlanningFact::PackageInterface(artifact) => {
                    package_interface = Some(artifact);
                }
                ProductEmissionPlanningFact::Diagnostics(fact_diagnostics) => {
                    diagnostics = Some(fact_diagnostics);
                }
            }
        }

        let diagnostics = diagnostics
            .unwrap_or(Err(ProductEmissionErrorKind::Query(
                FactQueryError::InfrastructureFailure,
            )))
            .map_err(|kind| ProductEmissionError::new(kind, DiagnosticBag::new()))?;

        let package_interface = package_interface
            .unwrap_or(Err(ProductEmissionErrorKind::Query(
                FactQueryError::InfrastructureFailure,
            )))
            .map_err(|kind| ProductEmissionError::new(kind, diagnostics.clone()))?;

        Ok(ProductEmissionPlanningFacts {
            package_interface,
            diagnostics,
        })
    }

    fn product_interface_artifact(
        &self,
        required: bool,
        cancellation: &CancellationToken,
    ) -> Result<Option<bray_package_interface::InterfaceArtifact>, ProductEmissionErrorKind> {
        if !required {
            return Ok(None);
        }

        cancellation
            .check()
            .map_err(|_| ProductEmissionErrorKind::Cancelled)?;

        let Some(bundle) = self.package_interface_export_bundle() else {
            return Err(ProductEmissionErrorKind::PackageInterfaceUnavailable);
        };

        // The error crosses the cached fact borrow and therefore retains its Arc-backed identity.
        let bundle = bundle
            .as_ref()
            .map_err(|error| ProductEmissionErrorKind::PackageInterface(error.clone()))?;

        let artifact = encode_package_interface(bundle)
            .map_err(ProductEmissionErrorKind::PackageInterfaceEncoding)?;

        cancellation
            .check()
            .map_err(|_| ProductEmissionErrorKind::Cancelled)?;

        Ok(Some(artifact))
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

        Ok(ProductEmissionContributions {
            backend: Some(contributions),
            diagnostics,
        })
    }

    fn publish_product(
        &self,
        plan: &EmissionPlan,
        backend: Option<BackendContributionSet>,
        linking: Option<ProductLinkingInputs<'_>>,
        resolver: Option<&dyn OutputSinkResolver>,
        cancellation: &CancellationToken,
    ) -> Result<EmissionOutcome, ProductEmissionErrorKind> {
        let requires_linking = plan
            .artifacts()
            .iter()
            .any(|artifact| matches!(artifact.producer(), ArtifactProducer::Linker(_)));

        match (requires_linking, linking) {
            (false, None) => {
                let contributions = backend
                    .iter()
                    .flat_map(|contributions| contributions.published(plan))
                    .cloned();

                let publisher = publisher(cancellation, resolver);

                Ok(publisher.publish(plan, contributions))
            }
            (false, Some(_)) => Err(ProductEmissionErrorKind::UnexpectedLinker),
            (true, None) => Err(ProductEmissionErrorKind::MissingLinker),
            (true, Some(linking)) => {
                let published = backend
                    .iter()
                    .flat_map(|contributions| contributions.published(plan))
                    .cloned();

                let staged = backend
                    .iter()
                    .flat_map(|contributions| contributions.staged(plan))
                    .cloned();

                let staging = LinkStaging::prepare(plan, staged, cancellation)
                    .map_err(product_staging_error)?;

                let link_plan = construct_link_plan(
                    plan,
                    staging.inputs().iter().cloned(),
                    staging.outputs().iter().cloned(),
                    linking.facts,
                )
                .map_err(ProductEmissionErrorKind::LinkPlan)?;

                self.emit_linked_product_with_cancellation(
                    linking.linker,
                    plan,
                    &link_plan,
                    published,
                    resolver,
                    cancellation,
                )
                .map_err(product_query_error)
            }
        }
    }
}

struct ProductEmissionPlanningFacts {
    package_interface: Option<bray_package_interface::InterfaceArtifact>,
    diagnostics: DiagnosticBag,
}

enum ProductEmissionPlanningFact {
    PackageInterface(
        Result<Option<bray_package_interface::InterfaceArtifact>, ProductEmissionErrorKind>,
    ),
    Diagnostics(Result<DiagnosticBag, ProductEmissionErrorKind>),
}

struct ProductEmissionContributions {
    backend: Option<BackendContributionSet>,
    diagnostics: DiagnosticBag,
}

/// Failure before a product reached emitter-owned publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductEmissionError {
    kind: ProductEmissionErrorKind,
    diagnostics: DiagnosticBag,
}

impl ProductEmissionError {
    fn new(kind: ProductEmissionErrorKind, diagnostics: DiagnosticBag) -> Self {
        Self { kind, diagnostics }
    }

    fn cancelled() -> Self {
        Self::new(ProductEmissionErrorKind::Cancelled, DiagnosticBag::new())
    }

    /// Returns the exact failed product-emission boundary.
    pub const fn kind(&self) -> &ProductEmissionErrorKind {
        &self.kind
    }

    /// Returns deterministic diagnostics completed before the failure.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }
}

/// Structured reason product emission could not reach publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductEmissionErrorKind {
    /// Cancellation was observed before publication completed.
    Cancelled,
    /// The requested package product differs from the loaded compilation.
    ProductMismatch,
    /// The request or output facts differ from the compilation target.
    TargetMismatch {
        /// Target named by the emission request.
        requested: TargetIdentity,
        /// Target selected by the compilation.
        selected: TargetIdentity,
    },
    /// A package-interface artifact was requested without a configured export fact.
    PackageInterfaceUnavailable,
    /// The configured package-interface fact could not be completed.
    PackageInterface(PackageInterfaceExportError),
    /// The completed package-interface fact could not be encoded.
    PackageInterfaceEncoding(InterfaceValidationError),
    /// Immutable emitter planning rejected the selected request and producer facts.
    Planning(EmissionPlanningError),
    /// An executable plan does not request its generated host-stub codegen unit.
    MissingExecutableHost,
    /// An async executable plan does not request its root protected-frame descriptor.
    MissingRootFrame(ProtectedAsyncFrameId),
    /// Compilation diagnostics prevent complete product publication.
    InvalidCompilation,
    /// Planned lazy code generation could not produce complete contributions.
    Codegen(EmissionCodegenError),
    /// A linked plan has no linker and resolved product link facts.
    MissingLinker,
    /// Link inputs were supplied for a plan without a linked product.
    UnexpectedLinker,
    /// Emitter-owned native input or output staging failed.
    Staging(LinkStagingError),
    /// Resolved product, runtime, staging, and target facts could not form a link plan.
    LinkPlan(LinkPlanConstructionError),
    /// Lazy fact or bounded operation scheduling failed.
    Query(FactQueryError),
}

fn validate_executable_units(
    plan: &EmissionPlan,
    units: &[CodegenUnit],
) -> Result<(), ProductEmissionErrorKind> {
    let Some(host) = plan.request().executable_host() else {
        return Ok(());
    };

    let planned_units = units
        .iter()
        .filter(|unit| plan.backend_request(unit.key()).is_some());

    let mut has_host = false;
    let mut has_root_frame = !matches!(host.root(), RootExecution::Asynchronous { .. });

    for unit in planned_units {
        if let RootExecution::Asynchronous { frame } = host.root()
            && unit
                .instances()
                .iter()
                .any(|instance| instance.protected_frame_identity() == Some(frame))
        {
            has_root_frame = true;
        }

        for mir in unit.mir_units() {
            if matches!(
                mir.kind(),
                bray_ir::MirUnitKind::ExecutableHost(candidate) if candidate == host
            ) {
                has_host = true;
            }
        }
    }

    if !has_host {
        return Err(ProductEmissionErrorKind::MissingExecutableHost);
    }

    if !has_root_frame && let RootExecution::Asynchronous { frame } = host.root() {
        return Err(ProductEmissionErrorKind::MissingRootFrame(frame));
    }

    Ok(())
}

fn publisher<'operation>(
    cancellation: &'operation CancellationToken,
    resolver: Option<&'operation dyn OutputSinkResolver>,
) -> ArtifactPublisher<'operation> {
    match resolver {
        Some(resolver) => ArtifactPublisher::with_sink_resolver(cancellation, resolver),
        None => ArtifactPublisher::new(cancellation),
    }
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

#[cfg(test)]
mod tests {
    use bray_emitter::{
        ArtifactKind, ArtifactRequirement, BackendEmissionPolicy, EmissionBackend, EmissionPlanner,
        EmissionRequest, EmissionStatus, ReplacementPolicy, RequestedArtifact,
        RequestedArtifactDestination,
    };
    use bray_ir::{
        MirBlockKind, MirFrameDescriptor, MirFrameStateFacts, MirFrameStateId, MirSourceAnchor,
        MirTerminatorKind, MirUnit, MirUnitBuilder, MirUnitId, MirUnitKind,
    };
    use bray_lowering::{ExecutableHostLoweringInput, lower_executable_host};
    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
        PackageInterfaceIdentity,
    };
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameAbiVersions, RootExecution, RuntimeAbiVersion,
        RuntimeArtifactId,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
    use bray_target::test_support::test_target_profile;
    use bray_target::{TargetOutputDescription, TargetOutputKind, TargetOutputName};
    use bray_testing::{
        TemporaryFile, test_async_executable_host_contract_for_frame, test_bound_unit,
        test_mir_target, test_mir_type,
    };

    use super::{ProductEmissionErrorKind, ProductEmissionInputs, validate_executable_units};
    use crate::{
        CancellationToken, Compilation, CompilationOptions, CompilationRequest,
        PackageInterfaceExportRequest, SelectedTarget, WorkerBudget,
    };

    #[test]
    fn package_interface_emission_reuses_pure_facts_across_publications() {
        let compilation = compilation();
        let target_outputs = target_outputs();
        let destination = TemporaryFile::write("library.brayi", b"old");

        let inputs = ProductEmissionInputs::new(&target_outputs);

        let first = compilation
            .emit_product(emission_request(destination.path()), inputs)
            .unwrap_or_else(|error| panic!("first interface emission must complete: {error:?}"));

        let first_bytes = std::fs::read(destination.path())
            .unwrap_or_else(|error| panic!("first interface output must be readable: {error:?}"));

        let second = compilation
            .emit_product(emission_request(destination.path()), inputs)
            .unwrap_or_else(|error| panic!("repeated interface emission must complete: {error:?}"));

        let second_bytes = std::fs::read(destination.path())
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
            .unwrap_or_else(|| panic!("interface bundle fact must remain published"));

        let second_bundle = compilation
            .package_interface_export_bundle()
            .and_then(|result| result.as_ref().ok())
            .unwrap_or_else(|| panic!("interface bundle fact must remain reusable"));

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
    fn async_executable_plans_require_exact_generated_host_and_root_frame_artifacts() {
        let product = ProductIdentity::try_new(package_identity(), "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let runtime = RuntimeArtifactId::try_new("runtime.test")
            .unwrap_or_else(|| panic!("test runtime identity must be valid"));

        let frame_template = ProtectedAsyncFrameId::new([7; 32]);
        let frame_mir = protected_frame_mir(frame_template);

        let frame = bray_codegen::CodegenInstance::non_generic(frame_mir.clone())
            .protected_frame_identity()
            .unwrap_or_else(|| panic!("test frame must have a concrete identity"));

        let host = test_async_executable_host_contract_for_frame(
            product.clone(),
            test_mir_target().identity().clone(),
            runtime,
            frame,
        );

        let RootExecution::Asynchronous { frame: host_frame } = host.root() else {
            panic!("test executable host must be asynchronous");
        };

        assert_eq!(host_frame, frame);

        let host_mir = executable_host_mir(host.clone());

        let complete_unit = bray_codegen::CodegenUnit::try_new(1, [host_mir.clone(), frame_mir])
            .unwrap_or_else(|error| panic!("complete test unit must be valid: {error:?}"));

        let complete_plan = async_plan(product.clone(), host.clone(), &complete_unit);

        assert_eq!(
            validate_executable_units(&complete_plan, &[complete_unit]),
            Ok(()),
        );

        let host_only = bray_codegen::CodegenUnit::try_new(1, [host_mir.clone()])
            .unwrap_or_else(|error| panic!("host-only test unit must be valid: {error:?}"));

        let host_only_plan = async_plan(product.clone(), host.clone(), &host_only);

        assert_eq!(
            validate_executable_units(&host_only_plan, &[host_only]),
            Err(ProductEmissionErrorKind::MissingRootFrame(frame)),
        );

        let other_frame = ProtectedAsyncFrameId::new([8; 32]);

        let wrong_frame = bray_codegen::CodegenUnit::try_new(
            1,
            [host_mir.clone(), protected_frame_mir(other_frame)],
        )
        .unwrap_or_else(|error| panic!("wrong-frame test unit must be valid: {error:?}"));

        let wrong_frame_plan = async_plan(product.clone(), host.clone(), &wrong_frame);

        assert_eq!(
            validate_executable_units(&wrong_frame_plan, &[wrong_frame]),
            Err(ProductEmissionErrorKind::MissingRootFrame(frame)),
        );

        let mismatched_runtime = RuntimeArtifactId::try_new("runtime.other")
            .unwrap_or_else(|| panic!("mismatched runtime identity must be valid"));

        let mismatched_host = test_async_executable_host_contract_for_frame(
            product.clone(),
            test_mir_target().identity().clone(),
            mismatched_runtime,
            frame,
        );

        let wrong_host = bray_codegen::CodegenUnit::try_new(
            1,
            [
                executable_host_mir(mismatched_host),
                protected_frame_mir(frame),
            ],
        )
        .unwrap_or_else(|error| panic!("wrong-host test unit must be valid: {error:?}"));

        let wrong_host_plan = async_plan(product, host, &wrong_host);

        assert_eq!(
            validate_executable_units(&wrong_host_plan, &[wrong_host]),
            Err(ProductEmissionErrorKind::MissingExecutableHost),
        );
    }

    fn compilation() -> Compilation {
        let package = package_identity();

        let product = InterfaceProductIdentity::try_new("library")
            .unwrap_or_else(|| panic!("test interface product identity must be valid"));

        let identity = PackageInterfaceIdentity::try_new(
            package.clone(),
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
            RequestedArtifactDestination::FilesystemFile(destination.to_path_buf()),
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
            root,
            host,
            test_mir_target(),
        ))
        .unwrap_or_else(|error| panic!("test executable host must lower: {error:?}"))
    }

    fn protected_frame_mir(frame: ProtectedAsyncFrameId) -> MirUnit {
        let bound = test_bound_unit(8);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::ProtectedAsyncFrame(frame),
            test_mir_target(),
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap_or_else(|error| panic!("test frame block must be valid: {error:?}"));

        builder
            .set_terminator(entry, source, MirTerminatorKind::Return(None))
            .unwrap_or_else(|error| panic!("test frame terminator must be valid: {error:?}"));

        let state = MirFrameStateFacts::new(MirFrameStateId::new(0), entry, [], None, [], []);

        let descriptor = MirFrameDescriptor::try_new(
            frame,
            RuntimeAbiVersion::new(1, 0),
            ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(1, 0)),
            test_mir_type(),
            [state],
        )
        .unwrap_or_else(|error| panic!("test frame descriptor must be valid: {error:?}"));

        builder
            .set_frame_descriptor(descriptor)
            .unwrap_or_else(|error| panic!("test frame descriptor must commit: {error:?}"));

        builder
            .finish(entry)
            .unwrap_or_else(|error| panic!("test frame MIR must be valid: {error:?}"))
    }

    fn async_plan(
        product: ProductIdentity,
        host: bray_runtime_interface::ExecutableHostContract,
        unit: &bray_codegen::CodegenUnit,
    ) -> bray_emitter::EmissionPlan {
        let backend = EmissionBackend::try_new(
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
        )
        .unwrap_or_else(|error| panic!("test emission backend must be valid: {error:?}"));

        let name = TargetOutputName::try_new(TargetOutputKind::Executable, "", "")
            .unwrap_or_else(|error| panic!("test executable name must be valid: {error:?}"));

        let target = TargetOutputDescription::try_new(test_target_profile(), [name])
            .unwrap_or_else(|error| panic!("test target outputs must be valid: {error:?}"));

        let request = EmissionRequest::try_new(
            product,
            ProductKind::Executable,
            Some(host),
            test_mir_target().identity().clone(),
            RequestedArtifactDestination::FilesystemFile("application".into()),
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
        bray_codegen::BackendCapabilities::new(
            [bray_codegen::BackendTargetPlatform::new(
                bray_target::TargetArchitecture::X86_64,
                bray_target::ObjectFormat::Elf,
            )],
            [bray_codegen::BackendArtifactKind::RelocatableObject],
            [bray_codegen::DebugInformationMode::None],
            [bray_codegen::AssemblySyntaxKind::TargetDefault],
        )
    }
}
