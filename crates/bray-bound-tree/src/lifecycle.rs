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
    /// Resolve child runs while retaining an error for later incident destruction.
    Quiesce,
    /// Destroy the remaining represented value.
    Destroy,
    /// Broadcast cancellation before resolving owners.
    Cancel,
    /// Finalize and destroy an owner.
    Resolve,
}

/// One selected lifecycle invocation with its substituted signature.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

impl LifecycleCallable {
    /// Returns the immediate value or inactive future created by this invocation.
    pub fn invocation_result(
        self,
        values: &bray_symbols::SemanticValueStore,
        symbols: &bray_symbols::AvailableCompilerKnownSymbols,
    ) -> Result<crate::BoundCallResult, bray_symbols::SemanticValueStoreError> {
        Ok(match self.execution {
            CallableExecution::Synchronous => crate::BoundCallResult::Immediate(self.result),
            CallableExecution::Asynchronous => {
                let future = symbols
                    .unary_representation_type(
                        values,
                        bray_compiler_known::RepresentationRole::Future,
                        self.result,
                    )?
                    .expect("checked asynchronous lifecycle retains its Future representation");

                crate::BoundCallResult::LazyFuture(crate::BoundFutureConstruction::new(
                    self.result,
                    future,
                ))
            }
        })
    }

    /// Retains a checked lifecycle selection without resolving its signature again.
    pub fn from_signature(
        values: &bray_symbols::SemanticValueStore,
        callable: CallableInstanceData,
        signature: &bray_symbols::CallableSignature,
    ) -> Result<Self, bray_symbols::SemanticValueStoreError> {
        let data = values.type_data(signature.callable_type());

        let bray_symbols::TypeData::Callable(selected) = data.as_ref() else {
            panic!("checked lifecycle signature retains its callable type");
        };

        let receiver = signature
            .receiver()
            .expect("checked lifecycle signature retains its receiver");

        Ok(Self {
            callable,
            abi: selected.abi(),
            receiver: receiver.mode().input_type(values, receiver.ty())?,
            result: signature.result(),
            execution: selected.execution(),
        })
    }
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
    /// Visit a raw buffer's initialized prefix or release its recognized representation.
    RawBuffer(TypeId),
    /// Release retained string bytes.
    ReleaseString,
    /// Destroy an owned panic report.
    DestroyReport,
    /// Resolve a task using the operation selected by the requested phase.
    Task,
}

impl LifecycleAction {
    /// Returns local invocations in execution order, preserving repeated calls.
    /// Structural children retain their own selected actions and cleanup allowances.
    pub fn local_calls(&self) -> impl Iterator<Item = LifecycleCallable> {
        let calls = match self {
            Self::Call(callable) => [Some(*callable), None, None],
            Self::Owned {
                borrow, teardown, ..
            } => [
                Some(*borrow),
                teardown.map(|calls| calls[0]),
                teardown.map(|calls| calls[1]),
            ],
            Self::None
            | Self::Resolve
            | Self::Members(_)
            | Self::Alternatives(_)
            | Self::Nullable(_)
            | Self::Array { .. }
            | Self::Generator(_)
            | Self::RawBuffer(_)
            | Self::ReleaseString
            | Self::DestroyReport
            | Self::Task => [None; 3],
        };

        calls.into_iter().flatten()
    }
}
