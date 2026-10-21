use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_symbols::AnyLocalSymbolId;

use super::BoundCallDependencyContracts;
#[cfg(test)]
use super::BoundCallReference;

use crate::{
    BorrowCapabilityId, BoundDependencyContract, BoundDependencyContractId, BoundExpression,
    BoundExpressionId, BoundReferenceTarget, BoundUnit, BoundUnitId, BoundUnitKind,
    StorageAccessId, StoragePlan,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct ContractEntry<I> {
    occurrence: I,
    contract: BoundDependencyContractId,
}

impl<I> ContractEntry<I> {
    const fn new(occurrence: I, contract: BoundDependencyContractId) -> Self {
        Self {
            occurrence,
            contract,
        }
    }

    const fn occurrence(&self) -> &I {
        &self.occurrence
    }

    const fn contract(&self) -> BoundDependencyContractId {
        self.contract
    }
}

/// A malformed per-unit dependency-contract table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DependencyContractsBuildError {
    /// The supplied storage plan describes another checked unit.
    ForeignStoragePlan,
    /// An expression contract names another unit or a missing expression.
    InvalidExpression,
    /// An access contract names another unit or a missing storage access.
    InvalidAccess,
    /// A borrow contract names another unit or a missing borrow capability.
    InvalidBorrow,
    /// A normalized contract references a semantic identity from another unit.
    ForeignContract,
    /// The table cannot assign another compact contract identity.
    ContractCapacityExceeded,
}

/// Durable dependency contracts for values, accesses, and borrows in one checked unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedDependencyContracts {
    static_carriers: BTreeMap<
        bray_symbols::StaticReferenceSelection,
        Arc<[bray_symbols::DependencyWitnessCarrier]>,
    >,
    projection_variants:
        BTreeMap<bray_symbols::UnionPayloadFieldSymbolId, bray_symbols::UnionVariantSymbolId>,
    unit: BoundUnitId,
    kind: BoundUnitKind,
    contracts: Arc<[BoundDependencyContract]>,
    calls: Arc<BTreeMap<crate::BoundExecutionSite, BoundCallDependencyContracts>>,
    expressions: Arc<[ContractEntry<crate::BoundExecutionSite>]>,
    deferred_expressions: Arc<[ContractEntry<crate::BoundExecutionSite>]>,
    accesses: Arc<[ContractEntry<StorageAccessId>]>,
    borrows: Arc<[ContractEntry<BorrowCapabilityId>]>,
    pub(crate) lifecycle_consumers:
        Arc<BTreeMap<crate::StorageIdentityId, BTreeSet<crate::StorageIdentityId>>>,
    lifecycle_dependencies: Arc<
        BTreeMap<bray_symbols::CallableInstanceData, bray_symbols::DependencyContractTemplateData>,
    >,
    is_recovered: bool,
}

impl CheckedDependencyContracts {
    /// Retains the initializer authority instantiated for referenced static storage.
    pub fn with_static_carriers(
        mut self,
        carriers: BTreeMap<
            bray_symbols::StaticReferenceSelection,
            Arc<[bray_symbols::DependencyWitnessCarrier]>,
        >,
    ) -> Self {
        self.static_carriers = carriers;

        self
    }

    /// Returns the checked initializer authority for each referenced instance.
    pub fn static_carriers(
        &self,
    ) -> &BTreeMap<
        bray_symbols::StaticReferenceSelection,
        Arc<[bray_symbols::DependencyWitnessCarrier]>,
    > {
        &self.static_carriers
    }

    /// Retains declaration relationships resolved before dependency flow.
    pub fn with_projection_variants(
        mut self,
        variants: BTreeMap<
            bray_symbols::UnionPayloadFieldSymbolId,
            bray_symbols::UnionVariantSymbolId,
        >,
    ) -> Self {
        self.projection_variants = variants;

        self
    }

    /// Returns the checked variant owning a projected payload field.
    pub fn projection_variant(
        &self,
        field: bray_symbols::UnionPayloadFieldSymbolId,
    ) -> bray_symbols::UnionVariantSymbolId {
        *self
            .projection_variants
            .get(&field)
            .expect("dependency preparation retains every projected union field")
    }

