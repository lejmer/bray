use std::collections::{BTreeMap, BTreeSet};
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
    /// A checked structural value retaining the values placed in its actual storage components.
    Constructed(bray_bound_tree::BoundExpressionId, Arc<[(bray_bound_tree::StorageProjection, ExecutionCondition)]>),
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
    /// A checked constant callable application with stable declaration and argument identity.
    Call(bray_symbols::CallableInstanceData, Arc<[ExecutionCondition]>),
    /// One field of an observed immutable value.
    Field(bray_symbols::AnySymbolId, Arc<ExecutionCondition>),
    /// Meaning that cannot currently be established.
    Unknown,
}

impl ExecutionCondition {
    // Bound recursive normalization and implication work, including adversarial source depth.
    pub(crate) const WORK_LIMIT: usize = bray_symbols::EXECUTION_CONDITION_WORK_LIMIT;

    pub(crate) fn prove_trusted(&self, trusted: &BTreeSet<(Self, bool)>, ordinary: &BTreeSet<(Self, bool)>) -> Option<bool> {
        if let Some(value) = self.prove(trusted, &mut { Self::WORK_LIMIT }) { return Some(value); }

        let mut known = trusted.clone();

        for _ in 0..Self::WORK_LIMIT {
            let mut established = Vec::new();

            for (condition, holds) in known.iter().take(Self::WORK_LIMIT) {
                if *holds && let Self::Operation(BoundOperator::LogicalOr, operands) = condition
                    && let [guard, postcondition] = operands.as_ref()
                    && guard.prove(ordinary, &mut { Self::WORK_LIMIT }) == Some(false) {
                    established.push(postcondition.clone());
                }
            }

            let before = known.len();

            for condition in established { condition.assume(true, &mut known); }

            if known.len() == before { break; }
        }

        if let Self::Operation(BoundOperator::LogicalOr, operands) = self
            && let [guard, postcondition] = operands.as_ref() {
            if guard.prove(ordinary, &mut { Self::WORK_LIMIT }) == Some(true) { return Some(true); }

            if guard.prove(ordinary, &mut { Self::WORK_LIMIT }) == Some(false) {
                return postcondition.prove_trusted(&known, ordinary);
            }
        }

        self.prove(&known, &mut { Self::WORK_LIMIT })
    }

    /// Returns the exact input places observed by this checked condition.
    pub fn inputs(&self) -> Vec<&super::ExecutionPlace> {
        let mut pending = vec![self];
        let mut inputs = Vec::new();

        while let Some(condition) = pending.pop() {
            match condition {
                Self::Input(place) => inputs.push(place),
                Self::Operation(_, operands) | Self::Predicate(_, _, operands) | Self::Call(_, operands) => {
                    pending.extend(operands.iter())
                }
                Self::Field(_, value) => pending.push(value),
                Self::Constructed(_, fields) => pending.extend(fields.iter().map(|(_, value)| value)),
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
            Self::Call(callable, operands) => Self::call(*callable, operands.iter()
                .map(|operand| operand.substitute(input, result, budget)).collect()),
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
            Self::Constructed(expression, fields) => Self::Constructed(*expression, fields.iter().map(|(field, value)|
                (*field, value.substitute(input, result, budget))).collect::<Vec<_>>().into()),
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
                Self::Constructed(candidate, _) if *candidate == expression => return true,
                Self::Constructed(_, fields) => pending.extend(fields.iter().map(|(_, value)| value)),
                Self::Operation(_, operands) | Self::Predicate(_, _, operands) | Self::Call(_, operands) => {
                    pending.extend(operands.iter())
                }
                Self::Field(_, value) => pending.push(value),
                _ => {}
            }
        }

        false
    }

    pub(crate) fn contains_value(&self, value: &Self) -> bool {
        let mut pending = vec![self];

        while let Some(condition) = pending.pop() {
            if condition == value { return true; }

            if let Self::Constructed(_, fields) = condition {
                pending.extend(fields.iter().map(|(_, value)| value));
            }
        }

        false
    }

    pub(crate) fn observes(&self, value: &Self) -> bool {
        let mut pending = vec![self];

        while let Some(condition) = pending.pop() {
            if condition == value {
                return true;
            }

            match (condition, value) {
                (Self::Input(observed), Self::Input(changed)) if observed.overlaps(changed) => return true,
                (Self::Operation(_, operands) | Self::Predicate(_, _, operands) | Self::Call(_, operands), _) => pending.extend(operands.iter()),
                (Self::Field(_, subject), _) => pending.push(subject),
                (Self::Constructed(_, fields), _) => pending.extend(fields.iter().map(|(_, value)| value)),
                _ => {}
            }
        }

        false
    }

