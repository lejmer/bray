use std::alloc::{Layout, alloc, dealloc};
use std::mem::{align_of, size_of};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::atomic::AtomicUsize;

use bray_runtime_interface::{
    CHARACTER_UNICODE_DATA_VERSION,
    NativeExecutionLaneResult, NativeFrameProgress, NativeFrameProgressKind, NativeInactiveFrame,
    NativeProtectedFrame, NativeProtectedFrameTransfer, NativeRootHandle, NativeRootStart,
    NativeRunOutcome, NativeRunState, NativeRuntimeConfiguration, NativeRuntimeEventCallback,
    NativeRuntimeStatus, NativeStringView, NativeSynchronousRootCallback, NativeTaskAllocation,
    NativeTaskHandle, NativeWakeCallback,
};

const _: () = assert!(
    char::UNICODE_VERSION.0 == CHARACTER_UNICODE_DATA_VERSION.0
        && char::UNICODE_VERSION.1 == CHARACTER_UNICODE_DATA_VERSION.1
        && char::UNICODE_VERSION.2 == CHARACTER_UNICODE_DATA_VERSION.2
);

use crate::current_run_cancellation_observable;

use super::state::{initialize, runtime_failure, shutdown, with_runtime};

macro_rules! native_export {
    ($item:item) => {
        #[expect(
            unsafe_code,
            reason = "the native runtime artifact requires a stable exported ABI symbol"
        )]
        #[unsafe(no_mangle)]
        $item
    };
}

