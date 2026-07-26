use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_symbols::{
    BorrowKind, DependencyContractTemplateData, DependencyGuard, DependencyRequirement,
    DependencyRequirementKind, DependencySubject, ImplementationInstanceId,
    LifecycleObligationKind, UnionVariantSymbolId,
};

use crate::identity::define_unit_scoped_id;
use crate::{BorrowCapabilityId, BoundUnitId, StorageAccessId, StorageIdentityId, StoragePlan};

define_unit_scoped_id!(
    BoundDependencyContractId,
    "Identifies one normalized instantiated dependency contract in a checked semantic unit."
);
define_unit_scoped_id!(
    ScopedCapabilityId,
    "Identifies one scoped capability value in a checked semantic unit."
);
define_unit_scoped_id!(
    LifecycleObligationId,
    "Identifies one lifecycle obligation in a checked semantic unit."
);

impl BoundDependencyContractId {
    pub(crate) const fn from_contract_slot(unit: BoundUnitId, slot: u32) -> Self {
        Self { unit, slot }
    }

    pub(crate) const fn contract_slot(self) -> u32 {
        self.slot
    }
}

/// An exact bound-unit subject referenced by an instantiated dependency contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundDependencySubject {
    /// A persistent storage origin.
    Storage(StorageIdentityId),
    /// One evaluated access to storage or substorage.
    StorageAccess(StorageAccessId),
    /// A borrow capability established by one operation.
    BorrowCapability(BorrowCapabilityId),
    /// A capability valid only within its checked scope.
    ScopedCapability(ScopedCapabilityId),
    /// A selected semantic implementation witness.
    ImplementationWitness(ImplementationInstanceId),
    /// A value-attached lifecycle obligation.
    LifecycleObligation(LifecycleObligationId),
}

impl BoundDependencySubject {
    pub(crate) fn is_valid_for(self, unit: BoundUnitId) -> bool {
        match self {
            Self::Storage(storage) => storage.unit() == unit,
            Self::StorageAccess(access) => access.unit() == unit,
            Self::BorrowCapability(capability) => capability.unit() == unit,
            Self::ScopedCapability(capability) => capability.unit() == unit,
            Self::LifecycleObligation(obligation) => obligation.unit() == unit,
            Self::ImplementationWitness(_) => true,
        }
    }

    fn exists_in(self, storage: &StoragePlan) -> bool {
        match self {
            Self::Storage(identity) => storage.identity(identity).is_some(),
            Self::StorageAccess(access) => storage.access(access).is_some(),
            Self::BorrowCapability(capability) => storage.borrow_capability(capability).is_some(),
            Self::ScopedCapability(capability) => capability.unit() == storage.unit(),
            Self::LifecycleObligation(obligation) => obligation.unit() == storage.unit(),
            Self::ImplementationWitness(_) => true,
        }
    }
}

/// The exact semantic state required from a bound dependency subject.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundDependencyRequirementKind {
    /// Reached storage must remain alive.
    StorageAlive,
    /// Reached storage or substorage must remain initialized.
    StorageInitialized,
    /// A borrow capability of the exact kind must remain active.
    BorrowCapabilityActive(BorrowKind),
    /// Mutation authority over reached storage must remain exclusive.
    ExclusiveMutationAuthority,
    /// A scoped capability must remain live.
    ScopedCapabilityLive,
    /// A lifecycle obligation of the exact kind must remain attached.
    LifecycleObligationAttached(LifecycleObligationKind),
}

impl From<DependencyRequirementKind> for BoundDependencyRequirementKind {
    fn from(kind: DependencyRequirementKind) -> Self {
        match kind {
            DependencyRequirementKind::StorageAlive => Self::StorageAlive,
            DependencyRequirementKind::StorageInitialized => Self::StorageInitialized,
            DependencyRequirementKind::BorrowCapabilityActive(kind) => {
                Self::BorrowCapabilityActive(kind)
            }
            DependencyRequirementKind::ExclusiveMutationAuthority => {
                Self::ExclusiveMutationAuthority
            }
            DependencyRequirementKind::ScopedCapabilityLive => Self::ScopedCapabilityLive,
            DependencyRequirementKind::LifecycleObligation(kind) => {
                Self::LifecycleObligationAttached(kind)
            }
        }
    }
}

