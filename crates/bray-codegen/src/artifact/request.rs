use std::sync::Arc;

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
    /// Include the module summary required for LLVM ThinLTO planning.
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
    /// Creates an artifact request after validating all requested outputs.
    pub fn try_new(
        unit: CodegenUnitKey,
        entries: impl IntoIterator<Item = BackendArtifactRequestEntry>,
        debug_information: DebugInformationOutputMode,
        linkable_artifact: Option<LinkableArtifactRequirement>,
        serialization: BackendSerializationOptions,
    ) -> Result<Self, BackendArtifactRequestBuildError> {
        let mut entries: Vec<_> = entries.into_iter().collect();

        entries.sort_unstable_by(|left, right| left.id().cmp(right.id()));

        if entries.is_empty() {
            return Err(BackendArtifactRequestBuildError::Empty);
        }

        if let Some(entry) = entries.iter().find(|entry| entry.id().unit() != &unit) {
            // Validation errors retain the Arc-backed planned identity after this borrow ends.
            return Err(BackendArtifactRequestBuildError::ForeignUnit(
                entry.id().clone(),
            ));
        }

        if let Some(pair) = entries.windows(2).find(|pair| pair[0].id() == pair[1].id()) {
            // Validation errors retain the Arc-backed planned identity after this borrow ends.
            return Err(BackendArtifactRequestBuildError::DuplicateIdentity(
                pair[0].id().clone(),
            ));
        }

        validate_linkable_requirement(&entries, linkable_artifact)?;
        validate_debug_output(&entries, debug_information)?;
        validate_serialization_options(&entries, serialization)?;

        Ok(Self {
            unit,
            entries: entries.into(),
            debug_information,
            linkable_artifact,
            serialization,
        })
    }

    /// Returns the codegen unit covered by this exact request.
    pub const fn unit(&self) -> &CodegenUnitKey {
        &self.unit
    }

    /// Returns planned contributions in canonical logical-identity order.
    pub fn entries(&self) -> &[BackendArtifactRequestEntry] {
        &self.entries
    }

    /// Returns the planned contribution entry for one logical identity.
    pub fn entry(&self, id: &BackendArtifactId) -> Option<&BackendArtifactRequestEntry> {
        self.entries
            .binary_search_by(|entry| entry.id().cmp(id))
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Returns required planned contributions in canonical order.
    pub fn required(&self) -> impl Iterator<Item = &BackendArtifactRequestEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.requirement() == BackendArtifactRequirement::Required)
    }

    /// Returns optional planned contributions in canonical order.
    pub fn optional(&self) -> impl Iterator<Item = &BackendArtifactRequestEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.requirement() == BackendArtifactRequirement::Optional)
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

/// A contract violation that prevents creation of an artifact request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendArtifactRequestBuildError {
    /// No output contribution was requested.
    Empty,
    /// One logical contribution identity belongs to another codegen unit.
    ForeignUnit(BackendArtifactId),
    /// One logical contribution identity appears more than once.
    DuplicateIdentity(BackendArtifactId),
    /// The selected linkable contribution is not present with sufficient requirement strength.
    MissingLinkableArtifact {
        /// Missing backend artifact category.
        kind: BackendArtifactKind,
        /// Required contribution strength.
        requirement: BackendArtifactRequirement,
    },
    /// Separate debug output has no required debug companion contribution.
    MissingRequiredDebugCompanion,
    /// A debug companion was requested for an embedded or omitted debug mode.
    UnexpectedDebugCompanion,
    /// A non-default assembly syntax was selected without an assembly contribution.
    UnexpectedAssemblySyntax,
    /// Specialized bitcode semantics were selected without a backend-bitcode contribution.
    UnexpectedBitcodeSemantics,
}

fn validate_linkable_requirement(
    entries: &[BackendArtifactRequestEntry],
    requirement: Option<LinkableArtifactRequirement>,
) -> Result<(), BackendArtifactRequestBuildError> {
    let Some(requirement) = requirement else {
        return Ok(());
    };

    let kind = requirement.artifact_kind();

    if entries.iter().any(|entry| {
        entry.id().kind() == kind
            && (requirement.requirement() == BackendArtifactRequirement::Optional
                || entry.requirement() == BackendArtifactRequirement::Required)
    }) {
        return Ok(());
    }

    Err(BackendArtifactRequestBuildError::MissingLinkableArtifact {
        kind,
        requirement: requirement.requirement(),
    })
}

