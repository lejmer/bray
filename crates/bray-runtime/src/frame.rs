use std::any::Any;
use std::fmt;
use std::pin::Pin;

use bray_runtime_model::{
    ProtectedFrameDescriptor, ProtectedFrameStateDescriptor, ProtectedFrameStateId,
};

/// Runtime state supplied to one protected-frame resume operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameContext {
    cancellation_requested: bool,
}

impl FrameContext {
    /// Creates a resume context from the current run-cancellation state.
    pub const fn new(cancellation_requested: bool) -> Self {
        Self {
            cancellation_requested,
        }
    }

    /// Returns whether cancellation has been requested for the current run.
    pub const fn cancellation_requested(self) -> bool {
        self.cancellation_requested
    }
}

/// Checked protected-frame state retained while execution is suspended.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FrameSuspension {
    kind: FrameSuspensionKind,
    state: ProtectedFrameStateId,
    payload: Option<usize>,
}

/// Runtime action that caused a protected frame to suspend.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FrameSuspensionKind {
    /// The frame is waiting for a directly composed child frame.
    Awaited,
    /// The frame yielded so another ready task can run.
    Yield,
    /// The frame waits for one runtime task event.
    TaskEvent,
}

impl FrameSuspension {
    /// Creates a suspension at one descriptor-local state.
    pub const fn new(state: ProtectedFrameStateId) -> Self {
        Self {
            kind: FrameSuspensionKind::Awaited,
            state,
            payload: None,
        }
    }

    /// Creates a cooperative-yield suspension.
    pub const fn yielding(state: ProtectedFrameStateId) -> Self {
        Self {
            kind: FrameSuspensionKind::Yield,
            state,
            payload: None,
        }
    }

    /// Creates a runtime-task-event suspension.
    pub const fn task_event(state: ProtectedFrameStateId, event: usize) -> Self {
        Self {
            kind: FrameSuspensionKind::TaskEvent,
            state,
            payload: Some(event),
        }
    }

    /// Returns the runtime action that caused the suspension.
    pub const fn kind(self) -> FrameSuspensionKind {
        self.kind
    }

    /// Returns the descriptor-local suspended state.
    pub const fn state(self) -> ProtectedFrameStateId {
        self.state
    }

    /// Returns the opaque payload associated with this suspension.
    pub const fn payload(self) -> Option<usize> {
        self.payload
    }
}

/// Result of entering or resuming one protected frame.
#[derive(Debug)]
pub enum FrameProgress<T> {
    /// Execution suspended with initialized state retained by the frame.
    Suspended(FrameSuspension),
    /// Execution completed normally with its result moved out.
    Completed(T),
    /// Execution reached cancellation cleanup without a completion value.
    Cancelled,
    /// A panic crossed the protected frame boundary.
    Panicked(RuntimePanic),
    /// The frame violated its compiler/runtime execution contract.
    RuntimeFailure,
}

/// Terminal path whose retained lifecycle state must be resolved.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FrameExit {
    /// The frame completed normally.
    Completed,
    /// The frame completed cancellation cleanup.
    Cancelled,
    /// The frame terminated through panic propagation.
    Panicked,
    /// The frame violated its compiler/runtime contract.
    RuntimeFailure,
}

/// Opaque owned panic crossing a protected runtime boundary.
///
/// Disposal visits the primary before suppressed failures. Reports are flattened when
/// transferred, so their depth does not affect disposal stack usage.
pub struct RuntimePanic {
    primary: Option<crate::outgoing::Payload>,
    suppressed: crate::outgoing::OutgoingRecords,
    reserved: crate::outgoing::OutgoingRecords,
}

impl RuntimePanic {
    /// Creates a runtime panic from one owned payload.
    pub fn new(payload: impl Any + Send) -> Self {
        Self::from_payload(Box::new(payload))
    }

    /// Returns whether the primary panic payload has the requested Rust type.
    pub fn primary_is<T: Any>(&self) -> bool {
        self.primary.is_some() && self.primary_type_id() == std::any::TypeId::of::<T>()
    }

    /// Returns the number of later panics retained behind the primary panic.
    pub fn suppressed_count(&self) -> usize {
        self.suppressed.len()
    }