impl From<BoundDependencyRequirementKind> for DependencyRequirementKind {
    fn from(kind: BoundDependencyRequirementKind) -> Self {
        match kind {
            BoundDependencyRequirementKind::StorageAlive => Self::StorageAlive,
            BoundDependencyRequirementKind::StorageInitialized => Self::StorageInitialized,
            BoundDependencyRequirementKind::BorrowCapabilityActive(kind) => {
                Self::BorrowCapabilityActive(kind)
            }
            BoundDependencyRequirementKind::ExclusiveMutationAuthority => {
                Self::ExclusiveMutationAuthority
            }
            BoundDependencyRequirementKind::ScopedCapabilityLive => Self::ScopedCapabilityLive,
            BoundDependencyRequirementKind::LifecycleObligationAttached(kind) => {
                Self::LifecycleObligation(kind)
            }
        }
    }
}

/// Resolves portable dependency-template subjects into exact bound-unit facts.
///
/// Implementations must resolve projections and validate them against the current checked
/// expressions, selected implementations, and semantic facts.
pub trait DependencyContractInstantiationContext {
    /// The typed failure returned when a formal subject or guard cannot be instantiated.
    type Error;

    /// Returns the checked semantic unit receiving the instantiated contract.
    fn unit(&self) -> BoundUnitId;

    /// Resolves one formal requirement subject to the exact fact selected in this unit.
    fn resolve_subject(
        &mut self,
        subject: &DependencySubject,
        requirement: DependencyRequirementKind,
    ) -> Result<BoundDependencySubject, Self::Error>;

    /// Resolves one formal semantic guard to its exact bound-unit condition.
    fn resolve_guard(
        &mut self,
        guard: &DependencyGuard,
    ) -> Result<BoundDependencyGuard, Self::Error>;
}

/// A typed failure while instantiating a portable dependency contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DependencyContractInstantiationError<E> {
    /// Binder-owned subject or guard resolution failed.
    Resolution(E),
    /// A resolver returned a bound fact owned by another checked semantic unit.
    ForeignUnit,
}

/// A semantic condition guarding nested instantiated requirements.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundDependencyGuard {
    /// The nullable storage access currently contains a value.
    NullablePresent(StorageAccessId),
    /// The union storage access currently has the exact active variant.
    ActiveUnionVariant {
        /// The union access carrying the guarded payload.
        access: StorageAccessId,
        /// The variant required to be active.
        variant: UnionVariantSymbolId,
    },
    /// A borrow capability remains active.
    BorrowCapabilityActive(BorrowCapabilityId),
    /// A scoped capability remains live.
    ScopedCapabilityLive(ScopedCapabilityId),
}

impl BoundDependencyGuard {
    fn is_valid_for(self, unit: BoundUnitId) -> bool {
        match self {
            Self::NullablePresent(access) | Self::ActiveUnionVariant { access, .. } => {
                access.unit() == unit
            }
            Self::BorrowCapabilityActive(capability) => capability.unit() == unit,
            Self::ScopedCapabilityLive(capability) => capability.unit() == unit,
        }
    }

    fn exists_in(self, storage: &StoragePlan) -> bool {
        match self {
            Self::NullablePresent(access) | Self::ActiveUnionVariant { access, .. } => {
                storage.access(access).is_some()
            }
            Self::BorrowCapabilityActive(capability) => {
                storage.borrow_capability(capability).is_some()
            }
            Self::ScopedCapabilityLive(capability) => capability.unit() == storage.unit(),
        }
    }
}

/// A normalized guarded set of instantiated dependency requirements.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GuardedBoundDependencyRequirement {
    guard: BoundDependencyGuard,
    requirements: Arc<[BoundDependencyRequirement]>,
}

impl GuardedBoundDependencyRequirement {
    /// Creates a guarded requirement after deterministic sorting and deduplication.
    pub fn new(
        guard: BoundDependencyGuard,
        requirements: impl IntoIterator<Item = BoundDependencyRequirement>,
    ) -> Self {
        Self {
            guard,
            requirements: sorted_unique_shared_slice(requirements),
        }
    }

