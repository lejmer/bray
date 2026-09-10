use std::cell::RefCell;
use std::collections::HashMap;

use bray_runtime_abi::{NativeProductHostObservation, NativeProductHostStatus};

use super::host::{
    acquire_thread_attachment, discard_thread_attachment, drain_product_thread_statics,
    initialize_thread_static_registry, mark_thread_attachment_acquired, observation_with_status,
    prepare_thread_attachment,
};

thread_local! {
    static FOREIGN_ATTACHMENTS: RefCell<ForeignAttachments> =
        RefCell::new(ForeignAttachments::new());
}

struct ForeignAttachments {
    scope: Option<bray_platform::RuntimeThreadScope>,
    products: HashMap<usize, usize>,
}

impl ForeignAttachments {
    fn new() -> Self {
        Self {
            scope: None,
            products: HashMap::new(),
        }
    }
}

pub(super) fn attach_current_thread(product: usize) -> NativeProductHostObservation {
    // The attachment owns the runtime scope whose destructor drains thread statics. Initialize the
    // registry first so it remains alive until that scope has finished during native thread exit.
    initialize_thread_static_registry();

    let inserted = FOREIGN_ATTACHMENTS.with(|attachments| {
        let mut attachments = attachments.borrow_mut();

        if let Some(depth) = attachments.products.get_mut(&product) {
            let Some(next) = depth.checked_add(1) else {
                return Err(NativeProductHostStatus::INVALID_ARGUMENT);
            };

            *depth = next;

            return Ok(false);
        }

        crate::allocation::reserve_map_entries(&mut attachments.products, 1)
            .map_err(|_| NativeProductHostStatus::ALLOCATION_FAILURE)?;

        if bray_platform::current_runtime_thread().is_none() && attachments.scope.is_none() {
            let scope = bray_platform::RuntimeThreadScope::enter().map_err(|error| {
                super::host::host_status(crate::native::thread_attachment_status(error))
            })?;

            attachments.scope = Some(scope);
        }

        attachments.products.insert(product, 1);

        Ok(true)
    });

    let inserted = match inserted {
        Ok(inserted) => inserted,
        Err(status) => return observation_with_status(product, status),
    };

    if !inserted {
        return observation_with_status(product, NativeProductHostStatus::SUCCESS);
    }

    let already_acquired = match prepare_thread_attachment(product) {
        Ok(acquired) => acquired,
        Err(status) => {
            rollback(product);

            return observation_with_status(product, status);
        }
    };

    if !already_acquired {
        let observation = acquire_thread_attachment(product, false);

        if observation.status() != NativeProductHostStatus::SUCCESS {
            discard_thread_attachment(product);
            rollback(product);

            return observation;
        }

        mark_thread_attachment_acquired(product);
    }

    observation_with_status(product, NativeProductHostStatus::SUCCESS)
}

pub(super) fn has_foreign_attachment(product: usize) -> bool {
    FOREIGN_ATTACHMENTS.with(|attachments| attachments.borrow().products.contains_key(&product))
}

pub(super) fn detach_current_thread(product: usize) -> NativeProductHostObservation {
    let detached = FOREIGN_ATTACHMENTS.with(|attachments| {
        let mut attachments = attachments.borrow_mut();

        let Some(depth) = attachments.products.get_mut(&product) else {
            return None;
        };

        *depth = depth.checked_sub(1)?;

        if *depth != 0 {
            return Some((false, None));
        }

        attachments.products.remove(&product);

        let scope = attachments
            .products
            .is_empty()
            .then(|| attachments.scope.take())
            .flatten();

        Some((true, scope))
    });

    let Some((outermost, scope)) = detached else {
        return observation_with_status(product, NativeProductHostStatus::INVALID_ARGUMENT);
    };

    let observation = outermost
        .then(|| drain_product_thread_statics(product))
        .flatten();

    drop(scope);

    observation
        .unwrap_or_else(|| observation_with_status(product, NativeProductHostStatus::SUCCESS))
}

fn rollback(product: usize) {
    let scope = FOREIGN_ATTACHMENTS.with(|attachments| {
        let mut attachments = attachments.borrow_mut();

        attachments.products.remove(&product);

        attachments
            .products
            .is_empty()
            .then(|| attachments.scope.take())
            .flatten()
    });

    drop(scope);
}
