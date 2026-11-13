use std::collections::{BTreeSet, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::ControlFlow;

use super::ExecutionCondition;
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

            match condition {
                Self::ValueAlias(left, right) => pending.extend([left.as_ref(), right.as_ref()]),
                Self::ScalarValue(value) | Self::Discriminant(value) | Self::Descriptor(_, value) => pending.push(value),
                Self::Observation { owner, contents, .. } => pending.extend([owner.as_ref(), contents.as_ref()]),
                Self::Constructed(base, fields) => {
                    pending.push(base);
                    pending.extend(fields.iter().map(|(_, value)| value));
                }
                Self::Operation(_, operands) | Self::Predicate(_, _, operands) | Self::Call(_, _, _, operands, _) => pending.extend(operands.iter()),
                Self::Tagged(_, value) | Self::VariantTest(_, value) | Self::Projection(_, value)
                    | Self::Trusted(value) | Self::Borrowed(value) | Self::Present(value)
                    | Self::Entry { condition: value, .. } | Self::CapturedInput { owner: value, .. } => pending.push(value),
                Self::InitializedSnapshot(_, join, value) => pending.extend([join.as_ref(), value.as_ref()]),
                Self::Boolean(_) | Self::Variant(_) | Self::Literal(_) | Self::Input(_) | Self::Result
                    | Self::Expression(_) | Self::ScopedCapability(_) | Self::PostState(_, _)
                    | Self::Joined(_, _, _) | Self::Unknown => {}
            }
        }

        ControlFlow::Continue(())
    }

    pub(crate) fn depends_on(&self, mutation: StorageMutation) -> bool {
        self.visit(|condition| {
            let depends = match condition {
                Self::Expression(candidate) | Self::ScopedCapability(candidate) => mutation == StorageMutation::Operation((*candidate).into()),
                Self::PostState(site, _) => *site == mutation,
                _ => false,
            };

            if depends { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
        }).is_break()
    }

    pub(super) fn contains_equivalence(&self) -> bool {
        self.visit(|condition| if matches!(condition, Self::ValueAlias(_, _) | Self::Operation(bray_bound_tree::BoundOperator::Equal | bray_bound_tree::BoundOperator::NotEqual, _)) {
            ControlFlow::Break(())
        } else { ControlFlow::Continue(()) }).is_break()
    }

    /// Mutations already retained by the current values, without reconstructing invalidation history.
    pub(crate) fn observed_mutations(&self) -> BTreeSet<StorageMutation> {
        let mut changes = BTreeSet::new();

        let _ = self.visit(|condition| {
            if let Self::PostState(mutation, _) = condition {
                changes.insert(*mutation);
            }

            ControlFlow::Continue(())
        });

        changes
    }
}
