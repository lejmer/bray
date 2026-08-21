use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
use bray_symbols::NativeLinkRequirement;
use bray_target::NativeTarget;

use super::error::BuildError;
use crate::standard_library::optimization::BuiltOptimizationArchive;

pub(super) struct Partition {
    pub(super) name: &'static str,
    pub(super) feature: &'static str,
    pub(super) uses_rust_standard_library: bool,
    pub(super) roles: &'static [PlatformServiceRole],
    pub(super) optimization_roles: &'static [PlatformServiceRole],
    pub(super) uses_temporal_dependency_metadata: bool,
}

const CORE_OPTIMIZATION_ROLES: &[PlatformServiceRole] = &[
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
];

const STANDARD_STREAM_ROLES: &[PlatformServiceRole] = &[
    PlatformServiceRole::StandardInputRead,
    PlatformServiceRole::StandardInputLock,
    PlatformServiceRole::StandardInputUnlock,
    PlatformServiceRole::StandardOutputWrite,
    PlatformServiceRole::StandardOutputFlush,
    PlatformServiceRole::StandardOutputLock,
    PlatformServiceRole::StandardOutputUnlock,
    PlatformServiceRole::StandardErrorWrite,
    PlatformServiceRole::StandardErrorFlush,
    PlatformServiceRole::StandardErrorLock,
    PlatformServiceRole::StandardErrorUnlock,
];

pub(super) const PARTITIONS: &[Partition] = &[
    Partition {
        name: "bray_platform_core",
        feature: "core",
        uses_rust_standard_library: true,
        roles: &[
            PlatformServiceRole::ContextIdentity,
            PlatformServiceRole::ContextNativeTextWidth,
            PlatformServiceRole::ContextWorkingDirectory,
            PlatformServiceRole::ContextArgumentCount,
            PlatformServiceRole::ContextArgument,
            PlatformServiceRole::ContextEnvironmentCount,
            PlatformServiceRole::ContextEnvironmentEntry,
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
        optimization_roles: CORE_OPTIMIZATION_ROLES,
        uses_temporal_dependency_metadata: true,
    },
    Partition {
        name: "bray_platform_standard_streams",
        feature: "standard-streams",
        uses_rust_standard_library: false,
        roles: STANDARD_STREAM_ROLES,
        optimization_roles: STANDARD_STREAM_ROLES,
        uses_temporal_dependency_metadata: false,
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
        optimization_roles: &[],
        uses_temporal_dependency_metadata: false,
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
        optimization_roles: &[],
        uses_temporal_dependency_metadata: false,
    },
];

pub(super) struct BuiltPlatformArchive {
    pub(super) name: &'static str,
    pub(super) roles: &'static [PlatformServiceRole],
    pub(super) optimization_roles: &'static [PlatformServiceRole],
    pub(super) uses_temporal_dependency_metadata: bool,
    pub(super) bytes: Vec<u8>,
    pub(super) native_links: Vec<NativeLinkRequirement>,
    pub(super) optimization: Option<BuiltOptimizationArchive>,
}

pub(super) fn build_archives(
    root: &Path,
    native: NativeTarget,
    work: &Path,
) -> Result<Vec<BuiltPlatformArchive>, BuildError> {
    PARTITIONS
        .iter()
        .map(|partition| build_archive(root, native, work, partition))
        .collect()
}

fn build_archive(
    root: &Path,
    native: NativeTarget,
    work: &Path,
    partition: &Partition,
) -> Result<BuiltPlatformArchive, BuildError> {
    let build = if partition.uses_rust_standard_library {
        crate::native_archive::build_rust_static_library
    } else {
        crate::native_archive::build_no_std_rust_static_library
    };

    let built = build(
        root,
        native,
        "bray-platform-abi",
        "release",
        &[partition.feature],
    )
    .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

    let bytes =
        fs::read(built.archive()).map_err(|error| BuildError::read(built.archive(), error))?;

    let optimization = if partition.optimization_roles.is_empty()
        || !native_optimization_is_supported(NativeTarget::current(), native)
    {
        None
    } else {
        let optimized = crate::native_archive::build_thin_lto_rust_static_library(
            root,
            native,
            "bray-platform-abi",
            "release",
            &[partition.feature],
            !partition.uses_rust_standard_library,
        )
        .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

        Some(
            crate::standard_library::optimization::from_native_archive(
                root,
                work,
                optimized.archive(),
                native,
            )?
            .ok_or_else(|| {
                BuildError::NativeArchive(format!(
                    "{} produced no LLVM optimization modules",
                    partition.name
                ))
            })?,
        )
    };

    Ok(BuiltPlatformArchive {
        name: partition.name,
        roles: partition.roles,
        optimization_roles: partition.optimization_roles,
        uses_temporal_dependency_metadata: partition.uses_temporal_dependency_metadata,
        bytes,
        native_links: built.native_links().to_vec(),
        optimization,
    })
}

fn native_optimization_is_supported(host: Option<NativeTarget>, target: NativeTarget) -> bool {
    host == Some(target)
}

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

#[cfg(test)]
mod tests {
    use bray_target::NativeTarget;

    use super::native_optimization_is_supported;

    #[test]
    fn native_optimization_requires_the_target_sdk_of_the_build_host() {
        assert!(native_optimization_is_supported(
            Some(NativeTarget::X86_64WindowsMsvc),
            NativeTarget::X86_64WindowsMsvc,
        ));

        assert!(!native_optimization_is_supported(
            Some(NativeTarget::X86_64WindowsMsvc),
            NativeTarget::X86_64LinuxGnu,
        ));

        assert!(!native_optimization_is_supported(
            None,
            NativeTarget::Aarch64MacOs,
        ));
    }
}
