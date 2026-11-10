use std::collections::BTreeSet;
use std::sync::Arc;

use super::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition};
use bray_symbols::{DependencyBorrowRelation, DependencyReferenceKind, DependencyReferenceProvenance,
    DependencySubjectRoot};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    /// Resolve only complete forwarded origins that still select one address.
    pub(in crate::analysis::guarantee) fn reference_conditions(
        &self, condition: &ExecutionCondition, ordinary: &BTreeSet<(ExecutionCondition, bool)>,
    ) -> ExecutionCondition {
        if self.dependencies.condition_references().is_empty() { return condition.clone(); }

        let result = crate::map_execution_condition_references(condition,
            &Ok::<_, std::convert::Infallible>,
            &|place| Ok(ExecutionCondition::Input(place.clone())), &|value| Ok(Arc::clone(value)),
            &mut |callable, dispatch, witnesses, operands, reference_type| {
                let witnesses: Arc<[_]> = witnesses.into();
                let origin = reference_type.and_then(|_| {
                    self.dependencies.condition_references().get(&(callable, dispatch, Arc::clone(&witnesses)))
                        .and_then(|(receiver, relations)| self.selected_reference_origin(
                            *receiver, relations, &operands, ordinary))
                });

                Ok(origin.unwrap_or_else(|| ExecutionCondition::Call(
                    callable, dispatch, witnesses, operands.into(), reference_type)))
            });

        result.unwrap_or_else(|error| match error {})
    }

    fn selected_reference_origin(
        &self, receiver: bool, relations: &[DependencyBorrowRelation], arguments: &[ExecutionCondition],
        ordinary: &BTreeSet<(ExecutionCondition, bool)>,
    ) -> Option<ExecutionCondition> {
        let mut origins = BTreeSet::new();

        'origins: for relation in relations.iter().filter(|relation|
            relation.exit().subject_root() == DependencySubjectRoot::Result
                && relation.exit().projections().is_empty()) {
            if relation.kind() != DependencyReferenceKind::Borrow
                || relation.provenance() == DependencyReferenceProvenance::InputRegions {
                return None;
            }

            let entry = relation.entry()?;
            let ordinal = match entry.subject_root() {
                DependencySubjectRoot::Receiver => 0,
                DependencySubjectRoot::Parameter(ordinal) => ordinal.to_index()? + usize::from(receiver),
                _ => return None,
            };
            let mut origin = arguments.get(ordinal)?.clone();
            let projections = crate::storage::storage_projections(
                self.request.semantic_values(), self.dependencies, entry.projections())?;

            for projection in projections {
                origin = origin.project(projection);
            }

            for _ in 0..=relation.dereferences() {
                origin = origin.into_referent();
            }

            let projections = crate::storage::storage_projections(
                self.request.semantic_values(), self.dependencies, relation.target_projections())?;

            for projection in projections {
                if let bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, .. } = projection
                    && origin.clone().matches_variant(variant).prove(ordinary,
                        &mut { ExecutionCondition::WORK_LIMIT }) == Some(false) {
                    continue 'origins;
                }

                origin = origin.project(projection);
            }

            if origin.is_unknown() { return None; }

            origins.insert(origin.borrowed());
        }

        (origins.len() == 1).then(|| origins.into_iter().next().expect("one forwarded origin remains"))
    }
}
