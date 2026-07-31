use bray_bound_tree::BoundUnitKind;
use bray_runtime_interface::RuntimeAbiRole;

use crate::{MirHostOperation, MirOperationId, MirUnit, MirUnitBuildError};

use super::core::validate_runtime_role;

pub(super) fn validate_host_operation(
    unit: &MirUnit,
    operation: MirOperationId,
    host_operation: &MirHostOperation,
) -> Result<(), MirUnitBuildError> {
    let crate::MirUnitKind::ExecutableHost(host) = unit.kind() else {
        return Err(MirUnitBuildError::InvalidHostOperation(operation));
    };

    match host_operation {
        MirHostOperation::ExecuteRoot {
            root,
            execution,
            runtime,
        } => {
            if root.kind() != BoundUnitKind::CallableBody || *execution != host.root() {
                return Err(MirUnitBuildError::InvalidHostOperation(operation));
            }

            let role = if *execution == bray_runtime_interface::RootExecution::Synchronous
                && host
                    .requirements()
                    .roles()
                    .contains(&RuntimeAbiRole::SynchronousRootExecution)
            {
                RuntimeAbiRole::SynchronousRootExecution
            } else {
                RuntimeAbiRole::RootExecution
            };

            validate_runtime_role(unit, *runtime, role)
        }
        MirHostOperation::ObserveRootTerminal { runtime } => {
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::RootTerminalObservation)
        }
        MirHostOperation::ResolveRootTerminal {
            error,
            completion,
            panic,
            entry_failure,
        } => {
            let expected_error = match host.entry_result() {
                bray_runtime_interface::ExecutableEntryResult::Fallible { error, .. } => {
                    Some(error)
                }
                bray_runtime_interface::ExecutableEntryResult::Unit
                | bray_runtime_interface::ExecutableEntryResult::I32 => None,
            };

            if *error != expected_error {
                return Err(MirUnitBuildError::InvalidHostOperation(operation));
            }

            validate_runtime_role(unit, *completion, RuntimeAbiRole::RootCompletionResolution)?;

            validate_runtime_role(unit, *panic, RuntimeAbiRole::PanicReporting)?;

            validate_runtime_role(unit, *entry_failure, RuntimeAbiRole::EntryFailureReporting)
        }
        MirHostOperation::ReportCleanupIncidents { runtime } => {
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::CleanupIncidentReporting)
        }
        MirHostOperation::StructuredShutdown { runtime } => {
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::StructuredShutdown)
        }
    }
}
