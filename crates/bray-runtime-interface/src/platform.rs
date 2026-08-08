use std::sync::Arc;

use bray_base::{NonEmptySharedStr, shared_slice};

/// One closed platform-service role understood by the compiler and native provider.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlatformServiceRole {
    /// Measures the immutable process-context block.
    ContextMeasure,
    /// Copies the immutable process-context block into caller-owned storage.
    ContextCopy,
    /// Compares environment keys using the target's process-environment rules.
    ContextEnvironmentKeyEquals,
    /// Reads bytes from a borrowed stream handle.
    StreamRead,
    /// Writes bytes to a borrowed stream handle.
    StreamWrite,
    /// Flushes a borrowed stream handle.
    StreamFlush,
    /// Seeks a borrowed seekable stream handle.
    StreamSeek,
    /// Closes an owned stream handle.
    StreamClose,
    /// Acquires product-wide serialization for a borrowed stream handle.
    StreamLock,
    /// Releases product-wide serialization for a borrowed stream handle.
    StreamUnlock,
    /// Opens one file stream.
    FileOpen,
    /// Reads metadata from a borrowed file stream.
    FileMetadata,
    /// Reads metadata for one path.
    PathMetadata,
    /// Opens one directory traversal.
    DirectoryOpen,
    /// Reads the next directory traversal entry.
    DirectoryNext,
    /// Closes an owned directory traversal.
    DirectoryClose,
    /// Creates one directory.
    PathCreateDirectory,
    /// Removes one file.
    PathRemoveFile,
    /// Removes one empty directory.
    PathRemoveDirectory,
    /// Renames one filesystem entry.
    PathRename,
    /// Creates one child process and its requested pipe owners.
    ChildSpawn,
    /// Waits for a child to terminate without consuming its owner.
    ChildWait,
    /// Requests child-process termination without consuming its owner.
    ChildTerminate,
    /// Consumes one terminal child-process owner.
    ChildReap,
    /// Forcefully resolves and consumes one child-process owner.
    ChildDispose,
    /// Observes the process-local monotonic clock.
    ClockMonotonicNow,
    /// Observes the host wall clock.
    ClockWallNow,
    /// Blocks the current native thread for a duration.
    ClockSleep,
    /// Fills caller-owned bytes from the host entropy source.
    EntropyFill,
    /// Validates one proleptic Gregorian date.
    TimeDateValidate,
    /// Applies a checked calendar period to one date.
    TimeDateAdd,
    /// Loads one named timezone from the pinned database.
    TimeZoneLoad,
    /// Discovers and loads the host's local timezone when available.
    TimeZoneLocal,
    /// Retains one immutable timezone owner.
    TimeZoneRetain,
    /// Releases one immutable timezone owner.
    TimeZoneClose,
    /// Copies one timezone's canonical IANA name.
    TimeZoneName,
    /// Observes one timestamp through a named zone or fixed offset.
    TimeObserve,
    /// Resolves one local date-time through a named zone or fixed offset.
    TimeResolve,
    /// Parses one strict standard temporal representation.
    TimeParse,
    /// Formats one strict standard temporal representation.
    TimeFormat,
}