native_export! {
    pub extern "C" fn bray_runtime_root_execution_v1(
        frame: NativeProtectedFrameTransfer,
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRootStart {
        catch_unwind(AssertUnwindSafe(|| {
            let Some(frame) = take_transferred_frame(frame) else {
                return NativeRootStart::failure(NativeRuntimeStatus::INVALID_ARGUMENT);
            };

            execute_root(frame, configuration)
        }))
            .unwrap_or_else(|_| {
                NativeRootStart::failure(NativeRuntimeStatus::PANICKED)
            })
    }
}

#[derive(Debug)]
struct NativePanicReport {
    message: String,
}

#[derive(Debug)]
struct PropagatedPanicReport(usize);

#[derive(Debug)]
struct NativeMemoryAllocationFailure;

#[derive(Debug)]
struct NativeStringSliceBoundsFailure;

#[derive(Debug)]
struct NativeCharacterInvariantFailure;

native_export! {
    pub extern "C-unwind" fn bray_runtime_memory_allocation_v1(
        bytes: usize,
        alignment: usize,
    ) -> *mut u8 {
        let layout = native_memory_layout(bytes, alignment);

        let pointer = unsafe { alloc(layout) };

        if pointer.is_null() {
            panic_any(NativeMemoryAllocationFailure);
        }

        pointer
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_memory_deallocation_v1(
        pointer: *mut u8,
        bytes: usize,
        alignment: usize,
    ) {
        let layout = native_memory_layout(bytes, alignment);

        let Some(pointer) = std::ptr::NonNull::new(pointer) else {
            panic_any(NativeMemoryAllocationFailure);
        };

        unsafe { dealloc(pointer.as_ptr(), layout) };
    }
}

fn native_memory_layout(bytes: usize, alignment: usize) -> Layout {
    Layout::from_size_align(bytes.max(1), alignment)
        .unwrap_or_else(|_| panic_any(NativeMemoryAllocationFailure))
}

native_export! {
    pub extern "C" fn bray_runtime_string_scalar_count_v1(
        data: *const u8,
        length: usize,
    ) -> usize {
        unsafe { native_utf8(data, length) }.chars().count()
    }
}

native_export! {
    pub extern "C" fn bray_runtime_string_equals_v1(
        left_data: *const u8,
        left_length: usize,
        right_data: *const u8,
        right_length: usize,
    ) -> u8 {
        u8::from(
            (unsafe { native_bytes(left_data, left_length) })
                == (unsafe { native_bytes(right_data, right_length) }),
        )
    }
}

native_export! {
    pub extern "C" fn bray_runtime_string_scalar_at_v1(
        data: *const u8,
        length: usize,
        index: usize,
        scalar: *mut u32,
    ) -> u8 {
        let Some(value) = unsafe { native_utf8(data, length) }.chars().nth(index) else {
            return 0;
        };

        unsafe { scalar.write(u32::from(value)) };

        1
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_string_scalar_slice_v1(
        data: *const u8,
        length: usize,
        start: usize,
        end: usize,
        result_data: *mut *const u8,
        result_length: *mut usize,
        result_owner: *mut *mut u8,
    ) {
        let text = unsafe { native_utf8(data, length) };

        let (Some(start), Some(end)) =
            (scalar_byte_offset(text, start), scalar_byte_offset(text, end))
        else {
            panic_any(NativeStringSliceBoundsFailure);
        };

        if start > end {
            panic_any(NativeStringSliceBoundsFailure);
        }

        let bytes = &text.as_bytes()[start..end];

        write_owned_utf8(bytes, result_data, result_length, result_owner);
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_string_from_utf8_v1(
        data: *const u8,
        length: usize,
        result_data: *mut *const u8,
        result_length: *mut usize,
        result_owner: *mut *mut u8,
    ) -> u8 {
        let bytes = unsafe { native_bytes(data, length) };

        if std::str::from_utf8(bytes).is_err() {
            return 0;
        }

        write_owned_utf8(bytes, result_data, result_length, result_owner);

        1
    }
}

native_export! {
    pub extern "C" fn bray_runtime_string_scalar_cursor_next_v1(
        data: *const u8,
        length: usize,
        index: *mut usize,
        count: usize,
        scalar: *mut u32,
    ) -> u8 {
        let current = unsafe { index.read() };

        if current >= count {
            return 0;
        }

        let Some(value) = unsafe { native_utf8(data, length) }
            .get(current..)
            .and_then(|remaining| remaining.chars().next())
        else {
            return 0;
        };

        unsafe {
            index.write(current + value.len_utf8());
            scalar.write(u32::from(value));
        }

        1
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_scalar_value_v1(value: u32) -> u32 {
        value
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_from_scalar_value_v1(
        value: u32,
        scalar: *mut u32,
    ) -> u8 {
        let Some(character) = char::from_u32(value) else {
            return 0;
        };

        unsafe { scalar.write(u32::from(character)) };

        1
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_utf8_length_v1(value: u32) -> usize {
        native_character(value).len_utf8()
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_is_alphabetic_v1(value: u32) -> u8 {
        u8::from(native_character(value).is_alphabetic())
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_is_numeric_v1(value: u32) -> u8 {
        u8::from(native_character(value).is_numeric())
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_is_whitespace_v1(value: u32) -> u8 {
        u8::from(native_character(value).is_whitespace())
    }
}

fn native_character(value: u32) -> char {
    char::from_u32(value).unwrap_or_else(|| panic_any(NativeCharacterInvariantFailure))
}

#[expect(
    unsafe_code,
    reason = "private native ABI operations borrow caller-provided byte ranges"
)]
unsafe fn native_bytes<'bytes>(data: *const u8, length: usize) -> &'bytes [u8] {
    if length == 0 {
        return &[];
    }

    assert!(!data.is_null());

    unsafe { std::slice::from_raw_parts(data, length) }
}

#[expect(
    unsafe_code,
    reason = "private string operations receive compiler-proven valid UTF-8"
)]
unsafe fn native_utf8<'text>(data: *const u8, length: usize) -> &'text str {
    unsafe { std::str::from_utf8_unchecked(native_bytes(data, length)) }
}

fn scalar_byte_offset(text: &str, index: usize) -> Option<usize> {
    if index == text.chars().count() {
        return Some(text.len());
    }

    text.char_indices().nth(index).map(|(offset, _)| offset)
}

fn write_owned_utf8(
    bytes: &[u8],
    result_data: *mut *const u8,
    result_length: *mut usize,
    result_owner: *mut *mut u8,
) {
    let header = size_of::<AtomicUsize>();

    let total = header
        .checked_add(bytes.len())
        .unwrap_or_else(|| panic_any(NativeMemoryAllocationFailure));

    let owner = bray_runtime_memory_allocation_v1(total, align_of::<AtomicUsize>());

    #[expect(
        unsafe_code,
        reason = "private native ABI operations initialize compiler-owned string storage"
    )]
    unsafe {
        owner.cast::<AtomicUsize>().write(AtomicUsize::new(1));

        let data = owner.add(header);

        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());

        result_data.write(data);
        result_length.write(bytes.len());
        result_owner.write(owner);
    }
}

native_export! {
    pub extern "C" fn bray_runtime_root_cancellation_request_v1(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.request_root_cancellation(root))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_synchronous_root_execution_v1(
        callback: NativeSynchronousRootCallback,
        destination: usize,
    ) -> NativeRunOutcome {
        match catch_unwind(AssertUnwindSafe(|| callback(destination))) {
            Ok(()) => NativeRunOutcome::new(NativeRunState::COMPLETED, destination),
            Err(payload) => payload
                .downcast_ref::<PropagatedPanicReport>()
                .map_or_else(
                    || runtime_failure(NativeRuntimeStatus::PANICKED),
                    |report| NativeRunOutcome::new(NativeRunState::PANICKED, report.0),
                ),
        }
    }
}

native_export! {
    pub extern "C" fn bray_runtime_root_terminal_observation_v1(
        root: NativeRootHandle,
    ) -> NativeRunOutcome {
        catch_unwind(AssertUnwindSafe(|| {
            with_runtime(|runtime| runtime.observe_root(root))
                .unwrap_or_else(runtime_failure)
        }))
        .unwrap_or_else(|_| runtime_failure(NativeRuntimeStatus::PANICKED))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_root_completion_resolution_v1(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.resolve_root_completion(root))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_panic_reporting_v1(
        payload: usize,
    ) -> NativeRuntimeStatus {
        if payload == 0 {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        contain_status(|| {
            let report = unsafe {
                // The construction and propagation roles transfer this exact allocation.
                Box::from_raw(payload as *mut NativePanicReport)
            };

            eprintln!("{}", report.message);

            NativeRuntimeStatus::SUCCESS
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_entry_failure_reporting_v1(
        payload: usize,
        size: usize,
    ) -> NativeRuntimeStatus {
        if payload == 0 && size != 0 {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        contain_status(|| {
            let bytes = if size == 0 {
                &[][..]
            } else {
                unsafe {
                    // The host retains the reported value for lifecycle resolution.
                    std::slice::from_raw_parts(payload as *const u8, size)
                }
            };

            eprintln!("{bytes:02x?}");

            NativeRuntimeStatus::SUCCESS
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_panic_report_construction_v1(
        message: NativeStringView,
    ) -> usize {
        catch_unwind(AssertUnwindSafe(|| {
            if message.length() != 0 && message.data().is_null() {
                return 0;
            }

            let bytes = if message.length() == 0 {
                &[][..]
            } else {
                unsafe {
                    // The view is borrowed only for this construction call.
                    std::slice::from_raw_parts(message.data(), message.length())
                }
            };

            Box::into_raw(Box::new(NativePanicReport {
                message: String::from_utf8_lossy(bytes).into_owned(),
            })) as usize
        }))
        .unwrap_or(0)
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_panic_propagation_v1(
        payload: usize,
    ) -> ! {
        panic_any(PropagatedPanicReport(payload))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_cleanup_incident_reporting_v1(
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.report_cleanup_incidents())
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_main_thread_lane_startup_v1(
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRuntimeStatus {
        contain_status(|| initialize(configuration))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_main_thread_lane_drive_v1() -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.drive_main_thread())
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_allocation_v1() -> NativeTaskAllocation {
        catch_unwind(AssertUnwindSafe(|| {
            with_runtime(|runtime| runtime.allocate()).unwrap_or_else(
                NativeTaskAllocation::failure,
            )
        }))
        .unwrap_or_else(|_| {
            NativeTaskAllocation::failure(NativeRuntimeStatus::PANICKED)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_start_v1(
        task: NativeTaskHandle,
        frame: NativeProtectedFrameTransfer,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            let Some(frame) = take_transferred_frame(frame) else {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            };

            with_runtime(|runtime| runtime.start(task, frame))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_awaited_frame_composition_v1(
        frame: NativeInactiveFrame,
    ) {
        let status = with_runtime(|runtime| runtime.compose_awaited(frame))
            .unwrap_or_else(|status| status);

        assert!(status.is_success(), "awaited-frame composition failed");
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_frame_completion_move_v1() -> usize {
        with_runtime(|runtime| runtime.resolve_awaited_completion())
            .and_then(|result| result)
            .unwrap_or_else(|_| panic!("awaited-frame completion resolution failed"))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_suspension_registration_v1(
        state: u32,
    ) -> NativeFrameProgress {
        NativeFrameProgress::new(
            NativeFrameProgressKind::SUSPENDED,
            state,
            0,
        )
    }
}

native_export! {
    pub extern "C" fn bray_runtime_wake_v1(
        task: NativeTaskHandle,
        state: u32,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.wake(task, state))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_cancellation_request_v1(
        task: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.request_cancellation(task))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_current_run_cancellation_observation_v1() -> u8 {
        catch_unwind(AssertUnwindSafe(current_run_cancellation_observable))
            .map(u8::from)
            .unwrap_or(0)
    }
}

native_export! {
    pub extern "C" fn bray_runtime_join_registration_v1(
        task: NativeTaskHandle,
        callback: NativeWakeCallback,
        context: usize,
    ) -> NativeRunOutcome {
        catch_unwind(AssertUnwindSafe(|| {
            with_runtime(|runtime| {
                runtime.join(task, callback, context)
            })
            .unwrap_or_else(runtime_failure)
        }))
        .unwrap_or_else(|_| runtime_failure(NativeRuntimeStatus::PANICKED))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_terminal_publication_v1(
        state: NativeRunState,
        payload: usize,
    ) -> NativeFrameProgress {
        if state == NativeRunState::COMPLETED {
            return NativeFrameProgress::new(
                NativeFrameProgressKind::COMPLETED,
                0,
                payload,
            );
        }

        if state == NativeRunState::CANCELLED {
            return NativeFrameProgress::new(
                NativeFrameProgressKind::CANCELLED,
                0,
                0,
            );
        }

        if state == NativeRunState::PANICKED {
            return NativeFrameProgress::new(
                NativeFrameProgressKind::PANICKED,
                0,
                payload,
            );
        }

        NativeFrameProgress::new(
            NativeFrameProgressKind::RUNTIME_FAILURE,
            0,
            0,
        )
    }
}

native_export! {
    pub extern "C" fn bray_runtime_event_v1(
        callback: NativeRuntimeEventCallback,
        context: usize,
    ) -> NativeRuntimeStatus {
        contain_status(|| callback(context))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_compatible_lane_selection_v1(
        task: NativeTaskHandle,
        state: u32,
    ) -> NativeExecutionLaneResult {
        catch_unwind(AssertUnwindSafe(|| {
            with_runtime(|runtime| runtime.lane(task, state)).unwrap_or_else(
                NativeExecutionLaneResult::failure,
            )
        }))
        .unwrap_or_else(|_| {
            NativeExecutionLaneResult::failure(NativeRuntimeStatus::PANICKED)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_structured_shutdown_v1() -> NativeRuntimeStatus {
        contain_status(shutdown)
    }
}

fn contain_status(callback: impl FnOnce() -> NativeRuntimeStatus) -> NativeRuntimeStatus {
    catch_unwind(AssertUnwindSafe(callback)).unwrap_or(NativeRuntimeStatus::PANICKED)
}

#[expect(
    unsafe_code,
    reason = "the native ownership-transfer ABI exposes a validated descriptor address"
)]
fn take_transferred_frame(transfer: NativeProtectedFrameTransfer) -> Option<NativeProtectedFrame> {
    let address = transfer.address();

    if address == 0 || !address.is_multiple_of(align_of::<NativeProtectedFrame>()) {
        return None;
    }

    let frame = unsafe {
        // The native ABI requires the caller to keep this descriptor live for
        // the call and transfers its generated context to this copied value.
        (address as *const NativeProtectedFrame).read()
    };

    Some(frame)
}

fn execute_root(
    frame: NativeProtectedFrame,
    configuration: NativeRuntimeConfiguration,
) -> NativeRootStart {
    let status = initialize(configuration);

    if !status.is_success() {
        return NativeRootStart::failure(status);
    }

    let allocation =
        with_runtime(|runtime| runtime.allocate()).unwrap_or_else(NativeTaskAllocation::failure);

    let Some(task) = allocation.task() else {
        return NativeRootStart::failure(allocation.status());
    };

    let status = with_runtime(|runtime| runtime.start(task, frame)).unwrap_or_else(|status| status);

    if !status.is_success() {
        return NativeRootStart::failure(status);
    }

    NativeRootHandle::new(task.raw()).map_or_else(
        || NativeRootStart::failure(NativeRuntimeStatus::RUNTIME_FAILURE),
        NativeRootStart::success,
    )
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, size_of};
    use std::panic::catch_unwind;
    use std::ptr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_interface::{
        CHARACTER_UNICODE_DATA_VERSION,
        NativeFrameAffinity, NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
        NativeFrameState, NativeLaneRequirements, NativeProtectedFrame,
        NativeProtectedFrameTransfer, NativeRunState, NativeRuntimeConfiguration,
        NativeRuntimeStatus, NativeStringView,
    };

    use super::{
        bray_runtime_character_from_scalar_value_v1,
        bray_runtime_character_is_alphabetic_v1, bray_runtime_character_is_numeric_v1,
        bray_runtime_character_is_whitespace_v1, bray_runtime_character_scalar_value_v1,
        bray_runtime_character_utf8_length_v1,
        bray_runtime_join_registration_v1, bray_runtime_main_thread_lane_drive_v1,
        bray_runtime_main_thread_lane_startup_v1, bray_runtime_memory_allocation_v1,
        bray_runtime_memory_deallocation_v1, bray_runtime_root_completion_resolution_v1,
        bray_runtime_root_execution_v1, bray_runtime_root_terminal_observation_v1,
        bray_runtime_structured_shutdown_v1, bray_runtime_synchronous_root_execution_v1,
        bray_runtime_string_equals_v1, bray_runtime_string_from_utf8_v1,
        bray_runtime_string_scalar_at_v1, bray_runtime_string_scalar_count_v1,
        bray_runtime_string_scalar_cursor_next_v1, bray_runtime_string_scalar_slice_v1,
        bray_runtime_task_allocation_v1, bray_runtime_task_start_v1,
    };

    static DESTROYED: AtomicUsize = AtomicUsize::new(0);
    static ROOT_DESTROYED: AtomicUsize = AtomicUsize::new(0);
    static ROOT_COMPLETION_DESTINATION: AtomicUsize = AtomicUsize::new(0);
    static REJECTED_DESTROYED: AtomicUsize = AtomicUsize::new(0);
    static STARTED_DESTROYED: AtomicUsize = AtomicUsize::new(0);
    static SUSPENDED_ROOT: AtomicUsize = AtomicUsize::new(0);
    static SUSPENDED_RESUMES: AtomicUsize = AtomicUsize::new(0);
    static CANCEL_ENTRIES: AtomicUsize = AtomicUsize::new(0);
    static FAILURE_ROOT: AtomicUsize = AtomicUsize::new(0);
    static FAILURE_RESUMES: AtomicUsize = AtomicUsize::new(0);
    static FAILURE_BROADCASTS: AtomicUsize = AtomicUsize::new(0);
    static FAILURE_RESOLUTIONS: AtomicUsize = AtomicUsize::new(0);
    static FAILURE_DESTRUCTIONS: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn native_memory_allocation_obeys_empty_and_nonempty_layout_contracts() {
        for (bytes, alignment) in [(0, 1), (32, 16)] {
            let address = bray_runtime_memory_allocation_v1(bytes, alignment);

            assert!(!address.is_null());
            assert_eq!(address.addr() % alignment, 0);

            bray_runtime_memory_deallocation_v1(address, bytes, alignment);
        }
    }

    #[test]
    fn utf8_text_operations_use_scalar_indices_and_preserve_bytes() {
        let text = "Aé🙂";

        assert_eq!(
            bray_runtime_string_scalar_count_v1(text.as_ptr(), text.len()),
            3
        );

        assert_eq!(
            bray_runtime_string_equals_v1(
                text.as_ptr(),
                text.len(),
                "Aé🙂".as_ptr(),
                "Aé🙂".len(),
            ),
            1
        );

        assert_eq!(
            bray_runtime_string_equals_v1(text.as_ptr(), text.len(), b"other".as_ptr(), 5),
            0
        );

        let mut scalar = 0;

        assert_eq!(
            bray_runtime_string_scalar_at_v1(text.as_ptr(), text.len(), 1, &raw mut scalar),
            1
        );

        assert_eq!(scalar, u32::from('é'));

        assert_eq!(
            bray_runtime_string_scalar_at_v1(text.as_ptr(), text.len(), 3, &raw mut scalar),
            0
        );
    }

    #[test]
    fn scalar_slicing_returns_owned_valid_utf8() {
        let text = "Aé🙂Z";
        let mut data = ptr::null();
        let mut length = 0;
        let mut owner = ptr::null_mut();

        bray_runtime_string_scalar_slice_v1(
            text.as_ptr(),
            text.len(),
            1,
            3,
            &raw mut data,
            &raw mut length,
            &raw mut owner,
        );

        #[expect(
            unsafe_code,
            reason = "the test verifies bytes returned through the private native ABI"
        )]
        let result = unsafe { std::slice::from_raw_parts(data, length) };

        assert_eq!(result, "é🙂".as_bytes());

        release_owned_text(owner, length);

        assert!(
            catch_unwind(|| {
                let mut data = ptr::null();
                let mut length = 0;
                let mut owner = ptr::null_mut();

                bray_runtime_string_scalar_slice_v1(
                    text.as_ptr(),
                    text.len(),
                    3,
                    2,
                    &raw mut data,
                    &raw mut length,
                    &raw mut owner,
                );
            })
            .is_err()
        );
    }

    #[test]
    fn utf8_conversion_rejects_invalid_bytes_without_allocating() {
        let valid = "Grüße".as_bytes();
        let mut data = ptr::null();
        let mut length = 0;
        let mut owner = ptr::null_mut();

        assert_eq!(
            bray_runtime_string_from_utf8_v1(
                valid.as_ptr(),
                valid.len(),
                &raw mut data,
                &raw mut length,
                &raw mut owner,
            ),
            1
        );

        #[expect(
            unsafe_code,
            reason = "the test verifies bytes returned through the private native ABI"
        )]
        let result = unsafe { std::slice::from_raw_parts(data, length) };

        assert_eq!(result, valid);

        release_owned_text(owner, length);

        let invalid = [0xf0, 0x28, 0x8c, 0x28];
        let mut data = ptr::null();
        let mut length = 0;
        let mut owner = ptr::null_mut();

        assert_eq!(
            bray_runtime_string_from_utf8_v1(
                invalid.as_ptr(),
                invalid.len(),
                &raw mut data,
                &raw mut length,
                &raw mut owner,
            ),
            0
        );

        assert!(data.is_null());
        assert_eq!(length, 0);
        assert!(owner.is_null());
    }

    #[test]
    fn scalar_cursor_advances_once_and_has_stable_exhaustion() {
        let text = "é🙂";
        let mut index = 0;
        let mut scalar = 0;

        for expected in ['é', '🙂'] {
            assert_eq!(
                bray_runtime_string_scalar_cursor_next_v1(
                    text.as_ptr(),
                    text.len(),
                    &raw mut index,
                    text.len(),
                    &raw mut scalar,
                ),
                1
            );

            assert_eq!(scalar, u32::from(expected));
        }

        assert_eq!(
            bray_runtime_string_scalar_cursor_next_v1(
                text.as_ptr(),
                text.len(),
                &raw mut index,
                text.len(),
                &raw mut scalar,
            ),
            0
        );

        assert_eq!(index, text.len());

        assert_eq!(
            bray_runtime_string_scalar_cursor_next_v1(
                text.as_ptr(),
                text.len(),
                &raw mut index,
                text.len(),
                &raw mut scalar,
            ),
            0
        );

        assert_eq!(index, text.len());
    }

    #[test]
    fn character_operations_follow_unicode_scalar_semantics() {
        assert_eq!(char::UNICODE_VERSION, CHARACTER_UNICODE_DATA_VERSION);

        let character = u32::from('٣');

        assert_eq!(bray_runtime_character_scalar_value_v1(character), character);
        assert_eq!(bray_runtime_character_utf8_length_v1(character), 2);
        assert_eq!(bray_runtime_character_is_alphabetic_v1(character), 0);
        assert_eq!(bray_runtime_character_is_numeric_v1(character), 1);
        assert_eq!(bray_runtime_character_is_whitespace_v1(character), 0);
        assert_eq!(bray_runtime_character_is_whitespace_v1(u32::from('\u{2003}')), 1);

        let mut scalar = 0;

        assert_eq!(
            bray_runtime_character_from_scalar_value_v1(character, &raw mut scalar),
            1
        );

        assert_eq!(scalar, character);

        assert_eq!(
            bray_runtime_character_from_scalar_value_v1(0x11_0000, &raw mut scalar),
            0
        );
    }

    fn release_owned_text(owner: *mut u8, length: usize) {
        bray_runtime_memory_deallocation_v1(
            owner,
            size_of::<AtomicUsize>() + length,
            align_of::<AtomicUsize>(),
        );
    }

    #[test]
    fn root_execution_moves_completion_before_frame_destruction() {
        ROOT_DESTROYED.store(0, Ordering::Relaxed);
        ROOT_COMPLETION_DESTINATION.store(0, Ordering::Relaxed);

        let start = execute_test_root(
            protected_frame(
                8,
                resume_frame,
                record_root_completion_destination,
                root_destroy,
            ),
            NativeRuntimeConfiguration::new(2, 1),
        );

        let Some(root) = start.root() else {
            panic!("root frame must transfer");
        };

        let outcome = bray_runtime_root_terminal_observation_v1(root);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_ne!(outcome.payload(), 0);

        assert_eq!(
            outcome.payload(),
            ROOT_COMPLETION_DESTINATION.load(Ordering::Relaxed)
        );

        assert_eq!(ROOT_DESTROYED.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::UNKNOWN_TASK
        );

        assert_eq!(
            bray_runtime_task_allocation_v1().status(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(ROOT_DESTROYED.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn root_terminal_observation_waits_for_real_suspend_and_resume() {
        SUSPENDED_RESUMES.store(0, Ordering::Relaxed);

        let start = execute_test_root(
            protected_frame(
                8,
                suspend_and_self_wake,
                ignore_completion_move,
                ignore_action,
            ),
            NativeRuntimeConfiguration::new(2, 1),
        );

        let Some(root) = start.root() else {
            panic!("suspending root must transfer");
        };

        SUSPENDED_ROOT.store(
            usize::try_from(root.raw()).unwrap_or_else(|_| panic!("test root must fit usize")),
            Ordering::Relaxed,
        );

        let outcome = bray_runtime_root_terminal_observation_v1(root);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(SUSPENDED_RESUMES.load(Ordering::Relaxed), 2);

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn host_cancellation_wakes_a_suspended_root_into_cancellation_entry() {
        CANCEL_ENTRIES.store(0, Ordering::Relaxed);

        let frame = NativeProtectedFrame::new(
            0,
            [8; 32],
            2,
            8,
            8,
            8,
            8,
            frame_state,
            suspend_without_wake,
            record_cancel_entry,
            ignore_action,
            ignore_resolution,
            ignore_completion_move,
            ignore_action,
        );

        let start = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1));

        let Some(root) = start.root() else {
            panic!("cancellable root must transfer");
        };

        assert_eq!(
            bray_runtime_main_thread_lane_drive_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            super::bray_runtime_root_cancellation_request_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        let outcome = bray_runtime_root_terminal_observation_v1(root);

        assert_eq!(outcome.state(), NativeRunState::CANCELLED);
        assert_eq!(CANCEL_ENTRIES.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn cleanup_callback_failures_are_owned_until_host_drain() {
        let frame = NativeProtectedFrame::new(
            0,
            [9; 32],
            1,
            8,
            8,
            8,
            8,
            frame_state,
            resume_frame,
            cancel_frame,
            panic_action,
            panic_resolution,
            ignore_completion_move,
            panic_action,
        );

        let start = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1));

        let Some(root) = start.root() else {
            panic!("cleanup-failing root must transfer");
        };

        assert_eq!(
            bray_runtime_root_terminal_observation_v1(root).state(),
            NativeRunState::COMPLETED
        );

        let pending = super::with_runtime(|runtime| runtime.pending_cleanup_incidents())
            .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

        assert_eq!(pending, 3);

        assert_eq!(
            super::bray_runtime_cleanup_incident_reporting_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        let pending = super::with_runtime(|runtime| runtime.pending_cleanup_incidents())
            .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

        assert_eq!(pending, 0);

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn runtime_failure_after_suspension_resolves_the_retained_frame() {
        FAILURE_RESUMES.store(0, Ordering::Relaxed);
        FAILURE_BROADCASTS.store(0, Ordering::Relaxed);
        FAILURE_RESOLUTIONS.store(0, Ordering::Relaxed);
        FAILURE_DESTRUCTIONS.store(0, Ordering::Relaxed);

        let frame = NativeProtectedFrame::new(
            0,
            [10; 32],
            2,
            8,
            8,
            8,
            8,
            frame_state,
            suspend_then_fail,
            cancel_frame,
            record_failure_broadcast,
            record_failure_resolution,
            ignore_completion_move,
            record_failure_destruction,
        );

        let start = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1));

        let Some(root) = start.root() else {
            panic!("failure-path root must transfer");
        };

        FAILURE_ROOT.store(
            usize::try_from(root.raw()).unwrap_or_else(|_| panic!("test root must fit usize")),
            Ordering::Relaxed,
        );

        assert_eq!(
            bray_runtime_root_terminal_observation_v1(root).state(),
            NativeRunState::RUNTIME_FAILURE
        );

        assert_eq!(FAILURE_RESUMES.load(Ordering::Relaxed), 2);
        assert_eq!(FAILURE_BROADCASTS.load(Ordering::Relaxed), 1);
        assert_eq!(FAILURE_RESOLUTIONS.load(Ordering::Relaxed), 1);
        assert_eq!(FAILURE_DESTRUCTIONS.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn native_runtime_boundary_starts_executes_and_shuts_down() {
        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(NativeRuntimeConfiguration::new(8, 8)),
            NativeRuntimeStatus::SUCCESS
        );

        let allocation = bray_runtime_task_allocation_v1();

        let Some(task) = allocation.task() else {
            panic!("native frame must allocate");
        };

        assert_eq!(start_test_task(task, frame()), NativeRuntimeStatus::SUCCESS);

        assert_eq!(
            bray_runtime_main_thread_lane_drive_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        let outcome = bray_runtime_join_registration_v1(task, ignore_wake, 0);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_ne!(outcome.payload(), 0);

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(DESTROYED.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn native_runtime_boundary_validates_state_and_capacities() {
        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::NOT_INITIALIZED
        );

        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(NativeRuntimeConfiguration::new(0, 1)),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_task_allocation_v1().status(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_task_allocation_v1().status(),
            NativeRuntimeStatus::RUNTIME_FAILURE
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn task_start_transfers_each_frame_exactly_once() {
        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let allocation = bray_runtime_task_allocation_v1();

        let Some(task) = allocation.task() else {
            panic!("native task storage must allocate");
        };

        assert_eq!(
            start_test_task(
                task,
                protected_frame(0, resume_frame, ignore_completion_move, rejected_destroy,)
            ),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(REJECTED_DESTROYED.load(Ordering::Relaxed), 1);

        assert_eq!(
            start_test_task(
                task,
                protected_frame(8, resume_frame, ignore_completion_move, started_destroy,)
            ),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_main_thread_lane_drive_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_join_registration_v1(task, ignore_wake, 0).state(),
            NativeRunState::COMPLETED
        );

        assert_eq!(STARTED_DESTROYED.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn frame_contract_failures_are_not_published_as_panics() {
        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        for resume in [
            resume_runtime_failure as extern "C-unwind" fn(usize) -> NativeFrameProgress,
            resume_unknown_progress,
        ] {
            let allocation = bray_runtime_task_allocation_v1();

            let Some(task) = allocation.task() else {
                panic!("native task storage must allocate");
            };

            assert_eq!(
                start_test_task(
                    task,
                    protected_frame(8, resume, ignore_completion_move, ignore_action,)
                ),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(
                bray_runtime_main_thread_lane_drive_v1(),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );

            assert_eq!(
                bray_runtime_join_registration_v1(task, ignore_wake, 0).state(),
                NativeRunState::RUNTIME_FAILURE
            );
        }

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    fn frame() -> NativeProtectedFrame {
        protected_frame(8, resume_frame, ignore_completion_move, destroy_frame)
    }

    fn execute_test_root(
        frame: NativeProtectedFrame,
        configuration: NativeRuntimeConfiguration,
    ) -> super::NativeRootStart {
        let transfer = NativeProtectedFrameTransfer::new(&frame);

        bray_runtime_root_execution_v1(transfer, configuration)
    }

    fn start_test_task(
        task: super::NativeTaskHandle,
        frame: NativeProtectedFrame,
    ) -> NativeRuntimeStatus {
        let transfer = NativeProtectedFrameTransfer::new(&frame);

        bray_runtime_task_start_v1(task, transfer)
    }

    #[test]
    fn synchronous_root_boundary_catches_reports_and_resolves_panic() {
        let outcome = bray_runtime_synchronous_root_execution_v1(propagate_test_panic, 0);

        assert_eq!(outcome.state(), NativeRunState::PANICKED);
        assert_ne!(outcome.payload(), 0);

        assert_eq!(
            super::bray_runtime_panic_reporting_v1(outcome.payload()),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn entry_failure_reporting_borrows_the_complete_payload() {
        let payload = 42_i32;

        assert_eq!(
            super::bray_runtime_entry_failure_reporting_v1(
                (&raw const payload).addr(),
                size_of::<i32>(),
            ),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            super::bray_runtime_entry_failure_reporting_v1(0, size_of::<i32>()),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );
    }

    fn protected_frame(
        alignment: usize,
        resume: extern "C-unwind" fn(usize) -> NativeFrameProgress,
        move_completion: extern "C-unwind" fn(usize, usize),
        destroy: extern "C-unwind" fn(usize),
    ) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            0,
            [7; 32],
            2,
            8,
            alignment,
            8,
            8,
            frame_state,
            resume,
            cancel_frame,
            ignore_action,
            ignore_resolution,
            move_completion,
            destroy,
        )
    }

    extern "C" fn frame_state(_: usize, _: u32) -> NativeFrameState {
        NativeFrameState::new(
            NativeFrameAffinity::MAIN_THREAD,
            NativeLaneRequirements::MAIN_THREAD,
        )
    }

    extern "C-unwind" fn resume_frame(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17)
    }

    extern "C-unwind" fn resume_runtime_failure(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0)
    }

    extern "C-unwind" fn resume_unknown_progress(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::from_code(u32::MAX), 0, 0)
    }

    extern "C-unwind" fn cancel_frame(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 0, 0)
    }

    extern "C-unwind" fn propagate_test_panic(_: usize) {
        const MESSAGE: &[u8] = b"synchronous root panic";

        let report = super::bray_runtime_panic_report_construction_v1(NativeStringView::new(
            MESSAGE.as_ptr(),
            MESSAGE.len(),
        ));

        super::bray_runtime_panic_propagation_v1(report)
    }

    extern "C-unwind" fn suspend_and_self_wake(_: usize) -> NativeFrameProgress {
        let resume = SUSPENDED_RESUMES.fetch_add(1, Ordering::Relaxed);

        if resume == 0 {
            let raw = SUSPENDED_ROOT.load(Ordering::Relaxed);

            let raw = u64::try_from(raw).unwrap_or_else(|_| panic!("test root must fit u64"));

            let task = bray_runtime_interface::NativeTaskHandle::new(raw)
                .unwrap_or_else(|| panic!("test root task must be nonzero"));

            assert_eq!(
                super::bray_runtime_wake_v1(task, 1),
                NativeRuntimeStatus::SUCCESS
            );

            return NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
        }

        resume_frame(0)
    }

    extern "C-unwind" fn suspend_without_wake(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0)
    }

    extern "C-unwind" fn suspend_then_fail(_: usize) -> NativeFrameProgress {
        if FAILURE_RESUMES.fetch_add(1, Ordering::Relaxed) == 0 {
            let raw = FAILURE_ROOT.load(Ordering::Relaxed);

            let raw = u64::try_from(raw).unwrap_or_else(|_| panic!("test root must fit u64"));

            let task = bray_runtime_interface::NativeTaskHandle::new(raw)
                .unwrap_or_else(|| panic!("test root task must be nonzero"));

            assert_eq!(
                super::bray_runtime_wake_v1(task, 1),
                NativeRuntimeStatus::SUCCESS
            );

            return NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
        }

        resume_runtime_failure(0)
    }

    extern "C-unwind" fn record_cancel_entry(_: usize) -> NativeFrameProgress {
        CANCEL_ENTRIES.fetch_add(1, Ordering::Relaxed);

        cancel_frame(0)
    }

    extern "C-unwind" fn ignore_action(_: usize) {}

    extern "C-unwind" fn panic_action(_: usize) {
        panic!("cleanup action failed");
    }

    extern "C" fn ignore_wake(_: usize) {}

    extern "C-unwind" fn ignore_resolution(_: usize, _: NativeFrameExit) {}

    extern "C-unwind" fn panic_resolution(_: usize, _: NativeFrameExit) {
        panic!("cleanup resolution failed");
    }

    extern "C-unwind" fn record_failure_broadcast(_: usize) {
        FAILURE_BROADCASTS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn record_failure_resolution(_: usize, _: NativeFrameExit) {
        FAILURE_RESOLUTIONS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn record_failure_destruction(_: usize) {
        FAILURE_DESTRUCTIONS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn destroy_frame(_: usize) {
        DESTROYED.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn record_root_completion_destination(_: usize, destination: usize) {
        ROOT_COMPLETION_DESTINATION.store(destination, Ordering::Relaxed);
    }

    extern "C-unwind" fn ignore_completion_move(_: usize, _: usize) {}

    extern "C-unwind" fn root_destroy(_: usize) {
        ROOT_DESTROYED.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn rejected_destroy(_: usize) {
        REJECTED_DESTROYED.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn started_destroy(_: usize) {
        STARTED_DESTROYED.fetch_add(1, Ordering::Relaxed);
    }
}
