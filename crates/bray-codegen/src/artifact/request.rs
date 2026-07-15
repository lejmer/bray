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

/// Linkable contribution required by the selected product plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkableArtifactRequirement {
    /// The selected product does not require a separately linkable contribution.
    None,
    /// A relocatable native object is required.
    RelocatableObject,
    /// Backend bitcode is required for a later link operation.
    BackendBitcode,
}

impl LinkableArtifactRequirement {
    const fn artifact_kind(self) -> Option<BackendArtifactKind> {
        match self {
            Self::None => None,
            Self::RelocatableObject => Some(BackendArtifactKind::RelocatableObject),
            Self::BackendBitcode => Some(BackendArtifactKind::BackendBitcode),
        }
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

/// Validated output-affecting serialization policy shared with a backend.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendSerializationOptions {
    assembly_syntax_kind: AssemblySyntaxKind,
    annotate_backend_ir: bool,
}

impl BackendSerializationOptions {
    /// Creates typed backend serialization policy.
    pub const fn new(assembly_syntax_kind: AssemblySyntaxKind, annotate_backend_ir: bool) -> Self {
        Self {
            assembly_syntax_kind,
            annotate_backend_ir,
        }
    }

    /// Returns the requested assembly syntax.
    pub const fn assembly_syntax_kind(self) -> AssemblySyntaxKind {
        self.assembly_syntax_kind
    }

    /// Returns whether inspection IR should retain backend annotations.
    pub const fn annotate_backend_ir(self) -> bool {
        self.annotate_backend_ir
    }
}

/// Exact immutable backend outputs derived from one emission plan.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BackendArtifactRequest {
    unit: CodegenUnitKey,
    entries: Arc<[BackendArtifactRequestEntry]>,
    debug_information: DebugInformationOutputMode,
    linkable_artifact: LinkableArtifactRequirement,
    serialization: BackendSerializationOptions,
}

impl BackendArtifactRequest {
    /// Creates an artifact request after validating all requested outputs.
    pub fn try_new(
        unit: CodegenUnitKey,
        entries: impl IntoIterator<Item = BackendArtifactRequestEntry>,
        debug_information: DebugInformationOutputMode,
        linkable_artifact: LinkableArtifactRequirement,
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
    pub const fn linkable_artifact(&self) -> LinkableArtifactRequirement {
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
    /// The selected linkable contribution is not present as a required entry.
    MissingRequiredLinkableArtifact(BackendArtifactKind),
    /// Separate debug output has no required debug companion contribution.
    MissingRequiredDebugCompanion,
    /// A debug companion was requested for an embedded or omitted debug mode.
    UnexpectedDebugCompanion,
    /// A non-default assembly syntax was selected without an assembly contribution.
    UnexpectedAssemblySyntax,
    /// Backend IR annotations were selected without a backend IR contribution.
    UnexpectedBackendIrAnnotations,
}

fn validate_linkable_requirement(
    entries: &[BackendArtifactRequestEntry],
    requirement: LinkableArtifactRequirement,
) -> Result<(), BackendArtifactRequestBuildError> {
    let Some(kind) = requirement.artifact_kind() else {
        return Ok(());
    };

    if has_required_kind(entries, kind) {
        return Ok(());
    }

    Err(BackendArtifactRequestBuildError::MissingRequiredLinkableArtifact(kind))
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

    if options.annotate_backend_ir() && !has_kind(entries, BackendArtifactKind::BackendIr) {
        return Err(BackendArtifactRequestBuildError::UnexpectedBackendIrAnnotations);
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
        BackendSerializationOptions, DebugInformationOutputMode, LinkableArtifactRequirement,
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
                LinkableArtifactRequirement::BackendBitcode,
                serialization(),
            ),
            Err(
                BackendArtifactRequestBuildError::MissingRequiredLinkableArtifact(
                    BackendArtifactKind::BackendBitcode
                )
            )
        );

        let debug = entry(unit.clone(), BackendArtifactKind::DebugCompanion, 0);

        assert_eq!(
            BackendArtifactRequest::try_new(
                unit,
                [object, debug],
                DebugInformationOutputMode::Embedded,
                LinkableArtifactRequirement::RelocatableObject,
                serialization(),
            ),
            Err(BackendArtifactRequestBuildError::UnexpectedDebugCompanion)
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
        BackendSerializationOptions::new(AssemblySyntaxKind::TargetDefault, false)
    }
}
