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
        .chain(support_exports(kind));

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

pub(super) fn support_exports(kind: RuntimeArchiveKind) -> impl Iterator<Item = &'static str> {
    let bootstrap = (kind == RuntimeArchiveKind::Bootstrap)
        .then_some([
            "bray_runtime_bootstrap_thread_static_probe",
            "bray_runtime_bootstrap_thread_static_cleanup_observation",
        ])
        .into_iter()
        .flatten();

    let observation = (kind == RuntimeArchiveKind::Observation)
        .then_some([
            bray_runtime_abi::MEMORY_OBSERVATION_BEGIN_SYMBOL,
            bray_runtime_abi::MEMORY_ALLOCATION_OBSERVATION_SYMBOL,
            bray_runtime_abi::MEMORY_COPY_OBSERVATION_SYMBOL,
            bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL,
            bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL,
        ])
        .into_iter()
        .flatten();

    let host = matches!(
        kind,
        RuntimeArchiveKind::Host | RuntimeArchiveKind::TestHost
    )
    .then_some([
        "bray_runtime_substrate_initialization",
        "bray_runtime_substrate_product_host_control",
        "bray_runtime_substrate_thread_attachment_identity",
        "bray_runtime_substrate_thread_static_cleanup_registration",
        "bray_runtime_substrate_static_outcome_reporting",
        "bray_runtime_substrate_shutdown",
    ])
    .into_iter()
    .flatten();

    let callback = matches!(
        kind,
        RuntimeArchiveKind::Callback | RuntimeArchiveKind::TestHost
    )
    .then_some([
        "bray_runtime_substrate_panic_report_initialization",
        "bray_runtime_substrate_synchronous_root_execution",
        "bray_runtime_substrate_foreign_callback_execution",
        "bray_runtime_substrate_native_thread_execution",
    ])
    .into_iter()
    .flatten();

    bootstrap.chain(observation).chain(host).chain(callback)
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
            .chain(support_exports(kind))
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
    fn rejects_unprojected_exports_and_misplaced_support() {
        let misplaced = support_exports(RuntimeArchiveKind::Callback)
            .next()
            .unwrap();

        for symbol in [
            "bray_runtime_unprojected_operation",
            "bray_platform_unprojected_operation",
            misplaced,
        ] {
            let mut host = exports(RuntimeArchiveKind::Host);
            host.push(symbol.to_owned());

            assert!(matches!(validate_exports(RuntimeArchiveKind::Host, &host),
                Err(CommandError::RuntimeRoleExports { error, .. })
                if error.unexpected == [symbol]));
        }
    }

    #[test]
    fn each_archive_requires_owned_and_inherited_support_once() {
        for kind in RuntimeArchiveKind::ALL {
            let owners = match kind {
                RuntimeArchiveKind::TestHost => {
                    vec![RuntimeArchiveKind::Host, RuntimeArchiveKind::Callback]
                }
                _ => vec![kind],
            };

            for symbol in owners.into_iter().flat_map(support_exports) {
                let mut missing = exports(kind);
                missing.retain(|candidate| candidate != symbol);

                assert!(matches!(validate_exports(kind, &missing),
                Err(CommandError::RuntimeRoleExports { error, .. })
                if error.missing == [symbol]));

                let mut duplicate = exports(kind);
                duplicate.push(symbol.to_owned());

                assert!(matches!(validate_exports(kind, &duplicate),
                Err(CommandError::RuntimeRoleExports { error, .. })
                if error.duplicates == [symbol]));
            }
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