    pub(crate) fn from_native(mut report: bray_runtime_abi::NativePanicReport) -> Self {
        let (primary, head, tail, count, reserved) = report.take_parts();

        Self {
            primary: Some(crate::outgoing::Payload::Native(primary)),
            suppressed: crate::outgoing::OutgoingRecords::from_parts(head, tail, count),
            reserved: crate::outgoing::OutgoingRecords::from_parts(
                reserved,
                reserved,
                usize::from(reserved != 0),
            ),
        }
    }

    pub(crate) fn into_native(
        mut self,
        admitted: &mut crate::outgoing::OutgoingRecords,
    ) -> bray_runtime_abi::NativePanicReport {
        let primary = match self.primary.take() {
            Some(crate::outgoing::Payload::Native(primary)) => primary,
            Some(payload @ crate::outgoing::Payload::Rust(_)) => {
                let mut record = crate::outgoing::OutgoingRecords::default();

                let capacity = if self.reserved.len() == 0 {
                    admitted
                } else {
                    &mut self.reserved
                };

                record.push(payload, capacity);

                let (head, _, _) = record.into_parts();

                bray_runtime_abi::NativePanicPrimary::new(
                    bray_runtime_abi::NativePanicCause::RUNTIME_PANIC,
                    bray_runtime_abi::NativeSourceAnchor::unavailable(),
                    bray_runtime_abi::NativePanicMessage::new(
                        head,
                        0,
                        None,
                        Some(release_rust_primary),
                    ),
                )
            }
            None => bray_runtime_abi::NativePanicPrimary::empty(),
        };

        let mut report = native_report(primary);

        let (head, tail, count) = std::mem::take(&mut self.suppressed).into_parts();

        report.set_outgoing(head, tail, count);

        let (reserved, _, _) = std::mem::take(&mut self.reserved).into_parts();

        report.set_reserved(reserved);

        report
    }

    pub(crate) fn retain_reservation(&mut self, reservation: crate::outgoing::OutgoingRecords) {
        if self.reserved.len() == 0 {
            self.reserved = reservation;
        }
    }

    pub(crate) fn from_payload(payload: Box<dyn Any + Send>) -> Self {
        match payload.downcast::<Self>() {
            Ok(panic) => *panic,
            Err(payload) => Self {
                primary: Some(crate::outgoing::Payload::Rust(payload)),
                suppressed: crate::outgoing::OutgoingRecords::default(),
                reserved: crate::outgoing::OutgoingRecords::default(),
            },
        }
    }

    pub(crate) fn into_records(
        mut self,
        admitted: &mut crate::outgoing::OutgoingRecords,
    ) -> crate::outgoing::OutgoingRecords {
        let mut records = crate::outgoing::OutgoingRecords::default();

        if let Some(primary) = self.primary.take() {
            let capacity = if self.reserved.len() == 0 {
                admitted
            } else {
                &mut self.reserved
            };

            records.push(primary, capacity);
        }

        records.append(&mut self.suppressed);

        records
    }

    pub(crate) fn from_records(mut records: crate::outgoing::OutgoingRecords) -> Self {
        Self {
            primary: records.pop(),
            suppressed: records,
            reserved: crate::outgoing::OutgoingRecords::default(),
        }
    }

    pub(crate) fn primary_type_id(&self) -> std::any::TypeId {
        match self.primary.as_ref() {
            Some(crate::outgoing::Payload::Rust(payload)) => payload.as_ref().type_id(),
            Some(crate::outgoing::Payload::Native(primary)) => {
                primary.provider_handle(release_rust_primary).map_or(
                    std::any::TypeId::of::<bray_runtime_abi::NativePanicPrimary>(),
                    crate::outgoing::OutgoingRecords::payload_type_id,
                )
            }
            None => unreachable!("a live cleanup incident owns its primary"),
        }
    }

    pub(crate) fn pop_payload(&mut self) -> Option<crate::outgoing::Payload> {
        self.primary.take().or_else(|| self.suppressed.pop())
    }

    pub(crate) fn prepend(&mut self, mut incident: Self) {
        // Disposal has already consumed the primary before a destructor can report a failure.
        incident.suppressed.append(&mut self.suppressed);

        self.primary = incident.primary.take();
        self.reserved = std::mem::take(&mut incident.reserved);
        self.suppressed = std::mem::take(&mut incident.suppressed);
    }

    pub(crate) fn push_suppressed(
        &mut self,
        payload: Box<dyn Any + Send>,
        admitted: &mut crate::outgoing::OutgoingRecords,
    ) {
        self.append(Self::from_payload(payload), admitted);
    }

