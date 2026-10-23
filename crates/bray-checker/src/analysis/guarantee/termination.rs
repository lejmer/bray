use std::cmp::Ordering;
use std::collections::BTreeMap;

use bray_bound_tree::BoundOperator;

use super::super::id::AnalysisBlockId;
use super::super::model::AnalysisEdge;
use super::flow::ExecutionFlow;
use super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

/// Proves progress from the established machine-value transitions at every reachable back edge.
/// Exact symbolic machine arithmetic exists only where the owning numerical analysis proved no wrap.
pub(super) fn bounded_loop_ranks<C: CheckerRequestContext + ?Sized>(
    flow: &ExecutionFlow<'_, '_, C>,
    cycles: &[(&AnalysisEdge, ExecutionState)],
) -> BTreeMap<AnalysisBlockId, (ExecutionPlace, Ordering)> {
    let mut incoming = BTreeMap::<_, Vec<&ExecutionState>>::new();

    for (edge, state) in cycles {
        incoming.entry(edge.target()).or_default().push(state);
    }

    incoming
        .into_iter()
        .filter_map(|(header, ends)| {
            let initial = flow.states.state(header)?.as_ref()?;

            let ends = ends
                .into_iter()
                .map(|end| (end, ExecutionCondition::equalities(&end.assumptions, None)))
                .collect::<Vec<_>>();

            initial.current.keys().find_map(|place| {
                let start = place.value_in(&initial.current)?;

                if !flow
                    .domain
                    .unsigned_value(&ExecutionCondition::Input(place.clone()))
                {
                    return None;
                }

                let mut direction = None;

                for (end, equalities) in &ends {
                    let value = place.value_in(&end.current)?;
                    let step = strict_step(&value, &start, equalities);
                    let step = step?;

                    if direction.is_some_and(|previous| previous != step) {
                        return None;
                    }

                    direction = Some(step);
                }

                Some((header, (place.clone(), direction?)))
            })
        })
        .collect()
}

/// Returns the direction of a nonzero constant displacement in an established exact value.
fn strict_step(
    value: &ExecutionCondition,
    base: &ExecutionCondition,
    equalities: &BTreeMap<ExecutionCondition, ExecutionCondition>,
) -> Option<Ordering> {
    displacement(
        &value.with_equalities(equalities),
        &base.with_equalities(equalities),
        &mut { ExecutionCondition::WORK_LIMIT },
    )
    .map(|delta| delta.cmp(&num_bigint::BigInt::from(0)))
    .filter(|direction| *direction != Ordering::Equal)
}

fn displacement(
    value: &ExecutionCondition,
    base: &ExecutionCondition,
    remaining: &mut usize,
) -> Option<num_bigint::BigInt> {
    if *remaining == 0 {
        return None;
    }

    *remaining -= 1;

    if value == base {
        return Some(num_bigint::BigInt::from(0));
    }

    let ExecutionCondition::Operation(
        operator @ (BoundOperator::Add | BoundOperator::Subtract),
        operands,
    ) = value
    else {
        return None;
    };

    let [subject, ExecutionCondition::Literal(amount)] = operands.as_ref() else {
        return None;
    };

    let bray_symbols::ConstantValueKind::Integer(amount) = amount.kind() else {
        return None;
    };

    let amount = crate::constant::to_big_integer(amount);
    let delta = displacement(subject, base, remaining)?;

    Some(if *operator == BoundOperator::Add {
        delta + amount
    } else {
        delta - amount
    })
}

