use bray_bound_tree::{AnyBoundNodeId, StorageAccessId, StorageCleanupPart, StoragePlan};
use bray_diagnostics::DiagnosticBag;

use crate::asynchronous::{ExecutionCleanupMode, execution_cleanup_dependencies};
use crate::execution_guarantees::{ExecutionDependency, ExecutionProperty};
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

#[expect(
    clippy::too_many_arguments,
    reason = "scope cleanup proof retains its checked location, property, and diagnostics"
)]
pub(super) fn check_scope_cleanup<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    cleanup: &bray_bound_tree::CheckedAsync,
    scope: bray_bound_tree::BoundBlockId,
    exit: AnyBoundNodeId,
    phase: super::super::model::AnalysisScopeExitPhase,
    property: ExecutionProperty,
    dependencies: &mut Vec<ExecutionDependency>,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
    let mut valid = true;

    for plan in cleanup
        .scope_exits()
        .iter()
        .filter(|plan| plan.scope() == scope && plan.exit() == exit)
    {
        valid &= !plan.is_recovered() && plan.cancellation_broadcast().is_empty();

        if phase == super::super::model::AnalysisScopeExitPhase::LifecycleResolution {
            for access in plan.lifecycle_resolution() {
                let identity = storage.root_identity(*access);

                let parts = cleanup
                    .storage_requirements()
                    .iter()
                    .find(|requirement| Some(requirement.identity()) == identity)
                    .and_then(|requirement| requirement.parts());

                valid &= check_cleanup(
                    request,
                    storage,
                    *access,
                    parts,
                    property,
                    exit,
                    dependencies,
                    diagnostics,
                )?;
            }
        }
    }

    Ok(valid)
}

pub(super) fn check_cleanup<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    access: StorageAccessId,
    parts: Option<&[StorageCleanupPart]>,
    property: ExecutionProperty,
    node: AnyBoundNodeId,
    dependencies: &mut Vec<ExecutionDependency>,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
    let Some(access) = storage.access(access) else {
        return Ok(false);
    };

    let mut valid = true;

    let types = match parts {
        None => vec![access.reached_type()],
        Some(parts) => {
            valid &= parts
                .iter()
                .all(|part| part.release().is_none() && !part.phases().includes_cancellation());

            parts
                .iter()
                .filter(|part| part.phases().includes_lifecycle())
                .map(|part| {
                    part.projections()
                        .last()
                        .map_or(access.reached_type(), |projection| projection.result_type())
                })
                .collect()
        }
    };

    for ty in types {
        let checked = execution_cleanup_dependencies(
            request,
            ty,
            property,
            ExecutionCleanupMode::Disposal,
            node,
        )?;

        let (proof, owned) = checked.into_parts();

        diagnostics.add_range(owned);

        match proof {
            Some(proof) => dependencies.extend(proof),
            None => valid = false,
        }
    }

    Ok(valid)
}
