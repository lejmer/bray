use bray_bound_tree::BoundOperator;
use bray_symbols::ConstantValueData;
use std::sync::Arc;
use std::collections::HashSet;
use std::cmp::Ordering;

use super::traversal::{NodeIdentity, visit_once};
use super::operands::ExecutionOperands;

/// How a current observation relates to the storage or value that retains it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionObservationKind {
    /// Current contents belong to this complete owned value.
    Owned,
    /// Current reached data is observed through a retained reference value.
    Reference,
    /// Copied scalar data is observed at an independently identified storage place.
    Scalar,
}

/// Whether a descriptor is supplied directly or read through a view of its storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionDescriptorKind {
    /// Retain a reference descriptor independently of its reached contents.
    Reference,
    /// Read the current value before selecting its descriptor.
    Value,
}

/// A bounded, source-independent value used to compare entry and completion conditions.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd)]
pub enum ExecutionCondition {
    /// A checked Boolean value.
    Boolean(bool),
    /// An active union tag in a checked aggregate snapshot.
    Variant(bray_symbols::UnionVariantSymbolId),
    /// A constructed union snapshot with its exact active tag.
    Tagged(bray_symbols::UnionVariantSymbolId, Arc<ExecutionCondition>),
    /// A test of a union tag, independent of its payload.
    VariantTest(bray_symbols::UnionVariantSymbolId, Arc<ExecutionCondition>),
    /// An immutable nullable presence or union tag, without payload data or authority.
    Discriminant(Arc<ExecutionCondition>),
    /// Immutable descriptor data, independently of current reached contents.
    Descriptor(ExecutionDescriptorKind, Arc<ExecutionCondition>),
    /// A checked literal value.
    Literal(Arc<ConstantValueData>),
    /// The value of a supplied input before execution begins.
    Input(crate::execution_guarantees::ExecutionPlace),
    /// The value produced by normal completion.
    Result,
    /// The value produced by one runtime expression, without inventing its contents.
    Expression(bray_bound_tree::BoundExpressionId),
    /// An input captured by one deferred producer in the actual future owner.
    CapturedInput {
        owner: Arc<ExecutionCondition>,
        invocation: bray_bound_tree::BoundExecutionSite,
        ordinal: bray_symbols::SymbolOrdinal,
    },
    /// The capability produced by successful entry at one actual with occurrence.
    ScopedCapability(bray_bound_tree::BoundExpressionId),
    /// The present branch of a nullable value, retaining its contained owner.
    Present(Arc<ExecutionCondition>),
    /// An aggregate snapshot with checked component values over its immutable base.
    /// Unlisted components retain the base identity; explicit unknown components do not.
    Constructed(
        Arc<ExecutionCondition>,
        Arc<[(bray_bound_tree::StorageProjection, ExecutionCondition)]>,
    ),
    /// Current data retains its identity independently of its observation role.
    /// Reference contents never transfer ownership of the reached storage.
    Observation {
        owner: Arc<ExecutionCondition>,
        contents: Arc<ExecutionCondition>,
        kind: ExecutionObservationKind,
    },
    /// Fresh contents of one value identity after a checked mutation.
    /// The identity carries no previous contents or new guarantee.
    PostState(
        bray_bound_tree::StorageMutation,
        Arc<ExecutionCondition>,
    ),
    /// The current value of storage where control-flow predecessors meet.
    Joined(
        bray_bound_tree::BoundUnitId,
        u32,
        crate::execution_guarantees::ExecutionPlace,
    ),
    /// A produced opaque scalar datum with an immutable execution identity and no owner authority.
    ScalarValue(Arc<ExecutionCondition>),
    /// A proof-only correspondence between the same immutable data observations.
    /// This is not numeric equality and carries no ownership or trusted evidence.
    ValueAlias(Arc<ExecutionCondition>, Arc<ExecutionCondition>),
    /// Immutable initialized-state observation at an ownership join. The value
    /// identity distinguishes incarnations without becoming access authority.
    InitializedSnapshot(
        crate::execution_guarantees::ExecutionPlace,
        Arc<ExecutionCondition>,
        Arc<ExecutionCondition>,
    ),
    /// A selected built-in operation on other known values.
    Operation(BoundOperator, ExecutionOperands),
    /// A checked predicate application, compared by declaration and selected arguments.
    Predicate(
        bray_symbols::PredicateDefinitionSymbolId,
        bray_symbols::GenericSubstitutionId,
        ExecutionOperands,
    ),
    /// A checked deterministic callable application with stable declaration and argument identity.
    Call(
        bray_symbols::CallableInstanceData,
        Option<bray_symbols::ImplementationInstanceData>,
        Arc<[bray_symbols::ImplementationInstanceData]>,
        ExecutionOperands,
        // A checked borrowed result type identifies a reference computation without granting ownership.
        Option<bray_symbols::TypeId>,
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

fn compare_operands(left: &[ExecutionCondition], right: &[ExecutionCondition], child: &mut impl FnMut(&ExecutionCondition, &ExecutionCondition) -> Ordering) -> Ordering {
    left.iter().zip(right).map(|(left, right)| child(left, right)).find(|order| !order.is_eq())
        .unwrap_or_else(|| left.len().cmp(&right.len()))
}

impl ExecutionCondition {
    pub(super) fn eq_parts(&self, other: &Self, child: &mut impl FnMut(&Self, &Self) -> bool) -> bool {
        self.cmp_parts(other, &mut |left, right| if child(left, right) { Ordering::Equal } else { Ordering::Less }).is_eq()
    }

    /// Compare exact root fields in declaration order, delegating recursive identity to the caller.
    pub(super) fn cmp_parts(&self, other: &Self, child: &mut impl FnMut(&Self, &Self) -> Ordering) -> Ordering {
        use ExecutionCondition as C;

        match (self, other) {
            (C::Boolean(a), C::Boolean(b)) => a.cmp(b),
            (C::Variant(a), C::Variant(b)) => a.cmp(b),
            (C::Literal(a), C::Literal(b)) => a.cmp(b),
            (C::Input(a), C::Input(b)) => a.cmp(b),
            (C::Expression(a), C::Expression(b)) | (C::ScopedCapability(a), C::ScopedCapability(b)) => a.cmp(b),
            (C::Result, C::Result) | (C::Unknown, C::Unknown) => Ordering::Equal,
            (C::Tagged(tag, a), C::Tagged(other_tag, b))
            | (C::VariantTest(tag, a), C::VariantTest(other_tag, b)) => tag.cmp(other_tag).then_with(|| child(a, b)),
            (C::Discriminant(a), C::Discriminant(b)) | (C::Present(a), C::Present(b))
            | (C::ScalarValue(a), C::ScalarValue(b)) | (C::Trusted(a), C::Trusted(b))
            | (C::Borrowed(a), C::Borrowed(b)) => child(a, b),
            (C::Descriptor(kind, a), C::Descriptor(other_kind, b)) => kind.cmp(other_kind).then_with(|| child(a, b)),
            (C::CapturedInput { owner, invocation, ordinal }, C::CapturedInput { owner: other_owner, invocation: other_invocation, ordinal: other_ordinal }) =>
                child(owner, other_owner).then_with(|| (invocation, ordinal).cmp(&(other_invocation, other_ordinal))),
            (C::Constructed(base, fields), C::Constructed(other_base, other_fields)) =>
                child(base, other_base).then_with(|| fields.iter().zip(other_fields.iter())
                    .map(|((field, value), (other_field, other_value))| field.cmp(other_field).then_with(|| child(value, other_value)))
                    .find(|order| !order.is_eq()).unwrap_or_else(|| fields.len().cmp(&other_fields.len()))),
            (C::Observation { owner, contents, kind }, C::Observation { owner: other_owner, contents: other_contents, kind: other_kind }) =>
                child(owner, other_owner).then_with(|| child(contents, other_contents)).then_with(|| kind.cmp(other_kind)),
            (C::PostState(mutation, owner), C::PostState(other_mutation, other_owner)) =>
                mutation.cmp(other_mutation).then_with(|| child(owner, other_owner)),
            (C::Joined(unit, block, place), C::Joined(other_unit, other_block, other_place)) =>
                (unit, block, place).cmp(&(other_unit, other_block, other_place)),
            (C::ValueAlias(left, right), C::ValueAlias(other_left, other_right)) =>
                child(left, other_left).then_with(|| child(right, other_right)),
            (C::InitializedSnapshot(place, join, value), C::InitializedSnapshot(other_place, other_join, other_value)) =>
                place.cmp(other_place).then_with(|| child(join, other_join)).then_with(|| child(value, other_value)),
            (C::Operation(operator, operands), C::Operation(other_operator, other_operands)) =>
                operator.cmp(other_operator).then_with(|| compare_operands(operands, other_operands, child)),
            (C::Predicate(predicate, substitution, operands), C::Predicate(other_predicate, other_substitution, other_operands)) =>
                (predicate, substitution).cmp(&(other_predicate, other_substitution)).then_with(|| compare_operands(operands, other_operands, child)),
            (C::Call(callable, implementation, witnesses, operands, reference_type), C::Call(other_callable, other_implementation, other_witnesses, other_operands, other_reference_type)) =>
                (callable, implementation, witnesses).cmp(&(other_callable, other_implementation, other_witnesses))
                    .then_with(|| compare_operands(operands, other_operands, child)).then_with(|| reference_type.cmp(other_reference_type)),
            (C::Projection(field, value), C::Projection(other_field, other_value)) => field.cmp(other_field).then_with(|| child(value, other_value)),
            (C::Entry { condition, captured }, C::Entry { condition: other_condition, captured: other_captured }) =>
                child(condition, other_condition).then_with(|| captured.cmp(other_captured)),
            // Different variants need only their derived declaration order, with no operand traversal.
            (C::Boolean(_) | C::Variant(_) | C::Literal(_) | C::Input(_) | C::Expression(_) | C::ScopedCapability(_)
                | C::Result | C::Unknown | C::Tagged(..) | C::VariantTest(..) | C::Discriminant(_) | C::Present(_)
                | C::ScalarValue(_) | C::Trusted(_) | C::Borrowed(_) | C::Descriptor(..) | C::CapturedInput { .. }
                | C::Constructed(..) | C::Observation { .. } | C::PostState(..) | C::Joined(..) | C::ValueAlias(..)
                | C::InitializedSnapshot(..) | C::Operation(..) | C::Predicate(..) | C::Call(..) | C::Projection(..)
                | C::Entry { .. }, _) => self.partial_cmp(other).expect("execution condition variants retain their declaration order"),
        }
    }
}

impl Ord for ExecutionCondition {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // A recursive comparison stops at shared storage. Root fields are compared
        // once in declaration order, without a separate equality traversal.
        if std::ptr::eq(self, other) {
            return std::cmp::Ordering::Equal;
        }

        self.cmp_parts(other, &mut |left, right| left.cmp(right))
    }
}

impl ExecutionCondition {
    // Bound recursive normalization and implication work, including adversarial source depth.
    pub(crate) const WORK_LIMIT: usize = bray_symbols::EXECUTION_CONDITION_WORK_LIMIT;

