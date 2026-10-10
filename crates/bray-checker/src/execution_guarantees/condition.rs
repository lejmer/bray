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
    /// The capability produced by successful entry at one actual with occurrence.
    ScopedCapability(bray_bound_tree::BoundExpressionId),
    /// A checked structural value retaining the values placed in its actual storage components.
    Constructed(
        bray_bound_tree::BoundExpressionId,
        Arc<[(bray_bound_tree::StorageProjection, ExecutionCondition)]>,
    ),
    /// A retained receiver observed after one call completes normally.
    PostState(
        bray_bound_tree::BoundExpressionId,
        bray_bound_tree::BoundReferenceTarget,
    ),
    /// The current value of storage where control-flow predecessors meet.
    Joined(bray_bound_tree::BoundUnitId, u32, super::ExecutionPlace),
    /// A selected built-in operation on other known values.
    Operation(BoundOperator, Arc<[ExecutionCondition]>),
    /// A checked predicate application, compared by declaration and selected arguments.
    Predicate(
        bray_symbols::PredicateDefinitionSymbolId,
        bray_symbols::GenericSubstitutionId,
        Arc<[ExecutionCondition]>,
    ),
    /// A checked deterministic callable application with stable declaration and argument identity.
    Call(
        bray_symbols::CallableInstanceData,
        Arc<[ExecutionCondition]>,
    ),
    /// One component of an observed immutable value.
    Projection(
        bray_symbols::ConstantProjectionKind,
        Arc<ExecutionCondition>,
    ),
    /// A predicate occurrence that must be established by trusted evidence.
    Trusted(Arc<ExecutionCondition>),
    /// The contents reached through a borrowed predicate argument, rather than an immutable value snapshot.
    Borrowed(Arc<ExecutionCondition>),
    /// A conditional guarantee's entry guard; captured guards have already been substituted at entry.
    Entry {
        condition: Arc<ExecutionCondition>,
        captured: bool,
    },
    /// Meaning that cannot currently be established.
    Unknown,
}

impl ExecutionCondition {
    // Bound recursive normalization and implication work, including adversarial source depth.
    pub(crate) const WORK_LIMIT: usize = bray_symbols::EXECUTION_CONDITION_WORK_LIMIT;

    pub(crate) fn prove_trusted(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
    ) -> Option<bool> {
        if let Some(value) = self.prove(trusted, &mut { Self::WORK_LIMIT }) {
            return Some(value);
        }

        let known =
            super::trusted_implication::trusted_condition_closure(trusted.clone(), ordinary);

        self.prove_mixed(&known, ordinary, &mut { Self::WORK_LIMIT })
    }

