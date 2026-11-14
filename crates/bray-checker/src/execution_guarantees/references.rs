use std::collections::BTreeSet;
use std::sync::Arc;

use super::{ExecutionCondition, ExecutionConditionKind as K, ExecutionPlace};
use bray_bound_tree::CheckedDependencyContracts;
use bray_symbols::{DependencyReferenceKind, DependencyReferenceProvenance, DependencySubjectRoot, SemanticValueStore};

impl ExecutionCondition {
    /// Resolve complete forwarded origins only when branch facts leave one address.
    pub(crate) fn with_reference_origins(&self, values: &SemanticValueStore,
        dependencies: &CheckedDependencyContracts, ordinary: &BTreeSet<(Self, bool)>) -> Self {
        if dependencies.condition_references().is_empty() { return self.clone(); }

        let result = super::map_execution_condition_references(self,
            &Ok::<_, std::convert::Infallible>,
            &|place| Ok(Self::new(K::Input(place.clone()))), &|value| Ok(Arc::clone(value)),
            &mut |callable, dispatch, witnesses, operands, reference_type| {
                let call = Self::new(K::Call(callable, dispatch, witnesses.into(), operands.into(), reference_type));

                let selected = call.reference_origins(values, dependencies, Some(ordinary))
                    .and_then(|(mut origins, complete)| (complete && origins.len() == 1)
                        .then(|| origins.pop_first().expect("one forwarded origin remains")));

                Ok(selected.unwrap_or(call))
            });

        self.reuse_unchanged(result.unwrap_or_else(|error| match error {}))
    }

    /// Retain actual carrier paths and defer marker containment until origins are known.
    pub(crate) fn carrier_inputs(&self, values: &SemanticValueStore,
        dependencies: &CheckedDependencyContracts, containing_value: bool) -> Vec<ExecutionPlace> {
        let mut pending = vec![(self.clone(), Vec::new())];
        let mut carriers = BTreeSet::new();
        let mut remaining = Self::WORK_LIMIT;

        while let Some((subject, mut projections)) = pending.pop() {
            remaining = remaining.checked_sub(1).expect("bounded carrier subject traversal retains its finite condition tree");

            match subject.kind() {
                K::Borrowed(value) | K::Observation { contents: value, .. } =>
                    pending.push((value.as_ref().clone(), projections)),
                K::Projection(projection, value) => {
                    projections.push(*projection);
                    pending.push((value.as_ref().clone(), projections));
                }
                K::Call(_, _, _, _, Some(_)) => {
                    let (origins, _) = subject.reference_origins(values, dependencies, None)
                        .expect("symbolic reference carriers retain prepared result origins");

                    pending.extend(origins.into_iter().map(|origin| (origin, projections.clone())));
                }
                _ => {
                    let value = projections.into_iter().rev().fold(subject.clone(), Self::component)
                        .witness_carrier(containing_value);

                    carriers.extend(value.inputs().into_iter().cloned());
                }
            }
        }

        carriers.into_iter().collect()
    }

    fn reference_origins(&self, values: &SemanticValueStore, dependencies: &CheckedDependencyContracts,
        ordinary: Option<&BTreeSet<(Self, bool)>>) -> Option<(BTreeSet<Self>, bool)> {
        let K::Call(callable, dispatch, witnesses, arguments, Some(_)) = self.kind() else { return None; };

        let (receiver, relations) = dependencies.condition_references()
            .get(&(*callable, *dispatch, Arc::clone(witnesses)))?;

        let mut origins = BTreeSet::new();
        let mut complete = true;

        'origins: for relation in relations.iter().filter(|relation|
            relation.exit().subject_root() == DependencySubjectRoot::Result
                && relation.exit().projections().is_empty()) {
            let Some(entry) = relation.entry() else { complete = false; continue; };

            let ordinal = match entry.subject_root() {
                DependencySubjectRoot::Receiver => 0,
                DependencySubjectRoot::Parameter(ordinal) => ordinal.to_index()? + usize::from(*receiver),
                _ => return None,
            };

            let mut origin = arguments.get(ordinal)?.clone();

            if relation.kind() != DependencyReferenceKind::Borrow
                || relation.provenance() == DependencyReferenceProvenance::InputRegions
                || relation.target_projections().contains(&bray_symbols::DependencyProjection::AnyElement) {
                complete = false;
                origins.insert(origin);
                continue;
            }

            for (path, dereference) in [(entry.projections(), false), (relation.target_projections(), true)] {
                if dereference {
                    for _ in 0..=relation.dereferences() { origin = origin.into_referent(); }
                }

                for projection in crate::storage::storage_projections(values, dependencies, path)? {
                    if let bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, .. } = projection
                        && ordinary.is_some_and(|ordinary| origin.clone().matches_variant(variant)
                            .prove(ordinary, &mut { Self::WORK_LIMIT }) == Some(false)) {
                        continue 'origins;
                    }

                    origin = origin.project(projection);
                }
            }

            if origin.is_unknown() { return None; }

            origins.insert(origin.borrowed());
        }

        Some((origins, complete))
    }
}
