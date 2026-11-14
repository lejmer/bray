use crate::execution_guarantees::ExecutionAssumptions;
use super::{ExecutionCondition, ExecutionConditionKind as K, ExecutionProof, ExecutionProofCache, PreparedCondition};
use super::context::ProofMemo;
use super::memo::memoized;
use crate::execution_guarantees::boolean::{alias_value, prove_boolean};
use bray_bound_tree::BoundOperator;
use std::collections::BTreeSet;

impl ExecutionCondition {
    pub(crate) fn prove_ordinary(&self, ordinary: &ExecutionAssumptions, cache: Option<&ExecutionProofCache>) -> Option<bool> {
        let trusted = BTreeSet::new();

        let proof = cache.map_or_else(|| ExecutionProof::new(&trusted, ordinary), |cache| cache.ordinary_proof(ordinary));

        proof.prove_direct(self, &mut { Self::WORK_LIMIT })
            .or_else(|| proof.prove_mixed(self, &mut { Self::WORK_LIMIT }))
    }

    pub(crate) fn prove_trusted(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
    ) -> Option<bool> {
        ExecutionProof::new(trusted, ordinary).prove_trusted(self)
    }

    pub(crate) fn prove_mixed(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
    ) -> Option<bool> {
        ExecutionProof::new(trusted, ordinary).prove_mixed(self, budget)
    }

    pub(super) fn prove_mixed_with_implications(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
        implications: Option<&ExecutionProof<'_>>,
        memo: Option<&ProofMemo>,
    ) -> Option<bool> {
        if *budget == 0 { return None; }

        let local;
        let memo = if let Some(memo) = memo { memo } else { local = ProofMemo::new(); &local };

        memoized(Some(&memo.queries), self, budget, |budget| self.prove_mixed_inner(trusted, ordinary, budget, implications, memo))
    }

    fn prove_mixed_inner(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
        implications: Option<&ExecutionProof<'_>>,
        memo: &ProofMemo,
    ) -> Option<bool> {
        *budget = budget.checked_sub(1)?;

        if self.is_unknown() {
            return None;
        }

        if let Some(value) = self.explicit_value(trusted, Some(ordinary), Some(memo)) {
            return Some(value);
        }

        if let K::Observation { owner: _, contents: value, .. } | K::Borrowed(value) = self.kind() {
            return value.prove_mixed_with_implications(trusted, ordinary, budget, implications, Some(memo));
        }

        if let K::Entry { condition, .. } = self.kind() {
            return condition.prove_mixed_with_implications(trusted, ordinary, budget, implications, Some(memo));
        }

        let proven = match self.kind() {
            K::Trusted(_) => None,
            K::Operation(BoundOperator::LogicalNot, operands)
                if let [operand] = operands.as_ref() =>
            {
                operand
                    .prove_mixed_with_implications(trusted, ordinary, budget, implications, Some(memo))
                    .map(|value| !value)
            }
            K::Operation(operator @ (BoundOperator::LogicalAnd | BoundOperator::LogicalOr), operands)
                if let [left, right] = operands.as_ref() =>
            {
                prove_boolean(*operator,
                    left.prove_mixed_with_implications(trusted, ordinary, budget, implications, Some(memo)),
                    || right.prove_mixed_with_implications(trusted, ordinary, budget, implications, Some(memo)))
            }
            _ => {
                let [trusted_memo, ordinary_memo] = memo.single_views();

                self.prove_with_memo(trusted, budget, Some(trusted_memo))
                    .or_else(|| self.prove_with_memo(ordinary, budget, Some(ordinary_memo)))
            }
        };

        if proven.is_some() {
            return proven;
        }

        if let Some(implications) = implications
            && matches!(self.kind(), K::Operation(BoundOperator::LogicalOr, _)) {
            return implications.proves_alternative(self).then_some(true);
        }

        None
    }

    pub(crate) fn prove(
        &self,
        assumptions: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
    ) -> Option<bool> {
        self.prove_with_memo(assumptions, budget, None)
    }