    /// Validates and creates one normalized per-unit dependency table.
    pub fn try_new(
        unit: &BoundUnit,
        storage: &StoragePlan,
        expressions: impl IntoIterator<Item = (crate::BoundExecutionSite, BoundDependencyContract)>,
        deferred_expressions: impl IntoIterator<
            Item = (crate::BoundExecutionSite, BoundDependencyContract),
        >,
        accesses: impl IntoIterator<Item = (StorageAccessId, BoundDependencyContract)>,
        borrows: impl IntoIterator<Item = (BorrowCapabilityId, BoundDependencyContract)>,
        is_recovered: bool,
    ) -> Result<Self, DependencyContractsBuildError> {
        if storage.unit() != unit.unit() || storage.kind() != unit.key().kind() {
            return Err(DependencyContractsBuildError::ForeignStoragePlan);
        }

        let mut contracts = Vec::new();
        let mut contract_ids = BTreeMap::new();

        let expressions = build_entries(
            unit.unit(),
            expressions,
            &mut contracts,
            &mut contract_ids,
            |expression| {
                expression.node().unit() == unit.unit()
                    && expression
                        .expression()
                        .is_some_and(|expression| unit.view().expression(expression).is_some())
            },
            DependencyContractsBuildError::InvalidExpression,
        )?;

        let deferred_expressions = build_entries(
            unit.unit(),
            deferred_expressions,
            &mut contracts,
            &mut contract_ids,
            |expression| {
                expression.node().unit() == unit.unit()
                    && expression
                        .expression()
                        .is_some_and(|expression| unit.view().expression(expression).is_some())
            },
            DependencyContractsBuildError::InvalidExpression,
        )?;

        let accesses = build_entries(
            unit.unit(),
            accesses,
            &mut contracts,
            &mut contract_ids,
            |access| access.unit() == unit.unit() && storage.access(access).is_some(),
            DependencyContractsBuildError::InvalidAccess,
        )?;

        let borrows = build_entries(
            unit.unit(),
            borrows,
            &mut contracts,
            &mut contract_ids,
            |borrow| borrow.unit() == unit.unit() && storage.borrow_capability(borrow).is_some(),
            DependencyContractsBuildError::InvalidBorrow,
        )?;

        let mut result = Self {
            projection_variants: BTreeMap::new(),
            static_carriers: BTreeMap::new(),
            unit: unit.unit(),
            kind: unit.key().kind(),
            contracts: contracts.into(),
            calls: Arc::default(),
            expressions: expressions.into(),
            deferred_expressions: deferred_expressions.into(),
            accesses: accesses.into(),
            borrows: borrows.into(),
            lifecycle_consumers: Arc::default(),
            lifecycle_dependencies: Arc::default(),
            is_recovered,
        };

        result.lifecycle_consumers = Arc::new(result.prepare_lifecycle_dependencies(unit, storage));

        Ok(result)
    }

    /// Publishes selected-call contracts after checking their unit-local subjects.
    pub fn try_with_calls(
        mut self,
        calls: BTreeMap<crate::BoundExecutionSite, BoundCallDependencyContracts>,
    ) -> Result<Self, DependencyContractsBuildError> {
        for (occurrence, contract) in &calls {
            if self.expression(*occurrence).is_none()
                || contract
                    .retained_inputs()
                    .into_iter()
                    .flatten()
                    .copied()
                    .any(|input| self.expression(input).is_none())
            {
                return Err(DependencyContractsBuildError::InvalidExpression);
            }

            if !contract.is_valid_for(self.unit) {
                return Err(DependencyContractsBuildError::ForeignContract);
            }
        }

        self.calls = Arc::new(calls);

        Ok(self)
    }

    /// Returns direct call contracts without inherited child-expression requirements.
    pub fn calls(&self) -> &BTreeMap<crate::BoundExecutionSite, BoundCallDependencyContracts> {
        &self.calls
    }

    /// Attaches the checked dependency and mutation summaries for selected cleanup callees.
    pub fn with_lifecycle_dependencies(
        mut self,
        references: BTreeMap<
            bray_symbols::CallableInstanceData,
            bray_symbols::DependencyContractTemplateData,
        >,
    ) -> Self {
        self.lifecycle_dependencies = Arc::new(references);

        self
    }

    /// Returns instantiated cleanup summaries shared by flow and execution-property checking.
    pub fn lifecycle_dependencies(
        &self,
    ) -> &BTreeMap<bray_symbols::CallableInstanceData, bray_symbols::DependencyContractTemplateData>
    {
        &self.lifecycle_dependencies
    }

    /// Returns the checked semantic unit described by this table.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the checked unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns one normalized contract by unit-local identity.
    pub fn contract(&self, id: BoundDependencyContractId) -> Option<&BoundDependencyContract> {
        if id.unit() != self.unit {
            return None;
        }

        usize::try_from(id.contract_slot())
            .ok()
            .and_then(|index| self.contracts.get(index))
    }

