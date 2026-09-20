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
        let Some(kind) = component.kind else {
            continue;
        };

        let symbols = crate::native_symbols::defined_external_symbols(
            root,
            &component.archive,
            target.object_format(),
        )
        .map_err(CommandError::NativeSymbolInspection)?;

        let strong = symbols
            .iter()
            .filter(|symbol| !is_fallback(kind, symbol))
            .map(|symbol| symbol.name().to_owned())
            .collect::<Vec<_>>();

        let weak = symbols
            .iter()
            .filter(|symbol| is_fallback(kind, symbol))
            .map(|symbol| symbol.name().to_owned())
            .collect::<Vec<_>>();

        validate_exports(kind, &strong, &weak)?;
    }

    Ok(())
}

fn validate_exports(
    kind: RuntimeArchiveKind,
    strong: &[String],
    weak: &[String],
) -> Result<(), CommandError> {
    let required = kind
        .runtime_roles()
        .filter_map(RuntimeAbiRole::native_symbol)
        .chain(
            kind.platform_services()
                .map(PlatformServiceRole::native_symbol),
        )
        .chain(support_exports(kind));

    crate::native_symbols::validate_role_exports(strong, required)
        .map_err(|error| CommandError::RuntimeRoleExports { kind, error })?;

    let forbidden = strong
        .iter()
        .chain(weak)
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
    matches!(
        kind,
        RuntimeArchiveKind::Host | RuntimeArchiveKind::TestHost
    )
    .then_some([
        "bray_runtime_substrate_initialization",
        "bray_runtime_substrate_shutdown",
        "bray_runtime_substrate_report_primary",
    ])
    .into_iter()
    .flatten()
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

    fn validate(kind: RuntimeArchiveKind, strong: &[String]) -> Result<(), CommandError> {
        validate_exports(kind, strong, &[])
    }

    #[test]
    fn every_archive_accepts_its_complete_catalog_contract() {
        for kind in RuntimeArchiveKind::ALL {
            assert!(validate(kind, &exports(kind)).is_ok(), "{kind:?}");
        }
    }

    #[test]
    fn report_provider_roles_have_one_bray_owner_and_no_rust_record_arena() {
        for role in [
            RuntimeAbiRole::ReportRecordAdmission,
            RuntimeAbiRole::ReportRecordTake,
            RuntimeAbiRole::ReportRecordExchange,
            RuntimeAbiRole::ReportRecordAppend,
            RuntimeAbiRole::ReportRecordPop,
            RuntimeAbiRole::ReportSegmentMark,
            RuntimeAbiRole::ReportSegmentTake,
            RuntimeAbiRole::ReportConsumer,
            RuntimeAbiRole::OutgoingAdmission,
            RuntimeAbiRole::OutgoingDischarge,
            RuntimeAbiRole::OutgoingActivation,
            RuntimeAbiRole::OutgoingRetirement,
            RuntimeAbiRole::PanicReportSuppression,
            RuntimeAbiRole::PanicReportConstruction,
        ] {
            let owners: Vec<_> = RuntimeArchiveKind::ALL
                .into_iter()
                .filter(|kind| kind.runtime_roles().any(|candidate| candidate == role))
                .collect();

            assert_eq!(owners, [RuntimeArchiveKind::Bootstrap], "{role:?}");
        }

        let outgoing = include_str!("../../../crates/bray-runtime/src/outgoing.rs");

        for obsolete in ["struct Record", "static RECORDS", "Mutex", "Vec<"] {
            assert!(
                !outgoing.contains(obsolete),
                "Rust report storage remains: {obsolete}"
            );
        }

        assert!(
            !include_str!("../../../crates/bray-runtime/src/frame.rs")
                .contains("consume_native_report")
        );

        assert!(
            !include_str!("../../../crates/bray-runtime/src/native/callback.rs")
                .contains("substrate_panic_report_initialization")
        );
    }

    #[test]
    fn rejects_unprojected_exports_and_misplaced_support() {
        for symbol in [
            "bray_runtime_unprojected_operation",
            "bray_platform_unprojected_operation",
        ] {
            let mut host = exports(RuntimeArchiveKind::Host);
            host.push(symbol.to_owned());

            assert!(matches!(validate(RuntimeArchiveKind::Host, &host),
                Err(CommandError::RuntimeRoleExports { error, .. })
                if error.unexpected == [symbol]));
        }

        let misplaced = support_exports(RuntimeArchiveKind::Host).next().unwrap();

        let mut callback = exports(RuntimeArchiveKind::Callback);
        callback.push(misplaced.to_owned());

        assert!(matches!(validate(RuntimeArchiveKind::Callback, &callback),
            Err(CommandError::RuntimeRoleExports { error, .. })
            if error.unexpected == [misplaced]));
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

                assert!(matches!(validate(kind, &missing),
                Err(CommandError::RuntimeRoleExports { error, .. })
                if error.missing == [symbol]));

                let mut duplicate = exports(kind);
                duplicate.push(symbol.to_owned());

                assert!(matches!(validate(kind, &duplicate),
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
            validate(
                RuntimeArchiveKind::TestHost,
                &exports(RuntimeArchiveKind::TestHost)
            )
            .is_ok()
        );

        assert!(matches!(validate(RuntimeArchiveKind::Host, &host),
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

        assert!(matches!(validate(RuntimeArchiveKind::Host, &host),
            Err(CommandError::RuntimeRoleExports { error, .. })
            if error.unexpected == [misplaced] && error.duplicates == [duplicate]));
    }

    #[test]
    fn weak_platform_fallbacks_are_not_owned_exports() {
        let kind = RuntimeArchiveKind::Observation;
        let fallback = PlatformServiceRole::FileOpen.native_symbol().to_owned();

        assert!(validate_exports(kind, &exports(kind), std::slice::from_ref(&fallback)).is_ok());

        let mut strong = exports(kind);
        strong.push(fallback.clone());

        assert!(matches!(validate_exports(kind, &strong, &[]),
            Err(CommandError::RuntimeRoleExports { error, .. })
            if error.unexpected == [fallback]));
    }
}

fn is_fallback(
    kind: RuntimeArchiveKind,
    symbol: &crate::native_symbols::DefinedExternalSymbol,
) -> bool {
    symbol.is_weak()
        || symbol.is_coff_comdat()
            && matches!(
                kind,
                RuntimeArchiveKind::Bootstrap | RuntimeArchiveKind::Observation
            )
            && !kind
                .platform_services()
                .any(|role| role.native_symbol() == symbol.name())
}
