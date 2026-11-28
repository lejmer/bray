use bray_bound_tree::BoundOperator;
use bray_symbols::ConstantValueData;
use std::sync::{Arc, OnceLock};
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

/// The control-flow boundary that joins observations of current storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionJoinSite {
    /// A CFG block with converging predecessors.
    Block(bray_bound_tree::BoundUnitId, u32),
    /// A publication that joins possible outcomes of the same expression.
    Publication(bray_bound_tree::BoundExpressionId),
}

/// A bounded, source-independent value used to compare entry and completion conditions.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd)]
pub enum ExecutionConditionKind {
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
    /// A checked whole-value storage operation over actual captured operands.
    StorageValue(
        bray_symbols::CapturedValueOperation<bray_bound_tree::StorageProjection>,
        ExecutionOperands,
    ),
    /// One evaluated value and the inline physical information captured with it.
    CapturedValue {
        value: Arc<ExecutionCondition>,
        storage: Arc<bray_bound_tree::StorageCapturedValue>,
    },
    /// The actual future or task value and its complete latent environment.
    DeferredValue {
        value: Arc<ExecutionCondition>,
        frame: Arc<crate::execution_guarantees::ExecutionDeferredValue>,
    },
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
        ExecutionJoinSite,
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

/// An immutable execution condition with shared structure and a cached structural hash.
#[derive(Clone)]
pub struct ExecutionCondition(Arc<ConditionNode>);

type ComparisonForms = [[OnceLock<Box<PreparedCondition>>; 2]; 6];

pub(in crate::execution_guarantees) struct ConditionNode {
    kind: ExecutionConditionKind,
    hash: u64,
    contains_deferred: bool,
    contains_storage: bool,
    contains_redundant_reference_context: bool,
    may_observe_live_value: bool,
    prefix_starts_empty: bool,
    // Each identified value or storage root sets one bit. A subterm can equal
    // another condition only if this node's bits include all of that condition's bits.
    roots: u64,
    // Each storage mutation that can invalidate a subterm sets one bit. A
    // mutation whose bit is absent cannot invalidate this condition.
    mutations: u64,
    // Nodes in the expanded tree, saturating. Bounded work cannot exhaust its
    // budget on a condition whose tree fits that budget.
    size: u32,
    // A stripped value is a rebuilt descendant tree, so retaining it here cannot
    // form a cycle. None records that stripping leaves this node unchanged.
    without_storage: OnceLock<Option<ExecutionCondition>>,
    comparisons: OnceLock<Box<ComparisonForms>>,
    // Only a published canonical node owns an interner entry.
    interned: bool,
}

impl Drop for ConditionNode {
    fn drop(&mut self) {
        if self.interned {
            super::interning::release(self);
        }
    }
}

impl std::fmt::Debug for ConditionNode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ConditionNode").field("kind", &self.kind).field("hash", &self.hash).finish()
    }
}

impl ConditionNode {
    fn new(kind: ExecutionConditionKind) -> Self {
        let hash = super::hashing::structural_hash(&kind);
        let may_observe_live_value = kind.may_observe_live_value();

        let mut contains_deferred = matches!(kind, ExecutionConditionKind::DeferredValue { .. });
        let mut contains_storage = contains_deferred || matches!(kind, ExecutionConditionKind::CapturedValue { .. });

        let mut contains_redundant_reference_context = match &kind {
            ExecutionConditionKind::Observation { owner, contents, kind: super::ExecutionObservationKind::Reference } =>
                ExecutionCondition::redundant_reference_contents(owner, contents).is_some(),
            ExecutionConditionKind::DeferredValue { value, .. } => value.contains_redundant_reference_context(),
            _ => false,
        };

        let mut roots = kind.root_bit();
        let mut mutations = kind.mutation_bits();
        let mut size = 1_u32;

        // One pass keeps node construction to a single child iterator.
        let _ = kind.try_for_each_structural_child(&mut |child| {
            contains_deferred |= child.contains_deferred();
            contains_storage |= child.contains_storage();
            contains_redundant_reference_context |= child.contains_redundant_reference_context();
            roots |= child.0.roots;
            mutations |= child.0.mutations;
            size = size.saturating_add(child.0.size);

            std::ops::ControlFlow::Continue(())
        });

        let prefix_starts_empty = match &kind {
            ExecutionConditionKind::StorageValue(bray_symbols::CapturedValueOperation::EmptyPrefix, _) => true,
            ExecutionConditionKind::StorageValue(bray_symbols::CapturedValueOperation::Append, operands) => {
                operands[0].prefix_starts_empty()
            }
            _ => false,
        };

        Self { kind, hash, contains_deferred, contains_storage, contains_redundant_reference_context, may_observe_live_value, prefix_starts_empty, roots, mutations, size, without_storage: OnceLock::new(), comparisons: OnceLock::new(), interned: false }
    }

