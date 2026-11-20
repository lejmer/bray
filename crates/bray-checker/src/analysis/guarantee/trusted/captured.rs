use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use bray_symbols::{CapturedValueOutcome, CapturedValueOperation as Op, CapturedValueTemplate as Value, DependencySubject, DependencySubjectRoot};

use super::super::{flow::ExecutionFlowDomain, state::ExecutionState};
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionConditionKind as K};

pub(super) fn captured_outcome<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, state: &ExecutionState, result: &ExecutionCondition,
    writes: &std::collections::BTreeSet<bray_bound_tree::StoragePath>, guards: &[bray_symbols::ConstantTermId],
) -> CapturedValueOutcome {
    CapturedValueOutcome::new(conjunction(domain.request.semantic_values(), guards.iter().copied()),
        captured_bindings(domain, state, Some(result), writes))
}

pub(super) fn captured_bindings<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, state: &ExecutionState, result: Option<&ExecutionCondition>,
    writes: &std::collections::BTreeSet<bray_bound_tree::StoragePath>,
) -> BTreeMap<DependencySubject, Value> {
    let mut memo = HashMap::new();
    let mut values = BTreeMap::new();

    if let Some(result) = result {
        values.insert(DependencySubject::root(DependencySubjectRoot::Result),
            captured(domain, state, result, &mut memo));
    }

    for path in writes {
        let Some(place) = crate::ExecutionPlace::storage_identity(domain.storage, path.root) else { continue; };

        let Some(reference) = place.reference() else { continue; };

        let Some(root) = crate::dependency::parameter_root(domain.request, reference) else { continue; };

        let Some(projections) = path.projections.iter().map(|projection| value_projection(domain, *projection)).collect::<Option<Vec<_>>>() else { continue; };

        let Some(value) = place.value_in(&state.current) else { continue; };

        let value = if value.is_reference() {
            ExecutionCondition::storage_value(Op::Read, vec![value].into()).copied_value().clone()
        } else { value };

        let value = path.projections.iter().copied().fold(value, ExecutionCondition::project);

        values.insert(DependencySubject::new(root, projections), captured(domain, state, &value, &mut memo));
    }

    values
}

pub(super) fn exit_guards<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, state: &ExecutionState, result: &ExecutionCondition,
    entry_conditions: &std::collections::BTreeSet<(ExecutionCondition, bool)>,
) -> Vec<bray_symbols::ConstantTermId> {
    let return_substitution = super::super::flow::return_substitution(result);
    let mut guards = Vec::new();

        for (condition, holds) in state.assumptions.difference(entry_conditions) {
            let formal = matches!(condition.kind(), crate::ExecutionConditionKind::Input(place)
                if place.is_callable_input() && place.reference().is_some_and(|reference|
                    domain.request.trusted_contracts().is_some_and(|inputs| inputs.inputs.contains(&reference))));

            let condition = if formal { condition.clone() } else {
                if condition.contains_trusted() { continue; }

                let mapped = condition.boolean_consequence(*holds, &mut |atom| {
                    let mapped = return_substitution(atom);

                    super::carriers::portable(domain.request, &mapped)?;

                    // Mapping each atom keeps one clause's unrelated local terms
                    // from consuming the current-snapshot rewrite budget.
                    let actual = mapped.substitute(&|place| ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())), result,
                        &mut { ExecutionCondition::WORK_LIMIT });

                    (actual == *atom).then_some(mapped)
                }, &mut { ExecutionCondition::WORK_LIMIT });

                if matches!(mapped.kind(), K::Boolean(_) | K::Unknown) { continue; }

                mapped
            };

            let condition = if !formal || *holds {
                condition
            } else {
                ExecutionCondition::operation(
                    bray_bound_tree::BoundOperator::LogicalNot,
                    vec![condition],
                )
            };

            let condition = match super::carriers::portable(domain.request, &condition) {
                Some(condition) => condition,
                None if formal => unreachable!("formal Boolean exit condition is portable"),
                None => continue,
            };

            guards.push(condition);
        }

    if let Some(variant) = result.known_variant()
        && let Some(guard) = super::carriers::portable(domain.request, &ExecutionCondition::result().matches_variant(variant))
    { guards.push(guard); }

    guards.sort_unstable();
    guards.dedup();

    guards
}

fn captured<C: CheckerRequestContext + ?Sized>(domain: &ExecutionFlowDomain<'_, '_, C>, state: &ExecutionState, value: &ExecutionCondition, memo: &mut HashMap<(ExecutionCondition, bool), Value>) -> Value {
    captured_data(domain, state, value, false, memo)
}

