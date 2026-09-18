//! Movable native panic ownership. Report destinations belong to callers.

use crate::NativeRuntimeStatus;

/// Structured cause retained by one native panic report.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePanicCause(u32);

impl NativePanicCause {
    /// An explicit language `panic` expression.
    pub const MESSAGE: Self = Self(0);

    /// A failed built-in assertion.
    pub const ASSERTION: Self = Self(1);

    /// An explicit failure produced by `std.testing.fail`.
    pub const EXPLICIT_TEST_FAILURE: Self = Self(2);

    /// A contained Rust implementation panic.
    pub const RUNTIME_PANIC: Self = Self(3);

    /// Storage required for a report or its outgoing records could not be acquired.
    pub const ALLOCATION_FAILURE: Self = Self(4);

    /// Returns whether this cause is defined by the current native ABI.
    pub const fn is_known(&self) -> bool {
        matches!(self.0, 0..=4)
    }

    /// Returns the stable native ABI code.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Exact source occurrence retained by a native panic report.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeSourceAnchor {
    present: u32,
    source: u32,
    start: u32,
    end: u32,
    version: u64,
}

impl NativeSourceAnchor {
    /// Creates one source anchor from its stable scalar ABI fields.
    pub const fn new(source: u32, start: u32, end: u32, version: u64) -> Self {
        Self {
            present: 1,
            source,
            start,
            end,
            version,
        }
    }

    /// Creates an anchor for generated or imported code without local source coordinates.
    pub const fn unavailable() -> Self {
        Self {
            present: 0,
            source: 0,
            start: 0,
            end: 0,
            version: 0,
        }
    }

    /// Returns whether this anchor carries local source coordinates.
    pub const fn is_available(self) -> bool {
        self.present == 1
    }

    /// Returns the source snapshot identity.
    pub const fn source(self) -> u32 {
        self.source
    }

    /// Returns the inclusive UTF-8 byte start offset.
    pub const fn start(self) -> u32 {
        self.start
    }

    /// Returns the exclusive UTF-8 byte end offset.
    pub const fn end(self) -> u32 {
        self.end
    }

    /// Returns the logical source revision.
    pub const fn version(self) -> u64 {
        self.version
    }

    /// Returns whether the half-open source range is ordered.
    pub const fn is_valid(&self) -> bool {
        match self.present {
            0 => self.source == 0 && self.start == 0 && self.end == 0 && self.version == 0,
            1 => self.start <= self.end,
            _ => false,
        }
    }
}

/// Copies a bounded range of retained message bytes into caller-owned storage.
pub type NativePanicMessageCopy =
    extern "C" fn(usize, usize, *mut u8, usize) -> NativeRuntimeStatus;

/// Releases message backing and publishes any contained disposal failure.
pub type NativePanicMessageRelease = extern "C" fn(usize, usize, &mut crate::NativeRunOutcome);

/// Consumes a report and its detached records, optionally reporting each primary.
pub type NativePanicReportConsumer =
    extern "C" fn(&mut NativePanicReport, bool) -> NativeRuntimeStatus;

/// Immutable message bytes and their owning provider's operations.
#[repr(C)]
#[derive(Debug)]
pub struct NativePanicMessage {
    address: usize,
    length: usize,
    copy: Option<NativePanicMessageCopy>,
    release: Option<NativePanicMessageRelease>,
}

impl NativePanicMessage {
    /// Creates an empty message without backing ownership.
    pub const fn empty() -> Self {
        Self {
            address: 0,
            length: 0,
            copy: None,
            release: None,
        }
    }

    /// Adopts immutable bytes. The provider keeps its callbacks and backing valid until release.
    pub const fn new(
        address: usize,
        length: usize,
        copy: Option<NativePanicMessageCopy>,
        release: Option<NativePanicMessageRelease>,
    ) -> Self {
        Self {
            address,
            length,
            copy,
            release,
        }
    }

    /// Returns the number of retained bytes.
    pub const fn len(&self) -> usize {
        self.length
    }

    /// Returns whether this message has no bytes.
    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Copies a checked range without allocating or transferring ownership.
    pub fn copy_to(&self, offset: usize, destination: &mut [u8]) -> NativeRuntimeStatus {
        if offset > self.length || destination.len() > self.length - offset {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        if destination.is_empty() {
            return NativeRuntimeStatus::SUCCESS;
        }

        self.copy
            .map_or(NativeRuntimeStatus::INVALID_ARGUMENT, |copy| {
                copy(
                    self.address,
                    offset,
                    destination.as_mut_ptr(),
                    destination.len(),
                )
            })
    }

    /// Releases owned backing and returns any failure contained by its provider.
    pub fn release(&mut self) -> crate::NativeRunOutcome {
        let mut outcome = crate::NativeRunOutcome::new(crate::NativeRunState::COMPLETED, 0);

        if let Some(release) = self.release.take() {
            release(self.address, self.length, &mut outcome);
        }

        outcome
    }
}

impl Drop for NativePanicMessage {
    fn drop(&mut self) {
        if self.release.is_some() {
            drop(self.release());
        }
    }
}

/// One inline incident. Detached records contain primaries, never complete reports.
#[repr(C)]
#[derive(Debug)]
pub struct NativePanicPrimary {
    source: NativeSourceAnchor,
    cause: NativePanicCause,
    message: NativePanicMessage,
}

impl NativePanicPrimary {
    /// Adopts a message and its exact structured cause and source occurrence.
    pub const fn new(
        cause: NativePanicCause,
        source: NativeSourceAnchor,
        message: NativePanicMessage,
    ) -> Self {
        Self {
            cause,
            source,
            message,
        }
    }