    #[cfg(test)]
    fn with_test_hash(kind: ExecutionConditionKind, hash: u64) -> Self {
        let mut node = Self::new(kind);

        node.hash = hash;

        node
    }

    pub(super) const fn hash(&self) -> u64 { self.hash }

    pub(super) fn mark_interned(&mut self) { self.interned = true; }

    /// Whether this node and a node with canonical children have equal fields,
    /// identical children and the same storage and frame records.
    ///
    /// Captured storage and deferred frames compare by contents, but state
    /// transitions map each record by identity, so equal records stay distinct.
    pub(super) fn same_node(&self, other: &Self) -> bool {
        let same_records = match (&self.kind, &other.kind) {
            (ExecutionConditionKind::CapturedValue { storage, .. }, ExecutionConditionKind::CapturedValue { storage: other, .. }) =>
                Arc::ptr_eq(storage, other),
            (ExecutionConditionKind::DeferredValue { frame, .. }, ExecutionConditionKind::DeferredValue { frame: other, .. }) =>
                Arc::ptr_eq(frame, other),
            _ => true,
        };

        same_records
            && self.shallow_cmp(other).is_eq()
            && self.cmp_parts(other, &mut |left, right| {
                if Arc::ptr_eq(&left.0, &right.0) { Ordering::Equal } else { Ordering::Less }
            }).is_eq()
    }
}

/// A condition prepared for exact lookup without allocating its root handle.
pub(in crate::execution_guarantees) enum PreparedCondition {
    Retained(ExecutionCondition),
    Unpublished(ConditionNode),
}

impl PreparedCondition {
    pub(in crate::execution_guarantees) fn new(kind: ExecutionConditionKind) -> Self {
        Self::Unpublished(ConditionNode::new(kind))
    }

    pub(in crate::execution_guarantees) fn finish(self) -> ExecutionCondition {
        match self {
            Self::Retained(value) => value,
            Self::Unpublished(node) => ExecutionCondition(super::interning::intern(node)),
        }
    }

    fn node(&self) -> &ConditionNode {
        match self {
            Self::Retained(value) => &value.0,
            Self::Unpublished(node) => node,
        }
    }

    pub(super) fn cached_hash(&self) -> u64 { self.node().hash }

    #[cfg(test)]
    pub(super) fn with_test_hash(kind: ExecutionConditionKind, hash: u64) -> Self {
        Self::Unpublished(ConditionNode::with_test_hash(kind, hash))
    }
}

impl From<ExecutionCondition> for PreparedCondition {
    fn from(value: ExecutionCondition) -> Self { Self::Retained(value) }
}

impl hashbrown::Equivalent<ExecutionCondition> for PreparedCondition {
    fn equivalent(&self, key: &ExecutionCondition) -> bool {
        compare_nodes(self.node(), &key.0).is_eq()
    }
}

impl ExecutionCondition {
    /// Creates a condition from its checked structural kind.
    pub fn new(kind: ExecutionConditionKind) -> Self {
        Self(super::interning::intern(ConditionNode::new(kind)))
    }

