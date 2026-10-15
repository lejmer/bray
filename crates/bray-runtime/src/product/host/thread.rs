use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeProductHostObservation, NativeProductHostStatus, NativeProductIdentity,
};

use super::super::cleanup::run_static_cleanup;

use super::model::{THREAD_STATICS, ThreadStaticEntry, product_hosts};
use super::operations::{observation_with_status, release_thread_attachment, report_incidents};

pub(super) extern "C" fn thread_exit_cleanup(identity: u64) -> u32 {
    catch_unwind(AssertUnwindSafe(|| {
        crate::bootstrap::with_thread_cleanup(identity, drain_thread_statics);
    }))
    .map_or(1, |()| 0)
}

fn drain_thread_statics() {
    let (mut entries, products) = THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();
        let entries = std::mem::take(&mut registry.entries);
        let products = std::mem::take(&mut registry.products);

        registry.callback_registered = false;

        (entries, products)
    });

    entries.sort_unstable_by_key(|entry| (entry.product_identity, entry.product, entry.order));

    // Retirement already supplies the exact-thread cleanup borrow. No new attachment is needed.
    assert!(
        run_thread_cleanups(entries).is_empty(),
        "retained terminal cleanup must reuse its established attachment"
    );

    for (product, attachment) in products {
        if attachment.acquired {
            let _ = release_thread_attachment(product, attachment.worker);
        }
    }
}

pub(crate) fn drain_product_thread_statics(product: usize) -> Option<NativeProductHostObservation> {
    let (mut entries, attachment) = THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();

        let selected = registry
            .entries
            .extract_if(.., |entry| entry.product == product)
            .collect::<Vec<_>>();

        let attachment = registry.products.remove(&product);

        (selected, attachment)
    });

    entries.sort_unstable_by_key(|entry| entry.order);

    let pending = run_thread_cleanups(entries);

    if !pending.is_empty() {
        THREAD_STATICS.with_borrow_mut(|registry| {
            registry.entries.extend(pending);

            if let Some(attachment) = attachment {
                registry.products.insert(product, attachment);
            }
        });

        return Some(observation_with_status(
            product,
            NativeProductHostStatus::RUNTIME_FAILURE,
        ));
    }

    if let Some(attachment) = attachment
        && attachment.acquired
    {
        return Some(release_thread_attachment(product, attachment.worker));
    }

    None
}

fn run_thread_cleanups(entries: Vec<ThreadStaticEntry>) -> Vec<ThreadStaticEntry> {
    let mut products = BTreeMap::<(NativeProductIdentity, usize), Vec<ThreadStaticEntry>>::new();

    for entry in entries {
        products
            .entry((entry.product_identity, entry.product))
            .or_default()
            .push(entry);
    }

    let mut pending = Vec::new();

    for ((_, product), entries) in products {
        if let Err(entries) = run_product_thread_cleanups(product, entries) {
            pending.extend(entries);
        }
    }

    pending
}

fn run_product_thread_cleanups(
    product: usize,
    mut entries: Vec<ThreadStaticEntry>,
) -> Result<(), Vec<ThreadStaticEntry>> {
    let identity = entries
        .first()
        .expect("grouped cleanup contains a static owner")
        .static_identity;

    // Cleanup runs outside the registry lock while retaining the selected runtime.
    let runtime = product_hosts()
        .lock()
        .expect("thread cleanup registry remains available")
        .get(&product)
        .expect("thread cleanup retains its product host")
        .runtime
        .clone();

    let cleanup = || {
        for entry in &mut entries {
            let incidents = run_static_cleanup(
                &mut entry.admission,
                entry.prepare,
                entry.finalizer,
                entry.destroy,
                entry.detach,
            );

            let count = incidents.iter().flatten().count();

            for incident in incidents.into_iter().flatten() {
                let _ = incident.report();
            }

            report_incidents(entry.product, entry.static_identity, count);
        }
    };

    let (_, runtime_incidents) =
        match crate::native::with_static_cleanup_context(runtime.as_ref(), cleanup) {
            Ok(result) => result,
            Err(_) => return Err(entries),
        };

    let count = usize::from(runtime_incidents.is_some());

    if let Some(incident) = runtime_incidents {
        let _ = incident.report();
    }

    report_incidents(product, identity, count);

    Ok(())
}
