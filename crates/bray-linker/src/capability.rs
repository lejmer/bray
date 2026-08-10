use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_target::{ObjectFormat, TargetArchitecture, TargetIdentity};

use crate::{
    DeadStripPolicy, DebugLinkPolicy, LinkInputKind, LinkInputMode, LinkModel, LinkPlan,
    LinkSearchPathKind, LinkSubsystem, LinkedArtifactKind, LinkedProductKind, LinkerDriverIdentity,
    LinkerTargetIdentity, SectionGarbageCollectionPolicy,
};

use crate::archive::ArchiveFormat;
use crate::external_tool::ResponseFileEncoding;
use crate::{LinkerDriverKind, LldFlavor, SystemLinkerFamily};

/// One plan requirement that a linker driver can declare as supported.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkPlanCapability {
    /// One native product category.
    Product(LinkedProductKind),
    /// One native input category.
    Input(LinkInputKind),
    /// One archive treatment mode.
    InputMode(LinkInputMode),
    /// One staged output category.
    Output(LinkedArtifactKind),
    /// One native search-path category.
    SearchPath(LinkSearchPathKind),
    /// One native linkage model.
    LinkModel(LinkModel),
    /// One dead-code removal policy.
    DeadStrip(DeadStripPolicy),
    /// One section garbage-collection policy.
    SectionGarbageCollection(SectionGarbageCollectionPolicy),
    /// One linked debug-information policy.
    Debug(DebugLinkPolicy),
    /// One target subsystem.
    Subsystem(LinkSubsystem),
    /// One symbol-control mechanism.
    Symbol(LinkSymbolRequirement),
    /// One startup-contract ownership mode.
    Startup(LinkStartupMode),
    /// One runtime-contract ownership mode.
    Runtime(LinkRuntimeMode),
}

/// Symbol control represented directly by the common link plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkSymbolRequirement {
    /// An explicit native entry point.
    EntryPoint,
    /// A canonical exported-symbol list.
    ExportedSymbols,
    /// A canonical retained-symbol list.
    RetainedSymbols,
}

/// Ownership of native product startup and termination support.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkStartupMode {
    /// Startup ownership does not apply to this product category.
    NotApplicable,
    /// The plan supplies explicit startup or termination inputs.
    ExplicitInputs,
    /// A platform compiler driver supplies the native startup contract.
    PlatformCompilerDriver,
}

/// Ownership of the selected Bray runtime link input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkRuntimeMode {
    /// The plan supplies the selected runtime artifact explicitly.
    ExplicitInput,
}

/// Deterministic response-file behavior of one driver.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkResponseFileCapability {
    /// The driver passes the canonical argument vector directly.
    InlineArguments,
    /// The driver can encode a deterministic UTF-8 response file.
    Utf8,
    /// The driver can encode a deterministic UTF-16 little-endian response file.
    Utf16LittleEndian,
}

/// Child-environment behavior at the invocation boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkEnvironmentCapability {
    /// No external child environment exists.
    NotApplicable,
    /// The configured complete child environment replaces ambient inheritance.
    Explicit,
}

/// Cancellation behavior published by one driver.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkCancellationCapability {
    /// An in-process driver observes the compiler cancellation contract.
    Cooperative,
    /// Cancellation is delegated to the configured external-process boundary.
    ExternalProcessBoundary,
}

/// Output determinism guarantee published by one driver.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkDeterminismCapability {
    /// Equivalent inputs produce byte-equivalent outputs.
    Reproducible,
    /// Determinism depends on the explicitly identified external toolchain.
    ToolchainDependent,
}

/// Operational behavior that does not depend on the selected target.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkerOperationalCapabilities {
    response_files: LinkResponseFileCapability,
    environment: LinkEnvironmentCapability,
    cancellation: LinkCancellationCapability,
    determinism: LinkDeterminismCapability,
}

impl LinkerOperationalCapabilities {
    /// Creates the complete driver-operation contract.
    pub const fn new(
        response_files: LinkResponseFileCapability,
        environment: LinkEnvironmentCapability,
        cancellation: LinkCancellationCapability,
        determinism: LinkDeterminismCapability,
    ) -> Self {
        Self {
            response_files,
            environment,
            cancellation,
            determinism,
        }
    }

    /// Returns the deterministic response-file behavior.
    pub const fn response_files(self) -> LinkResponseFileCapability {
        self.response_files
    }

    /// Returns the external child-environment behavior.
    pub const fn environment(self) -> LinkEnvironmentCapability {
        self.environment
    }

