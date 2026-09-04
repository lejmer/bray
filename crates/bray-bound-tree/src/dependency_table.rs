use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_symbols::AnyLocalSymbolId;

use crate::{
    BorrowCapabilityId, BoundBlockItem, BoundDependencyContract, BoundDependencyContractId,
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundUnit, BoundUnitId,
    BoundUnitKind, StorageAccessId, StoragePlan,
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
    unit: BoundUnitId,
    kind: BoundUnitKind,
    contracts: Arc<[BoundDependencyContract]>,
    expressions: Arc<[ContractEntry<BoundExpressionId>]>,
    deferred_expressions: Arc<[ContractEntry<BoundExpressionId>]>,
    accesses: Arc<[ContractEntry<StorageAccessId>]>,
    borrows: Arc<[ContractEntry<BorrowCapabilityId>]>,
    is_recovered: bool,
}

impl CheckedDependencyContracts {
    /// Validates and creates one normalized per-unit dependency table.
    pub fn try_new(
        unit: &BoundUnit,
        storage: &StoragePlan,
        expressions: impl IntoIterator<Item = (BoundExpressionId, BoundDependencyContract)>,
        deferred_expressions: impl IntoIterator<Item = (BoundExpressionId, BoundDependencyContract)>,
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
                expression.unit() == unit.unit() && unit.view().expression(expression).is_some()
            },
            DependencyContractsBuildError::InvalidExpression,
        )?;

        let deferred_expressions = build_entries(
            unit.unit(),
            deferred_expressions,
            &mut contracts,
            &mut contract_ids,
            |expression| {
                expression.unit() == unit.unit() && unit.view().expression(expression).is_some()
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

        Ok(Self {
            unit: unit.unit(),
            kind: unit.key().kind(),
            contracts: contracts.into(),
            expressions: expressions.into(),
            deferred_expressions: deferred_expressions.into(),
            accesses: accesses.into(),
            borrows: borrows.into(),
            is_recovered,
        })
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

    /// Returns the contract carried by one expression result.
    pub fn expression(&self, expression: BoundExpressionId) -> Option<BoundDependencyContractId> {
        find_contract(&self.expressions, expression)
    }

    /// Returns the contract required when one lazy expression is executed.
    pub fn deferred_expression(
        &self,
        expression: BoundExpressionId,
    ) -> Option<BoundDependencyContractId> {
        find_contract(&self.deferred_expressions, expression)
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

        let initializers = local_initializers(unit);
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
            .map(|entry| *entry.occurrence())
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
                .all(|contract| contract.exists_in(storage))
    }

    /// Returns whether semantic recovery contributed to the table.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

fn local_initializers(unit: &BoundUnit) -> BTreeMap<AnyLocalSymbolId, BoundExpressionId> {
    let mut initializers = BTreeMap::new();

    for (_, block) in unit.tree().blocks() {
        for item in block.items() {
            match item {
                BoundBlockItem::LocalBinding(binding) => {
                    for symbol in binding.bindings() {
                        initializers.insert((*symbol).into(), binding.initializer());
                    }
                }
                BoundBlockItem::LocalConstant(constant) => {
                    if let Some(symbol) = constant.symbol() {
                        initializers.insert(symbol.into(), constant.initializer());
                    }
                }
                BoundBlockItem::Expression(_) => {}
            }
        }
    }

    initializers
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
            [(expression, contract.clone())],
            [(expression, deferred)],
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
                    BoundExpressionId::from_slot(BoundUnitId::new(4), 0),
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
            [(expression, contract.clone())],
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