    /// Returns the contract carried by one source or implicit invocation occurrence.
    pub fn expression(
        &self,
        occurrence: impl Into<crate::BoundExecutionSite>,
    ) -> Option<BoundDependencyContractId> {
        find_contract(&self.expressions, occurrence.into())
    }

    /// Returns the contract required when one source or implicit invocation is driven.
    pub fn deferred_expression(
        &self,
        occurrence: impl Into<crate::BoundExecutionSite>,
    ) -> Option<BoundDependencyContractId> {
        find_contract(&self.deferred_expressions, occurrence.into())
    }

    /// Resolves the deferred contract carried through local reference bindings.
    pub fn deferred_expression_through_bindings(
        &self,
        unit: &BoundUnit,
        expression: BoundExpressionId,
    ) -> Option<BoundDependencyContractId> {
        if self.unit != unit.unit() || expression.unit() != self.unit {
            return None;
        }

        let initializers = unit.tree().local_initializers();
        let mut current = expression;
        let mut active = BTreeSet::new();

        loop {
            if let Some(contract) = self.deferred_expression(current) {
                return Some(contract);
            }

            if !active.insert(current) {
                return None;
            }

            current = match unit.view().expression(current)? {
                BoundExpression::Name(name) => {
                    let BoundReferenceTarget::Local(local) = name.target() else {
                        return None;
                    };

                    *initializers.get(&local)?
                }
                BoundExpression::PatternReference(reference) => {
                    *initializers.get(&AnyLocalSymbolId::from(reference.binding()))?
                }
                _ => return None,
            };
        }
    }

    /// Returns the contract carried by one evaluated storage access.
    pub fn access(&self, access: StorageAccessId) -> Option<BoundDependencyContractId> {
        find_contract(&self.accesses, access)
    }

    /// Returns the contract carried by one borrow capability.
    pub fn borrow(&self, borrow: BorrowCapabilityId) -> Option<BoundDependencyContractId> {
        find_contract(&self.borrows, borrow)
    }

    /// Returns whether this table exactly covers the supplied unit and storage plan.
    pub fn is_complete_for(&self, unit: &BoundUnit, storage: &StoragePlan) -> bool {
        if self.unit != unit.unit()
            || self.kind != unit.key().kind()
            || storage.unit() != unit.unit()
            || storage.kind() != unit.key().kind()
        {
            return false;
        }

        let expressions_match = self
            .expressions
            .iter()
            .filter_map(|entry| match entry.occurrence() {
                crate::BoundExecutionSite::Node(crate::AnyBoundNodeId::Expression(expression)) => {
                    Some(*expression)
                }
                _ => None,
            })
            .eq(unit.tree().expressions().map(|(expression, _)| expression));

        let accesses_match = self
            .accesses
            .iter()
            .map(|entry| *entry.occurrence())
            .eq(storage.access_entries().map(|(access, _)| access));

        let borrows_match = self
            .borrows
            .iter()
            .map(|entry| *entry.occurrence())
            .eq(storage
                .borrow_capability_entries()
                .map(|(borrow, _)| borrow));

        expressions_match
            && accesses_match
            && borrows_match
            && self
                .contracts
                .iter()
                .chain(
                    self.calls
                        .values()
                        .flat_map(BoundCallDependencyContracts::phase_contracts),
                )
                .all(|contract| contract.exists_in(storage))
    }