    /// Returns the cancellation guarantee.
    pub const fn cancellation(self) -> LinkCancellationCapability {
        self.cancellation
    }

    /// Returns the linked-output determinism guarantee.
    pub const fn determinism(self) -> LinkDeterminismCapability {
        self.determinism
    }
}

/// Complete plan support for one exact architecture and object-format pair.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkerTargetCapabilities {
    exact_target: Option<LinkerTargetIdentity>,
    architecture: TargetArchitecture,
    object_format: ObjectFormat,
    plan: Arc<[LinkPlanCapability]>,
}

impl LinkerTargetCapabilities {
    /// Creates a target contract and canonicalizes its supported plan requirements.
    pub fn new(
        architecture: TargetArchitecture,
        object_format: ObjectFormat,
        plan: impl IntoIterator<Item = LinkPlanCapability>,
    ) -> Self {
        Self {
            exact_target: None,
            architecture,
            object_format,
            plan: sorted_unique_shared_slice(plan),
        }
    }

    /// Creates a capability contract scoped to one exact target identity and triple.
    pub fn for_target(
        target: LinkerTargetIdentity,
        plan: impl IntoIterator<Item = LinkPlanCapability>,
    ) -> Self {
        Self {
            architecture: target.architecture(),
            object_format: target.object_format(),
            exact_target: Some(target),
            plan: sorted_unique_shared_slice(plan),
        }
    }

    /// Returns the exact target scope or `None` for a machine-wide contract.
    pub const fn exact_target(&self) -> Option<&LinkerTargetIdentity> {
        self.exact_target.as_ref()
    }

    /// Returns the supported processor architecture.
    pub const fn architecture(&self) -> TargetArchitecture {
        self.architecture
    }

    /// Returns the paired native object format.
    pub const fn object_format(&self) -> ObjectFormat {
        self.object_format
    }

    /// Returns supported plan requirements in canonical order.
    pub fn plan(&self) -> &[LinkPlanCapability] {
        &self.plan
    }

    fn supports(&self, capability: LinkPlanCapability) -> bool {
        self.plan.binary_search(&capability).is_ok()
    }

    fn matches(&self, target: &crate::LinkTarget) -> bool {
        self.architecture == target.architecture()
            && self.object_format == target.object_format()
            && self.exact_target.as_ref().is_none_or(|exact| {
                exact.identity() == target.identity() && exact.triple() == target.triple()
            })
    }
}

/// Complete immutable capability record published by one linker driver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkerDriverCapabilities {
    identity: LinkerDriverIdentity,
    targets: Arc<[LinkerTargetCapabilities]>,
    operational: LinkerOperationalCapabilities,
}

impl LinkerDriverCapabilities {
    /// Declares the complete capability record of an embedded or external LLD driver.
    pub fn try_for_lld(
        identity: LinkerDriverIdentity,
    ) -> Result<Self, LinkerDriverCapabilitiesBuildError> {
        lld_driver_capabilities(identity)
    }

    /// Declares the complete capability record of one configured system-linker family.
    pub fn try_for_system(
        identity: LinkerDriverIdentity,
        family: SystemLinkerFamily,
        target: LinkerTargetIdentity,
    ) -> Result<Self, LinkerDriverCapabilitiesBuildError> {
        system_driver_capabilities(identity, family, target)
    }

    /// Declares the complete capability record of the deterministic LLVM archiver.
    pub fn try_for_llvm_archive(
        identity: LinkerDriverIdentity,
    ) -> Result<Self, LinkerDriverCapabilitiesBuildError> {
        archive_driver_capabilities(identity)
    }

    /// Creates a capability record after rejecting duplicate target contracts.
    pub fn try_new(
        identity: LinkerDriverIdentity,
        targets: impl IntoIterator<Item = LinkerTargetCapabilities>,
        operational: LinkerOperationalCapabilities,
    ) -> Result<Self, LinkerDriverCapabilitiesBuildError> {
        let mut targets = targets.into_iter().collect::<Vec<_>>();

        targets.sort_unstable_by(|left, right| {
            (left.architecture, left.object_format, &left.exact_target).cmp(&(
                right.architecture,
                right.object_format,
                &right.exact_target,
            ))
        });

        if targets.is_empty() {
            return Err(LinkerDriverCapabilitiesBuildError::MissingTargets);
        }

        if targets.windows(2).any(|pair| {
            pair[0].architecture == pair[1].architecture
                && pair[0].object_format == pair[1].object_format
                && pair[0].exact_target == pair[1].exact_target
        }) {
            return Err(LinkerDriverCapabilitiesBuildError::DuplicateTarget);
        }

        Ok(Self {
            identity,
            targets: targets.into(),
            operational,
        })
    }

