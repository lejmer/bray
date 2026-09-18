use std::any::Any;
use std::collections::TryReserveError;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard};

use bray_runtime_model::{
    ProtectedFrameDescriptor, ProtectedFrameStateDescriptor, ProtectedFrameStateId,
};

/// Identity and checked execution metadata for one active frame-local state.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FrameExecutionState {
    frame: bray_runtime_model::ProtectedAsyncFrameId,
    origin: Option<bray_platform::RuntimeThreadId>,
    descriptor: ProtectedFrameStateDescriptor,
}

impl FrameExecutionState {
    /// Associates a checked local state with its owning activation's frame.
    pub const fn new(
        frame: bray_runtime_model::ProtectedAsyncFrameId,
        descriptor: ProtectedFrameStateDescriptor,
    ) -> Self {
        Self {
            frame,
            origin: None,
            descriptor,
        }
    }

    pub(crate) const fn with_origin(mut self, origin: bray_platform::RuntimeThreadId) -> Self {
        self.origin = Some(origin);

        self
    }

    pub(crate) const fn origin(&self) -> Option<bray_platform::RuntimeThreadId> {
        self.origin
    }

    /// Returns the active activation's frame identity.
    pub const fn frame(&self) -> bray_runtime_model::ProtectedAsyncFrameId {
        self.frame
    }

    /// Returns the state identity local to the active frame.
    pub const fn state(&self) -> ProtectedFrameStateId {
        self.descriptor.state()
    }

    /// Returns the checked metadata for the active state.
    pub const fn descriptor(&self) -> &ProtectedFrameStateDescriptor {
        &self.descriptor
    }
}

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
    primary: Option<bray_runtime_abi::NativePanicPrimary>,
    suppressed: crate::outgoing::OutgoingRecords,
    reserved: crate::outgoing::OutgoingRecords,
}

#[derive(Default)]
struct RustPanicBackings {
    slots: Vec<Option<Box<dyn Any + Send>>>,
    free: Vec<usize>,
}

// Reports cross native threads, so their Rust payload provider must outlive every producer.
static RUST_PANIC_BACKINGS: Mutex<RustPanicBackings> = Mutex::new(RustPanicBackings {
    slots: Vec::new(),
    free: Vec::new(),
});

impl RuntimePanic {
    #[cfg(test)]
    pub(crate) fn new(
        payload: impl Any + Send,
        admitted: &mut crate::outgoing::OutgoingRecords,
    ) -> Self {
        Self::from_payload(Box::new(payload), admitted)
    }

    /// Returns the number of later panics retained behind the primary panic.
    pub fn suppressed_count(&self) -> usize {
        self.suppressed.len()
    }

    pub(crate) fn from_native(mut report: bray_runtime_abi::NativePanicReport) -> Self {
        let (primary, head, tail, count, reserved) = report.take_parts();

        Self {
            primary: Some(primary),
            suppressed: crate::outgoing::OutgoingRecords::from_parts(head, tail, count),
            reserved: crate::outgoing::OutgoingRecords::from_parts(
                reserved,
                reserved,
                usize::from(reserved != 0),
            ),
        }
    }

