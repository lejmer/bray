use super::super::mapping::{LocalIds, concrete_type, copy_unit_layout, predict_results};
use super::control::{copy_inline_body, copy_operation, store};
use super::values::InlineValues;
use crate::unit::local_id_remap::{MirLocalIdMapping, remap_terminator};
use crate::{
    MirBlockId, MirBlockKind, MirCall, MirCallArgument, MirCapacityError, MirEdge, MirOperand,
    MirOperationId, MirOperationKind, MirPlace, MirStorageId, MirStorageKind, MirTerminatorKind,
    MirUnit, MirUnitBuilder, MirUnitKind,
};
use bray_symbols::TypeId;
use std::collections::BTreeMap;

/// Splices a checked synchronous callee into a direct call and its outcome continuations.
///
/// The caller supplies the callee's concrete semantic types and callable references.
/// Parameter storage retains borrowed projections. Explicit abnormal exits transfer reports into
/// the original caller's failure edges, after the callee's own cleanup has completed.
/// `scalar_parameters` requires the caller's scalar-body eligibility proof: parameter reads
/// have no projections or address uses, and the body has only ordinary scalar control flow.
pub fn inline_call(
    caller: &MirUnit,
    site: MirOperationId,
    callee: &MirUnit,
    concrete_types: &BTreeMap<TypeId, TypeId>,
    concrete_callables: &BTreeMap<crate::MirCallableReference, crate::MirCallableReference>,
    caller_types: &BTreeMap<TypeId, TypeId>,
    scalar_parameters: bool,
) -> Result<Option<MirUnit>, MirCapacityError> {
    let Some((call_block_id, call_block)) = caller
        .blocks_with_ids()
        .find(|(_, block)| block.operations().contains(&site))
    else {
        return Ok(None);
    };

    let operation = caller.operation(site).expect("call site exists");

    let MirOperationKind::Call(call) = operation.kind() else {
        return Ok(None);
    };

    let last = call_block.operations().last() == Some(&site);

    let (mut continuation, report, failure) = match call_block.terminator().kind() {
        MirTerminatorKind::CheckCallOutcome {
            completed,
            completed_report,
            panicked,
            cancelled,
        } if last => (
            MirTerminatorKind::Goto(completed.clone()),
            completed_report.as_ref(),
            Some((panicked, cancelled)),
        ),
        MirTerminatorKind::TakeCallReport { completed, report } if last => (
            MirTerminatorKind::Goto(completed.clone()),
            Some(report),
            None,
        ),
        kind if !call.may_propagate_panic() => (kind.clone(), None, None),
        _ => return Ok(None),
    };

    let parameters = callee
        .storages_with_ids()
        .filter_map(|(id, storage)| match storage.kind() {
            MirStorageKind::Parameter(position) => Some((*position, id)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();

    let result_type = operation.result().map(|_| call.result().ty());

    let Some(conversions) = matched_arguments(
        caller,
        callee,
        call,
        &parameters,
        concrete_types,
        caller_types,
    ) else {
        return Ok(None);
    };

    if !body_is_compatible(
        caller,
        callee,
        call_block.kind(),
        result_type,
        concrete_types,
        report,
        failure.is_some(),
    ) {
        return Ok(None);
    }

    let mut builder = MirUnitBuilder::for_reconstruction(caller);
    let unchanged_types = BTreeMap::new();
    let mut caller_ids = LocalIds::new(caller, &unchanged_types);
    let mut callee_ids = LocalIds::new(callee, concrete_types);
    let source = operation.source().clone();

    callee_ids.callables = Some(concrete_callables);

    copy_inline_layout(
        caller,
        &mut caller_ids,
        callee,
        &mut callee_ids,
        &mut builder,
    )?;

    let join = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;

    if let Some(result) = operation.result() {
        caller_ids.values[result.to_index().expect("valid result")] =
            Some(builder.push_block_parameter(
                join,
                source.clone(),
                result_type.expect("call result type"),
            )?);
    }

    let scalar_inputs = if scalar_parameters {
        parameters.clone()
    } else {
        BTreeMap::new()
    };

    let values = InlineValues::prepare(
        caller,
        callee,
        (site, call_block),
        (join, &mut continuation),
        &scalar_inputs,
        &callee_ids,
        &mut builder,
    )?;

    predict_inline_results(
        caller,
        &mut caller_ids,
        callee,
        &mut callee_ids,
        site,
        conversions,
        builder.value_count(),
    )?;

    let mut entry_arguments = Vec::new();

    for (old, block) in caller.blocks_with_ids() {
        let mut destination = caller_ids.block(old);

        for id in block.operations() {
            if *id == site {
                // Evaluate every argument exactly once before entering the callee. The saved
                // caller values survive all callee blocks, including report-only failure edges.
                entry_arguments = initialize_parameters(
                    caller,
                    callee,
                    call,
                    &parameters,
                    (&caller_ids, &callee_ids),
                    (destination, &source),
                    &mut builder,
                    scalar_parameters,
                )?;

                for (value, place) in &values.spills {
                    store(
                        &mut builder,
                        destination,
                        &source,
                        place.clone(),
                        MirOperand::Value(caller_ids.value(*value)),
                    )?;
                }

                entry_arguments.extend(
                    values
                        .continuation
                        .keys()
                        .map(|value| MirOperand::Value(caller_ids.value(*value))),
                );

                values.map_caller(&mut caller_ids, values.continuation.values().copied());
                destination = join;
            } else {
                copy_operation(caller, *id, destination, &caller_ids, &mut builder)?;
            }
        }

        let mut kind = if old == call_block_id {
            MirTerminatorKind::Goto(MirEdge::new(
                callee_ids.block(callee.entry()),
                entry_arguments.iter().cloned(),
            ))
        } else {
            block.terminator().kind().clone()
        };

        if old != call_block_id {
            remap_terminator(&mut kind, &caller_ids);
        }

        builder.set_terminator(
            caller_ids.block(old),
            block.terminator().source().clone(),
            kind,
        );
    }

    copy_inline_body(
        callee,
        &mut callee_ids,
        &mut builder,
        (join, report),
        failure,
        &mut caller_ids,
        &values,
    )?;

    values.map_caller(&mut caller_ids, values.continuation.values().copied());

    remap_terminator(&mut continuation, &caller_ids);
    builder.set_terminator(join, call_block.terminator().source().clone(), continuation);

    let inlined = builder.finish(caller_ids.block(caller.entry()));

    assert!(
        inlined.is_valid(),
        "call splice must preserve valid MIR for caller {:?}, callee {:?}, site {:?}",
        caller.key(),
        callee.key(),
        site
    );

    Ok(Some(inlined))
}

#[expect(
    clippy::too_many_arguments,
    reason = "call splicing retains both checked scopes and initializes one callee's arguments"
)]
fn initialize_parameters(
    caller: &MirUnit,
    callee: &MirUnit,
    call: &MirCall,
    parameters: &BTreeMap<u32, MirStorageId>,
    ids: (&LocalIds<'_>, &LocalIds<'_>),
    entry: (MirBlockId, &crate::MirSourceAnchor),
    builder: &mut MirUnitBuilder,
    scalar_parameters: bool,
) -> Result<Vec<MirOperand>, MirCapacityError> {
    let mut parameter_arguments = BTreeMap::new();

    for argument in call.arguments() {
        let position = argument_position(call, argument);
        let storage = parameters[&position];

        let ty = ids
            .1
            .ty(callee.storage(storage).expect("parameter storage").ty());

        let mut value = argument.value().clone();
        let source_type = caller.operand_type(&value).expect("argument type");

        value.remap_local_ids(ids.0);

        value = normalize_argument(builder, entry.0, entry.1, value, source_type, ty)?;

        if scalar_parameters {
            parameter_arguments.insert(position, value);
        } else {
            store(
                builder,
                entry.0,
                entry.1,
                MirPlace::new(ids.1.storage(storage), [], ty),
                value,
            )?;
        }
    }

    Ok(parameter_arguments.into_values().collect())
}

fn predict_inline_results(
    caller: &MirUnit,
    caller_ids: &mut LocalIds<'_>,
    callee: &MirUnit,
    callee_ids: &mut LocalIds<'_>,
    site: MirOperationId,
    conversions: usize,
    first: usize,
) -> Result<(), MirCapacityError> {
    let mut next_value = first;

    predict_results(
        caller,
        caller_ids,
        caller.blocks_with_ids().map(|(id, _)| id),
        Some((site, conversions)),
        &mut next_value,
        caller.unit(),
    )?;

    predict_results(
        callee,
        callee_ids,
        callee.blocks_with_ids().map(|(id, _)| id),
        None,
        &mut next_value,
        caller.unit(),
    )?;

    Ok(())
}

fn copy_inline_layout(
    caller: &MirUnit,
    caller_ids: &mut LocalIds<'_>,
    callee: &MirUnit,
    callee_ids: &mut LocalIds<'_>,
    builder: &mut MirUnitBuilder,
) -> Result<(), MirCapacityError> {
    copy_unit_layout(
        caller,
        caller_ids,
        builder,
        caller.blocks_with_ids(),
        caller.storages_with_ids(),
        MirStorageKind::clone,
    )?;

    copy_unit_layout(
        callee,
        callee_ids,
        builder,
        callee.blocks_with_ids(),
        callee.storages_with_ids(),
        |kind| match kind {
            MirStorageKind::Parameter(_) | MirStorageKind::Return => MirStorageKind::Local,
            kind => kind.clone(),
        },
    )?;

    Ok(())
}

fn argument_position(call: &MirCall, argument: &MirCallArgument) -> u32 {
    match argument {
        MirCallArgument::Receiver { .. } => 0,
        MirCallArgument::Explicit { ordinal, .. } => {
            ordinal
                + u32::from(
                    call.arguments()
                        .iter()
                        .any(|argument| matches!(argument, MirCallArgument::Receiver { .. })),
                )
        }
    }
}

fn matched_arguments(
    caller: &MirUnit,
    callee: &MirUnit,
    call: &MirCall,
    parameters: &BTreeMap<u32, MirStorageId>,
    types: &BTreeMap<TypeId, TypeId>,
    caller_types: &BTreeMap<TypeId, TypeId>,
) -> Option<usize> {
    if parameters.len() != call.arguments().len()
        || !parameters
            .keys()
            .enumerate()
            .all(|(index, position)| usize::try_from(*position) == Ok(index))
    {
        return None;
    }

    let mut conversions = 0;

    for argument in call.arguments() {
        let parameter = parameters.get(&argument_position(call, argument))?;
        let expected = concrete_type(callee.storage(*parameter)?.ty(), types);
        let actual = caller.operand_type(argument.value())?;

        if concrete_type(actual, caller_types) != expected {
            return None;
        }

        conversions += usize::from(actual != expected);
    }

    Some(conversions)
}

fn normalize_argument(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &crate::MirSourceAnchor,
    value: MirOperand,
    source_type: TypeId,
    target_type: TypeId,
) -> Result<MirOperand, MirCapacityError> {
    if source_type == target_type {
        return Ok(value);
    }

    // The call site's existing substitution proves these are the same semantic type.
    // Retain both scopes in the MIR instead of rewriting the caller's other references.
    let operation = builder.push_operation(
        block,
        source.clone(),
        MirOperationKind::Convert {
            operand: value,
            conversion: crate::SelectedConversion::new(
                source_type,
                target_type,
                crate::ConversionTarget::Identity,
            ),
        },
        Some(target_type),
    )?;

    Ok(MirOperand::Value(
        operation.result().expect("normalized argument"),
    ))
}

fn body_is_compatible(
    caller: &MirUnit,
    callee: &MirUnit,
    kind: MirBlockKind,
    result: Option<TypeId>,
    types: &BTreeMap<TypeId, TypeId>,
    normal_report: Option<&MirPlace>,
    has_failure: bool,
) -> bool {
    matches!(caller.kind(), MirUnitKind::Synchronous)
        && matches!(callee.kind(), MirUnitKind::Synchronous)
        && caller.frame_descriptor().is_none()
        && callee.frame_descriptor().is_none()
        && callee.cleanup_continuation().is_none()
        && kind == MirBlockKind::Ordinary
        && callee
            .block(callee.entry())
            .expect("callee entry")
            .parameters()
            .is_empty()
        && callee
            .storages_with_ids()
            .all(|(_, storage)| !matches!(storage.kind(), MirStorageKind::BorrowedParameter(_)))
        && callee
            .blocks()
            .iter()
            .any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Return { .. }))
        && callee
            .blocks()
            .iter()
            .all(|block| match block.terminator().kind() {
                MirTerminatorKind::Return { value, report } => {
                    (report.is_none() || normal_report.is_some())
                        && value.as_ref().map(|value| {
                            concrete_type(callee.operand_type(value).expect("return type"), types)
                        }) == result
                }
                MirTerminatorKind::PropagatePanic { .. }
                | MirTerminatorKind::PropagateCancellation { .. } => has_failure,
                MirTerminatorKind::RangeIterate { .. }
                | MirTerminatorKind::Suspend { .. }
                | MirTerminatorKind::ForwardRunResult { .. }
                | MirTerminatorKind::InlineAssembly(_) => false,
                _ => true,
            })
        && body_supports_type_substitution(callee, types)
}