    /// Returns the exact driver, capability, and toolchain identity.
    pub const fn identity(&self) -> &LinkerDriverIdentity {
        &self.identity
    }

    /// Returns target contracts in canonical architecture and object-format order.
    pub fn targets(&self) -> &[LinkerTargetCapabilities] {
        &self.targets
    }

    /// Returns target-independent invocation guarantees.
    pub const fn operational(&self) -> LinkerOperationalCapabilities {
        self.operational
    }

    /// Returns whether the record supports one target and product pair.
    pub fn supports_target_product(
        &self,
        target: &crate::LinkTarget,
        product: LinkedProductKind,
    ) -> bool {
        self.target(target)
            .is_some_and(|capabilities| capabilities.supports(LinkPlanCapability::Product(product)))
    }

    /// Returns whether one target contract declares a typed plan capability.
    pub fn supports_plan_capability(
        &self,
        target: &crate::LinkTarget,
        capability: LinkPlanCapability,
    ) -> bool {
        self.target(target)
            .is_some_and(|target| target.supports(capability))
    }

    /// Validates every driver-owned requirement of one complete immutable plan.
    pub fn validate(&self, plan: &LinkPlan) -> Result<(), UnsupportedLinkRequirement> {
        let Some(target) = self.target(plan.target()) else {
            return Err(UnsupportedLinkRequirement::Target {
                // The failure can outlive the borrowed link plan.
                identity: plan.target().identity().clone(),
                triple: Arc::from(plan.target().triple()),
                architecture: plan.target().architecture(),
                object_format: plan.target().object_format(),
            });
        };

        require(
            target,
            LinkPlanCapability::Product(plan.product_kind()),
            UnsupportedLinkRequirement::Product(plan.product_kind()),
        )?;

        for input in plan.inputs() {
            require(
                target,
                LinkPlanCapability::Input(input.kind()),
                UnsupportedLinkRequirement::Input(input.kind()),
            )?;

            require(
                target,
                LinkPlanCapability::InputMode(input.mode()),
                UnsupportedLinkRequirement::InputMode(input.mode()),
            )?;
        }

        for output in plan.outputs() {
            require(
                target,
                LinkPlanCapability::Output(output.kind()),
                UnsupportedLinkRequirement::Output(output.kind()),
            )?;
        }

        for search_path in plan.search_paths() {
            require(
                target,
                LinkPlanCapability::SearchPath(search_path.kind()),
                UnsupportedLinkRequirement::SearchPath(search_path.kind()),
            )?;
        }

        require(
            target,
            LinkPlanCapability::LinkModel(plan.target().link_model()),
            UnsupportedLinkRequirement::LinkModel(plan.target().link_model()),
        )?;

        require(
            target,
            LinkPlanCapability::DeadStrip(plan.policy().dead_strip()),
            UnsupportedLinkRequirement::DeadStrip(plan.policy().dead_strip()),
        )?;

        require(
            target,
            LinkPlanCapability::SectionGarbageCollection(
                plan.policy().section_garbage_collection(),
            ),
            UnsupportedLinkRequirement::SectionGarbageCollection(
                plan.policy().section_garbage_collection(),
            ),
        )?;

        require(
            target,
            LinkPlanCapability::Debug(plan.policy().debug()),
            UnsupportedLinkRequirement::Debug(plan.policy().debug()),
        )?;

        if let Some(subsystem) = plan.policy().subsystem() {
            require(
                target,
                LinkPlanCapability::Subsystem(subsystem),
                UnsupportedLinkRequirement::Subsystem(subsystem),
            )?;
        }

        validate_symbol_requirements(target, plan)?;
        validate_startup_requirement(target, plan)?;

        validate_runtime_requirement(target, plan)
    }

    fn target(&self, target: &crate::LinkTarget) -> Option<&LinkerTargetCapabilities> {
        self.targets
            .iter()
            .find(|candidate| candidate.exact_target.is_some() && candidate.matches(target))
            .or_else(|| {
                self.targets
                    .iter()
                    .find(|candidate| candidate.exact_target.is_none() && candidate.matches(target))
            })
    }
}

