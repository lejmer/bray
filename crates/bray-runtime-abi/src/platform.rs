/// Reserved monotonic tick value reporting that the clock could not be observed.
pub const NATIVE_MONOTONIC_TICKS_UNAVAILABLE: u64 = u64::MAX;

/// Call-only target-native path bytes passed across the platform ABI.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformPath {
    address: *const u8,
    length: u64,
}

/// Call-only target-native text units passed across the platform ABI.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformText {
    address: *const u8,
    length: u64,
}

impl NativePlatformText {
    /// Creates one native-text view.
    pub const fn new(address: *const u8, length: u64) -> Self {
        Self { address, length }
    }

    /// Returns the first native-text byte.
    pub const fn address(self) -> *const u8 {
        self.address
    }

    /// Returns the native-text byte length.
    pub const fn length(self) -> u64 {
        self.length
    }
}

impl NativePlatformPath {
    /// Creates one native path view.
    pub const fn new(address: *const u8, length: u64) -> Self {
        Self { address, length }
    }

    /// Returns the first native path byte.
    pub const fn address(self) -> *const u8 {
        self.address
    }

    /// Returns the native path byte length.
    pub const fn length(self) -> u64 {
        self.length
    }
}

/// File access and creation policy passed across the platform ABI.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformFileOptions {
    access: u32,
    creation: u32,
    reserved: u64,
}

impl NativePlatformFileOptions {
    /// Creates one file-open policy record.
    pub const fn new(access: u32, creation: u32) -> Self {
        Self {
            access,
            creation,
            reserved: 0,
        }
    }

    /// Returns the encoded file access policy.
    pub const fn access(self) -> u32 {
        self.access
    }

    /// Returns the encoded file creation policy.
    pub const fn creation(self) -> u32 {
        self.creation
    }

    /// Returns the reserved field, which must be zero.
    pub const fn reserved(self) -> u64 {
        self.reserved
    }
}

/// File metadata returned across the platform ABI.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformFileMetadata {
    kind: u32,
    present: u32,
    bytes: u64,
    modified_seconds: i64,
    modified_nanoseconds: u32,
    reserved: u32,
}

/// Call-only contiguous native byte spans.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformSpanList {
    entries: *const NativePlatformText,
    count: u64,
}

impl NativePlatformSpanList {
    /// Creates one call-only span list.
    pub const fn new(entries: *const NativePlatformText, count: u64) -> Self {
        Self { entries, count }
    }

    /// Returns the first entry address.
    pub const fn entries(self) -> *const NativePlatformText {
        self.entries
    }

    /// Returns the entry count.
    pub const fn count(self) -> u64 {
        self.count
    }
}

/// One complete child-process environment entry.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformEnvironmentEntry {
    key: NativePlatformText,
    value: NativePlatformText,
}

impl NativePlatformEnvironmentEntry {
    /// Creates one native environment entry.
    pub const fn new(key: NativePlatformText, value: NativePlatformText) -> Self {
        Self { key, value }
    }

    /// Returns the key span.
    pub const fn key(self) -> NativePlatformText {
        self.key
    }

    /// Returns the value span.
    pub const fn value(self) -> NativePlatformText {
        self.value
    }
}

/// Call-only contiguous child-process environment entries.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformEnvironmentList {
    entries: *const NativePlatformEnvironmentEntry,
    count: u64,
}

impl NativePlatformEnvironmentList {
    /// Creates one call-only environment list.
    pub const fn new(entries: *const NativePlatformEnvironmentEntry, count: u64) -> Self {
        Self { entries, count }
    }

    /// Returns the first entry address.
    pub const fn entries(self) -> *const NativePlatformEnvironmentEntry {
        self.entries
    }

    /// Returns the entry count.
    pub const fn count(self) -> u64 {
        self.count
    }
}

impl NativePlatformFileMetadata {
    /// Creates one validated native metadata record.
    pub const fn new(
        kind: u32,
        present: u32,
        bytes: u64,
        modified_seconds: i64,
        modified_nanoseconds: u32,
    ) -> Self {
        Self {
            kind,
            present,
            bytes,
            modified_seconds,
            modified_nanoseconds,
            reserved: 0,
        }
    }

