use std::collections::BTreeSet;
use std::path::Path;

use bray_runtime_interface::{PlatformServiceRole, RuntimeAbiRole};
use bray_target::NativeTarget;

use super::command::{CommandError, Package, RuntimeArchiveKind};

pub(super) fn validate(
    root: &Path,
    package: &Package,
    target: NativeTarget,
) -> Result<(), CommandError> {
    for component in &package.components {
        let symbols = crate::native_symbols::defined_exports(
            root,
            &component.archive,
            target.object_format(),
        )
        .map_err(CommandError::NativeSymbolInspection)?;

        validate_exports(component.kind, &symbols)?;
    }

    Ok(())
}

fn validate_exports(kind: RuntimeArchiveKind, symbols: &[String]) -> Result<(), CommandError> {
    let required = kind
        .runtime_roles()
        .filter_map(RuntimeAbiRole::native_symbol)
        .chain(
            kind.platform_services()
                .map(PlatformServiceRole::native_symbol),
        )
        .chain(support_exports(kind).iter().copied());

    crate::native_symbols::validate_role_exports(symbols, required)
        .map_err(|error| CommandError::RuntimeRoleExports { kind, error })?;

    let forbidden = symbols
        .iter()
        .filter(|symbol| {
            kind == RuntimeArchiveKind::Bootstrap
                && ["__rust_", "rust_"]
                    .iter()
                    .any(|prefix| symbol.starts_with(prefix))
                || ["bray_runtime_string_", "bray_runtime_character_"]
                    .iter()
                    .any(|prefix| symbol.starts_with(prefix))
                || kind != RuntimeArchiveKind::Observation
                    && symbol.starts_with("bray_runtime_memory_")
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();

    if !forbidden.is_empty() {
        return Err(CommandError::RuntimeComponentBoundary { kind, forbidden });
    }

    Ok(())
}

fn support_exports(kind: RuntimeArchiveKind) -> &'static [&'static str] {
    match kind {
        RuntimeArchiveKind::Observation => &[
            bray_runtime_abi::MEMORY_OBSERVATION_BEGIN_SYMBOL,
            bray_runtime_abi::MEMORY_ALLOCATION_OBSERVATION_SYMBOL,
            bray_runtime_abi::MEMORY_COPY_OBSERVATION_SYMBOL,
            bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL,
            bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL,
        ],
        RuntimeArchiveKind::Host => &[
            "bray_runtime_substrate_initialization",
            "bray_runtime_substrate_shutdown",
        ],
        RuntimeArchiveKind::Callback => &[
            "bray_runtime_substrate_panic_reporting",
            "bray_runtime_substrate_synchronous_root_execution",
            "bray_runtime_substrate_foreign_callback_execution",
            "bray_runtime_substrate_native_thread_execution",
        ],
        RuntimeArchiveKind::Common
        | RuntimeArchiveKind::TestCommon
        | RuntimeArchiveKind::Bootstrap
        | RuntimeArchiveKind::Scheduler
        | RuntimeArchiveKind::Cancellation
        | RuntimeArchiveKind::Event
        | RuntimeArchiveKind::TestHost => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::{support_exports, validate_exports};
    use crate::runtime_artifact::command::{CommandError, RuntimeArchiveKind};
    use bray_runtime_interface::{PlatformServiceRole, RuntimeAbiRole};

    fn exports(kind: RuntimeArchiveKind) -> Vec<String> {
        kind.runtime_roles()
            .filter_map(RuntimeAbiRole::native_symbol)
            .chain(
                kind.platform_services()
                    .map(PlatformServiceRole::native_symbol),
            )
            .chain(support_exports(kind).iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn every_archive_accepts_its_complete_catalog_contract() {
        for kind in RuntimeArchiveKind::ALL {
            assert!(validate_exports(kind, &exports(kind)).is_ok(), "{kind:?}");
        }
    }

    #[test]
    fn test_host_definition_cannot_replace_an_ordinary_owner() {
        let mut host = exports(RuntimeArchiveKind::Host);

        let symbol = RuntimeArchiveKind::Host
            .runtime_roles()
            .next()
            .unwrap()
            .native_symbol()
            .unwrap();

        host.retain(|candidate| candidate != symbol);

        assert!(
            validate_exports(
                RuntimeArchiveKind::TestHost,
                &exports(RuntimeArchiveKind::TestHost)
            )
            .is_ok()
        );

        assert!(matches!(validate_exports(RuntimeArchiveKind::Host, &host),
            Err(CommandError::RuntimeRoleExports { error, .. }) if error.missing == [symbol]));
    }

    #[test]
    fn rejects_misplaced_and_duplicate_role_definitions() {
        let mut host = exports(RuntimeArchiveKind::Host);

        let misplaced = RuntimeArchiveKind::Scheduler
            .runtime_roles()
            .next()
            .unwrap()
            .native_symbol()
            .unwrap();

        let duplicate = RuntimeArchiveKind::Host
            .runtime_roles()
            .next()
            .unwrap()
            .native_symbol()
            .unwrap();

        host.push(misplaced.to_owned());
        host.push(duplicate.to_owned());

        assert!(matches!(validate_exports(RuntimeArchiveKind::Host, &host),
            Err(CommandError::RuntimeRoleExports { error, .. })
            if error.unexpected == [misplaced] && error.duplicates == [duplicate]));
    }
}