    /// Returns whether semantic recovery contributed to the table.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

fn build_entries<I>(
    unit: BoundUnitId,
    entries: impl IntoIterator<Item = (I, BoundDependencyContract)>,
    contracts: &mut Vec<BoundDependencyContract>,
    contract_ids: &mut BTreeMap<BoundDependencyContract, BoundDependencyContractId>,
    is_valid: impl Fn(I) -> bool,
    invalid: DependencyContractsBuildError,
) -> Result<Vec<ContractEntry<I>>, DependencyContractsBuildError>
where
    I: Copy + Ord,
{
    let mut entries = entries.into_iter().collect::<Vec<_>>();

    entries.sort_unstable_by_key(|(occurrence, _)| *occurrence);

    let mut output = Vec::with_capacity(entries.len());

    for (occurrence, contract) in entries {
        if !is_valid(occurrence) {
            return Err(invalid);
        }

        if !contract.is_valid_for(unit) {
            return Err(DependencyContractsBuildError::ForeignContract);
        }

        let contract = match contract_ids.get(&contract).copied() {
            Some(contract) => contract,
            None => {
                let slot = u32::try_from(contracts.len())
                    .map_err(|_| DependencyContractsBuildError::ContractCapacityExceeded)?;

                let id = BoundDependencyContractId::from_contract_slot(unit, slot);

                contracts.push(contract.clone());
                contract_ids.insert(contract, id);

                id
            }
        };

        output.push(ContractEntry::new(occurrence, contract));
    }

    if output
        .windows(2)
        .any(|pair| pair[0].occurrence() == pair[1].occurrence())
    {
        return Err(invalid);
    }

    Ok(output)
}

fn find_contract<I>(
    entries: &[ContractEntry<I>],
    occurrence: I,
) -> Option<BoundDependencyContractId>
where
    I: Copy + Ord,
{
    entries
        .binary_search_by_key(&occurrence, |entry| *entry.occurrence())
        .ok()
        .and_then(|index| entries.get(index))
        .map(ContractEntry::contract)
}

#[cfg(test)]
mod tests {
    use super::{CheckedDependencyContracts, DependencyContractsBuildError};
    use crate::{
        BoundDependencyContract, BoundDependencyRequirement, BoundDependencyRequirementKind,
        BoundDependencySubject, BoundExpressionId, BoundUnit, BoundUnitId, StorageAccess,
        StorageAccessId, StorageAccessRoot, StorageIdentity, StoragePlan, StoragePlanBuilder,
    };

    #[test]
    fn dependency_tables_intern_equal_contracts_across_occurrence_categories() {
        let unit = BoundUnitId::new(2);

        let (bound, expression, storage, access) = expression_unit_with_storage(unit);

        let contract = BoundDependencyContract::new([BoundDependencyRequirement::direct(
            BoundDependencySubject::StorageAccess(access),
            BoundDependencyRequirementKind::StorageAlive,
        )]);

        let deferred = BoundDependencyContract::new([BoundDependencyRequirement::direct(
            BoundDependencySubject::StorageAccess(access),
            BoundDependencyRequirementKind::StorageInitialized,
        )]);

        let table = CheckedDependencyContracts::try_new(
            &bound,
            &storage,
            [(expression.into(), contract.clone())],
            [(expression.into(), deferred)],
            [(access, contract)],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("unit-local dependency table must build: {error:?}"));

        let expression_contract = table
            .expression(expression)
            .unwrap_or_else(|| panic!("expression contract must exist"));

        assert_eq!(table.access(access), Some(expression_contract));

        let deferred_contract = table
            .deferred_expression(expression)
            .unwrap_or_else(|| panic!("deferred expression contract must exist"));

        assert_eq!(
            table
                .contract(expression_contract)
                .map(BoundDependencyContract::requirements),
            Some(
                [BoundDependencyRequirement::direct(
                    BoundDependencySubject::StorageAccess(access),
                    BoundDependencyRequirementKind::StorageAlive,
                )]
                .as_slice()
            )
        );

        assert_eq!(
            table
                .contract(deferred_contract)
                .map(BoundDependencyContract::requirements),
            Some(
                [BoundDependencyRequirement::direct(
                    BoundDependencySubject::StorageAccess(access),
                    BoundDependencyRequirementKind::StorageInitialized,
                )]
                .as_slice()
            )
        );
    }

    #[test]
    fn dependency_tables_reject_foreign_occurrences() {
        let unit = BoundUnitId::new(3);

        let (bound, _, storage, _) = expression_unit_with_storage(unit);

        assert_eq!(
            CheckedDependencyContracts::try_new(
                &bound,
                &storage,
                [(
                    BoundExpressionId::from_slot(BoundUnitId::new(4), 0).into(),
                    BoundDependencyContract::new([]),
                )],
                [],
                [(
                    StorageAccessId::from_slot(unit, 0),
                    BoundDependencyContract::new([]),
                )],
                [],
                false,
            ),
            Err(DependencyContractsBuildError::InvalidExpression)
        );
    }

    #[test]
    fn dependency_tables_require_exact_unit_and_storage_coverage() {
        let unit = BoundUnitId::new(4);

        let (bound, expression, storage, access) = expression_unit_with_storage(unit);

        let contract = BoundDependencyContract::new([]);

        let complete = CheckedDependencyContracts::try_new(
            &bound,
            &storage,
            [(expression.into(), contract.clone())],
            [],
            [(access, contract)],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("complete dependency table must build: {error:?}"));

        assert!(complete.is_complete_for(&bound, &storage));

        let alternate_storage = StoragePlanBuilder::new(unit, bound.key().kind()).finish();

        assert!(!complete.is_complete_for(&bound, &alternate_storage));

        let incomplete =
            CheckedDependencyContracts::try_new(&bound, &storage, [], [], [], [], false)
                .unwrap_or_else(|error| panic!("partial dependency table must build: {error:?}"));

        assert!(!incomplete.is_complete_for(&bound, &storage));
    }

