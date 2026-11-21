use bray_bound_tree::{AnyBoundNodeId, BoundAssignmentOperator, BoundExecutionSite, BoundExpression, StorageAccessId, StorageAddressValue};

use super::super::core::StorageFlowCollector;
use crate::analysis::model::AnalysisOperationKind;
use crate::analysis::storage_flow::memory::{Place, Source, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn prepare_assignment(
        &self,
        state: &mut StorageFlowState,
        operation: AnalysisOperationKind,
        access: StorageAccessId,
        address: Option<&StorageAddressValue>,
    ) -> Vec<Target> {
        let Some(expression) = operation.occurrence().expression() else { return Vec::new(); };

        let Some(BoundExpression::Assignment(assignment)) = self.request.view().expression(expression) else {
            return Vec::new();
        };

        let [_, operand] = assignment.operands() else {
            unreachable!("checked assignment has a destination and stored value");
        };

        let site = operation.occurrence();

        let operator_result = matches!(operation, AnalysisOperationKind::AssignmentInstall(_))
            .then(|| self.storage.invocation_result(site).expect("operator has its checked output slot"));

        let (owner, place, source) = if let Some(result) = operator_result {
            let place = self.storage.logical_path(result).expect("operator result has a complete storage path").clone();
            let source = Source::Value(Target::storage(place.clone()));

            (site, Some(place), Some(source))
        } else {
            ((*operand).into(), self.argument_storage(*operand), self.expression_reference(state, *operand))
        };

        let roots = self.assignment_escape_regions(state, owner, place, source.as_ref());

        if operator_result.is_some() || matches!(operation, AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(_))) {
            state.memory.begin_transition(site);

            if let Some(destinations) = address.and_then(StorageAddressValue::regions) {
                if !address.is_some_and(StorageAddressValue::is_exact) {
                    state.memory.weaken_transition(site);
                }

                let ty = self.storage.access(access).expect("assignment destination is planned").reached_type();

                let projections = if let Some(result) = operator_result {
                    let source_type = self.storage.access(result).expect("operator result retains its type").reached_type();

                    crate::storage::nullable_wrapping(self.request.semantic_values(), source_type, ty)
                } else if assignment.operator() == BoundAssignmentOperator::Assign {
                    self.reference_wrapping(*operand, ty)
                } else {
                    None
                };

                if let Some(projections) = projections
                    && let Some(source) = source
                {
                    for destination in destinations {
                        source.clone().stage(&mut state.memory, site, destination.project(&projections));
                    }
                } else {
                    let (complete, references) = self.input.footprint.reference_shape(ty);

                    for destination in destinations {
                        state.memory.stage_unresolved_value(site, destination, complete, references.iter().copied());
                    }
                }
            }
        }

        roots
    }

    fn assignment_escape_regions(
        &self,
        state: &StorageFlowState,
        argument: BoundExecutionSite,
        place: Option<Place>,
        source: Option<&Source>,
    ) -> Vec<Target> {
        let mut roots = Vec::new();
        let regions = source.and_then(|source| source.regions(&state.memory));

        if regions.is_none()
            && let Some(regions) = place.as_ref().and_then(|place| {
                self.input
                    .inline_regions(&state.memory, self.storage, place)
            })
        {
            return regions;
        }

        if let Some(place) = &place {
            roots.extend(
                state
                    .memory
                    .contained_regions(&Target::storage(place.clone()))
                    .cloned(),
            );

            if regions.is_none() && self.input.reference_free(self.storage, place).is_none() {
                // Unknown value contents remain symbolic entry values until instantiation.
                roots.push(Target::storage(place.clone()));
            }
        }

        if let Some(regions) = regions {
            // The physical target is authoritative. Its lifetime anchor need
            // not be reachable through the reference, as with a lock guard.
            roots.extend(regions);
        } else {
            // Opaque task representations can retain a loan without
            // publishing a physical field. Its possible targets bound effects,
            // but never establish exact reference equality.
            for dependency in self.liveness.retained_by(argument) {
                if let bray_bound_tree::BoundDependencySubject::BorrowCapability(capability) =
                    dependency
                {
                    roots.extend(
                        state
                            .memory
                            .borrow_regions(capability)
                            .into_iter()
                            .flatten(),
                    );
                }
            }
        }

        roots
    }
}