fn captured_data<C: CheckerRequestContext + ?Sized>(domain: &ExecutionFlowDomain<'_, '_, C>, state: &ExecutionState, value: &ExecutionCondition,
    descriptor: bool, memo: &mut HashMap<(ExecutionCondition, bool), Value>,
) -> Value {
    let key = (value.clone(), descriptor);

    if let Some(value) = memo.get(&key) { return value.clone(); }

    let encoded = match value.kind() {
        K::CapturedValue { value, .. } => captured_data(domain, state, value, descriptor, memo),
        K::DeferredValue { value, frame } if frame.producers.is_empty() && frame.unknown.is_some()
            && matches!(value.computed_value().kind(), K::Input(place) if place.is_callable_input()) =>
            captured_data(domain, state, value, descriptor, memo),
        K::DeferredValue { frame, .. } => captured_deferred(domain, state, frame, memo),
        K::Input(place) if matches!(place.root, crate::execution_guarantees::ExecutionInput::Static(_)) => {
            let crate::execution_guarantees::ExecutionInput::Static(selection) = &place.root else { unreachable!(); };

            let mut value = Value::Static(selection.as_ref().clone());

            for projection in crate::storage::owned_storage_projections(domain.dependencies, &place.projections) {
                let Some(projection) = value_projection(domain, projection) else { return Value::Unknown; };

                value = Value::Operation(Op::Select(projection), [value].into());
            }

            value
        }
        K::Input(place) if place.is_callable_input() => {
            captured_input(domain, state, value, place, descriptor)
        }
        K::Observation { owner, contents, kind } => Value::Operation(match kind {
            crate::execution_guarantees::ExecutionObservationKind::Owned => Op::Owned,
            crate::execution_guarantees::ExecutionObservationKind::Reference => Op::Reference,
            crate::execution_guarantees::ExecutionObservationKind::Scalar => Op::Scalar,
        }, [captured_data(domain, state, owner,
                *kind != crate::execution_guarantees::ExecutionObservationKind::Owned, memo),
            captured(domain, state, contents, memo)].into()),
        K::Borrowed(contents) => {
            let descriptor = match contents.kind() {
                K::Input(place) if domain.borrowed_inputs.contains(place) && place.projections.is_empty() =>
                    captured_data(domain, state, contents, true, memo),
                _ => Value::Unknown,
            };

            if descriptor != Value::Unknown { descriptor } else {
                Value::Operation(Op::Reference, [descriptor, captured(domain, state, contents, memo)].into())
            }
        }
        K::Tagged(variant, contents) => Value::Tagged(*variant, Arc::new(captured(domain, state, contents, memo))),
        K::StorageValue(operation, operands) => {
            let operation = match operation {
                Op::Select(field) => {
                    let Some(field) = value_projection(domain, *field) else { return Value::Unknown; };

                    Op::Select(field)
                }
                Op::OwningStorage(field) => Op::OwningStorage(value_projection(domain, *field)
                    .expect("checked owning storage retains its descriptor selector")),
                Op::Read => Op::Read,
                Op::Reference => Op::Reference,
                Op::Owned => Op::Owned,
                Op::Scalar => Op::Scalar,
                Op::Inline => Op::Inline,
                Op::Present => Op::Present,
                Op::Add => Op::Add,
                Op::Subtract => Op::Subtract,
                Op::EmptyPrefix => Op::EmptyPrefix,
                Op::Append => Op::Append,
                Op::Last => Op::Last,
                Op::Remainder => Op::Remainder,
            };

            Value::Operation(operation, operands.iter().map(|value| captured(domain, state, value, memo)).collect())
        }
        K::Operation(operator @ (bray_bound_tree::BoundOperator::Add | bray_bound_tree::BoundOperator::Subtract), operands)
            if operands.len() == 2 =>
            Value::Operation(if *operator == bray_bound_tree::BoundOperator::Add { Op::Add } else { Op::Subtract },
                operands.iter().map(|value| captured(domain, state, value, memo)).collect()),
        K::Constructed(base, fields) => {
            let fields = fields.iter().map(|(field, value)|
                Some((value_projection(domain, *field)?, captured(domain, state, value, memo))))
                .collect::<Option<Arc<[_]>>>();

            match fields {
                Some(fields) => Value::Aggregate { base: Arc::new(captured(domain, state, base, memo)), fields },
                None => {
                    // An unrepresentable write may overlap other children of this value.
                    // Retain its established owner while asserting no old contents.
                    let Some((role, owner)) = base.ownership_identity() else { return Value::Unknown; };

                    Value::Operation(match role {
                        crate::execution_guarantees::ExecutionObservationKind::Owned => Op::Owned,
                        crate::execution_guarantees::ExecutionObservationKind::Reference => Op::Reference,
                        crate::execution_guarantees::ExecutionObservationKind::Scalar => Op::Scalar,
                    }, [captured_data(domain, state, owner, true, memo), Value::Unknown].into())
                }
            }
        }
        K::Present(value) => Value::Operation(Op::Present, [captured(domain, state, value, memo)].into()),
        _ => super::carriers::portable(domain.request, value.copied_value()).map_or(Value::Unknown, Value::Data),
    };

    memo.insert(key, encoded.clone());

    encoded
}

