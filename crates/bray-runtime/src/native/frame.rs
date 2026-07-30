use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use bray_runtime_interface::{
    BinarySymbolName, ExecutionLaneRequirement, NativeFrameAffinity,
    NativeFrameExit, NativeFrameProgressKind, NativeLaneRequirements,
    NativeProtectedFrame, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
    ProtectedFrameAffinity, ProtectedFrameDescriptor, ProtectedFrameLayout,
    ProtectedFrameOperations, ProtectedFrameStateDescriptor,
    ProtectedFrameStateId, RuntimeAbiVersion,
};

use crate::{
    FrameContext, FrameExit, FrameProgress, FrameSuspension, ProtectedFrame,
    RuntimePanic,
};

pub(super) struct NativeFrame {
    descriptor: ProtectedFrameDescriptor,
    abi: NativeProtectedFrame,
    terminal_payload: Arc<Mutex<Option<NativeTerminalPayload>>>,
}

pub(super) enum NativeTerminalPayload {
    Completion {
        address: usize,
        _storage: Box<[u128]>,
    },
    Opaque(usize),
}

impl NativeTerminalPayload {
    fn completion(size: usize, alignment: usize) -> Option<Self> {
        if alignment > align_of::<u128>() {
            return None;
        }

        let word_count = size.div_ceil(size_of::<u128>()).max(1);
        let storage = vec![0_u128; word_count].into_boxed_slice();
        let address = storage.as_ptr() as usize;

        Some(Self::Completion {
            address,
            _storage: storage,
        })
    }

    pub(super) const fn handle(&self) -> usize {
        match self {
            Self::Completion { address, .. } | Self::Opaque(address) => *address,
        }
    }
}

impl NativeFrame {
    pub(super) fn try_new(abi: NativeProtectedFrame) -> Option<Self> {
        let mut transfer = NativeFrameTransfer::new(abi);
        let abi = transfer.frame();

        let alignment = NonZeroUsize::new(abi.alignment())?;

        let completion_alignment =
            NonZeroUsize::new(abi.completion_alignment())?;

        let layout = ProtectedFrameLayout::try_new(abi.size(), alignment).ok()?;

        let completion_layout = ProtectedFrameLayout::try_new(
            abi.completion_size(),
            completion_alignment,
        )
        .ok()?;

        let states = states(abi)?;
        let operations = operations(abi.identity())?;
        let version = RuntimeAbiVersion::new(1, 0);

        let descriptor = ProtectedFrameDescriptor::try_new(
            ProtectedAsyncFrameId::new(abi.identity()),
            version,
            ProtectedFrameAbiVersions::uniform(version),
            layout,
            completion_layout,
            operations,
            states,
        )
        .ok()?;

        Some(Self {
            descriptor,
            abi: transfer.take(),
            terminal_payload: Arc::new(Mutex::new(None)),
        })
    }

    pub(super) fn terminal_payload(
        &self,
    ) -> Arc<Mutex<Option<NativeTerminalPayload>>> {
        self.terminal_payload.clone()
    }

    fn record_terminal_payload(&self, payload: NativeTerminalPayload) {
        *self
            .terminal_payload
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(payload);
    }
}

impl ProtectedFrame for NativeFrame {
    type Output = usize;

    fn descriptor(&self) -> &ProtectedFrameDescriptor {
        &self.descriptor
    }

    fn resume(
        self: Pin<&mut Self>,
        context: FrameContext,
    ) -> FrameProgress<Self::Output> {
        let progress = (self.abi.resume())(
            self.abi.context(),
            u8::from(context.cancellation_requested()),
        );

        let kind = progress.kind();

        if kind == NativeFrameProgressKind::SUSPENDED {
            return FrameProgress::Suspended(FrameSuspension::new(
                ProtectedFrameStateId::new(progress.state()),
            ));
        }

        if kind == NativeFrameProgressKind::COMPLETED {
            let Some(payload) = NativeTerminalPayload::completion(
                self.abi.completion_size(),
                self.abi.completion_alignment(),
            ) else {
                return FrameProgress::RuntimeFailure;
            };

            (self.abi.move_completion())(self.abi.context(), payload.handle());

            let handle = payload.handle();
            self.record_terminal_payload(payload);

            return FrameProgress::Completed(handle);
        }

        if kind == NativeFrameProgressKind::CANCELLED {
            return FrameProgress::Cancelled;
        }

        if kind == NativeFrameProgressKind::PANICKED {
            self.record_terminal_payload(NativeTerminalPayload::Opaque(
                progress.payload(),
            ));

            return FrameProgress::Panicked(RuntimePanic::new(progress.payload()));
        }

        FrameProgress::RuntimeFailure
    }

