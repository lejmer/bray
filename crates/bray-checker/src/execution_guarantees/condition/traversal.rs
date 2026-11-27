use std::collections::{BTreeSet, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::ControlFlow;
use std::sync::Arc;

use super::{ExecutionCondition, ExecutionConditionKind as K};
use bray_bound_tree::StorageMutation;

/// Physical sharing only, never semantic value or owner identity.
#[derive(Clone, Copy)]
pub(in crate::execution_guarantees) struct NodeIdentity<'a>(pub(in crate::execution_guarantees) &'a ExecutionCondition);

impl PartialEq for NodeIdentity<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for NodeIdentity<'_> {}

impl Hash for NodeIdentity<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) { std::ptr::hash(self.0, state); }
}

/// Acyclic value graphs need remembered nodes only while another path is pending.
/// One bit for the expression a mutation occurs at, or for its site when it
/// has no expression. Every mutation a condition can depend on shares the bit
/// of the term that names it.
pub(super) fn mutation_bit(occurrence: bray_bound_tree::BoundExecutionSite) -> u64 {
    let mut hash = std::hash::DefaultHasher::new();

    match occurrence.expression() {
        Some(expression) => (0_u8, expression).hash(&mut hash),
        None => (1_u8, occurrence).hash(&mut hash),
    }

    1 << (hash.finish() % 64)
}

#[inline(never)]
fn visit_frame_conditions<'a>(
    frame: &'a crate::execution_guarantees::ExecutionDeferredValue,
    visit: &mut dyn FnMut(&'a ExecutionCondition) -> ControlFlow<()>,
) -> ControlFlow<()> {
    for condition in frame.conditions() {
        visit(condition)?;
    }

    ControlFlow::Continue(())
}

pub(super) fn visit_once<K: Eq + Hash>(visited: &mut HashSet<K>, node: K, has_pending: bool) -> bool {
    if has_pending { visited.insert(node) } else { !visited.contains(&node) }
}

impl ExecutionCondition {
    /// Rebuild one node from its mapped children, retaining the established constructors.
    pub(super) fn map_children(&self, mut map: impl FnMut(&Self) -> Self) -> Self {
        match self.kind() {
            K::CapturedValue { value, storage } => map(value).captured_value(storage.clone()),
            K::DeferredValue { value, frame } => map(value).with_deferred_value(Arc::clone(frame)),
            K::StorageValue(operation, operands) => Self::storage_value(*operation, operands.iter().map(&mut map).collect()),
            K::ScalarValue(value) => self.map_wrapped_child(value, &mut map, K::ScalarValue),
            K::PostState(mutation, value) => self.map_wrapped_child(value, &mut map, |value| K::PostState(*mutation, value)),
            K::Discriminant(value) => map(value).discriminant(),
            K::Descriptor(kind, value) => map(value).descriptor(*kind),
            K::ValueAlias(left, right) => Self::value_alias(map(left), map(right)),
            K::InitializedSnapshot(place, identity, extent) => Self::new(K::InitializedSnapshot(
                place.clone(), Arc::new(map(identity)), Arc::new(map(extent)),
            )),
            K::Call(callable, implementation, witnesses, operands, reference_type) => Self::call(
                *callable, *implementation, witnesses.iter().copied(), operands.iter().map(&mut map).collect(), *reference_type,
            ),
            K::Predicate(predicate, substitution, operands) => Self::predicate(
                *predicate, *substitution, operands.iter().map(&mut map).collect(),
            ),
            K::Trusted(value) => self.map_wrapped_child(value, &mut map, K::Trusted),
            K::Borrowed(value) => map(value).borrowed(),
            K::Present(value) => self.map_wrapped_child(value, &mut map, K::Present),
            K::Tagged(variant, value) => self.map_wrapped_child(value, &mut map, |value| K::Tagged(*variant, value)),
            K::VariantTest(variant, value) => map(value).matches_variant(*variant),
            K::Entry { condition, captured } => self.map_wrapped_child(condition, &mut map, |condition| K::Entry { condition, captured: *captured }),
            K::Projection(field, value) => map(value).component(*field),
            K::Observation { owner, contents, kind } => Self::observe_retaining(map(owner), map(contents), *kind, Some(self)),
            K::Constructed(base, fields) => self.map_constructed(base, fields, |_, value| map(value)),
            K::Operation(operator, operands) => Self::operation(*operator, operands.iter().map(&mut map).collect()),
            K::Boolean(_) | K::Variant(_) | K::Literal(_) | K::Input(_) | K::Result | K::Expression(_)
            | K::ScopedCapability(_) | K::Joined(..) | K::Unknown => self.clone(),
        }
    }

