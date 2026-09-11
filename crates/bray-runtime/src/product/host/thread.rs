use bray_runtime_abi::NativeProductHostObservation;

use super::super::cleanup::report_static_cleanup;

use super::model::{THREAD_STATICS, ThreadStaticEntry, product_hosts};
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

fn run_product_thread_cleanups(product: usize, entries: Vec<ThreadStaticEntry>) {
    let Some(owner) = entries.first().copied() else {
        return;
    };

    let runtime = product_hosts()
        .lock()
        .ok()
        .and_then(|hosts| hosts.get(&product).map(|host| host.runtime.clone()));

    let cleanup = || {
        for entry in entries {
            let count =
                report_static_cleanup(entry.prepare, entry.finalizer, entry.destroy, entry.detach);

            report_incidents(product, entry.static_identity, count);
        }
    };

    let (_, runtime_incidents) = match runtime.as_ref() {
        Some(runtime) => crate::native::with_retained_static_cleanup_runtime(runtime, cleanup),
        None => crate::native::with_static_cleanup_runtime(cleanup),
    };

    let count = runtime_incidents.len();

    for incident in runtime_incidents {
        let _ = incident.report();
    }

    report_incidents(product, owner.static_identity, count);
}