    /// Returns the condition's structural kind.
    pub fn kind(&self) -> &ExecutionConditionKind { &self.0.kind }

    pub(super) fn contains_deferred(&self) -> bool { self.0.contains_deferred }

    /// The root bits of every identified value or storage root in this condition.
    pub(crate) fn roots(&self) -> u64 { self.0.roots }

    /// The memoized result of a pure rewrite of this node into a value without its own root.
    pub(super) fn without_storage_memo(&self, rewrite: impl FnOnce(&Self) -> Self) -> Self {
        self.0.without_storage.get_or_init(|| {
            let rewritten = rewrite(self);

            (!Arc::ptr_eq(&rewritten.0, &self.0)).then_some(rewritten)
        }).clone().unwrap_or_else(|| self.clone())
    }

    /// Whether a subterm can equal an input with these root bits. `u64::MAX`
    /// admits every subterm, including inputs that have no identified root.
    pub(crate) fn may_contain(&self, roots: u64) -> bool { roots == u64::MAX || self.0.roots & roots != 0 }

    pub(super) fn may_depend_on(&self, mutation: bray_bound_tree::StorageMutation) -> bool {
        self.0.mutations & super::traversal::mutation_bit(mutation.occurrence()) != 0
    }

    pub(super) fn contains_storage(&self) -> bool { self.0.contains_storage }

    pub(super) fn contains_redundant_reference_context(&self) -> bool { self.0.contains_redundant_reference_context }
    pub(super) fn may_observe_live_value(&self) -> bool { self.0.may_observe_live_value }

    /// The prefix was built from an empty buffer, so it covers all initialized elements.
    pub(crate) fn prefix_starts_empty(&self) -> bool { self.computed_value().0.prefix_starts_empty }

    /// Returns a shared Boolean condition.
    pub fn boolean(value: bool) -> Self {
        static TRUE: std::sync::LazyLock<ExecutionCondition> = std::sync::LazyLock::new(|| ExecutionCondition::new(ExecutionConditionKind::Boolean(true)));
        static FALSE: std::sync::LazyLock<ExecutionCondition> = std::sync::LazyLock::new(|| ExecutionCondition::new(ExecutionConditionKind::Boolean(false)));

        if value { TRUE.clone() } else { FALSE.clone() }
    }

    /// Returns the shared unknown condition.
    pub fn unknown() -> Self {
        static UNKNOWN: std::sync::LazyLock<ExecutionCondition> = std::sync::LazyLock::new(|| ExecutionCondition::new(ExecutionConditionKind::Unknown));

        UNKNOWN.clone()
    }

    /// Returns the shared completion result condition.
    pub fn result() -> Self {
        static RESULT: std::sync::LazyLock<ExecutionCondition> = std::sync::LazyLock::new(|| ExecutionCondition::new(ExecutionConditionKind::Result));

        RESULT.clone()
    }

    /// Derived syntax belongs to the immutable operands, while proof truth belongs to its evidence scope.
    pub(in crate::execution_guarantees) fn comparison_form(&self, operator: BoundOperator, reversed: bool) -> &PreparedCondition {
        let index = match operator {
            BoundOperator::Equal => 0,
            BoundOperator::NotEqual => 1,
            BoundOperator::Less => 2,
            BoundOperator::LessEqual => 3,
            BoundOperator::Greater => 4,
            BoundOperator::GreaterEqual => 5,
            _ => panic!("comparison form requires a comparison operator, found {operator:?}"),
        };

        let ExecutionConditionKind::Operation(_, operands) = self.kind() else {
            panic!("comparison form requires operation operands, found {:?}", self.kind());
        };

        let [left, right] = operands.as_ref() else {
            panic!("comparison form requires two operands, found {}", operands.len());
        };

        let forms = self.0.comparisons.get_or_init(|| Box::new(Default::default()));

        forms[index][usize::from(reversed)].get_or_init(|| {
            let (left, right) = if reversed { (right, left) } else { (left, right) };

            // Fresh operand storage keeps this derived form from retaining its owning table.
            Box::new(Self::prepare_operation(operator, vec![left.clone(), right.clone()]))
        })
    }

