use super::model::ExecutionFlowDomain;
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::AnyBoundNodeId;

use crate::CheckerRequestContext;
use crate::execution_guarantees::ExecutionCondition;

use super::super::state::ExecutionState;
use bray_bound_tree::StorageInvalidation;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    fn spare_storage_header(
        &self,
        state: &ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> Option<crate::ExecutionPlace> {
        use bray_bound_tree::{SelectedArgument, SemanticSelection};
        use bray_compiler_known::ImplementationHook;

        let SemanticSelection::Call(call) = self
            .semantics
            .selections()
            .expression(occurrence.expression()?)?
        else {
            return None;
        };

        let destination = match call.implementation_hook()? {
            ImplementationHook::MemoryCopy | ImplementationHook::MemoryCopyOverlapping => 1,
            ImplementationHook::RawPointerWrite
            | ImplementationHook::ByteBufferFill
            | ImplementationHook::VolatileStore => 0,
            _ => return None,
        };

        let argument = |call: &bray_bound_tree::SelectedCall, index| {
            call.arguments().iter().find_map(|argument| match argument {
                SelectedArgument::Explicit {
                    expression,
                    ordinal,
                    ..
                } if *ordinal == index => Some(*expression),
                _ => None,
            })
        };

        let destination_value = self.value(state, argument(call, destination)?);

        let ExecutionCondition::Expression(producer) = destination_value else {
            return None;
        };

        let SemanticSelection::Call(producer_call) =
            self.semantics.selections().expression(producer)?
        else {
            return None;
        };

        if producer_call.implementation_hook() != Some(ImplementationHook::RawBufferSparePointer) {
            return None;
        }

        let contract = self.request.trusted_contracts()?.calls.get(&occurrence)?;
        let entry = state.entries.get(&occurrence)?;

        if !entry.proves(&contract.preconditions)
            || !self.trusted_requirements_proven(entry, &contract.requirements)
        {
            return None;
        }

        let buffer = argument(producer_call, 0)?;
        let value = self.value(state, buffer);
        let producer_entry = state.entries.get(&producer.into())?;

        if producer_entry
            .arguments
            .get(&crate::ExecutionPlace::argument(
                bray_symbols::SymbolOrdinal::new(0),
            ))
            != Some(&value)
        {
            return None;
        }

        // This closed accessor names only the live owner's spare allocation. A
        // checked store there can change payload contents, but cannot reach its header.
        match value {
            ExecutionCondition::Input(place) => Some(place),
            ExecutionCondition::Borrowed(value) => match value.as_ref() {
                ExecutionCondition::Input(place) => Some(place.clone()),
                _ => None,
            },
            _ => crate::execution_guarantees::expression_place(
                self.request.unit(),
                self.semantics,
                self.request.semantic_values(),
                buffer,
            ),
        }
    }

    pub(super) fn invalidate(
        &self,
        state: &mut ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        invalidation: Option<&StorageInvalidation>,
    ) {
        // Resolving a destination does not turn an ownership transfer into a write.
        let changes_contents = occurrence.expression().is_some_and(|expression| {
            matches!(
                self.semantics.selections().expression(expression),
                Some(bray_bound_tree::SemanticSelection::Call(_))
            )
        });

        let Some(invalidation) = invalidation.or_else(|| self.refinements.get(&occurrence)) else {
            return;
        };

        // A proven zero extent has no writes, epoch changes, or ownership effects.
        if self.empty_memory_copy(state, occurrence) {
            return;
        }

        if matches!(invalidation, StorageInvalidation::All) {
            if let Some(contract) = self
                .request
                .trusted_contracts()
                .and_then(|contracts| contracts.calls.get(&occurrence))
                && contract.preserves_inputs
                && state.entries.get(&occurrence).is_some_and(|entry| {
                    entry.proves(&contract.preconditions)
                        && (entry.trusted_boundary
                            || self.trusted_requirements_proven(entry, &contract.requirements))
                })
            {
                return;
            }

            let preserves_storage = match occurrence {
                bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(
                    expression,
                )) => {
                    matches!(self.semantics.selections().expression(expression),
                    Some(bray_bound_tree::SemanticSelection::Call(call)) if matches!(call.implementation_hook(), Some(
                        bray_compiler_known::ImplementationHook::RawPointerWrite
                        | bray_compiler_known::ImplementationHook::RawPointerRead
                        | bray_compiler_known::ImplementationHook::VolatileStore
                        | bray_compiler_known::ImplementationHook::DeviceVolatileStore)))
                }
                _ => false,
            };

            let spatial = if preserves_storage {
                state.trusted_assumptions.iter().filter(|(condition, _)| {
                    let condition = match condition {
                        ExecutionCondition::Trusted(condition) => condition.as_ref(),
                        condition => condition,
                    };

                    matches!(condition, ExecutionCondition::Predicate(predicate, _, _) if self.spatial_predicates.contains(predicate))
                })
                    .cloned().collect()
            } else {
                BTreeSet::new()
            };

            let dependencies = if preserves_storage {
                // Reached addresses keep their owner lifetimes even when their contents change.
                // Retain the complete dependency chain, including intermediate address views.
                std::mem::take(&mut state.witness_dependencies)
            } else {
                BTreeMap::new()
            };

            let preserved_header = self.spare_storage_header(state, occurrence);

            state.invalidate_cleanup(preserved_header.as_ref());
            state.trusted_assumptions = spatial;
            state.witness_dependencies = dependencies;

            return;
        }

        let StorageInvalidation::Paths(paths) = invalidation else {
            unreachable!("global invalidation is handled before per-access invalidation");
        };

        let transfers = !changes_contents
            && !self.storage.occurrence_plans(occurrence).any(|plan| {
                matches!(
                    plan.purpose(),
                    bray_bound_tree::StorageAccessPurpose::Assignment
                        | bray_bound_tree::StorageAccessPurpose::Initialize
                )
            });

        let mut changed_places = BTreeSet::new();

        for (target, binding) in self.storage.bindings() {
            let access = match binding {
                bray_bound_tree::StorageBinding::Access(access) => *access,
                bray_bound_tree::StorageBinding::Identity(identity) => {
                    let Some(access) = self.storage.root_access(*identity) else {
                        continue;
                    };

                    access
                }
            };

            if !invalidation.invalidates(access, self.storage) {
                continue;
            }

            let Some(reference) = crate::execution_guarantees::storage_binding_reference(*target)
            else {
                continue;
            };

            for invalidated in paths {
                if self.storage.relationship_to_path(access, invalidated)
                    == bray_bound_tree::StorageRelationship::Disjoint
                {
                    continue;
                }

                // Checked constant selectors identify components independently of their occurrences.
                let path = invalidated
                    .projections
                    .iter()
                    .map(|projection| {
                        if let bray_bound_tree::StorageProjection::Element(selector) = projection
                            && let Some(ExecutionCondition::Literal(value)) =
                                self.literals.get(selector)
                            && let bray_symbols::ConstantValueKind::Integer(index) = value.kind()
                            && let Some(index) =
                                index.to_u64().and_then(|index| u32::try_from(index).ok())
                        {
                            return bray_bound_tree::StorageProjection::ElementFromStart(
                                bray_symbols::SymbolOrdinal::new(index),
                            );
                        }

                        *projection
                    })
                    .collect::<Vec<_>>();

                let changed = crate::ExecutionPlace::from(reference)
                    .project(&path)
                    .unwrap_or_else(|| reference.into());

                changed_places.insert(changed);
            }
        }

        let invalidated_values = if transfers {
            Vec::new()
        } else {
            changed_places
                .iter()
                .flat_map(|place| {
                    [
                        place
                            .value_in(&state.current)
                            .unwrap_or_else(|| ExecutionCondition::Input(place.clone())),
                        ExecutionCondition::Input(place.clone()),
                    ]
                })
                .collect()
        };

        // Resolve every affected place before changing observations, and index witness
        // dependencies once for the complete mutation rather than once per destination.
        let content_only = occurrence
            .expression()
            .and_then(|expression| self.memory.operation(expression))
            .is_some_and(|operation| {
                crate::memory::execution::storage_effect(operation.kind())
                    == crate::memory::execution::StorageEffect::Contents
            });

        if content_only {
            state.invalidate_contents(
                &changed_places,
                invalidated_values,
                &self.spatial_predicates,
            );
        } else {
            state.invalidate_trusted_places(&changed_places, invalidated_values);
        }

        // Preserve observations of disjoint components before retiring the containing snapshot.
        let retained = self
            .observed_types
            .keys()
            .filter(|place| !place.overlaps_any(&changed_places))
            .filter_map(|place| {
                place
                    .value_in(&state.current)
                    .map(|value| (place.clone(), value))
            })
            .collect::<Vec<_>>();

        state.current.extend(retained);

        for place in changed_places {
            state.assign(place, ExecutionCondition::Unknown);
        }
    }
}
