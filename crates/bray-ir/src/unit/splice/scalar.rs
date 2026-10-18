use std::collections::BTreeMap;

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

    copy_unit_operations(
        caller,
        &mut caller_ids,
        &mut builder,
        Some((site, join)),
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
            let arguments = scalar_call_arguments(call, &parameters, &caller_ids);

            MirTerminatorKind::Goto(MirEdge::new(callee_ids.block(callee.entry()), arguments))
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

    for (old, block) in callee.blocks_with_ids() {
        callee_ids.parameters = parameter_values[&old].clone();

        let kind = match block.terminator().kind() {
            MirTerminatorKind::Return(value) => {
                let arguments = value
                    .iter()
                    .map(|value| {
                        let mut value = value.clone();

                        value.remap_local_ids(&callee_ids);

                        value
                    })
                    .collect::<Vec<_>>();

                MirTerminatorKind::Goto(MirEdge::new(join, arguments))
            }
            other => {
                let mut kind = other.clone();

                remap_terminator(&mut kind, &callee_ids);

                kind.try_for_each_edge_mut::<()>(|edge| {
                    *edge = MirEdge::new(
                        edge.target(),
                        edge.arguments().iter().cloned().chain(
                            callee_ids
                                .parameters
                                .values()
                                .copied()
                                .map(MirOperand::Value),
                        ),
                    );

                    Ok(())
                })
                .expect("callee edges can be extended without failure");

                kind
            }
        };

        builder.set_terminator(
            callee_ids.block(old),
            block.terminator().source().clone(),
            kind,
        );
    }

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
        .any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Return(_)))
        && callee
            .blocks()
            .iter()
            .all(|block| match block.terminator().kind() {
                MirTerminatorKind::Return(value) => {
                    value.as_ref().map(|value| {
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
    split: Option<(MirOperationId, MirBlockId)>,
    parameter_values: Option<&BTreeMap<MirBlockId, BTreeMap<MirStorageId, MirValueId>>>,
) -> Result<(), MirCapacityError> {
    for (old, block) in unit.blocks_with_ids() {
        if let Some(parameters) = parameter_values {
            // Each block remaps immutable callee parameters to its own argument values.
            ids.parameters = parameters[&old].clone();
        }

        let mut destination = ids.block(old);

        for operation_id in block.operations() {
            if let Some((site, continuation)) = split
                && *operation_id == site
            {
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
