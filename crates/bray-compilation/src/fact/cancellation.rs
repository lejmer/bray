use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bray_base::Cancellation;

use super::FactQueryError;

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
            CancellationState::Shared(shared) => {
                let Ok(interests) = shared.interests.lock() else {
                    return true;
                };

                interests.is_empty() || interests.values().all(CancellationToken::is_cancelled)
            }
        }
    }

    pub(crate) fn check(&self) -> Result<(), FactQueryError> {
        if self.is_cancelled() {
            Err(FactQueryError::Cancelled)
        } else {
            Ok(())
        }
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
            return Err(FactQueryError::InfrastructureFailure);
        }

        let CancellationState::Shared(state) = &*self.token.state else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let identity = state
            .next_interest
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let mut interests = state
            .interests
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

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

        let Ok(mut interests) = state.interests.lock() else {
            return;
        };

        interests.remove(&self.identity);
    }
}

impl Cancellation for CancellationToken {
    fn is_cancelled(&self) -> bool {
        Self::is_cancelled(self)
    }
}

#[cfg(test)]
mod tests {
    use bray_base::Cancellation;

    use super::CancellationToken;

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
}
