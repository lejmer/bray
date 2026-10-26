use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bray_base::Cancellation;

use super::{
    CancellationStateKind, CapacityResource, FactQueryError, FactRuntimeFailure,
    SynchronizationComponent,
};

/// A shareable cancellation signal for one compiler request.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    state: Arc<CancellationState>,
}

#[derive(Debug)]
enum CancellationState {
    Request(AtomicBool),
    Shared(SharedCancellationState),
}

#[derive(Debug)]
struct SharedCancellationState {
    next_interest: AtomicU64,
    interests: Mutex<BTreeMap<u64, CancellationToken>>,
}

#[derive(Clone, Debug)]
pub(crate) struct SharedCancellation {
    token: CancellationToken,
}

#[derive(Debug)]
pub(crate) struct CancellationInterest {
    state: Arc<CancellationState>,
    identity: u64,
}

impl CancellationToken {
    /// Creates an uncancelled request token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation for every observer of this token.
    pub fn cancel(&self) {
        match &*self.state {
            CancellationState::Request(cancelled) => {
                cancelled.store(true, Ordering::Release);
            }
            CancellationState::Shared(_) => {
                panic!("shared evaluation cancellation is derived from request interest")
            }
        }
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        match &*self.state {
            CancellationState::Request(cancelled) => cancelled.load(Ordering::Acquire),
            CancellationState::Shared(_) => self.is_cancelled_with(&mut Vec::new()),
        }
    }

    fn is_cancelled_with(&self, active: &mut Vec<usize>) -> bool {
        let shared = match &*self.state {
            CancellationState::Request(cancelled) => {
                return cancelled.load(Ordering::Acquire);
            }
            CancellationState::Shared(shared) => shared,
        };

        let identity = Arc::as_ptr(&self.state).addr();

        if active.contains(&identity) {
            panic!("query runtime RecursiveCancellationInterest contract failed");
        }

        active.push(identity);

        let result = self.shared_is_cancelled(shared, active);

        active.pop();

        result
    }

    fn shared_is_cancelled(
        &self,
        shared: &SharedCancellationState,
        active: &mut Vec<usize>,
    ) -> bool {
        let interests = {
            let interests = shared
                .interests
                .lock()
                .expect("shared cancellation interests must remain unpoisoned");

            interests.values().cloned().collect::<Vec<_>>()
        };

        if interests.is_empty() {
            return true;
        }

        for cancellation in interests {
            if !cancellation.is_cancelled_with(active) {
                return false;
            }
        }

        true
    }

    pub(crate) fn check(&self) -> Result<(), FactQueryError> {
        if self.is_cancelled() {
            Err(FactQueryError::Cancelled)
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    pub(crate) fn poison_shared_interests(&self) {
        let CancellationState::Shared(state) = &*self.state else {
            panic!("test cancellation token must use shared state");
        };

        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _interests = state
                .interests
                .lock()
                .unwrap_or_else(|_| panic!("test cancellation state should begin available"));

            panic!("poison test shared cancellation interests");
        }));
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self {
            state: Arc::new(CancellationState::Request(AtomicBool::new(false))),
        }
    }
}

impl SharedCancellation {
    pub(crate) fn new() -> Self {
        Self {
            token: CancellationToken {
                state: Arc::new(CancellationState::Shared(SharedCancellationState {
                    next_interest: AtomicU64::new(0),
                    interests: Mutex::new(BTreeMap::new()),
                })),
            },
        }
    }

    pub(crate) fn token(&self) -> &CancellationToken {
        &self.token
    }

    pub(crate) fn register(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<CancellationInterest, FactQueryError> {
        if Arc::ptr_eq(&self.token.state, &cancellation.state) {
            panic!("query runtime RecursiveCancellationInterest contract failed");
        }

        let CancellationState::Shared(state) = &*self.token.state else {
            panic!(
                "query runtime InvalidCancellationState contract failed: expected={:?}, actual={:?}",
                CancellationStateKind::Shared,
                cancellation_state_kind(&self.token.state)
            );
        };

        let identity = state
            .next_interest
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| FactRuntimeFailure::CapacityExhausted {
                resource: CapacityResource::CancellationInterestIdentity,
                fact: None,
                task: None,
            })?;

        let mut interests = state.interests.lock().unwrap_or_else(|_| {
            panic!(
                "query runtime SynchronizationPoisoned contract failed: component={:?}",
                SynchronizationComponent::CancellationInterests
            )
        });

        // Shared demand retains the caller's Arc-backed request signal until interest ends.
        interests.insert(identity, cancellation.clone());

        Ok(CancellationInterest {
            state: Arc::clone(&self.token.state),
            identity,
        })
    }
}

