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

/// An exact input-to-output reference relation of a selected call.
/// Address formation locates storage without equating a pointer with the borrowed value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundCallReference {
    relation: bray_symbols::DependencyBorrowRelation,
    preserves_value: bool,
    addressed_type: Option<bray_symbols::TypeId>,
}

impl BoundCallReference {
    /// Records whether the reference forwards an input value or forms an address.
    pub const fn new(
        relation: bray_symbols::DependencyBorrowRelation,
        preserves_value: bool,
        addressed_type: Option<bray_symbols::TypeId>,
    ) -> Self {
        Self {
            relation,
            preserves_value,
            addressed_type,
        }
    }

    /// Returns the checked formal reference relation.
    pub const fn relation(&self) -> &bray_symbols::DependencyBorrowRelation {
        &self.relation
    }

    /// The source storage type when the result forms an address instead of forwarding a reference.
    pub const fn addressed_type(&self) -> Option<bray_symbols::TypeId> {
        self.addressed_type
    }

    /// Whether the output retains the input reference value, rather than only its target.
    pub const fn preserves_value(&self) -> bool {
        self.preserves_value
    }
}

/// The distinct invocation, returned-value and deferred requirements of one selected call.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundCallDependencyContracts {
    input_count: usize,
    carriers: Arc<[bray_symbols::DependencyWitnessCarrier]>,
    invocation: BoundDependencyContract,
    result: BoundDependencyContract,
    deferred: Option<BoundDependencyContract>,
    references: Vec<BoundCallReference>,
    effects: Arc<[bray_symbols::DependencyStorageEffect]>,
    retained_inputs: Option<BTreeSet<BoundExpressionId>>,
    reference_paths: BTreeMap<bray_symbols::DependencySubject, crate::StorageReferencePath>,
    normal_completion: bool,
    deferred_effects: Option<Arc<[bray_symbols::DependencyStorageEffect]>>,
    deferred_completion: bool,
}

impl BoundCallDependencyContracts {
    /// Creates the instantiated phase contracts for a selected call.
    pub fn new(
        invocation: BoundDependencyContract,
        result: BoundDependencyContract,
        deferred: Option<BoundDependencyContract>,
    ) -> Self {
        Self {
            invocation,
            result,
            carriers: Arc::new([]),
            input_count: 0,
            deferred,
            references: Vec::new(),
            effects: Arc::new([bray_symbols::DependencyStorageEffect::Unknown]),
            retained_inputs: None,
            reference_paths: BTreeMap::new(),
            normal_completion: true,
            deferred_effects: None,
            deferred_completion: true,
        }
    }

    /// Retains receiver-first formal arity for portable completion conditions.
    pub const fn with_input_count(mut self, count: usize) -> Self {
        self.input_count = count;

        self
    }

    /// Returns the ordinal reserved for the completion result.
    pub const fn input_count(&self) -> usize { self.input_count }

    /// Retains the returned carrier paths established by the selected callable.
    pub fn with_carriers(mut self, carriers: impl IntoIterator<Item = bray_symbols::DependencyWitnessCarrier>) -> Self {
        self.carriers = carriers.into_iter().collect();

        self
    }

    /// Returns complete owners supporting the callable's trusted completion clauses.
    pub fn carriers(&self) -> &[bray_symbols::DependencyWitnessCarrier] { &self.carriers }

    /// Records whether this invocation phase can produce a normal result.
    pub fn with_normal_completion(mut self, possible: bool) -> Self {
        self.normal_completion = possible;

        self
    }

    /// Whether normal completion is possible for this selected invocation phase.
    pub const fn normal_completion(&self) -> bool {
        self.normal_completion
    }

    /// Records the body effects and outcome separately from lazy frame creation.
    pub fn with_deferred_execution(
        mut self,
        effects: impl IntoIterator<Item = bray_symbols::DependencyStorageEffect>,
        normal_completion: bool,
    ) -> Self {
        self.deferred_effects = Some(bray_symbols::DependencyStorageEffect::normalize(effects));
        self.deferred_completion = normal_completion;

        self
    }

    /// Body effects applied when this lazy invocation is driven.
    pub fn deferred_effects(&self) -> Option<&[bray_symbols::DependencyStorageEffect]> {
        self.deferred_effects.as_deref()
    }