    pub(crate) fn into_native(mut self) -> bray_runtime_abi::NativePanicReport {
        let primary = self
            .primary
            .take()
            .unwrap_or_else(bray_runtime_abi::NativePanicPrimary::empty);

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

    pub(crate) fn reserve_from(&mut self, admitted: &mut crate::outgoing::OutgoingRecords) {
        if self.reserved.len() == 0 {
            self.reserved = admitted.take(1);
        }
    }

    pub(crate) fn from_payload(
        payload: Box<dyn Any + Send>,
        admitted: &mut crate::outgoing::OutgoingRecords,
    ) -> Self {
        match payload.downcast::<Self>() {
            Ok(panic) => *panic,
            Err(payload) => {
                let (primary, reserved) = admitted.take_rust_primary(payload);

                Self {
                    primary: Some(primary),
                    suppressed: crate::outgoing::OutgoingRecords::default(),
                    reserved,
                }
            }
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

    fn pop_primary(&mut self) -> Option<bray_runtime_abi::NativePanicPrimary> {
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
        match payload.downcast::<Self>() {
            Ok(panic) => self.append(*panic, admitted),
            Err(payload) => self.suppressed.push_rust(payload, admitted),
        }
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
            *panic = Some(Self::from_payload(payload, admitted));
        }
    }

    pub(crate) fn consume(&mut self, reporting: bool) -> bray_runtime_abi::NativeRuntimeStatus {
        let mut status = bray_runtime_abi::NativeRuntimeStatus::SUCCESS;

        while let Some(mut primary) = self.pop_primary() {
            if reporting {
                let found = crate::native::report_primary(&primary);

                if status.is_success() {
                    status = found;
                }
            }

            let mut release = primary.release_message();

            match release.state() {
                bray_runtime_abi::NativeRunState::COMPLETED => {}
                bray_runtime_abi::NativeRunState::PANICKED => {
                    if status.is_success() {
                        status = bray_runtime_abi::NativeRuntimeStatus::PANICKED;
                    }

                    self.prepend(Self::from_native(release.take_report()));
                }
                _ => unreachable!("message release returns completed or panicked"),
            }
        }

        status
    }
}

pub(crate) fn reserve_rust_panic_backings(count: usize) -> Result<(), TryReserveError> {
    rust_panic_backings().reserve_free(count)
}

pub(crate) fn reserved_rust_panic_primary() -> bray_runtime_abi::NativePanicPrimary {
    let handle = rust_panic_backings().take_free() + 1;

    rust_panic_primary(handle, 0)
}

pub(crate) fn attach_rust_panic_payload(
    primary: &mut bray_runtime_abi::NativePanicPrimary,
    payload: Box<dyn Any + Send>,
) {
    let handle = primary
        .take_provider_handle(release_rust_panic)
        .unwrap_or_else(|| unreachable!("an admitted Rust panic owns reserved callback backing"));

    let length = rust_panic_message(payload.as_ref()).len();

    let index = handle
        .checked_sub(1)
        .unwrap_or_else(|| unreachable!("Rust panic backing handles are nonzero"));

    let mut backings = rust_panic_backings();

    let payload_slot = backings
        .slots
        .get_mut(index)
        .unwrap_or_else(|| unreachable!("Rust panic backing handle remains provider-owned"));

    assert!(
        payload_slot.replace(payload).is_none(),
        "reserved Rust panic backing is initialized exactly once"
    );

    drop(backings);

    *primary = rust_panic_primary(handle, length);
}

fn rust_panic_primary(handle: usize, length: usize) -> bray_runtime_abi::NativePanicPrimary {
    bray_runtime_abi::NativePanicPrimary::new(
        bray_runtime_abi::NativePanicCause::RUNTIME_PANIC,
        bray_runtime_abi::NativeSourceAnchor::unavailable(),
        bray_runtime_abi::NativePanicMessage::new(
            handle,
            length,
            Some(copy_rust_panic_message),
            Some(release_rust_panic),
        ),
    )
}

fn rust_panic_message(payload: &(dyn Any + Send)) -> &[u8] {
    payload
        .downcast_ref::<String>()
        .map(String::as_bytes)
        .or_else(|| {
            payload
                .downcast_ref::<&'static str>()
                .map(|message| message.as_bytes())
        })
        .unwrap_or_default()
}

#[expect(
    unsafe_code,
    reason = "the panic-message ABI callback writes the caller-validated destination range"
)]
extern "C" fn copy_rust_panic_message(
    handle: usize,
    offset: usize,
    destination: *mut u8,
    length: usize,
) -> bray_runtime_abi::NativeRuntimeStatus {
    let Some(index) = handle.checked_sub(1) else {
        return bray_runtime_abi::NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    let backings = rust_panic_backings();

    let Some(payload) = backings
        .slots
        .get(index)
        .and_then(Option::as_deref)
    else {
        return bray_runtime_abi::NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    let message = rust_panic_message(payload);

    if offset > message.len() || length > message.len() - offset {
        return bray_runtime_abi::NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(message.as_ptr().add(offset), destination, length);
    }

    bray_runtime_abi::NativeRuntimeStatus::SUCCESS
}

extern "C" fn release_rust_panic(
    handle: usize,
    _: usize,
    outcome: &mut bray_runtime_abi::NativeRunOutcome,
) {
    let index = handle
        .checked_sub(1)
        .unwrap_or_else(|| unreachable!("Rust panic backing handles are nonzero"));

    let payload = rust_panic_backings()
        .slots
        .get_mut(index)
        .unwrap_or_else(|| unreachable!("Rust panic backing handle remains provider-owned"))
        .take();

    if let Some(payload) = payload {
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
            let length = rust_panic_message(payload.as_ref()).len();

            rust_panic_backings().slots[index] = Some(payload);

            *outcome = bray_runtime_abi::NativeRunOutcome::panicked(native_report(
                rust_panic_primary(handle, length),
            ));

            return;
        }
    }

    rust_panic_backings().release(index);
}

impl RustPanicBackings {
    fn reserve_free(&mut self, count: usize) -> Result<(), TryReserveError> {
        let additional = count.saturating_sub(self.free.len());

        self.slots.try_reserve(additional)?;
        self.free.try_reserve(additional)?;

        for _ in 0..additional {
            let index = self.slots.len();

            self.slots.push(None);
            self.free.push(index);
        }

        Ok(())
    }

    fn take_free(&mut self) -> usize {
        self.free
            .pop()
            .unwrap_or_else(|| unreachable!("admission reserves Rust panic callback backing"))
    }

    fn release(&mut self, index: usize) {
        assert!(
            self.slots[index].is_none(),
            "Rust panic backing releases only after its payload"
        );

        self.free.push(index);
    }
}

fn rust_panic_backings() -> MutexGuard<'static, RustPanicBackings> {
    RUST_PANIC_BACKINGS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn native_report(
    primary: bray_runtime_abi::NativePanicPrimary,
) -> bray_runtime_abi::NativePanicReport {
    bray_runtime_abi::NativePanicReport::new(primary, consume_native_report)
}

extern "C" fn consume_native_report(
    report: &mut bray_runtime_abi::NativePanicReport,
    reporting: bool,
) -> bray_runtime_abi::NativeRuntimeStatus {
    let mut panic = RuntimePanic::from_native(std::mem::replace(
        report,
        bray_runtime_abi::NativePanicReport::empty(),
    ));

    panic.consume(reporting)
}

impl Drop for RuntimePanic {
    #[inline(never)]
    fn drop(&mut self) {
        self.consume(false);
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

    /// Resolves a local state against the currently active activation.
    fn execution_state(&self, state: ProtectedFrameStateId) -> Option<FrameExecutionState> {
        let descriptor = self.descriptor();

        descriptor
            .state(state)
            .cloned()
            .map(|state| FrameExecutionState::new(descriptor.frame(), state))
    }

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

pub(crate) fn terminalize_frame<T, F>(
    mut frame: Pin<Box<F>>,
    progress: FrameProgress<T>,
    outgoing: &mut crate::outgoing::OutgoingRecords,
) -> Option<crate::RunOutcome<T>>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    let (mut outcome, mut exit) = terminal_outcome(progress);

    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
        frame.as_mut().broadcast_tasks();
    })) {
        record_terminal_panic(&mut outcome, payload, outgoing);

        if exit != FrameExit::RuntimeFailure {
            exit = FrameExit::Panicked;
        }
    }

    resolve_terminal_frame(frame, outcome, exit, outgoing)
}