    /// Returns the semantic condition controlling the nested requirements.
    pub const fn guard(&self) -> BoundDependencyGuard {
        self.guard
    }

    /// Returns normalized requirements active while the guard holds.
    pub fn requirements(&self) -> &[BoundDependencyRequirement] {
        &self.requirements
    }

    fn is_valid_for(&self, unit: BoundUnitId) -> bool {
        self.guard.is_valid_for(unit)
            && self
                .requirements
                .iter()
                .all(|requirement| requirement.is_valid_for(unit))
    }

    fn exists_in(&self, storage: &StoragePlan) -> bool {
        self.guard.exists_in(storage)
            && self
                .requirements
                .iter()
                .all(|requirement| requirement.exists_in(storage))
    }
}

/// One direct or guarded requirement in an instantiated dependency contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundDependencyRequirement {
    /// An unconditional requirement on one exact bound-unit subject.
    Direct {
        /// The subject carrying the dependency.
        subject: BoundDependencySubject,
        /// The exact required semantic state.
        kind: BoundDependencyRequirementKind,
    },
    /// Requirements active only while a semantic condition holds.
    Guarded(GuardedBoundDependencyRequirement),
}

impl BoundDependencyRequirement {
    /// Creates an unconditional requirement.
    pub const fn direct(
        subject: BoundDependencySubject,
        kind: BoundDependencyRequirementKind,
    ) -> Self {
        Self::Direct { subject, kind }
    }

    /// Creates normalized guarded requirements.
    pub fn guarded(
        guard: BoundDependencyGuard,
        requirements: impl IntoIterator<Item = BoundDependencyRequirement>,
    ) -> Self {
        Self::Guarded(GuardedBoundDependencyRequirement::new(guard, requirements))
    }

    fn is_valid_for(&self, unit: BoundUnitId) -> bool {
        match self {
            Self::Direct { subject, .. } => subject.is_valid_for(unit),
            Self::Guarded(requirement) => requirement.is_valid_for(unit),
        }
    }

    fn exists_in(&self, storage: &StoragePlan) -> bool {
        match self {
            Self::Direct { subject, .. } => subject.exists_in(storage),
            Self::Guarded(requirement) => requirement.exists_in(storage),
        }
    }
}

/// A normalized unit-local instantiated dependency contract.
///
/// Unlike a source-independent `bray_symbols::DependencyContractTemplateData`, every subject in
/// this record refers to the exact storage, access, capability, witness, or obligation selected
/// while checking one semantic unit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundDependencyContract {
    requirements: Arc<[BoundDependencyRequirement]>,
}

impl BoundDependencyContract {
    /// Creates an instantiated contract after deterministic sorting and deduplication.
    pub fn new(requirements: impl IntoIterator<Item = BoundDependencyRequirement>) -> Self {
        Self {
            requirements: sorted_unique_shared_slice(requirements),
        }
    }

    /// Instantiates a portable symbol contract through exact binder-provided subject mappings.
    ///
    /// Guard nesting is retained. The completed contract is sorted and deduplicated only after
    /// every formal subject has been resolved successfully.
    pub fn try_instantiate<C>(
        template: &DependencyContractTemplateData,
        context: &mut C,
    ) -> Result<Self, DependencyContractInstantiationError<C::Error>>
    where
        C: DependencyContractInstantiationContext,
    {
        let requirements = instantiate_requirements(template.requirements(), context)?;

        Ok(Self::new(requirements))
    }

    /// Returns the normalized instantiated requirements.
    pub fn requirements(&self) -> &[BoundDependencyRequirement] {
        &self.requirements
    }

    pub(crate) fn is_valid_for(&self, unit: BoundUnitId) -> bool {
        self.requirements
            .iter()
            .all(|requirement| requirement.is_valid_for(unit))
    }

    pub(crate) fn exists_in(&self, storage: &StoragePlan) -> bool {
        self.requirements
            .iter()
            .all(|requirement| requirement.exists_in(storage))
    }
}

fn instantiate_requirements<C>(
    requirements: &[DependencyRequirement],
    context: &mut C,
) -> Result<Vec<BoundDependencyRequirement>, DependencyContractInstantiationError<C::Error>>
where
    C: DependencyContractInstantiationContext,
{
    requirements
        .iter()
        .map(|requirement| instantiate_requirement(requirement, context))
        .collect()
}