impl PlatformServiceRole {
    /// Returns the stable numeric role identity.
    pub const fn id(self) -> u32 {
        match self {
            Self::ContextMeasure => 0x0001,
            Self::ContextCopy => 0x0002,
            Self::ContextEnvironmentKeyEquals => 0x0003,
            Self::StreamRead => 0x0101,
            Self::StreamWrite => 0x0102,
            Self::StreamFlush => 0x0103,
            Self::StreamSeek => 0x0104,
            Self::StreamClose => 0x0105,
            Self::StreamLock => 0x0106,
            Self::StreamUnlock => 0x0107,
            Self::FileOpen => 0x0201,
            Self::FileMetadata => 0x0202,
            Self::PathMetadata => 0x0203,
            Self::DirectoryOpen => 0x0204,
            Self::DirectoryNext => 0x0205,
            Self::DirectoryClose => 0x0206,
            Self::PathCreateDirectory => 0x0210,
            Self::PathRemoveFile => 0x0211,
            Self::PathRemoveDirectory => 0x0212,
            Self::PathRename => 0x0213,
            Self::ChildSpawn => 0x0301,
            Self::ChildWait => 0x0302,
            Self::ChildTerminate => 0x0303,
            Self::ChildReap => 0x0304,
            Self::ChildDispose => 0x0305,
            Self::ClockMonotonicNow => 0x0401,
            Self::ClockWallNow => 0x0402,
            Self::ClockSleep => 0x0403,
            Self::EntropyFill => 0x0501,
            Self::TimeDateValidate => 0x0701,
            Self::TimeDateAdd => 0x0702,
            Self::TimeZoneLoad => 0x0710,
            Self::TimeZoneLocal => 0x0711,
            Self::TimeZoneRetain => 0x0712,
            Self::TimeZoneClose => 0x0713,
            Self::TimeZoneName => 0x0714,
            Self::TimeObserve => 0x0720,
            Self::TimeResolve => 0x0721,
            Self::TimeParse => 0x0730,
            Self::TimeFormat => 0x0731,
        }
    }

