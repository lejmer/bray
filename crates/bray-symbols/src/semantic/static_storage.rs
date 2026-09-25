use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_target::TargetIdentity;

use crate::{
    DependencyContractTemplateId, GenericSubstitutionId, ImplementationInstanceId,
    LifecycleObligationKind, StaticSymbolId, SymbolKey, TypeExpressionTemplate,
};

/// The owner domain of one static declaration instance.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StaticStorageDuration {
    /// One instance belongs to each exact product instance.
    Product,
    /// One instance belongs to each exact native-thread attachment.
    ExactThread,
}

/// One selected static reference before or after its substitution becomes fully closed.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StaticReferenceSelection {
    /// An open template reference retained for later generic instantiation.
    Open {
        /// Selected declaration template.
        template: StaticInstanceTemplateId,
        /// Exact symbolic substitution.
        substitution: GenericSubstitutionId,
        /// Selected implementation witnesses in requirement order.
        selected_witnesses: Arc<[ImplementationInstanceId]>,
        /// Selected target profile.
        target: TargetIdentity,
    },
    /// A fully closed semantic instance.
    Closed(StaticInstanceKey),
}

impl StaticReferenceSelection {
    /// Creates one open template reference.
    pub fn open(
        template: StaticInstanceTemplateId,
        substitution: GenericSubstitutionId,
        selected_witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        target: TargetIdentity,
    ) -> Self {
        Self::Open {
            template,
            substitution,
            selected_witnesses: shared_slice(selected_witnesses),
            target,
        }
    }

    /// Returns the selected declaration template.
    pub const fn template(&self) -> StaticInstanceTemplateId {
        match self {
            Self::Open { template, .. } => *template,
            Self::Closed(instance) => instance.template(),
        }
    }

    /// Returns the exact open or closed substitution.
    pub const fn substitution(&self) -> GenericSubstitutionId {
        match self {
            Self::Open { substitution, .. } => *substitution,
            Self::Closed(instance) => instance.substitution(),
        }
    }

    /// Returns the selected target profile for this static reference.
    pub const fn target(&self) -> &TargetIdentity {
        match self {
            Self::Open { target, .. } => target,
            Self::Closed(instance) => instance.target(),
        }
    }

    /// Returns the closed instance when all arguments are concrete.
    pub const fn closed_instance(&self) -> Option<&StaticInstanceKey> {
        match self {
            Self::Open { .. } => None,
            Self::Closed(instance) => Some(instance),
        }
    }
}

/// Stable identity of one open static-instance template.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StaticInstanceTemplateId(StaticSymbolId);

impl StaticInstanceTemplateId {
    /// Creates the unique template identity for one static declaration.
    pub const fn new(declaration: StaticSymbolId) -> Self {
        Self(declaration)
    }

    /// Returns the declaration that uniquely owns this template.
    pub const fn declaration(self) -> StaticSymbolId {
        self.0
    }
}

/// Checked open semantics published for one static declaration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StaticInstanceTemplate {
    id: StaticInstanceTemplateId,
    duration: StaticStorageDuration,
    declared_type: TypeExpressionTemplate,
    dependency_contract: DependencyContractTemplateId,
    lifecycle_dependencies: Arc<[StaticSymbolId]>,
    lifecycle_obligations: Arc<[LifecycleObligationKind]>,
    witness_requirements: Arc<[SymbolKey]>,
}

impl StaticInstanceTemplate {
    /// Creates a checked open static-instance template.
    pub fn new(
        id: StaticInstanceTemplateId,
        duration: StaticStorageDuration,
        declared_type: TypeExpressionTemplate,
        dependency_contract: DependencyContractTemplateId,
        lifecycle_dependencies: impl IntoIterator<Item = StaticSymbolId>,
        lifecycle_obligations: impl IntoIterator<Item = LifecycleObligationKind>,
        witness_requirements: impl IntoIterator<Item = SymbolKey>,
    ) -> Self {
        Self {
            id,
            duration,
            declared_type,
            dependency_contract,
            lifecycle_dependencies: sorted_unique_shared_slice(lifecycle_dependencies),
            lifecycle_obligations: sorted_unique_shared_slice(lifecycle_obligations),
            witness_requirements: sorted_unique_shared_slice(witness_requirements),
        }
    }

    /// Returns this declaration-owned template's stable identity.
    pub const fn id(&self) -> StaticInstanceTemplateId {
        self.id
    }

    /// Returns the owner domain selected by the declaration surface.
    pub const fn duration(&self) -> StaticStorageDuration {
        self.duration
    }

    /// Returns the checked declared type template.
    pub const fn declared_type(&self) -> &TypeExpressionTemplate {
        &self.declared_type
    }

    /// Returns dependencies retained by the initialized value.
    pub const fn dependency_contract(&self) -> DependencyContractTemplateId {
        self.dependency_contract
    }

    /// Returns static instances that must remain alive until this instance completes cleanup.
    pub fn lifecycle_dependencies(&self) -> &[StaticSymbolId] {
        &self.lifecycle_dependencies
    }

    /// Returns lifecycle obligations attached after materialization.
    pub fn lifecycle_obligations(&self) -> &[LifecycleObligationKind] {
        &self.lifecycle_obligations
    }

    /// Returns selected-witness requirements in canonical declaration order.
    pub fn witness_requirements(&self) -> &[SymbolKey] {
        &self.witness_requirements
    }
}

/// Canonical semantic identity of one demanded closed static-instance template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StaticInstanceKey {
    template: StaticInstanceTemplateId,
    substitution: GenericSubstitutionId,
    selected_witnesses: Arc<[ImplementationInstanceId]>,
    target: TargetIdentity,
}

impl StaticInstanceKey {
    /// Creates one closed identity from its exact semantic inputs.
    pub fn new(
        template: StaticInstanceTemplateId,
        substitution: GenericSubstitutionId,
        selected_witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        target: TargetIdentity,
    ) -> Self {
        Self {
            template,
            substitution,
            selected_witnesses: shared_slice(selected_witnesses),
            target,
        }
    }

    /// Returns the open template being instantiated.
    pub const fn template(&self) -> StaticInstanceTemplateId {
        self.template
    }

    /// Returns the exact normalized closed substitution.
    pub const fn substitution(&self) -> GenericSubstitutionId {
        self.substitution
    }

    /// Returns the exact selected implementations in requirement order.
    pub fn selected_witnesses(&self) -> &[ImplementationInstanceId] {
        &self.selected_witnesses
    }

    /// Returns the selected target-profile identity.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }
}

#[cfg(test)]
mod tests {
    use super::{StaticInstanceKey, StaticInstanceTemplate};

    #[test]
    fn static_semantics_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<StaticInstanceTemplate>();
        assert_send_sync::<StaticInstanceKey>();
    }
}
