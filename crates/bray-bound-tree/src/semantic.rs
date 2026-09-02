use super::{
    BodyBehaviorContributions, BoundUnitId, BoundUnitKind, CheckedAsync,
    CheckedDependencyContracts, CheckedExpressionTypes, CheckedLiteralValues, CheckedRefinements,
    CheckedSemanticSelections, Liveness, StorageFlow,
};

/// Identifies one semantic result whose unit identity is validated for a snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticSnapshotInputKind {
    /// Checked semantic selections.
    Selections,
    /// Checked source-literal values.
    Literals,
    /// Checked flow-sensitive refinements.
    Refinements,
    /// Checked storage flow.
    StorageFlow,
    /// Checked dependency contracts.
    Dependencies,
    /// Checked asynchronous behavior.
    Asynchronous,
    /// Direct body-behavior contributions.
    Behavior,
}

impl SemanticSnapshotInputKind {
    /// Returns the stable machine-readable name of this semantic result.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Selections => "selections",
            Self::Literals => "literals",
            Self::Refinements => "refinements",
            Self::StorageFlow => "storage_flow",
            Self::Dependencies => "dependencies",
            Self::Asynchronous => "asynchronous",
            Self::Behavior => "behavior",
        }
    }
}

/// An exact unit-identity mismatch that prevents construction of one semantic snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SemanticSnapshotBuildError {
    input: SemanticSnapshotInputKind,
    expected_unit: BoundUnitId,
    expected_kind: BoundUnitKind,
    actual_unit: BoundUnitId,
    actual_kind: BoundUnitKind,
}

impl SemanticSnapshotBuildError {
    /// Returns the semantic result whose identity did not match.
    pub const fn input(self) -> SemanticSnapshotInputKind {
        self.input
    }

    /// Returns the expected bound unit identity.
    pub const fn expected_unit(self) -> BoundUnitId {
        self.expected_unit
    }

    /// Returns the expected bound unit category.
    pub const fn expected_kind(self) -> BoundUnitKind {
        self.expected_kind
    }

    /// Returns the supplied bound unit identity.
    pub const fn actual_unit(self) -> BoundUnitId {
        self.actual_unit
    }

    /// Returns the supplied bound unit category.
    pub const fn actual_kind(self) -> BoundUnitKind {
        self.actual_kind
    }
}

/// Expression semantics established together for one bound semantic unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedExpressionSemantics {
    types: CheckedExpressionTypes,
    selections: CheckedSemanticSelections,
    literals: CheckedLiteralValues,
}

impl CheckedExpressionSemantics {
    /// Creates a snapshot when every result describes the same unit and unit kind.
    pub fn try_new(
        types: CheckedExpressionTypes,
        selections: CheckedSemanticSelections,
        literals: CheckedLiteralValues,
    ) -> Result<Self, SemanticSnapshotBuildError> {
        require_semantic_identity(
            types.unit(),
            types.kind(),
            selections.unit(),
            selections.kind(),
            SemanticSnapshotInputKind::Selections,
        )?;

        require_semantic_identity(
            types.unit(),
            types.kind(),
            literals.unit(),
            literals.kind(),
            SemanticSnapshotInputKind::Literals,
        )?;

        Ok(Self {
            types,
            selections,
            literals,
        })
    }

    /// Returns the exact bound unit described by this snapshot.
    pub const fn unit(&self) -> BoundUnitId {
        self.types.unit()
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.types.kind()
    }

    /// Returns the complete expression type table.
    pub const fn types(&self) -> &CheckedExpressionTypes {
        &self.types
    }

    /// Returns the complete semantic selection table.
    pub const fn selections(&self) -> &CheckedSemanticSelections {
        &self.selections
    }

    /// Returns the complete source-literal value table.
    pub const fn literals(&self) -> &CheckedLiteralValues {
        &self.literals
    }
}

/// Correlated flow and behavior semantics established for one checked body.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedBodySemantics {
    liveness: Liveness,
    refinements: CheckedRefinements,
    storage_flow: StorageFlow,
    dependencies: CheckedDependencyContracts,
    asynchronous: CheckedAsync,
    behavior: BodyBehaviorContributions,
}

impl CheckedBodySemantics {
    /// Creates a snapshot when every result describes the same unit and unit kind.
    pub fn try_new(
        liveness: Liveness,
        refinements: CheckedRefinements,
        storage_flow: StorageFlow,
        dependencies: CheckedDependencyContracts,
        asynchronous: CheckedAsync,
        behavior: BodyBehaviorContributions,
    ) -> Result<Self, SemanticSnapshotBuildError> {
        let unit = liveness.unit();
        let kind = liveness.kind();

        require_semantic_identity(
            unit,
            kind,
            refinements.unit(),
            refinements.kind(),
            SemanticSnapshotInputKind::Refinements,
        )?;

        require_semantic_identity(
            unit,
            kind,
            storage_flow.unit(),
            storage_flow.kind(),
            SemanticSnapshotInputKind::StorageFlow,
        )?;

        require_semantic_identity(
            unit,
            kind,
            dependencies.unit(),
            dependencies.kind(),
            SemanticSnapshotInputKind::Dependencies,
        )?;

        require_semantic_identity(
            unit,
            kind,
            asynchronous.unit(),
            asynchronous.kind(),
            SemanticSnapshotInputKind::Asynchronous,
        )?;

        require_semantic_identity(
            unit,
            kind,
            behavior.unit(),
            behavior.kind(),
            SemanticSnapshotInputKind::Behavior,
        )?;

        Ok(Self {
            liveness,
            refinements,
            storage_flow,
            dependencies,
            asynchronous,
            behavior,
        })
    }

    /// Returns the exact bound unit described by this snapshot.
    pub const fn unit(&self) -> BoundUnitId {
        self.liveness.unit()
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.liveness.kind()
    }

    /// Returns durable last-use and lexical scope-boundary decisions.
    pub const fn liveness(&self) -> &Liveness {
        &self.liveness
    }

    /// Returns flow-sensitive refinements at checked operation occurrences.
    pub const fn refinements(&self) -> &CheckedRefinements {
        &self.refinements
    }

    /// Returns checked storage, ownership, movement, and borrow decisions.
    pub const fn storage_flow(&self) -> &StorageFlow {
        &self.storage_flow
    }

    /// Returns normalized dependency contracts.
    pub const fn dependencies(&self) -> &CheckedDependencyContracts {
        &self.dependencies
    }

    /// Returns async frame, suspension, task, and cleanup analysis.
    pub const fn asynchronous(&self) -> &CheckedAsync {
        &self.asynchronous
    }

    /// Returns direct body-behavior contributions.
    pub const fn behavior(&self) -> &BodyBehaviorContributions {
        &self.behavior
    }
}

fn require_semantic_identity(
    expected_unit: BoundUnitId,
    expected_kind: BoundUnitKind,
    actual_unit: BoundUnitId,
    actual_kind: BoundUnitKind,
    input: SemanticSnapshotInputKind,
) -> Result<(), SemanticSnapshotBuildError> {
    if (actual_unit, actual_kind) != (expected_unit, expected_kind) {
        return Err(SemanticSnapshotBuildError {
            input,
            expected_unit,
            expected_kind,
            actual_unit,
            actual_kind,
        });
    }

    Ok(())
}
