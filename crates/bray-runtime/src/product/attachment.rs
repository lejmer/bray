use std::cell::RefCell;
use std::collections::BTreeMap;

use bray_runtime_abi::{NativeProductHostObservation, NativeProductHostStatus};

use super::host::{
    acquire_thread_attachment, discard_thread_attachment, drain_product_thread_statics,
    mark_thread_attachment_acquired, observation_with_status, prepare_thread_attachment,
};

thread_local! {
    static FOREIGN_ATTACHMENTS: RefCell<ForeignAttachments> =
        RefCell::new(ForeignAttachments::new());
}

struct ForeignAttachments {
    scope: Option<crate::bootstrap::RuntimeThreadScope>,
    products: BTreeMap<usize, usize>,
}

impl ForeignAttachments {
    fn new() -> Self {
        // TLS owners retire in reverse initialization order. Establish every context and registry
        // borrowed by attachment cleanup before publishing this owner, including observation-first use.
        crate::native::initialize_thread_context();

        Self {
            scope: None,
            products: BTreeMap::new(),
        }
    }
}

pub(crate) fn initialize_thread_attachment_storage() {
    FOREIGN_ATTACHMENTS.with(|_| {});
}

pub(super) fn attach_current_thread(product: usize) -> NativeProductHostObservation {
    let inserted = FOREIGN_ATTACHMENTS.with(|attachments| {
        let mut attachments = attachments.borrow_mut();

        if let Some(depth) = attachments.products.get_mut(&product) {
            let next = depth.checked_add(1)?;

            *depth = next;

            return Some(false);
        }

        if crate::bootstrap::current_runtime_thread().is_none() && attachments.scope.is_none() {
            let Ok(scope) = crate::bootstrap::RuntimeThreadScope::enter() else {
                return None;
            };

            attachments.scope = Some(scope);
        }

        attachments.products.insert(product, 1);

        Some(true)
    });

    let Some(inserted) = inserted else {
        return observation_with_status(product, NativeProductHostStatus::INVALID_ARGUMENT);
    };

    if !inserted {
        return observation_with_status(product, NativeProductHostStatus::SUCCESS);
    }

    let Some(already_acquired) = prepare_thread_attachment(product) else {
        rollback(product);

        return observation_with_status(product, NativeProductHostStatus::RUNTIME_FAILURE);
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

        let depth = attachments.products.get_mut(&product)?;

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

    if observation
        .as_ref()
        .is_some_and(|observation| observation.status() == NativeProductHostStatus::RUNTIME_FAILURE)
    {
        FOREIGN_ATTACHMENTS.with_borrow_mut(|attachments| {
            attachments.products.insert(product, 1);

            if let Some(scope) = scope {
                assert!(
                    attachments.scope.is_none(),
                    "detached scope remains uniquely owned"
                );

                attachments.scope = Some(scope);
            }
        });

        return observation.expect("failed cleanup retains its observation");
    }

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