/// A successful step either advances one bounded input value or a retained finite cursor.
/// Unknown result tags remain possible successes, so absence never hides a missing proof.
pub(super) fn finite_step<C: CheckerRequestContext + ?Sized>(
    flow: &ExecutionFlow<'_, '_, C>,
    candidate: &mut crate::ExecutionCandidate,
    ranks: &BTreeMap<AnalysisBlockId, (ExecutionPlace, Ordering)>,
) {
    let exits = flow
        .domain
        .graph
        .completion_exits()
        .filter_map(|exit| flow.output(exit.block()))
        .filter(|state| !absent(&state.result, state))
        .collect::<Vec<_>>();

    if exits.is_empty() {
        candidate.finite_iteration = true;
        return;
    }

    let direct = exits[0]
        .current
        .keys()
        .filter(|place| {
            flow.domain
                .borrowed_inputs
                .iter()
                .any(|input| input.contains(place))
        })
        .any(|place| {
            let base = ExecutionCondition::Input(place.clone());

            if !flow.domain.unsigned_value(&base) {
                return false;
            }

            let mut direction = None;

            exits.iter().all(|state| {
                let Some(value) = place.value_in(&state.current) else {
                    return false;
                };

                let equalities = ExecutionCondition::equalities(&state.assumptions, None);

                let Some(step) = strict_step(&value, &base, &equalities) else {
                    return false;
                };

                if direction.is_some_and(|previous| previous != step) {
                    return false;
                }

                direction = Some(step);

                true
            })
        });

    if direct {
        candidate.finite_iteration = true;
        return;
    }

    let Some(body) = flow.domain.checked_body else {
        return;
    };

    for (occurrence, evidence) in &candidate.calls {
        let Some(expression) = occurrence.expression() else {
            continue;
        };

        let Some(bray_bound_tree::SemanticSelection::Call(call)) =
            flow.domain.semantics.selections().expression(expression)
        else {
            continue;
        };

        let Some(ty) = flow
            .domain
            .semantics
            .types()
            .expression(expression)
            .map(|entry| entry.ty())
        else {
            continue;
        };

        if !matches!(
            flow.domain.request.semantic_values().type_data(ty).as_ref(),
            bray_symbols::TypeData::Nullable(_)
        ) {
            continue;
        }

        let selection = flow
            .domain
            .semantics
            .selections()
            .expression(expression)
            .expect("selected call retains its input selection");

        let mutable = selection
            .inputs()
            .filter(|input| {
                matches!(
                    flow.domain
                        .request
                        .semantic_values()
                        .type_data(input.ty(flow.domain.request.semantic_values()))
                        .as_ref(),
                    bray_symbols::TypeData::Borrow {
                        kind: bray_symbols::BorrowKind::Mutable,
                        ..
                    }
                )
            })
            .collect::<Vec<_>>();

        let [input] = mutable.as_slice() else {
            continue;
        };

        let input = selection
            .input_ordinal(input.root())
            .expect("mutable formal retains its ordinal");

        {
            let Some(target) = body
                .storage_flow()
                .invocation_input_target(*occurrence, input)
            else {
                continue;
            };

            let Some(path) = target.inline_state_path() else {
                continue;
            };

            let Some(place) = ExecutionPlace::storage_path(flow.domain.storage, &path) else {
                continue;
            };

            if !flow
                .domain
                .borrowed_inputs
                .iter()
                .any(|input| input.contains(&place))
            {
                continue;
            }

            // The next receiver must still be the original input, not a replacement cursor.
            let base = ExecutionCondition::Input(place.clone());

            if ExecutionPlace::argument(input)
                .value_in(&evidence.arguments)
                .as_ref()
                != Some(&base)
                && !finite_prefix(flow, ranks, &exits, &path, *occurrence)
            {
                continue;
            }

            let valid = exits.iter().all(|state| {
                let result = flow.domain.value(state, expression);

                let (_, missing) = crate::execution_guarantees::pattern_value(
                    flow.domain.request.semantic_values(),
                    ty,
                    bray_bound_tree::PatternPredicate::NullableAbsent,
                )
                .expect("selected nullable result has its absence value");

                let present = ExecutionCondition::pattern(result, BoundOperator::NotEqual, missing)
                    .prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                    == Some(true);

                let after = ExecutionCondition::PostState(
                    bray_bound_tree::StorageMutation::Operation(*occurrence),
                    place.clone(),
                );

                present && place.value_in(&state.current).as_ref() == Some(&after)
            });

            if valid {
                candidate.dependencies.push(crate::ExecutionDependency {
                    target: crate::ExecutionDependencyTarget::selected(call),
                    obligation: crate::ExecutionObligation::FiniteIteration,
                    occurrence: *occurrence,
                });

                candidate.finite_iteration = true;
                return;
            }
        }
    }
}