pub(crate) fn lld_driver_capabilities(
    identity: LinkerDriverIdentity,
) -> Result<LinkerDriverCapabilities, LinkerDriverCapabilitiesBuildError> {
    let operational = match identity.kind() {
        LinkerDriverKind::EmbeddedLld => LinkerOperationalCapabilities::new(
            LinkResponseFileCapability::InlineArguments,
            LinkEnvironmentCapability::NotApplicable,
            LinkCancellationCapability::Cooperative,
            LinkDeterminismCapability::ToolchainDependent,
        ),
        LinkerDriverKind::ExternalLld => LinkerOperationalCapabilities::new(
            LinkResponseFileCapability::InlineArguments,
            LinkEnvironmentCapability::Explicit,
            LinkCancellationCapability::ExternalProcessBoundary,
            LinkDeterminismCapability::ToolchainDependent,
        ),
        LinkerDriverKind::System
        | LinkerDriverKind::Archiver
        | LinkerDriverKind::TargetSpecific => {
            return Err(LinkerDriverCapabilitiesBuildError::DriverKindMismatch);
        }
    };

    LinkerDriverCapabilities::try_new(
        identity,
        linked_target_capabilities(LinkStartupMode::ExplicitInputs, true),
        operational,
    )
}

pub(crate) fn system_driver_capabilities(
    identity: LinkerDriverIdentity,
    family: SystemLinkerFamily,
    target: LinkerTargetIdentity,
) -> Result<LinkerDriverCapabilities, LinkerDriverCapabilitiesBuildError> {
    if identity.kind() != LinkerDriverKind::System {
        return Err(LinkerDriverCapabilitiesBuildError::DriverKindMismatch);
    }

    if !LldFlavor::TARGETS.iter().any(|candidate| {
        candidate.0 == target.architecture()
            && candidate.1 == target.object_format()
            && candidate.2 == family.flavor()
    }) {
        return Err(LinkerDriverCapabilitiesBuildError::UnsupportedTarget);
    }

    let response_files = match family.response_file_encoding() {
        Some(ResponseFileEncoding::Utf8) => LinkResponseFileCapability::Utf8,
        Some(ResponseFileEncoding::Utf16LittleEndian) => {
            LinkResponseFileCapability::Utf16LittleEndian
        }
        None => LinkResponseFileCapability::InlineArguments,
    };

    let startup = if family.supplies_platform_startup() {
        LinkStartupMode::PlatformCompilerDriver
    } else {
        LinkStartupMode::ExplicitInputs
    };

    LinkerDriverCapabilities::try_new(
        identity,
        [LinkerTargetCapabilities::for_target(
            target,
            linked_plan_capabilities(family.flavor().object_format(), startup, false),
        )],
        LinkerOperationalCapabilities::new(
            response_files,
            LinkEnvironmentCapability::Explicit,
            LinkCancellationCapability::ExternalProcessBoundary,
            LinkDeterminismCapability::ToolchainDependent,
        ),
    )
}

pub(crate) fn archive_driver_capabilities(
    identity: LinkerDriverIdentity,
) -> Result<LinkerDriverCapabilities, LinkerDriverCapabilitiesBuildError> {
    if identity.kind() != LinkerDriverKind::Archiver {
        return Err(LinkerDriverCapabilitiesBuildError::DriverKindMismatch);
    }

    let requirements = [
        LinkPlanCapability::Product(LinkedProductKind::StaticLibrary),
        LinkPlanCapability::Input(LinkInputKind::RelocatableObject),
        LinkPlanCapability::InputMode(LinkInputMode::Ordinary),
        LinkPlanCapability::Output(LinkedArtifactKind::StaticLibrary),
        LinkPlanCapability::LinkModel(LinkModel::Default),
        LinkPlanCapability::LinkModel(LinkModel::Static),
        LinkPlanCapability::LinkModel(LinkModel::Dynamic),
        LinkPlanCapability::DeadStrip(DeadStripPolicy::Preserve),
        LinkPlanCapability::SectionGarbageCollection(SectionGarbageCollectionPolicy::Preserve),
        LinkPlanCapability::Debug(DebugLinkPolicy::None),
        LinkPlanCapability::Debug(DebugLinkPolicy::Embedded),
        LinkPlanCapability::Startup(LinkStartupMode::NotApplicable),
    ];

    let targets = ArchiveFormat::TARGETS
        .into_iter()
        .map(|(architecture, object_format, _)| {
            LinkerTargetCapabilities::new(architecture, object_format, requirements)
        });

    LinkerDriverCapabilities::try_new(
        identity,
        targets,
        LinkerOperationalCapabilities::new(
            LinkResponseFileCapability::Utf8,
            LinkEnvironmentCapability::Explicit,
            LinkCancellationCapability::ExternalProcessBoundary,
            LinkDeterminismCapability::Reproducible,
        ),
    )
}