    #[test]
    fn call_publication_preserves_distinct_phases_and_rejects_foreign_subjects() {
        let unit = BoundUnitId::new(5);

        let (bound, expression, storage, access) = expression_unit_with_storage(unit);

        let table = || {
            CheckedDependencyContracts::try_new(
                &bound,
                &storage,
                [(expression.into(), BoundDependencyContract::new([]))],
                [],
                [(access, BoundDependencyContract::new([]))],
                [],
                false,
            )
            .unwrap()
        };

        let contract = |access, kind| {
            BoundDependencyContract::new([BoundDependencyRequirement::direct(
                BoundDependencySubject::StorageAccess(access),
                kind,
            )])
        };

        let invocation = contract(access, BoundDependencyRequirementKind::StorageAlive);
        let result = contract(access, BoundDependencyRequirementKind::ValueDependencies);
        let deferred = contract(access, BoundDependencyRequirementKind::StorageInitialized);

        let phases = super::BoundCallDependencyContracts::new(
            invocation.clone(),
            result.clone(),
            Some(deferred.clone()),
        );

        let relation = bray_symbols::DependencyBorrowRelation::new(
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Parameter(
                bray_symbols::SymbolOrdinal::new(0),
            )),
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Result),
            bray_symbols::DependencyReferenceKind::Borrow,
        );

        let references = vec![super::BoundCallReference::new(relation, true, None)];

        let retained = Some(std::collections::BTreeSet::from([expression.into()]));
        let phases = phases.with_references(references.clone(), retained.clone());

        let published = table()
            .try_with_calls([(expression.into(), phases.clone())].into())
            .unwrap();

        let call = &published.calls()[&expression.into()];

        assert_eq!(call.invocation(), &invocation);
        assert_eq!(call.result(), &result);
        assert_eq!(call.deferred(), Some(&deferred));
        assert_eq!(call.references(), &references);
        assert_eq!(call.retained_inputs(), retained.as_ref());

        let missing = BoundExpressionId::from_slot(unit, 1);

        assert_eq!(
            table().try_with_calls(
                [(
                    expression.into(),
                    phases.clone().with_references(
                        references,
                        Some(std::collections::BTreeSet::from([missing.into()]))
                    )
                )]
                .into()
            ),
            Err(DependencyContractsBuildError::InvalidExpression)
        );

        for occurrence in [
            BoundExpressionId::from_slot(BoundUnitId::new(6), 0),
            BoundExpressionId::from_slot(unit, 1),
        ] {
            assert_eq!(
                table().try_with_calls([(occurrence.into(), phases.clone())].into()),
                Err(DependencyContractsBuildError::InvalidExpression),
            );
        }

        let (_, _, _, foreign) = expression_unit_with_storage(BoundUnitId::new(6));

        let foreign = contract(foreign, BoundDependencyRequirementKind::StorageAlive);

        for phases in [
            super::BoundCallDependencyContracts::new(
                foreign.clone(),
                result.clone(),
                Some(deferred.clone()),
            ),
            super::BoundCallDependencyContracts::new(
                invocation.clone(),
                foreign.clone(),
                Some(deferred.clone()),
            ),
            super::BoundCallDependencyContracts::new(invocation, result, Some(foreign)),
        ] {
            assert_eq!(
                table().try_with_calls([(expression.into(), phases)].into()),
                Err(DependencyContractsBuildError::ForeignContract),
            );
        }
    }

    fn expression_unit_with_storage(
        unit: BoundUnitId,
    ) -> (BoundUnit, BoundExpressionId, StoragePlan, StorageAccessId) {
        let (bound, expressions) = crate::test_support::expression_unit(unit, |tree, _| {
            vec![crate::test_support::push_expression(
                tree,
                crate::test_support::error_expression(),
            )]
        });

        let [expression] = expressions.as_slice() else {
            panic!("test unit must contain one expression");
        };

        let mut storage = StoragePlanBuilder::new(unit, bound.key().kind());

        let identity = storage
            .push_identity(StorageIdentity::Temporary(*expression))
            .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));

        let access = storage
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [],
                crate::test_support::error_type(),
                crate::test_support::source_anchor(),
                false,
            ))
            .unwrap_or_else(|error| panic!("test storage access must build: {error:?}"));

        (bound, *expression, storage.finish(), access)
    }
}
