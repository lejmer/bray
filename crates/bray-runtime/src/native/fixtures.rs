use std::alloc::Layout;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

use bray_runtime_abi::{
    NativeActivationDescriptor, NativeCleanupStorageShape, NativeFrameExit, NativeFrameProgress,
    NativeFrameState, NativeInactiveFrame, NativeReportRecords,
};

// Native callbacks need stable context across worker threads. This test-only table
// owns the actual resident lease and releases it in the production destroy callback.
static FRAMES: LazyLock<Mutex<BTreeMap<usize, Arc<Frame>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
thread_local! { static CURRENT: Cell<Option<usize>> = const { Cell::new(None) }; }

struct Frame {
    backing: crate::backing::Backing,
    descriptor: NativeActivationDescriptor,
    callbacks: NativeActivationDescriptor,
    context: usize,
}

pub(super) fn admit(context: usize, callbacks: NativeActivationDescriptor) -> NativeInactiveFrame {
    let mut bytes = 0;
    let mut alignment = 0;

    crate::report_provider::bray_runtime_activation_header_layout(&mut bytes, &mut alignment);

    let header =
        Layout::from_size_align(bytes, alignment.max(callbacks.storage.alignment())).unwrap();

    let capture =
        Layout::from_size_align(callbacks.storage.bytes(), callbacks.storage.alignment()).unwrap();

    let (header, _) = header.extend(capture).unwrap();

    let completion =
        Layout::from_size_align(callbacks.completion_bytes, callbacks.completion_align).unwrap();

    let (layout, completion) = header.extend(completion).unwrap();

    let layout = layout.pad_to_align();

    let storage =
        NativeCleanupStorageShape::new(callbacks.storage.identity(), layout.size(), layout.align());

    let backing = crate::backing::Backing::admit_shape(storage)
        .expect("test frame backing is admitted before publication");

    let address = backing.address();

    let frame = Arc::new(Frame {
        backing,
        descriptor: NativeActivationDescriptor {
            storage,
            header: 0,
            completion,
            completion_bytes: callbacks.completion_bytes,
            completion_align: callbacks.completion_align,
            states: callbacks.states,
            state,
            resume,
            cancel,
            broadcast,
            resolve,
            move_completion,
            destroy,
        },
        callbacks,
        context,
    });

    let mut outgoing = crate::outgoing::OutgoingRecords::admit(4).unwrap();

    let (head, tail, count) = std::mem::take(&mut outgoing).into_parts();

    let mut records = NativeReportRecords { head, tail, count };
    let token = NativeInactiveFrame::new(frame.backing.lease(), &frame.descriptor);

    assert!(
        FRAMES
            .lock()
            .unwrap()
            .insert(address, Arc::clone(&frame))
            .is_none()
    );

    crate::report_provider::bray_runtime_activation_initialization(&token, address, &mut records);

    assert_eq!(
        crate::report_provider::bray_runtime_activation_publication(&token),
        0
    );

    token
}

fn frame(address: usize) -> Arc<Frame> {
    Arc::clone(
        FRAMES
            .lock()
            .unwrap()
            .get(&address)
            .expect("callback retains its admitted fixture"),
    )
}

struct Scope(Option<usize>);
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.set(self.0);
    }
}

fn invoke<T>(address: usize, callback: impl FnOnce(&Frame) -> T) -> T {
    let frame = frame(address);
    let _scope = Scope(CURRENT.replace(Some(address)));

    callback(&frame)
}

pub(super) fn destination() -> usize {
    let address = CURRENT
        .get()
        .expect("composition runs inside a fixture callback");

    let frame = frame(address);

    frame.backing.address() + frame.descriptor.completion
}

pub(super) fn take_completion(outcome: &mut bray_runtime_abi::NativeRunOutcome) {
    let address = CURRENT
        .get()
        .expect("completion belongs to the active parent callback");

    crate::report_provider::bray_runtime_frame_completion_move(
        std::ptr::with_exposed_provenance_mut(address),
        outcome,
    );
}

pub(super) fn compose(frame: NativeInactiveFrame, state: u32) {
    super::export::bray_runtime_awaited_frame_composition(frame, state, destination(), 0)
}