impl Drop for CancellationInterest {
    fn drop(&mut self) {
        let CancellationState::Shared(state) = &*self.state else {
            return;
        };

        let mut interests = match state.interests.lock() {
            Ok(interests) => interests,
            Err(_) if std::thread::panicking() => return,
            Err(cause) => {
                panic!("cancellation interest cleanup state must remain unpoisoned: {cause}")
            }
        };

        interests.remove(&self.identity);
    }
}

fn cancellation_state_kind(state: &Arc<CancellationState>) -> CancellationStateKind {
    match &**state {
        CancellationState::Request(_) => CancellationStateKind::Request,
        CancellationState::Shared(_) => CancellationStateKind::Shared,
    }
}

impl Cancellation for CancellationToken {
    fn is_cancelled(&self) -> bool {
        Self::is_cancelled(self)
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use bray_base::Cancellation;

    use super::{CancellationToken, SharedCancellation};
    use crate::fact::{BatchWork, FactRuntime};

    #[test]
    fn cloned_tokens_share_cancellation() {
        let token = CancellationToken::new();
        let clone = token.clone();

        clone.cancel();

        assert!(token.is_cancelled());
        assert!(clone.is_cancelled());
    }

    #[test]
    fn cancellation_tokens_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CancellationToken>();
    }

    #[test]
    fn cancellation_tokens_serve_binder_requests_read_only() {
        let token = CancellationToken::new();

        assert!(!Cancellation::is_cancelled(&token));

        token.cancel();

        assert!(Cancellation::is_cancelled(&token));
    }

    #[test]
    fn recursive_shared_interest_retains_the_exact_state_failure() {
        let shared = SharedCancellation::new();

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            shared.register(shared.token())
        }))
        .expect_err("compiler invariant must panic");

        let message = bray_testing::panic_payload_text(panic.as_ref());

        assert!(message.contains("RecursiveCancellationInterest"));
    }

    #[test]
    fn poisoned_shared_interest_never_becomes_cancellation() {
        let shared = SharedCancellation::new();

        shared.token().poison_shared_interests();

        for operation in 0..3 {
            let panic = catch_unwind(AssertUnwindSafe(|| match operation {
                0 => {
                    let _ = shared.token().check();
                }
                1 => {
                    let _ = shared.token().is_cancelled();
                }
                _ => {
                    let _ = FactRuntime::default().complete_batch([1_u32], shared.token(), |_| {
                        Ok::<_, ()>(BatchWork::leaf(()))
                    });
                }
            }))
            .expect_err("compiler invariant must panic");

            assert!(
                bray_testing::panic_payload_text(panic.as_ref())
                    .contains("shared cancellation interests")
            );
        }
    }

    #[test]
    fn indirect_shared_interest_cycles_report_recursion_without_deadlocking() {
        let first = SharedCancellation::new();
        let second = SharedCancellation::new();

        let _first_interest = first
            .register(second.token())
            .unwrap_or_else(|error| panic!("first shared interest must register: {error:?}"));

        let _second_interest = second
            .register(first.token())
            .unwrap_or_else(|error| panic!("second shared interest must register: {error:?}"));

        let panic =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| first.token().check()))
                .expect_err("compiler invariant must panic");

        let message = bray_testing::panic_payload_text(panic.as_ref());

        assert!(message.contains("RecursiveCancellationInterest"));

        let panic = catch_unwind(AssertUnwindSafe(|| first.token().is_cancelled()))
            .expect_err("compiler invariant must panic");

        assert!(
            bray_testing::panic_payload_text(panic.as_ref())
                .contains("RecursiveCancellationInterest")
        );
    }
}
