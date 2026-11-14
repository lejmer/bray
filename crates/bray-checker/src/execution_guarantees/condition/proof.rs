use crate::execution_guarantees::ExecutionAssumptions;
use super::{ExecutionCondition, ExecutionProof, ExecutionProofCache};
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
        memoized(memo.map(|memo| &memo.queries), self, budget, |budget| self.prove_mixed_inner(trusted, ordinary, budget, implications, memo))
    }

    fn prove_mixed_inner(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
        implications: Option<&ExecutionProof<'_>>,
        memo: Option<&ProofMemo>,
    ) -> Option<bool> {
        *budget = budget.checked_sub(1)?;

        if self.is_unknown() {
            return None;
        }

        if let Some(value) = self.explicit_value(trusted, Some(ordinary), memo) {
            return Some(value);
        }

        if let Self::Observation { owner: _, contents: value, .. } | Self::Borrowed(value) = self {
            return value.prove_mixed_with_implications(trusted, ordinary, budget, implications, memo);
        }

        if let Self::Entry { condition, .. } = self {
            return condition.prove_mixed_with_implications(trusted, ordinary, budget, implications, memo);
        }

        let proven = match self {
            Self::Trusted(_) => None,
            Self::Operation(BoundOperator::LogicalNot, operands)
                if let [operand] = operands.as_ref() =>
            {
                operand
                    .prove_mixed_with_implications(trusted, ordinary, budget, implications, memo)
                    .map(|value| !value)
            }
            Self::Operation(operator @ (BoundOperator::LogicalAnd | BoundOperator::LogicalOr), operands)
                if let [left, right] = operands.as_ref() =>
            {
                prove_boolean(*operator,
                    left.prove_mixed_with_implications(trusted, ordinary, budget, implications, memo),
                    || right.prove_mixed_with_implications(trusted, ordinary, budget, implications, memo))
            }
            _ => {
                let single = memo.map(ProofMemo::single_views);

                self.prove_with_memo(trusted, budget, single.map(|[trusted, _]| trusted))
                    .or_else(|| self.prove_with_memo(ordinary, budget, single.map(|[_, ordinary]| ordinary)))
            }
        };

        if proven.is_some() {
            return proven;
        }

        if let Some(implications) = implications
            && matches!(self, Self::Operation(BoundOperator::LogicalOr, _)) {
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
        memoized(memo.map(|memo| &memo.queries), self, budget, |budget| self.prove_inner(assumptions, budget, memo))
    }

    fn prove_inner(
        &self,
        assumptions: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
        memo: Option<&ProofMemo>,
    ) -> Option<bool> {
        *budget = budget.checked_sub(1)?;

        if self.is_unknown() {
            return None;
        }

        if let Some(value) = self.explicit_value(assumptions, None, memo) {
            return Some(value);
        }

        if let Self::Observation { owner: _, contents: value, .. } | Self::Borrowed(value) = self {
            return value.prove_with_memo(assumptions, budget, memo);
        }

        if let Self::Entry { condition, .. } = self {
            return condition.prove_with_memo(assumptions, budget, memo);
        }

        if let Self::ValueAlias(left, right) = self {
            return alias_value(left.prove_with_memo(assumptions, budget, memo), || right.prove_with_memo(assumptions, budget, memo));
        }

        if let Self::VariantTest(variant, subject) = self {
            return assumptions.iter().any(|(condition, value)| {
                *value && matches!(condition, Self::VariantTest(active, other) if active != variant && other == subject)
            }).then_some(false);
        }

        let Self::Operation(operator, operands) = self else {
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
            let inverse = Self::Operation(inverse, operands.clone());

            for value in [true, false] {
                if assumptions.contains(&(inverse.clone(), value)) {
                    return Some(!value);
                }
            }
        }

        match (operator, operands.as_ref()) {
            (BoundOperator::LogicalNot, [operand]) => {
                operand.prove_with_memo(assumptions, budget, memo).map(|value| !value)
            }
            (operator @ (BoundOperator::LogicalAnd | BoundOperator::LogicalOr), [left, right]) =>
                prove_boolean(*operator, left.prove_with_memo(assumptions, budget, memo), || right.prove_with_memo(assumptions, budget, memo)),
            _ => None,
        }
    }

    pub(super) fn explicit_value(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: Option<&BTreeSet<(Self, bool)>>,
        memo: Option<&ProofMemo>,
    ) -> Option<bool> {
        if let Self::Boolean(value) = self { return Some(*value); }
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

            let Some(remaining) = budget.checked_sub(1) else { return ExecutionCondition::Boolean(true); };

            *budget = remaining;

            let value = match condition {
                ExecutionCondition::Boolean(value) => ExecutionCondition::Boolean(*value == holds),
                ExecutionCondition::Operation(BoundOperator::LogicalNot, operands) if operands.len() == 1 =>
                    visit(&operands[0], !holds, map, budget, known),
                ExecutionCondition::Operation(operator @ (BoundOperator::LogicalAnd | BoundOperator::LogicalOr), operands) => {
                    let operator = if holds { *operator } else if *operator == BoundOperator::LogicalAnd {
                        BoundOperator::LogicalOr
                    } else { BoundOperator::LogicalAnd };

                    ExecutionCondition::operation(operator, operands.iter()
                        .map(|operand| visit(operand, holds, map, budget, known)).collect())
                }
                condition => map(condition).map_or(ExecutionCondition::Boolean(true), |condition| if holds { condition } else {
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

        match self {
            Self::Unknown => {}
            condition @ (Self::Observation { .. } | Self::Borrowed(_)) => {
                let contents = condition.computed_value().clone();

                if !condition.contains_unknown() {
                    insert(condition, value);
                }

                contents.assume_proof(value, expose_trusted_truth, insert);
            }
            Self::Entry {
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
            Self::Trusted(condition) => {
                // Keep the occurrence qualifier for contract implication and authority-bearing values.
                if !condition.contains_unknown() {
                    insert(Self::Trusted(condition.clone()), value);
                }

                if expose_trusted_truth {
                    condition.as_ref().clone().assume_proof(value, true, insert);
                }
            }
            Self::Operation(BoundOperator::LogicalNot, operands) if operands.len() == 1 => {
                // The immutable operand is retained independently by the condition set.
                operands[0]
                    .clone()
                    .assume_proof(!value, expose_trusted_truth, insert);
            }
            Self::Operation(operator, operands)
                if (operator == BoundOperator::LogicalAnd && value)
                    || (operator == BoundOperator::LogicalOr && !value) =>
            {
                for operand in operands.iter() {
                    // The immutable operand is retained independently by the condition set.
                    operand
                        .clone()
                        .assume_proof(value, expose_trusted_truth, insert);
                }
            }
            condition if !condition.contains_unknown() => {
                insert(condition, value);
            }
            _ => {}
        }
    }
}
