use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::ExecutionCondition;
use super::traversal::NodeIdentity;

impl Hash for ExecutionCondition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if matches!(self, Self::Boolean(_) | Self::Variant(_) | Self::Literal(_) | Self::Input(_)
            | Self::Expression(_) | Self::ScopedCapability(_) | Self::Joined(..) | Self::Result | Self::Unknown) {
            self.hash_linear(state);
        } else {
            self.structural_hash(&mut HashMap::new(), false).hash(state);
        }
    }
}

fn hash_operands<'a, H: Hasher>(operands: &'a [ExecutionCondition], state: &mut H,
    child: &mut impl FnMut(&'a ExecutionCondition, bool, &mut H)) {
    operands.len().hash(state);

    for (index, operand) in operands.iter().enumerate() { child(operand, index + 1 < operands.len(), state); }
}

impl ExecutionCondition {
    fn hash_linear<H: Hasher>(&self, state: &mut H) {
        self.hash_parts(state, &mut |value, _, state| value.hash_linear(state));
    }

    fn structural_hash<'a>(&'a self, memo: &mut HashMap<NodeIdentity<'a>, u64>, has_pending: bool) -> u64 {
        if let Some(hash) = memo.get(&NodeIdentity(self)) { return *hash; }

        let mut hash = std::hash::DefaultHasher::new();

        self.hash_parts(&mut hash, &mut |value, later, state| {
            value.structural_hash(memo, has_pending || later).hash(state);
        });

        let hash = hash.finish();

        if has_pending { memo.insert(NodeIdentity(self), hash); }

        hash
    }

    pub(super) fn hash_parts<'a, H: Hasher>(&'a self, state: &mut H, child: &mut impl FnMut(&'a Self, bool, &mut H)) {
        std::mem::discriminant(self).hash(state);

        match self {
            Self::Boolean(value) => value.hash(state),
            Self::Variant(value) => value.hash(state),
            Self::Literal(value) => value.hash(state),
            Self::Input(place) => place.hash(state),
            Self::Expression(value) | Self::ScopedCapability(value) => value.hash(state),
            Self::Result | Self::Unknown => {}
            Self::Tagged(variant, value) | Self::VariantTest(variant, value) => {
                variant.hash(state);
                child(value, false, state);
            }
            Self::Discriminant(value) | Self::Present(value) | Self::ScalarValue(value)
            | Self::Trusted(value) | Self::Borrowed(value) => child(value, false, state),
            Self::Descriptor(kind, value) => {
                kind.hash(state);
                child(value, false, state);
            }
            Self::CapturedInput { owner, invocation, ordinal } => {
                (invocation, ordinal).hash(state);
                child(owner, false, state);
            }
            Self::Constructed(base, fields) => {
                child(base, !fields.is_empty(), state);
                fields.len().hash(state);

                for (index, (field, value)) in fields.iter().enumerate() {
                    field.hash(state);
                    child(value, index + 1 < fields.len(), state);
                }
            }
            Self::Observation { owner, contents, kind } => {
                kind.hash(state);
                child(owner, true, state);
                child(contents, false, state);
            }
            Self::PostState(mutation, owner) => {
                mutation.hash(state);
                child(owner, false, state);
            }
            Self::Joined(unit, block, place) => (unit, block, place).hash(state),
            Self::ValueAlias(left, right) => {
                child(left, true, state);
                child(right, false, state);
            }
            Self::InitializedSnapshot(place, join, value) => {
                place.hash(state);
                child(join, true, state);
                child(value, false, state);
            }
            Self::Operation(operator, operands) => {
                operator.hash(state);
                hash_operands(operands, state, child);
            }
            Self::Predicate(predicate, substitution, operands) => {
                (predicate, substitution).hash(state);
                hash_operands(operands, state, child);
            }
            Self::Call(callable, implementation, witnesses, operands, reference_type) => {
                (callable, implementation, witnesses, reference_type).hash(state);
                hash_operands(operands, state, child);
            }
            Self::Projection(field, value) => {
                field.hash(state);
                child(value, false, state);
            }
            Self::Entry { condition, captured } => {
                captured.hash(state);
                child(condition, false, state);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};

    use super::ExecutionCondition;

    fn tree(depth: usize) -> ExecutionCondition {
        if depth == 0 { return ExecutionCondition::Boolean(false); }

        ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LogicalOr,
            vec![tree(depth - 1), tree(depth - 1)].into())
    }

    #[test]
    fn shared_and_independent_equal_graphs_hash_identically_without_expanding_shared_nodes() {
        let mut shared = ExecutionCondition::Boolean(false);

        for _ in 0..10 {
            shared = ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LogicalOr,
                vec![shared.clone(), shared].into());
        }

        let independent = tree(10);

        let digest = |value: &ExecutionCondition| {
            let mut hash = std::hash::DefaultHasher::new();

            value.hash(&mut hash);

            hash.finish()
        };

        assert_eq!(shared, independent);
        assert_eq!(digest(&shared), digest(&independent));

        let mut memo = HashMap::new();

        shared.structural_hash(&mut memo, false);
        assert!(memo.len() <= 21, "hashing scales with the shared graph rather than the expanded tree");
    }
}
