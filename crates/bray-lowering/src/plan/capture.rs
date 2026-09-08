use std::collections::BTreeSet;

use bray_bound_tree::{BoundUnit, CheckedAsync, CheckedDependencyContracts, StoragePlan};
use bray_symbols::CallableExecution;

use super::{LoweringPlanFailure, LoweringPlanFailureCause};

pub(super) fn verify_capture_cleanup(
    unit: &BoundUnit,
    storage: &StoragePlan,
    dependencies: &CheckedDependencyContracts,
    analysis: &CheckedAsync,
) -> Result<(), LoweringPlanFailure> {
    let Some(cleanup) = analysis.capture_cleanup() else {
        return Ok(());
    };

    let fail = LoweringPlanFailure::capture_cleanup;

    if cleanup.is_recovered() {
        return Err(fail(LoweringPlanFailureCause::Recovered, None));
    }

    let expected = storage
        .identity_entries()
        .rev()
        .filter(|(_, record)| record.is_parameter())
        .map(|(identity, _)| {
            storage.root_access(identity).ok_or_else(|| {
                LoweringPlanFailure::storage_requirement(
                    LoweringPlanFailureCause::Missing,
                    identity,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let actual = cleanup.captures().iter().copied().collect::<BTreeSet<_>>();

    if actual.len() != cleanup.captures().len() {
        return Err(fail(LoweringPlanFailureCause::Duplicate, None));
    }

    if let Some(access) = expected.iter().find(|access| !actual.contains(access)) {
        return Err(fail(LoweringPlanFailureCause::Missing, Some(*access)));
    }

    if let Some(access) = actual.iter().find(|access| !expected.contains(access)) {
        return Err(fail(LoweringPlanFailureCause::Unexpected, Some(*access)));
    }

    let ordered = dependencies
        .lifecycle_order(unit, storage, &expected)
        .map_err(|access| fail(LoweringPlanFailureCause::Contradictory, Some(access)))?;

    if ordered != cleanup.captures() {
        return Err(fail(LoweringPlanFailureCause::OutOfOrder, None));
    }

    let mut execution = Some(CallableExecution::Synchronous);

    for access in cleanup.captures() {
        let ty = storage
            .root_identity(*access)
            .and_then(|identity| storage.storage_type(identity))
            .ok_or_else(|| fail(LoweringPlanFailureCause::Missing, Some(*access)))?;

        let shape = analysis
            .cleanup_types()
            .iter()
            .find(|shape| shape.ty() == ty)
            .ok_or_else(|| fail(LoweringPlanFailureCause::Missing, Some(*access)))?;

        execution = execution
            .zip(shape.lifecycle_execution())
            .map(|(current, required)| current.max(required));
    }

    if execution != cleanup.execution() {
        return Err(fail(LoweringPlanFailureCause::Contradictory, None));
    }

    Ok(())
}
