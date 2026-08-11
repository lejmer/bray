use std::sync::Arc;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticExternalToolFailureKind,
    DiagnosticExternalToolExit, DiagnosticExternalToolOperation, DiagnosticId,
    DiagnosticIoErrorKind, DiagnosticKind, DiagnosticLinkRequirement,
    DiagnosticLinkerDriverIdentity, DiagnosticLinkerDriverKind, DiagnosticNote,
    DiagnosticNoteKind, SeverityKind,
};
use bray_platform::{PlatformErrorKind, PlatformOperation};
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use crate::{
    ExternalToolFailure, ExternalToolOutput, ExternalToolResponseFileOperation,
    ExternalToolStream, LinkInputId, LinkPlan, LinkedArtifact, LinkedArtifactRequirement,
    LinkerDriverIdentity,
    StagingDestinationId, UnsupportedLinkRequirement,
};

pub(crate) fn failed_outcome(plan: &LinkPlan, failure: LinkFailure) -> LinkOutcome {
    LinkOutcome::failed(plan, failure, DiagnosticBag::new())
}

/// Creates stable structured diagnostics for one typed native link failure.
pub fn link_failure_diagnostics(failure: &LinkFailure) -> DiagnosticBag {
    DiagnosticBag::single(link_failure_diagnostic(failure))
}

fn link_failure_diagnostics_for_plan(plan: &LinkPlan, failure: &LinkFailure) -> DiagnosticBag {
    let mut diagnostic = with_plan_context(link_failure_diagnostic(failure), plan);

    match failure {
        LinkFailure::MissingInput(id) => {
            if let Some(input) = plan.input(*id) {
                diagnostic = diagnostic.with_arg(DiagnosticArg::link_requirement(
                    diagnostic_input_requirement(input.kind()),
                ));

                if let crate::LinkInputSource::File(path) = input.source() {
                    diagnostic = diagnostic.with_arg(DiagnosticArg::file_path(path));
                }
            }
        }
        LinkFailure::MissingOutput(id) | LinkFailure::InvalidOutput(id) => {
            if let Some(output) = plan.output(*id) {
                diagnostic = diagnostic
                    .with_arg(DiagnosticArg::link_requirement(
                        diagnostic_output_requirement(output.kind()),
                    ))
                    .with_arg(DiagnosticArg::file_path(output.destination().path()));
            }
        }
        LinkFailure::DriverUnavailable
        | LinkFailure::DriverIncompatible
        | LinkFailure::UnsupportedRequirement(_)
        | LinkFailure::ResponseFile
        | LinkFailure::Invocation
        | LinkFailure::ToolExit(_)
        | LinkFailure::ResourceExhausted => {}
    }

    DiagnosticBag::single(diagnostic)
}

fn with_plan_context(diagnostic: Diagnostic, plan: &LinkPlan) -> Diagnostic {
    diagnostic
        .with_arg(DiagnosticArg::target_triple(plan.target().triple()))
        .with_arg(DiagnosticArg::actual_product_identity(
            plan.product().to_string(),
        ))
        .with_arg(DiagnosticArg::linker_driver_identity(
            diagnostic_driver_identity(plan.driver()),
        ))
        .with_note(
            DiagnosticNote::new(DiagnosticNoteKind::LinkPlanContext)
                .with_arg(DiagnosticArg::actual_product_identity(
                    plan.product().to_string(),
                ))
                .with_arg(DiagnosticArg::target_triple(plan.target().triple()))
                .with_arg(DiagnosticArg::linker_driver_identity(
                    diagnostic_driver_identity(plan.driver()),
                )),
        )
}

fn diagnostic_driver_identity(driver: &LinkerDriverIdentity) -> DiagnosticLinkerDriverIdentity {
    let kind = match driver.kind() {
        crate::LinkerDriverKind::EmbeddedLld => DiagnosticLinkerDriverKind::EmbeddedLld,
        crate::LinkerDriverKind::ExternalLld => DiagnosticLinkerDriverKind::ExternalLld,
        crate::LinkerDriverKind::System => DiagnosticLinkerDriverKind::System,
        crate::LinkerDriverKind::Archiver => DiagnosticLinkerDriverKind::Archiver,
        crate::LinkerDriverKind::TargetSpecific => DiagnosticLinkerDriverKind::TargetSpecific,
    };

    DiagnosticLinkerDriverIdentity::new(
        kind,
        driver.name(),
        driver.capability_revision(),
        driver.toolchain_revision(),
    )
}

fn link_failure_diagnostic(failure: &LinkFailure) -> Diagnostic {
    match failure {
        LinkFailure::UnsupportedRequirement(requirement) => Diagnostic::new(
            DiagnosticId::new(0),
            unsupported_requirement_diagnostic(requirement),
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::link_requirement(
            diagnostic_link_requirement(requirement),
        )),
        LinkFailure::DriverUnavailable => {
            failure_diagnostic(DiagnosticKind::LinkerDriverUnavailable)
        }
        LinkFailure::DriverIncompatible => compiler_defect_diagnostic(
            DiagnosticKind::LinkerDriverIncompatible,
        ),
        LinkFailure::MissingInput(input) => failure_diagnostic(DiagnosticKind::LinkerInputMissing)
            .with_arg(
                DiagnosticArg::input_index(
                    usize::try_from(input.ordinal())
                        .unwrap_or_else(|_| unreachable!("u32 link input ordinal must fit usize")),
                )
                .unwrap_or_else(|| unreachable!("u32 link input ordinal must fit usize")),
            ),
        LinkFailure::ResponseFile => failure_diagnostic(DiagnosticKind::LinkerResponseFileFailed),
        LinkFailure::Invocation => compiler_defect_diagnostic(
            DiagnosticKind::LinkerInvocationFailed,
        ),
        LinkFailure::ToolExit(output) => failure_diagnostic(
            DiagnosticKind::LinkerExternalToolExitedUnsuccessfully,
        )
        .with_arg(DiagnosticArg::external_tool_exit(
            DiagnosticExternalToolExit::new(
                output.exit_code(),
                output.standard_output(),
                output.standard_error(),
            ),
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::ExternalToolExitRequiresCorrection,
        )),
        LinkFailure::MissingOutput(output) => {
            failure_diagnostic(DiagnosticKind::LinkerOutputMissing)
                .with_arg(DiagnosticArg::artifact_ordinal(output.ordinal()))
        }
        LinkFailure::InvalidOutput(output) => {
            failure_diagnostic(DiagnosticKind::LinkerOutputInvalid)
                .with_arg(DiagnosticArg::artifact_ordinal(output.ordinal()))
        }
        LinkFailure::ResourceExhausted => {
            failure_diagnostic(DiagnosticKind::LinkerResourceExhausted)
        }
    }
}

