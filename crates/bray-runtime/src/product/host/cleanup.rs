use bray_runtime_abi::{NativeRuntimeStatus, NativeStaticIdentity};

use crate::product::{ProductCleanup, ProductExecution, StaticCleanup};

/// Runs an owned batch with its admitted execution and reports incidents outside host locks.
pub(super) fn run_cleanup_batch(
    product: usize,
    mut entries: Vec<StaticCleanup>,
    mut driver: Option<Box<dyn ProductCleanup>>,
    execution: Option<&dyn ProductExecution>,
    completed: fn(usize, NativeStaticIdentity, usize),
) -> Result<(), NativeRuntimeStatus> {
    let identity = entries
        .first()
        .map_or(NativeStaticIdentity::new([0; 32]), |entry| entry.identity);

    let mut result = Ok(());

    let mut cleanup = || {
        let entries = std::mem::take(&mut entries);

        if let Some(driver) = driver.take() {
            result = driver.run(product, entries, completed);
        } else {
            for entry in entries {
                completed(product, entry.identity, entry.report());
            }
        }
    };

    let incidents = match execution {
        Some(execution) => execution.with_cleanup(&mut cleanup),
        None => {
            cleanup();

            Vec::new()
        }
    };

    let count = incidents.len();

    for incident in incidents {
        let _ = incident.report();
    }

    super::operations::report_incidents(product, identity, count);

    result
}
