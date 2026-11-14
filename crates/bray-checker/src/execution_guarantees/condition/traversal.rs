use std::collections::{BTreeSet, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::ControlFlow;

use super::{ExecutionCondition, ExecutionConditionKind as K};
use bray_bound_tree::StorageMutation;

/// Physical sharing only, never semantic value or owner identity.
#[derive(Clone, Copy)]
pub(in crate::execution_guarantees) struct NodeIdentity<'a>(pub(in crate::execution_guarantees) &'a ExecutionCondition);

impl PartialEq for NodeIdentity<'_> {
    fn eq(&self, other: &Self) -> bool { std::ptr::eq(self.0, other.0) }
}

impl Eq for NodeIdentity<'_> {}

impl Hash for NodeIdentity<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) { std::ptr::hash(self.0, state); }
}

/// Acyclic value graphs need remembered nodes only while another path is pending.
pub(super) fn visit_once<K: Eq + Hash>(visited: &mut HashSet<K>, node: K, has_pending: bool) -> bool {
    if has_pending { visited.insert(node) } else { !visited.contains(&node) }
}

impl ExecutionCondition {
    fn visit(&self, mut inspect: impl FnMut(&Self) -> ControlFlow<()>) -> ControlFlow<()> {
        let mut pending = vec![self];
        let mut visited = HashSet::new();

        while let Some(condition) = pending.pop() {
            if !visit_once(&mut visited, NodeIdentity(condition), !pending.is_empty()) { continue; }

            inspect(condition)?;

            match condition.kind() {
                K::ValueAlias(left, right) => pending.extend([left.as_ref(), right.as_ref()]),
                K::ScalarValue(value) | K::Discriminant(value) | K::Descriptor(_, value) => pending.push(value),
                K::Observation { owner, contents, .. } => pending.extend([owner.as_ref(), contents.as_ref()]),
                K::Constructed(base, fields) => {
                    pending.push(base);
                    pending.extend(fields.iter().map(|(_, value)| value));
                }
                K::Operation(_, operands) | K::Predicate(_, _, operands) | K::Call(_, _, _, operands, _) => pending.extend(operands.iter()),
                K::Tagged(_, value) | K::VariantTest(_, value) | K::Projection(_, value)
                    | K::Trusted(value) | K::Borrowed(value) | K::Present(value)
                    | K::Entry { condition: value, .. } | K::CapturedInput { owner: value, .. } => pending.push(value),
                K::InitializedSnapshot(_, join, value) => pending.extend([join.as_ref(), value.as_ref()]),
                K::Boolean(_) | K::Variant(_) | K::Literal(_) | K::Input(_) | K::Result
                    | K::Expression(_) | K::ScopedCapability(_) | K::PostState(_, _)
                    | K::Joined(_, _, _) | K::Unknown => {}
            }
        }

        ControlFlow::Continue(())
    }

    pub(crate) fn depends_on(&self, mutation: StorageMutation) -> bool {
        self.visit(|condition| {
            let depends = match condition.kind() {
                K::Expression(candidate) | K::ScopedCapability(candidate) => mutation == StorageMutation::Operation((*candidate).into()),
                K::PostState(site, _) => *site == mutation,
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