    /// Returns the canonical role name used by product metadata.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContextMeasure => "platform.context.measure",
            Self::ContextCopy => "platform.context.copy",
            Self::ContextEnvironmentKeyEquals => "platform.context.environment_key_equals",
            Self::StreamRead => "platform.stream.read",
            Self::StreamWrite => "platform.stream.write",
            Self::StreamFlush => "platform.stream.flush",
            Self::StreamSeek => "platform.stream.seek",
            Self::StreamClose => "platform.stream.close",
            Self::StreamLock => "platform.stream.lock",
            Self::StreamUnlock => "platform.stream.unlock",
            Self::FileOpen => "platform.file.open",
            Self::FileMetadata => "platform.file.metadata",
            Self::PathMetadata => "platform.path.metadata",
            Self::DirectoryOpen => "platform.directory.open",
            Self::DirectoryNext => "platform.directory.next",
            Self::DirectoryClose => "platform.directory.close",
            Self::PathCreateDirectory => "platform.path.create_directory",
            Self::PathRemoveFile => "platform.path.remove_file",
            Self::PathRemoveDirectory => "platform.path.remove_directory",
            Self::PathRename => "platform.path.rename",
            Self::ChildSpawn => "platform.child.spawn",
            Self::ChildWait => "platform.child.wait",
            Self::ChildTerminate => "platform.child.terminate",
            Self::ChildReap => "platform.child.reap",
            Self::ChildDispose => "platform.child.dispose",
            Self::ClockMonotonicNow => "platform.clock.monotonic_now",
            Self::ClockWallNow => "platform.clock.wall_now",
            Self::ClockSleep => "platform.clock.sleep",
            Self::EntropyFill => "platform.entropy.fill",
            Self::TimeDateValidate => "platform.time.date_validate",
            Self::TimeDateAdd => "platform.time.date_add",
            Self::TimeZoneLoad => "platform.time.zone_load",
            Self::TimeZoneLocal => "platform.time.zone_local",
            Self::TimeZoneRetain => "platform.time.zone_retain",
            Self::TimeZoneClose => "platform.time.zone_close",
            Self::TimeZoneName => "platform.time.zone_name",
            Self::TimeObserve => "platform.time.observe",
            Self::TimeResolve => "platform.time.resolve",
            Self::TimeParse => "platform.time.parse",
            Self::TimeFormat => "platform.time.format",
        }
    }

    /// Returns the role with an exact canonical metadata name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "platform.context.measure" => Some(Self::ContextMeasure),
            "platform.context.copy" => Some(Self::ContextCopy),
            "platform.context.environment_key_equals" => Some(Self::ContextEnvironmentKeyEquals),
            "platform.stream.read" => Some(Self::StreamRead),
            "platform.stream.write" => Some(Self::StreamWrite),
            "platform.stream.flush" => Some(Self::StreamFlush),
            "platform.stream.seek" => Some(Self::StreamSeek),
            "platform.stream.close" => Some(Self::StreamClose),
            "platform.stream.lock" => Some(Self::StreamLock),
            "platform.stream.unlock" => Some(Self::StreamUnlock),
            "platform.file.open" => Some(Self::FileOpen),
            "platform.file.metadata" => Some(Self::FileMetadata),
            "platform.path.metadata" => Some(Self::PathMetadata),
            "platform.directory.open" => Some(Self::DirectoryOpen),
            "platform.directory.next" => Some(Self::DirectoryNext),
            "platform.directory.close" => Some(Self::DirectoryClose),
            "platform.path.create_directory" => Some(Self::PathCreateDirectory),
            "platform.path.remove_file" => Some(Self::PathRemoveFile),
            "platform.path.remove_directory" => Some(Self::PathRemoveDirectory),
            "platform.path.rename" => Some(Self::PathRename),
            "platform.child.spawn" => Some(Self::ChildSpawn),
            "platform.child.wait" => Some(Self::ChildWait),
            "platform.child.terminate" => Some(Self::ChildTerminate),
            "platform.child.reap" => Some(Self::ChildReap),
            "platform.child.dispose" => Some(Self::ChildDispose),
            "platform.clock.monotonic_now" => Some(Self::ClockMonotonicNow),
            "platform.clock.wall_now" => Some(Self::ClockWallNow),
            "platform.clock.sleep" => Some(Self::ClockSleep),
            "platform.entropy.fill" => Some(Self::EntropyFill),
            "platform.time.date_validate" => Some(Self::TimeDateValidate),
            "platform.time.date_add" => Some(Self::TimeDateAdd),
            "platform.time.zone_load" => Some(Self::TimeZoneLoad),
            "platform.time.zone_local" => Some(Self::TimeZoneLocal),
            "platform.time.zone_retain" => Some(Self::TimeZoneRetain),
            "platform.time.zone_close" => Some(Self::TimeZoneClose),
            "platform.time.zone_name" => Some(Self::TimeZoneName),
            "platform.time.observe" => Some(Self::TimeObserve),
            "platform.time.resolve" => Some(Self::TimeResolve),
            "platform.time.parse" => Some(Self::TimeParse),
            "platform.time.format" => Some(Self::TimeFormat),
            _ => None,
        }
    }

    /// Returns the exact private callable shape required by this role.
    pub const fn signature(self) -> PlatformServiceSignature {
        use PlatformAbiType::{
            ChildRequest, ExitStatusPointer, FileMetadataPointer, FileOptions, I32, I64,
            NativeText, Path, PointerI64, PointerU8, PointerU32, PointerU64, Status,
            TemporalDateTime, TemporalDateTimePointer, TemporalObservationPointer,
            TemporalResolutionPointer, TemporalValue, TemporalValuePointer, U32, U64,
        };

        const CONTEXT_MEASURE: &[PlatformAbiType] = &[PointerU64];
        const CONTEXT_COPY: &[PlatformAbiType] = &[PointerU8, U64, PointerU64];

        const CONTEXT_ENVIRONMENT_KEY_EQUALS: &[PlatformAbiType] =
            &[NativeText, NativeText, PointerU32];

        const STREAM_TRANSFER: &[PlatformAbiType] = &[U64, PointerU8, U64, PointerU64];
        const STREAM_HANDLE: &[PlatformAbiType] = &[U64];
        const STREAM_SEEK: &[PlatformAbiType] = &[U64, U64, U32, PointerU64];
        const FILE_OPEN: &[PlatformAbiType] = &[Path, FileOptions, PointerU64];
        const FILE_METADATA: &[PlatformAbiType] = &[U64, FileMetadataPointer];
        const PATH_METADATA: &[PlatformAbiType] = &[Path, FileMetadataPointer];
        const DIRECTORY_OPEN: &[PlatformAbiType] = &[Path, PointerU64];

        const DIRECTORY_NEXT: &[PlatformAbiType] = &[
            U64,
            PointerU8,
            U64,
            PointerU64,
            PointerU32,
            FileMetadataPointer,
        ];

        const PATH: &[PlatformAbiType] = &[Path];
        const PATH_PAIR: &[PlatformAbiType] = &[Path, Path];
        const CLOCK_MONOTONIC_NOW: &[PlatformAbiType] = &[PointerU64, PointerU64, PointerU64];
        const CLOCK_WALL_NOW: &[PlatformAbiType] = &[PointerI64, PointerU32];
        const CLOCK_SLEEP: &[PlatformAbiType] = &[U64, U32];
        const ENTROPY_FILL: &[PlatformAbiType] = &[PointerU8, U64, PointerU64];
        const TIME_DATE_VALIDATE: &[PlatformAbiType] = &[I32, U32, U32, PointerU32];
        const TIME_DATE_ADD: &[PlatformAbiType] = &[
            TemporalDateTime,
            I32,
            I32,
            I32,
            U32,
            TemporalDateTimePointer,
            PointerU32,
        ];
        const TIME_ZONE_LOAD: &[PlatformAbiType] = &[NativeText, PointerU64, PointerU32];
        const TIME_ZONE_LOCAL: &[PlatformAbiType] = &[PointerU64, PointerU32];
        const TIME_ZONE_HANDLE: &[PlatformAbiType] = &[U64];
        const TIME_ZONE_NAME: &[PlatformAbiType] = &[U64, PointerU8, U64, PointerU64, PointerU32];
        const TIME_OBSERVE: &[PlatformAbiType] = &[
            U64,
            I32,
            I64,
            U32,
            TemporalObservationPointer,
            PointerU8,
            U64,
            PointerU64,
            PointerU32,
        ];
        const TIME_RESOLVE: &[PlatformAbiType] = &[
            U64,
            I32,
            TemporalDateTime,
            TemporalResolutionPointer,
            PointerU32,
        ];
        const TIME_PARSE: &[PlatformAbiType] = &[
            U32,
            NativeText,
            TemporalValuePointer,
            PointerU64,
            PointerU32,
        ];
        const TIME_FORMAT: &[PlatformAbiType] =
            &[U32, TemporalValue, PointerU8, U64, PointerU64, PointerU32];

        const CHILD_SPAWN: &[PlatformAbiType] =
            &[ChildRequest, PointerU64, PointerU64, PointerU64, PointerU64];

        const CHILD_WAIT: &[PlatformAbiType] = &[U64, PointerU32, ExitStatusPointer];
        const CHILD_TERMINATE: &[PlatformAbiType] = &[U64, U32];
        const CHILD_REAP: &[PlatformAbiType] = &[U64, ExitStatusPointer];

        let parameters = match self {
            Self::ContextMeasure => CONTEXT_MEASURE,
            Self::ContextCopy => CONTEXT_COPY,
            Self::ContextEnvironmentKeyEquals => CONTEXT_ENVIRONMENT_KEY_EQUALS,
            Self::StreamRead | Self::StreamWrite => STREAM_TRANSFER,
            Self::StreamFlush
            | Self::StreamClose
            | Self::StreamLock
            | Self::StreamUnlock
            | Self::DirectoryClose => STREAM_HANDLE,
            Self::StreamSeek => STREAM_SEEK,
            Self::FileOpen => FILE_OPEN,
            Self::FileMetadata => FILE_METADATA,
            Self::PathMetadata => PATH_METADATA,
            Self::DirectoryOpen => DIRECTORY_OPEN,
            Self::DirectoryNext => DIRECTORY_NEXT,
            Self::PathCreateDirectory | Self::PathRemoveFile | Self::PathRemoveDirectory => PATH,
            Self::PathRename => PATH_PAIR,
            Self::ChildSpawn => CHILD_SPAWN,
            Self::ChildWait => CHILD_WAIT,
            Self::ChildTerminate => CHILD_TERMINATE,
            Self::ChildReap => CHILD_REAP,
            Self::ChildDispose => STREAM_HANDLE,
            Self::ClockMonotonicNow => CLOCK_MONOTONIC_NOW,
            Self::ClockWallNow => CLOCK_WALL_NOW,
            Self::ClockSleep => CLOCK_SLEEP,
            Self::EntropyFill => ENTROPY_FILL,
            Self::TimeDateValidate => TIME_DATE_VALIDATE,
            Self::TimeDateAdd => TIME_DATE_ADD,
            Self::TimeZoneLoad => TIME_ZONE_LOAD,
            Self::TimeZoneLocal => TIME_ZONE_LOCAL,
            Self::TimeZoneRetain | Self::TimeZoneClose => TIME_ZONE_HANDLE,
            Self::TimeZoneName => TIME_ZONE_NAME,
            Self::TimeObserve => TIME_OBSERVE,
            Self::TimeResolve => TIME_RESOLVE,
            Self::TimeParse => TIME_PARSE,
            Self::TimeFormat => TIME_FORMAT,
        };

        PlatformServiceSignature::new(parameters, Status)
    }
}

