use super::{
    BodyBehaviorContributions, BoundUnitId, BoundUnitKind, CheckedAsync,
    CheckedDependencyContracts, CheckedExpressionTypes, CheckedLiteralValues, CheckedRefinements,
    CheckedSemanticSelections, Liveness, StorageFlow,
};

/// An inconsistent set of semantic results cannot form one immutable snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SemanticSnapshotBuildError;

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
        if !semantic_identity_matches(
            types.unit(),
            types.kind(),
            [
                (selections.unit(), selections.kind()),
                (literals.unit(), literals.kind()),
            ],
        ) {
            return Err(SemanticSnapshotBuildError);
        }

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

        if !semantic_identity_matches(
            unit,
            kind,
            [
                (refinements.unit(), refinements.kind()),
                (storage_flow.unit(), storage_flow.kind()),
                (dependencies.unit(), dependencies.kind()),
                (asynchronous.unit(), asynchronous.kind()),
                (behavior.unit(), behavior.kind()),
            ],
        ) {
            return Err(SemanticSnapshotBuildError);
        }

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

fn semantic_identity_matches<const N: usize>(
    unit: BoundUnitId,
    kind: BoundUnitKind,
    components: [(BoundUnitId, BoundUnitKind); N],
) -> bool {
    components
        .into_iter()
        .all(|component| component == (unit, kind))
}
