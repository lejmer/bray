use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceFamily, PlatformServiceRole};
use bray_symbols::NativeLinkRequirement;
use bray_target::NativeTarget;

use super::error::BuildError;
use crate::standard_library::optimization::BuiltOptimizationArchive;

pub(super) struct BuiltPlatformArchive {
    pub(super) name: &'static str,
    pub(super) roles: Vec<PlatformServiceRole>,
    pub(super) uses_temporal_dependency_metadata: bool,
    pub(super) bytes: Vec<u8>,
    pub(super) native_links: Vec<NativeLinkRequirement>,
    pub(super) optimization: Option<BuiltOptimizationArchive>,
}

pub(super) fn build_archives(
    root: &Path,
    native: NativeTarget,
    work: &Path,
    bindings: &[PlatformServiceBinding],
) -> Result<Vec<BuiltPlatformArchive>, BuildError> {
    let families = bindings
        .iter()
        .map(|binding| binding.role().family())
        .collect::<BTreeSet<_>>();

    families
        .into_iter()
        .filter_map(|family| provider(family).map(|configuration| (family, configuration)))
        .map(|(family, (feature, name))| build_archive(root, native, work, family, feature, name))
        .collect()
}

fn provider(family: PlatformServiceFamily) -> Option<(&'static str, &'static str)> {
    match family {
        PlatformServiceFamily::Temporal => Some(("temporal", "bray_platform_temporal")),
        PlatformServiceFamily::DynamicLibrary => Some(("dynamic", "bray_platform_dynamic")),
        PlatformServiceFamily::Core
        | PlatformServiceFamily::StandardStreams
        | PlatformServiceFamily::Filesystem
        | PlatformServiceFamily::Process
        | PlatformServiceFamily::Thread => None,
    }
}

fn build_archive(
    root: &Path,
    native: NativeTarget,
    work: &Path,
    family: PlatformServiceFamily,
    feature: &str,
    name: &'static str,
) -> Result<BuiltPlatformArchive, BuildError> {
    let built = crate::native_archive::build_rust_static_library(
        root,
        native,
        "bray-platform-abi",
        "release",
        &[feature],
    )
    .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

    let roles = family.roles().collect::<Vec<_>>();

    validate_exports(root, native, built.archive(), family, &roles)?;

    let bytes =
        fs::read(built.archive()).map_err(|error| BuildError::read(built.archive(), error))?;

    let optimization = if NativeTarget::current() == Some(native) {
        let optimized = crate::native_archive::build_thin_lto_rust_static_library(
            root,
            native,
            "bray-platform-abi",
            "release",
            &[feature],
            false,
        )
        .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

        validate_exports(root, native, optimized.archive(), family, &roles)?;

        Some(
            crate::standard_library::optimization::from_native_archive(
                root,
                work,
                optimized.archive(),
                native,
            )?
            .ok_or_else(|| {
                BuildError::NativeArchive(format!("{name} produced no LLVM optimization modules"))
            })?,
        )
    } else {
        None
    };

    Ok(BuiltPlatformArchive {
        name,
        roles,
        uses_temporal_dependency_metadata: family == PlatformServiceFamily::Temporal,
        bytes,
        native_links: built.native_links().to_vec(),
        optimization,
    })
}

fn validate_exports(
    root: &Path,
    target: NativeTarget,
    archive: &Path,
    family: PlatformServiceFamily,
    roles: &[PlatformServiceRole],
) -> Result<(), BuildError> {
    let exports = crate::native_symbols::defined_exports(root, archive, target.object_format())
        .map_err(BuildError::NativeSymbolInspection)?;

    crate::native_symbols::validate_role_exports(
        &exports,
        roles
            .iter()
            .copied()
            .map(PlatformServiceRole::native_symbol),
    )
    .map_err(|error| BuildError::PlatformRoleExports { family, error })
}

pub(super) fn inventory_matches(bindings: &[PlatformServiceBinding]) -> bool {
    let required = bindings
        .iter()
        .map(PlatformServiceBinding::role)
        .collect::<BTreeSet<_>>();

    required.len() == bindings.len()
        && required.iter().all(|role| {
            provider(role.family()).is_none()
                || role.family().roles().all(|role| required.contains(&role))
        })
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{
        PlatformServiceBinding, PlatformServiceFamily, PlatformServiceRole,
    };

    use super::{inventory_matches, provider};

    #[test]
    fn providers_require_complete_independently_selected_inventories() {
        assert!(inventory_matches(&[]));

        assert!(inventory_matches(&[binding(
            PlatformServiceRole::StandardOutputWrite
        )]));

        for family in [
            PlatformServiceFamily::Temporal,
            PlatformServiceFamily::DynamicLibrary,
        ] {
            let roles = family.roles().collect::<Vec<_>>();
            let complete = roles.iter().copied().map(binding).collect::<Vec<_>>();

            assert!(provider(family).is_some());
            assert!(!inventory_matches(&[binding(roles[0])]));
            assert!(inventory_matches(&complete));

            let mut duplicate = complete;
            duplicate.push(binding(roles[0]));
            assert!(!inventory_matches(&duplicate));
        }
    }

    fn binding(role: PlatformServiceRole) -> PlatformServiceBinding {
        let declaration = format!("std.platform_service_{}", role.id());

        PlatformServiceBinding::try_new(role, &declaration)
            .unwrap_or_else(|| panic!("platform service binding must validate"))
    }
}
