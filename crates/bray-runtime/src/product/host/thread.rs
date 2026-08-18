use std::collections::BTreeMap;

use bray_runtime_abi::{NativeProductHostObservation, NativeProductIdentity};

use super::super::cleanup::run_static_cleanup;

use super::model::{THREAD_STATICS, ThreadStaticEntry, product_hosts};
use super::operations::{release_thread_attachment, report_incidents};

pub(super) extern "C-unwind" fn drain_thread_statics() {
    let (mut entries, products) = THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();
        let entries = std::mem::take(&mut registry.entries);
        let products = std::mem::take(&mut registry.products);

        registry.callback_registered = false;

        (entries, products)
    });

    entries.sort_unstable_by_key(|entry| (entry.product_identity, entry.product, entry.order));

    run_thread_cleanups(entries);

    for (product, attachment) in products {
        if attachment.acquired {
            let _ = release_attachment(product, attachment.worker);
        }
    }
}

pub(crate) fn drain_product_thread_statics(
    product: usize,
) -> Option<NativeProductHostObservation> {
    let (mut entries, attachment) = THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();
        let mut selected = Vec::new();

        registry.entries.retain(|entry| {
            if entry.product == product {
                selected.push(*entry);

                false
            } else {
                true
            }
        });

        let attachment = registry.products.remove(&product);

        (selected, attachment)
    });

    entries.sort_unstable_by_key(|entry| entry.order);

    run_thread_cleanups(entries);

    if let Some(attachment) = attachment
        && attachment.acquired
    {
        return release_attachment(product, attachment.worker);
    }

    None
}

fn run_thread_cleanups(entries: Vec<ThreadStaticEntry>) {
    let mut products = BTreeMap::<(NativeProductIdentity, usize), Vec<ThreadStaticEntry>>::new();

    for entry in entries {
        products
            .entry((entry.product_identity, entry.product))
            .or_default()
            .push(entry);
    }

    for ((_, product), entries) in products {
        run_product_thread_cleanups(product, entries);
    }
}

fn run_product_thread_cleanups(product: usize, entries: Vec<ThreadStaticEntry>) {
    let owner = entries.first().copied();

    let runtime = product_hosts()
        .lock()
        .ok()
        .and_then(|hosts| hosts.get(&product).map(|host| host.runtime.clone()));

    let cleanup = || {
        for entry in entries {
            let incidents =
                run_static_cleanup(entry.prepare, entry.finalizer, entry.destroy, entry.detach);

            let count = incidents.len();

            for incident in incidents {
                let _ = incident.report();
            }

            report_incidents(entry.product, entry.static_identity, count);
        }
    };

    let (_, runtime_incidents) = match runtime.as_ref() {
        Some(runtime) => crate::native::with_retained_static_cleanup_runtime(runtime, cleanup),
        None => crate::native::with_static_cleanup_runtime(cleanup),
    };

    let Some(owner) = owner else {
        return;
    };

    let count = runtime_incidents.len();

    for incident in runtime_incidents {
        let _ = incident.report();
    }

    report_incidents(owner.product, owner.static_identity, count);
}

fn release_attachment(
    product: usize,
    worker: bool,
) -> Option<NativeProductHostObservation> {
    Some(release_thread_attachment(product, worker))
}