fn body_supports_type_substitution(callee: &MirUnit, types: &BTreeMap<TypeId, TypeId>) -> bool {
    if types.iter().all(|(source, target)| source == target) {
        return true;
    }

    callee
        .operations()
        .iter()
        .all(|operation| match operation.kind() {
            MirOperationKind::Memory(memory) => {
                let mut retained = std::collections::BTreeSet::new();

                crate::types::collect_memory_types(memory.kind(), &mut retained);
                retained.extend(memory.operand_types());
                retained.extend(memory.result_type());

                retained
                    .into_iter()
                    .all(|ty| concrete_type(ty, types) == ty)
            }
            MirOperationKind::Text(text) => text
                .operand_types()
                .iter()
                .copied()
                .chain(text.result_type())
                .all(|ty| concrete_type(ty, types) == ty),
            MirOperationKind::Convert { conversion, .. } => {
                matches!(
                    conversion.target(),
                    bray_bound_tree::ConversionTarget::Identity
                )
            }
            MirOperationKind::TraitView { .. } | MirOperationKind::TraitViewMethod { .. } => false,
            MirOperationKind::Construct(construction) => !matches!(
                construction.target(),
                bray_bound_tree::ConstructionTarget::TypeForm { .. }
            ),
            MirOperationKind::Call(call) => {
                call.witnesses().is_empty()
                    && !matches!(call.target(), crate::MirCallTarget::DefaultValue { .. })
            }
            _ => true,
        })
}
