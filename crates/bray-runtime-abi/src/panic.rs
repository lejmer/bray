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

    /// Native entry or terminal cleanup failed without a transferable detailed report.
    pub const ENTRY_FAILURE: Self = Self(5);

    /// A requested memory layout cannot fit the target address size.
    pub const MEMORY_LAYOUT_SIZE_OVERFLOW: Self = Self(6);

    /// A requested memory layout requires unsupported alignment.
    pub const MEMORY_LAYOUT_UNSUPPORTED_ALIGNMENT: Self = Self(7);

    /// Returns whether this cause is defined by the current native ABI.
    pub const fn is_known(&self) -> bool {
        matches!(self.0, 0..=7)
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
    namespace: [u8; 32],
}

impl NativeSourceAnchor {
    /// Creates one source anchor from its stable scalar ABI fields.
    pub const fn new(namespace: [u8; 32], source: u32, start: u32, end: u32, version: u64) -> Self {
        Self {
            present: 1,
            source,
            start,
            end,
            version,
            namespace,
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
            namespace: [0; 32],
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

    /// Returns the product namespace that qualifies the source ID.
    pub const fn namespace(self) -> [u8; 32] {
        self.namespace
    }

    /// Returns whether the half-open source range is ordered.
    pub const fn is_valid(&self) -> bool {
        match self.present {
            0 => {
                let mut index = 0;

                while index < self.namespace.len() {
                    if self.namespace[index] != 0 {
                        return false;
                    }

                    index += 1;
                }

                self.source == 0 && self.start == 0 && self.end == 0 && self.version == 0
            }
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
    extern "C" fn(&mut NativePanicReport, u32) -> NativeRuntimeStatus;

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
    present: u32,
    message: NativePanicMessage,
    provider: crate::NativeProviderOwner,
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
            present: 1,
            message,
            provider: crate::NativeProviderOwner::resident(),
        }
    }

    /// Creates an empty primary for an unpublished destination.
    pub const fn empty() -> Self {
        Self {
            cause: NativePanicCause::MESSAGE,
            source: NativeSourceAnchor::unavailable(),
            present: 0,
            message: NativePanicMessage::empty(),
            provider: crate::NativeProviderOwner::resident(),
        }
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

    /// Publishes the originating provider before this primary leaves its producer.
    /// The provider covers the message, callbacks and product statics together.
    pub fn retain_provider(&mut self, provider: crate::NativeProviderOwner) {
        assert!(
            !self.provider.reference().is_retained(),
            "primary provider ownership publishes once"
        );

        self.provider = provider;
    }
}

/// Caller-owned report header with an optional inline primary and a detached record chain.
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

    /// Adopts a detached record chain without introducing an inline panic.
    pub const fn from_records(
        records: crate::NativeReportRecords,
        consume: NativePanicReportConsumer,
    ) -> Self {
        Self {
            primary: NativePanicPrimary::empty(),
            head: records.head,
            tail: records.tail,
            count: records.count,
            reserved: 0,
            consume: Some(consume),
        }
    }

    /// Returns whether this report owns an inline panic primary.
    pub const fn has_primary(&self) -> bool {
        self.primary.present != 0
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

    /// Returns the number of incident records following the primary.
    pub const fn outgoing_count(&self) -> usize {
        self.count
    }

    /// Returns whether the report owns a record for a subsequent ownership transfer.
    pub const fn has_reservation(&self) -> bool {
        self.reserved != 0
    }

    /// Transfers the primary and detached record identities to their owning consumer.
    pub fn take_parts(&mut self) -> (Option<NativePanicPrimary>, usize, usize, usize, usize) {
        self.consume = None;

        let primary = std::mem::replace(&mut self.primary, NativePanicPrimary::empty());
        let primary = (primary.present != 0).then_some(primary);

        (
            primary,
            std::mem::take(&mut self.head),
            std::mem::take(&mut self.tail),
            std::mem::take(&mut self.count),
            std::mem::take(&mut self.reserved),
        )
    }

    /// Consumes live ownership exactly once, reporting it when requested.
    pub fn consume(&mut self, report: bool) -> NativeRuntimeStatus {
        // A native caller holds this guard outside the image callback and its local cleanup.
        let _provider = self.primary.provider.retain();

        self.consume
            .take()
            .map_or(NativeRuntimeStatus::SUCCESS, |consume| {
                consume(self, u32::from(report))
            })
    }

    /// Publishes the originating provider for a primary or a chain-only report.
    pub fn retain_provider(&mut self, provider: crate::NativeProviderOwner) {
        self.primary.retain_provider(provider);
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
        assert_abi_layout!(NativeSourceAnchor, size: 56, align: 8, fields: {
            present: 0,
            source: 4,
            start: 8,
            end: 12,
            version: 16,
            namespace: 24,
        });
    }

    #[test]
    fn report_layout_keeps_inline_primary_and_independent_record_ownership() {
        assert_abi_layout!(NativePanicMessage, size: 32, align: 8, fields: {
            address: 0, length: 8, copy: 16, release: 24,
        });

        assert_abi_layout!(NativePanicPrimary, size: 112, align: 8, fields: {
            source: 0, cause: 56, present: 60, message: 64, provider: 96,
        });

        assert_abi_layout!(NativePanicReport, size: 152, align: 8, fields: {
            primary: 0, head: 112, tail: 120, count: 128, reserved: 136, consume: 144,
        });
    }

    #[test]
    fn absent_primary_is_distinct_from_an_empty_panic_message() {
        extern "C" fn consume(
            report: &mut NativePanicReport,
            _: u32,
        ) -> crate::NativeRuntimeStatus {
            drop(report.take_parts());

            crate::NativeRuntimeStatus::SUCCESS
        }

        let mut absent = NativePanicReport::empty();

        let mut panic = NativePanicReport::new(
            NativePanicPrimary::new(
                super::NativePanicCause::MESSAGE,
                NativeSourceAnchor::unavailable(),
                NativePanicMessage::empty(),
            ),
            consume,
        );

        assert!(!absent.has_primary());
        assert!(absent.take_parts().0.is_none());
        assert!(panic.has_primary());

        assert_eq!(
            panic.take_parts().0.expect("explicit panic").cause(),
            super::NativePanicCause::MESSAGE
        );

        assert!(!panic.has_primary());
    }

    #[test]
    fn chain_only_report_transfers_all_records_to_its_consumer_once() {
        extern "C" fn consume(
            report: &mut NativePanicReport,
            _: u32,
        ) -> crate::NativeRuntimeStatus {
            let (primary, head, tail, count, reserved) = report.take_parts();

            if primary.is_none() && (head, tail, count, reserved) == (11, 13, 3, 0) {
                crate::NativeRuntimeStatus::PANICKED
            } else {
                crate::NativeRuntimeStatus::INVALID_ARGUMENT
            }
        }

        let mut report = NativePanicReport::from_records(
            crate::NativeReportRecords {
                head: 11,
                tail: 13,
                count: 3,
            },
            consume,
        );

        assert!(!report.has_primary());
        assert!(!report.has_reservation());
        assert_eq!(report.outgoing_count(), 3);
        assert_eq!(report.consume(false), crate::NativeRuntimeStatus::PANICKED);
        assert_eq!(report.consume(false), crate::NativeRuntimeStatus::SUCCESS);
        assert_eq!(report.outgoing_count(), 0);
    }
}
