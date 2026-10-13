use std::collections::BTreeMap;

use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Place, Source, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{BoundExecutionSite, BoundExpressionId, SemanticSelection};
use bray_symbols::DependencySubjectRoot;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_borrow_origins(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
    ) -> std::collections::BTreeMap<Place, std::collections::BTreeSet<Place>> {
        let mut borrow_origins = BTreeMap::new();

        if let Some(bray_bound_tree::SemanticSelection::Call(call)) = occurrence
            .expression()
            .and_then(|expression| self.execution.semantics.selections().expression(expression))
        {
            let arguments = call
                .arguments()
                .iter()
                .filter_map(|argument| match argument {
                    bray_bound_tree::SelectedArgument::Explicit {
                        ordinal,
                        conversion,
                        ..
                    } if matches!(
                        self.request
                            .semantic_values()
                            .type_data(conversion.target_type())
                            .as_ref(),
                        bray_symbols::TypeData::Borrow { .. }
                    ) =>
                    {
                        Some(bray_symbols::DependencySubjectRoot::Parameter(
                            bray_symbols::SymbolOrdinal::new(*ordinal),
                        ))
                    }
                    _ => None,
                })
                .chain(
                    call.receiver()
                        .filter(|receiver| {
                            matches!(
                                receiver.mode(),
                                bray_symbols::ReceiverMode::Shared
                                    | bray_symbols::ReceiverMode::Mutable
                            )
                        })
                        .map(|_| bray_symbols::DependencySubjectRoot::Receiver),
                );

            for root in arguments {
                let Some(base) = self.invocation_place(occurrence, root) else {
                    continue;
                };

                let Some(targets) = self
                    .invocation_reference(state, occurrence, root)
                    .and_then(|source| source.targets(&state.memory))
                else {
                    continue;
                };

                if let Some(targets) = targets
                    .iter()
                    .map(Target::observation_path)
                    .collect::<Option<std::collections::BTreeSet<_>>>()
                {
                    borrow_origins.insert(base, targets);
                }
            }
        }

        borrow_origins
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_place(
        &self,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Option<Place> {
        if let Some(argument) =
            invocation_argument(self.execution.semantics.selections(), occurrence, root)
        {
            return self.argument_storage(argument);
        }

        if matches!(occurrence, BoundExecutionSite::ScopedExit(_))
            && root == DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0))
        {
            // A transition owns its observed path while sharing the immutable projection list.
            return self
                .storage
                .occurrence_plans(occurrence)
                .next()
                .and_then(|plan| self.storage.logical_path(plan.access()))
                .cloned();
        }

        None
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_reference(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Option<Source> {
        if let Some(argument) =
            invocation_argument(self.execution.semantics.selections(), occurrence, root)
        {
            let owner = if matches!(occurrence, BoundExecutionSite::ScopedEnter(_)) {
                occurrence.expression()?
            } else {
                argument
            };

            if root == DependencySubjectRoot::Receiver
                && let Some((capability, _)) =
                    self.storage.borrow_capability_entries().find(|(id, loan)| {
                        loan.expression() == Some(owner) && state.memory.has_borrow(*id)
                    })
            {
                return Some(Source::Borrow(capability));
            }

            return self.expression_reference(state, argument);
        }

        self.invocation_place(occurrence, root)
            .map(|place| Source::Value(Target::storage(place)))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_locations(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        subject: &bray_symbols::DependencySubject,
    ) -> Option<Vec<Target>> {
        let path = self
            .dependencies
            .calls()
            .get(&occurrence)?
            .reference_path(subject)?;

        if let Some(argument) = invocation_argument(
            self.execution.semantics.selections(),
            occurrence,
            subject.subject_root(),
        ) {
            return self.call_reference_locations(state, argument, path);
        }

        let start = Target::storage(self.invocation_place(occurrence, subject.subject_root())?);

        Some(vec![state.memory.reference_location(start, path, None)?])
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_result(
        &self,
        occurrence: BoundExecutionSite,
    ) -> Option<Place> {
        match occurrence {
            BoundExecutionSite::ScopedEnter(expression) => self.storage.identity_entries()
                .find_map(|(id, identity)| matches!(identity, bray_bound_tree::StorageIdentity::ScopedCapability { expression: owner, .. } if owner == expression).then_some(Place::root(id))),
            BoundExecutionSite::ScopedExit(_) => None,
            BoundExecutionSite::Node(_) => self.expression_result_storage(occurrence.expression()?),
        }
    }
}

pub(super) fn invocation_argument(
    selections: &bray_bound_tree::CheckedSemanticSelections,
    occurrence: BoundExecutionSite,
    root: DependencySubjectRoot,
) -> Option<BoundExpressionId> {
    match selections.expression(occurrence.expression()?)? {
        SemanticSelection::Call(call) => crate::dependency::result_argument(call, root),
        SemanticSelection::ScopedUse(scoped)
            if matches!(occurrence, BoundExecutionSite::ScopedEnter(_))
                && root == DependencySubjectRoot::Receiver =>
        {
            Some(scoped.initializer())
        }
        _ => None,
    }
}
