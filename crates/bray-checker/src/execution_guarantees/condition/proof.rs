use super::ExecutionCondition;
use bray_bound_tree::BoundOperator;
use std::collections::BTreeSet;

impl ExecutionCondition {
    pub(crate) fn prove_trusted(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
    ) -> Option<bool> {
        if let Some(value) = self.prove(trusted, &mut { Self::WORK_LIMIT }) {
            return Some(value);
        }

        let known = crate::execution_guarantees::trusted_implication::trusted_condition_closure(
            trusted.clone(),
            ordinary,
        );

        self.prove_mixed(&known, ordinary, &mut { Self::WORK_LIMIT })
    }

    pub(crate) fn prove_mixed(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
    ) -> Option<bool> {
        *budget = budget.checked_sub(1)?;

        if self.is_unknown() {
            return None;
        }

        if let Self::Boolean(value) = self {
            return Some(*value);
        }

        if let Self::Entry { condition, .. } = self {
            return condition.prove_mixed(trusted, ordinary, budget);
        }

        // Test exact evidence once per Boolean node. Recursively proving its entire
        // subtree at each level makes a chain of clauses require quadratic work.
        for value in [true, false] {
            if (trusted.contains(&(self.clone(), value))
                || (ordinary.contains(&(self.clone(), value)) && !self.contains_trusted()))
                && !self.contains_unknown()
            {
                return Some(value);
            }
        }

        match self {
            Self::Trusted(_) => None,
            Self::Operation(BoundOperator::LogicalNot, operands)
                if let [operand] = operands.as_ref() =>
            {
                operand
                    .prove_mixed(trusted, ordinary, budget)
                    .map(|value| !value)
            }
            Self::Operation(BoundOperator::LogicalAnd, operands)
                if let [left, right] = operands.as_ref() =>
            {
                match (
                    left.prove_mixed(trusted, ordinary, budget),
                    right.prove_mixed(trusted, ordinary, budget),
                ) {
                    (Some(false), _) | (_, Some(false)) => Some(false),
                    (Some(true), Some(true)) => Some(true),
                    _ => None,
                }
            }
            Self::Operation(BoundOperator::LogicalOr, operands)
                if let [left, right] = operands.as_ref() =>
            {
                match (
                    left.prove_mixed(trusted, ordinary, budget),
                    right.prove_mixed(trusted, ordinary, budget),
                ) {
                    (Some(true), _) | (_, Some(true)) => Some(true),
                    (Some(false), Some(false)) => Some(false),
                    _ => None,
                }
            }
            _ => self
                .prove(trusted, budget)
                .or_else(|| self.prove(ordinary, budget)),
        }
    }

    pub(crate) fn prove(
        &self,
        assumptions: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
    ) -> Option<bool> {
        *budget = budget.checked_sub(1)?;

        if let Self::Entry { condition, .. } = self {
            return condition.prove(assumptions, budget);
        }

        if self.is_unknown() {
            return None;
        }

        if let Self::Boolean(value) = self {
            return Some(*value);
        }

        // These owned keys share immutable operation operands with the proof input.
        if assumptions.contains(&(self.clone(), true)) && !self.contains_unknown() {
            return Some(true);
        }

        if assumptions.contains(&(self.clone(), false)) && !self.contains_unknown() {
            return Some(false);
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
        ) {
            return Some(value);
        }

        let inverse = match operator {
            BoundOperator::Equal => Some(BoundOperator::NotEqual),
            BoundOperator::NotEqual => Some(BoundOperator::Equal),
            _ => None,
        };

        if let Some(inverse) = inverse {
            let inverse = Self::Operation(inverse, operands.clone());

            for value in [true, false] {
                if assumptions.contains(&(inverse.clone(), value)) {
                    return Some(!value);
                }
            }
        }

        match (operator, operands.as_ref()) {
            (BoundOperator::LogicalNot, [operand]) => {
                operand.prove(assumptions, budget).map(|value| !value)
            }
            (BoundOperator::LogicalAnd, [left, right]) => match (
                left.prove(assumptions, budget),
                right.prove(assumptions, budget),
            ) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            },
            (BoundOperator::LogicalOr, [left, right]) => match (
                left.prove(assumptions, budget),
                right.prove(assumptions, budget),
            ) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    pub(crate) fn assume(self, value: bool, assumptions: &mut BTreeSet<(Self, bool)>) {
        self.assume_with(value, &mut |condition, value| {
            assumptions.insert((condition, value));
        });
    }

    pub(crate) fn assume_with(self, value: bool, insert: &mut impl FnMut(Self, bool)) {
        if self.is_unknown() {
            return;
        }

        match self {
            Self::Unknown => {}
            Self::Trusted(condition) => {
                // Keep the occurrence qualifier for contract implication and authority-bearing values.
                if !condition.contains_unknown() {
                    insert(Self::Trusted(condition.clone()), value);
                }

                condition.as_ref().clone().assume_with(value, insert);
            }
            Self::Operation(BoundOperator::LogicalNot, operands) if operands.len() == 1 => {
                // The immutable operand is retained independently by the condition set.
                operands[0].clone().assume_with(!value, insert);
            }
            Self::Operation(operator, operands)
                if (operator == BoundOperator::LogicalAnd && value)
                    || (operator == BoundOperator::LogicalOr && !value) =>
            {
                for operand in operands.iter() {
                    // The immutable operand is retained independently by the condition set.
                    operand.clone().assume_with(value, insert);
                }
            }
            condition if !condition.contains_unknown() => {
                insert(condition, value);
            }
            _ => {}
        }
    }
}