    pub(crate) fn is_unknown(&self) -> bool {
        let mut condition = self;

        loop {
            match condition {
                Self::Unknown => return true,
                Self::Trusted(value)
                | Self::Borrowed(value)
                | Self::Discriminant(value)
                | Self::Descriptor(_, value)
                | Self::Present(value)
                | Self::Projection(_, value)
                | Self::VariantTest(_, value)
                | Self::Entry {
                    condition: value, ..
                }
                | Self::CapturedInput { owner: value, .. } => condition = value,
                _ => return false,
            }
        }
    }
    pub(crate) fn contains_trusted(&self) -> bool {
        if let Self::Operation(_, operands) | Self::Predicate(_, _, operands) | Self::Call(_, _, _, operands, _) = self {
            return operands.contains_trusted();
        }

        self.contains_trusted_in(&mut HashSet::new(), false)
    }

    fn contains_trusted_in<'a>(&'a self, seen: &mut HashSet<NodeIdentity<'a>>, pending: bool) -> bool {
        if !visit_once(seen, NodeIdentity(self), pending) { return false; }

        match self {
            Self::ValueAlias(left, right) => left.contains_trusted_in(seen, true) || right.contains_trusted_in(seen, pending),
            Self::Trusted(_) => true,
            Self::Operation(_, operands)
            | Self::Predicate(_, _, operands)
            | Self::Call(_, _, _, operands, _) => operands.contains_trusted(),
            Self::Tagged(_, value)
            | Self::VariantTest(_, value)
            | Self::Discriminant(value)
            | Self::Descriptor(_, value)
            | Self::Present(value)
            | Self::Projection(_, value)
            | Self::Borrowed(value)
            | Self::Entry {
                condition: value, ..
            }
            | Self::CapturedInput { owner: value, .. } => value.contains_trusted_in(seen, pending),
            Self::Observation { owner, contents: value, .. } => owner.contains_trusted_in(seen, true) || value.contains_trusted_in(seen, pending),
            Self::Constructed(base, fields) => {
                base.contains_trusted_in(seen, pending || !fields.is_empty())
                    || fields.iter().enumerate().any(|(index, (_, value))| value.contains_trusted_in(seen, pending || index + 1 < fields.len()))
            }
            Self::Boolean(_)
            | Self::Variant(_)
            | Self::Literal(_)
            | Self::Input(_)
            | Self::Result
            | Self::Expression(_)
            | Self::ScopedCapability(_)
            | Self::PostState(_, _)
            | Self::Joined(_, _, _)
            | Self::ScalarValue(_)
            | Self::InitializedSnapshot(_, _, _)
            | Self::Unknown => false,
        }
    }