    /// Returns the encoded file kind.
    pub const fn kind(self) -> u32 {
        self.kind
    }

    /// Returns whether the modified timestamp is present.
    pub const fn modified_is_present(self) -> bool {
        self.present == 1
    }

    /// Returns the file byte length.
    pub const fn bytes(self) -> u64 {
        self.bytes
    }

    /// Returns the whole seconds of the modified timestamp.
    pub const fn modified_seconds(self) -> i64 {
        self.modified_seconds
    }

    /// Returns the fractional nanoseconds of the modified timestamp.
    pub const fn modified_nanoseconds(self) -> u32 {
        self.modified_nanoseconds
    }

    /// Returns the reserved field, which must be zero.
    pub const fn reserved(self) -> u32 {
        self.reserved
    }
}

/// Status returned by every native platform-service operation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformStatus {
    category: u32,
    reserved: u32,
    native_code: i64,
}

impl NativePlatformStatus {
    /// Successful platform operation.
    pub const SUCCESS: Self = Self::new(0, 0);
    /// Unsupported platform operation.
    pub const UNSUPPORTED: Self = Self::new(1, 0);
    /// Requested platform object is unavailable.
    pub const NOT_FOUND: Self = Self::new(3, 0);
    /// Invalid platform-service input.
    pub const INVALID_INPUT: Self = Self::new(5, 0);
    /// Platform resource exhaustion.
    pub const EXHAUSTED: Self = Self::new(7, 0);
    /// Broken stream operation.
    pub const BROKEN_STREAM: Self = Self::new(8, 0);
    /// Caller-provided context buffer is too small.
    pub const INSUFFICIENT_BUFFER: Self = Self::new(10, 0);
    /// Other target failure.
    pub const OTHER: Self = Self::new(12, 0);

    /// Creates one stable category with optional target-native inspection data.
    pub const fn new(category: u32, native_code: i64) -> Self {
        Self {
            category,
            reserved: 0,
            native_code,
        }
    }

    /// Returns the stable status category.
    pub const fn category(self) -> u32 {
        self.category
    }

    /// Returns target-native inspection data, or zero when unavailable.
    pub const fn native_code(self) -> i64 {
        self.native_code
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NativePlatformEnvironmentEntry, NativePlatformEnvironmentList, NativePlatformFileMetadata,
        NativePlatformFileOptions, NativePlatformPath, NativePlatformSpanList,
        NativePlatformStatus, NativePlatformText,
    };

    #[test]
    fn platform_records_have_the_native_abi_layout() {
        assert_abi_layout!(NativePlatformPath, size: 16, align: 8, fields: {
            address: 0,
            length: 8,
        });

        assert_abi_layout!(NativePlatformText, size: 16, align: 8, fields: {
            address: 0,
            length: 8,
        });

        assert_abi_layout!(NativePlatformFileOptions, size: 16, align: 8, fields: {
            access: 0,
            creation: 4,
            reserved: 8,
        });

        assert_abi_layout!(NativePlatformFileMetadata, size: 32, align: 8, fields: {
            kind: 0,
            present: 4,
            bytes: 8,
            modified_seconds: 16,
            modified_nanoseconds: 24,
            reserved: 28,
        });

        assert_abi_layout!(NativePlatformSpanList, size: 16, align: 8, fields: {
            entries: 0,
            count: 8,
        });

        assert_abi_layout!(NativePlatformEnvironmentEntry, size: 32, align: 8, fields: {
            key: 0,
            value: 16,
        });

        assert_abi_layout!(NativePlatformEnvironmentList, size: 16, align: 8, fields: {
            entries: 0,
            count: 8,
        });

        assert_abi_layout!(NativePlatformStatus, size: 16, align: 8, fields: {
            category: 0,
            reserved: 4,
            native_code: 8,
        });
    }
}
