use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::TypeId;

use crate::{
    MirBlockId, MirBlockKind, MirCall, MirCallArgument, MirCapacityError, MirEdge, MirOperand,
    MirOperationId, MirOperationKind, MirStorageId, MirStorageKind, MirTerminatorKind, MirUnit,
    MirUnitBuilder, MirUnitKind, MirValueId,
};

use super::super::local_id_remap::{MirLocalIdMapping, remap_operation, remap_terminator};
use super::mapping::{LocalIds, concrete_type, copy_unit_layout, predict_results};

/// Splices a checked scalar callee into one direct call and its normal continuation.
///
/// The caller must supply a synchronous callee whose operations cannot panic, suspend, access
/// addresses, or require cleanup. Ineligible call shapes are left unchanged. The resulting body
/// keeps the caller's semantic identity and each callee node's source provenance.
pub fn inline_scalar_call(
    caller: &MirUnit,
    site: MirOperationId,
    callee: &MirUnit,
    concrete_types: &BTreeMap<TypeId, TypeId>,
) -> Result<Option<MirUnit>, MirCapacityError> {
    let Some((call_block_id, call_block)) = caller
        .blocks_with_ids()
        .find(|(_, block)| block.operations().contains(&site))
    else {
        return Ok(None);
    };

    let Some(operation) = caller.operation(site) else {
        return Ok(None);
    };

    let MirOperationKind::Call(call) = operation.kind() else {
        return Ok(None);
    };

    let mut continuation = match call_block.terminator().kind() {
        MirTerminatorKind::CheckCallOutcome { completed, .. }
            if call_block.operations().last() == Some(&site) =>
        {
            MirTerminatorKind::Goto(completed.clone())
        }
        kind if !call.may_propagate_panic() => kind.clone(),
        _ => return Ok(None),
    };

    let parameters = callee
        .storages_with_ids()
        .filter_map(|(id, storage)| match storage.kind() {
            MirStorageKind::Parameter(position) => Some((*position, id)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();

    if !scalar_arguments_match(caller, callee, call, &parameters, concrete_types) {
        return Ok(None);
    }

    let result_type = operation
        .result()
        .map(|value| caller.value(value).expect("valid call result").ty());

    if !scalar_body_is_compatible(
        caller,
        callee,
        call_block.kind(),
        result_type,
        concrete_types,
    ) {
        return Ok(None);
    }

    // The builder owns its MIR nodes; cloning from the immutable inputs preserves their source
    // and semantic payloads while the two local identity maps rewrite only unit-local IDs.
    let mut builder = MirUnitBuilder::for_reconstruction(caller);
    let caller_types = BTreeMap::new();
    let mut caller_ids = LocalIds::new(caller, &caller_types);
    let mut callee_ids = LocalIds::new(callee, concrete_types);
    let source = operation.source().clone();

    copy_unit_layout(
        caller,
        &mut caller_ids,
        &mut builder,
        caller.blocks_with_ids(),
        caller.storages_with_ids(),
        MirStorageKind::clone,
    )?;

    copy_unit_layout(
        callee,
        &mut callee_ids,
        &mut builder,
        callee.blocks_with_ids(),
        callee.storages_with_ids(),
        |kind| match kind {
            MirStorageKind::Parameter(_) | MirStorageKind::Return => MirStorageKind::Local,
            kind => kind.clone(),
        },
    )?;

    let join = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;

    let retained = continuation_values(caller, call_block, site, &mut continuation);
    let mut forwarded = BTreeMap::new();
    let mut parameter_values = BTreeMap::new();

    for (block_id, _) in callee.blocks_with_ids() {
        let mut block_values = BTreeMap::new();

        for storage in parameters.values() {
            let ty = concrete_type(
                callee.storage(*storage).expect("parameter storage").ty(),
                concrete_types,
            );

            let value =
                builder.push_block_parameter(callee_ids.block(block_id), source.clone(), ty)?;

            block_values.insert(*storage, value);
        }

        if !retained.is_empty() {
            let values = retained
                .iter()
                .map(|value| {
                    builder.push_block_parameter(
                        callee_ids.block(block_id),
                        source.clone(),
                        caller
                            .value(*value)
                            .expect("retained caller value exists")
                            .ty(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;

            forwarded.insert(block_id, values);
        }

        parameter_values.insert(block_id, block_values);
    }

    if let Some(result) = operation.result() {
        caller_ids.values[result.to_index().expect("valid result")] =
            Some(builder.push_block_parameter(
                join,
                source.clone(),
                result_type.expect("call result type"),
            )?);
    }

    let retained_parameters = retained
        .iter()
        .map(|value| {
            let parameter = builder.push_block_parameter(
                join,
                source.clone(),
                caller
                    .value(*value)
                    .expect("retained caller value exists")
                    .ty(),
            )?;

            Ok((*value, parameter))
        })
        .collect::<Result<BTreeMap<_, _>, MirCapacityError>>()?;

    let mut next_value = builder.value_count();

    predict_results(
        caller,
        &mut caller_ids,
        caller.blocks_with_ids().map(|(id, _)| id),
        Some(site),
        &mut next_value,
        caller.unit(),
    )?;

    predict_results(
        callee,
        &mut callee_ids,
        callee.blocks_with_ids().map(|(id, _)| id),
        None,
        &mut next_value,
        caller.unit(),
    )?;

    let arguments = scalar_call_arguments(call, &parameters, &caller_ids)
        .into_iter()
        .chain(
            retained
                .iter()
                .map(|value| MirOperand::Value(caller_ids.value(*value))),
        )
        .collect::<Vec<_>>();

    copy_unit_operations(
        caller,
        &mut caller_ids,
        &mut builder,
        Some((site, join, &retained_parameters)),
        None,
    )?;

    copy_unit_operations(
        callee,
        &mut callee_ids,
        &mut builder,
        None,
        Some(&parameter_values),
    )?;

    for (old, block) in caller.blocks_with_ids() {
        let terminator = if old == call_block_id {
            // Each reconstruction owns the edge's operand descriptors.
            MirTerminatorKind::Goto(MirEdge::new(
                callee_ids.block(callee.entry()),
                arguments.iter().cloned(),
            ))
        } else {
            let mut kind = block.terminator().kind().clone();

            remap_terminator(&mut kind, &caller_ids);

            kind
        };

        builder.set_terminator(
            caller_ids.block(old),
            block.terminator().source().clone(),
            terminator,
        );
    }

    copy_callee_terminators(
        callee,
        &mut callee_ids,
        &mut builder,
        join,
        &parameter_values,
        &forwarded,
    );

    remap_terminator(&mut continuation, &caller_ids);
    builder.set_terminator(join, call_block.terminator().source().clone(), continuation);

    let inlined = builder.finish(caller_ids.block(caller.entry()));

    assert!(
        inlined.is_valid(),
        "scalar call splice must preserve valid MIR for caller {:?}, callee {:?}, site {:?}",
        caller.key(),
        callee.key(),
        site
    );

    Ok(Some(inlined))
}

fn scalar_call_arguments(
    call: &MirCall,
    parameters: &BTreeMap<u32, MirStorageId>,
    caller_ids: &LocalIds<'_>,
) -> Vec<MirOperand> {
    parameters
        .keys()
        .map(|position| {
            let value = call
                .arguments()
                .iter()
                .find_map(|argument| match argument {
                    MirCallArgument::Explicit { ordinal, value, .. } if ordinal == position => {
                        Some(value)
                    }
                    _ => None,
                })
                .expect("matched call argument");

            let mut value = value.clone();

            value.remap_local_ids(caller_ids);

            value
        })
        .collect()
}

fn scalar_arguments_match(
    caller: &MirUnit,
    callee: &MirUnit,
    call: &MirCall,
    parameters: &BTreeMap<u32, MirStorageId>,
    concrete_types: &BTreeMap<TypeId, TypeId>,
) -> bool {
    parameters.len() == call.arguments().len()
        && parameters
            .keys()
            .enumerate()
            .all(|(index, position)| usize::try_from(*position) == Ok(index))
        && call.arguments().iter().all(|argument| match argument {
            MirCallArgument::Explicit { ordinal, value, .. } => {
                parameters.get(ordinal).is_some_and(|id| {
                    concrete_type(
                        callee.storage(*id).expect("parameter storage").ty(),
                        concrete_types,
                    ) == caller.operand_type(value).expect("valid call argument")
                })
            }
            MirCallArgument::Receiver { .. } => false,
        })
}

fn scalar_body_is_compatible(
    caller: &MirUnit,
    callee: &MirUnit,
    call_block_kind: MirBlockKind,
    result_type: Option<TypeId>,
    concrete_types: &BTreeMap<TypeId, TypeId>,
) -> bool {
    if !matches!(caller.kind(), MirUnitKind::Synchronous)
        || !matches!(callee.kind(), MirUnitKind::Synchronous)
        || caller.frame_descriptor().is_some()
        || callee.frame_descriptor().is_some()
        || call_block_kind != MirBlockKind::Ordinary
        || callee
            .blocks()
            .iter()
            .any(|block| block.kind() != MirBlockKind::Ordinary)
        || !callee
            .block(callee.entry())
            .expect("valid callee entry")
            .parameters()
            .is_empty()
    {
        return false;
    }

    callee
        .blocks()
        .iter()
        .any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Return { .. }))
        && callee
            .blocks()
            .iter()
            .all(|block| match block.terminator().kind() {
                MirTerminatorKind::Return { value, report } => {
                    report.is_none()
                        && value.as_ref().map(|value| {
                            concrete_type(
                                callee.operand_type(value).expect("valid return"),
                                concrete_types,
                            )
                        }) == result_type
                }
                MirTerminatorKind::Goto(_)
                | MirTerminatorKind::Branch { .. }
                | MirTerminatorKind::Switch { .. } => true,
                _ => false,
            })
}

fn copy_unit_operations(
    unit: &MirUnit,
    ids: &mut LocalIds<'_>,
    builder: &mut MirUnitBuilder,
    split: Option<(
        MirOperationId,
        MirBlockId,
        &BTreeMap<MirValueId, MirValueId>,
    )>,
    parameter_values: Option<&BTreeMap<MirBlockId, BTreeMap<MirStorageId, MirValueId>>>,
) -> Result<(), MirCapacityError> {
    for (old, block) in unit.blocks_with_ids() {
        if let Some(parameters) = parameter_values {
            // Each block remaps immutable callee parameters to its own argument values.
            ids.parameters = parameters[&old].clone();
        }

        let mut destination = ids.block(old);

        for operation_id in block.operations() {
            if let Some((site, continuation, retained)) = split
                && *operation_id == site
            {
                // Uses after the split observe the join's parameters, while the prefix
                // definitions and callee-entry arguments retain their original identities.
                for (value, parameter) in retained {
                    ids.values[value.to_index().expect("retained value has a local slot")] =
                        Some(*parameter);
                }

                destination = continuation;
                continue;
            }

            let original = unit.operation(*operation_id).expect("valid operation");
            let mut kind = original.kind().clone();

            remap_operation(&mut kind, ids);

            let ty = original
                .result()
                .map(|value| ids.ty(unit.value(value).expect("result").ty()));

            let commit =
                builder.push_operation(destination, original.source().clone(), kind, ty)?;

            assert_eq!(
                commit.result(),
                original.result().map(|value| ids.value(value))
            );
        }
    }

    Ok(())
}

fn continuation_values(
    unit: &MirUnit,
    block: &crate::MirBlock,
    site: MirOperationId,
    continuation: &mut MirTerminatorKind,
) -> BTreeSet<MirValueId> {
    // Expanded effects retain arena identities while inserting work before an old
    // suffix. Only execution order establishes which values exist at this call.
    let available = block
        .parameters()
        .iter()
        .copied()
        .chain(
            block
                .operations()
                .iter()
                .take_while(|operation| **operation != site)
                .filter_map(|operation| {
                    unit.operation(*operation)
                        .expect("valid prefix operation")
                        .result()
                }),
        )
        .collect::<BTreeSet<_>>();

    let mut values = BTreeSet::new();

    let mut visit = |operand: &MirOperand| {
        if let MirOperand::Value(value) = operand
            && available.contains(value)
        {
            values.insert(*value);
        }
    };

    for operation in block
        .operations()
        .iter()
        .skip_while(|operation| **operation != site)
        .skip(1)
    {
        unit.operation(*operation)
            .expect("valid suffix operation")
            .kind()
            .for_each_operand(&mut visit);
    }

    continuation.for_each_input(&mut visit);

    continuation
        .try_for_each_edge_mut::<()>(|edge| {
            for argument in edge.arguments() {
                argument.for_each_operand(&mut visit);
            }

            Ok(())
        })
        .expect("reading continuation edges cannot fail");

    values
}

fn copy_callee_terminators(
    callee: &MirUnit,
    ids: &mut LocalIds<'_>,
    builder: &mut MirUnitBuilder,
    join: MirBlockId,
    parameters: &BTreeMap<MirBlockId, BTreeMap<MirStorageId, MirValueId>>,
    forwarded: &BTreeMap<MirBlockId, Vec<MirValueId>>,
) {
    for (old, block) in callee.blocks_with_ids() {
        // Each block resolves parameter reads to its own SSA arguments.
        ids.parameters = parameters[&old].clone();

        let retained = if forwarded.is_empty() {
            &[][..]
        } else {
            forwarded[&old].as_slice()
        };

        let kind = match block.terminator().kind() {
            MirTerminatorKind::Return { value, report } => {
                assert!(
                    report.is_none(),
                    "scalar inlining requires a return without an owned report"
                );

                let arguments = value
                    .iter()
                    .map(|value| {
                        let mut value = value.clone();

                        value.remap_local_ids(ids);

                        value
                    })
                    .chain(retained.iter().copied().map(MirOperand::Value))
                    .collect::<Vec<_>>();

                MirTerminatorKind::Goto(MirEdge::new(join, arguments))
            }
            other => {
                let mut kind = other.clone();

                remap_terminator(&mut kind, ids);

                kind.try_for_each_edge_mut::<()>(|edge| {
                    *edge = MirEdge::new(
                        edge.target(),
                        edge.arguments().iter().cloned().chain(
                            ids.parameters
                                .values()
                                .copied()
                                .map(MirOperand::Value)
                                .chain(retained.iter().copied().map(MirOperand::Value)),
                        ),
                    );

                    Ok(())
                })
                .expect("callee edges can be extended without failure");

                kind
            }
        };

        builder.set_terminator(ids.block(old), block.terminator().source().clone(), kind);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_testing::test_bound_unit;

    use super::continuation_values;
    use crate::{
        MirBlock, MirBlockKind, MirImmediateValue, MirOperand, MirOperationKind,
        MirSourceAnchor, MirTerminatorKind, MirUnaryOperator, MirUnitBuilder, MirUnitKind,
    };

    #[test]
    fn scalar_continuation_tracks_execution_order_after_effect_expansion() {
        let bound = test_bound_unit(801);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            crate::test_support::test_target(),
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        let mut unary = |operand| {
            builder
                .push_operation(
                    entry,
                    source.clone(),
                    MirOperationKind::Unary { operator: MirUnaryOperator::Not, operand },
                    Some(ty),
                )
                .unwrap()
        };

        let prefix = unary(MirOperand::Immediate { ty, value: MirImmediateValue::Boolean(false) });
        let suffix = unary(MirOperand::Value(prefix.result().unwrap()));
        let use_suffix = unary(MirOperand::Value(suffix.result().unwrap()));
        let site = unary(MirOperand::Immediate { ty, value: MirImmediateValue::Boolean(true) });

        builder.set_terminator(
            entry,
            source,
            MirTerminatorKind::Return { value: Some(MirOperand::Value(site.result().unwrap())), report: None },
        );

        let mut unit = builder.finish(entry);
        let block = unit.blocks()[0].clone();

        // Effect expansion inserts newly allocated operations ahead of an existing suffix.
        Arc::make_mut(&mut unit.blocks)[0] = MirBlock::new(
            block.source().clone(),
            block.kind(),
            block.parameters().to_vec(),
            vec![prefix.operation(), site.operation(), suffix.operation(), use_suffix.operation()],
            block.terminator().clone(),
        );

        assert!(unit.is_valid());

        let block = unit.block(entry).unwrap();
        let mut continuation = block.terminator().kind().clone();

        assert_eq!(
            continuation_values(&unit, block, site.operation(), &mut continuation),
            BTreeSet::from([prefix.result().unwrap()]),
        );
    }
}