    fn broadcast_tasks(self: Pin<&mut Self>) {
        (self.abi.broadcast_tasks())(self.abi.context());
    }

    fn resolve_lifecycle(self: Pin<&mut Self>, exit: FrameExit) {
        let exit = match exit {
            FrameExit::Completed => NativeFrameExit::COMPLETED,
            FrameExit::Cancelled => NativeFrameExit::CANCELLED,
            FrameExit::Panicked => NativeFrameExit::PANICKED,
            FrameExit::RuntimeFailure => NativeFrameExit::RUNTIME_FAILURE,
        };

        (self.abi.resolve_lifecycle())(self.abi.context(), exit);
    }
}

impl Drop for NativeFrame {
    fn drop(&mut self) {
        (self.abi.destroy())(self.abi.context());
    }
}

fn states(
    abi: &NativeProtectedFrame,
) -> Option<Vec<ProtectedFrameStateDescriptor>> {
    (0..abi.state_count())
        .map(|state| {
            let native = (abi.state())(abi.context(), state);
            let affinity = affinity(native.affinity())?;
            let requirements = lane_requirements(native.lane_requirements())?;

            Some(ProtectedFrameStateDescriptor::new(
                ProtectedFrameStateId::new(state),
                requirements,
                [],
                [],
                affinity,
            ))
        })
        .collect()
}

struct NativeFrameTransfer {
    frame: Option<NativeProtectedFrame>,
}

impl NativeFrameTransfer {
    const fn new(frame: NativeProtectedFrame) -> Self {
        Self { frame: Some(frame) }
    }

    fn frame(&self) -> &NativeProtectedFrame {
        let Some(frame) = &self.frame else {
            unreachable!("frame transfer must remain owned");
        };

        frame
    }

    fn take(&mut self) -> NativeProtectedFrame {
        let Some(frame) = self.frame.take() else {
            unreachable!("frame transfer must remain owned");
        };

        frame
    }
}

impl Drop for NativeFrameTransfer {
    fn drop(&mut self) {
        let Some(frame) = self.frame.take() else {
            return;
        };

        (frame.destroy())(frame.context());
    }
}

fn affinity(native: NativeFrameAffinity) -> Option<ProtectedFrameAffinity> {
    match native.code() {
        code if code == NativeFrameAffinity::MOVABLE.code() => {
            Some(ProtectedFrameAffinity::Movable)
        }
        code if code == NativeFrameAffinity::ORIGIN_THREAD.code() => {
            Some(ProtectedFrameAffinity::OriginThread)
        }
        code if code == NativeFrameAffinity::MAIN_THREAD.code() => {
            Some(ProtectedFrameAffinity::MainThread)
        }
        _ => None,
    }
}

fn lane_requirements(
    native: NativeLaneRequirements,
) -> Option<Vec<ExecutionLaneRequirement>> {
    let known = NativeLaneRequirements::BLOCKING.bits()
        | NativeLaneRequirements::COMPUTE.bits()
        | NativeLaneRequirements::MAIN_THREAD.bits();

    if native.bits() & !known != 0 {
        return None;
    }

    let mut requirements = Vec::new();

    if native.bits() & NativeLaneRequirements::BLOCKING.bits() != 0 {
        requirements.push(ExecutionLaneRequirement::Blocking);
    }

    if native.bits() & NativeLaneRequirements::COMPUTE.bits() != 0 {
        requirements.push(ExecutionLaneRequirement::Compute);
    }

    if native.bits() & NativeLaneRequirements::MAIN_THREAD.bits() != 0 {
        requirements.push(ExecutionLaneRequirement::MainThread);
    }

    Some(requirements)
}

fn operations(identity: [u8; 32]) -> Option<ProtectedFrameOperations> {
    let prefix = frame_symbol_prefix(identity);

    Some(ProtectedFrameOperations::new(
        operation_symbol(&prefix, "move_before_start")?,
        operation_symbol(&prefix, "resume")?,
        operation_symbol(&prefix, "cancellation_entry")?,
        operation_symbol(&prefix, "task_broadcast")?,
        operation_symbol(&prefix, "lifecycle_resolution")?,
        operation_symbol(&prefix, "completion_move")?,
        operation_symbol(&prefix, "destruction")?,
    ))
}

fn frame_symbol_prefix(identity: [u8; 32]) -> String {
    let mut prefix = String::from("bray_frame_");

    for byte in identity {
        use std::fmt::Write as _;

        let _ = write!(prefix, "{byte:02x}");
    }

    prefix
}

fn operation_symbol(prefix: &str, operation: &str) -> Option<BinarySymbolName> {
    BinarySymbolName::try_new(format!("{prefix}_{operation}"))
}
