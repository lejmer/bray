use bray_ir::{MirOperationKind, MirUnit};
use bray_symbols::PackageIdentity;

use crate::{CodegenInstance, CodegenLinkage};

const MIR_STRUCTURE_COST_MODEL_REVISION: u32 = 1;

/// Estimated deterministic code generation work measured from canonical MIR.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenWork(u64);

impl CodegenWork {
    /// Creates a work value from deterministic cost-model units.
    pub const fn new(units: u64) -> Self {
        Self(units)
    }

    /// Returns the deterministic cost-model units.
    pub const fn units(self) -> u64 {
        self.0
    }

    pub(super) const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }
}

/// Visibility boundary selected for one generated definition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenDefinitionVisibility {
    /// Visible only within one generated unit.
    Unit,
    /// Visible throughout the linked product.
    Product,
    /// Visible across product boundaries.
    Public,
}

/// Per-definition symbol and source identities retained inside a generated unit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenPartitionCompatibility {
    package: PackageIdentity,
    source_namespace: [u8; 32],
    linkage: CodegenLinkage,
    visibility: CodegenDefinitionVisibility,
    native_selection_boundary: bool,
    native_storage_dependencies_identity: [u8; 32],
}

impl CodegenPartitionCompatibility {
    /// Creates one exact package, source, linkage, and visibility identity.
    pub const fn new(
        package: PackageIdentity,
        source_namespace: [u8; 32],
        linkage: CodegenLinkage,
        visibility: CodegenDefinitionVisibility,
    ) -> Self {
        Self {
            package,
            source_namespace,
            linkage,
            visibility,
            native_selection_boundary: false,
            native_storage_dependencies_identity: [0; 32],
        }
    }

    /// Keeps a native selection-sensitive definition independently publishable.
    pub const fn with_native_selection_boundary(mut self) -> Self {
        self.native_selection_boundary = true;

        self
    }

    /// Whether a native reference requires an independent selection boundary.
    pub const fn native_selection_boundary(&self) -> bool {
        self.native_selection_boundary
    }

    /// Records the canonical identity of directly referenced native and owned storage.
    pub const fn with_native_storage_dependencies_identity(mut self, identity: [u8; 32]) -> Self {
        self.native_storage_dependencies_identity = identity;

        self
    }

    /// Returns the canonical storage dependency set identity. Zero denotes no storage.
    pub const fn native_storage_dependencies_identity(&self) -> [u8; 32] {
        self.native_storage_dependencies_identity
    }

    /// Returns the package whose generated definitions are being partitioned.
    pub const fn package(&self) -> &PackageIdentity {
        &self.package
    }

    /// Returns the namespace of source IDs emitted by this product.
    pub const fn source_namespace(&self) -> [u8; 32] {
        self.source_namespace
    }

    /// Returns the selected linkage of this definition.
    pub const fn linkage(&self) -> CodegenLinkage {
        self.linkage
    }

    /// Returns the visibility boundary of this definition.
    pub const fn visibility(&self) -> CodegenDefinitionVisibility {
        self.visibility
    }
}

/// Complete deterministic policy for constructing code generation units.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenPartitionPolicy {
    identity: u32,
    revision: u32,
    cost_model_revision: u32,
    lower_bound: CodegenWork,
    target_work: CodegenWork,
    upper_bound: CodegenWork,
}

impl CodegenPartitionPolicy {
    /// Policy used for balanced native compilation.
    pub const NATIVE_BALANCED: Self = Self {
        identity: 1,
        revision: 2,
        cost_model_revision: MIR_STRUCTURE_COST_MODEL_REVISION,
        lower_bound: CodegenWork::new(1_024),
        target_work: CodegenWork::new(2_048),
        upper_bound: CodegenWork::new(4_096),
    };

    /// Publishes bounded, independently selectable groups while preserving mandatory co-location.
    pub const NATIVE_LIBRARY_PUBLICATION: Self = Self {
        identity: 2,
        revision: 4,
        cost_model_revision: MIR_STRUCTURE_COST_MODEL_REVISION,
        lower_bound: CodegenWork::new(64),
        target_work: CodegenWork::new(128),
        upper_bound: CodegenWork::new(256),
    };

    /// Creates a policy when its identities and work bounds are valid.
    pub const fn try_new(
        identity: u32,
        revision: u32,
        cost_model_revision: u32,
        lower_bound: CodegenWork,
        target_work: CodegenWork,
        upper_bound: CodegenWork,
    ) -> Result<Self, CodegenPartitionPolicyBuildError> {
        if identity == 0 || revision == 0 {
            return Err(CodegenPartitionPolicyBuildError::ZeroIdentity);
        }

        if cost_model_revision != MIR_STRUCTURE_COST_MODEL_REVISION {
            return Err(CodegenPartitionPolicyBuildError::UnsupportedCostModelRevision);
        }

        if lower_bound.units() == 0
            || lower_bound.units() > target_work.units()
            || target_work.units() > upper_bound.units()
        {
            return Err(CodegenPartitionPolicyBuildError::InvalidWorkBounds);
        }

        Ok(Self {
            identity,
            revision,
            cost_model_revision,
            lower_bound,
            target_work,
            upper_bound,
        })
    }

    /// Returns the stable identity of the partition algorithm family.
    pub const fn identity(self) -> u32 {
        self.identity
    }

    /// Returns the selected partition-policy revision.
    pub const fn revision(self) -> u32 {
        self.revision
    }

    /// Returns the selected MIR cost-model revision.
    pub const fn cost_model_revision(self) -> u32 {
        self.cost_model_revision
    }

