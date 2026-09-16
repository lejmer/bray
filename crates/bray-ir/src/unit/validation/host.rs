use bray_bound_tree::BoundUnitKind;
use bray_runtime_interface::RuntimeAbiRole;

use crate::{MirHostOperation, MirUnit};

use super::core::validate_runtime_role;

pub(super) fn validate_host_operation(
    unit: &MirUnit,
    host_operation: &MirHostOperation,
) -> Option<()> {
    let crate::MirUnitKind::ExecutableHost(host) = unit.kind() else {
        return None;
    };

    match host_operation {
        MirHostOperation::MaterializeStatic { place } => {
            if !matches!(
                unit.storage(place.storage()).map(crate::MirStorage::kind),
                Some(crate::MirStorageKind::Static(_))
            ) {
                return None;
            }

            Some(())
        }
        MirHostOperation::SelectTestEntry { entry, runtime } => {
            if host.entry(*entry).is_none()
                || !host
                    .requirements()
                    .requires_role(RuntimeAbiRole::TestEntrySelection)
            {
                return None;
            }

            validate_runtime_role(unit, *runtime, RuntimeAbiRole::TestEntrySelection)
        }
        MirHostOperation::ExecuteRoot {
            entry,
            root,
            execution,
            runtime,
        } => {
            let Some(contract_entry) = host.entry(*entry) else {
                return None;
            };

            if root.kind() != BoundUnitKind::CallableBody || *execution != contract_entry.root() {
                return None;
            }

            let role = if *execution == bray_runtime_interface::RootExecution::Synchronous
                && host
                    .requirements()
                    .requires_role(RuntimeAbiRole::SynchronousRootExecution)
            {
                RuntimeAbiRole::SynchronousRootExecution
            } else {
                RuntimeAbiRole::RootExecution
            };

            validate_runtime_role(unit, *runtime, role)
        }
        MirHostOperation::ObserveRootTerminal { entry, runtime } => {
            if host.entry(*entry).is_none() {
                return None;
            }

            validate_runtime_role(unit, *runtime, RuntimeAbiRole::RootTerminalObservation)
        }
        MirHostOperation::ResolveRootTerminal {
            entry,
            error,
            completion,
            panic,
            entry_failure,
        } => {
            let Some(contract_entry) = host.entry(*entry) else {
                return None;
            };

            let expected_error = match contract_entry.result() {
                bray_runtime_interface::ExecutableEntryResult::Fallible { error, .. } => {
                    Some(error)
                }
                bray_runtime_interface::ExecutableEntryResult::Unit
                | bray_runtime_interface::ExecutableEntryResult::I32 => None,
            };

            if *error != expected_error {
                return None;
            }

            validate_runtime_role(unit, *completion, RuntimeAbiRole::RootCompletionResolution)?;
            validate_runtime_role(unit, *panic, RuntimeAbiRole::PanicReporting)?;

            validate_runtime_role(unit, *entry_failure, RuntimeAbiRole::EntryFailureReporting)
        }
        MirHostOperation::BeginStaticCleanup => Some(()),
        MirHostOperation::ReportCleanupIncidents { runtime } => {
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::CleanupIncidentReporting)
        }
        MirHostOperation::StructuredShutdown { runtime } => {
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::StructuredShutdown)
        }
    }
}
