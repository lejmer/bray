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
        self.prove_mixed_with_implications(trusted, ordinary, budget, true)
    }

    fn prove_mixed_with_implications(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
        use_implications: bool,
    ) -> Option<bool> {
        *budget = budget.checked_sub(1)?;

        if self.is_unknown() {
            return None;
        }

        if let Self::Boolean(value) = self {
            return Some(*value);
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

        if let Self::Observation { owner: _, contents: value, .. } | Self::Borrowed(value) = self {
            return value.prove_mixed_with_implications(trusted, ordinary, budget, use_implications);
        }

        if let Self::Entry { condition, .. } = self {
            return condition.prove_mixed_with_implications(trusted, ordinary, budget, use_implications);
        }

        let proven = match self {
            Self::Trusted(_) => None,
            Self::Operation(BoundOperator::LogicalNot, operands)
                if let [operand] = operands.as_ref() =>
            {
                operand
                    .prove_mixed_with_implications(trusted, ordinary, budget, use_implications)
                    .map(|value| !value)
            }
            Self::Operation(BoundOperator::LogicalAnd, operands)
                if let [left, right] = operands.as_ref() =>
            {
                match (
                    left.prove_mixed_with_implications(trusted, ordinary, budget, use_implications),
                    right.prove_mixed_with_implications(trusted, ordinary, budget, use_implications),
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
                    left.prove_mixed_with_implications(trusted, ordinary, budget, use_implications),
                    right.prove_mixed_with_implications(trusted, ordinary, budget, use_implications),
                ) {
                    (Some(true), _) | (_, Some(true)) => Some(true),
                    (Some(false), Some(false)) => Some(false),
                    _ => None,
                }
            }
            _ => self
                .prove(trusted, budget)
                .or_else(|| self.prove(ordinary, budget)),
        };

        if proven.is_some() {
            return proven;
        }

        if use_implications && matches!(self, Self::Operation(BoundOperator::LogicalOr, _)) {
            for (premise, holds) in trusted.iter().chain(ordinary.iter().filter(|(condition, _)| !condition.contains_trusted())) {
                if *holds && matches!(premise, Self::Operation(BoundOperator::LogicalOr, _))
                    // Each implication has its own bounded proof. Unrelated clauses cannot
                    // exhaust the budget for an already established Boolean operand.
                    && premise.implies(self)
                {
                    return Some(true);
                }
            }
        }

        None
    }

    pub(crate) fn prove(
        &self,
        assumptions: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
    ) -> Option<bool> {
        *budget = budget.checked_sub(1)?;

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

        if let Self::Observation { owner: _, contents: value, .. } | Self::Borrowed(value) = self {
            return value.prove(assumptions, budget);
        }

        if let Self::Entry { condition, .. } = self {
            return condition.prove(assumptions, budget);
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

    /// Checks a branch against both direct evidence and retained Boolean alternatives.
    pub(crate) fn can_assume(&self, value: bool, assumptions: &BTreeSet<(Self, bool)>) -> bool {
        if let Some(known) = self.prove(assumptions, &mut { Self::WORK_LIMIT }) {
            return known == value;
        }

        let contrary = if value {
            Self::operation(BoundOperator::LogicalNot, vec![self.clone()])
        } else {
            self.clone()
        };

        // assume_proof decomposes true conjunctions and false disjunctions.
        // Their opposite outcomes retain the complete clause without choosing
        // an alternative, which can still exclude an incompatible branch.
        !assumptions.iter().any(|(condition, holds)| match condition {
            Self::Operation(BoundOperator::LogicalOr, _) if *holds => condition.implies(&contrary),
            Self::Operation(BoundOperator::LogicalAnd, _) if !*holds =>
                Self::operation(BoundOperator::LogicalNot, vec![condition.clone()]).implies(&contrary),
            _ => false,
        })
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
        if self.is_unknown() || consequence.is_unknown() {
            return false;
        }

        if self == consequence {
            return !self.contains_unknown();
        }

        let mut evidence = BTreeSet::new();

        self.clone().assume(true, &mut evidence);

        let ordinary = BTreeSet::new();
        let known = super::super::trusted_condition_closure(evidence.clone(), &ordinary);

        // Implication uses Boolean consequences directly, so premise checking cannot call itself.
        if consequence.prove_mixed_with_implications(&known, &ordinary, &mut { Self::WORK_LIMIT }, false) == Some(true) {
            return true;
        }

        evidence.clear();

        // Negating the requested clause exposes its Boolean path conditions.
        // Keep a negated trusted occurrence opaque: lack of that evidence must
        // never become an assumption that the ordinary predicate is false.
        consequence
            .clone()
            .assume_proof(false, false, &mut |condition, holds| {
                evidence.insert((condition, holds));
            });

        self.prove(&evidence, &mut { Self::WORK_LIMIT }) == Some(false)
    }

    pub(crate) fn assume(self, value: bool, assumptions: &mut BTreeSet<(Self, bool)>) {
        self.assume_with(value, &mut |condition, value| {
            assumptions.insert((condition, value));
        });
    }

    pub(crate) fn assume_with(self, value: bool, insert: &mut impl FnMut(Self, bool)) {
        self.assume_proof(value, true, insert);
    }

    fn assume_proof(
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