    pub(crate) fn call(callable: bray_symbols::CallableInstanceData, arguments: Vec<Self>) -> Self {
        if arguments.iter().any(|argument| matches!(argument, Self::Unknown)) {
            Self::Unknown
        } else {
            Self::Call(callable, arguments.into())
        }
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
            Self::Constructed(_, fields) => fields.iter().find(|(candidate, _)| match candidate {
                bray_bound_tree::StorageProjection::ProductField(candidate) => bray_symbols::AnySymbolId::from(*candidate) == field,
                bray_bound_tree::StorageProjection::ActiveUnionPayloadField { field: candidate, .. } => bray_symbols::AnySymbolId::from(*candidate) == field,
                _ => false,
            })
                .map(|(_, value)| value.clone()).unwrap_or(Self::Unknown),
            Self::Unknown => Self::Unknown,
            value => Self::Field(field, Arc::new(value)),
        }
    }

    pub(crate) fn project(self, projection: bray_bound_tree::StorageProjection) -> Self {
        use bray_bound_tree::StorageProjection;

        if projection == StorageProjection::NullableValue { return self; }

        if let Self::Constructed(_, fields) = &self {
            let projection = match projection {
                StorageProjection::ElementFromEnd(index) => {
                    let Some(index) = u32::try_from(fields.len()).ok().and_then(|length| length.checked_sub(index.raw() + 1)) else {
                        return Self::Unknown;
                    };

                    StorageProjection::ElementFromStart(bray_symbols::SymbolOrdinal::new(index))
                },
                projection => projection,
            };

            return fields.iter().find(|(candidate, _)| *candidate == projection)
                .map(|(_, value)| value.clone()).unwrap_or(Self::Unknown);
        }

        match projection {
            StorageProjection::ProductField(field) => Self::field(field.into(), self),
            StorageProjection::ActiveUnionPayloadField { field, .. } => Self::field(field.into(), self),
            _ => Self::Unknown,
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
            (BoundOperator::Equal | BoundOperator::LessEqual | BoundOperator::GreaterEqual, [left, right]) if left == right => Self::Boolean(true),
            (BoundOperator::NotEqual | BoundOperator::Less | BoundOperator::Greater, [left, right]) if left == right => Self::Boolean(false),
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

    pub(crate) fn equalities(assumptions: &BTreeSet<(Self, bool)>) -> BTreeMap<Self, Self> {
        let mut replacements = BTreeMap::new();
        let mut budget = Self::WORK_LIMIT;

        for (condition, value) in assumptions.iter().take(Self::WORK_LIMIT) {
            if !value { continue; }

            let Self::Operation(BoundOperator::Equal, operands) = condition else { continue; };

            let [left, right] = operands.as_ref() else { continue; };

            if left == &Self::Unknown || right == &Self::Unknown { continue; }

            let left = resolve_equality(left, &replacements, &mut budget);
            let right = resolve_equality(right, &replacements, &mut budget);

            if left < right { replacements.insert(right, left); }
            else if right < left { replacements.insert(left, right); }
        }

        replacements
    }

    pub(crate) fn with_equalities(&self, replacements: &BTreeMap<Self, Self>) -> Self {
        rewrite_equalities(self, replacements, &mut { Self::WORK_LIMIT })
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

fn resolve_equality(value: &ExecutionCondition, replacements: &std::collections::BTreeMap<ExecutionCondition, ExecutionCondition>, budget: &mut usize) -> ExecutionCondition {
    let mut value = value.clone();

    while let Some(replacement) = replacements.get(&value) {
        let Some(next) = budget.checked_sub(1) else { return ExecutionCondition::Unknown; };

        *budget = next;
        value = replacement.clone();
    }

    value
}

fn rewrite_equalities(value: &ExecutionCondition, replacements: &std::collections::BTreeMap<ExecutionCondition, ExecutionCondition>, budget: &mut usize) -> ExecutionCondition {
    let Some(next) = budget.checked_sub(1) else { return ExecutionCondition::Unknown; };

    *budget = next;

    let value = resolve_equality(value, replacements, budget);

    let value = match value {
        ExecutionCondition::Field(field, base) => ExecutionCondition::field(field, rewrite_equalities(&base, replacements, budget)),
        ExecutionCondition::Constructed(expression, fields) => ExecutionCondition::Constructed(expression, fields.iter().map(|(field, value)|
            (*field, rewrite_equalities(value, replacements, budget))).collect::<Vec<_>>().into()),
        ExecutionCondition::Operation(operator, operands) => ExecutionCondition::operation(operator, operands.iter().map(|operand| rewrite_equalities(operand, replacements, budget)).collect()),
        ExecutionCondition::Predicate(predicate, substitution, operands) => ExecutionCondition::predicate(predicate, substitution, operands.iter().map(|operand| rewrite_equalities(operand, replacements, budget)).collect()),
        ExecutionCondition::Call(callable, operands) => ExecutionCondition::call(callable, operands.iter().map(|operand| rewrite_equalities(operand, replacements, budget)).collect()),
        value => value,
    };

    resolve_equality(&value, replacements, budget)
}