fn linked_target_capabilities(
    startup: LinkStartupMode,
    accepts_bitcode: bool,
) -> impl Iterator<Item = LinkerTargetCapabilities> {
    LldFlavor::TARGETS
        .into_iter()
        .map(move |(architecture, object_format, _)| {
            LinkerTargetCapabilities::new(
                architecture,
                object_format,
                linked_plan_capabilities(object_format, startup, accepts_bitcode),
            )
        })
}

fn linked_plan_capabilities(
    object_format: ObjectFormat,
    startup: LinkStartupMode,
    accepts_bitcode: bool,
) -> Vec<LinkPlanCapability> {
    let mut capabilities = vec![
        LinkPlanCapability::Product(LinkedProductKind::Executable),
        LinkPlanCapability::Product(LinkedProductKind::SharedLibrary),
        LinkPlanCapability::Input(LinkInputKind::RelocatableObject),
        LinkPlanCapability::Input(LinkInputKind::Archive),
        LinkPlanCapability::Input(LinkInputKind::StartupObject),
        LinkPlanCapability::Input(LinkInputKind::TerminationObject),
        LinkPlanCapability::Input(LinkInputKind::RuntimeComponent),
        LinkPlanCapability::Input(LinkInputKind::NativeLibrary),
        LinkPlanCapability::InputMode(LinkInputMode::Ordinary),
        LinkPlanCapability::InputMode(LinkInputMode::WholeArchive),
        LinkPlanCapability::Output(LinkedArtifactKind::Executable),
        LinkPlanCapability::Output(LinkedArtifactKind::SharedLibrary),
        LinkPlanCapability::SearchPath(LinkSearchPathKind::Library),
        LinkPlanCapability::LinkModel(LinkModel::Default),
        LinkPlanCapability::LinkModel(LinkModel::Dynamic),
        LinkPlanCapability::DeadStrip(DeadStripPolicy::Preserve),
        LinkPlanCapability::DeadStrip(DeadStripPolicy::RemoveUnreachable),
        LinkPlanCapability::SectionGarbageCollection(SectionGarbageCollectionPolicy::Preserve),
        LinkPlanCapability::SectionGarbageCollection(
            SectionGarbageCollectionPolicy::RemoveUnreferenced,
        ),
        LinkPlanCapability::Subsystem(LinkSubsystem::Console),
        LinkPlanCapability::Symbol(LinkSymbolRequirement::EntryPoint),
        LinkPlanCapability::Symbol(LinkSymbolRequirement::ExportedSymbols),
        LinkPlanCapability::Symbol(LinkSymbolRequirement::RetainedSymbols),
        LinkPlanCapability::Startup(startup),
        LinkPlanCapability::Runtime(LinkRuntimeMode::ExplicitInput),
    ];

    if accepts_bitcode {
        capabilities.push(LinkPlanCapability::Input(LinkInputKind::Bitcode));
    }

    match object_format {
        ObjectFormat::Elf => {
            capabilities.push(LinkPlanCapability::LinkModel(LinkModel::Static));
            capabilities.push(LinkPlanCapability::Debug(DebugLinkPolicy::None));
            capabilities.push(LinkPlanCapability::Debug(DebugLinkPolicy::Embedded));
        }
        ObjectFormat::Coff => {
            capabilities.push(LinkPlanCapability::LinkModel(LinkModel::Static));

            capabilities.push(LinkPlanCapability::Output(
                LinkedArtifactKind::ImportLibrary,
            ));

            capabilities.push(LinkPlanCapability::Output(
                LinkedArtifactKind::DebugCompanion,
            ));

            capabilities.push(LinkPlanCapability::Debug(DebugLinkPolicy::None));
            capabilities.push(LinkPlanCapability::Debug(DebugLinkPolicy::Companion));
            capabilities.push(LinkPlanCapability::Subsystem(LinkSubsystem::Windowed));
            capabilities.push(LinkPlanCapability::Subsystem(LinkSubsystem::Native));
        }
        ObjectFormat::MachO => {
            capabilities.push(LinkPlanCapability::Input(LinkInputKind::Framework));

            capabilities.push(LinkPlanCapability::SearchPath(
                LinkSearchPathKind::Framework,
            ));

            capabilities.push(LinkPlanCapability::Debug(DebugLinkPolicy::None));
            capabilities.push(LinkPlanCapability::Debug(DebugLinkPolicy::Embedded));
        }
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {}
    }

    capabilities
}

