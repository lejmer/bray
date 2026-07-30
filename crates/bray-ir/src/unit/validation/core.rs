use crate::{
    MirBlock, MirBlockId, MirBlockKind, MirOperationId, MirUnit, MirUnitBuildError, MirValueId,
    MirValueOrigin,
};

use super::control::validate_terminator;
use super::operation::validate_operation;

pub(in crate::unit) fn validate_unit(unit: &MirUnit) -> Result<(), MirUnitBuildError> {
    let Some(entry) = unit.block(unit.entry()) else {
        return Err(MirUnitBuildError::MissingBlock(unit.entry()));
    };

    if entry.kind() != MirBlockKind::Ordinary {
        return Err(MirUnitBuildError::CleanupPhaseOrderViolation(unit.entry()));
    }

    validate_frame_descriptor(unit)?;
    validate_value_definitions(unit)?;
    validate_blocks(unit)?;
    validate_host_sequence(unit)?;

    Ok(())
}

fn validate_host_sequence(unit: &MirUnit) -> Result<(), MirUnitBuildError> {
    if !matches!(unit.kind(), crate::MirUnitKind::ExecutableHost(_)) {
        return Ok(());
    }

    let operations = unit
        .operations()
        .iter()
        .map(crate::MirOperation::kind)
        .collect::<Vec<_>>();

    if !matches!(
        operations.as_slice(),
        [
            crate::MirOperationKind::Host(crate::MirHostOperation::ExecuteRoot { .. }),
            crate::MirOperationKind::Host(
                crate::MirHostOperation::RequestRootCancellation { .. }
            ),
            crate::MirOperationKind::Host(
                crate::MirHostOperation::ObserveRootTerminal { .. }
            ),
            crate::MirOperationKind::Host(
                crate::MirHostOperation::ReportCleanupIncidents { .. }
            ),
            crate::MirOperationKind::Host(crate::MirHostOperation::StructuredShutdown { .. }),
        ]
    ) {
        return Err(MirUnitBuildError::InvalidHostSequence);
    }

    Ok(())
}

fn validate_frame_descriptor(unit: &MirUnit) -> Result<(), MirUnitBuildError> {
    let descriptor = match (unit.kind(), unit.frame_descriptor()) {
        (crate::MirUnitKind::ProtectedAsyncFrame(frame), Some(descriptor)) => {
            if descriptor.frame() != *frame {
                return Err(MirUnitBuildError::ProtectedFrameMismatch);
            }

            if descriptor.abi_version() != unit.target().runtime_abi() {
                return Err(MirUnitBuildError::RuntimeAbiVersionMismatch);
            }

            descriptor
        }
        (crate::MirUnitKind::ProtectedAsyncFrame(_), None) => {
            return Err(MirUnitBuildError::MissingFrameDescriptor);
        }
        (
            crate::MirUnitKind::Synchronous
            | crate::MirUnitKind::ExecutableHost(_)
            | crate::MirUnitKind::GeneratedLifecycle(_),
            Some(_),
        ) => {
            return Err(MirUnitBuildError::UnexpectedFrameDescriptor);
        }
        (crate::MirUnitKind::ExecutableHost(host), None) => {
            if host.abi_version() != unit.target().runtime_abi() {
                return Err(MirUnitBuildError::RuntimeAbiVersionMismatch);
            }

            return Ok(());
        }
        (crate::MirUnitKind::Synchronous | crate::MirUnitKind::GeneratedLifecycle(_), None) => {
            return Ok(());
        }
    };

    for state in descriptor.states() {
        if state.entry().unit() != unit.unit() || unit.block(state.entry()).is_none() {
            return Err(MirUnitBuildError::InvalidFrameStateEntry(state.entry()));
        }

        for storage in state.initialized_storages() {
            if storage.unit() != unit.unit() || unit.storage(*storage).is_none() {
                return Err(MirUnitBuildError::MissingStorage(*storage));
            }
        }
    }

    Ok(())
}

