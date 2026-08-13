use std::collections::BTreeSet;

use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};

pub(super) struct Partition {
    pub(super) name: &'static str,
    pub(super) feature: &'static str,
    pub(super) uses_rust_standard_library: bool,
    pub(super) roles: &'static [PlatformServiceRole],
}

pub(super) const PARTITIONS: &[Partition] = &[
    Partition {
        name: "bray_platform_core",
        feature: "core",
        uses_rust_standard_library: true,
        roles: &[
            PlatformServiceRole::ContextMeasure,
            PlatformServiceRole::ContextCopy,
            PlatformServiceRole::ContextEnvironmentKeyEquals,
            PlatformServiceRole::ClockMonotonicNow,
            PlatformServiceRole::ClockWallNow,
            PlatformServiceRole::ClockSleep,
            PlatformServiceRole::EntropyFill,
            PlatformServiceRole::TimeDateValidate,
            PlatformServiceRole::TimeDateAdd,
            PlatformServiceRole::TimeZoneLoad,
            PlatformServiceRole::TimeZoneLocal,
            PlatformServiceRole::TimeZoneRetain,
            PlatformServiceRole::TimeZoneClose,
            PlatformServiceRole::TimeZoneName,
            PlatformServiceRole::TimeObserve,
            PlatformServiceRole::TimeResolve,
            PlatformServiceRole::TimeParse,
            PlatformServiceRole::TimeFormat,
            PlatformServiceRole::DynamicLibraryOpenPath,
            PlatformServiceRole::DynamicLibraryOpenSystem,
            PlatformServiceRole::DynamicLibrarySymbol,
            PlatformServiceRole::DynamicLibraryClose,
        ],
    },
    Partition {
        name: "bray_platform_standard_streams",
        feature: "standard-streams",
        uses_rust_standard_library: false,
        roles: &[
            PlatformServiceRole::StandardInputRead,
            PlatformServiceRole::StandardOutputWrite,
            PlatformServiceRole::StandardOutputFlush,
            PlatformServiceRole::StandardOutputLock,
            PlatformServiceRole::StandardOutputUnlock,
            PlatformServiceRole::StandardErrorWrite,
            PlatformServiceRole::StandardErrorFlush,
            PlatformServiceRole::StandardErrorLock,
            PlatformServiceRole::StandardErrorUnlock,
        ],
    },
    Partition {
        name: "bray_platform_filesystem",
        feature: "filesystem",
        uses_rust_standard_library: true,
        roles: &[
            PlatformServiceRole::FileRead,
            PlatformServiceRole::FileWrite,
            PlatformServiceRole::FileFlush,
            PlatformServiceRole::FileSeek,
            PlatformServiceRole::FileClose,
            PlatformServiceRole::FileOpen,
            PlatformServiceRole::FileMetadata,
            PlatformServiceRole::PathMetadata,
            PlatformServiceRole::DirectoryOpen,
            PlatformServiceRole::DirectoryNext,
            PlatformServiceRole::DirectoryClose,
            PlatformServiceRole::PathCreateDirectory,
            PlatformServiceRole::PathRemoveFile,
            PlatformServiceRole::PathRemoveDirectory,
            PlatformServiceRole::PathRename,
        ],
    },
    Partition {
        name: "bray_platform_process",
        feature: "process",
        uses_rust_standard_library: true,
        roles: &[
            PlatformServiceRole::ProcessPipeRead,
            PlatformServiceRole::ProcessPipeWrite,
            PlatformServiceRole::ProcessPipeFlush,
            PlatformServiceRole::ProcessPipeClose,
            PlatformServiceRole::ChildSpawn,
            PlatformServiceRole::ChildWait,
            PlatformServiceRole::ChildTerminate,
            PlatformServiceRole::ChildReap,
            PlatformServiceRole::ChildDispose,
        ],
    },
];

pub(super) fn inventory_matches(bindings: &[PlatformServiceBinding]) -> bool {
    if bindings.is_empty() {
        return true;
    }

    let provided_roles = PARTITIONS
        .iter()
        .flat_map(|partition| partition.roles.iter().copied())
        .collect::<Vec<_>>();

    let provided = provided_roles.iter().copied().collect::<BTreeSet<_>>();

    let required = bindings
        .iter()
        .map(PlatformServiceBinding::role)
        .collect::<BTreeSet<_>>();

    provided_roles.len() == provided.len() && provided == required
}
