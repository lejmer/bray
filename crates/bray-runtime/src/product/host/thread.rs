use bray_runtime_abi::NativeProductHostObservation;

use super::attachment::ThreadProductAttachment;

use super::model::{THREAD_STATICS, product_hosts};
use super::operations::{release_thread_attachment, report_incidents};

pub(super) extern "C-unwind" fn drain_thread_statics() {
    let mut products = THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();
        registry.callback_registered = false;

        std::mem::take(&mut registry.products)
    });

    products.sort_unstable_by_key(|attachment| (attachment.product_identity, attachment.product));

    products.retain_mut(|attachment| run_product_thread_cleanups(attachment).is_ok());

    products.sort_unstable_by_key(|attachment| attachment.product);

    // Keep every product attachment alive until all exact-thread finalizers have run.
    for attachment in products {
        let _ = release_thread_attachment(attachment.product, attachment.worker);
    }
}

pub(crate) fn drain_product_thread_statics(product: usize) -> Option<NativeProductHostObservation> {
    let mut attachment =
        THREAD_STATICS.with(|registry| registry.borrow_mut().remove_product(product))?;

    if let Err(observation) = run_product_thread_cleanups(&mut attachment) {
        return Some(observation);
    }

    Some(release_thread_attachment(product, attachment.worker))
}

fn run_product_thread_cleanups(
    attachment: &mut ThreadProductAttachment,
) -> Result<(), NativeProductHostObservation> {
    let product = attachment.product;
    attachment.entries.sort_unstable_by_key(|entry| entry.order);

    // Keep execution alive while callbacks run outside the registry lock.
    let execution = product_hosts()
        .lock()
        .ok()
        .and_then(|hosts| hosts.get(&product).and_then(|host| host.execution.clone()));

    let execution = execution.as_ref().map(|owner| owner.as_ref().as_ref());

    let result = super::cleanup::run_cleanup_batch(
        product,
        std::mem::take(&mut attachment.entries),
        attachment.cleanup_driver.take(),
        execution,
        report_incidents,
    );

    if let Err(status) = result {
        let Ok(mut hosts) = product_hosts().lock() else {
            return Err(NativeProductHostObservation::invalid());
        };

        let Some(host) = hosts.get_mut(&product) else {
            return Err(NativeProductHostObservation::invalid());
        };

        // Keep the failed attachment obligation until mandatory execution can be resolved.
        host.state = bray_runtime_abi::NativeProductHostState::FAILED;

        return Err(host.observation(super::model::host_status(status)));
    }

    Ok(())
}