fn failure_diagnostic(kind: DiagnosticKind) -> Diagnostic {
    Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
}

fn compiler_defect_diagnostic(kind: DiagnosticKind) -> Diagnostic {
    failure_diagnostic(kind).with_note(DiagnosticNote::new(
        DiagnosticNoteKind::ReportCompilerDefect,
    ))
}

fn external_tool_io_diagnostics(
    plan: &LinkPlan,
    operation: DiagnosticExternalToolOperation,
    kind: std::io::ErrorKind,
) -> DiagnosticBag {
    DiagnosticBag::single(with_plan_context(
        failure_diagnostic(DiagnosticKind::LinkerExternalToolIoFailed)
            .with_arg(DiagnosticArg::external_tool_operation(operation))
            .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
                kind,
            ))),
        plan,
    ))
}

fn external_tool_contract_diagnostics(
    plan: &LinkPlan,
    operation: DiagnosticExternalToolOperation,
    kind: DiagnosticExternalToolFailureKind,
) -> DiagnosticBag {
    DiagnosticBag::single(with_plan_context(
        failure_diagnostic(DiagnosticKind::LinkerExternalToolContractFailed)
            .with_arg(DiagnosticArg::external_tool_operation(operation))
            .with_arg(DiagnosticArg::external_tool_failure_kind(kind)),
        plan,
    ))
}

const fn response_file_operation(
    operation: ExternalToolResponseFileOperation,
) -> DiagnosticExternalToolOperation {
    match operation {
        ExternalToolResponseFileOperation::Write => {
            DiagnosticExternalToolOperation::ResponseFileWrite
        }
        ExternalToolResponseFileOperation::Remove => {
            DiagnosticExternalToolOperation::ResponseFileRemove
        }
    }
}

const fn stream_operation(
    stream: ExternalToolStream,
    stdout: DiagnosticExternalToolOperation,
    stderr: DiagnosticExternalToolOperation,
) -> DiagnosticExternalToolOperation {
    match stream {
        ExternalToolStream::StandardOutput => stdout,
        ExternalToolStream::StandardError => stderr,
    }
}

const fn platform_operation(operation: PlatformOperation) -> DiagnosticExternalToolOperation {
    match operation {
        PlatformOperation::ThreadSpawn => DiagnosticExternalToolOperation::ThreadSpawn,
        PlatformOperation::ThreadJoin => DiagnosticExternalToolOperation::ThreadJoin,
        PlatformOperation::ThreadRuntimeInitialization => {
            DiagnosticExternalToolOperation::ThreadRuntimeInitialization
        }
        PlatformOperation::Event => DiagnosticExternalToolOperation::Event,
        PlatformOperation::EventPoll => DiagnosticExternalToolOperation::EventPoll,
        PlatformOperation::EventRegistration => DiagnosticExternalToolOperation::EventRegistration,
        PlatformOperation::VirtualMemory => DiagnosticExternalToolOperation::VirtualMemory,
        PlatformOperation::ProcessSpawn => DiagnosticExternalToolOperation::ProcessSpawn,
        PlatformOperation::ProcessSignal => DiagnosticExternalToolOperation::ProcessSignal,
        PlatformOperation::ProcessWait => DiagnosticExternalToolOperation::ProcessWait,
        PlatformOperation::SocketAddressResolution => {
            DiagnosticExternalToolOperation::SocketAddressResolution
        }
        PlatformOperation::SocketAddress => DiagnosticExternalToolOperation::SocketAddress,
        PlatformOperation::SocketBind => DiagnosticExternalToolOperation::SocketBind,
        PlatformOperation::SocketConfiguration => {
            DiagnosticExternalToolOperation::SocketConfiguration
        }
        PlatformOperation::SocketConnect => DiagnosticExternalToolOperation::SocketConnect,
        PlatformOperation::SocketAccept => DiagnosticExternalToolOperation::SocketAccept,
        PlatformOperation::SocketReceive => DiagnosticExternalToolOperation::SocketReceive,
        PlatformOperation::SocketSend => DiagnosticExternalToolOperation::SocketSend,
    }
}

const fn platform_failure(kind: PlatformErrorKind) -> DiagnosticExternalToolFailureKind {
    match kind {
        PlatformErrorKind::Io(_) => DiagnosticExternalToolFailureKind::Io,
        PlatformErrorKind::InvalidSize => DiagnosticExternalToolFailureKind::InvalidSize,
        PlatformErrorKind::ThreadIdentityExhausted => {
            DiagnosticExternalToolFailureKind::ThreadIdentityExhausted
        }
        PlatformErrorKind::EventGenerationExhausted => {
            DiagnosticExternalToolFailureKind::EventGenerationExhausted
        }
        PlatformErrorKind::InvalidEventIdentity => {
            DiagnosticExternalToolFailureKind::InvalidEventIdentity
        }
        PlatformErrorKind::RuntimeThreadAlreadyInitialized => {
            DiagnosticExternalToolFailureKind::RuntimeThreadAlreadyInitialized
        }
        PlatformErrorKind::SynchronizationPoisoned => {
            DiagnosticExternalToolFailureKind::SynchronizationPoisoned
        }
        PlatformErrorKind::Unsupported => DiagnosticExternalToolFailureKind::Unsupported,
    }
}