pub(crate) fn terminalize_frame_after_broadcast<T, F>(
    frame: Pin<Box<F>>,
    progress: FrameProgress<T>,
    outgoing: &mut crate::outgoing::OutgoingRecords,
) -> Option<crate::RunOutcome<T>>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    let (outcome, exit) = terminal_outcome(progress);

    resolve_terminal_frame(frame, outcome, exit, outgoing)
}

fn terminal_outcome<T>(progress: FrameProgress<T>) -> (Option<crate::RunOutcome<T>>, FrameExit) {
    match progress {
        FrameProgress::Suspended(_) => unreachable!("suspended frames are not terminalized"),
        FrameProgress::Completed(value) => (
            Some(crate::RunOutcome::Completed(value)),
            FrameExit::Completed,
        ),
        FrameProgress::Cancelled => (Some(crate::RunOutcome::Cancelled), FrameExit::Cancelled),
        FrameProgress::Panicked(panic) => (
            Some(crate::RunOutcome::Panicked(panic)),
            FrameExit::Panicked,
        ),
        FrameProgress::RuntimeFailure => (None, FrameExit::RuntimeFailure),
    }
}

fn resolve_terminal_frame<T, F>(
    mut frame: Pin<Box<F>>,
    mut outcome: Option<crate::RunOutcome<T>>,
    exit: FrameExit,
    outgoing: &mut crate::outgoing::OutgoingRecords,
) -> Option<crate::RunOutcome<T>>
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
        frame.as_mut().resolve_lifecycle(exit);
    })) {
        record_terminal_panic(&mut outcome, payload, outgoing);
    }

    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(frame))) {
        record_terminal_panic(&mut outcome, payload, outgoing);
    }

    outcome
}

fn record_terminal_panic<T>(
    outcome: &mut Option<crate::RunOutcome<T>>,
    payload: Box<dyn Any + Send>,
    outgoing: &mut crate::outgoing::OutgoingRecords,
) {
    let panic = match outcome.take() {
        Some(crate::RunOutcome::Panicked(mut panic)) => {
            panic.push_suppressed(payload, outgoing);

            panic
        }
        Some(crate::RunOutcome::Completed(value)) => {
            let mut panic = RuntimePanic::from_payload(payload, outgoing);

            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(value))) {
                panic.push_suppressed(payload, outgoing);
            }

            panic
        }
        Some(crate::RunOutcome::Cancelled) | None => RuntimePanic::from_payload(payload, outgoing),
    };

    *outcome = Some(crate::RunOutcome::Panicked(panic));
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
    fn native_round_trip_owns_rust_primary_until_report_disposal() {
        struct Payload(std::sync::Arc<std::sync::atomic::AtomicUsize>);

        impl Drop for Payload {
            fn drop(&mut self) {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }

        let drops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut admitted = crate::outgoing::OutgoingRecords::admit(1).unwrap();

        let panic = super::RuntimePanic::new(
            Payload(std::sync::Arc::clone(&drops)),
            &mut admitted,
        );

        let native = panic.into_native();

        drop(admitted);

        let failure = crate::outgoing::tests::reject_admission();
        let panic = super::RuntimePanic::from_native(native);
        let native = panic.into_native();

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