    pub(super) fn cached_hash(&self) -> u64 { self.0.hash }

    #[cfg(test)]
    pub(super) fn with_test_hash(kind: ExecutionConditionKind, hash: u64) -> Self {
        Self(Arc::new(ConditionNode::with_test_hash(kind, hash)))
    }
}

fn compare_operands(left: &[ExecutionCondition], right: &[ExecutionCondition], child: &mut impl FnMut(&ExecutionCondition, &ExecutionCondition) -> Ordering) -> Ordering {
    left.iter().zip(right).map(|(left, right)| child(left, right)).find(|order| !order.is_eq())
        .unwrap_or_else(|| left.len().cmp(&right.len()))
}

/// Orders shared records by contents, ending at identity first.
fn same_or_cmp<T: Ord>(left: &Arc<T>, right: &Arc<T>) -> Ordering {
    if Arc::ptr_eq(left, right) { Ordering::Equal } else { left.cmp(right) }
}

impl ConditionNode {
    /// Compares variant declaration order, then structural hashes, without traversal.
    fn shallow_cmp(&self, other: &Self) -> Ordering {
        if std::mem::discriminant(&self.kind) != std::mem::discriminant(&other.kind) {
            return self.kind.partial_cmp(&other.kind).expect("execution condition variants retain their declaration order");
        }

        self.hash.cmp(&other.hash)
    }

    /// Compare exact root fields in declaration order, delegating recursive identity to the caller.
    fn cmp_parts(&self, other: &Self, child: &mut impl FnMut(&ExecutionCondition, &ExecutionCondition) -> Ordering) -> Ordering {
        use ExecutionConditionKind as C;

        match (&self.kind, &other.kind) {
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
            (C::Constructed(base, fields), C::Constructed(other_base, other_fields)) =>
                child(base, other_base).then_with(|| fields.iter().zip(other_fields.iter())
                    .map(|((field, value), (other_field, other_value))| field.cmp(other_field).then_with(|| child(value, other_value)))
                    .find(|order| !order.is_eq()).unwrap_or_else(|| fields.len().cmp(&other_fields.len()))),
            (C::Observation { owner, contents, kind }, C::Observation { owner: other_owner, contents: other_contents, kind: other_kind }) =>
                child(owner, other_owner).then_with(|| child(contents, other_contents)).then_with(|| kind.cmp(other_kind)),
            (C::StorageValue(operation, operands), C::StorageValue(other_operation, other_operands)) =>
                operation.cmp(other_operation).then_with(|| compare_operands(operands, other_operands, child)),
            (C::CapturedValue { value, storage }, C::CapturedValue { value: other_value, storage: other_storage }) =>
                child(value, other_value).then_with(|| same_or_cmp(storage, other_storage)),
            (C::DeferredValue { value, frame }, C::DeferredValue { value: other_value, frame: other_frame }) =>
                child(value, other_value).then_with(|| same_or_cmp(frame, other_frame)),
            (C::PostState(mutation, owner), C::PostState(other_mutation, other_owner)) =>
                mutation.cmp(other_mutation).then_with(|| child(owner, other_owner)),
            (C::Joined(site, place), C::Joined(other_site, other_place)) =>
                (site, place).cmp(&(other_site, other_place)),
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
                | C::ScalarValue(_) | C::Trusted(_) | C::Borrowed(_) | C::Descriptor(..)
                | C::Constructed(..) | C::Observation { .. } | C::PostState(..) | C::Joined(..) | C::ValueAlias(..)
                | C::InitializedSnapshot(..) | C::Operation(..) | C::Predicate(..) | C::Call(..) | C::Projection(..)
                | C::Entry { .. } | C::StorageValue(..) | C::CapturedValue { .. } | C::DeferredValue { .. }, _) => self.kind.partial_cmp(&other.kind).expect("execution condition variants retain their declaration order"),
        }
    }
}