fn diagnostic_link_requirement(
    requirement: &UnsupportedLinkRequirement,
) -> DiagnosticLinkRequirement {
    match requirement {
        UnsupportedLinkRequirement::Target { triple, .. } => {
            DiagnosticLinkRequirement::Target(triple.to_string())
        }
        UnsupportedLinkRequirement::Product(value) => diagnostic_product_requirement(*value),
        UnsupportedLinkRequirement::Input(value) => diagnostic_input_requirement(*value),
        UnsupportedLinkRequirement::InputMode(value) => diagnostic_input_mode_requirement(*value),
        UnsupportedLinkRequirement::Output(value) => diagnostic_output_requirement(*value),
        UnsupportedLinkRequirement::SearchPath(value) => diagnostic_search_path_requirement(*value),
        UnsupportedLinkRequirement::LinkModel(value) => diagnostic_link_model_requirement(*value),
        UnsupportedLinkRequirement::DeadStrip(value) => diagnostic_dead_strip_requirement(*value),
        UnsupportedLinkRequirement::SectionGarbageCollection(value) => {
            diagnostic_section_garbage_collection_requirement(*value)
        }
        UnsupportedLinkRequirement::Debug(value) => diagnostic_debug_requirement(*value),
        UnsupportedLinkRequirement::Subsystem(value) => diagnostic_subsystem_requirement(*value),
        UnsupportedLinkRequirement::Symbol(value) => diagnostic_symbol_requirement(*value),
        UnsupportedLinkRequirement::Startup(value) => diagnostic_startup_requirement(*value),
        UnsupportedLinkRequirement::Runtime(value) => diagnostic_runtime_requirement(*value),
    }
}

const fn diagnostic_product_requirement(
    value: crate::LinkedProductKind,
) -> DiagnosticLinkRequirement {
    match value {
        crate::LinkedProductKind::Executable => DiagnosticLinkRequirement::ProductExecutable,
        crate::LinkedProductKind::SharedLibrary => DiagnosticLinkRequirement::ProductSharedLibrary,
        crate::LinkedProductKind::StaticLibrary => DiagnosticLinkRequirement::ProductStaticLibrary,
    }
}

const fn diagnostic_input_requirement(value: crate::LinkInputKind) -> DiagnosticLinkRequirement {
    match value {
        crate::LinkInputKind::RelocatableObject => {
            DiagnosticLinkRequirement::InputRelocatableObject
        }
        crate::LinkInputKind::Bitcode => DiagnosticLinkRequirement::InputBitcode,
        crate::LinkInputKind::Archive => DiagnosticLinkRequirement::InputArchive,
        crate::LinkInputKind::StartupObject => DiagnosticLinkRequirement::InputStartupObject,
        crate::LinkInputKind::TerminationObject => {
            DiagnosticLinkRequirement::InputTerminationObject
        }
        crate::LinkInputKind::RuntimeComponent => DiagnosticLinkRequirement::InputRuntimeComponent,
        crate::LinkInputKind::NativeLibrary => DiagnosticLinkRequirement::InputNativeLibrary,
        crate::LinkInputKind::Framework => DiagnosticLinkRequirement::InputFramework,
    }
}

const fn diagnostic_input_mode_requirement(
    value: crate::LinkInputMode,
) -> DiagnosticLinkRequirement {
    match value {
        crate::LinkInputMode::Ordinary => DiagnosticLinkRequirement::InputModeOrdinary,
        crate::LinkInputMode::WholeArchive => DiagnosticLinkRequirement::InputModeWholeArchive,
    }
}

const fn diagnostic_output_requirement(
    value: crate::LinkedArtifactKind,
) -> DiagnosticLinkRequirement {
    match value {
        crate::LinkedArtifactKind::Executable => DiagnosticLinkRequirement::OutputExecutable,
        crate::LinkedArtifactKind::SharedLibrary => DiagnosticLinkRequirement::OutputSharedLibrary,
        crate::LinkedArtifactKind::StaticLibrary => DiagnosticLinkRequirement::OutputStaticLibrary,
        crate::LinkedArtifactKind::ImportLibrary => DiagnosticLinkRequirement::OutputImportLibrary,
        crate::LinkedArtifactKind::DebugCompanion => {
            DiagnosticLinkRequirement::OutputDebugCompanion
        }
        crate::LinkedArtifactKind::PlatformCompanion => {
            DiagnosticLinkRequirement::OutputPlatformCompanion
        }
    }
}

const fn diagnostic_search_path_requirement(
    value: crate::LinkSearchPathKind,
) -> DiagnosticLinkRequirement {
    match value {
        crate::LinkSearchPathKind::Library => DiagnosticLinkRequirement::SearchPathLibrary,
        crate::LinkSearchPathKind::Framework => DiagnosticLinkRequirement::SearchPathFramework,
    }
}

const fn diagnostic_link_model_requirement(value: crate::LinkModel) -> DiagnosticLinkRequirement {
    match value {
        crate::LinkModel::Default => DiagnosticLinkRequirement::LinkModelDefault,
        crate::LinkModel::Static => DiagnosticLinkRequirement::LinkModelStatic,
        crate::LinkModel::Dynamic => DiagnosticLinkRequirement::LinkModelDynamic,
    }
}