    /// Whether driving this invocation can return normally.
    pub const fn deferred_completion(&self) -> bool {
        self.deferred_completion
    }

    /// Shares checked access paths between reference transfers and mutation effects.
    pub fn with_reference_paths(
        mut self,
        paths: BTreeMap<bray_symbols::DependencySubject, crate::StorageReferencePath>,
    ) -> Self {
        self.reference_paths = paths;

        self
    }

    /// Returns the input subjects whose reference paths were prepared by call analysis.
    pub fn reference_subjects(&self) -> impl Iterator<Item = &bray_symbols::DependencySubject> {
        self.reference_paths.keys()
    }

    /// Returns the selected input path prepared by type analysis.
    pub fn reference_path(
        &self,
        subject: &bray_symbols::DependencySubject,
    ) -> Option<&crate::StorageReferencePath> {
        self.reference_paths.get(subject)
    }

    /// Records the dependencies carried by the returned value.
    pub fn with_result(mut self, result: BoundDependencyContract) -> Self {
        self.result = result;

        self
    }

    /// Records reference transfers and the inputs retained by a returned witness.
    pub fn with_references(
        mut self,
        references: Vec<BoundCallReference>,
        retained_inputs: Option<BTreeSet<BoundExpressionId>>,
    ) -> Self {
        self.references = references;
        self.retained_inputs = retained_inputs;

        self
    }

    /// Retains the selected callable's checked formal mutation effects.
    pub fn with_effects(
        mut self,
        effects: impl IntoIterator<Item = bray_symbols::DependencyStorageEffect>,
    ) -> Self {
        self.effects = bray_symbols::DependencyStorageEffect::normalize(effects);

        self
    }

    /// Returns the complete may-mutation summary for this invocation.
    pub fn effects(&self) -> &[bray_symbols::DependencyStorageEffect] {
        &self.effects
    }

    /// Returns exact reference transfers on normal completion.
    pub fn references(&self) -> &[BoundCallReference] {
        &self.references
    }

    /// Returns retained witness inputs when the returned value has a checked dependency source.
    pub const fn retained_inputs(&self) -> Option<&BTreeSet<BoundExpressionId>> {
        self.retained_inputs.as_ref()
    }

    /// Returns the requirements for beginning the invocation.
    pub const fn invocation(&self) -> &BoundDependencyContract {
        &self.invocation
    }

    /// Returns the dependencies retained by the result after completion.
    pub const fn result(&self) -> &BoundDependencyContract {
        &self.result
    }

    /// Returns the requirements for driving deferred execution, when present.
    pub const fn deferred(&self) -> Option<&BoundDependencyContract> {
        self.deferred.as_ref()
    }

    fn phase_contracts(&self) -> impl Iterator<Item = &BoundDependencyContract> {
        [&self.invocation, &self.result]
            .into_iter()
            .chain(self.deferred.as_ref())
    }

    fn is_valid_for(&self, unit: BoundUnitId) -> bool {
        self.phase_contracts()
            .all(|contract| contract.is_valid_for(unit))
            && self
                .retained_inputs
                .iter()
                .flatten()
                .all(|input| input.unit() == unit)
    }
}

/// Durable dependency contracts for values, accesses, and borrows in one checked unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedDependencyContracts {
    projection_variants: BTreeMap<bray_symbols::UnionPayloadFieldSymbolId, bray_symbols::UnionVariantSymbolId>,
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
    /// Retains declaration relationships resolved before dependency flow.
    pub fn with_projection_variants(mut self, variants: BTreeMap<bray_symbols::UnionPayloadFieldSymbolId, bray_symbols::UnionVariantSymbolId>) -> Self {
        self.projection_variants = variants;
        self
    }

    /// Returns the checked variant owning a projected payload field.
    pub fn projection_variant(&self, field: bray_symbols::UnionPayloadFieldSymbolId) -> bray_symbols::UnionVariantSymbolId {
        *self.projection_variants.get(&field).expect("dependency preparation retains every projected union field")
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
                    .retained_inputs
                    .iter()
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

        let retained = Some(std::collections::BTreeSet::from([expression]));
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
                        Some(std::collections::BTreeSet::from([missing]))
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