/// One ABI value kind used by the closed platform-service callable schema.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlatformAbiType {
    /// Fixed-width signed 32-bit scalar.
    I32,
    /// Fixed-width unsigned 32-bit scalar.
    U32,
    /// Fixed-width unsigned 64-bit scalar.
    U64,
    /// Fixed-width signed 64-bit scalar.
    I64,
    /// Raw pointer to byte storage.
    PointerU8,
    /// Raw pointer to unsigned 32-bit storage.
    PointerU32,
    /// Raw pointer to unsigned 64-bit storage.
    PointerU64,
    /// Raw pointer to signed 64-bit storage.
    PointerI64,
    /// Call-only target-native path bytes.
    Path,
    /// Call-only target-native text units.
    NativeText,
    /// The fixed-layout file-open options record.
    FileOptions,
    /// Raw pointer to a fixed-layout file metadata record.
    FileMetadataPointer,
    /// The fixed-layout child-process construction record.
    ChildRequest,
    /// Raw pointer to a fixed-layout child exit-status record.
    ExitStatusPointer,
    /// The fixed-layout civil date-time record.
    TemporalDateTime,
    /// Raw pointer to a civil date-time record.
    TemporalDateTimePointer,
    /// Raw pointer to a timezone observation record.
    TemporalObservationPointer,
    /// Raw pointer to a local-time resolution record.
    TemporalResolutionPointer,
    /// The fixed-layout parsing and formatting value record.
    TemporalValue,
    /// Raw pointer to a parsing and formatting value record.
    TemporalValuePointer,
    /// The fixed-layout platform status record.
    Status,
}

