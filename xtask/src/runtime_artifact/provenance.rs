use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use bray_compilation::SelectedTarget;
use bray_native_artifact::{NativeContentDigest, NativeUnit, NativeUnitKind};
use bray_package_interface::PackageArtifactInput;
use bray_target::NativeTarget;

use super::bootstrap::BrayRuntimeComponent;
use super::command::{CommandError, Package};

pub(super) fn audit_product_runtime(
    native: &bray_compilation::NativeProductPlan,
    bray_units: &BTreeSet<NativeContentDigest>,
    output: &Path,
) -> Result<(), String> {
    let Some(runtime) = native
        .link()
        .and_then(bray_emitter::ProductLinkInputs::runtime)
    else {
        return Ok(());
    };

    let mut receipt = String::new();

    for unit in runtime.native_units() {
        if unit.kind() == NativeUnitKind::OpaqueArchive || !bray_units.contains(&unit.digest()) {
            return Err(format!(
                "product selected non-Bray runtime unit {}",
                unit.digest()
            ));
        }

        use std::fmt::Write;

        writeln!(
            receipt,
            "{} {:?} {:?}",
            unit.digest(),
            unit.kind(),
            unit.inclusion(),
        )
        .expect("writing runtime inclusion receipt to a string must succeed");
    }

    fs::write(output.with_extension("runtime-units"), receipt)
        .map_err(|error| format!("could not write runtime inclusion receipt: {error}"))?;

    Ok(())
}

fn bray_unit_digests(
    output: &Path,
    target: NativeTarget,
) -> Result<BTreeSet<NativeContentDigest>, String> {
    let mut units = BTreeSet::new();

    for component in BrayRuntimeComponent::ALL {
        let implementation =
            PackageArtifactInput::file(component.implementation_path(output, target), None)
                .load_implementation()
                .map_err(|error| format!("runtime provenance could not load: {error:?}"))?;

        let index = implementation
            .native_artifact()
            .expect("producer implementation must authenticate")
            .expect("producer implementation must carry native units");

        units.extend(
            index
                .units()
                .iter()
                .filter(|unit| unit.kind() != NativeUnitKind::OpaqueArchive)
                .map(|unit| unit.digest()),
        );
    }

    Ok(units)
}

pub(super) fn load_runtime_provenance(
    runtime: &Package,
    target: NativeTarget,
) -> Result<(BTreeSet<NativeContentDigest>, Vec<NativeUnit>), CommandError> {
    let output = runtime
        .metadata
        .parent()
        .expect("runtime package has an artifact directory");

    let bray_units = bray_unit_digests(output, target).map_err(CommandError::EntrySmoke)?;

    let artifact = bray_tooling::load_runtime_artifact(
        &runtime.metadata,
        &target.identity(),
        SelectedTarget::for_native(target).runtime_abi(),
    )
    .map_err(|error| {
        CommandError::EntrySmoke(format!(
            "runtime provenance could not authenticate: {error:?}"
        ))
    })?;

    Ok((bray_units, artifact.native_indexes()[0].index().units().to_vec()))
}

pub(super) fn audit_runtime_units(
    units: &[NativeUnit],
    bray_units: &BTreeSet<NativeContentDigest>,
    target: NativeTarget,
    map: &Path,
) -> Result<(), CommandError> {
    let contents = fs::read_to_string(map).map_err(|error| CommandError::read(map, error))?;

    let retained = retained_runtime_units(
        units,
        bray_units,
        target,
        &contents,
    )
    .map_err(CommandError::SynchronousLinkMapBoundary)?;

    fs::write(map.with_extension("runtime-members"), retained.join("\n"))
        .map_err(|error| CommandError::write(map, error))?;

    Ok(())
}

fn retained_runtime_units(
    units: &[NativeUnit],
    bray_units: &BTreeSet<NativeContentDigest>,
    target: NativeTarget,
    contents: &str,
) -> Result<Vec<String>, String> {
    let mut retained = Vec::new();

    // Every bundled runtime input is content addressed. Compare complete member paths
    // with authenticated producer identities, including archives with opaque member names.
    for unit in units {
        let name = unit.kind().file_name(unit.digest(), target);

        if contents.contains(&name) {
            if unit.kind() == NativeUnitKind::OpaqueArchive || !bray_units.contains(&unit.digest())
            {
                return Err(format!("retained non-Bray runtime unit {name}"));
            }

            retained.push(name);
        }
    }

    Ok(retained)
}

#[cfg(test)]
mod tests {
    use super::retained_runtime_units;
    use bray_native_artifact::{NativeContentDigest, NativeUnit};
    use bray_native_artifact::{NativeUnitKind, NativeUnitSummary};
    use bray_target::NativeTarget;
    use std::collections::BTreeSet;

    #[test]
    fn runtime_provenance_rejects_opaque_members_without_rust_symbol_names() {
        let digest = NativeContentDigest::new([7; 32]);
        let target = NativeTarget::X86_64LinuxGnu;

        let unit = NativeUnit::new(
            digest,
            NativeUnitKind::OpaqueArchive,
            NativeUnitSummary::opaque([]),
            [],
        );

        let name = unit.kind().file_name(digest, target);
        let map = format!("{name}(0001.o):(.text.renamed)");

        assert_eq!(
            retained_runtime_units(&[unit.clone()], &BTreeSet::new(), target, &map),
            Err(format!("retained non-Bray runtime unit {name}"))
        );

        assert_eq!(
            retained_runtime_units(&[unit.clone()], &BTreeSet::from([digest]), target, &map),
            Err(format!("retained non-Bray runtime unit {name}"))
        );

        assert_eq!(
            retained_runtime_units(
                &[unit],
                &BTreeSet::new(),
                target,
                "generated.o:(.text.main)"
            ),
            Ok(Vec::new())
        );
    }
}