    /// Creates an empty primary for an unpublished destination.
    pub const fn empty() -> Self {
        Self::new(
            NativePanicCause::MESSAGE,
            NativeSourceAnchor::unavailable(),
            NativePanicMessage::empty(),
        )
    }

    /// Returns the structured cause.
    pub const fn cause(&self) -> NativePanicCause {
        self.cause
    }

    /// Returns the source occurrence.
    pub const fn source(&self) -> NativeSourceAnchor {
        self.source
    }

    /// Transfers opaque message backing back to its matching provider.
    pub fn take_provider_handle(&mut self, provider: NativePanicMessageRelease) -> Option<usize> {
        self.message
            .release
            .filter(|release| std::ptr::fn_addr_eq(*release, provider))?;

        self.message.release = None;

        Some(self.message.address)
    }

    /// Borrows the retained immutable message.
    pub const fn message(&self) -> &NativePanicMessage {
        &self.message
    }

    /// Releases message backing and returns any failure contained by its provider.
    pub fn release_message(&mut self) -> crate::NativeRunOutcome {
        self.message.release()
    }
}

/// Caller-owned report header with one inline primary and a detached record chain.
#[repr(C)]
#[derive(Debug)]
pub struct NativePanicReport {
    primary: NativePanicPrimary,
    head: usize,
    tail: usize,
    count: usize,
    reserved: usize,
    consume: Option<NativePanicReportConsumer>,
}

impl NativePanicReport {
    /// Creates an empty, unpublished report destination.
    pub const fn empty() -> Self {
        Self {
            primary: NativePanicPrimary::empty(),
            head: 0,
            tail: 0,
            count: 0,
            reserved: 0,
            consume: None,
        }
    }

    /// Adopts a primary and the owning runtime's record consumer.
    pub const fn new(primary: NativePanicPrimary, consume: NativePanicReportConsumer) -> Self {
        Self {
            primary,
            head: 0,
            tail: 0,
            count: 0,
            reserved: 0,
            consume: Some(consume),
        }
    }

    /// Transfers detached record ownership into this report.
    pub fn set_outgoing(&mut self, head: usize, tail: usize, count: usize) {
        assert_eq!(
            self.count, 0,
            "report destination must not already own outgoing records"
        );

        self.head = head;
        self.tail = tail;
        self.count = count;
    }

    /// Retains one admitted record for attaching this primary after its producer exits.
    pub fn set_reserved(&mut self, reserved: usize) {
        assert_eq!(
            self.reserved, 0,
            "primary reservation transfers exactly once"
        );

        self.reserved = reserved;
    }

    /// Transfers the primary and detached record identities to their owning consumer.
    pub fn take_parts(&mut self) -> (NativePanicPrimary, usize, usize, usize, usize) {
        self.consume = None;

        (
            std::mem::replace(&mut self.primary, NativePanicPrimary::empty()),
            std::mem::take(&mut self.head),
            std::mem::take(&mut self.tail),
            std::mem::take(&mut self.count),
            std::mem::take(&mut self.reserved),
        )
    }

    /// Consumes live ownership exactly once, reporting it when requested.
    pub fn consume(&mut self, report: bool) -> NativeRuntimeStatus {
        self.consume
            .take()
            .map_or(NativeRuntimeStatus::SUCCESS, |consume| {
                consume(self, report)
            })
    }
}

impl Drop for NativePanicReport {
    fn drop(&mut self) {
        self.consume(false);
    }
}

#[cfg(test)]
mod tests {
    use super::{NativePanicMessage, NativePanicPrimary, NativePanicReport, NativeSourceAnchor};

    #[test]
    fn source_anchor_has_native_layout() {
        assert_abi_layout!(NativeSourceAnchor, size: 24, align: 8, fields: {
            present: 0,
            source: 4,
            start: 8,
            end: 12,
            version: 16,
        });
    }

    #[test]
    fn report_layout_keeps_inline_primary_and_independent_record_ownership() {
        assert_abi_layout!(NativePanicMessage, size: 32, align: 8, fields: {
            address: 0, length: 8, copy: 16, release: 24,
        });

        assert_abi_layout!(NativePanicPrimary, size: 64, align: 8, fields: {
            source: 0, cause: 24, message: 32,
        });

        assert_abi_layout!(NativePanicReport, size: 104, align: 8, fields: {
            primary: 0, head: 64, tail: 72, count: 80, reserved: 88, consume: 96,
        });
    }
}
