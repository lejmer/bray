use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
use bray_symbols::NativeLinkRequirement;
use bray_target::NativeTarget;

use super::error::BuildError;
use crate::standard_library::optimization::BuiltOptimizationArchive;

const ROLES: &[PlatformServiceRole] = &[
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
    let built = crate::native_archive::build_rust_static_library(
        root,
        native,
        "bray-platform-abi",
        "release",
        &["temporal"],
    )
    .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

    let bytes =
        fs::read(built.archive()).map_err(|error| BuildError::read(built.archive(), error))?;

    let optimization = if NativeTarget::current() == Some(native) {
        let optimized = crate::native_archive::build_thin_lto_rust_static_library(
            root,
            native,
            "bray-platform-abi",
            "release",
            &["temporal"],
            false,
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
                BuildError::NativeArchive(
                    "bray_platform_temporal produced no LLVM optimization modules".to_owned(),
                )
            })?,
        )
    } else {
        None
    };

    Ok(vec![BuiltPlatformArchive {
        name: "bray_platform_temporal",
        roles: ROLES,
        optimization_roles: ROLES,
        uses_temporal_dependency_metadata: true,
        bytes,
        native_links: built.native_links().to_vec(),
        optimization,
    }])
}

pub(super) fn inventory_matches(bindings: &[PlatformServiceBinding]) -> bool {
    let required = bindings
        .iter()
        .map(PlatformServiceBinding::role)
        .collect::<BTreeSet<_>>();

    let provided = ROLES.iter().copied().collect::<BTreeSet<_>>();

    required.len() == bindings.len()
        && provided.len() == ROLES.len()
        && provided.is_subset(&required)
}