fn instantiate_requirement<C>(
    requirement: &DependencyRequirement,
    context: &mut C,
) -> Result<BoundDependencyRequirement, DependencyContractInstantiationError<C::Error>>
where
    C: DependencyContractInstantiationContext,
{
    match requirement {
        DependencyRequirement::Direct { subject, kind } => {
            let subject = context
                .resolve_subject(subject, *kind)
                .map_err(DependencyContractInstantiationError::Resolution)?;

            if !subject.is_valid_for(context.unit()) {
                return Err(DependencyContractInstantiationError::ForeignUnit);
            }

            Ok(BoundDependencyRequirement::direct(subject, (*kind).into()))
        }
        DependencyRequirement::Guarded(guarded) => {
            let guard = context
                .resolve_guard(guarded.guard())
                .map_err(DependencyContractInstantiationError::Resolution)?;

            if !guard.is_valid_for(context.unit()) {
                return Err(DependencyContractInstantiationError::ForeignUnit);
            }

            let requirements = instantiate_requirements(guarded.requirements(), context)?;

            Ok(BoundDependencyRequirement::guarded(guard, requirements))
        }
    }
}

impl BoundUnitId {
    /// Returns an instantiated dependency contract only when it belongs to this unit.
    pub fn checked_dependency_contract(
        self,
        id: BoundDependencyContractId,
    ) -> Option<BoundDependencyContractId> {
        (id.unit() == self).then_some(id)
    }

    /// Returns a scoped capability only when it belongs to this unit.
    pub fn checked_scoped_capability(self, id: ScopedCapabilityId) -> Option<ScopedCapabilityId> {
        (id.unit() == self).then_some(id)
    }

    /// Returns a lifecycle obligation only when it belongs to this unit.
    pub fn checked_lifecycle_obligation(
        self,
        id: LifecycleObligationId,
    ) -> Option<LifecycleObligationId> {
        (id.unit() == self).then_some(id)
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        BorrowKind, DependencyContractTemplateData, DependencyGuard, DependencyRequirement,
        DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
        LifecycleObligationKind, SymbolOrdinal,
    };

    use super::{
        BoundDependencyContract, BoundDependencyGuard, BoundDependencyRequirement,
        BoundDependencyRequirementKind, BoundDependencySubject,
        DependencyContractInstantiationContext, DependencyContractInstantiationError,
        LifecycleObligationId, ScopedCapabilityId,
    };
    use crate::{BorrowCapabilityId, BoundUnitId, StorageAccessId, StorageIdentityId};

    #[test]
    fn instantiated_contracts_are_normalized() {
        let unit = BoundUnitId::new(6);
        let subject = BoundDependencySubject::Storage(StorageIdentityId::from_slot(unit, 0));

        let requirement = BoundDependencyRequirement::direct(
            subject,
            BoundDependencyRequirementKind::StorageAlive,
        );

        let contract = BoundDependencyContract::new([
            requirement.clone(),
            requirement.clone(),
            requirement.clone(),
        ]);

        assert_eq!(contract.requirements(), &[requirement]);
        assert!(contract.is_valid_for(unit));
    }

    #[test]
    fn guarded_requirements_retain_conditions_and_nested_dependencies() {
        let unit = BoundUnitId::new(2);
        let access = StorageAccessId::from_slot(unit, 1);
        let capability = BorrowCapabilityId::from_slot(unit, 3);

        let nested = BoundDependencyRequirement::direct(
            BoundDependencySubject::BorrowCapability(capability),
            BoundDependencyRequirementKind::BorrowCapabilityActive(BorrowKind::Shared),
        );

        let guarded = BoundDependencyRequirement::guarded(
            BoundDependencyGuard::NullablePresent(access),
            [nested.clone(), nested.clone()],
        );

        let BoundDependencyRequirement::Guarded(guarded) = guarded else {
            panic!("guarded constructor must retain the guard");
        };

        assert_eq!(
            guarded.guard(),
            BoundDependencyGuard::NullablePresent(access)
        );

        assert_eq!(guarded.requirements(), &[nested]);
    }