const fn diagnostic_dead_strip_requirement(
    value: crate::DeadStripPolicy,
) -> DiagnosticLinkRequirement {
    match value {
        crate::DeadStripPolicy::Preserve => DiagnosticLinkRequirement::DeadStripPreserve,
        crate::DeadStripPolicy::RemoveUnreachable => {
            DiagnosticLinkRequirement::DeadStripRemoveUnreachable
        }
    }
}

const fn diagnostic_section_garbage_collection_requirement(
    value: crate::SectionGarbageCollectionPolicy,
) -> DiagnosticLinkRequirement {
    match value {
        crate::SectionGarbageCollectionPolicy::Preserve => {
            DiagnosticLinkRequirement::SectionGarbageCollectionPreserve
        }
        crate::SectionGarbageCollectionPolicy::RemoveUnreferenced => {
            DiagnosticLinkRequirement::SectionGarbageCollectionRemoveUnreferenced
        }
    }
}

const fn diagnostic_debug_requirement(value: crate::DebugLinkPolicy) -> DiagnosticLinkRequirement {
    match value {
        crate::DebugLinkPolicy::None => DiagnosticLinkRequirement::DebugNone,
        crate::DebugLinkPolicy::Embedded => DiagnosticLinkRequirement::DebugEmbedded,
        crate::DebugLinkPolicy::Companion => DiagnosticLinkRequirement::DebugCompanion,
    }
}

const fn diagnostic_subsystem_requirement(
    value: crate::LinkSubsystem,
) -> DiagnosticLinkRequirement {
    match value {
        crate::LinkSubsystem::Console => DiagnosticLinkRequirement::SubsystemConsole,
        crate::LinkSubsystem::Windowed => DiagnosticLinkRequirement::SubsystemWindowed,
        crate::LinkSubsystem::Native => DiagnosticLinkRequirement::SubsystemNative,
        crate::LinkSubsystem::WasiCommand => DiagnosticLinkRequirement::SubsystemWasiCommand,
        crate::LinkSubsystem::WasiReactor => DiagnosticLinkRequirement::SubsystemWasiReactor,
    }
}

const fn diagnostic_symbol_requirement(
    value: crate::LinkSymbolRequirement,
) -> DiagnosticLinkRequirement {
    match value {
        crate::LinkSymbolRequirement::EntryPoint => DiagnosticLinkRequirement::SymbolEntryPoint,
        crate::LinkSymbolRequirement::ExportedSymbols => {
            DiagnosticLinkRequirement::SymbolExportedSymbols
        }
        crate::LinkSymbolRequirement::RetainedSymbols => {
            DiagnosticLinkRequirement::SymbolRetainedSymbols
        }
    }
}

const fn diagnostic_startup_requirement(
    value: crate::LinkStartupMode,
) -> DiagnosticLinkRequirement {
    match value {
        crate::LinkStartupMode::NotApplicable => DiagnosticLinkRequirement::StartupNotApplicable,
        crate::LinkStartupMode::ExplicitInputs => DiagnosticLinkRequirement::StartupExplicitInputs,
        crate::LinkStartupMode::PlatformCompilerDriver => {
            DiagnosticLinkRequirement::StartupPlatformCompilerDriver
        }
    }
}

const fn diagnostic_runtime_requirement(
    value: crate::LinkRuntimeMode,
) -> DiagnosticLinkRequirement {
    match value {
        crate::LinkRuntimeMode::ExplicitInput => DiagnosticLinkRequirement::RuntimeExplicitInput,
    }
}

const fn unsupported_requirement_diagnostic(
    requirement: &UnsupportedLinkRequirement,
) -> DiagnosticKind {
    match requirement {
        UnsupportedLinkRequirement::Target { .. } => DiagnosticKind::LinkerUnsupportedTarget,
        UnsupportedLinkRequirement::Product(_) => DiagnosticKind::LinkerUnsupportedProduct,
        UnsupportedLinkRequirement::Input(_) => DiagnosticKind::LinkerUnsupportedInput,
        UnsupportedLinkRequirement::InputMode(_) => DiagnosticKind::LinkerUnsupportedInputMode,
        UnsupportedLinkRequirement::Output(_) => DiagnosticKind::LinkerUnsupportedOutput,
        UnsupportedLinkRequirement::SearchPath(_) => DiagnosticKind::LinkerUnsupportedSearchPath,
        UnsupportedLinkRequirement::LinkModel(_) => DiagnosticKind::LinkerUnsupportedLinkModel,
        UnsupportedLinkRequirement::DeadStrip(_) => DiagnosticKind::LinkerUnsupportedDeadStrip,
        UnsupportedLinkRequirement::SectionGarbageCollection(_) => {
            DiagnosticKind::LinkerUnsupportedSectionGarbageCollection
        }
        UnsupportedLinkRequirement::Debug(_) => DiagnosticKind::LinkerUnsupportedDebug,
        UnsupportedLinkRequirement::Subsystem(_) => DiagnosticKind::LinkerUnsupportedSubsystem,
        UnsupportedLinkRequirement::Symbol(_) => DiagnosticKind::LinkerUnsupportedSymbol,
        UnsupportedLinkRequirement::Startup(_) => DiagnosticKind::LinkerUnsupportedStartup,
        UnsupportedLinkRequirement::Runtime(_) => DiagnosticKind::LinkerUnsupportedRuntime,
    }
}