    pub(crate) fn append(
        &mut self,
        mut incident: Self,
        admitted: &mut crate::outgoing::OutgoingRecords,
    ) {
        if let Some(primary) = incident.primary.take() {
            let capacity = if incident.reserved.len() != 0 {
                &mut incident.reserved
            } else {
                admitted
            };

            self.suppressed.push(primary, capacity);
        }

        self.suppressed.append(&mut incident.suppressed);
    }

    pub(crate) fn record(
        panic: &mut Option<Self>,
        payload: Box<dyn Any + Send>,
        admitted: &mut crate::outgoing::OutgoingRecords,
    ) {
        if let Some(panic) = panic {
            panic.push_suppressed(payload, admitted);
        } else {
            *panic = Some(Self::from_payload(payload));
        }
    }
}

pub(crate) fn native_report(
    primary: bray_runtime_abi::NativePanicPrimary,
) -> bray_runtime_abi::NativePanicReport {
    bray_runtime_abi::NativePanicReport::new(primary, consume_native_report)
}

extern "C" fn release_rust_primary(record: usize, _: usize) {
    drop(crate::outgoing::OutgoingRecords::from_parts(
        record, record, 1,
    ));
}

extern "C" fn consume_native_report(
    report: &mut bray_runtime_abi::NativePanicReport,
    reporting: bool,
) -> bray_runtime_abi::NativeRuntimeStatus {
    let mut panic = RuntimePanic::from_native(std::mem::replace(
        report,
        bray_runtime_abi::NativePanicReport::empty(),
    ));

    let mut status = bray_runtime_abi::NativeRuntimeStatus::SUCCESS;

    while let Some(payload) = panic.pop_payload() {
        let payload = match payload {
            crate::outgoing::Payload::Native(mut primary) if reporting => {
                if let Some(record) = primary.take_provider_handle(release_rust_primary) {
                    crate::outgoing::OutgoingRecords::from_parts(record, record, 1)
                        .pop()
                        .unwrap_or_else(|| {
                            unreachable!("the Rust provider retains its primary until consumption")
                        })
                } else {
                    crate::outgoing::Payload::Native(primary)
                }
            }
            payload => payload,
        };

        match payload {
            crate::outgoing::Payload::Native(primary) if reporting => {
                let found = crate::native::report_primary(&primary);

                if status.is_success() {
                    status = found;
                }
            }
            crate::outgoing::Payload::Rust(payload) if reporting => {
                let message = match payload.downcast::<String>() {
                    Ok(message) => *message,
                    Err(payload) => match payload.downcast::<&'static str>() {
                        Ok(message) => (*message).to_owned(),
                        Err(payload) => {
                            crate::incident::dispose_panic(payload);

                            String::new()
                        }
                    },
                };

                crate::native::report_panic(
                    bray_runtime_abi::NativePanicCause::RUNTIME_PANIC,
                    bray_runtime_abi::NativeSourceAnchor::unavailable(),
                    message,
                );
            }
            payload => payload.dispose(),
        }
    }

    status
}

impl Drop for RuntimePanic {
    #[inline(never)]
    fn drop(&mut self) {
        while let Some(payload) = self.pop_payload() {
            payload.dispose();
        }
    }
}

impl fmt::Debug for RuntimePanic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimePanic")
            .field("suppressed_count", &self.suppressed.len())
            .finish_non_exhaustive()
    }
}

/// Safe in-process adapter over validated compiler-emitted frame operations.
///
/// The binary operation identities and retained-state contract live in
/// [`ProtectedFrameDescriptor`]. Implementations own their retained values and
/// child computations while exposing those operations without unsafe code.
pub trait ProtectedFrame: 'static {
    /// Result moved out after normal completion.
    type Output;

    /// Returns the immutable compiler-generated descriptor.
    fn descriptor(&self) -> &ProtectedFrameDescriptor;

    /// Enters or resumes the pinned frame.
    fn resume(self: Pin<&mut Self>, context: FrameContext) -> FrameProgress<Self::Output>;

    /// Broadcasts cancellation to unresolved tasks owned by initialized state.
    fn broadcast_tasks(self: Pin<&mut Self>);

    /// Resolves retained lifecycle state after broadcast has completed.
    fn resolve_lifecycle(self: Pin<&mut Self>, exit: FrameExit);
}

/// A protected frame whose retained state may cross thread boundaries.
pub trait SendableProtectedFrame: ProtectedFrame + Send {}