    pub(super) fn map_wrapped_child(
        &self,
        child: &Self,
        map: impl FnOnce(&Self) -> Self,
        wrap: impl FnOnce(Arc<Self>) -> K,
    ) -> Self {
        let value = map(child);

        if &value == child { self.clone() } else { Self::new(wrap(Arc::new(value))) }
    }

    pub(super) fn map_constructed(
        &self,
        base: &Self,
        fields: &Arc<[(bray_bound_tree::StorageProjection, Self)]>,
        mut map: impl FnMut(Option<bray_bound_tree::StorageProjection>, &Self) -> Self,
    ) -> Self {
        let base = map(None, base);
        let mut updated = Arc::clone(fields);

        for (index, (field, old)) in fields.iter().enumerate() {
            let value = map(Some(*field), old);

            if &value != old {
                Arc::make_mut(&mut updated)[index].1 = value;
            }
        }

        Self::construct_retaining(base, updated, Some(self))
    }

    pub(super) fn children(&self) -> impl Iterator<Item = &Self> { self.kind().children() }

}

impl super::ExecutionConditionKind {
    pub(super) fn may_observe_live_value(&self) -> bool {
        // These are the places a by-value observation can enter a live view.
        match self {
            K::Borrowed(_) | K::Entry { captured: false, .. }
            | K::Observation { kind: super::ExecutionObservationKind::Reference, .. } => true,
            K::Descriptor(..) | K::Entry { captured: true, .. } => false,
            _ => self.children().any(ExecutionCondition::may_observe_live_value),
        }
    }

    fn children(&self) -> impl Iterator<Item = &ExecutionCondition> {
        let (first, second, operands, fields) = match self {
            K::ValueAlias(left, right) | K::InitializedSnapshot(_, left, right) =>
                (Some(left.as_ref()), Some(right.as_ref()), &[][..], &[][..]),
            K::Observation { owner, contents, .. } =>
                (Some(owner.as_ref()), Some(contents.as_ref()), &[][..], &[][..]),
            K::Constructed(base, fields) => (Some(base.as_ref()), None, &[][..], fields.as_ref()),
            K::ScalarValue(value) | K::Discriminant(value) | K::Descriptor(_, value)
            | K::CapturedValue { value, .. } | K::DeferredValue { value, .. } | K::Tagged(_, value) | K::VariantTest(_, value)
            | K::Projection(_, value) | K::Trusted(value) | K::Borrowed(value) | K::Present(value)
            | K::Entry { condition: value, .. } =>
                (Some(value.as_ref()), None, &[][..], &[][..]),
            K::Operation(_, operands) | K::StorageValue(_, operands) | K::Predicate(_, _, operands)
            | K::Call(_, _, _, operands, _) => (None, None, operands.as_ref(), &[][..]),
            K::Boolean(_) | K::Variant(_) | K::Literal(_) | K::Input(_) | K::Result
            | K::Expression(_) | K::ScopedCapability(_) | K::PostState(_, _)
            | K::Joined(..) | K::Unknown => (None, None, &[][..], &[][..]),
        };

        first.into_iter().chain(second).chain(operands).chain(fields.iter().map(|(_, value)| value))
    }

    /// One bit for an identified value or storage root. Input paths share their
    /// root bit with every prefix, because rewriting also matches input prefixes.
    pub(super) fn root_bit(&self) -> u64 {
        let mut hash = std::hash::DefaultHasher::new();

        match self {
            K::Input(place) => (0_u8, &place.root).hash(&mut hash),
            K::Joined(site, place) => (1_u8, site, &place.root).hash(&mut hash),
            K::InitializedSnapshot(place, _, _) => (0_u8, &place.root).hash(&mut hash),
            K::Expression(expression) => (2_u8, expression).hash(&mut hash),
            K::ScopedCapability(expression) => (3_u8, expression).hash(&mut hash),
            K::PostState(mutation, _) => (4_u8, mutation).hash(&mut hash),
            K::Result => 5_u8.hash(&mut hash),
            _ => return 0,
        }

        1 << (hash.finish() % 64)
    }

