use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::ExecutionCondition;
use super::traversal::NodeIdentity;

impl Hash for ExecutionCondition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.structural_hash(&mut HashMap::new()).hash(state);
    }
}

impl ExecutionCondition {
    fn structural_hash<'a>(&'a self, memo: &mut HashMap<NodeIdentity<'a>, u64>) -> u64 {
        if let Some(hash) = memo.get(&NodeIdentity(self)) { return *hash; }

        let mut hash = std::hash::DefaultHasher::new();
        std::mem::discriminant(self).hash(&mut hash);
        let mut recursive = false;
        let mut child = |value: &'a Self| {
            recursive = true;

            value.structural_hash(memo)
        };

        match self {
            Self::Boolean(value) => value.hash(&mut hash),
            Self::Variant(value) => value.hash(&mut hash),
            Self::Literal(value) => value.hash(&mut hash),
            Self::Input(place) => place.hash(&mut hash),
            Self::Expression(value) | Self::ScopedCapability(value) => value.hash(&mut hash),
            Self::Result | Self::Unknown => {}
            Self::Tagged(variant, value) | Self::VariantTest(variant, value) => {
                variant.hash(&mut hash);
                child(value).hash(&mut hash);
            }
            Self::Discriminant(value) | Self::Present(value) | Self::ScalarValue(value)
            | Self::Trusted(value) | Self::Borrowed(value) => child(value).hash(&mut hash),
            Self::Descriptor(kind, value) => {
                kind.hash(&mut hash);
                child(value).hash(&mut hash);
            }
            Self::CapturedInput { owner, invocation, ordinal } => {
                (invocation, ordinal).hash(&mut hash);
                child(owner).hash(&mut hash);
            }
            Self::Constructed(base, fields) => {
                child(base).hash(&mut hash);
                fields.len().hash(&mut hash);

                for (field, value) in fields.iter() {
                    field.hash(&mut hash);
                    child(value).hash(&mut hash);
                }
            }
            Self::Observation { owner, contents, kind } => {
                kind.hash(&mut hash);
                child(owner).hash(&mut hash);
                child(contents).hash(&mut hash);
            }
            Self::PostState(mutation, owner) => {
                mutation.hash(&mut hash);
                child(owner).hash(&mut hash);
            }
            Self::Joined(unit, block, place) => (unit, block, place).hash(&mut hash),
            Self::ValueAlias(left, right) => {
                child(left).hash(&mut hash);
                child(right).hash(&mut hash);
            }
            Self::InitializedSnapshot(place, join, value) => {
                place.hash(&mut hash);
                child(join).hash(&mut hash);
                child(value).hash(&mut hash);
            }
            Self::Operation(operator, operands) => {
                operator.hash(&mut hash);
                operands.len().hash(&mut hash);

                for operand in operands.iter() { child(operand).hash(&mut hash); }
            }
            Self::Predicate(predicate, substitution, operands) => {
                (predicate, substitution).hash(&mut hash);
                operands.len().hash(&mut hash);

                for operand in operands.iter() { child(operand).hash(&mut hash); }
            }
            Self::Call(callable, implementation, witnesses, operands, reference_type) => {
                (callable, implementation, witnesses, reference_type).hash(&mut hash);
                operands.len().hash(&mut hash);

                for operand in operands.iter() { child(operand).hash(&mut hash); }
            }
            Self::Projection(field, value) => {
                field.hash(&mut hash);
                child(value).hash(&mut hash);
            }
            Self::Entry { condition, captured } => {
                captured.hash(&mut hash);
                child(condition).hash(&mut hash);
            }
        }

        let hash = hash.finish();

        // Only composite nodes need memo entries. Equal values hash identically
        // regardless of whether their immutable operands happen to share storage.
        if recursive { memo.insert(NodeIdentity(self), hash); }

        hash
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
        shared.structural_hash(&mut memo);
        assert!(memo.len() <= 21, "hashing scales with the shared graph rather than the expanded tree");
    }
}