fn absent(value: &ExecutionCondition, state: &ExecutionState) -> bool {
    if matches!(value, ExecutionCondition::Literal(value)
        if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent))
    {
        return true;
    }

    state.assumptions.iter().any(|(condition, holds)| {
        let ExecutionCondition::Operation(operator, operands) = condition else {
            return false;
        };

        let [left, ExecutionCondition::Literal(missing)] = operands.as_ref() else {
            return false;
        };

        left == value
            && matches!(
                missing.kind(),
                bray_symbols::ConstantValueKind::NullableAbsent
            )
            && ((*operator == BoundOperator::Equal && *holds)
                || (*operator == BoundOperator::NotEqual && !holds))
    })
}

/// A protocol loop advances its owned cursor on every back edge and cannot replace it.
pub(super) fn finite_protocol_loops<C: CheckerRequestContext + ?Sized>(
    flow: &ExecutionFlow<'_, '_, C>,
    cycles: &[(&AnalysisEdge, ExecutionState)],
    candidate: &mut crate::ExecutionCandidate,
) -> std::collections::BTreeSet<AnalysisBlockId> {
    use super::super::model::{AnalysisCallPhase, AnalysisOperationKind};
    use bray_bound_tree::{BoundExecutionSite, SemanticSelection};

    let mut proved = std::collections::BTreeSet::new();

    let headers = cycles
        .iter()
        .map(|(edge, _)| edge.target())
        .collect::<std::collections::BTreeSet<_>>();

    for header in headers {
        let members = loop_members(flow, header);

        for occurrence in candidate.calls.keys().copied() {
            let BoundExecutionSite::Next(expression) = occurrence else {
                continue;
            };

            let Some(SemanticSelection::Iteration(selected)) =
                flow.domain.semantics.selections().expression(expression)
            else {
                panic!("checked next retains its iteration selection");
            };

            let access = flow
                .domain
                .storage
                .invocation_result(BoundExecutionSite::Iterate(expression))
                .expect("checked next retains its resident cursor");

            let Some(body) = flow.domain.checked_body else {
                continue;
            };

            let Some(expected) = flow.domain.storage.logical_path(access) else {
                continue;
            };

            let actual = body
                .storage_flow()
                .invocation_input_target(occurrence, bray_symbols::SymbolOrdinal::new(0))
                .and_then(|target| target.inline_state_path());

            if actual.as_ref() != Some(expected) {
                continue;
            };

            // The receiver is the resident cursor, and no other operation may reset it.
            // Next's finite-step certificate owns its precise field-level transition.
            let reset =
                members
                    .iter()
                    .flat_map(|block| {
                        flow.domain
                            .graph
                            .block(*block)
                            .expect("loop member retains its block")
                            .operations()
                    })
                    .filter_map(|id| flow.domain.graph.operation(*id))
                    .map(|operation| operation.kind().occurrence())
                    .any(|site| {
                        site != occurrence
                            && flow.domain.refinements.get(&site).is_some_and(|effect| {
                                effect.invalidates(access, flow.domain.storage)
                            })
                    });

            if reset {
                continue;
            }

            let completed = flow.domain.graph.blocks().iter().find(|block| {
                block
                    .operations()
                    .iter()
                    .filter_map(|id| flow.domain.graph.operation(*id))
                    .any(|operation| {
                        matches!(operation.kind(),
                    AnalysisOperationKind::Call { invocation, phase: AnalysisCallPhase::Completion }
                        if invocation == occurrence)
                    })
            });

            let Some(completed) = completed else {
                continue;
            };

            let matching = cycles
                .iter()
                .filter(|(edge, _)| edge.target() == header)
                .collect::<Vec<_>>();

            if matching.iter().any(|(edge, state)| {
                (header != completed.id()
                    && path_exists(flow, header, edge.source(), |candidate| {
                        candidate.source() != completed.id() && candidate.target() != completed.id()
                    }))
                    || !next_present(flow, state, occurrence)
            }) {
                continue;
            }

            candidate.dependencies.push(crate::ExecutionDependency {
                target: bray_bound_tree::BoundCallableTarget::Declaration(selected.next()).into(),
                obligation: crate::ExecutionObligation::FiniteIteration,
                occurrence,
            });

            proved.insert(header);
            break;
        }
    }

    proved
}