/// Fixed-layout proleptic Gregorian date and wall-time fields.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NativePlatformDateTime {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
}

impl NativePlatformDateTime {
    /// Creates one fixed-layout civil date-time record.
    pub const fn new(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        nanosecond: u32,
    ) -> Self {
        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanosecond,
        }
    }

    /// Returns the proleptic Gregorian year.
    pub const fn year(self) -> i32 {
        self.year
    }

    /// Returns the one-based month.
    pub const fn month(self) -> u32 {
        self.month
    }

    /// Returns the one-based day of month.
    pub const fn day(self) -> u32 {
        self.day
    }

    /// Returns the zero-based hour of day.
    pub const fn hour(self) -> u32 {
        self.hour
    }

    /// Returns the minute within the hour.
    pub const fn minute(self) -> u32 {
        self.minute
    }

    /// Returns the second within the minute.
    pub const fn second(self) -> u32 {
        self.second
    }

    /// Returns the nanosecond within the second.
    pub const fn nanosecond(self) -> u32 {
        self.nanosecond
    }
}

/// Fixed-layout result of observing one absolute timestamp.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NativePlatformTemporalObservation {
    local: NativePlatformDateTime,
    offset_seconds: i32,
    daylight: u32,
}

impl NativePlatformTemporalObservation {
    /// Returns the observed local date and time.
    pub const fn local(self) -> NativePlatformDateTime {
        self.local
    }

    /// Returns the observed UTC offset in seconds.
    pub const fn offset_seconds(self) -> i32 {
        self.offset_seconds
    }

    /// Returns whether daylight-saving time applies.
    pub const fn is_daylight_saving(self) -> bool {
        self.daylight == 1
    }
}

/// Fixed-layout result of resolving one local date-time.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NativePlatformTemporalResolution {
    kind: u32,
    reserved: u32,
    first_seconds: i64,
    first_nanoseconds: u32,
    first_reserved: u32,
    second_seconds: i64,
    second_nanoseconds: u32,
    second_reserved: u32,
}