    #[test]
    fn bound_subjects_cover_unit_local_capabilities_and_obligations() {
        let unit = BoundUnitId::new(9);
        let scoped = ScopedCapabilityId::from_slot(unit, 4);
        let obligation = LifecycleObligationId::from_slot(unit, 5);

        let requirements = [
            BoundDependencyRequirement::direct(
                BoundDependencySubject::ScopedCapability(scoped),
                BoundDependencyRequirementKind::ScopedCapabilityLive,
            ),
            BoundDependencyRequirement::direct(
                BoundDependencySubject::LifecycleObligation(obligation),
                BoundDependencyRequirementKind::LifecycleObligationAttached(
                    LifecycleObligationKind::Finalization,
                ),
            ),
        ];

        let contract = BoundDependencyContract::new(requirements);

        assert_eq!(contract.requirements().len(), 2);
        assert!(contract.is_valid_for(unit));
        assert!(!contract.is_valid_for(BoundUnitId::new(10)));
    }

    #[test]
    fn checked_contract_access_rejects_foreign_units() {
        let unit = BoundUnitId::new(3);
        let local = super::BoundDependencyContractId::from_slot(unit, 1);
        let foreign = super::BoundDependencyContractId::from_slot(BoundUnitId::new(4), 1);

        assert_eq!(unit.checked_dependency_contract(local), Some(local));
        assert_eq!(unit.checked_dependency_contract(foreign), None);
    }

    #[test]
    fn portable_templates_instantiate_to_exact_unit_local_contracts() {
        let unit = BoundUnitId::new(14);

        let parameter =
            DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)));

        let direct = DependencyRequirement::direct(
            parameter.clone(),
            DependencyRequirementKind::StorageInitialized,
        );

        let guarded = DependencyRequirement::guarded(
            DependencyGuard::NullablePresent(parameter),
            [direct.clone(), direct],
        );

        let template = DependencyContractTemplateData::new([guarded]);

        let access = StorageAccessId::from_slot(unit, 7);
        let mut context = TestInstantiationContext { unit, access };

        let Ok(contract) = BoundDependencyContract::try_instantiate(&template, &mut context) else {
            panic!("test template subjects must resolve");
        };

        let [BoundDependencyRequirement::Guarded(guarded)] = contract.requirements() else {
            panic!("template guard must remain nested");
        };

        assert_eq!(
            guarded.guard(),
            BoundDependencyGuard::NullablePresent(access)
        );

        assert_eq!(guarded.requirements().len(), 1);

        assert_eq!(
            guarded.requirements()[0],
            BoundDependencyRequirement::direct(
                BoundDependencySubject::StorageAccess(access),
                BoundDependencyRequirementKind::StorageInitialized,
            )
        );
    }

    #[test]
    fn template_instantiation_rejects_foreign_unit_mappings() {
        let unit = BoundUnitId::new(20);

        let parameter =
            DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)));

        let template = DependencyContractTemplateData::new([DependencyRequirement::direct(
            parameter,
            DependencyRequirementKind::StorageAlive,
        )]);

        let mut context = TestInstantiationContext {
            unit,
            access: StorageAccessId::from_slot(BoundUnitId::new(21), 0),
        };

        assert_eq!(
            BoundDependencyContract::try_instantiate(&template, &mut context),
            Err(DependencyContractInstantiationError::ForeignUnit)
        );
    }

    struct TestInstantiationContext {
        unit: BoundUnitId,
        access: StorageAccessId,
    }

    impl DependencyContractInstantiationContext for TestInstantiationContext {
        type Error = ();

        fn unit(&self) -> BoundUnitId {
            self.unit
        }

        fn resolve_subject(
            &mut self,
            _subject: &DependencySubject,
            _requirement: DependencyRequirementKind,
        ) -> Result<BoundDependencySubject, Self::Error> {
            Ok(BoundDependencySubject::StorageAccess(self.access))
        }

        fn resolve_guard(
            &mut self,
            _guard: &DependencyGuard,
        ) -> Result<BoundDependencyGuard, Self::Error> {
            Ok(BoundDependencyGuard::NullablePresent(self.access))
        }
    }
}
