use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_codegen::{BackendArtifactKind, CodegenSpecialization, CodegenSymbolKey, CodegenUnitKey};
use bray_emitter::{
    ArtifactKind, ArtifactRole, EmissionPlan, LinkStaging, LinkStagingError, StagedArtifact,
};
use bray_linker::{LinkInputMode, LinkInputProvenance, LinkInputSource, LinkInputSpec};
use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeIndexError, NativeUnit, NativeUnitKind,
    NativeUnitSummary,
};
use bray_package_interface::{
    CURRENT_TEMPLATE_SCHEMA_REVISION, ImplementationExternalSymbolIdentity, InterfaceArtifact,
    InterfaceNativeBinding, InterfaceValidationLimits, PackageImplementationArtifact,
    PackageImplementationSpecializationKey, PackageInterfaceExportBundle,
};
use bray_symbols::SymbolKeyData;
use bray_target::NativeTarget;

use super::diagnostics::ProductEmissionErrorKind;
use crate::compilation::{Compilation, NativeProductPlan};
use crate::fact::CancellationToken;

pub(super) fn stage_selected_native_inputs(
    native: &NativeProductPlan,
    staging: &LinkStaging,
    cancellation: &CancellationToken,
) -> Result<Vec<LinkInputSpec>, ProductEmissionErrorKind> {
    let target = NativeTarget::for_identity(native.target().identity())
        .expect("selected native product target must be supported");

    let mut seen = std::collections::BTreeSet::new();

    let selected = native
        .selected_native_units()
        .iter()
        .filter(|unit| seen.insert((unit.package.clone(), unit.digest)))
        .collect::<Vec<_>>();

    let imported = selected
        .iter()
        .copied()
        .enumerate()
        .map(|(ordinal, unit)| {
            let kind = unit.kind.into();

            let path = staging
                .stage_imported_native_unit(ordinal, kind, target, &unit.bytes, cancellation)
                .map_err(product_staging_error)?;

            LinkInputSpec::try_new(
                kind,
                LinkInputSource::file(path),
                LinkInputProvenance::Package(unit.package.clone()),
                LinkInputMode::Ordinary,
            )
            .map_err(|error| {
                ProductEmissionErrorKind::LinkPlan(
                    bray_emitter::LinkPlanConstructionError::InvalidInput(error),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let native_links = selected
        .iter()
        .flat_map(|unit| unit.native_links.iter().cloned());

    Ok(imported.into_iter().chain(native_links).collect())
}

pub(super) fn product_staging_error(error: LinkStagingError) -> ProductEmissionErrorKind {
    if error == LinkStagingError::Cancelled {
        ProductEmissionErrorKind::Cancelled
    } else {
        ProductEmissionErrorKind::Staging(error)
    }
}

pub(super) fn package_native_implementation(
    compilation: &Compilation,
    plan: &EmissionPlan,
    staging: &LinkStaging,
    native: &NativeProductPlan,
    contributions: &bray_emitter::BackendContributionSet,
    interface: &InterfaceArtifact,
    bundle: &PackageInterfaceExportBundle,
    linking: Option<(&bray_linker::Linker, &bray_emitter::ProductLinkInputs)>,
    cancellation: &CancellationToken,
) -> Result<Vec<(ArtifactKind, PackageImplementationArtifact)>, ProductEmissionErrorKind> {
    let target = NativeTarget::ALL
        .into_iter()
        .find(|target| target.as_str() == native.target().profile().identity().as_str())
        .expect("native product target must be one of the toolchain's supported targets");

    let mut units = Vec::new();
    let mut primary_members = Vec::new();
    let mut opaque_keys = Vec::new();
    let mut primary_is_opaque = false;
    let mut primary_has_bitcode = false;
    let mut payloads = Vec::new();
    let mut unit_digests = BTreeMap::new();

    let wants_bitcode = plan
        .request()
        .artifact(ArtifactKind::PackageNativeImplementation)
        .is_some();

    let mut bitcode_units = Vec::new();
    let mut bitcode_payloads = Vec::new();
    let mut bitcode_members = Vec::new();
    let mut bitcode_is_opaque = false;

    let contributions: BTreeMap<_, _> = contributions
        .contributions()
        .iter()
        .map(|contribution| (contribution.id(), contribution))
        .collect();

    for staged in staging.inputs() {
        let result = native_contribution_unit(
            plan,
            staged,
            contributions[staged.artifact()],
            compilation.native_link_inputs(),
        )?;

        let Some((key, unit, bytes)) = result else {
            continue;
        };

        let mapping = native
            .mappings()
            .iter()
            .find(|mapping| mapping.unit() == &key)
            .expect("staged native unit must have mappings");

        let unit = unit.with_statics(
            native
                .native_statics()
                .iter()
                .filter(|entry| {
                    mapping.static_storages().iter().any(|storage| {
                        storage.defines_storage() && storage.host_name() == entry.symbol()
                    })
                })
                .cloned(),
        );

        let digest = unit.digest();

        if wants_bitcode && unit.kind() == NativeUnitKind::Bitcode {
            bitcode_members.push(staged.clone());

            if matches!(unit.summary(), NativeUnitSummary::Opaque { .. }) {
                bitcode_is_opaque = true;
            } else {
                bitcode_units.push(unit.clone());
                bitcode_payloads.push((digest.bytes(), Arc::clone(&bytes)));
            }
        }

        if plan
            .artifact(staged.artifact())
            .expect("staged native artifact must be planned")
            .role()
            != ArtifactRole::LinkInput
        {
            continue;
        }

        primary_members.push(staged.clone());
        primary_has_bitcode |= unit.kind() == NativeUnitKind::Bitcode;

        if matches!(unit.summary(), NativeUnitSummary::Opaque { .. }) {
            primary_is_opaque = true;
            opaque_keys.push(key);
        } else {
            units.push(unit);
            payloads.push((digest.bytes(), bytes));
            unit_digests.insert(key, digest);
        }
    }

    // An opaque member can refer to any member in its producer. Preserve a lazy archive
    // of the complete representation so the native linker can finish that unknown closure.
    let primary_archive = primary_is_opaque
        .then(|| {
            package_opaque_archive(
                compilation,
                plan,
                staging,
                &primary_members,
                linking,
                cancellation,
                &contributions,
            )
        })
        .transpose()?
        .map(|(unit, bytes)| {
            (
                unit.with_statics(native.native_statics().iter().cloned()),
                bytes,
            )
        });

    if let Some((unit, bytes)) = &primary_archive {
        unit_digests.extend(opaque_keys.into_iter().map(|key| (key, unit.digest())));
        payloads.push((unit.digest().bytes(), Arc::clone(bytes)));
        units.push(unit.clone());
    }

    let producer = crate::compilation::product::native_producer_identity(
        native.backend().identity(),
        native.options(),
        bundle.implementation_configuration(),
        target,
    );

    let index = NativeArtifactIndex::try_new(target, producer, units, [])
        .expect("compiler-produced native unit set must form a valid index");

    let index = if primary_has_bitcode {
        index.with_bitcode_toolchain(native.backend().identity().toolchain_revision())
    } else {
        index
    };

    let index_bytes = match index.encode() {
        Ok(bytes) => bytes,
        Err(NativeIndexError::SizeLimitExceeded) => {
            return Err(ProductEmissionErrorKind::NativeIndexSizeLimitExceeded);
        }
        Err(error) => panic!("compiler-produced native index must encode: {error:?}"),
    };

    let bindings = native_bindings(compilation, native, bundle, target, &index, &unit_digests)?;

    let artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
        interface,
        bundle,
        &index_bytes,
        &payloads,
        &bindings,
        InterfaceValidationLimits::default(),
    )
    .map_err(ProductEmissionErrorKind::PackageImplementation)?;

    artifact
        .native_artifact()
        .expect("compiler-produced native package must authenticate before publication");

    let mut artifacts = Vec::new();

    if wants_bitcode {
        if bitcode_is_opaque {
            let (unit, bytes) = match primary_archive.filter(|_| primary_members == bitcode_members)
            {
                Some(archive) => archive,
                None => package_opaque_archive(
                    compilation,
                    plan,
                    staging,
                    &bitcode_members,
                    linking,
                    cancellation,
                    &contributions,
                )?,
            };

            bitcode_payloads.push((unit.digest().bytes(), bytes));
            bitcode_units.push(unit.with_statics(native.native_statics().iter().cloned()));
        }

        let index = NativeArtifactIndex::try_new(target, producer, bitcode_units, [])
            .expect("compiler-produced bitcode variant must form a valid index")
            .with_bitcode_toolchain(native.backend().identity().toolchain_revision());

        let bytes = index.encode().map_err(|error| match error {
            NativeIndexError::SizeLimitExceeded => {
                ProductEmissionErrorKind::NativeIndexSizeLimitExceeded
            }
            error => panic!("compiler-produced bitcode index must encode: {error:?}"),
        })?;

        let bitcode = artifact
            .try_native_only_artifact(&[(NativeUnitKind::Bitcode, &bytes)], &bitcode_payloads)
            .map_err(ProductEmissionErrorKind::PackageImplementation)?;

        artifacts.push((ArtifactKind::PackageNativeImplementation, bitcode));
    }

    if plan
        .request()
        .artifact(ArtifactKind::PackageImplementation)
        .is_some()
    {
        artifacts.push((ArtifactKind::PackageImplementation, artifact));
    }

    Ok(artifacts)
}

fn package_opaque_archive(
    compilation: &Compilation,
    plan: &EmissionPlan,
    staging: &LinkStaging,
    members: &[StagedArtifact],
    linking: Option<(&bray_linker::Linker, &bray_emitter::ProductLinkInputs)>,
    cancellation: &CancellationToken,
    contributions: &BTreeMap<&bray_emitter::ArtifactId, &bray_emitter::ArtifactContribution>,
) -> Result<(NativeUnit, Arc<[u8]>), ProductEmissionErrorKind> {
    let (linker, inputs) = linking.ok_or(ProductEmissionErrorKind::MissingLinker)?;

    let archive =
        bray_emitter::construct_package_archive_plan(plan, staging, members, inputs, linker)
            .map_err(ProductEmissionErrorKind::LinkPlan)?;

    let outcome = compilation
        .link_product_with_cancellation(linker, &archive, cancellation)
        .map_err(ProductEmissionErrorKind::Query)?;

    match outcome.status() {
        bray_linker::LinkStatus::Complete(_) => {}
        bray_linker::LinkStatus::Cancelled => return Err(ProductEmissionErrorKind::Cancelled),
        bray_linker::LinkStatus::Failed(failure) => {
            return Err(ProductEmissionErrorKind::LinkPlan(
                bray_emitter::LinkPlanConstructionError::Linker(failure.clone()),
            ));
        }
    }

    let path = archive.outputs()[0].destination().path();

    let bytes = std::fs::read(path).map_err(|error| ProductEmissionErrorKind::NativeRead {
        path: path.to_path_buf(),
        kind: error.kind(),
    })?;

    let digest = NativeContentDigest::new(
        bray_base::sha256_reader(bytes.as_slice()).expect("hashing archive memory cannot fail"),
    );

    let member_units = members
        .iter()
        .map(|member| {
            contributions[member.artifact()]
                .native_unit()
                .expect("compiler-produced archive member must have native obligations")
        })
        .collect::<Vec<_>>();

    let summary = NativeUnitSummary::opaque_archive(
        member_units
            .iter()
            .flat_map(|unit| unit.summary().defined_symbols())
            .cloned(),
        member_units
            .iter()
            .flat_map(|unit| unit.summary().references())
            .cloned(),
    );

    Ok((
        NativeUnit::new(
            digest,
            NativeUnitKind::OpaqueArchive,
            summary,
            compilation.native_link_inputs().iter().cloned(),
        ),
        Arc::from(bytes),
    ))
}

fn native_bindings(
    compilation: &Compilation,
    native: &NativeProductPlan,
    bundle: &PackageInterfaceExportBundle,
    target: NativeTarget,
    index: &NativeArtifactIndex,
    unit_digests: &BTreeMap<CodegenUnitKey, NativeContentDigest>,
) -> Result<Vec<InterfaceNativeBinding>, ProductEmissionErrorKind> {
    let graph = compilation
        .symbol_graph()
        .map_err(ProductEmissionErrorKind::Query)?;

    let mut bindings = Vec::new();

    for mappings in native.mappings() {
        let Some(&digest) = unit_digests.get(mappings.unit()) else {
            continue;
        };

        let unit = index
            .units()
            .iter()
            .find(|unit| unit.digest() == digest)
            .expect("indexed unit must be present for every native mapping");

        for mapping in mappings.symbols() {
            if !mapping.defines_in(mappings.unit()) {
                continue;
            }

            let CodegenSymbolKey::Instance(instance) = mapping.key() else {
                continue;
            };

            if instance.specialization() != &CodegenSpecialization::NonGeneric
                || !instance.witnesses().is_empty()
                || instance.contextual_self_witness().is_some()
            {
                continue;
            }

            let bray_ir::MirUnitKey::Bound(bound) = instance.template() else {
                continue;
            };

            let external = match bound.declared_owner().data() {
                SymbolKeyData::External(external) => {
                    // The binding key owns the external identity after this graph borrow ends.
                    external.clone()
                }
                SymbolKeyData::SourceDeclaration { .. } | SymbolKeyData::Synthesized(_) => {
                    let symbol = graph.symbol_for_key(bound.declared_owner()).expect(
                        "emitted source declaration must belong to the loaded symbol graph",
                    );

                    crate::compilation::export::external_symbol_key(
                        graph,
                        compilation.package_identity(),
                        symbol,
                    )
                    .map_err(ProductEmissionErrorKind::PackageInterface)?
                }
                _ => continue,
            };

            let Some(owner) = bundle.surface().symbol_by_external_key(&external) else {
                continue;
            };

            let object_name = target.object_symbol_name(mapping.name().as_str());

            let symbol = match unit.summary() {
                NativeUnitSummary::Exact { definitions, .. } => {
                    definitions.iter().find_map(|definition| {
                        let name = definition.symbol().identity().name()?;

                        (name == object_name)
                            .then(|| NonEmptySharedStr::try_new(name))
                            .flatten()
                    })
                }
                NativeUnitSummary::Opaque { .. } => {
                    NonEmptySharedStr::try_new(object_name.as_ref())
                }
            };

            let Some(symbol) = symbol else {
                continue;
            };

            // The binding key owns the configuration after the export bundle is released.
            let configuration = bundle.implementation_configuration().clone();

            let key = PackageImplementationSpecializationKey::new(
                ImplementationExternalSymbolIdentity::new(&external),
                [],
                [],
                configuration,
                CURRENT_TEMPLATE_SCHEMA_REVISION,
                bundle.surface().dependencies().iter().cloned(),
            );

            bindings.push(
                InterfaceNativeBinding::new(owner, key, *native.options(), digest.bytes(), symbol)
                    .with_main_thread_requirement(native.native_requires_main_thread(instance)),
            );
        }
    }

    Ok(bindings)
}

fn native_contribution_unit(
    plan: &EmissionPlan,
    staged: &StagedArtifact,
    contribution: &bray_emitter::ArtifactContribution,
    native_links: &[bray_symbols::NativeLinkRequirement],
) -> Result<Option<(CodegenUnitKey, NativeUnit, Arc<[u8]>)>, ProductEmissionErrorKind> {
    let planned = plan
        .artifact(staged.artifact())
        .expect("staged native input must belong to the immutable emission plan");

    let Some(artifact) = planned.producer().backend_artifact() else {
        return Ok(None);
    };

    if !matches!(
        artifact.kind(),
        BackendArtifactKind::RelocatableObject | BackendArtifactKind::BackendBitcode
    ) {
        return Ok(None);
    }

    let unit = contribution
        .native_unit()
        .expect("compiler-produced native contribution must carry final-byte obligations");

    let bytes = contribution.content().read_shared().map_err(|error| {
        ProductEmissionErrorKind::NativeRead {
            path: staged.path().to_path_buf(),
            kind: error.source().map_or(io::ErrorKind::Other, io::Error::kind),
        }
    })?;

    // The index retains shared summary arrays after the contribution set is released.
    let unit = NativeUnit::new(
        unit.digest(),
        unit.kind(),
        unit.summary().clone(),
        native_links.iter().cloned(),
    );

    Ok(Some((artifact.unit().clone(), unit, bytes)))
}
