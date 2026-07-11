/// Read-only cancellation observation for one binding request.
///
/// The compilation query layer owns the cancellation signal and publication behavior. Binder
/// operations observe that signal through this contract and return
/// [`BinderFactError::Cancelled`](crate::BinderFactError) without publishing partial work.
pub trait BinderCancellation: Send + Sync {
    /// Returns whether the current binding request should stop.
    fn is_cancelled(&self) -> bool;
}

impl<Observe> BinderCancellation for Observe
where
    Observe: Fn() -> bool + Send + Sync,
{
    fn is_cancelled(&self) -> bool {
        self()
    }
}