/// Structured reason one native link operation could not produce complete staged outputs.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum LinkFailure {
    /// No configured linker driver is available for the request.
    DriverUnavailable,
    /// The selected driver is incompatible with the target or product contract.
    DriverIncompatible,
    /// The selected driver does not support one typed plan requirement.
    UnsupportedRequirement(UnsupportedLinkRequirement),
    /// One planned input was unavailable at invocation time.
    MissingInput(LinkInputId),
    /// Driver-owned response-file construction failed.
    ResponseFile,
    /// Linker or archiver invocation could not complete successfully.
    Invocation,
    /// An external linker or archiver completed with an unsuccessful status and captured output.
    ToolExit(ExternalToolOutput),
    /// One required linked output was not produced.
    MissingOutput(StagingDestinationId),
    /// One produced linked output violated its staging contract.
    InvalidOutput(StagingDestinationId),
    /// Linking exceeded an available resource or process budget.
    ResourceExhausted,
}

/// Complete canonically ordered staging records for one link plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkedArtifactSet {
    product: ProductIdentity,
    target: TargetIdentity,
    driver: LinkerDriverIdentity,
    artifacts: Arc<[LinkedArtifact]>,
}

impl LinkedArtifactSet {
    fn try_new(
        plan: &LinkPlan,
        artifacts: impl IntoIterator<Item = LinkedArtifact>,
    ) -> Result<Self, LinkedArtifactSetBuildError> {
        let mut artifacts: Vec<_> = artifacts.into_iter().collect();

        artifacts.sort_unstable_by_key(LinkedArtifact::destination);

        if let Some(pair) = artifacts
            .windows(2)
            .find(|pair| pair[0].destination() == pair[1].destination())
        {
            return Err(LinkedArtifactSetBuildError::DuplicateArtifact(
                pair[0].destination(),
            ));
        }

        for artifact in &artifacts {
            validate_artifact(plan, artifact)?;
        }

        for output in plan.outputs() {
            if output.requirement() == LinkedArtifactRequirement::Required
                && artifacts
                    .binary_search_by_key(&output.destination().id(), LinkedArtifact::destination)
                    .is_err()
            {
                return Err(LinkedArtifactSetBuildError::MissingRequired(
                    output.destination().id(),
                ));
            }
        }

        Ok(Self {
            // Completed results retain the Arc-backed product identity independently of the plan.
            product: plan.product().clone(),
            // Completed results retain the Arc-backed target identity independently of the plan.
            target: plan.target().identity().clone(),
            // Completed results retain driver revision metadata independently of the plan.
            driver: plan.driver().clone(),
            artifacts: artifacts.into(),
        })
    }

    /// Returns the selected package product.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the exact target identity used by the link operation.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns the exact driver and toolchain identity used by the operation.
    pub const fn driver(&self) -> &LinkerDriverIdentity {
        &self.driver
    }

    /// Returns staged artifacts in canonical staging-identity order.
    pub fn artifacts(&self) -> &[LinkedArtifact] {
        &self.artifacts
    }

    /// Returns one completed artifact by emitter-owned staging identity.
    pub fn artifact(&self, destination: StagingDestinationId) -> Option<&LinkedArtifact> {
        self.artifacts
            .binary_search_by_key(&destination, LinkedArtifact::destination)
            .ok()
            .map(|index| &self.artifacts[index])
    }
}

/// A contract violation that prevents complete linked artifacts from being exposed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkedArtifactSetBuildError {
    /// One staging identity appears more than once.
    DuplicateArtifact(StagingDestinationId),
    /// A required staged output has no completed record.
    MissingRequired(StagingDestinationId),
    /// A completed record was not present in the authoritative plan.
    UnplannedArtifact(StagingDestinationId),
    /// A completed record names the wrong staged artifact category.
    KindMismatch(StagingDestinationId),
}

fn validate_artifact(
    plan: &LinkPlan,
    artifact: &LinkedArtifact,
) -> Result<(), LinkedArtifactSetBuildError> {
    let Some(output) = plan.output(artifact.destination()) else {
        return Err(LinkedArtifactSetBuildError::UnplannedArtifact(
            artifact.destination(),
        ));
    };

    if output.kind() != artifact.kind() {
        return Err(LinkedArtifactSetBuildError::KindMismatch(
            artifact.destination(),
        ));
    }

    Ok(())
}

/// Atomic completion state of one native link operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkStatus {
    /// Every required staged artifact was produced and validated.
    Complete(LinkedArtifactSet),
    /// Linking failed without returning partial staged outputs.
    Failed(LinkFailure),
    /// Cancellation was observed before a successful result was available.
    Cancelled,
}

/// Immutable native-link result and locale-neutral diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkOutcome {
    status: LinkStatus,
    diagnostics: DiagnosticBag,
}

impl LinkOutcome {
    pub(crate) fn from_external_tool_failure(
        plan: &LinkPlan,
        failure: ExternalToolFailure,
    ) -> Self {
        match failure {
            ExternalToolFailure::Cancelled => Self::cancelled(DiagnosticBag::new()),
            ExternalToolFailure::ProcessBudgetUnavailable => {
                failed_outcome(plan, LinkFailure::ResourceExhausted)
            }
            ExternalToolFailure::ResponseFile {
                path,
                operation,
                kind,
            } => {
                let diagnostic = with_plan_context(
                    failure_diagnostic(DiagnosticKind::LinkerResponseFileFailed)
                        .with_arg(DiagnosticArg::file_path(path))
                        .with_arg(DiagnosticArg::external_tool_operation(
                            response_file_operation(operation),
                        ))
                        .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
                            kind,
                        ))),
                    plan,
                );

