use std::sync::Arc;

use bray_base::{NonEmptySharedStr, shared_slice};

macro_rules! define_platform_service_roles {
    ($( $(#[$documentation:meta])* $role:ident, )+) => {
        /// One closed platform-service role understood by the compiler, standard library, and runtime.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum PlatformServiceRole {
            $( $(#[$documentation])* $role, )+
        }

        impl PlatformServiceRole {
            /// Every platform-service role in stable numeric order.
            pub const ALL: &'static [Self] = &[$(Self::$role, )+];
        }
    };
}

define_platform_service_roles! {
    /// Reads the immutable process identity.
    ContextIdentity,
    /// Reads the target-native text width.
    ContextNativeTextWidth,
    /// Borrows the startup working directory.
    ContextWorkingDirectory,
    /// Reads the startup argument count.
    ContextArgumentCount,
    /// Borrows one startup argument.
    ContextArgument,
    /// Reads the startup environment-entry count.
    ContextEnvironmentCount,
    /// Borrows one startup environment entry.
    ContextEnvironmentEntry,
    /// Compares environment keys using the target's process-environment rules.
    ContextEnvironmentKeyEquals,
    /// Reads bytes from standard input.
    StandardInputRead,
    /// Acquires product-wide standard-input serialization.
    StandardInputLock,
    /// Releases product-wide standard-input serialization.
    StandardInputUnlock,
    /// Writes bytes to standard output.
    StandardOutputWrite,
    /// Flushes standard output.
    StandardOutputFlush,
    /// Acquires product-wide standard-output serialization.
    StandardOutputLock,
    /// Releases product-wide standard-output serialization.
    StandardOutputUnlock,
    /// Writes bytes to standard error.
    StandardErrorWrite,
    /// Flushes standard error.
    StandardErrorFlush,
    /// Acquires product-wide standard-error serialization.
    StandardErrorLock,
    /// Releases product-wide standard-error serialization.
    StandardErrorUnlock,
    /// Reads bytes from a borrowed file owner.
    FileRead,
    /// Writes bytes to a borrowed file owner.
    FileWrite,
    /// Flushes a borrowed file owner.
    FileFlush,
    /// Seeks a borrowed file owner.
    FileSeek,
    /// Closes an owned file owner.
    FileClose,
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
    /// Reads bytes from a borrowed child-process pipe owner.
    ProcessPipeRead,
    /// Writes bytes to a borrowed child-process pipe owner.
    ProcessPipeWrite,
    /// Flushes a borrowed child-process pipe owner.
    ProcessPipeFlush,
    /// Closes an owned child-process pipe owner.
    ProcessPipeClose,
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
    /// Creates one operating-system thread and transfers its callback context.
    ThreadCreate,
    /// Waits for one operating-system thread and consumes its owner.
    ThreadJoin,
    /// Releases the join authority for one operating-system thread.
    ThreadDetach,
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
    /// Opens one dynamic library from an explicit path.
    DynamicLibraryOpenPath,
    /// Opens one target-defined system library.
    DynamicLibraryOpenSystem,
    /// Resolves one exact symbol from a borrowed dynamic library.
    DynamicLibrarySymbol,
    /// Closes one owned dynamic library.
    DynamicLibraryClose,
}

impl PlatformServiceRole {
    /// Returns the stable numeric role identity.
    pub const fn id(self) -> u32 {
        match self {
            Self::ContextIdentity => 0x0001,
            Self::ContextNativeTextWidth => 0x0002,
            Self::ContextWorkingDirectory => 0x0003,
            Self::ContextArgumentCount => 0x0004,
            Self::ContextArgument => 0x0005,
            Self::ContextEnvironmentCount => 0x0006,
            Self::ContextEnvironmentEntry => 0x0007,
            Self::ContextEnvironmentKeyEquals => 0x0008,
            Self::StandardInputRead => 0x0101,
            Self::StandardInputLock => 0x0102,
            Self::StandardInputUnlock => 0x0103,
            Self::StandardOutputWrite => 0x0111,
            Self::StandardOutputFlush => 0x0112,
            Self::StandardOutputLock => 0x0113,
            Self::StandardOutputUnlock => 0x0114,
            Self::StandardErrorWrite => 0x0121,
            Self::StandardErrorFlush => 0x0122,
            Self::StandardErrorLock => 0x0123,
            Self::StandardErrorUnlock => 0x0124,
            Self::FileRead => 0x0201,
            Self::FileWrite => 0x0202,
            Self::FileFlush => 0x0203,
            Self::FileSeek => 0x0204,
            Self::FileClose => 0x0205,
            Self::FileOpen => 0x0211,
            Self::FileMetadata => 0x0212,
            Self::PathMetadata => 0x0213,
            Self::DirectoryOpen => 0x0221,
            Self::DirectoryNext => 0x0222,
            Self::DirectoryClose => 0x0223,
            Self::PathCreateDirectory => 0x0230,
            Self::PathRemoveFile => 0x0231,
            Self::PathRemoveDirectory => 0x0232,
            Self::PathRename => 0x0233,
            Self::ProcessPipeRead => 0x0301,
            Self::ProcessPipeWrite => 0x0302,
            Self::ProcessPipeFlush => 0x0303,
            Self::ProcessPipeClose => 0x0304,
            Self::ChildSpawn => 0x0311,
            Self::ChildWait => 0x0312,
            Self::ChildTerminate => 0x0313,
            Self::ChildReap => 0x0314,
            Self::ChildDispose => 0x0315,
            Self::ThreadCreate => 0x0321,
            Self::ThreadJoin => 0x0322,
            Self::ThreadDetach => 0x0323,
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
            Self::DynamicLibraryOpenPath => 0x0801,
            Self::DynamicLibraryOpenSystem => 0x0802,
            Self::DynamicLibrarySymbol => 0x0803,
            Self::DynamicLibraryClose => 0x0804,
        }
    }

    /// Returns the role with an exact stable numeric identity.
    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            0x0001 => Some(Self::ContextIdentity),
            0x0002 => Some(Self::ContextNativeTextWidth),
            0x0003 => Some(Self::ContextWorkingDirectory),
            0x0004 => Some(Self::ContextArgumentCount),
            0x0005 => Some(Self::ContextArgument),
            0x0006 => Some(Self::ContextEnvironmentCount),
            0x0007 => Some(Self::ContextEnvironmentEntry),
            0x0008 => Some(Self::ContextEnvironmentKeyEquals),
            0x0101 => Some(Self::StandardInputRead),
            0x0102 => Some(Self::StandardInputLock),
            0x0103 => Some(Self::StandardInputUnlock),
            0x0111 => Some(Self::StandardOutputWrite),
            0x0112 => Some(Self::StandardOutputFlush),
            0x0113 => Some(Self::StandardOutputLock),
            0x0114 => Some(Self::StandardOutputUnlock),
            0x0121 => Some(Self::StandardErrorWrite),
            0x0122 => Some(Self::StandardErrorFlush),
            0x0123 => Some(Self::StandardErrorLock),
            0x0124 => Some(Self::StandardErrorUnlock),
            0x0201 => Some(Self::FileRead),
            0x0202 => Some(Self::FileWrite),
            0x0203 => Some(Self::FileFlush),
            0x0204 => Some(Self::FileSeek),
            0x0205 => Some(Self::FileClose),
            0x0211 => Some(Self::FileOpen),
            0x0212 => Some(Self::FileMetadata),
            0x0213 => Some(Self::PathMetadata),
            0x0221 => Some(Self::DirectoryOpen),
            0x0222 => Some(Self::DirectoryNext),
            0x0223 => Some(Self::DirectoryClose),
            0x0230 => Some(Self::PathCreateDirectory),
            0x0231 => Some(Self::PathRemoveFile),
            0x0232 => Some(Self::PathRemoveDirectory),
            0x0233 => Some(Self::PathRename),
            0x0301 => Some(Self::ProcessPipeRead),
            0x0302 => Some(Self::ProcessPipeWrite),
            0x0303 => Some(Self::ProcessPipeFlush),
            0x0304 => Some(Self::ProcessPipeClose),
            0x0311 => Some(Self::ChildSpawn),
            0x0312 => Some(Self::ChildWait),
            0x0313 => Some(Self::ChildTerminate),
            0x0314 => Some(Self::ChildReap),
            0x0315 => Some(Self::ChildDispose),
            0x0321 => Some(Self::ThreadCreate),
            0x0322 => Some(Self::ThreadJoin),
            0x0323 => Some(Self::ThreadDetach),
            0x0401 => Some(Self::ClockMonotonicNow),
            0x0402 => Some(Self::ClockWallNow),
            0x0403 => Some(Self::ClockSleep),
            0x0501 => Some(Self::EntropyFill),
            0x0701 => Some(Self::TimeDateValidate),
            0x0702 => Some(Self::TimeDateAdd),
            0x0710 => Some(Self::TimeZoneLoad),
            0x0711 => Some(Self::TimeZoneLocal),
            0x0712 => Some(Self::TimeZoneRetain),
            0x0713 => Some(Self::TimeZoneClose),
            0x0714 => Some(Self::TimeZoneName),
            0x0720 => Some(Self::TimeObserve),
            0x0721 => Some(Self::TimeResolve),
            0x0730 => Some(Self::TimeParse),
            0x0731 => Some(Self::TimeFormat),
            0x0801 => Some(Self::DynamicLibraryOpenPath),
            0x0802 => Some(Self::DynamicLibraryOpenSystem),
            0x0803 => Some(Self::DynamicLibrarySymbol),
            0x0804 => Some(Self::DynamicLibraryClose),
            _ => None,
        }
    }

    /// Returns the canonical role name used by product metadata.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContextIdentity => "platform.context.identity",
            Self::ContextNativeTextWidth => "platform.context.native_text_width",
            Self::ContextWorkingDirectory => "platform.context.working_directory",
            Self::ContextArgumentCount => "platform.context.argument_count",
            Self::ContextArgument => "platform.context.argument",
            Self::ContextEnvironmentCount => "platform.context.environment_count",
            Self::ContextEnvironmentEntry => "platform.context.environment_entry",
            Self::ContextEnvironmentKeyEquals => "platform.context.environment_key_equals",
            Self::StandardInputRead => "platform.standard_input.read",
            Self::StandardInputLock => "platform.standard_input.lock",
            Self::StandardInputUnlock => "platform.standard_input.unlock",
            Self::StandardOutputWrite => "platform.standard_output.write",
            Self::StandardOutputFlush => "platform.standard_output.flush",
            Self::StandardOutputLock => "platform.standard_output.lock",
            Self::StandardOutputUnlock => "platform.standard_output.unlock",
            Self::StandardErrorWrite => "platform.standard_error.write",
            Self::StandardErrorFlush => "platform.standard_error.flush",
            Self::StandardErrorLock => "platform.standard_error.lock",
            Self::StandardErrorUnlock => "platform.standard_error.unlock",
            Self::FileRead => "platform.file.read",
            Self::FileWrite => "platform.file.write",
            Self::FileFlush => "platform.file.flush",
            Self::FileSeek => "platform.file.seek",
            Self::FileClose => "platform.file.close",
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
            Self::ProcessPipeRead => "platform.process_pipe.read",
            Self::ProcessPipeWrite => "platform.process_pipe.write",
            Self::ProcessPipeFlush => "platform.process_pipe.flush",
            Self::ProcessPipeClose => "platform.process_pipe.close",
            Self::ChildSpawn => "platform.child.spawn",
            Self::ChildWait => "platform.child.wait",
            Self::ChildTerminate => "platform.child.terminate",
            Self::ChildReap => "platform.child.reap",
            Self::ChildDispose => "platform.child.dispose",
            Self::ThreadCreate => "platform.thread.create",
            Self::ThreadJoin => "platform.thread.join",
            Self::ThreadDetach => "platform.thread.detach",
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
            Self::DynamicLibraryOpenPath => "platform.dynamic_library.open_path",
            Self::DynamicLibraryOpenSystem => "platform.dynamic_library.open_system",
            Self::DynamicLibrarySymbol => "platform.dynamic_library.symbol",
            Self::DynamicLibraryClose => "platform.dynamic_library.close",
        }
    }

    /// Returns the role with an exact canonical metadata name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "platform.context.identity" => Some(Self::ContextIdentity),
            "platform.context.native_text_width" => Some(Self::ContextNativeTextWidth),
            "platform.context.working_directory" => Some(Self::ContextWorkingDirectory),
            "platform.context.argument_count" => Some(Self::ContextArgumentCount),
            "platform.context.argument" => Some(Self::ContextArgument),
            "platform.context.environment_count" => Some(Self::ContextEnvironmentCount),
            "platform.context.environment_entry" => Some(Self::ContextEnvironmentEntry),
            "platform.context.environment_key_equals" => Some(Self::ContextEnvironmentKeyEquals),
            "platform.standard_input.read" => Some(Self::StandardInputRead),
            "platform.standard_input.lock" => Some(Self::StandardInputLock),
            "platform.standard_input.unlock" => Some(Self::StandardInputUnlock),
            "platform.standard_output.write" => Some(Self::StandardOutputWrite),
            "platform.standard_output.flush" => Some(Self::StandardOutputFlush),
            "platform.standard_output.lock" => Some(Self::StandardOutputLock),
            "platform.standard_output.unlock" => Some(Self::StandardOutputUnlock),
            "platform.standard_error.write" => Some(Self::StandardErrorWrite),
            "platform.standard_error.flush" => Some(Self::StandardErrorFlush),
            "platform.standard_error.lock" => Some(Self::StandardErrorLock),
            "platform.standard_error.unlock" => Some(Self::StandardErrorUnlock),
            "platform.file.read" => Some(Self::FileRead),
            "platform.file.write" => Some(Self::FileWrite),
            "platform.file.flush" => Some(Self::FileFlush),
            "platform.file.seek" => Some(Self::FileSeek),
            "platform.file.close" => Some(Self::FileClose),
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
            "platform.process_pipe.read" => Some(Self::ProcessPipeRead),
            "platform.process_pipe.write" => Some(Self::ProcessPipeWrite),
            "platform.process_pipe.flush" => Some(Self::ProcessPipeFlush),
            "platform.process_pipe.close" => Some(Self::ProcessPipeClose),
            "platform.child.spawn" => Some(Self::ChildSpawn),
            "platform.child.wait" => Some(Self::ChildWait),
            "platform.child.terminate" => Some(Self::ChildTerminate),
            "platform.child.reap" => Some(Self::ChildReap),
            "platform.child.dispose" => Some(Self::ChildDispose),
            "platform.thread.create" => Some(Self::ThreadCreate),
            "platform.thread.join" => Some(Self::ThreadJoin),
            "platform.thread.detach" => Some(Self::ThreadDetach),
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
            "platform.dynamic_library.open_path" => Some(Self::DynamicLibraryOpenPath),
            "platform.dynamic_library.open_system" => Some(Self::DynamicLibraryOpenSystem),
            "platform.dynamic_library.symbol" => Some(Self::DynamicLibrarySymbol),
            "platform.dynamic_library.close" => Some(Self::DynamicLibraryClose),
            _ => None,
        }
    }

    /// Returns the exact private callable shape required by this role.
    pub const fn signature(self) -> PlatformServiceSignature {
        use PlatformAbiType::{
            ChildRequest, ExitStatusPointer, FileMetadataPointer, FileOptions, I32, I64,
            NativeText, Path, PointerI64, PointerU8, PointerU32, PointerU64, RawAddressPointer,
            Status, TemporalDateTime, TemporalDateTimePointer, TemporalObservationPointer,
            TemporalResolutionPointer, TemporalValue, TemporalValuePointer, U32, U64,
        };

        const CONTEXT_COUNT: &[PlatformAbiType] = &[PointerU64];
        const CONTEXT_TEXT: &[PlatformAbiType] = &[RawAddressPointer, PointerU64];
        const CONTEXT_ARGUMENT: &[PlatformAbiType] = &[U64, RawAddressPointer, PointerU64];

        const CONTEXT_ENVIRONMENT_ENTRY: &[PlatformAbiType] = &[
            U64,
            RawAddressPointer,
            PointerU64,
            RawAddressPointer,
            PointerU64,
        ];

        const CONTEXT_ENVIRONMENT_KEY_EQUALS: &[PlatformAbiType] =
            &[NativeText, NativeText, PointerU32];

        const STANDARD_STREAM_TRANSFER: &[PlatformAbiType] = &[PointerU8, U64, PointerU64];
        const OWNED_STREAM_TRANSFER: &[PlatformAbiType] = &[U64, PointerU8, U64, PointerU64];
        const NO_PARAMETERS: &[PlatformAbiType] = &[];
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
        const CLOCK_MONOTONIC_NOW: &[PlatformAbiType] = &[PointerU64];
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

        const DYNAMIC_LIBRARY_OPEN_PATH: &[PlatformAbiType] = &[Path, U32, PointerU64];
        const DYNAMIC_LIBRARY_OPEN_SYSTEM: &[PlatformAbiType] = &[U32, U32, PointerU64];

        const DYNAMIC_LIBRARY_SYMBOL: &[PlatformAbiType] =
            &[U64, PointerU8, U64, RawAddressPointer];

        const CHILD_SPAWN: &[PlatformAbiType] =
            &[ChildRequest, PointerU64, PointerU64, PointerU64, PointerU64];

        const CHILD_WAIT: &[PlatformAbiType] = &[U64, PointerU32, ExitStatusPointer];
        const CHILD_TERMINATE: &[PlatformAbiType] = &[U64, U32];
        const CHILD_REAP: &[PlatformAbiType] = &[U64, ExitStatusPointer];

        const THREAD_CREATE: &[PlatformAbiType] = &[PointerU8, PointerU8, PointerU64, PointerI64];

        const THREAD_OWNER: &[PlatformAbiType] = &[U64];

        let parameters = match self {
            Self::ContextIdentity | Self::ContextNativeTextWidth => NO_PARAMETERS,
            Self::ContextWorkingDirectory => CONTEXT_TEXT,
            Self::ContextArgumentCount | Self::ContextEnvironmentCount => CONTEXT_COUNT,
            Self::ContextArgument => CONTEXT_ARGUMENT,
            Self::ContextEnvironmentEntry => CONTEXT_ENVIRONMENT_ENTRY,
            Self::ContextEnvironmentKeyEquals => CONTEXT_ENVIRONMENT_KEY_EQUALS,
            Self::StandardInputRead | Self::StandardOutputWrite | Self::StandardErrorWrite => {
                STANDARD_STREAM_TRANSFER
            }
            Self::StandardInputLock
            | Self::StandardInputUnlock
            | Self::StandardOutputFlush
            | Self::StandardOutputLock
            | Self::StandardOutputUnlock
            | Self::StandardErrorFlush
            | Self::StandardErrorLock
            | Self::StandardErrorUnlock => NO_PARAMETERS,
            Self::FileRead | Self::FileWrite | Self::ProcessPipeRead | Self::ProcessPipeWrite => {
                OWNED_STREAM_TRANSFER
            }
            Self::FileFlush
            | Self::FileClose
            | Self::ProcessPipeFlush
            | Self::ProcessPipeClose
            | Self::DirectoryClose => STREAM_HANDLE,
            Self::FileSeek => STREAM_SEEK,
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
            Self::ThreadCreate => THREAD_CREATE,
            Self::ThreadJoin | Self::ThreadDetach => THREAD_OWNER,
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
            Self::DynamicLibraryOpenPath => DYNAMIC_LIBRARY_OPEN_PATH,
            Self::DynamicLibraryOpenSystem => DYNAMIC_LIBRARY_OPEN_SYSTEM,
            Self::DynamicLibrarySymbol => DYNAMIC_LIBRARY_SYMBOL,
            Self::DynamicLibraryClose => STREAM_HANDLE,
        };

        let result = match self {
            Self::ContextIdentity => U64,
            Self::ContextNativeTextWidth => U32,
            _ => Status,
        };

        PlatformServiceSignature::new(parameters, result)
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
    /// Raw pointer to one target-native raw address output.
    RawAddressPointer,
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{PlatformAbiType, PlatformServiceBinding, PlatformServiceRole};

    #[test]
    fn complete_platform_role_catalog_has_unique_identities() {
        let ids = PlatformServiceRole::ALL
            .iter()
            .map(|role| role.id())
            .collect::<BTreeSet<_>>();

        let names = PlatformServiceRole::ALL
            .iter()
            .map(|role| role.as_str())
            .collect::<BTreeSet<_>>();

        let symbols = PlatformServiceRole::ALL
            .iter()
            .map(|role| crate::native_platform_service_role_symbol(*role))
            .collect::<BTreeSet<_>>();

        assert_eq!(ids.len(), PlatformServiceRole::ALL.len());
        assert_eq!(names.len(), PlatformServiceRole::ALL.len());
        assert_eq!(symbols.len(), PlatformServiceRole::ALL.len());

        for role in PlatformServiceRole::ALL {
            assert_eq!(PlatformServiceRole::from_id(role.id()), Some(*role));
            assert_eq!(PlatformServiceRole::from_name(role.as_str()), Some(*role));
        }
    }

    #[test]
    fn platform_roles_have_stable_names_ids_and_shapes() {
        let role = PlatformServiceRole::StandardOutputWrite;
        let input_lock = PlatformServiceRole::StandardInputLock;

        assert_eq!(role.id(), 0x0111);
        assert_eq!(role.as_str(), "platform.standard_output.write");
        assert_eq!(PlatformServiceRole::from_id(role.id()), Some(role));
        assert_eq!(PlatformServiceRole::from_name(role.as_str()), Some(role));

        assert_eq!(
            role.signature().parameters(),
            [
                PlatformAbiType::PointerU8,
                PlatformAbiType::U64,
                PlatformAbiType::PointerU64,
            ]
        );

        assert_eq!(role.signature().result(), PlatformAbiType::Status);

        assert_eq!(input_lock.id(), 0x0102);
        assert_eq!(input_lock.as_str(), "platform.standard_input.lock");

        assert_eq!(
            PlatformServiceRole::from_id(input_lock.id()),
            Some(input_lock)
        );

        assert_eq!(
            PlatformServiceRole::from_name(input_lock.as_str()),
            Some(input_lock)
        );

        assert!(input_lock.signature().parameters().is_empty());
        assert_eq!(input_lock.signature().result(), PlatformAbiType::Status);
        assert_eq!(PlatformServiceRole::from_id(0), None);
        assert_eq!(PlatformServiceRole::from_id(u32::MAX), None);
    }

    #[test]
    fn platform_scalar_role_shapes_are_exact() {
        let identity = PlatformServiceRole::ContextIdentity.signature();
        let width = PlatformServiceRole::ContextNativeTextWidth.signature();
        let monotonic = PlatformServiceRole::ClockMonotonicNow.signature();

        assert!(identity.parameters().is_empty());
        assert_eq!(identity.result(), PlatformAbiType::U64);
        assert!(width.parameters().is_empty());
        assert_eq!(width.result(), PlatformAbiType::U32);
        assert_eq!(monotonic.parameters(), [PlatformAbiType::PointerU64]);
        assert_eq!(monotonic.result(), PlatformAbiType::Status);
    }

    #[test]
    fn child_spawn_role_has_closed_request_and_owner_outputs() {
        let role = PlatformServiceRole::ChildSpawn;

        assert_eq!(role.id(), 0x0311);
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
    fn dynamic_symbol_role_has_closed_handle_name_and_address_shape() {
        let role = PlatformServiceRole::DynamicLibrarySymbol;

        assert_eq!(role.id(), 0x0803);
        assert_eq!(role.as_str(), "platform.dynamic_library.symbol");
        assert_eq!(PlatformServiceRole::from_name(role.as_str()), Some(role));

        assert_eq!(
            role.signature().parameters(),
            [
                PlatformAbiType::U64,
                PlatformAbiType::PointerU8,
                PlatformAbiType::U64,
                PlatformAbiType::RawAddressPointer,
            ]
        );

        assert_eq!(role.signature().result(), PlatformAbiType::Status);
    }

    #[test]
    fn platform_bindings_require_module_qualified_declarations() {
        let Some(binding) = PlatformServiceBinding::try_new(
            PlatformServiceRole::StandardOutputFlush,
            "std.io.platform_standard_output_flush",
        ) else {
            panic!("test binding path must be valid");
        };

        assert_eq!(binding.module().collect::<Vec<_>>(), ["std", "io"]);
        assert_eq!(binding.declaration(), "platform_standard_output_flush");

        assert_eq!(
            binding.dotted_path(),
            "std.io.platform_standard_output_flush"
        );

        assert_eq!(
            PlatformServiceBinding::try_new(PlatformServiceRole::StandardOutputFlush, "flush"),
            None
        );
    }
}
