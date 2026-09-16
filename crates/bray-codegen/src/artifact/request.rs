use std::sync::Arc;

use bray_diagnostics::DiagnosticArtifactKind;

use crate::CodegenUnitKey;

/// Backend artifact categories understood by emission planning.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BackendArtifactKind {
    /// Relocatable native object suitable for a native linker.
    RelocatableObject,
    /// Human-readable target assembly.
    Assembly,
    /// Human-readable backend low-level IR.
    BackendIr,
    /// Backend-owned binary IR or bitcode.
    BackendBitcode,
    /// Directly executable module that does not require native linking.
    ExecutableModule,
    /// Codegen-owned debug data stored separately from another artifact.
    DebugCompanion,
}

impl BackendArtifactKind {
    /// Returns the locale-neutral diagnostic artifact category.
    pub const fn diagnostic_kind(self) -> DiagnosticArtifactKind {
        match self {
            Self::RelocatableObject => DiagnosticArtifactKind::RelocatableObject,
            Self::Assembly => DiagnosticArtifactKind::Assembly,
            Self::BackendIr => DiagnosticArtifactKind::BackendIr,
            Self::BackendBitcode => DiagnosticArtifactKind::BackendBitcode,
            Self::ExecutableModule => DiagnosticArtifactKind::ExecutableModule,
            Self::DebugCompanion => DiagnosticArtifactKind::DebugCompanion,
        }
    }

    /// Returns the stable machine-readable artifact category.
    pub const fn as_str(self) -> &'static str {
        self.diagnostic_kind().as_str()
    }
}

/// Stable logical identity of one contribution in an immutable emission plan.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendArtifactId {
    unit: CodegenUnitKey,
    kind: BackendArtifactKind,
    ordinal: u32,
}

impl BackendArtifactId {
    /// Creates a logical artifact identity from structural unit, kind, and plan ordinal.
    pub const fn new(unit: CodegenUnitKey, kind: BackendArtifactKind, ordinal: u32) -> Self {
        Self {
            unit,
            kind,
            ordinal,
        }
    }

    /// Returns the codegen unit expected to produce this contribution.
    pub const fn unit(&self) -> &CodegenUnitKey {
        &self.unit
    }

    /// Returns the planned artifact category.
    pub const fn kind(&self) -> BackendArtifactKind {
        self.kind
    }

    /// Returns the stable order among same-kind contributions from this unit.
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }
}

/// Whether one planned backend contribution is mandatory.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BackendArtifactRequirement {
    /// Generation cannot succeed without this contribution.
    Required,
    /// The backend may omit this contribution when it cannot produce it.
    Optional,
}

/// One logically identified contribution in an emitter-derived backend request.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendArtifactRequestEntry {
    id: BackendArtifactId,
    requirement: BackendArtifactRequirement,
}

impl BackendArtifactRequestEntry {
    /// Creates one planned contribution entry.
    pub const fn new(id: BackendArtifactId, requirement: BackendArtifactRequirement) -> Self {
        Self { id, requirement }
    }

    /// Returns the planned logical contribution identity.
    pub const fn id(&self) -> &BackendArtifactId {
        &self.id
    }

    /// Returns whether the contribution is mandatory.
    pub const fn requirement(&self) -> BackendArtifactRequirement {
        self.requirement
    }
}

/// Placement of requested debug information in backend outputs.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DebugInformationOutputMode {
    /// Do not serialize debug information.
    Omit,
    /// Embed debug information in another generated artifact.
    Embedded,
    /// Serialize debug information as a separate companion contribution.
    Separate,
}

/// Backend contribution category that can feed a later link operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkableArtifactKind {
    /// Relocatable native object.
    RelocatableObject,
    /// Backend bitcode consumed by a later link operation.
    BackendBitcode,
}

impl LinkableArtifactKind {
    /// Returns the corresponding backend artifact category.
    pub const fn artifact_kind(self) -> BackendArtifactKind {
        match self {
            Self::RelocatableObject => BackendArtifactKind::RelocatableObject,
            Self::BackendBitcode => BackendArtifactKind::BackendBitcode,
        }
    }
}

/// Exact linkable contribution needed by one selected product plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkableArtifactRequirement {
    kind: LinkableArtifactKind,
    requirement: BackendArtifactRequirement,
}