                Self::failed_with_diagnostics(
                    LinkFailure::ResponseFile,
                    DiagnosticBag::single(diagnostic),
                )
            }
            ExternalToolFailure::Process(error) => {
                let operation = platform_operation(error.operation());

                let diagnostics = match error.kind() {
                    PlatformErrorKind::Io(kind) => {
                        external_tool_io_diagnostics(plan, operation, kind)
                    }
                    kind => {
                        external_tool_contract_diagnostics(plan, operation, platform_failure(kind))
                    }
                };

                Self::failed_with_diagnostics(LinkFailure::Invocation, diagnostics)
            }
            ExternalToolFailure::MissingOutputPipe(stream) => Self::failed_with_diagnostics(
                LinkFailure::Invocation,
                external_tool_contract_diagnostics(
                    plan,
                    stream_operation(
                        stream,
                        DiagnosticExternalToolOperation::StandardOutputPipe,
                        DiagnosticExternalToolOperation::StandardErrorPipe,
                    ),
                    DiagnosticExternalToolFailureKind::MissingOutputPipe,
                ),
            ),
            ExternalToolFailure::OutputCapture { stream, kind } => Self::failed_with_diagnostics(
                LinkFailure::Invocation,
                external_tool_io_diagnostics(
                    plan,
                    stream_operation(
                        stream,
                        DiagnosticExternalToolOperation::StandardOutputCapture,
                        DiagnosticExternalToolOperation::StandardErrorCapture,
                    ),
                    kind,
                ),
            ),
            ExternalToolFailure::OutputReaderTerminated(stream) => Self::failed_with_diagnostics(
                LinkFailure::Invocation,
                external_tool_contract_diagnostics(
                    plan,
                    stream_operation(
                        stream,
                        DiagnosticExternalToolOperation::StandardOutputReader,
                        DiagnosticExternalToolOperation::StandardErrorReader,
                    ),
                    DiagnosticExternalToolFailureKind::OutputReaderTerminated,
                ),
            ),
        }
    }

    /// Validates staging records against the authoritative plan before publishing success.
    pub fn try_complete(
        plan: &LinkPlan,
        artifacts: impl IntoIterator<Item = LinkedArtifact>,
        diagnostics: DiagnosticBag,
    ) -> Result<Self, LinkOutcomeBuildError> {
        if diagnostics.has_errors() {
            return Err(LinkOutcomeBuildError::ErrorDiagnostics(diagnostics));
        }

        let artifacts = LinkedArtifactSet::try_new(plan, artifacts)
            .map_err(LinkOutcomeBuildError::InvalidArtifacts)?;

        Ok(Self {
            status: LinkStatus::Complete(artifacts),
            diagnostics,
        })
    }

    /// Creates a failed result with a canonical plan-aware terminal diagnostic.
    pub fn failed(plan: &LinkPlan, failure: LinkFailure, diagnostics: DiagnosticBag) -> Self {
        let diagnostics = diagnostics.merged(&link_failure_diagnostics_for_plan(plan, &failure));

        Self::failed_with_diagnostics(failure, diagnostics)
    }

    fn failed_with_diagnostics(failure: LinkFailure, diagnostics: DiagnosticBag) -> Self {
        Self {
            status: LinkStatus::Failed(failure),
            diagnostics,
        }
    }

    /// Creates a cancelled result without partial staged outputs.
    pub const fn cancelled(diagnostics: DiagnosticBag) -> Self {
        Self {
            status: LinkStatus::Cancelled,
            diagnostics,
        }
    }

    /// Returns the atomic completion state.
    pub const fn status(&self) -> &LinkStatus {
        &self.status
    }

    /// Returns diagnostics produced by the completed or failed operation.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Consumes the outcome and returns its diagnostics.
    pub fn into_diagnostics(self) -> DiagnosticBag {
        self.diagnostics
    }

    /// Returns complete staged artifacts only after successful validation.
    pub const fn artifacts(&self) -> Option<&LinkedArtifactSet> {
        match &self.status {
            LinkStatus::Complete(artifacts) => Some(artifacts),
            LinkStatus::Failed(_) | LinkStatus::Cancelled => None,
        }
    }
}