fn validate_value_definitions(unit: &MirUnit) -> Result<(), MirUnitBuildError> {
    for (index, value) in unit.values().iter().enumerate() {
        let id = MirValueId::from_slot(unit.unit(), compact_slot(index)?);

        match value.origin() {
            MirValueOrigin::BlockParameter(block) => {
                let Some(block) = unit.block(block) else {
                    return Err(missing_or_foreign_block(unit, block));
                };

                if !block.parameters().contains(&id) {
                    return Err(MirUnitBuildError::MissingValue(id));
                }
            }
            MirValueOrigin::Operation(operation) => {
                let Some(operation) = unit.operation(operation) else {
                    return Err(MirUnitBuildError::MissingOperation(operation));
                };

                if operation.result() != Some(id) {
                    return Err(MirUnitBuildError::MissingValue(id));
                }
            }
        }
    }

    Ok(())
}

fn validate_blocks(unit: &MirUnit) -> Result<(), MirUnitBuildError> {
    let mut seen_operations = vec![false; unit.operations().len()];

    for (index, block) in unit.blocks().iter().enumerate() {
        let id = MirBlockId::from_slot(unit.unit(), compact_slot(index)?);

        validate_block_parameters(unit, block)?;

        for operation in block.operations() {
            let operation_id = *operation;

            let Some(operation_index) = local_index(unit, operation.unit(), operation.to_index())
            else {
                return Err(missing_or_foreign_operation(unit, operation_id));
            };

            let Some(seen) = seen_operations.get_mut(operation_index) else {
                return Err(MirUnitBuildError::MissingOperation(operation_id));
            };

            if *seen {
                return Err(MirUnitBuildError::MissingOperation(operation_id));
            }

            *seen = true;

            let Some(operation) = unit.operations().get(operation_index) else {
                return Err(MirUnitBuildError::MissingOperation(operation_id));
            };

            validate_operation(unit, id, block.kind(), operation_id, operation)?;
        }

        validate_terminator(unit, id, block)?;
    }

    for (index, seen) in seen_operations.into_iter().enumerate() {
        if !seen {
            return Err(MirUnitBuildError::MissingOperation(
                MirOperationId::from_slot(unit.unit(), compact_slot(index)?),
            ));
        }
    }

    Ok(())
}

fn validate_block_parameters(unit: &MirUnit, block: &MirBlock) -> Result<(), MirUnitBuildError> {
    for parameter in block.parameters() {
        super::operation::validate_value(unit, *parameter)?;
    }

    Ok(())
}

pub(super) fn missing_or_foreign_block(unit: &MirUnit, block: MirBlockId) -> MirUnitBuildError {
    if block.unit() == unit.unit() {
        MirUnitBuildError::MissingBlock(block)
    } else {
        MirUnitBuildError::ForeignBlock {
            expected: unit.unit(),
            actual: block.unit(),
        }
    }
}

fn missing_or_foreign_operation(unit: &MirUnit, operation: MirOperationId) -> MirUnitBuildError {
    if operation.unit() == unit.unit() {
        MirUnitBuildError::MissingOperation(operation)
    } else {
        MirUnitBuildError::ForeignOperation(operation)
    }
}

fn local_index(unit: &MirUnit, owner: crate::MirUnitId, index: Option<usize>) -> Option<usize> {
    (owner == unit.unit()).then_some(index).flatten()
}

fn compact_slot(index: usize) -> Result<u32, MirUnitBuildError> {
    crate::id::compact_slot(index).ok_or(MirUnitBuildError::IdentityCapacityExceeded)
}

pub(super) fn validate_frame_state(
    unit: &MirUnit,
    state: crate::MirFrameStateId,
) -> Result<(), MirUnitBuildError> {
    let Some(descriptor) = unit.frame_descriptor() else {
        return Err(MirUnitBuildError::MissingFrameState);
    };

    if !descriptor
        .states()
        .iter()
        .any(|candidate| candidate.state() == state)
    {
        return Err(MirUnitBuildError::MissingFrameState);
    }

    Ok(())
}

pub(super) fn validate_runtime_role(
    unit: &MirUnit,
    runtime: crate::MirRuntimeReference,
    expected: bray_runtime_interface::RuntimeAbiRole,
) -> Result<(), MirUnitBuildError> {
    if runtime.abi_version() != unit.target().runtime_abi() {
        return Err(MirUnitBuildError::RuntimeAbiVersionMismatch);
    }

    if runtime.role() != expected {
        return Err(MirUnitBuildError::RuntimeRoleMismatch {
            expected,
            actual: runtime.role(),
        });
    }

    Ok(())
}