fn path_exists<C: CheckerRequestContext + ?Sized>(
    flow: &ExecutionFlow<'_, '_, C>,
    start: AnalysisBlockId,
    target: AnalysisBlockId,
    permits: impl Fn(&AnalysisEdge) -> bool,
) -> bool {
    let mut pending = vec![start];
    let mut seen = std::collections::BTreeSet::new();

    while let Some(block) = pending.pop() {
        if !seen.insert(block) {
            continue;
        }

        if block == target {
            return true;
        }

        let block = flow
            .domain
            .graph
            .block(block)
            .expect("checked path retains its block");

        pending.extend(block.successors().iter().filter_map(|edge| {
            let edge = flow
                .domain
                .graph
                .edge(*edge)
                .expect("checked path retains its edge");

            (permits(edge)
                && flow.is_block_reachable(edge.source())
                && flow.is_block_reachable(edge.target()))
            .then_some(edge.target())
        }));
    }

    false
}

fn next_present<C: CheckerRequestContext + ?Sized>(
    flow: &ExecutionFlow<'_, '_, C>,
    state: &ExecutionState,
    occurrence: bray_bound_tree::BoundExecutionSite,
) -> bool {
    let access = flow
        .domain
        .storage
        .invocation_result(occurrence)
        .expect("next has its actual nullable result");

    let ty = flow
        .domain
        .storage
        .access(access)
        .expect("next storage has its type")
        .reached_type();

    let (_, missing) = crate::execution_guarantees::pattern_value(
        flow.domain.request.semantic_values(),
        ty,
        bray_bound_tree::PatternPredicate::NullableAbsent,
    )
    .expect("next result has its absence value");

    ExecutionCondition::pattern(
        flow.domain.invocation_value(state, occurrence),
        BoundOperator::NotEqual,
        missing,
    )
    .prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
        == Some(true)
}

fn loop_members<C: CheckerRequestContext + ?Sized>(
    flow: &ExecutionFlow<'_, '_, C>,
    header: AnalysisBlockId,
) -> std::collections::BTreeSet<AnalysisBlockId> {
    let component = flow.domain.graph.component_order(header);

    flow.domain
        .graph
        .blocks()
        .iter()
        .filter(|block| {
            flow.is_block_reachable(block.id())
                && flow.domain.graph.component_order(block.id()) == component
        })
        .map(|block| block.id())
        .collect()
}

