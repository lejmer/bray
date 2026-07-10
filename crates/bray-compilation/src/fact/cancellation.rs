use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::FactQueryError;

/// A shareable cancellation signal for one compiler request.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Creates an uncancelled request token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation for every observer of this token.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> Result<(), FactQueryError> {
        if self.is_cancelled() {
            Err(FactQueryError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
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
}