impl NativePlatformTemporalResolution {
    /// Returns the encoded local-time resolution kind.
    pub const fn kind(self) -> u32 {
        self.kind
    }

    /// Returns the first candidate's whole Unix seconds.
    pub const fn first_seconds(self) -> i64 {
        self.first_seconds
    }

    /// Returns the first candidate's nanosecond remainder.
    pub const fn first_nanoseconds(self) -> u32 {
        self.first_nanoseconds
    }

    /// Returns the second candidate's whole Unix seconds.
    pub const fn second_seconds(self) -> i64 {
        self.second_seconds
    }

    /// Returns the second candidate's nanosecond remainder.
    pub const fn second_nanoseconds(self) -> u32 {
        self.second_nanoseconds
    }
}

/// Fixed-layout value used by strict temporal parsing and formatting.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NativePlatformTemporalValue {
    local: NativePlatformDateTime,
    timestamp_seconds: i64,
    offset_seconds: i32,
    reserved: u32,
}

impl NativePlatformTemporalValue {
    /// Returns the civil date-time component.
    pub const fn local(self) -> NativePlatformDateTime {
        self.local
    }

    /// Returns the absolute whole Unix seconds.
    pub const fn timestamp_seconds(self) -> i64 {
        self.timestamp_seconds
    }

    /// Returns the fixed UTC offset in seconds.
    pub const fn offset_seconds(self) -> i32 {
        self.offset_seconds
    }
}

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

/// Complete call-only child-process construction request.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformChildRequest {
    executable: NativePlatformText,
    working_directory: NativePlatformText,
    arguments: NativePlatformSpanList,
    environment: NativePlatformEnvironmentList,
    standard_input: u32,
    standard_output: u32,
    standard_error: u32,
    reserved: u32,
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

impl NativePlatformChildRequest {
    /// Creates one complete child-process request.
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor mirrors the fixed native ABI record"
    )]
    pub const fn new(
        executable: NativePlatformText,
        working_directory: NativePlatformText,
        arguments: NativePlatformSpanList,
        environment: NativePlatformEnvironmentList,
        standard_input: u32,
        standard_output: u32,
        standard_error: u32,
    ) -> Self {
        Self {
            executable,
            working_directory,
            arguments,
            environment,
            standard_input,
            standard_output,
            standard_error,
            reserved: 0,
        }
    }

    /// Returns the executable path span.
    pub const fn executable(self) -> NativePlatformText {
        self.executable
    }

    /// Returns the optional working-directory path span.
    pub const fn working_directory(self) -> NativePlatformText {
        self.working_directory
    }

    /// Returns the ordered argument spans.
    pub const fn arguments(self) -> NativePlatformSpanList {
        self.arguments
    }

    /// Returns the complete environment entries.
    pub const fn environment(self) -> NativePlatformEnvironmentList {
        self.environment
    }

    /// Returns the standard-input policy ordinal.
    pub const fn standard_input(self) -> u32 {
        self.standard_input
    }

    /// Returns the standard-output policy ordinal.
    pub const fn standard_output(self) -> u32 {
        self.standard_output
    }

    /// Returns the standard-error policy ordinal.
    pub const fn standard_error(self) -> u32 {
        self.standard_error
    }

    /// Returns the reserved field, which must be zero.
    pub const fn reserved(self) -> u32 {
        self.reserved
    }
}

/// Native child-process exit status.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformExitStatus {
    tag: u32,
    reserved: u32,
    payload: i64,
}

impl NativePlatformExitStatus {
    /// Creates one portable exit-code status.
    pub const fn code(code: i32) -> Self {
        Self {
            tag: 0,
            reserved: 0,
            payload: code as i64,
        }
    }

    /// Creates one target termination status.
    pub const fn target_termination(code: i64) -> Self {
        Self {
            tag: 1,
            reserved: 0,
            payload: code,
        }
    }

    /// Returns the status variant ordinal.
    pub const fn tag(self) -> u32 {
        self.tag
    }

    /// Returns the variant payload.
    pub const fn payload(self) -> i64 {
        self.payload
    }
}