/// A bounded prefix can change a cursor once before delegating all subsequent successful steps.
/// Its persistent unsigned phase is exhausted on every success. At phase zero, the existing
/// checked guard excludes every prefix write, including any write that could rearm the phase.
fn finite_prefix<C: CheckerRequestContext + ?Sized>(
    flow: &ExecutionFlow<'_, '_, C>,
    ranks: &BTreeMap<AnalysisBlockId, (ExecutionPlace, Ordering)>,
    exits: &[ExecutionState],
    cursor: &bray_bound_tree::StoragePath,
    suffix: bray_bound_tree::BoundExecutionSite,
) -> bool {
    use super::super::model::{AnalysisOperationKind, AnalysisRefinement};

    let Some(access) = flow
        .domain
        .storage
        .access_at(cursor.root, &cursor.projections)
    else {
        return false;
    };

    ranks
        .iter()
        .filter(|(_, (phase, direction))| {
            *direction == Ordering::Less
                && flow
                    .domain
                    .borrowed_inputs
                    .iter()
                    .any(|input| input.contains(phase))
        })
        .any(|(header, (phase, _))| {
            let ty = flow
                .domain
                .observation_type(&ExecutionCondition::Input(phase.clone()))
                .expect("bounded input rank retains its scalar type");

            let values = flow.domain.request.semantic_values();

            let zero = values
                .intern_constant_value(bray_symbols::ConstantValueData::new(
                    ty,
                    bray_symbols::ConstantValueKind::Integer(
                        bray_symbols::IntegerConstant::from_u64(0),
                    ),
                ))
                .expect("zero belongs to the bounded unsigned rank");

            let zero = ExecutionCondition::Literal(values.constant_value_data(zero));

            if exits.iter().any(|state| {
                phase.value_in(&state.current).is_none_or(|current| {
                    ExecutionCondition::operation(BoundOperator::Equal, vec![current, zero.clone()])
                        .prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                        != Some(true)
                })
            }) {
                return false;
            }

            let phase_access = flow
                .domain
                .storage
                .access_entries()
                .find_map(|(access, _)| {
                    (ExecutionPlace::storage(flow.domain.storage, access).as_ref() == Some(phase))
                        .then_some(access)
                });

            let Some(phase_access) = phase_access else {
                return false;
            };

            let members = loop_members(flow, *header);

            flow.domain
                .graph
                .edges()
                .iter()
                .filter(|edge| members.contains(&edge.source()))
                .any(|guard| {
                    let Some(AnalysisRefinement::Condition { expression, value }) =
                        guard.refinement()
                    else {
                        return false;
                    };

                    let Some(Some(entry)) = flow.states.state(guard.source()) else {
                        return false;
                    };

                    let Some(current) = phase.value_in(&entry.current) else {
                        return false;
                    };

                    let (_, condition, _) = flow.domain.condition_evidence(entry, expression);

                    if condition
                        .with_equalities(&BTreeMap::from([(current, zero.clone())]))
                        .prove(&Default::default(), &mut { ExecutionCondition::WORK_LIMIT })
                        != Some(!value)
                    {
                        return false;
                    }

                    flow.domain
                        .graph
                        .blocks()
                        .iter()
                        .filter(|block| flow.is_block_reachable(block.id()))
                        .all(|block| {
                            let changes = block
                                .operations()
                                .iter()
                                .filter_map(|id| flow.domain.graph.operation(*id))
                                .any(|operation| {
                                    let site = operation.kind().occurrence();

                                    if site == suffix {
                                        return false;
                                    }

                                    let cursor_write =
                                        flow.domain.refinements.get(&site).is_some_and(|effect| {
                                            effect.invalidates(access, flow.domain.storage)
                                        });

                                    let phase_effect =
                                        flow.domain.refinements.get(&site).is_some_and(|effect| {
                                            effect.invalidates(phase_access, flow.domain.storage)
                                        });

                                    let phase_write = if let AnalysisOperationKind::Bound(
                                        bray_bound_tree::AnyBoundNodeId::Expression(expression),
                                    ) = operation.kind()
                                    {
                                        flow.domain
                                            .checked_body
                                            .and_then(|body| {
                                                body.storage_flow().assignment_target(expression)
                                            })
                                            .and_then(|target| target.inline_state_path())
                                            .and_then(|path| {
                                                ExecutionPlace::storage_path(
                                                    flow.domain.storage,
                                                    &path,
                                                )
                                            })
                                            .is_some_and(|target| {
                                                target.contains(phase) || phase.contains(&target)
                                            })
                                    } else {
                                        false
                                    };

                                    cursor_write || phase_write || phase_effect
                                });

                            !changes
                                || members.contains(&block.id())
                                    && !path_exists(flow, *header, block.id(), |edge| {
                                        edge.id() != guard.id()
                                    })
                        })
                })
        })
}