impl LinkableArtifactRequirement {
    /// Creates one typed linkable contribution requirement.
    pub const fn new(kind: LinkableArtifactKind, requirement: BackendArtifactRequirement) -> Self {
        Self { kind, requirement }
    }

    /// Returns the selected linkable contribution category.
    pub const fn kind(self) -> LinkableArtifactKind {
        self.kind
    }

    /// Returns whether the linkable contribution is mandatory.
    pub const fn requirement(self) -> BackendArtifactRequirement {
        self.requirement
    }

    /// Returns the corresponding backend artifact category.
    pub const fn artifact_kind(self) -> BackendArtifactKind {
        self.kind.artifact_kind()
    }
}

/// Requested syntax kind for human-readable assembly contributions.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AssemblySyntaxKind {
    /// Use the target's canonical assembly syntax.
    TargetDefault,
    /// Use Intel assembly syntax where the target supports it.
    Intel,
    /// Use AT&T assembly syntax where the target supports it.
    Att,
}

/// Native optimization contract carried by serialized backend bitcode.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BackendBitcodeSemantics {
    /// Preserve backend bitcode without a cross-artifact import summary.
    #[default]
    Plain,
    /// Include the module summary required for thin link-time optimization planning.
    ThinLto,
}

/// Validated output-affecting serialization policy shared with a backend.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendSerializationOptions {
    assembly_syntax_kind: AssemblySyntaxKind,
    bitcode_semantics: BackendBitcodeSemantics,
}

impl BackendSerializationOptions {
    /// Creates typed backend serialization policy.
    pub const fn new(assembly_syntax_kind: AssemblySyntaxKind) -> Self {
        Self {
            assembly_syntax_kind,
            bitcode_semantics: BackendBitcodeSemantics::Plain,
        }
    }

    /// Returns this policy with the selected backend-bitcode contract.
    pub const fn with_bitcode_semantics(
        mut self,
        bitcode_semantics: BackendBitcodeSemantics,
    ) -> Self {
        self.bitcode_semantics = bitcode_semantics;

        self
    }

    /// Returns the requested assembly syntax.
    pub const fn assembly_syntax_kind(self) -> AssemblySyntaxKind {
        self.assembly_syntax_kind
    }

    /// Returns the requested backend-bitcode contract.
    pub const fn bitcode_semantics(self) -> BackendBitcodeSemantics {
        self.bitcode_semantics
    }
}

/// Exact immutable backend outputs derived from one emission plan.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendArtifactRequest {
    unit: CodegenUnitKey,
    entries: Arc<[BackendArtifactRequestEntry]>,
    debug_information: DebugInformationOutputMode,
    linkable_artifact: Option<LinkableArtifactRequirement>,
    serialization: BackendSerializationOptions,
}

impl BackendArtifactRequest {
    /// Creates an artifact request from emitter-planned outputs.
    pub fn new(
        unit: CodegenUnitKey,
        entries: impl IntoIterator<Item = BackendArtifactRequestEntry>,
        debug_information: DebugInformationOutputMode,
        linkable_artifact: Option<LinkableArtifactRequirement>,
        serialization: BackendSerializationOptions,
    ) -> Self {
        let mut entries: Vec<_> = entries.into_iter().collect();

        entries.sort_unstable_by(|left, right| left.id().cmp(right.id()));

        Self {
            unit,
            entries: entries.into(),
            debug_information,
            linkable_artifact,
            serialization,
        }
    }

    /// Returns the codegen unit covered by this exact request.
    pub const fn unit(&self) -> &CodegenUnitKey {
        &self.unit
    }

    /// Returns planned contributions in canonical logical-identity order.
    pub fn entries(&self) -> &[BackendArtifactRequestEntry] {
        &self.entries
    }

    /// Returns the planned debug-information placement.
    pub const fn debug_information(&self) -> DebugInformationOutputMode {
        self.debug_information
    }

    /// Returns the selected linkable contribution requirement.
    pub const fn linkable_artifact(&self) -> Option<LinkableArtifactRequirement> {
        self.linkable_artifact
    }

    /// Returns validated output-affecting serialization policy.
    pub const fn serialization(&self) -> BackendSerializationOptions {
        self.serialization
    }
}