impl<F> SendableProtectedFrame for F where F: ProtectedFrame + Send {}

/// Type-erased pinned protected frame with one known completion type.
pub type ErasedProtectedFrame<T> = Pin<Box<dyn ProtectedFrame<Output = T> + 'static>>;

/// Type-erased pinned frame whose retained state may cross threads.
pub type ErasedSendableProtectedFrame<T> =
    Pin<Box<dyn SendableProtectedFrame<Output = T> + 'static>>;

/// Moves a concrete inactive frame into stable erased storage.
pub fn erase_protected_frame<F>(frame: F) -> ErasedProtectedFrame<F::Output>
where
    F: ProtectedFrame,
{
    Box::pin(frame)
}

/// Moves a sendable inactive frame into stable erased storage.
pub fn erase_sendable_protected_frame<F>(frame: F) -> ErasedSendableProtectedFrame<F::Output>
where
    F: SendableProtectedFrame,
{
    Box::pin(frame)
}

/// Resumes a directly awaited frame without creating a task-control block.
pub fn resume_direct<F>(frame: Pin<&mut F>, context: FrameContext) -> FrameProgress<F::Output>
where
    F: ?Sized + ProtectedFrame,
{
    frame.resume(context)
}

pub(crate) fn suspension_state(
    descriptor: &ProtectedFrameDescriptor,
    suspension: FrameSuspension,
) -> Option<&ProtectedFrameStateDescriptor> {
    descriptor.state(suspension.state())
}

#[cfg(test)]
mod tests {
    use std::pin::pin;

    use bray_runtime_model::ProtectedFrameDescriptor;

    use super::{
        ErasedProtectedFrame, FrameContext, FrameExit, FrameProgress, ProtectedFrame,
        erase_protected_frame, resume_direct,
    };
    use crate::test_support::TestFrame;

    #[test]
    fn direct_resume_needs_no_task_storage() {
        let mut frame = pin!(TestFrame::completing(17));

        let progress = resume_direct(frame.as_mut(), FrameContext::new(false));

        assert!(matches!(progress, FrameProgress::Completed(17)));

        assert_eq!(
            frame.descriptor().frame(),
            bray_runtime_model::ProtectedAsyncFrameId::new([7; 32])
        );
    }

    #[test]
    fn direct_await_can_compose_an_erased_child_without_a_child_task() {
        let child = erase_protected_frame(TestFrame::completing(23));
        let descriptor = child.descriptor().clone();
        let mut parent = pin!(ParentFrame { descriptor, child });

        let progress = resume_direct(parent.as_mut(), FrameContext::new(false));

        assert!(matches!(progress, FrameProgress::Completed(23)));
    }

    #[test]
    fn native_round_trip_preserves_rust_primary_identity_after_producer_release() {
        struct Payload(std::sync::Arc<std::sync::atomic::AtomicUsize>);

        impl Drop for Payload {
            fn drop(&mut self) {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }

        let drops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut admitted = crate::outgoing::OutgoingRecords::admit(1).unwrap();
        let panic = super::RuntimePanic::new(Payload(std::sync::Arc::clone(&drops)));
        let native = panic.into_native(&mut admitted);
        drop(admitted);
        let failure = crate::outgoing::tests::reject_admission();
        let panic = super::RuntimePanic::from_native(native);
        assert!(panic.primary_is::<Payload>());
        assert!(!panic.primary_is::<String>());
        let native = panic.into_native(&mut crate::outgoing::OutgoingRecords::default());
        drop(super::RuntimePanic::from_native(native));
        assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst), 1);
        drop(failure);
    }

    struct ParentFrame {
        descriptor: ProtectedFrameDescriptor,
        child: ErasedProtectedFrame<i32>,
    }

    impl ProtectedFrame for ParentFrame {
        type Output = i32;

        fn descriptor(&self) -> &ProtectedFrameDescriptor {
            &self.descriptor
        }

        fn resume(
            mut self: std::pin::Pin<&mut Self>,
            context: FrameContext,
        ) -> FrameProgress<Self::Output> {
            self.child.as_mut().resume(context)
        }

        fn broadcast_tasks(mut self: std::pin::Pin<&mut Self>) {
            self.child.as_mut().broadcast_tasks();
        }

        fn resolve_lifecycle(mut self: std::pin::Pin<&mut Self>, exit: FrameExit) {
            self.child.as_mut().resolve_lifecycle(exit);
        }
    }
}