    pub(in crate::execution_guarantees) fn prove_with_memo(
        &self,
        assumptions: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
        memo: Option<&ProofMemo>,
    ) -> Option<bool> {
        if *budget == 0 { return None; }

        let local;
        let memo = if let Some(memo) = memo { memo } else { local = ProofMemo::new(); &local };

        memoized(Some(&memo.queries), self, budget, |budget| self.prove_inner(assumptions, budget, memo))
    }

    fn prove_inner(
        &self,
        assumptions: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
        memo: &ProofMemo,
    ) -> Option<bool> {
        *budget = budget.checked_sub(1)?;

        if self.is_unknown() {
            return None;
        }

        if let Some(value) = self.explicit_value(assumptions, None, Some(memo)) {
            return Some(value);
        }

        if let K::Observation { owner: _, contents: value, .. } | K::Borrowed(value) = self.kind() {
            return value.prove_with_memo(assumptions, budget, Some(memo));
        }

        if let K::Entry { condition, .. } = self.kind() {
            return condition.prove_with_memo(assumptions, budget, Some(memo));
        }

        if let K::ValueAlias(left, right) = self.kind() {
            return alias_value(left.prove_with_memo(assumptions, budget, Some(memo)), || right.prove_with_memo(assumptions, budget, Some(memo)));
        }

        if let K::VariantTest(variant, subject) = self.kind() {
            return assumptions.iter().any(|(condition, value)| {
                *value && matches!(condition.kind(), K::VariantTest(active, other) if active != variant && other == subject)
            }).then_some(false);
        }

        let K::Operation(operator, operands) = self.kind() else {
            return None;
        };

        if let Some(value) = crate::execution_guarantees::implication::comparison_is_implied(
            *operator,
            operands,
            assumptions,
            budget,
            memo,
        ) {
            return Some(value);
        }

        if let Some(inverse) = operator.equality_complement() {
            let inverse = PreparedCondition::new(K::Operation(inverse, operands.clone()));

            for value in [true, false] {
                if memo.raw_contains(&inverse, value, assumptions) {
                    return Some(!value);
                }
            }
        }

        match (operator, operands.as_ref()) {
            (BoundOperator::LogicalNot, [operand]) => {
                operand.prove_with_memo(assumptions, budget, Some(memo)).map(|value| !value)
            }
            (operator @ (BoundOperator::LogicalAnd | BoundOperator::LogicalOr), [left, right]) =>
                prove_boolean(*operator, left.prove_with_memo(assumptions, budget, Some(memo)), || right.prove_with_memo(assumptions, budget, Some(memo))),
            _ => None,
        }
    }

    pub(super) fn explicit_value(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: Option<&BTreeSet<(Self, bool)>>,
        memo: Option<&ProofMemo>,
    ) -> Option<bool> {
        if let K::Boolean(value) = self.kind() { return Some(*value); }
        if let Some(memo) = memo { return memo.explicit_value(self, trusted, ordinary); }

        // Lookup keys share the proof input's immutable operands.
        for value in [true, false] {
            if trusted.contains(&(self.clone(), value))
                || ordinary.is_some_and(|ordinary| ordinary.contains(&(self.clone(), value)) && !self.contains_trusted())
            {
                return (!self.contains_unknown()).then_some(value);
            }
        }

        None
    }

    /// Checks a branch against both direct evidence and retained Boolean alternatives.
    pub(crate) fn can_assume(&self, value: bool, assumptions: &BTreeSet<(Self, bool)>) -> bool {
        ExecutionProof::new(&BTreeSet::new(), assumptions).can_assume(self, value)
    }

