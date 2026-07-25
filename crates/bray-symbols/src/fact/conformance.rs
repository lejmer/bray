use std::sync::Arc;

use bray_base::shared_slice;

use crate::{
    ImplementationSymbolId, TraitApplicationId, TraitCallableFulfillmentSymbolId,
    TraitCallableMemberSymbolId, TraitConstantFulfillmentSymbolId, TraitConstantMemberSymbolId,
    TraitPredicateFulfillmentSymbolId, TraitPredicateMemberSymbolId,
    TraitScopeEnterFulfillmentSymbolId, TraitScopeEnterRequirementSymbolId,
    TraitScopeExitFulfillmentSymbolId, TraitScopeExitRequirementSymbolId,
    TraitTypeFulfillmentSymbolId, TraitTypeMemberSymbolId,
};

/// One exact member requirement of an applied trait.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TraitMemberRequirementId {
    /// A callable member requirement.
    Callable(TraitCallableMemberSymbolId),
    /// A constant-valued member requirement.
    Constant(TraitConstantMemberSymbolId),
    /// A type-valued member requirement.
    Type(TraitTypeMemberSymbolId),
    /// A predicate member requirement.
    Predicate(TraitPredicateMemberSymbolId),
    /// A scope-entry lifecycle requirement.
    ScopeEnter(TraitScopeEnterRequirementSymbolId),
    /// A scope-exit lifecycle requirement.
    ScopeExit(TraitScopeExitRequirementSymbolId),
}

impl TraitMemberRequirementId {
    /// Returns the underlying declaration symbol.
    pub const fn symbol(self) -> crate::AnySymbolId {
        match self {
            Self::Callable(id) => crate::AnySymbolId::TraitCallableMember(id),
            Self::Constant(id) => crate::AnySymbolId::TraitConstantMember(id),
            Self::Type(id) => crate::AnySymbolId::TraitTypeMember(id),
            Self::Predicate(id) => crate::AnySymbolId::TraitPredicateMember(id),
            Self::ScopeEnter(id) => crate::AnySymbolId::TraitScopeEnterRequirement(id),
            Self::ScopeExit(id) => crate::AnySymbolId::TraitScopeExitRequirement(id),
        }
    }
}

/// One exact member declaration in a trait implementation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TraitMemberFulfillmentId {
    /// A callable member fulfillment.
    Callable(TraitCallableFulfillmentSymbolId),
    /// A constant-valued member fulfillment.
    Constant(TraitConstantFulfillmentSymbolId),
    /// A type-valued member fulfillment.
    Type(TraitTypeFulfillmentSymbolId),
    /// A predicate member fulfillment.
    Predicate(TraitPredicateFulfillmentSymbolId),
    /// A scope-entry lifecycle fulfillment.
    ScopeEnter(TraitScopeEnterFulfillmentSymbolId),
    /// A scope-exit lifecycle fulfillment.
    ScopeExit(TraitScopeExitFulfillmentSymbolId),
}

impl TraitMemberFulfillmentId {
    /// Returns the underlying declaration symbol.
    pub const fn symbol(self) -> crate::AnySymbolId {
        match self {
            Self::Callable(id) => crate::AnySymbolId::TraitCallableFulfillment(id),
            Self::Constant(id) => crate::AnySymbolId::TraitConstantFulfillment(id),
            Self::Type(id) => crate::AnySymbolId::TraitTypeFulfillment(id),
            Self::Predicate(id) => crate::AnySymbolId::TraitPredicateFulfillment(id),
            Self::ScopeEnter(id) => crate::AnySymbolId::TraitScopeEnterFulfillment(id),
            Self::ScopeExit(id) => crate::AnySymbolId::TraitScopeExitFulfillment(id),
        }
    }
}

/// How one applied trait requirement is supplied by an implementation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TraitRequirementResolution {
    /// The implementation declares a compatible fulfillment.
    Explicit(TraitMemberFulfillmentId),
    /// The implementation uses behavior declared by the trait.
    TraitDefault,
    /// No usable fulfillment or trait default exists.
    Missing,
    /// A same-named fulfillment exists but is incompatible.
    Incompatible(TraitMemberFulfillmentId),
}

/// The resolution of one exact requirement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TraitRequirementConformance {
    requirement: TraitMemberRequirementId,
    resolution: TraitRequirementResolution,
}

impl TraitRequirementConformance {
    /// Creates one requirement resolution.
    pub const fn new(
        requirement: TraitMemberRequirementId,
        resolution: TraitRequirementResolution,
    ) -> Self {
        Self {
            requirement,
            resolution,
        }
    }

    /// Returns the exact applied-trait requirement.
    pub const fn requirement(self) -> TraitMemberRequirementId {
        self.requirement
    }

    /// Returns how the implementation supplies the requirement.
    pub const fn resolution(self) -> TraitRequirementResolution {
        self.resolution
    }
}

/// Immutable fulfillment validity for one trait implementation declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraitImplementationConformance {
    implementation: ImplementationSymbolId,
    trait_application: TraitApplicationId,
    requirements: Arc<[TraitRequirementConformance]>,
    extra_fulfillments: Arc<[TraitMemberFulfillmentId]>,
}

impl TraitImplementationConformance {
    /// Creates one conformance result in deterministic requirement and source order.
    pub fn new(
        implementation: ImplementationSymbolId,
        trait_application: TraitApplicationId,
        requirements: impl IntoIterator<Item = TraitRequirementConformance>,
        extra_fulfillments: impl IntoIterator<Item = TraitMemberFulfillmentId>,
    ) -> Self {
        Self {
            implementation,
            trait_application,
            requirements: shared_slice(requirements),
            extra_fulfillments: shared_slice(extra_fulfillments),
        }
    }

    /// Returns the implementation declaration that was checked.
    pub const fn implementation(&self) -> ImplementationSymbolId {
        self.implementation
    }

    /// Returns the exact trait application implemented by the declaration.
    pub const fn trait_application(&self) -> TraitApplicationId {
        self.trait_application
    }

    /// Returns requirement resolutions in trait declaration order.
    pub fn requirements(&self) -> &[TraitRequirementConformance] {
        &self.requirements
    }

    /// Returns unmatched implementation members in source order.
    pub fn extra_fulfillments(&self) -> &[TraitMemberFulfillmentId] {
        &self.extra_fulfillments
    }

    /// Returns whether every requirement is satisfied and no extra member was declared.
    pub fn is_valid(&self) -> bool {
        self.extra_fulfillments.is_empty()
            && self.requirements.iter().all(|entry| {
                matches!(
                    entry.resolution(),
                    TraitRequirementResolution::Explicit(_)
                        | TraitRequirementResolution::TraitDefault
                )
            })
    }
}
