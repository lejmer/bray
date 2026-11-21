use std::collections::BTreeMap;

use bray_bound_tree::{AnyBoundNodeId, StorageAccessId, StorageCleanupPart, StoragePlan};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{TypeAssociatedLifecycleSlot, TypeId};

use crate::asynchronous::{ExecutionCleanupMode, execution_cleanup_dependencies};
use crate::execution_guarantees::{ExecutionDependency, ExecutionProperty};
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

#[derive(Default)]
pub(in crate::analysis::guarantee) struct ReachedScopeCleanup {
    pub(in crate::analysis::guarantee) cancels_tasks: bool,
    pub(in crate::analysis::guarantee) parts: BTreeMap<(StorageAccessId, TypeId, bool), bool>,
    pub(in crate::analysis::guarantee) calls:
        BTreeMap<(StorageAccessId, usize, TypeAssociatedLifecycleSlot), Option<ExecutionDependency>>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "scope cleanup proof retains its checked location, property, and diagnostics"
)]
pub(in crate::analysis::guarantee) fn check_scope_cleanup<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    flow: &bray_bound_tree::StorageFlow,
    scope: bray_bound_tree::StorageScope,
    exit: AnyBoundNodeId,
    reached: Option<&ReachedScopeCleanup>,
    property: ExecutionProperty,
    inert: impl Fn(StorageAccessId) -> bool,
    dependencies: &mut Vec<ExecutionDependency>,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
    let mut valid = flow.exit(bray_bound_tree::StorageExitPoint::new(scope, exit)).is_none_or(|plan| !plan.is_recovered());

    let Some(reached) = reached else {
        return Ok(valid);
    };

    valid &= !reached.cancels_tasks;

    for (&(access, _, _), &dependency) in &reached.calls {
        if !inert(access) {
            valid &= dependency.is_some();
            dependencies.extend(dependency);
        }
    }

    for (&(access, ty, releases_storage), &preserves_inputs) in &reached.parts {
        if !(preserves_inputs || inert(access)) {
            valid &= !releases_storage;
            valid &= check_cleanup_type(request, ty, property, exit, dependencies, diagnostics)?;
        }
    }

    Ok(valid)
}

#[expect(
    clippy::too_many_arguments,
    reason = "cleanup certification retains the exact storage access, execution property and proof outputs"
)]
pub(in crate::analysis::guarantee) fn check_cleanup<C: CheckerRequestContext + ?Sized>(
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
        valid &= check_cleanup_type(request, ty, property, node, dependencies, diagnostics)?;
    }

    Ok(valid)
}

fn check_cleanup_type<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    property: ExecutionProperty,
    node: AnyBoundNodeId,
    dependencies: &mut Vec<ExecutionDependency>,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
    let checked = execution_cleanup_dependencies(request, ty, property, ExecutionCleanupMode::Disposal, node)?;

    let (proof, owned) = checked.into_parts();

    diagnostics.add_range(owned);

    let Some(proof) = proof else {
        return Ok(false);
    };

    dependencies.extend(proof);

    Ok(true)
}
