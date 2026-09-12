use bray_bound_tree::{AnyBoundNodeId, StorageAccessId, StorageCleanupPart, StoragePlan};
use bray_diagnostics::DiagnosticBag;

use crate::asynchronous::{ExecutionCleanupMode, execution_cleanup_dependencies};
use crate::execution_guarantees::{ExecutionDependency, ExecutionProperty};
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

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