    /// The bits of every mutation that `depends_on` can match at this node.
    pub(super) fn mutation_bits(&self) -> u64 {
        let pointee = |place: &crate::ExecutionPlace| match &place.root {
            crate::execution_guarantees::ExecutionInput::Pointee { producer: Some(producer), .. } => mutation_bit(*producer),
            _ => 0,
        };

        match self {
            K::Expression(expression) | K::ScopedCapability(expression) =>
                mutation_bit(StorageMutation::Operation((*expression).into()).occurrence()),
            // An installed assignment shares its expression with the operation it installs.
            K::PostState(site, _) => mutation_bit(site.occurrence()),
            K::Joined(site, place) => pointee(place) | match site {
                super::ExecutionJoinSite::Publication(expression) =>
                    mutation_bit(bray_bound_tree::BoundExecutionSite::Node((*expression).into())),
                _ => 0,
            },
            K::Input(place) | K::InitializedSnapshot(place, _, _) => pointee(place),
            _ => 0,
        }
    }

    /// Metadata belongs to the complete representation, including immutable epoch origins.
    /// Visits the operands, a post-state's origin and a deferred frame's
    /// conditions until `visit` breaks. The frame's conditions form a large
    /// iterator, so they are visited out of line to keep callers' frames small.
    pub(super) fn try_for_each_structural_child<'a>(
        &'a self,
        visit: &mut dyn FnMut(&'a ExecutionCondition) -> ControlFlow<()>,
    ) -> ControlFlow<()> {
        for child in self.children() {
            visit(child)?;
        }

        match self {
            K::PostState(_, origin) => visit(origin),
            K::DeferredValue { frame, .. } => visit_frame_conditions(frame, visit),
            _ => ControlFlow::Continue(()),
        }
    }
}

impl ExecutionCondition {
    /// Actual frame records anywhere in this value, including nested captured operands.
    pub(crate) fn deferred_values(&self) -> Vec<&Arc<super::super::ExecutionDeferredValue>> {
        if !self.contains_deferred() { return Vec::new(); }

        let mut pending = vec![self];
        let mut visited = HashSet::new();
        let mut frames = Vec::new();

        while let Some(value) = pending.pop() {
            if !visited.insert(NodeIdentity(value)) { continue; }

            if let K::DeferredValue { frame, .. } = value.kind() { frames.push(frame); }

            let _ = value.kind().try_for_each_structural_child(&mut |child| {
                if child.contains_deferred() { pending.push(child); }

                ControlFlow::Continue(())
            });
        }

        frames
    }

    fn visit(&self, mut inspect: impl FnMut(&Self) -> ControlFlow<()>) -> ControlFlow<()> {
        let mut pending = vec![self];
        let mut visited = HashSet::new();

        while let Some(condition) = pending.pop() {
            // Dependency queries check existence or collect sets. A second
            // handle to the same node supplies the same dependency facts.
            let node = std::ptr::from_ref(condition.kind());

            if !visit_once(&mut visited, node, !pending.is_empty()) { continue; }

            inspect(condition)?;

            pending.extend(condition.children());
        }

        ControlFlow::Continue(())
    }

    pub(crate) fn depends_on(&self, mutation: StorageMutation) -> bool {
        if !self.may_depend_on(mutation) {
            return false;
        }

        self.visit(|condition| {
            let depends = match condition.kind() {
                K::Expression(candidate) | K::ScopedCapability(candidate) => mutation == StorageMutation::Operation((*candidate).into()),
                K::PostState(site, _) => *site == mutation
                    || matches!(site, StorageMutation::AssignmentInstall(expression) if mutation == StorageMutation::Operation((*expression).into())),
                K::Joined(site, place) => matches!(site, super::ExecutionJoinSite::Publication(expression)
                    if mutation.occurrence().expression() == Some(*expression)) || place.depends_on(mutation),
                K::Input(place) | K::InitializedSnapshot(place, _, _) => place.depends_on(mutation),
                _ => false,
            };

            if depends { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
        }).is_break()
    }

    pub(super) fn contains_equivalence(&self) -> bool {
        self.visit(|condition| if matches!(condition.kind(), K::ValueAlias(_, _) | K::Operation(bray_bound_tree::BoundOperator::Equal | bray_bound_tree::BoundOperator::NotEqual, _)) {
            ControlFlow::Break(())
        } else { ControlFlow::Continue(()) }).is_break()
    }

    /// Mutations already retained by the current values, without reconstructing invalidation history.
    pub(crate) fn observed_mutations(&self) -> BTreeSet<StorageMutation> {
        let mut changes = BTreeSet::new();

        let _ = self.visit(|condition| {
            if let K::PostState(mutation, _) = condition.kind() {
                changes.insert(*mutation);
            }

            ControlFlow::Continue(())
        });

        changes
    }
}
