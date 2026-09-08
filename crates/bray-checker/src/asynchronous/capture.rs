use bray_bound_tree::{AsyncCaptureCleanup, CheckedDependencyContracts, StoragePlan};
use bray_symbols::CallableExecution;

use super::cleanup::CleanupShapeResolver;
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

pub(super) fn capture_cleanup<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    dependencies: &CheckedDependencyContracts,
    shapes: &mut CleanupShapeResolver<'_, C>,
) -> Result<AsyncCaptureCleanup, CheckerQueryError<C::UpstreamError>> {
    let mut captures = Vec::new();
    let mut recovered = false;

    // Entry ownership precedes every move or partial state established by the body.
    for (identity, record) in storage.identity_entries().rev() {
        if !record.is_parameter() {
            continue;
        }

        let (Some(access), Some(ty)) = (
            storage.root_access(identity),
            storage.storage_type(identity),
        ) else {
            recovered = true;
            continue;
        };

        let shape = shapes.resolve(ty)?;

        recovered |= shape.recovered;

        captures.push(access);
    }

    let captures = match dependencies.lifecycle_order(request.unit(), storage, &captures) {
        Ok(captures) => captures,
        Err(_) => {
            recovered = true;

            captures
        }
    };

    shapes.resolve_execution()?;

    let mut execution = Some(CallableExecution::Synchronous);

    for access in &captures {
        let cleanup = storage
            .root_identity(*access)
            .and_then(|identity| storage.storage_type(identity))
            .and_then(|ty| shapes.cleanup_types.get(&ty));

        execution = match (execution, cleanup) {
            (Some(execution), Some(cleanup)) => cleanup
                .lifecycle_execution()
                .map(|mode| execution.max(mode)),
            _ => None,
        };
    }

    Ok(AsyncCaptureCleanup::new(captures, execution, recovered))
}