// Input nodes and their children remain alive throughout one comparison.
type ComparedPairs = hashbrown::HashSet<(*const ConditionNode, *const ConditionNode)>;

/// Orders conditions by variant, then by cached structural hash. Variant order
/// keeps simple terms such as literals and inputs before compound terms. Unequal
/// trees of one variant almost always differ by hash, so only equal hashes need
/// a structural traversal.
fn compare_nodes(left: &ConditionNode, right: &ConditionNode) -> Ordering {
    if std::ptr::eq(left, right) {
        return Ordering::Equal;
    }

    left.shallow_cmp(right).then_with(|| compare_structure(left, right, &mut ComparedPairs::default()))
}

fn compare_structure(left: &ConditionNode, right: &ConditionNode, pairs: &mut ComparedPairs) -> Ordering {
    if std::ptr::eq(left, right) {
        return Ordering::Equal;
    }

    let order = left.shallow_cmp(right);

    if !order.is_eq() {
        return order;
    }

    let pair = (left as *const ConditionNode, right as *const ConditionNode);

    if pairs.contains(&pair) {
        return Ordering::Equal;
    }

    let mut descended = false;

    let result = left.cmp_parts(right, &mut |left, right| {
        descended = true;

        compare_structure(&left.0, &right.0, pairs)
    });

    // Only equal pairs can be visited again. A difference ends the comparison.
    if descended && result.is_eq() {
        pairs.insert(pair);
    }

    result
}

impl Ord for ExecutionCondition {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_nodes(&self.0, &other.0)
    }
}

impl std::fmt::Debug for ExecutionCondition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.kind(), formatter)
    }
}

impl PartialEq for ExecutionCondition {
    fn eq(&self, other: &Self) -> bool {
        compare_nodes(&self.0, &other.0).is_eq()
    }
}

impl Eq for ExecutionCondition {}

impl PartialOrd for ExecutionCondition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

impl ExecutionCondition {
    // Bound recursive normalization and implication work, including adversarial source depth.
    pub(crate) const WORK_LIMIT: usize = bray_symbols::EXECUTION_CONDITION_WORK_LIMIT;

    pub(crate) fn is_unknown(&self) -> bool {
        let mut condition = self;

        loop {
            match condition.kind() {
                ExecutionConditionKind::Unknown => return true,
                ExecutionConditionKind::Trusted(value)
                | ExecutionConditionKind::CapturedValue { value, .. }
                | ExecutionConditionKind::DeferredValue { value, .. }
                | ExecutionConditionKind::Borrowed(value)
                | ExecutionConditionKind::Discriminant(value)
                | ExecutionConditionKind::Descriptor(_, value)
                | ExecutionConditionKind::Present(value)
                | ExecutionConditionKind::Projection(_, value)
                | ExecutionConditionKind::VariantTest(_, value)
                | ExecutionConditionKind::Entry {
                    condition: value, ..
                }
                => condition = value,
                _ => return false,
            }
        }
    }
    pub(crate) fn contains_trusted(&self) -> bool {
        if let ExecutionConditionKind::Operation(_, operands) | ExecutionConditionKind::Predicate(_, _, operands) | ExecutionConditionKind::Call(_, _, _, operands, _) = self.kind() {
            return operands.contains_trusted();
        }

        self.contains_trusted_in(&mut HashSet::new(), false)
    }

    fn contains_trusted_in<'a>(&'a self, seen: &mut HashSet<NodeIdentity<'a>>, pending: bool) -> bool {
        if !visit_once(seen, NodeIdentity(self), pending) { return false; }

