use std::sync::Arc;

use bray_symbols::{
    CallableAbi, CallableExecution, CallableInstanceData, ConstantTermId, TypeId,
    UnionVariantSymbolId,
};

/// The ownership operation whose implementation is selected by checking.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecyclePhase {
    /// Invoke whole-value graceful finalization.
    Finalize,
    /// Destroy the remaining represented value.
    Destroy,
    /// Broadcast cancellation before resolving owners.
    Cancel,
    /// Finalize and destroy an owner.
    Resolve,
}

/// One selected lifecycle invocation with its substituted signature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifecycleCallable {
    /// Exact selected implementation.
    pub callable: CallableInstanceData,
    /// Calling convention of the selected implementation.
    pub abi: CallableAbi,
    /// Receiver argument type, including its borrow mode.
    pub receiver: TypeId,
    /// Completion type of the invocation.
    pub result: TypeId,
    /// Whether invoking the implementation creates an inactive future.
    pub execution: CallableExecution,
}

/// The selected local action for one type and lifecycle phase.
/// Child types retain separate ownership identities and are selected on demand.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleAction {
    /// This phase has no work.
    None,
    /// Invoke the selected whole-value implementation.
    Call(LifecycleCallable),
    /// Resolve whole-value finalization followed by destruction.
    Resolve,
    /// Visit represented members in reverse declaration order.
    Members(Arc<[TypeId]>),
    /// Visit only the active union payload.
    Alternatives(Arc<[(UnionVariantSymbolId, Arc<[TypeId]>)]>),
    /// Visit the payload only when present.
    Nullable(TypeId),
    /// Traverse elements in reverse index order.
    Array {
        /// Element type selected for each iteration.
        element: TypeId,
        /// Checked symbolic extent.
        length: ConstantTermId,
    },
    /// Resolve an owned target through its storage policy.
    Owned {
        /// Concrete storage policy type.
        storage: TypeId,
        /// Type of the stored owner.
        target: TypeId,
        /// Selected mutable projection of the initialized target.
        borrow: LifecycleCallable,
        /// Selected target destruction and storage release, in that order.
        teardown: Option<[LifecycleCallable; 2]>,
    },
    /// Resolve the initialized prefix of a generator.
    Generator(TypeId),
    /// Release an imported raw buffer through its recognized representation.
    RawBuffer(TypeId),
    /// Release retained string bytes.
    ReleaseString,
    /// Destroy an owned panic report.
    DestroyReport,
    /// Resolve a task using the operation selected by the requested phase.
    Task,
}
