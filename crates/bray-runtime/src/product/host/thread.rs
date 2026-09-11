use bray_runtime_abi::NativeProductHostObservation;

use super::super::cleanup::StaticCleanup;

use super::model::{THREAD_STATICS, product_hosts};
use super::operations::{release_thread_attachment, report_incidents};

pub(super) extern "C-unwind" fn drain_thread_statics() {
    let mut products = THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();
        registry.callback_registered = false;

        std::mem::take(&mut registry.products)
    });

    products.sort_unstable_by_key(|attachment| (attachment.product_identity, attachment.product));

    for attachment in &mut products {
        attachment.entries.sort_unstable_by_key(|entry| entry.order);
        run_product_thread_cleanups(attachment.product, std::mem::take(&mut attachment.entries));
    }

    products.sort_unstable_by_key(|attachment| attachment.product);

    // Keep every product attachment alive until all exact-thread finalizers have run.
    for attachment in products {
        if attachment.acquired {
            let _ = release_thread_attachment(attachment.product, attachment.worker);
        }
    }
}

pub(crate) fn drain_product_thread_statics(product: usize) -> Option<NativeProductHostObservation> {
    let mut attachment =
        THREAD_STATICS.with(|registry| registry.borrow_mut().remove_product(product))?;

    attachment.entries.sort_unstable_by_key(|entry| entry.order);
    run_product_thread_cleanups(product, attachment.entries);

    if attachment.acquired {
        return Some(release_thread_attachment(product, attachment.worker));
    }

    None
}

fn run_product_thread_cleanups(product: usize, entries: Vec<StaticCleanup>) {
    let Some(owner) = entries.first().copied() else {
        return;
    };

    // Keep execution alive while callbacks run outside the registry lock.
    let execution = product_hosts()
        .lock()
        .ok()
        .and_then(|hosts| hosts.get(&product).and_then(|host| host.execution.clone()));

    let execution = execution.as_ref().map(|owner| owner.as_ref().as_ref());

    let mut cleanup = || {
        for entry in &entries {
            let count = entry.report(execution);

            report_incidents(product, entry.identity, count);
        }
    };

    let runtime_incidents = match execution {
        Some(execution) => execution.with_cleanup(&mut cleanup),
        None => {
            cleanup();

            Vec::new()
        }
    };

    let count = runtime_incidents.len();

    for incident in runtime_incidents {
        let _ = incident.report();
    }

    report_incidents(product, owner.identity, count);
}