        match self.kind() {
            ExecutionConditionKind::ValueAlias(left, right) => left.contains_trusted_in(seen, true) || right.contains_trusted_in(seen, pending),
            ExecutionConditionKind::Trusted(_) => true,
            ExecutionConditionKind::Operation(_, operands)
            | ExecutionConditionKind::StorageValue(_, operands)
            | ExecutionConditionKind::Predicate(_, _, operands)
            | ExecutionConditionKind::Call(_, _, _, operands, _) => operands.contains_trusted(),
            ExecutionConditionKind::Tagged(_, value)
            | ExecutionConditionKind::CapturedValue { value, .. }
            | ExecutionConditionKind::DeferredValue { value, .. }
            | ExecutionConditionKind::VariantTest(_, value)
            | ExecutionConditionKind::Discriminant(value)
            | ExecutionConditionKind::Descriptor(_, value)
            | ExecutionConditionKind::Present(value)
            | ExecutionConditionKind::Projection(_, value)
            | ExecutionConditionKind::Borrowed(value)
            | ExecutionConditionKind::Entry {
                condition: value, ..
            }
            => value.contains_trusted_in(seen, pending),
            ExecutionConditionKind::Observation { owner, contents: value, .. } => owner.contains_trusted_in(seen, true) || value.contains_trusted_in(seen, pending),
            ExecutionConditionKind::Constructed(base, fields) => {
                base.contains_trusted_in(seen, pending || !fields.is_empty())
                    || fields.iter().enumerate().any(|(index, (_, value))| value.contains_trusted_in(seen, pending || index + 1 < fields.len()))
            }
            ExecutionConditionKind::Boolean(_)
            | ExecutionConditionKind::Variant(_)
            | ExecutionConditionKind::Literal(_)
            | ExecutionConditionKind::Input(_)
            | ExecutionConditionKind::Result
            | ExecutionConditionKind::Expression(_)
            | ExecutionConditionKind::ScopedCapability(_)
            | ExecutionConditionKind::PostState(_, _)
            | ExecutionConditionKind::Joined(..)
            | ExecutionConditionKind::ScalarValue(_)
            | ExecutionConditionKind::InitializedSnapshot(_, _, _)
            | ExecutionConditionKind::Unknown => false,
        }
    }

    pub(crate) fn contains_unknown(&self) -> bool {
        if let ExecutionConditionKind::Operation(_, operands) = self.kind() { return operands.contains_unknown(); }

        self.contains_unknown_in(&mut HashSet::new(), false)
    }

    fn contains_unknown_in<'a>(&'a self, seen: &mut HashSet<NodeIdentity<'a>>, pending: bool) -> bool {
        if !visit_once(seen, NodeIdentity(self), pending) { return false; }

        match self.kind() {
            ExecutionConditionKind::ValueAlias(left, right) => left.contains_unknown_in(seen, true) || right.contains_unknown_in(seen, pending),
            ExecutionConditionKind::Operation(_, operands) => operands.contains_unknown(),
            ExecutionConditionKind::StorageValue(_, operands) => operands.contains_unknown(),
            ExecutionConditionKind::Observation { owner, contents: value, .. } => owner.contains_unknown_in(seen, true) || value.contains_unknown_in(seen, pending),
            ExecutionConditionKind::Trusted(value)
            | ExecutionConditionKind::CapturedValue { value, .. }
            | ExecutionConditionKind::DeferredValue { value, .. }
            | ExecutionConditionKind::Borrowed(value)
            | ExecutionConditionKind::Discriminant(value)
            | ExecutionConditionKind::Descriptor(_, value)
            | ExecutionConditionKind::Present(value)
            | ExecutionConditionKind::Projection(_, value)
            | ExecutionConditionKind::VariantTest(_, value)
            | ExecutionConditionKind::Entry {
                condition: value, ..
            }
            => value.contains_unknown_in(seen, pending),
            _ => self.is_unknown(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use bray_bound_tree::BoundOperator::LogicalOr;
    use bray_symbols::{SymbolId, UnionVariantSymbolId};

    use super::{ExecutionCondition as C, ExecutionConditionKind as K};

    #[test]
    fn property_queries_visit_shared_nodes_once_without_crossing_opaque_boundaries() {
        let mut shared = C::boolean(false);

        for _ in 0..20 {
            shared = C::new(K::Operation(LogicalOr, vec![shared.clone(), shared].into()));
        }

        let mut seen = HashSet::new();

        assert!(!shared.contains_trusted_in(&mut seen, false));
        assert!(seen.len() <= 41, "qualification work follows the graph rather than its expanded tree");

        seen.clear();

        assert!(!shared.contains_unknown_in(&mut seen, false));
        assert!(seen.len() <= 41, "unknown detection follows the graph rather than its expanded tree");

        let qualified = C::new(K::Trusted(Arc::new(C::unknown())));
        let tagged = C::new(K::Tagged(UnionVariantSymbolId::from_symbol_id(SymbolId::new(900)), Arc::new(qualified.clone())));

        assert!(tagged.contains_trusted());
        assert!(!tagged.contains_unknown(), "tagged payloads are opaque to unknown detection");

        let scalar = C::new(K::ScalarValue(Arc::new(qualified.clone())));

        assert!(!scalar.contains_trusted(), "scalar observations do not carry qualified authority");
        assert!(!scalar.contains_unknown());

        let later = C::new(K::Operation(LogicalOr, vec![shared, qualified].into()));

        assert!(later.contains_trusted());
        assert!(later.contains_unknown(), "an earlier false shared subtree cannot hide a later unknown");
    }

    #[test]
    fn independent_shared_graphs_compare_with_bounded_node_pairs() {
        let graph = || {
            let mut value = C::boolean(false);

            for _ in 0..20 {
                value = C::new(K::Operation(LogicalOr, vec![value.clone(), value].into()));
            }

            value
        };

        let left = graph();
        let right = graph();
        let mut pairs = super::ComparedPairs::new();

        assert!(super::compare_structure(&left.0, &right.0, &mut pairs).is_eq());
        assert!(pairs.len() <= 20, "comparison must visit shared node pairs once");
        assert_eq!(left, right);
        assert_eq!(left.cmp(&right), std::cmp::Ordering::Equal);
    }

    #[test]
    fn ordering_is_total_and_agrees_with_structural_equality() {
        use bray_bound_tree::BoundOperator::LogicalAnd;
        use super::ExecutionObservationKind::{Owned, Reference};

        let mut shared = C::boolean(false);

        for _ in 0..7 {
            shared = C::new(K::Operation(LogicalOr, vec![shared.clone(), shared].into()));
        }

        let mut terms = vec![C::unknown(), C::result(), C::boolean(false), C::boolean(true), shared.clone()];

        for value in [C::boolean(false), C::boolean(true), shared] {
            let value = Arc::new(value);

            for other in [Arc::new(C::boolean(false)), Arc::new(C::boolean(true)), Arc::clone(&value)] {
                terms.extend([
                    C::new(K::ValueAlias(Arc::clone(&value), Arc::clone(&other))),
                    C::new(K::Observation { owner: Arc::clone(&value), contents: Arc::clone(&other), kind: Owned }),
                    C::new(K::Observation { owner: Arc::clone(&other), contents: Arc::clone(&value), kind: Reference }),
                    C::new(K::Entry { condition: Arc::clone(&value), captured: false }),
                    C::new(K::Entry { condition: Arc::clone(&other), captured: true }),
                    C::new(K::Constructed(Arc::clone(&value), vec![(bray_bound_tree::StorageProjection::NullableValue, (*other).clone())].into())),
                    C::new(K::Operation(LogicalOr, vec![(*value).clone(), (*other).clone()].into())),
                    C::new(K::Operation(LogicalAnd, vec![(*other).clone(), (*value).clone(), C::unknown()].into())),
                ]);
            }
        }

        for left in &terms {
            for right in &terms {
                assert_eq!(left == right, left.kind() == right.kind());
                assert_eq!(left == right, left.cmp(right).is_eq());
                assert_eq!(left.cmp(right), right.cmp(left).reverse());

                for third in &terms {
                    if left <= right && right <= third {
                        assert!(left <= third, "condition ordering must be transitive");
                    }
                }
            }
        }
    }
}
