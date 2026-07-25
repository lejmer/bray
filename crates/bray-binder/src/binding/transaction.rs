use crate::BinderFactContext;
use crate::binder::{Binder, BinderCheckpoint};

use super::{BindingError, BindingResult};

impl<C> Binder<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    /// Runs an atomic binding operation whose failed state is never published.
    pub(crate) fn bind_transaction<T>(
        &mut self,
        bind: impl FnOnce(&mut Self) -> BindingResult<T>,
    ) -> BindingResult<T> {
        let checkpoint = self.checkpoint();

        match bind(self) {
            Ok(value) if self.transaction_context_is_balanced(&checkpoint) => Ok(value),
            Ok(_) => {
                self.rollback_or_error(checkpoint)?;

                Err(BindingError::TransactionContextMismatch)
            }
            Err(error) => {
                self.rollback_or_error(checkpoint)?;

                Err(error)
            }
        }
    }

    fn rollback_or_error(&mut self, checkpoint: BinderCheckpoint) -> BindingResult<()> {
        if self.rollback(checkpoint) {
            Ok(())
        } else {
            Err(BindingError::RollbackFailed)
        }
    }
}