    /// Returns the lower balanced-unit work bound.
    pub const fn lower_bound(self) -> CodegenWork {
        self.lower_bound
    }

    /// Returns the content-defined boundary target.
    pub const fn target_work(self) -> CodegenWork {
        self.target_work
    }

    /// Returns the hard work bound for a divisible unit.
    pub const fn upper_bound(self) -> CodegenWork {
        self.upper_bound
    }

    pub(super) fn estimate(self, instance: &CodegenInstance) -> CodegenWork {
        estimate_mir_work(instance.mir())
    }
}

/// Invalid deterministic partition-policy configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodegenPartitionPolicyBuildError {
    /// A policy or cost-model identity is zero.
    ZeroIdentity,
    /// The requested MIR cost-model revision is not implemented.
    UnsupportedCostModelRevision,
    /// Work bounds are zero or not ordered from lower through target to upper.
    InvalidWorkBounds,
}

/// Why one generated unit exceeds the configured work bound.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenOversizedUnitReason {
    /// One concrete definition exceeds the configured upper work bound.
    IndivisibleDefinition,
    /// Dependency co-location made the definitions indivisible.
    IndivisibleDependencyGroup,
}

/// Deterministic metadata for a generated unit above the configured upper bound.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenOversizedUnit {
    reason: CodegenOversizedUnitReason,
    work: CodegenWork,
    upper_bound: CodegenWork,
}

impl CodegenOversizedUnit {
    pub(super) const fn indivisible(
        definitions: usize,
        work: CodegenWork,
        upper_bound: CodegenWork,
    ) -> Self {
        let reason = if definitions == 1 {
            CodegenOversizedUnitReason::IndivisibleDefinition
        } else {
            CodegenOversizedUnitReason::IndivisibleDependencyGroup
        };

        Self {
            reason,
            work,
            upper_bound,
        }
    }

    /// Returns why the generated unit could not be split at the upper bound.
    pub const fn reason(self) -> CodegenOversizedUnitReason {
        self.reason
    }

    /// Returns the estimated work of the indivisible unit.
    pub const fn work(self) -> CodegenWork {
        self.work
    }

    /// Returns the policy upper bound exceeded by the unit.
    pub const fn upper_bound(self) -> CodegenWork {
        self.upper_bound
    }
}

fn estimate_mir_work(mir: &MirUnit) -> CodegenWork {
    let mut work = CodegenWork::new(16);

    work = add_count(work, mir.blocks().len(), 8);
    work = add_count(work, mir.storages().len(), 2);
    work = add_count(work, mir.values().len(), 1);

    for operation in mir.operations() {
        work = work.saturating_add(CodegenWork::new(operation_weight(operation.kind())));
    }

    for block in mir.blocks() {
        let mut successors = 0_u64;

        block
            .terminator()
            .kind()
            .for_each_successor(|_| successors = successors.saturating_add(1));

        work = work.saturating_add(CodegenWork::new(2_u64.saturating_add(successors)));
    }

    if mir.frame_descriptor().is_some() {
        work = work.saturating_add(CodegenWork::new(32));
    }

    work
}

fn add_count(work: CodegenWork, count: usize, weight: u64) -> CodegenWork {
    let count = u64::try_from(count).unwrap_or(u64::MAX);

    work.saturating_add(CodegenWork::new(count.saturating_mul(weight)))
}

const fn operation_weight(operation: &MirOperationKind) -> u64 {
    match operation {
        MirOperationKind::AnonymousCallable(_)
        | MirOperationKind::DeclaredCallable(_)
        | MirOperationKind::Borrow { .. }
        | MirOperationKind::Unary { .. } => 1,
        MirOperationKind::Store { .. }
        | MirOperationKind::Binary { .. }
        | MirOperationKind::Aggregate(_)
        | MirOperationKind::Construct(_)
        | MirOperationKind::Convert { .. }
        | MirOperationKind::NumericConversion { .. }
        | MirOperationKind::NullableQuery(_)
        | MirOperationKind::PatternProjection { .. }
        | MirOperationKind::Generator(_)
        | MirOperationKind::Finalize(_)
        | MirOperationKind::Destroy(_) => 2,
        MirOperationKind::AdmitOutgoing { .. }
        | MirOperationKind::DischargeOutgoing { .. }
        | MirOperationKind::Call(_)
        | MirOperationKind::Memory(_)
        | MirOperationKind::Text(_)
        | MirOperationKind::PanicReport(_)
        | MirOperationKind::Cleanup { .. } => 4,
        MirOperationKind::Async(_) | MirOperationKind::Host(_) => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::{CodegenPartitionPolicy, CodegenPartitionPolicyBuildError, CodegenWork};

    #[test]
    fn policies_require_ordered_nonzero_identities_and_bounds() {
        assert_eq!(
            CodegenPartitionPolicy::try_new(
                1,
                1,
                2,
                CodegenWork::new(1),
                CodegenWork::new(2),
                CodegenWork::new(3),
            ),
            Err(CodegenPartitionPolicyBuildError::UnsupportedCostModelRevision)
        );

        assert_eq!(
            CodegenPartitionPolicy::try_new(
                0,
                1,
                1,
                CodegenWork::new(1),
                CodegenWork::new(2),
                CodegenWork::new(3),
            ),
            Err(CodegenPartitionPolicyBuildError::ZeroIdentity)
        );

        assert_eq!(
            CodegenPartitionPolicy::try_new(
                1,
                1,
                1,
                CodegenWork::new(3),
                CodegenWork::new(2),
                CodegenWork::new(4),
            ),
            Err(CodegenPartitionPolicyBuildError::InvalidWorkBounds)
        );
    }
}