    pub(crate) fn prove_mixed(
        &self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
    ) -> Option<bool> {
        *budget = budget.checked_sub(1)?;

        if matches!(self, Self::Unknown) {
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
            if trusted.contains(&(self.clone(), value)) {
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

    /// Returns the exact input places observed by this checked condition.
    pub fn inputs(&self) -> Vec<&super::ExecutionPlace> {
        let mut pending = vec![self];
        let mut inputs = Vec::new();

        while let Some(condition) = pending.pop() {
            match condition {
                Self::Input(place) => inputs.push(place),
                Self::Operation(_, operands)
                | Self::Predicate(_, _, operands)
                | Self::Call(_, operands) => pending.extend(operands.iter()),
                Self::Projection(_, value)
                | Self::Trusted(value)
                | Self::Borrowed(value)
                | Self::Entry {
                    condition: value, ..
                } => pending.push(value),
                Self::Constructed(_, fields) => {
                    pending.extend(fields.iter().map(|(_, value)| value))
                }
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
            Self::Call(callable, operands) => Self::call(
                *callable,
                operands
                    .iter()
                    .map(|operand| operand.substitute(input, result, budget))
                    .collect(),
            ),
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
            Self::Trusted(value) => {
                Self::Trusted(Arc::new(value.substitute(input, result, budget)))
            }
            Self::Borrowed(value) => value.substitute(input, result, budget).borrowed(),
            Self::Entry {
                condition,
                captured: false,
            } => Self::Entry {
                condition: Arc::new(condition.substitute(input, result, budget)),
                captured: false,
            },
            Self::Projection(field, value) => {
                value.substitute(input, result, budget).component(*field)
            }
            Self::Constructed(expression, fields) => Self::Constructed(
                *expression,
                fields
                    .iter()
                    .map(|(field, value)| (*field, value.substitute(input, result, budget)))
                    .collect::<Vec<_>>()
                    .into(),
            ),
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
                Self::Expression(candidate)
                | Self::ScopedCapability(candidate)
                | Self::PostState(candidate, _)
                    if *candidate == expression =>
                {
                    return true;
                }
                Self::Constructed(candidate, _) if *candidate == expression => return true,
                Self::Constructed(_, fields) => {
                    pending.extend(fields.iter().map(|(_, value)| value))
                }
                Self::Operation(_, operands)
                | Self::Predicate(_, _, operands)
                | Self::Call(_, operands) => pending.extend(operands.iter()),
                Self::Projection(_, value)
                | Self::Trusted(value)
                | Self::Borrowed(value)
                | Self::Entry {
                    condition: value, ..
                } => pending.push(value),
                _ => {}
            }
        }

        false
    }

    pub(crate) fn owned_values(&self) -> impl Iterator<Item = &Self> {
        let mut pending = vec![self];

        std::iter::from_fn(move || {
            let condition = pending.pop()?;

            if let Self::Constructed(_, fields) = condition {
                pending.extend(fields.iter().map(|(_, value)| value));
            }

            Some(condition)
        })
    }

    pub(crate) fn observations(&self) -> impl Iterator<Item = &Self> {
        let mut pending = vec![self];

        std::iter::from_fn(move || {
            let condition = pending.pop()?;

            match condition {
                Self::Operation(_, operands)
                | Self::Predicate(_, _, operands)
                | Self::Call(_, operands) => {
                    pending.extend(operands.iter());
                }
                Self::Projection(_, subject) | Self::Trusted(subject) | Self::Borrowed(subject) => {
                    pending.push(subject);
                }
                Self::Entry {
                    condition,
                    captured: false,
                } => pending.push(condition),
                Self::Constructed(_, fields) => {
                    pending.extend(fields.iter().map(|(_, value)| value));
                }
                _ => {}
            }

            Some(condition)
        })
    }

    pub(crate) fn observes(&self, value: &Self) -> bool {
        self.observations().any(|condition| {
            condition == value
                || matches!((condition, value), (Self::Input(observed), Self::Input(changed)) if observed.overlaps(changed))
        })
    }

    /// Reports whether a completion clause observes its result. Unknown meaning
    /// conservatively retains result authority until the owning query resolves it.
    pub fn observes_completion_result(&self) -> bool {
        self.observations()
            .any(|condition| matches!(condition, Self::Result | Self::Unknown))
    }

    pub(crate) fn call(callable: bray_symbols::CallableInstanceData, arguments: Vec<Self>) -> Self {
        if arguments
            .iter()
            .any(|argument| matches!(argument, Self::Unknown))
        {
            Self::Unknown
        } else {
            Self::Call(callable, arguments.into())
        }
    }

    pub(crate) fn borrowed(self) -> Self {
        if self == Self::Unknown {
            self
        } else {
            Self::Borrowed(Arc::new(self))
        }
    }

    pub(crate) fn observes_borrowed(&self, changed: Option<&Self>) -> bool {
        self.observes_borrowed_where(|value| changed.is_none_or(|changed| value.observes(changed)))
    }

    pub(crate) fn observes_borrowed_where(&self, observes: impl Fn(&Self) -> bool) -> bool {
        let mut pending = vec![self];

        while let Some(condition) = pending.pop() {
            match condition {
                Self::Borrowed(value) if observes(value) => {
                    return true;
                }
                Self::Operation(_, operands)
                | Self::Predicate(_, _, operands)
                | Self::Call(_, operands) => pending.extend(operands.iter()),
                Self::Trusted(value) | Self::Projection(_, value) => pending.push(value),
                Self::Entry {
                    condition,
                    captured: false,
                } => pending.push(condition),
                _ => {}
            }
        }

        false
    }

    pub(crate) fn capture_entry(
        &self,
        input: &impl Fn(&super::ExecutionPlace) -> Self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
    ) -> Self {
        let Some(next) = budget.checked_sub(1) else {
            return Self::Unknown;
        };

        *budget = next;

        match self {
            Self::Entry {
                condition,
                captured: false,
            } => {
                let condition = condition.substitute(input, &Self::Unknown, budget);

                let condition = match condition.prove_trusted(trusted, ordinary) {
                    Some(value) => Self::Boolean(value),
                    None if condition.observes_borrowed(None) => Self::Unknown,
                    None => condition,
                };

                Self::Entry {
                    condition: Arc::new(condition),
                    captured: true,
                }
            }
            Self::Operation(operator, operands) => Self::operation(
                *operator,
                operands
                    .iter()
                    .map(|operand| operand.capture_entry(input, trusted, ordinary, budget))
                    .collect(),
            ),
            value => value.clone(),
        }
    }

    pub(crate) fn requiring_trusted_predicates(self) -> Self {
        match self {
            Self::Predicate(_, _, _) => Self::Trusted(Arc::new(self)),
            Self::Operation(operator, operands) => Self::operation(
                operator,
                operands
                    .iter()
                    .map(|operand| operand.clone().requiring_trusted_predicates())
                    .collect(),
            ),
            value => value,
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
        let projection = match field {
            bray_symbols::AnySymbolId::StructField(field) => {
                bray_symbols::ConstantProjectionKind::ProductField(field)
            }
            bray_symbols::AnySymbolId::UnionPayloadField(field) => {
                bray_symbols::ConstantProjectionKind::UnionPayloadField(field)
            }
            _ => panic!(
                "checked execution field identifies a product or union payload field: {field:?}"
            ),
        };

        value.component(projection)
    }

    pub(crate) fn project(self, projection: bray_bound_tree::StorageProjection) -> Self {
        use bray_bound_tree::StorageProjection;

        if projection == StorageProjection::NullableValue {
            return self;
        }

        if let Self::Constructed(_, fields) = &self {
            let projection = match projection {
                StorageProjection::ElementFromEnd(index) => {
                    let Some(index) = u32::try_from(fields.len())
                        .ok()
                        .and_then(|length| length.checked_sub(index.raw() + 1))
                    else {
                        return Self::Unknown;
                    };

                    StorageProjection::ElementFromStart(bray_symbols::SymbolOrdinal::new(index))
                }
                projection => projection,
            };

            return fields
                .iter()
                .find(|(candidate, _)| *candidate == projection)
                .map(|(_, value)| value.clone())
                .unwrap_or(Self::Unknown);
        }

        let projection = match projection {
            StorageProjection::ProductField(field) => {
                bray_symbols::ConstantProjectionKind::ProductField(field)
            }
            StorageProjection::ActiveUnionPayloadField { field, .. } => {
                bray_symbols::ConstantProjectionKind::UnionPayloadField(field)
            }
            StorageProjection::TupleElement(index) => {
                bray_symbols::ConstantProjectionKind::TupleElement(index)
            }
            StorageProjection::ElementFromStart(index) => {
                bray_symbols::ConstantProjectionKind::ArrayElementOrdinal(index)
            }
            _ => return Self::Unknown,
        };

        self.component(projection)
    }

    pub(crate) fn component(self, projection: bray_symbols::ConstantProjectionKind) -> Self {
        use bray_bound_tree::StorageProjection;
        use bray_symbols::ConstantProjectionKind;

        if !matches!(
            projection,
            ConstantProjectionKind::ProductField(_)
                | ConstantProjectionKind::UnionPayloadField(_)
                | ConstantProjectionKind::TupleElement(_)
                | ConstantProjectionKind::ArrayElementOrdinal(_)
        ) {
            return Self::Unknown;
        }

        match self {
            Self::Input(place) => Self::Input(place.component(projection)),
            // A field access through a borrow observes the referent's field,
            // rather than projecting a field from the borrow value itself.
            Self::Borrowed(value) => value.as_ref().clone().component(projection),
            Self::Constructed(_, fields) => fields
                .iter()
                .find(|(candidate, _)| match (candidate, projection) {
                    (
                        StorageProjection::ProductField(candidate),
                        ConstantProjectionKind::ProductField(field),
                    ) => *candidate == field,
                    (
                        StorageProjection::ActiveUnionPayloadField {
                            field: candidate, ..
                        },
                        ConstantProjectionKind::UnionPayloadField(field),
                    ) => *candidate == field,
                    (
                        StorageProjection::TupleElement(candidate),
                        ConstantProjectionKind::TupleElement(index),
                    )
                    | (
                        StorageProjection::ElementFromStart(candidate),
                        ConstantProjectionKind::ArrayElementOrdinal(index),
                    ) => *candidate == index,
                    _ => false,
                })
                .map(|(_, value)| value.clone())
                .unwrap_or(Self::Unknown),
            Self::Unknown => Self::Unknown,
            value => Self::Projection(projection, Arc::new(value)),
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
            (
                BoundOperator::Equal | BoundOperator::LessEqual | BoundOperator::GreaterEqual,
                [left, right],
            ) if left == right => Self::Boolean(true),
            (
                BoundOperator::NotEqual | BoundOperator::Less | BoundOperator::Greater,
                [left, right],
            ) if left == right => Self::Boolean(false),
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

        if let Self::Entry { condition, .. } = self {
            return condition.prove(assumptions, budget);
        }

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

        if let Some(value) =
            super::implication::comparison_is_implied(*operator, operands, assumptions)
        {
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

    pub(crate) fn equalities(
        assumptions: &BTreeSet<(Self, bool)>,
        expiring: Option<bray_bound_tree::BoundExpressionId>,
    ) -> BTreeMap<Self, Self> {
        let mut replacements = BTreeMap::new();
        let mut budget = Self::WORK_LIMIT;

        for (condition, value) in assumptions.iter().take(Self::WORK_LIMIT) {
            let Self::Operation(operator, operands) = condition else {
                continue;
            };

            if !matches!(
                (operator, value),
                (BoundOperator::Equal, true) | (BoundOperator::NotEqual, false)
            ) {
                continue;
            }

            let [left, right] = operands.as_ref() else {
                continue;
            };

            if left == &Self::Unknown || right == &Self::Unknown {
                continue;
            }

            let left = resolve_equality(left, &replacements, &mut budget);
            let right = resolve_equality(right, &replacements, &mut budget);

            // An explicit exit transfer must name the surviving value rather than an alias
            // whose scoped authority is ending. Keep the same bounded equality solver.
            let left_order = (expiring.is_some_and(|scope| left.depends_on(scope)), &left);

            let right_order = (
                expiring.is_some_and(|scope| right.depends_on(scope)),
                &right,
            );

            if left_order < right_order {
                replacements.insert(right, left);
            } else if right_order < left_order {
                replacements.insert(left, right);
            }
        }

        replacements
    }

    pub(crate) fn with_equalities(&self, replacements: &BTreeMap<Self, Self>) -> Self {
        rewrite_equalities(self, replacements, false, &mut { Self::WORK_LIMIT })
    }

    pub(crate) fn with_joined_values(&self, replacements: &BTreeMap<Self, Self>) -> Self {
        rewrite_equalities(self, replacements, true, &mut { Self::WORK_LIMIT })
    }

    pub(crate) fn assume(self, value: bool, assumptions: &mut BTreeSet<(Self, bool)>) {
        self.assume_with(value, &mut |condition, value| {
            assumptions.insert((condition, value));
        });
    }

    pub(super) fn assume_with(self, value: bool, insert: &mut impl FnMut(Self, bool)) {
        match self {
            Self::Unknown => {}
            Self::Trusted(condition) => {
                // Keep the occurrence qualifier for contract implication and authority-bearing values.
                insert(Self::Trusted(condition.clone()), value);
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
            condition => {
                insert(condition, value);
            }
        }
    }
}

fn resolve_equality(
    value: &ExecutionCondition,
    replacements: &std::collections::BTreeMap<ExecutionCondition, ExecutionCondition>,
    budget: &mut usize,
) -> ExecutionCondition {
    let mut value = value.clone();

    while let Some(replacement) = replacements.get(&value) {
        let Some(next) = budget.checked_sub(1) else {
            return ExecutionCondition::Unknown;
        };

        *budget = next;
        value = replacement.clone();
    }

    value
}

fn rewrite_equalities(
    value: &ExecutionCondition,
    replacements: &std::collections::BTreeMap<ExecutionCondition, ExecutionCondition>,
    join: bool,
    budget: &mut usize,
) -> ExecutionCondition {
    let Some(next) = budget.checked_sub(1) else {
        return ExecutionCondition::Unknown;
    };

    *budget = next;

    let value = resolve_equality(value, replacements, budget);

    let value = match value {
        ExecutionCondition::Trusted(value) if join => ExecutionCondition::Trusted(Arc::new(
            rewrite_equalities(&value, replacements, join, budget),
        )),
        ExecutionCondition::Trusted(value) => {
            // Ordinary Boolean equalities cannot rename the predicate that owns trusted authority.
            let value = match value.as_ref() {
                ExecutionCondition::Predicate(predicate, substitution, operands) => {
                    ExecutionCondition::predicate(
                        *predicate,
                        *substitution,
                        operands
                            .iter()
                            .map(|operand| rewrite_equalities(operand, replacements, join, budget))
                            .collect(),
                    )
                }
                value => value.clone(),
            };

            ExecutionCondition::Trusted(Arc::new(value))
        }
        ExecutionCondition::Borrowed(value) if join => {
            rewrite_equalities(&value, replacements, join, budget).borrowed()
        }
        ExecutionCondition::Entry { captured: true, .. } if join => value,
        ExecutionCondition::Borrowed(value) => {
            // Equal contents do not equate the physical referents of borrowed observations.
            ExecutionCondition::Borrowed(value)
        }
        ExecutionCondition::Entry {
            condition,
            captured,
        } => ExecutionCondition::Entry {
            condition: Arc::new(rewrite_equalities(&condition, replacements, join, budget)),
            captured,
        },
        ExecutionCondition::Projection(field, base) => {
            rewrite_equalities(&base, replacements, join, budget).component(field)
        }
        ExecutionCondition::Constructed(expression, fields) => ExecutionCondition::Constructed(
            expression,
            fields
                .iter()
                .map(|(field, value)| {
                    (
                        *field,
                        rewrite_equalities(value, replacements, join, budget),
                    )
                })
                .collect::<Vec<_>>()
                .into(),
        ),
        ExecutionCondition::Operation(operator, operands) => ExecutionCondition::operation(
            operator,
            operands
                .iter()
                .map(|operand| rewrite_equalities(operand, replacements, join, budget))
                .collect(),
        ),
        ExecutionCondition::Predicate(predicate, substitution, operands) => {
            ExecutionCondition::predicate(
                predicate,
                substitution,
                operands
                    .iter()
                    .map(|operand| rewrite_equalities(operand, replacements, join, budget))
                    .collect(),
            )
        }
        ExecutionCondition::Call(callable, operands) => ExecutionCondition::call(
            callable,
            operands
                .iter()
                .map(|operand| rewrite_equalities(operand, replacements, join, budget))
                .collect(),
        ),
        value => value,
    };

    resolve_equality(&value, replacements, budget)
}