    /// Maps equivalent Boolean atoms, forgetting unsupported ones without strengthening the clause.
    pub(crate) fn boolean_consequence(
        &self,
        holds: bool,
        map: &mut impl FnMut(&Self) -> Option<Self>,
        budget: &mut usize,
    ) -> Self {
        fn visit<'a>(
            condition: &'a ExecutionCondition,
            holds: bool,
            map: &mut impl FnMut(&ExecutionCondition) -> Option<ExecutionCondition>,
            budget: &mut usize,
            known: &mut std::collections::BTreeMap<(&'a ExecutionCondition, bool), ExecutionCondition>,
        ) -> ExecutionCondition {
            if let Some(value) = known.get(&(condition, holds)) { return value.clone(); }

            let Some(remaining) = budget.checked_sub(1) else { return ExecutionCondition::boolean(true); };

            *budget = remaining;

            let value = match condition.kind() {
                K::Boolean(value) => ExecutionCondition::boolean(*value == holds),
                K::Operation(BoundOperator::LogicalNot, operands) if operands.len() == 1 =>
                    visit(&operands[0], !holds, map, budget, known),
                K::Operation(operator @ (BoundOperator::LogicalAnd | BoundOperator::LogicalOr), operands) => {
                    let operator = if holds { *operator } else if *operator == BoundOperator::LogicalAnd {
                        BoundOperator::LogicalOr
                    } else { BoundOperator::LogicalAnd };

                    ExecutionCondition::operation(operator, operands.iter()
                        .map(|operand| visit(operand, holds, map, budget, known)).collect())
                }
                _ => map(condition).map_or_else(|| ExecutionCondition::boolean(true), |condition| if holds { condition } else {
                    ExecutionCondition::operation(BoundOperator::LogicalNot, vec![condition])
                }),
            };

            known.insert((condition, holds), value.clone());

            value
        }

        visit(self, holds, map, budget, &mut Default::default())
    }

    /// Compare carrier promises without turning ordinary truth into trusted evidence.
    pub(crate) fn implies(&self, consequence: &Self) -> bool {
        ExecutionProof::implies(self, consequence)
    }

    pub(crate) fn assume(self, value: bool, assumptions: &mut impl Extend<(Self, bool)>) {
        self.assume_with(value, &mut |condition, value| {
            assumptions.extend(std::iter::once((condition, value)));
        });
    }

    pub(crate) fn assume_with(self, value: bool, insert: &mut impl FnMut(Self, bool)) {
        self.assume_proof(value, true, insert);
    }

    pub(in crate::execution_guarantees) fn assume_proof(
        self,
        value: bool,
        expose_trusted_truth: bool,
        insert: &mut impl FnMut(Self, bool),
    ) {
        if self.is_unknown() {
            return;
        }

        match self.kind() {
            K::Unknown => {}
            K::Observation { .. } | K::Borrowed(_) => {
                let contents = self.computed_value().clone();

                if !self.contains_unknown() {
                    insert(self.clone(), value);
                }

                contents.assume_proof(value, expose_trusted_truth, insert);
            }
            K::Entry {
                condition,
                captured: true,
            } if !condition.contains_trusted() && !condition.observes_live(None) => {
                // Instantiation has already frozen these ordinary input values.
                // Use the same value evidence that proof reads, without promoting
                // historical authority or observations of live borrowed storage.
                condition
                    .as_ref()
                    .clone()
                    .assume_proof(value, expose_trusted_truth, insert);
            }
            K::Trusted(condition) => {
                // Keep the occurrence qualifier for contract implication and authority-bearing values.
                if !condition.contains_unknown() {
                    insert(self.clone(), value);
                }

                if expose_trusted_truth {
                    condition.as_ref().clone().assume_proof(value, true, insert);
                }
            }
            K::Operation(BoundOperator::LogicalNot, operands) if operands.len() == 1 => {
                // The immutable operand is retained independently by the condition set.
                operands[0]
                    .clone()
                    .assume_proof(!value, expose_trusted_truth, insert);
            }
            K::Operation(operator, operands)
                if (*operator == BoundOperator::LogicalAnd && value)
                    || (*operator == BoundOperator::LogicalOr && !value) =>
            {
                for operand in operands.iter() {
                    // The immutable operand is retained independently by the condition set.
                    operand
                        .clone()
                        .assume_proof(value, expose_trusted_truth, insert);
                }
            }
            _ if !self.contains_unknown() => {
                insert(self.clone(), value);
            }
            _ => {}
        }
    }
}
