use crate::{
    NativeBrayCallOutcome, NativeInactiveFrame, NativePanicReportCallbacks, NativeRuntimeStatus,
};

/// How a compiler-generated cleanup operation reaches completion.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeCleanupExecution(u32);

impl NativeCleanupExecution {
    /// The owner has no operation for this cleanup step.
    pub const NONE: Self = Self(0);
    /// Cleanup completes during its start callback.
    pub const SYNCHRONOUS: Self = Self(1);
    /// Cleanup produces an inactive protected frame.
    pub const ASYNCHRONOUS: Self = Self(2);

    /// Returns whether the value belongs to this ABI version.
    pub const fn is_known(self) -> bool {
        matches!(self.0, 0..=2)
    }

    /// Returns the stable integer representation.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Starts cleanup over the value at the first address and publishes its checked outcome.
/// An asynchronous callback replaces the supplied inactive frame on success.
pub type NativeValueCleanupCallback = extern "C-unwind" fn(
    usize,
    &mut NativeInactiveFrame,
    &mut NativeBrayCallOutcome,
) -> NativeRuntimeStatus;

/// Execution mode and panic ownership for cleanup of a concrete erased value.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeValueCleanup {
    execution: NativeCleanupExecution,
    start: NativeValueCleanupCallback,
    panics: NativePanicReportCallbacks,
}

/// Synchronous destruction of a retained task completion and ownership of a terminal panic.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeTaskTerminalCleanup {
    value: Option<&'static NativeValueCleanup>,
    panics: NativePanicReportCallbacks,
}

impl NativeTaskTerminalCleanup {
    /// Creates terminal cleanup when completion destruction finishes synchronously.
    pub const fn try_new(
        value: Option<&'static NativeValueCleanup>,
        panics: NativePanicReportCallbacks,
    ) -> Option<Self> {
        let cleanup = Self { value, panics };

        if cleanup.is_valid() {
            Some(cleanup)
        } else {
            None
        }
    }

    /// Returns whether the completion can be destroyed without starting another frame.
    pub const fn is_valid(self) -> bool {
        match self.value {
            None => true,
            Some(value) => matches!(
                value.execution(),
                NativeCleanupExecution::NONE | NativeCleanupExecution::SYNCHRONOUS
            ),
        }
    }

    /// Returns the completion destructor, or no callback for a trivial completion.
    pub const fn value(self) -> Option<&'static NativeValueCleanup> {
        self.value
    }

    /// Returns ownership operations for the task's terminal panic report.
    pub const fn panics(self) -> NativePanicReportCallbacks {
        self.panics
    }
}

#[cfg(test)]
mod tests {
    use std::mem::offset_of;

    use super::{NativeCleanupExecution, NativeTaskTerminalCleanup, NativeValueCleanup};

    #[test]
    fn value_cleanup_layout_retains_execution_and_panic_ownership() {
        let word = size_of::<usize>();
        let callback_offset = size_of::<u32>().next_multiple_of(align_of::<usize>());

        assert_eq!(offset_of!(NativeValueCleanup, execution), 0);
        assert_eq!(offset_of!(NativeValueCleanup, start), callback_offset);

        assert_eq!(
            offset_of!(NativeValueCleanup, panics),
            callback_offset + word
        );

        assert_eq!(
            size_of::<NativeValueCleanup>(),
            callback_offset + word + size_of::<crate::NativePanicReportCallbacks>()
        );

        assert_eq!(offset_of!(NativeTaskTerminalCleanup, value), 0);
        assert_eq!(offset_of!(NativeTaskTerminalCleanup, panics), word);

        assert_eq!(
            size_of::<NativeTaskTerminalCleanup>(),
            word + size_of::<crate::NativePanicReportCallbacks>()
        );
    }

    #[test]
    fn terminal_task_cleanup_requires_synchronous_destruction() {
        extern "C-unwind" fn start(
            _: usize,
            _: &mut crate::NativeInactiveFrame,
            _: &mut crate::NativeBrayCallOutcome,
        ) -> crate::NativeRuntimeStatus {
            crate::NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn report(_: usize) -> crate::NativeRuntimeStatus {
            crate::NativeRuntimeStatus::SUCCESS
        }

        extern "C" fn construct(_: &crate::NativeCleanupIncident) -> usize {
            2
        }

        extern "C" fn attach(primary: usize, _: usize) -> usize {
            primary
        }

        const PANICS: crate::NativePanicReportCallbacks =
            crate::NativePanicReportCallbacks::new(report, report, construct, attach);

        static SYNC: NativeValueCleanup =
            NativeValueCleanup::new(NativeCleanupExecution::SYNCHRONOUS, start, PANICS);

        static ASYNC: NativeValueCleanup =
            NativeValueCleanup::new(NativeCleanupExecution::ASYNCHRONOUS, start, PANICS);

        assert!(
            NativeTaskTerminalCleanup::try_new(None, PANICS)
                .unwrap()
                .is_valid()
        );

        assert!(
            NativeTaskTerminalCleanup::try_new(Some(&SYNC), PANICS)
                .unwrap()
                .is_valid()
        );

        assert!(NativeTaskTerminalCleanup::try_new(Some(&ASYNC), PANICS).is_none());

        assert!(
            !NativeTaskTerminalCleanup {
                value: Some(&ASYNC),
                panics: PANICS
            }
            .is_valid()
        );
    }
}

impl NativeValueCleanup {
    /// Creates a cleanup contract. The callback must implement the selected execution mode.
    pub const fn new(
        execution: NativeCleanupExecution,
        start: NativeValueCleanupCallback,
        panics: NativePanicReportCallbacks,
    ) -> Self {
        Self {
            execution,
            start,
            panics,
        }
    }

    /// Returns how the cleanup operation reaches completion.
    pub const fn execution(self) -> NativeCleanupExecution {
        self.execution
    }

    /// Returns the checked callback over the erased value.
    pub const fn start(self) -> NativeValueCleanupCallback {
        self.start
    }

    /// Returns ownership operations for a panic produced by cleanup.
    pub const fn panics(self) -> NativePanicReportCallbacks {
        self.panics
    }
}
