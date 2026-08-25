use crate::PlatformServiceRole;

/// One independently retainable platform capability family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlatformServiceFamily {
    /// Process context, clocks, and host entropy.
    Core,
    /// Standard input, standard output, and standard error.
    StandardStreams,
    /// Files, directories, and paths.
    Filesystem,
    /// Child processes and their pipes.
    Process,
    /// Calendar, timezone, and temporal representation services.
    Temporal,
    /// Dynamic-library loading and symbol lookup.
    DynamicLibrary,
}

impl PlatformServiceRole {
    /// Returns the independently retainable capability family.
    pub const fn family(self) -> PlatformServiceFamily {
        match self {
            Self::ContextIdentity
            | Self::ContextNativeTextWidth
            | Self::ContextWorkingDirectory
            | Self::ContextArgumentCount
            | Self::ContextArgument
            | Self::ContextEnvironmentCount
            | Self::ContextEnvironmentEntry
            | Self::ContextEnvironmentKeyEquals
            | Self::ClockMonotonicNow
            | Self::ClockWallNow
            | Self::ClockSleep
            | Self::EntropyFill => PlatformServiceFamily::Core,
            Self::StandardInputRead
            | Self::StandardInputLock
            | Self::StandardInputUnlock
            | Self::StandardOutputWrite
            | Self::StandardOutputFlush
            | Self::StandardOutputLock
            | Self::StandardOutputUnlock
            | Self::StandardErrorWrite
            | Self::StandardErrorFlush
            | Self::StandardErrorLock
            | Self::StandardErrorUnlock => PlatformServiceFamily::StandardStreams,
            Self::FileRead
            | Self::FileWrite
            | Self::FileFlush
            | Self::FileSeek
            | Self::FileClose
            | Self::FileOpen
            | Self::FileMetadata
            | Self::PathMetadata
            | Self::DirectoryOpen
            | Self::DirectoryNext
            | Self::DirectoryClose
            | Self::PathCreateDirectory
            | Self::PathRemoveFile
            | Self::PathRemoveDirectory
            | Self::PathRename => PlatformServiceFamily::Filesystem,
            Self::ProcessPipeRead
            | Self::ProcessPipeWrite
            | Self::ProcessPipeFlush
            | Self::ProcessPipeClose
            | Self::ChildSpawn
            | Self::ChildWait
            | Self::ChildTerminate
            | Self::ChildReap
            | Self::ChildDispose => PlatformServiceFamily::Process,
            Self::TimeDateValidate
            | Self::TimeDateAdd
            | Self::TimeZoneLoad
            | Self::TimeZoneLocal
            | Self::TimeZoneRetain
            | Self::TimeZoneClose
            | Self::TimeZoneName
            | Self::TimeObserve
            | Self::TimeResolve
            | Self::TimeParse
            | Self::TimeFormat => PlatformServiceFamily::Temporal,
            Self::DynamicLibraryOpenPath
            | Self::DynamicLibraryOpenSystem
            | Self::DynamicLibrarySymbol
            | Self::DynamicLibraryClose => PlatformServiceFamily::DynamicLibrary,
        }
    }
}
