use crate::{MirBlock, MirBlockId, MirBlockKind, MirUnit, MirValueId, MirValueOrigin};

use super::control::validate_terminator;
use super::operation::validate_operation;

pub(in crate::unit) fn validate_unit(unit: &MirUnit) -> Option<()> {
    let Some(entry) = unit.block(unit.entry()) else {
        return None;
    };

    if entry.kind() != MirBlockKind::Ordinary {
        return None;
    }

    validate_frame_descriptor(unit)?;
    validate_value_definitions(unit)?;
    validate_blocks(unit)?;
    validate_host_sequence(unit)?;

    Some(())
}
fn validate_host_sequence(unit: &MirUnit) -> Option<()> {
    let crate::MirUnitKind::ExecutableHost(host) = unit.kind() else {
        return Some(());
    };

    let operations = unit
        .operations()
        .iter()
        .map(crate::MirOperation::kind)
        .collect::<Vec<_>>();

    let Some((shutdown, preceding)) = operations.split_last() else {
        return None;
    };

    let selects_entries = host
        .requirements()
        .requires_role(bray_runtime_interface::RuntimeAbiRole::TestEntrySelection);

    if !matches!(
        shutdown,
        crate::MirOperationKind::Host(crate::MirHostOperation::StructuredShutdown { .. })
    ) {
        return None;
    }

    let materialized = preceding.partition_point(|operation| {
        matches!(
            operation,
            crate::MirOperationKind::Host(crate::MirHostOperation::MaterializeStatic { .. })
        )
    });

    let entries = &preceding[materialized..];
    let operations_per_entry = if selects_entries { 4_usize } else { 3_usize };

    let entry_operation_count = operations_per_entry.checked_mul(host.entries().len())?;

    if entries.len() < entry_operation_count + 2 {
        return None;
    }

    let (entries, cleanup) = entries.split_at(entry_operation_count);

    for (expected, operations) in entries.chunks_exact(operations_per_entry).enumerate() {
        let operations = if selects_entries {
            let Some(crate::MirOperationKind::Host(crate::MirHostOperation::SelectTestEntry {
                entry,
                ..
            })) = operations.first()
            else {
                return None;
            };

            if usize::try_from(entry.slot()) != Ok(expected) {
                return None;
            }

            &operations[1..]
        } else {
            operations
        };

        let [
            crate::MirOperationKind::Host(crate::MirHostOperation::ExecuteRoot {
                entry: executed,
                ..
            }),
            crate::MirOperationKind::Host(crate::MirHostOperation::ObserveRootTerminal {
                entry: observed,
                ..
            }),
            crate::MirOperationKind::Host(crate::MirHostOperation::ResolveRootTerminal {
                entry: resolved,
                ..
            }),
        ] = operations
        else {
            return None;
        };

        if [executed, observed, resolved]
            .into_iter()
            .any(|entry| usize::try_from(entry.slot()) != Ok(expected))
        {
            return None;
        }
    }

    if !matches!(
        cleanup,
        [
            crate::MirOperationKind::Host(crate::MirHostOperation::BeginStaticCleanup),
            crate::MirOperationKind::Host(crate::MirHostOperation::ReportCleanupIncidents { .. }),
        ]
    ) {
        return None;
    }

    Some(())
}

fn validate_frame_descriptor(unit: &MirUnit) -> Option<()> {
    let descriptor = match (unit.kind(), unit.frame_descriptor()) {
        (crate::MirUnitKind::ProtectedAsyncFrame(frame), Some(descriptor)) => {
            if descriptor.frame() != *frame {
                return None;
            }

            if descriptor.abi_version() != unit.target().runtime_abi() {
                return None;
            }

            descriptor
        }
        (crate::MirUnitKind::ProtectedAsyncFrame(_), None) => {
            return None;
        }
        (
            crate::MirUnitKind::Synchronous
            | crate::MirUnitKind::ExecutableHost(_)
            | crate::MirUnitKind::GeneratedLifecycle(_),
            Some(_),
        ) => {
            return None;
        }
        (crate::MirUnitKind::ExecutableHost(host), None) => {
            if host.abi_version() != unit.target().runtime_abi() {
                return None;
            }

            return Some(());
        }
        (crate::MirUnitKind::Synchronous | crate::MirUnitKind::GeneratedLifecycle(_), None) => {
            return Some(());
        }
    };

    for state in descriptor.states() {
        if state.entry().unit() != unit.unit() || unit.block(state.entry()).is_none() {
            return None;
        }

        for storage in state.initialized_storages() {
            if storage.unit() != unit.unit() || unit.storage(*storage).is_none() {
                return None;
            }
        }
    }

    Some(())
}

fn validate_value_definitions(unit: &MirUnit) -> Option<()> {
    for (index, value) in unit.values().iter().enumerate() {
        let id = MirValueId::from_slot(unit.unit(), compact_slot(index)?);

        match value.origin() {
            MirValueOrigin::BlockParameter(block) => {
                let Some(block) = unit.block(block) else {
                    return None;
                };

                if !block.parameters().contains(&id) {
                    return None;
                }
            }
            MirValueOrigin::Operation(operation) => {
                let Some(operation) = unit.operation(operation) else {
                    return None;
                };

                if operation.result() != Some(id) {
                    return None;
                }
            }
        }
    }

    Some(())
}

fn validate_blocks(unit: &MirUnit) -> Option<()> {
    let mut seen_operations = vec![false; unit.operations().len()];

    for (index, block) in unit.blocks().iter().enumerate() {
        let id = MirBlockId::from_slot(unit.unit(), compact_slot(index)?);

        validate_block_parameters(unit, block)?;

        for operation in block.operations() {
            let operation_id = *operation;

            let Some(operation_index) = local_index(unit, operation.unit(), operation.to_index())
            else {
                return None;
            };

            let Some(seen) = seen_operations.get_mut(operation_index) else {
                return None;
            };

            if *seen {
                return None;
            }

            *seen = true;

            let Some(operation) = unit.operations().get(operation_index) else {
                return None;
            };

            validate_operation(unit, id, block.kind(), operation_id, operation)?;
        }

        validate_terminator(unit, id, block)?;
    }

    for seen in seen_operations {
        if !seen {
            return None;
        }
    }

    Some(())
}

fn validate_block_parameters(unit: &MirUnit, block: &MirBlock) -> Option<()> {
    for parameter in block.parameters() {
        super::operation::validate_value(unit, *parameter)?;
    }

    Some(())
}

fn local_index(unit: &MirUnit, owner: crate::MirUnitId, index: Option<usize>) -> Option<usize> {
    (owner == unit.unit()).then_some(index).flatten()
}

fn compact_slot(index: usize) -> Option<u32> {
    crate::id::compact_slot(index)
}

pub(super) fn validate_frame_state(unit: &MirUnit, state: crate::MirFrameStateId) -> Option<()> {
    let Some(descriptor) = unit.frame_descriptor() else {
        return None;
    };

    if !descriptor
        .states()
        .iter()
        .any(|candidate| candidate.state() == state)
    {
        return None;
    }

    Some(())
}

pub(super) fn validate_runtime_role(
    unit: &MirUnit,
    runtime: crate::MirRuntimeReference,
    expected: bray_runtime_interface::RuntimeAbiRole,
) -> Option<()> {
    if runtime.abi_version() != unit.target().runtime_abi() {
        return None;
    }

    if runtime.role() != expected {
        return None;
    }

    Some(())
}