/// The exact parameter and result shape of one platform-service role.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlatformServiceSignature {
    parameters: &'static [PlatformAbiType],
    result: PlatformAbiType,
}

impl PlatformServiceSignature {
    const fn new(parameters: &'static [PlatformAbiType], result: PlatformAbiType) -> Self {
        Self { parameters, result }
    }

    /// Returns parameter ABI kinds in calling order.
    pub const fn parameters(self) -> &'static [PlatformAbiType] {
        self.parameters
    }

    /// Returns the result ABI kind.
    pub const fn result(self) -> PlatformAbiType {
        self.result
    }
}

/// An explicit product association between a private declaration and platform role.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlatformServiceBinding {
    role: PlatformServiceRole,
    module: Arc<[NonEmptySharedStr]>,
    declaration: NonEmptySharedStr,
}

impl PlatformServiceBinding {
    /// Creates a binding from a role and dotted declaration path.
    pub fn try_new(role: PlatformServiceRole, path: &str) -> Option<Self> {
        let mut segments = path.split('.').map(NonEmptySharedStr::try_new);
        let mut present = segments.by_ref().collect::<Option<Vec<_>>>()?;
        let declaration = present.pop()?;

        if present.is_empty() {
            return None;
        }

        Some(Self {
            role,
            module: shared_slice(present),
            declaration,
        })
    }

    /// Returns the closed platform role selected by this binding.
    pub const fn role(&self) -> PlatformServiceRole {
        self.role
    }

    /// Returns the declaration's module path segments.
    pub fn module(&self) -> impl ExactSizeIterator<Item = &str> {
        self.module.iter().map(NonEmptySharedStr::as_str)
    }

    /// Returns the declaration name within its module.
    pub fn declaration(&self) -> &str {
        self.declaration.as_str()
    }

    /// Returns the canonical dotted declaration path.
    pub fn dotted_path(&self) -> String {
        self.module()
            .chain(std::iter::once(self.declaration()))
            .collect::<Vec<_>>()
            .join(".")
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
    use super::{PlatformAbiType, PlatformServiceBinding, PlatformServiceRole};

    #[test]
    fn platform_roles_have_stable_names_ids_and_shapes() {
        let role = PlatformServiceRole::StreamWrite;

        assert_eq!(role.id(), 0x0102);
        assert_eq!(role.as_str(), "platform.stream.write");
        assert_eq!(PlatformServiceRole::from_name(role.as_str()), Some(role));

        assert_eq!(
            role.signature().parameters(),
            [
                PlatformAbiType::U64,
                PlatformAbiType::PointerU8,
                PlatformAbiType::U64,
                PlatformAbiType::PointerU64,
            ]
        );

        assert_eq!(role.signature().result(), PlatformAbiType::Status);
    }

    #[test]
    fn child_spawn_role_has_closed_request_and_owner_outputs() {
        let role = PlatformServiceRole::ChildSpawn;

        assert_eq!(role.id(), 0x0301);
        assert_eq!(role.as_str(), "platform.child.spawn");
        assert_eq!(PlatformServiceRole::from_name(role.as_str()), Some(role));

        assert_eq!(
            role.signature().parameters(),
            [
                PlatformAbiType::ChildRequest,
                PlatformAbiType::PointerU64,
                PlatformAbiType::PointerU64,
                PlatformAbiType::PointerU64,
                PlatformAbiType::PointerU64,
            ]
        );

        assert_eq!(role.signature().result(), PlatformAbiType::Status);
    }

    #[test]
    fn platform_bindings_require_module_qualified_declarations() {
        let Some(binding) = PlatformServiceBinding::try_new(
            PlatformServiceRole::StreamFlush,
            "std.io.platform_stream_flush",
        ) else {
            panic!("test binding path must be valid");
        };

        assert_eq!(binding.module().collect::<Vec<_>>(), ["std", "io"]);
        assert_eq!(binding.declaration(), "platform_stream_flush");
        assert_eq!(binding.dotted_path(), "std.io.platform_stream_flush");

        assert_eq!(
            PlatformServiceBinding::try_new(PlatformServiceRole::StreamFlush, "flush"),
            None
        );
    }
}