pub(super) fn conjunction(values: &bray_symbols::SemanticValueStore, terms: impl IntoIterator<Item = bray_symbols::ConstantTermId>)
    -> Option<bray_symbols::ConstantTermId>
{
    terms.into_iter().reduce(|left, right| values.intern_constant_term(bray_symbols::ConstantTermData::Binary {
        operation: bray_symbols::ConstantBinaryOperation::LogicalAnd, left, right,
    }).expect("checked outcome guards belong to their semantic store"))
}

fn captured_deferred<C: CheckerRequestContext + ?Sized>(domain: &ExecutionFlowDomain<'_, '_, C>, state: &ExecutionState,
    frame: &crate::execution_guarantees::ExecutionDeferredValue, memo: &mut HashMap<(ExecutionCondition, bool), Value>,
) -> Value {
    let values = domain.request.semantic_values();
    let proof = state.ordinary_proof();

    let alternatives = frame.producers.values().filter(|producer| super::super::deferred::possible(&producer.selection, &proof))
        .map(|producer| {
            let entry = &producer.entry;
            let call = producer.callee.clone();

            let guard = conjunction(values, producer.selection.iter().filter_map(|(condition, holds)| {
                let condition = if *holds { condition.clone() } else {
                    ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalNot, vec![condition.clone()])
                };

                super::carriers::portable(domain.request, &condition)
            }));

            let captures = entry.positional_arguments().enumerate().map(|(index, (ordinal, value))| {
                assert_eq!(ordinal.to_index(), Some(index), "deferred entry retains all receiver-first captures");

                captured(domain, state, value, memo)
            }).collect();

            bray_symbols::CapturedDeferredInvocation { guard, call, captures, owned_inputs: Arc::clone(&entry.owned_inputs) }
        }).collect();

    Value::Deferred { alternatives, unknown: frame.unknown_possible(&proof) }
}

fn captured_input<C: CheckerRequestContext + ?Sized>(domain: &ExecutionFlowDomain<'_, '_, C>, state: &ExecutionState,
    value: &ExecutionCondition, place: &crate::ExecutionPlace, descriptor: bool,
) -> Value {
    let Some(term) = super::carriers::portable(domain.request, value) else { return Value::Unknown; };

    let mut root = place.clone();

    root.projections = Arc::new([]);

    let borrowed = domain.borrowed_inputs.contains(&root);
    let root = ExecutionCondition::new(K::Input(root));

    if descriptor || !borrowed
    { return Value::Input(term); }

    let root = super::carriers::portable(domain.request, &root)
        .expect("a portable formal projection retains its declared root");

    let mut value_expression = Value::Operation(Op::Read, [Value::Input(root)].into());

    for projection in crate::storage::owned_storage_projections(domain.dependencies, &place.projections) {
        let Some(projection) = value_projection(domain, projection) else { return Value::Unknown; };

        value_expression = Value::Operation(Op::Select(projection), [value_expression].into());
    }

    if state.owned_result(value).guaranteed_transfers().iter().any(|transfer| transfer.output().is_empty()) {
        Value::Operation(Op::Owned, [value_expression.clone(), value_expression].into())
    } else { value_expression }
}

fn value_projection<C: CheckerRequestContext + ?Sized>(domain: &ExecutionFlowDomain<'_, '_, C>,
    projection: bray_bound_tree::StorageProjection,
) -> Option<bray_symbols::DependencyProjection> {
    match projection {
        bray_bound_tree::StorageProjection::AnyElement | bray_bound_tree::StorageProjection::AddressTarget => None,
        projection => crate::storage::value_dependency_projection(domain.request, projection),
    }
}