extern "C" fn state(
    address: usize,
    state: u32,
    operation: u32,
    argument: usize,
) -> NativeFrameState {
    let frame = frame(address);

    (frame.callbacks.state)(frame.context, state, operation, argument)
}
extern "C-unwind" fn resume(progress: &mut NativeFrameProgress, address: usize) {
    invoke(address, |frame| {
        (frame.callbacks.resume)(progress, frame.context)
    });
}
extern "C-unwind" fn cancel(progress: &mut NativeFrameProgress, address: usize) {
    invoke(address, |frame| {
        (frame.callbacks.cancel)(progress, frame.context)
    });
}
extern "C-unwind" fn broadcast(progress: &mut NativeFrameProgress, address: usize) {
    invoke(address, |frame| {
        (frame.callbacks.broadcast)(progress, frame.context)
    });
}
extern "C-unwind" fn resolve(
    progress: &mut NativeFrameProgress,
    address: usize,
    exit: NativeFrameExit,
) {
    invoke(address, |frame| {
        (frame.callbacks.resolve)(progress, frame.context, exit)
    });
}
extern "C-unwind" fn move_completion(address: usize, destination: usize) {
    invoke(address, |frame| {
        (frame.callbacks.move_completion)(frame.context, destination)
    });
}
extern "C-unwind" fn destroy(address: usize) {
    let frame = FRAMES
        .lock()
        .unwrap()
        .remove(&address)
        .expect("fixture is destroyed exactly once");

    // The collection lock is released before user callbacks, unwinding or backing release.
    (frame.callbacks.destroy)(frame.context);
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{
        NativeActivationDescriptor, NativeActivationExecution, NativeCleanupStorageShape,
        NativeFrameAffinity, NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
        NativeFrameState, NativeInactiveFrameTransfer, NativeLaneRequirements,
        NativeRuntimeConfiguration,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn activation_queries_only_requested_compiler_states() {
        static QUERIES: AtomicUsize = AtomicUsize::new(0);

        extern "C" fn state(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
            QUERIES.fetch_add(1, Ordering::Relaxed);

            NativeFrameState::new(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE)
        }

        extern "C-unwind" fn resume(progress: &mut NativeFrameProgress, _: usize) {
            *progress = NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 0, 0);
        }

        extern "C-unwind" fn action(_: usize) {}
        extern "C-unwind" fn broadcast(_: &mut NativeFrameProgress, _: usize) {}
        extern "C-unwind" fn resolve(_: &mut NativeFrameProgress, _: usize, _: NativeFrameExit) {}

        extern "C-unwind" fn completion(_: usize, _: usize) {
            unreachable!();
        }

        let _isolation = super::super::state::test_runtime_isolation();

        QUERIES.store(0, Ordering::Relaxed);

        let token = super::admit(
            0,
            NativeActivationDescriptor {
                storage: NativeCleanupStorageShape::new([7; 32], 8, 8),
                header: 0,
                completion: 0,
                completion_bytes: 0,
                completion_align: 1,
                states: 128,
                state,
                resume,
                cancel: resume,
                broadcast,
                resolve,
                move_completion: completion,
                destroy: action,
            },
        );

        let mut execution = NativeActivationExecution::default();

        assert_eq!(
            crate::report_provider::bray_runtime_activation_inspection(
                &token,
                0,
                0,
                0,
                &mut execution
            ),
            0
        );

        assert_eq!(execution.state, 0);
        assert_eq!(QUERIES.load(Ordering::Relaxed), 1);

        assert_eq!(
            crate::report_provider::bray_runtime_activation_inspection(
                &token,
                127,
                0,
                0,
                &mut execution
            ),
            0
        );

        assert_eq!(execution.state, 127);
        assert_eq!(QUERIES.load(Ordering::Relaxed), 2);

        assert_ne!(
            crate::report_provider::bray_runtime_activation_inspection(
                &token,
                128,
                0,
                0,
                &mut execution
            ),
            0
        );

        assert_eq!(QUERIES.load(Ordering::Relaxed), 2);

        let root = super::super::export::bray_runtime_root_execution(
            NativeInactiveFrameTransfer::new(&token),
            NativeRuntimeConfiguration::new(1, 1),
        )
        .root()
        .unwrap();

        let _ = super::super::export::bray_runtime_root_terminal_observation(root);

        assert!(super::super::export::bray_runtime_root_completion_resolution(root).is_success());
        assert!(super::super::export::bray_runtime_structured_shutdown().is_success());
    }
}
