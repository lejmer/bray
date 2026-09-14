use std::collections::BTreeSet;
use std::sync::Arc;

use bray_bound_tree::BoundOperator;
use bray_symbols::ConstantValueData;

/// A bounded, source-independent value used to compare entry and completion conditions.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionCondition {
    /// A checked Boolean value.
    Boolean(bool),
    /// A checked literal value.
    Literal(Arc<ConstantValueData>),
    /// The value of a supplied input before execution begins.
    Input(super::ExecutionPlace),
    /// The value produced by normal completion.
    Result,
    /// The value produced by one runtime expression, without inventing its contents.
    Expression(bray_bound_tree::BoundExpressionId),
    /// A retained receiver observed after one call completes normally.
    PostState(
        bray_bound_tree::BoundExpressionId,
        bray_bound_tree::BoundReferenceTarget,
    ),
    /// A selected built-in operation on other known values.
    Operation(BoundOperator, Arc<[ExecutionCondition]>),
    /// A checked predicate application, compared by declaration and selected arguments.
    Predicate(
        bray_symbols::PredicateDefinitionSymbolId,
        bray_symbols::GenericSubstitutionId,
        Arc<[ExecutionCondition]>,
    ),
    /// One field of an observed immutable value.
    Field(bray_symbols::AnySymbolId, Arc<ExecutionCondition>),
    /// Meaning that cannot currently be established.
    Unknown,
}

impl ExecutionCondition {
    // Bound recursive normalization and implication work, including adversarial source depth.
    pub(crate) const WORK_LIMIT: usize = bray_symbols::EXECUTION_CONDITION_WORK_LIMIT;

    pub(crate) fn inputs(&self) -> Vec<&super::ExecutionPlace> {
        let mut pending = vec![self];
        let mut inputs = Vec::new();

        while let Some(condition) = pending.pop() {
            match condition {
                Self::Input(place) => inputs.push(place),
                Self::Operation(_, operands) | Self::Predicate(_, _, operands) => {
                    pending.extend(operands.iter())
                }
                Self::Field(_, value) => pending.push(value),
                _ => {}
            }
        }

        inputs
    }

    pub(crate) fn substitute(
        &self,
        input: &impl Fn(&super::ExecutionPlace) -> Self,
        result: &Self,
        budget: &mut usize,
    ) -> Self {
        let Some(next) = budget.checked_sub(1) else {
            return Self::Unknown;
        };

        *budget = next;

        // Substitution retains immutable terms independently in the caller or exit environment.
        match self {
            Self::Predicate(predicate, substitution, operands) => Self::predicate(
                *predicate,
                *substitution,
                operands
                    .iter()
                    .map(|operand| operand.substitute(input, result, budget))
                    .collect(),
            ),
            Self::Input(reference) => input(reference),
            Self::Result => result.clone(),
            Self::Field(field, value) => {
                Self::field(*field, value.substitute(input, result, budget))
            }
            Self::Operation(operator, operands) => Self::operation(
                *operator,
                operands
                    .iter()
                    .map(|operand| operand.substitute(input, result, budget))
                    .collect(),
            ),
            _ => self.clone(),
        }
    }
    pub(crate) fn depends_on(&self, expression: bray_bound_tree::BoundExpressionId) -> bool {
        let mut pending = vec![self];

        while let Some(condition) = pending.pop() {
            match condition {
                Self::Expression(candidate) | Self::PostState(candidate, _)
                    if *candidate == expression =>
                {
                    return true;
                }
                Self::Operation(_, operands) | Self::Predicate(_, _, operands) => {
                    pending.extend(operands.iter())
                }
                Self::Field(_, value) => pending.push(value),
                _ => {}
            }
        }

        false
    }

    pub(crate) fn predicate(
        predicate: bray_symbols::PredicateDefinitionSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
        arguments: Vec<Self>,
    ) -> Self {
        if arguments
            .iter()
            .any(|argument| matches!(argument, Self::Unknown))
        {
            Self::Unknown
        } else {
            Self::Predicate(predicate, substitution, arguments.into())
        }
    }

    pub(crate) fn field(field: bray_symbols::AnySymbolId, value: Self) -> Self {
        match value {
            Self::Input(place) => Self::Input(place.field(field)),
            Self::Unknown => Self::Unknown,
            value => Self::Field(field, Arc::new(value)),
        }
    }

    pub(crate) fn operation(operator: BoundOperator, operands: Vec<Self>) -> Self {
        if operands
            .iter()
            .any(|operand| matches!(operand, Self::Unknown))
        {
            return Self::Unknown;
        }

        if let [Self::Literal(left), Self::Literal(right)] = operands.as_slice() {
            if matches!(
                operator,
                BoundOperator::Equal
                    | BoundOperator::NotEqual
                    | BoundOperator::Less
                    | BoundOperator::LessEqual
                    | BoundOperator::Greater
                    | BoundOperator::GreaterEqual
            ) {
                if let Ok(bray_symbols::ConstantValueKind::Boolean(value)) =
                    crate::constant::fold_binary(
                        operator,
                        left.kind(),
                        right.kind(),
                        crate::ConstantEvaluationLimits::default().integer_bits(),
                    )
                {
                    return Self::Boolean(value);
                }
            }
        }

        match (operator, operands.as_slice()) {
            (BoundOperator::Equal, [Self::Boolean(left), Self::Boolean(right)]) => {
                Self::Boolean(left == right)
            }
            (BoundOperator::NotEqual, [Self::Boolean(left), Self::Boolean(right)]) => {
                Self::Boolean(left != right)
            }
            (BoundOperator::LogicalNot, [Self::Boolean(value)]) => Self::Boolean(!value),
            (BoundOperator::LogicalNot, [Self::Operation(BoundOperator::LogicalNot, inner)]) => {
                inner.first().cloned().unwrap_or(Self::Unknown)
            }
            (BoundOperator::LogicalAnd, [Self::Boolean(left), Self::Boolean(right)]) => {
                Self::Boolean(*left && *right)
            }
            (BoundOperator::LogicalOr, [Self::Boolean(left), Self::Boolean(right)]) => {
                Self::Boolean(*left || *right)
            }
            _ => Self::Operation(operator, operands.into()),
        }
    }

    pub(crate) fn prove(
        &self,
        assumptions: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
    ) -> Option<bool> {
        *budget = budget.checked_sub(1)?;

        if matches!(self, Self::Unknown) {
            return None;
        }

        if let Self::Boolean(value) = self {
            return Some(*value);
        }

        // These owned keys share immutable operation operands with the proof input.
        if assumptions.contains(&(self.clone(), true)) {
            return Some(true);
        }

        if assumptions.contains(&(self.clone(), false)) {
            return Some(false);
        }

        let Self::Operation(operator, operands) = self else {
            return None;
        };

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
        match self {
            Self::Unknown => {}
            Self::Operation(BoundOperator::LogicalNot, operands) if operands.len() == 1 => {
                // The immutable operand is retained independently by the condition set.
                operands[0].clone().assume(!value, assumptions);
            }
            Self::Operation(operator, operands)
                if (operator == BoundOperator::LogicalAnd && value)
                    || (operator == BoundOperator::LogicalOr && !value) =>
            {
                for operand in operands.iter() {
                    // The immutable operand is retained independently by the condition set.
                    operand.clone().assume(value, assumptions);
                }
            }
            condition => {
                assumptions.insert((condition, value));
            }
        }
    }
}