/// A contract violation that prevents creation of a successful link outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkOutcomeBuildError {
    /// Error diagnostics contradict a successful status.
    ErrorDiagnostics(DiagnosticBag),
    /// Completed staging records do not satisfy the authoritative link plan.
    InvalidArtifacts(LinkedArtifactSetBuildError),
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use std::num::NonZeroU64;

    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::{
        LinkFailure, LinkOutcome, LinkOutcomeBuildError, LinkStatus, LinkedArtifactSetBuildError,
        failed_outcome, link_failure_diagnostics,
    };
    use crate::test_support::{link_plan, linked_artifact};
    use crate::{
        DeadStripPolicy, DebugLinkPolicy, LinkInputKind, LinkInputMode, LinkModel, LinkRuntimeMode,
        LinkSearchPathKind, LinkStartupMode, LinkSubsystem, LinkSymbolRequirement, LinkedArtifact,
        LinkedArtifactKind, LinkedProductKind, SectionGarbageCollectionPolicy,
        StagingDestinationId, UnsupportedLinkRequirement,
    };
    use bray_platform::{PlatformError, PlatformErrorKind, PlatformOperation};
    use bray_testing::{assert_goal_state_diagnostic_kind, assert_goal_state_diagnostics};

    #[test]
    fn complete_outcomes_publish_only_plan_validated_artifacts() {
        let plan = link_plan();
        let artifact = linked_artifact(&plan);

        let Ok(outcome) = LinkOutcome::try_complete(&plan, [artifact], DiagnosticBag::new()) else {
            panic!("matching test artifact must complete linking");
        };

        let Some(artifacts) = outcome.artifacts() else {
            panic!("complete outcome must retain staged artifacts");
        };

        assert_eq!(artifacts.product(), plan.product());
        assert_eq!(artifacts.target(), plan.target().identity());
        assert_eq!(artifacts.driver(), plan.driver());
        assert_eq!(artifacts.driver().capability_revision(), "1");
        assert_eq!(artifacts.artifacts().len(), 1);
    }

    #[test]
    fn complete_outcomes_reject_missing_and_mismatched_outputs() {
        let plan = link_plan();

        assert_eq!(
            LinkOutcome::try_complete(&plan, [], DiagnosticBag::new()),
            Err(LinkOutcomeBuildError::InvalidArtifacts(
                LinkedArtifactSetBuildError::MissingRequired(StagingDestinationId::new(0))
            ))
        );

        let mismatched = LinkedArtifact::new(
            LinkedArtifactKind::SharedLibrary,
            StagingDestinationId::new(0),
            NonZeroU64::MIN,
        );

        assert_eq!(
            LinkOutcome::try_complete(&plan, [mismatched], DiagnosticBag::new()),
            Err(LinkOutcomeBuildError::InvalidArtifacts(
                LinkedArtifactSetBuildError::KindMismatch(StagingDestinationId::new(0))
            ))
        );
    }

    #[test]
    fn complete_outcomes_reject_error_diagnostics() {
        let plan = link_plan();
        let artifact = linked_artifact(&plan);

        let diagnostics = DiagnosticBag::single(Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::RequestMissingSourceInput,
            SeverityKind::Error,
        ));

        assert_eq!(
            LinkOutcome::try_complete(&plan, [artifact], diagnostics.clone()),
            Err(LinkOutcomeBuildError::ErrorDiagnostics(diagnostics))
        );
    }

    #[test]
    fn failed_and_cancelled_outcomes_expose_no_partial_artifacts() {
        let failure = LinkFailure::Invocation;
        let plan = link_plan();
        let failed = LinkOutcome::failed(&plan, failure.clone(), DiagnosticBag::new());

        let diagnostics = DiagnosticBag::single(Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::RequestMissingSourceInput,
            SeverityKind::Error,
        ));

        let cancelled = LinkOutcome::cancelled(diagnostics.clone());

        assert!(matches!(failed.status(), LinkStatus::Failed(_)));
        assert_eq!(failed.artifacts(), None);

        assert!(matches!(cancelled.status(), LinkStatus::Cancelled));
        assert_eq!(cancelled.artifacts(), None);
        assert_eq!(cancelled.diagnostics(), &diagnostics);
    }

    #[test]
    fn failed_outcomes_add_the_matching_plan_aware_terminal_diagnostic() {
        let plan = link_plan();
        let failed = LinkOutcome::failed(&plan, LinkFailure::Invocation, DiagnosticBag::new());

        assert!(failed.diagnostics().has_errors());

        assert!(
            failed
                .diagnostics()
                .iter()
                .any(|diagnostic| { diagnostic.kind() == DiagnosticKind::LinkerInvocationFailed })
        );
    }

    #[test]
    fn unsupported_requirements_produce_stable_error_diagnostics() {
        let plan = link_plan();
        let target = plan.target();

        let cases = [
            (
                UnsupportedLinkRequirement::Target {
                    identity: target.identity().clone(),
                    triple: Arc::from(target.triple()),
                    architecture: target.architecture(),
                    object_format: target.object_format(),
                },
                DiagnosticKind::LinkerUnsupportedTarget,
            ),
            (
                UnsupportedLinkRequirement::Product(LinkedProductKind::Executable),
                DiagnosticKind::LinkerUnsupportedProduct,
            ),
            (
                UnsupportedLinkRequirement::Input(LinkInputKind::Bitcode),
                DiagnosticKind::LinkerUnsupportedInput,
            ),
            (
                UnsupportedLinkRequirement::InputMode(LinkInputMode::WholeArchive),
                DiagnosticKind::LinkerUnsupportedInputMode,
            ),
            (
                UnsupportedLinkRequirement::Output(LinkedArtifactKind::Executable),
                DiagnosticKind::LinkerUnsupportedOutput,
            ),
            (
                UnsupportedLinkRequirement::SearchPath(LinkSearchPathKind::Framework),
                DiagnosticKind::LinkerUnsupportedSearchPath,
            ),
            (
                UnsupportedLinkRequirement::LinkModel(LinkModel::Static),
                DiagnosticKind::LinkerUnsupportedLinkModel,
            ),
            (
                UnsupportedLinkRequirement::DeadStrip(DeadStripPolicy::RemoveUnreachable),
                DiagnosticKind::LinkerUnsupportedDeadStrip,
            ),
            (
                UnsupportedLinkRequirement::SectionGarbageCollection(
                    SectionGarbageCollectionPolicy::RemoveUnreferenced,
                ),
                DiagnosticKind::LinkerUnsupportedSectionGarbageCollection,
            ),
            (
                UnsupportedLinkRequirement::Debug(DebugLinkPolicy::Companion),
                DiagnosticKind::LinkerUnsupportedDebug,
            ),
            (
                UnsupportedLinkRequirement::Subsystem(LinkSubsystem::Windowed),
                DiagnosticKind::LinkerUnsupportedSubsystem,
            ),
            (
                UnsupportedLinkRequirement::Symbol(LinkSymbolRequirement::EntryPoint),
                DiagnosticKind::LinkerUnsupportedSymbol,
            ),
            (
                UnsupportedLinkRequirement::Startup(LinkStartupMode::ExplicitInputs),
                DiagnosticKind::LinkerUnsupportedStartup,
            ),
            (
                UnsupportedLinkRequirement::Runtime(LinkRuntimeMode::ExplicitInput),
                DiagnosticKind::LinkerUnsupportedRuntime,
            ),
        ];

        for (requirement, expected) in cases {
            let diagnostics =
                link_failure_diagnostics(&LinkFailure::UnsupportedRequirement(requirement));

            assert_eq!(diagnostics.by_kind(expected).count(), 1);
            assert_goal_state_diagnostics(&diagnostics);

            match expected {
                DiagnosticKind::LinkerUnsupportedTarget => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedTarget,
                ),
                DiagnosticKind::LinkerUnsupportedProduct => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedProduct,
                ),
                DiagnosticKind::LinkerUnsupportedInput => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedInput,
                ),
                DiagnosticKind::LinkerUnsupportedInputMode => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedInputMode,
                ),
                DiagnosticKind::LinkerUnsupportedOutput => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedOutput,
                ),
                DiagnosticKind::LinkerUnsupportedSearchPath => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedSearchPath,
                ),
                DiagnosticKind::LinkerUnsupportedLinkModel => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedLinkModel,
                ),
                DiagnosticKind::LinkerUnsupportedDeadStrip => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedDeadStrip,
                ),
                DiagnosticKind::LinkerUnsupportedSectionGarbageCollection => {
                    assert_goal_state_diagnostic_kind(
                        &diagnostics,
                        DiagnosticKind::LinkerUnsupportedSectionGarbageCollection,
                    );
                }
                DiagnosticKind::LinkerUnsupportedDebug => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedDebug,
                ),
                DiagnosticKind::LinkerUnsupportedSubsystem => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedSubsystem,
                ),
                DiagnosticKind::LinkerUnsupportedSymbol => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedSymbol,
                ),
                DiagnosticKind::LinkerUnsupportedStartup => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedStartup,
                ),
                DiagnosticKind::LinkerUnsupportedRuntime => assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::LinkerUnsupportedRuntime,
                ),
                _ => panic!("test case must remain an unsupported-link requirement"),
            }
        }
    }

    #[test]
    fn every_terminal_link_failure_publishes_its_exact_diagnostic() {
        let plan = link_plan();

        let cases = [
            (
                LinkFailure::DriverUnavailable,
                DiagnosticKind::LinkerDriverUnavailable,
            ),
            (
                LinkFailure::DriverIncompatible,
                DiagnosticKind::LinkerDriverIncompatible,
            ),
            (
                LinkFailure::MissingInput(crate::LinkInputId::new(0)),
                DiagnosticKind::LinkerInputMissing,
            ),
            (
                LinkFailure::Invocation,
                DiagnosticKind::LinkerInvocationFailed,
            ),
            (
                LinkFailure::MissingOutput(StagingDestinationId::new(0)),
                DiagnosticKind::LinkerOutputMissing,
            ),
            (
                LinkFailure::InvalidOutput(StagingDestinationId::new(0)),
                DiagnosticKind::LinkerOutputInvalid,
            ),
            (
                LinkFailure::ResourceExhausted,
                DiagnosticKind::LinkerResourceExhausted,
            ),
        ];

        for (failure, expected) in cases {
            let outcome = failed_outcome(&plan, failure);

            match expected {
                DiagnosticKind::LinkerDriverUnavailable => assert_goal_state_diagnostic_kind(
                    outcome.diagnostics(),
                    DiagnosticKind::LinkerDriverUnavailable,
                ),
                DiagnosticKind::LinkerDriverIncompatible => assert_goal_state_diagnostic_kind(
                    outcome.diagnostics(),
                    DiagnosticKind::LinkerDriverIncompatible,
                ),
                DiagnosticKind::LinkerInputMissing => assert_goal_state_diagnostic_kind(
                    outcome.diagnostics(),
                    DiagnosticKind::LinkerInputMissing,
                ),
                DiagnosticKind::LinkerInvocationFailed => assert_goal_state_diagnostic_kind(
                    outcome.diagnostics(),
                    DiagnosticKind::LinkerInvocationFailed,
                ),
                DiagnosticKind::LinkerOutputMissing => assert_goal_state_diagnostic_kind(
                    outcome.diagnostics(),
                    DiagnosticKind::LinkerOutputMissing,
                ),
                DiagnosticKind::LinkerOutputInvalid => assert_goal_state_diagnostic_kind(
                    outcome.diagnostics(),
                    DiagnosticKind::LinkerOutputInvalid,
                ),
                DiagnosticKind::LinkerResourceExhausted => assert_goal_state_diagnostic_kind(
                    outcome.diagnostics(),
                    DiagnosticKind::LinkerResourceExhausted,
                ),
                _ => panic!("test case must remain a terminal link failure"),
            }
        }
    }

    #[test]
    fn external_tool_failures_preserve_typed_operation_and_failure_context() {
        let plan = link_plan();

        let response = LinkOutcome::from_external_tool_failure(
            &plan,
            crate::ExternalToolFailure::ResponseFile {
                path: "build/link.rsp".into(),
                operation: crate::ExternalToolResponseFileOperation::Write,
                kind: std::io::ErrorKind::PermissionDenied,
            },
        );

        assert_goal_state_diagnostic_kind(
            response.diagnostics(),
            DiagnosticKind::LinkerResponseFileFailed,
        );

        let process = LinkOutcome::from_external_tool_failure(
            &plan,
            crate::ExternalToolFailure::Process(PlatformError::new(
                PlatformOperation::ProcessSpawn,
                PlatformErrorKind::Io(std::io::ErrorKind::NotFound),
            )),
        );

        assert_goal_state_diagnostic_kind(
            process.diagnostics(),
            DiagnosticKind::LinkerExternalToolIoFailed,
        );

        let pipe = LinkOutcome::from_external_tool_failure(
            &plan,
            crate::ExternalToolFailure::MissingOutputPipe(crate::ExternalToolStream::StandardError),
        );

        assert_goal_state_diagnostic_kind(
            pipe.diagnostics(),
            DiagnosticKind::LinkerExternalToolContractFailed,
        );
    }
}