/// A contract violation that prevents capability-record construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkerDriverCapabilitiesBuildError {
    /// The driver category cannot publish the requested capability family.
    DriverKindMismatch,
    /// The configured target is incompatible with the driver command family.
    UnsupportedTarget,
    /// No supported target contract was declared.
    MissingTargets,
    /// One architecture and object-format pair was declared more than once.
    DuplicateTarget,
}

/// One typed plan requirement rejected before linker invocation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum UnsupportedLinkRequirement {
    /// The exact target identity and machine pair is unsupported.
    Target {
        /// Exact target-profile identity.
        identity: TargetIdentity,
        /// Canonical target triple.
        triple: Arc<str>,
        /// Requested processor architecture.
        architecture: TargetArchitecture,
        /// Requested native object format.
        object_format: ObjectFormat,
    },
    /// The native product category is unsupported.
    Product(LinkedProductKind),
    /// One native input category is unsupported.
    Input(LinkInputKind),
    /// One archive treatment mode is unsupported.
    InputMode(LinkInputMode),
    /// One staged output category is unsupported.
    Output(LinkedArtifactKind),
    /// One native search-path category is unsupported.
    SearchPath(LinkSearchPathKind),
    /// The native linkage model is unsupported.
    LinkModel(LinkModel),
    /// The dead-code removal policy is unsupported.
    DeadStrip(DeadStripPolicy),
    /// The section garbage-collection policy is unsupported.
    SectionGarbageCollection(SectionGarbageCollectionPolicy),
    /// The linked debug-information policy is unsupported.
    Debug(DebugLinkPolicy),
    /// The target subsystem is unsupported.
    Subsystem(LinkSubsystem),
    /// One symbol-control mechanism is unsupported.
    Symbol(LinkSymbolRequirement),
    /// The startup-contract ownership mode is unsupported.
    Startup(LinkStartupMode),
    /// The runtime-contract ownership mode is unsupported.
    Runtime(LinkRuntimeMode),
}

fn require(
    target: &LinkerTargetCapabilities,
    capability: LinkPlanCapability,
    unsupported: UnsupportedLinkRequirement,
) -> Result<(), UnsupportedLinkRequirement> {
    if !target.supports(capability) {
        return Err(unsupported);
    }

    Ok(())
}

fn validate_symbol_requirements(
    target: &LinkerTargetCapabilities,
    plan: &LinkPlan,
) -> Result<(), UnsupportedLinkRequirement> {
    if plan.entry_point().is_some() {
        require_symbol(target, LinkSymbolRequirement::EntryPoint)?;
    }

    if !plan.exported_symbols().is_empty() {
        require_symbol(target, LinkSymbolRequirement::ExportedSymbols)?;
    }

    if !plan.retained_symbols().is_empty() {
        require_symbol(target, LinkSymbolRequirement::RetainedSymbols)?;
    }

    Ok(())
}

fn require_symbol(
    target: &LinkerTargetCapabilities,
    requirement: LinkSymbolRequirement,
) -> Result<(), UnsupportedLinkRequirement> {
    require(
        target,
        LinkPlanCapability::Symbol(requirement),
        UnsupportedLinkRequirement::Symbol(requirement),
    )
}

fn validate_startup_requirement(
    target: &LinkerTargetCapabilities,
    plan: &LinkPlan,
) -> Result<(), UnsupportedLinkRequirement> {
    let mode = plan.startup_mode();

    require(
        target,
        LinkPlanCapability::Startup(mode),
        UnsupportedLinkRequirement::Startup(mode),
    )
}

fn validate_runtime_requirement(
    target: &LinkerTargetCapabilities,
    plan: &LinkPlan,
) -> Result<(), UnsupportedLinkRequirement> {
    if !plan
        .inputs()
        .iter()
        .any(|input| input.kind() == LinkInputKind::RuntimeComponent)
    {
        return Ok(());
    }

    require(
        target,
        LinkPlanCapability::Runtime(LinkRuntimeMode::ExplicitInput),
        UnsupportedLinkRequirement::Runtime(LinkRuntimeMode::ExplicitInput),
    )
}
