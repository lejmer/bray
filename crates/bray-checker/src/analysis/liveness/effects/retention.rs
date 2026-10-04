use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundDependencySubject, BoundExpressionId, StorageBinding, StorageIdentityId,
    StoragePlan,
};

use super::core::OperationEffects;
use crate::analysis::storage_index::index_storage_roots;
use crate::dependency::ValueInputs;
use crate::storage::value_transfer_bindings;
use crate::{CheckerRequestContext, CheckerUnitView};

impl OperationEffects {
    pub(super) fn retain_owned_call_dependencies<C>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
    ) where
        C: CheckerRequestContext + ?Sized,
    {
        let (accesses_by_root, _) = index_storage_roots(storage);

        let initializations = value_transfer_bindings(request, storage);

        let value_sources_by_root =
            index_value_sources_by_root(storage, &initializations, &self.value_inputs);

        for expression in self.owner_dependencies.keys().copied().collect::<Vec<_>>() {
            let nested_borrows = retained_storage_borrows(
                storage,
                &value_sources_by_root,
                self.owner_dependencies
                    .get(&expression)
                    .into_iter()
                    .flat_map(|subjects| subjects.iter().copied()),
                self,
            );

            self.owner_dependencies
                .entry(expression)
                .or_default()
                .extend(nested_borrows);
        }

        // Keep seed contracts immutable while local ownership propagation reaches its fixed point.
        let mut retained_by_expression = self.owner_dependencies.clone();

        loop {
            let mut changed = false;

            for (initializer, bindings) in &initializations {
                let retained = retained_subtree_subjects(
                    &self.value_inputs,
                    *initializer,
                    &retained_by_expression,
                );

                if retained.is_empty() {
                    continue;
                }

                for binding in bindings {
                    let root = match binding {
                        StorageBinding::Identity(identity) => Some(*identity),
                        StorageBinding::Access(access) => storage.root_identity(*access),
                    };

                    let Some(root) = root else {
                        continue;
                    };

                    for expression in accesses_by_root.get(&root).into_iter().flatten() {
                        if self.value_inputs.is_independent(*expression)
                            || self
                                .value_inputs
                                .projected_initializer(*expression)
                                .is_some()
                        {
                            continue;
                        }

                        let expression_retention =
                            retained_by_expression.entry(*expression).or_default();

                        let previous = expression_retention.len();

                        expression_retention.extend(retained.iter().copied());
                        changed |= expression_retention.len() != previous;
                    }
                }
            }

            if !changed {
                break;
            }
        }

        for (expression, retained) in &retained_by_expression {
            let node = AnyBoundNodeId::Expression(*expression);
            let defined = self.by_node.get(&node).map(|effect| &effect.definitions);

            let subjects = retained
                .iter()
                .copied()
                .filter(|subject| !defined.is_some_and(|definitions| definitions.contains(subject)))
                .collect::<Vec<_>>();

            self.extend_uses(*expression, subjects);
        }

        self.owner_dependencies = retained_by_expression;
    }
}

fn index_value_sources_by_root(
    storage: &StoragePlan,
    initializations: &BTreeMap<BoundExpressionId, Vec<StorageBinding>>,
    inputs: &ValueInputs,
) -> BTreeMap<StorageIdentityId, Vec<BoundExpressionId>> {
    let mut result = BTreeMap::<_, Vec<_>>::new();

    for (initializer, bindings) in initializations {
        for binding in bindings {
            let root = match binding {
                StorageBinding::Identity(identity) => Some(*identity),
                StorageBinding::Access(access) => storage.root_identity(*access),
            };

            if let Some(root) = root {
                result.entry(root).or_default().push(*initializer);
            }
        }
    }

    for plan in storage.access_plans() {
        let Some(root) = storage.root_identity(plan.access()) else {
            continue;
        };

        result.entry(root).or_default().extend(
            inputs
                .projected_operands(plan.expression())
                .map(|(value, path)| inputs.project(value, &path).0),
        );
    }

    result
}

fn retained_storage_borrows(
    storage: &StoragePlan,
    value_sources_by_root: &BTreeMap<StorageIdentityId, Vec<BoundExpressionId>>,
    subjects: impl IntoIterator<Item = BoundDependencySubject>,
    effects: &OperationEffects,
) -> BTreeSet<BoundDependencySubject> {
    let mut borrows = BTreeSet::new();
    let mut pending = subjects.into_iter().collect::<Vec<_>>();
    let mut visited_roots = BTreeSet::new();

    while let Some(subject) = pending.pop() {
        match subject {
            BoundDependencySubject::BorrowCapability(_) => {
                borrows.insert(subject);
            }
            BoundDependencySubject::Storage(root) => {
                if !visited_roots.insert(root) {
                    continue;
                }

                for initializer in value_sources_by_root.get(&root).into_iter().flatten() {
                    pending.extend(retained_subtree_subjects(
                        &effects.value_inputs,
                        *initializer,
                        &effects.owner_dependencies,
                    ));
                }

                if let Some(bray_bound_tree::StorageIdentity::Temporary(expression)) =
                    storage.identity(root)
                {
                    pending.extend(retained_subtree_subjects(
                        &effects.value_inputs,
                        expression,
                        &effects.owner_dependencies,
                    ));
                }
            }
            BoundDependencySubject::StorageAccess(access) => {
                if let Some(root) = storage.root_identity(access) {
                    pending.push(BoundDependencySubject::Storage(root));
                }
            }
            BoundDependencySubject::ScopedCapability(_)
            | BoundDependencySubject::ImplementationWitness(_)
            | BoundDependencySubject::ProductStatic(_)
            | BoundDependencySubject::ExactThreadStatic(_)
            | BoundDependencySubject::LifecycleObligation(_) => {}
        }
    }

    borrows
}

pub(super) fn retained_subtree_subjects(
    inputs: &ValueInputs,
    root: BoundExpressionId,
    retained_by_expression: &BTreeMap<BoundExpressionId, BTreeSet<BoundDependencySubject>>,
) -> BTreeSet<BoundDependencySubject> {
    let mut retained = BTreeSet::new();
    let mut pending = vec![root];
    let mut visited = BTreeSet::new();

    while let Some(expression) = pending.pop() {
        if !visited.insert(expression) || inputs.is_independent(expression) {
            continue;
        }

        if let Some(subjects) = retained_by_expression.get(&expression) {
            retained.extend(subjects.iter().copied());
        }

        pending.extend(inputs.operands(expression));

        pending.extend(
            inputs
                .projected_operands(expression)
                .map(|(value, path)| inputs.project(value, &path).0),
        );
    }

    retained
}