fn validate_debug_output(
    entries: &[BackendArtifactRequestEntry],
    output: DebugInformationOutputMode,
) -> Result<(), BackendArtifactRequestBuildError> {
    let has_companion = entries
        .iter()
        .any(|entry| entry.id().kind() == BackendArtifactKind::DebugCompanion);

    match output {
        DebugInformationOutputMode::Separate
            if !has_required_kind(entries, BackendArtifactKind::DebugCompanion) =>
        {
            Err(BackendArtifactRequestBuildError::MissingRequiredDebugCompanion)
        }
        DebugInformationOutputMode::Embedded | DebugInformationOutputMode::Omit
            if has_companion =>
        {
            Err(BackendArtifactRequestBuildError::UnexpectedDebugCompanion)
        }
        _ => Ok(()),
    }
}

fn has_required_kind(entries: &[BackendArtifactRequestEntry], kind: BackendArtifactKind) -> bool {
    entries.iter().any(|entry| {
        entry.id().kind() == kind && entry.requirement() == BackendArtifactRequirement::Required
    })
}

fn validate_serialization_options(
    entries: &[BackendArtifactRequestEntry],
    options: BackendSerializationOptions,
) -> Result<(), BackendArtifactRequestBuildError> {
    if options.assembly_syntax_kind() != AssemblySyntaxKind::TargetDefault
        && !has_kind(entries, BackendArtifactKind::Assembly)
    {
        return Err(BackendArtifactRequestBuildError::UnexpectedAssemblySyntax);
    }

    if options.bitcode_semantics() != BackendBitcodeSemantics::Plain
        && !has_kind(entries, BackendArtifactKind::BackendBitcode)
    {
        return Err(BackendArtifactRequestBuildError::UnexpectedBitcodeSemantics);
    }

    Ok(())
}

fn has_kind(entries: &[BackendArtifactRequestEntry], kind: BackendArtifactKind) -> bool {
    entries.iter().any(|entry| entry.id().kind() == kind)
}

#[cfg(test)]
mod tests {
    use super::{
        AssemblySyntaxKind, BackendArtifactId, BackendArtifactKind, BackendArtifactRequest,
        BackendArtifactRequestBuildError, BackendArtifactRequestEntry, BackendArtifactRequirement,
        BackendBitcodeSemantics, BackendSerializationOptions, DebugInformationOutputMode,
        LinkableArtifactKind, LinkableArtifactRequirement,
    };
    use crate::test_support::codegen_unit_key;

    #[test]
    fn requests_validate_linkable_debug_and_logical_identity_contracts() {
        let unit = codegen_unit_key(1);
        let object = entry(unit.clone(), BackendArtifactKind::RelocatableObject, 0);

        assert_eq!(
            BackendArtifactRequest::try_new(
                unit.clone(),
                [object.clone()],
                DebugInformationOutputMode::Omit,
                Some(LinkableArtifactRequirement::new(
                    LinkableArtifactKind::BackendBitcode,
                    BackendArtifactRequirement::Required,
                )),
                serialization(),
            ),
            Err(BackendArtifactRequestBuildError::MissingLinkableArtifact {
                kind: BackendArtifactKind::BackendBitcode,
                requirement: BackendArtifactRequirement::Required,
            })
        );

        let debug = entry(unit.clone(), BackendArtifactKind::DebugCompanion, 0);

        assert_eq!(
            BackendArtifactRequest::try_new(
                unit,
                [object, debug],
                DebugInformationOutputMode::Embedded,
                Some(LinkableArtifactRequirement::new(
                    LinkableArtifactKind::RelocatableObject,
                    BackendArtifactRequirement::Required,
                )),
                serialization(),
            ),
            Err(BackendArtifactRequestBuildError::UnexpectedDebugCompanion)
        );
    }

    #[test]
    fn specialized_bitcode_semantics_require_bitcode_output() {
        let unit = codegen_unit_key(1);
        let object = entry(unit.clone(), BackendArtifactKind::RelocatableObject, 0);

        assert_eq!(
            BackendArtifactRequest::try_new(
                unit,
                [object],
                DebugInformationOutputMode::Omit,
                Some(LinkableArtifactRequirement::new(
                    LinkableArtifactKind::RelocatableObject,
                    BackendArtifactRequirement::Required,
                )),
                serialization().with_bitcode_semantics(BackendBitcodeSemantics::ThinLto),
            ),
            Err(BackendArtifactRequestBuildError::UnexpectedBitcodeSemantics)
        );
    }

    fn entry(
        unit: crate::CodegenUnitKey,
        kind: BackendArtifactKind,
        ordinal: u32,
    ) -> BackendArtifactRequestEntry {
        BackendArtifactRequestEntry::new(
            BackendArtifactId::new(unit, kind, ordinal),
            BackendArtifactRequirement::Required,
        )
    }

    fn serialization() -> BackendSerializationOptions {
        BackendSerializationOptions::new(AssemblySyntaxKind::TargetDefault)
    }
}