    pub(crate) fn contains_unknown(&self) -> bool {
        if let Self::Operation(_, operands) = self { return operands.contains_unknown(); }

        self.contains_unknown_in(&mut HashSet::new(), false)
    }

    fn contains_unknown_in<'a>(&'a self, seen: &mut HashSet<NodeIdentity<'a>>, pending: bool) -> bool {
        if !visit_once(seen, NodeIdentity(self), pending) { return false; }

        match self {
            Self::ValueAlias(left, right) => left.contains_unknown_in(seen, true) || right.contains_unknown_in(seen, pending),
            Self::Operation(_, operands) => operands.contains_unknown(),
            Self::Observation { owner, contents: value, .. } => owner.contains_unknown_in(seen, true) || value.contains_unknown_in(seen, pending),
            Self::Trusted(value)
            | Self::Borrowed(value)
            | Self::Discriminant(value)
            | Self::Descriptor(_, value)
            | Self::Present(value)
            | Self::Projection(_, value)
            | Self::VariantTest(_, value)
            | Self::Entry {
                condition: value, ..
            }
            | Self::CapturedInput { owner: value, .. } => value.contains_unknown_in(seen, pending),
            _ => self.is_unknown(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
use std::cmp::Ordering;
    use std::sync::Arc;

    use bray_bound_tree::BoundOperator::LogicalOr;
    use bray_symbols::{SymbolId, UnionVariantSymbolId};

    use super::ExecutionCondition as C;

    #[test]
    fn property_queries_visit_shared_nodes_once_without_crossing_opaque_boundaries() {
        let mut shared = C::Boolean(false);

        for _ in 0..20 {
            shared = C::Operation(LogicalOr, vec![shared.clone(), shared].into());
        }

        let mut seen = HashSet::new();

        assert!(!shared.contains_trusted_in(&mut seen, false));
        assert!(seen.len() <= 41, "qualification work follows the graph rather than its expanded tree");
        seen.clear();
        assert!(!shared.contains_unknown_in(&mut seen, false));
        assert!(seen.len() <= 41, "unknown detection follows the graph rather than its expanded tree");

        let qualified = C::Trusted(Arc::new(C::Unknown));
        let tagged = C::Tagged(UnionVariantSymbolId::from_symbol_id(SymbolId::new(900)), Arc::new(qualified.clone()));

        assert!(tagged.contains_trusted());
        assert!(!tagged.contains_unknown(), "tagged payloads are opaque to unknown detection");

        let scalar = C::ScalarValue(Arc::new(qualified.clone()));

        assert!(!scalar.contains_trusted(), "scalar observations do not carry qualified authority");
        assert!(!scalar.contains_unknown());

        let later = C::Operation(LogicalOr, vec![shared, qualified].into());

        assert!(later.contains_trusted());
        assert!(later.contains_unknown(), "an earlier false shared subtree cannot hide a later unknown");
    }
}
