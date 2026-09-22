use std::collections::BTreeMap;
use std::hash::Hash;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use bray_base::{NonEmptySharedStr, StableDigestHasher};
use bray_codegen::{BackendArtifactKind, CodegenPartitionPolicy, CodegenSpecialization, CodegenSymbolKey, CodegenUnitKey};
use bray_diagnostics::DiagnosticLlvmToolRole;
use bray_emitter::{EmissionPlan, LinkStaging, StagedArtifact};
use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeIndexError, NativeUnit, NativeUnitKind, NativeUnitSummary,
    scan_native_unit_summary,
};
use bray_package_interface::{
    CURRENT_TEMPLATE_SCHEMA_REVISION, ImplementationExternalSymbolIdentity, InterfaceArtifact,
    InterfaceNativeBinding, InterfaceValidationLimits, PackageImplementationArtifact,
    PackageImplementationSpecializationKey, PackageInterfaceExportBundle,
};
use bray_symbols::SymbolKeyData;
use bray_target::NativeTarget;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

use super::diagnostics::ProductEmissionErrorKind;
use super::execution::NativeInspectionInputs;
use crate::compilation::{Compilation, NativeProductPlan};

pub(super) fn package_native_implementation(
    compilation: &Compilation,
    plan: &EmissionPlan,
    staging: &LinkStaging,
    native: &NativeProductPlan,
    tools: NativeInspectionInputs<'_>,
    interface: &InterfaceArtifact,
    bundle: &PackageInterfaceExportBundle,
) -> Result<PackageImplementationArtifact, ProductEmissionErrorKind> {
    let target = NativeTarget::ALL
        .into_iter()
        .find(|target| target.as_str() == native.target().profile().identity().as_str())
        .expect("native product target must be one of the toolchain's supported targets");

    let mut units = Vec::new();
    let mut payloads = Vec::new();
    let mut unit_digests = BTreeMap::new();

    // Keep at most eight LLVM inspector processes active while retaining stable staging order.
    for batch in staging.inputs().chunks(8) {
        let inspected = batch.par_iter()
            .map(|staged| inspect_staged_unit(plan, staged, tools))
            .collect::<Vec<_>>();

        for result in inspected {
            let Some((key, unit, bytes)) = result? else {
                continue;
            };

            let digest = unit.digest();
            units.push(unit);
            payloads.push((digest.bytes(), bytes));
            unit_digests.insert(key, digest);
        }
    }

    let mut hasher = StableDigestHasher::new();

    "bray native publication v1".hash(&mut hasher);
    native.backend().identity().hash(&mut hasher);
    native.options().hash(&mut hasher);
    CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION.hash(&mut hasher);
    bundle.implementation_configuration().hash(&mut hasher);
    target.hash(&mut hasher);

    let producer = NativeContentDigest::new(hasher.finalize());

    let index = NativeArtifactIndex::try_new(target, producer, units, [])
        .expect("compiler-produced native unit set must form a valid index");

    let index_bytes = match index.encode() {
        Ok(bytes) => bytes,
        Err(NativeIndexError::SizeLimitExceeded) => {
            return Err(ProductEmissionErrorKind::NativeIndexSizeLimitExceeded);
        }
        Err(error) => panic!("compiler-produced native index must encode: {error:?}"),
    };

    let graph = compilation.symbol_graph().map_err(ProductEmissionErrorKind::Query)?;
    let mut bindings = Vec::new();

    for mappings in native.mappings() {
        let Some(&digest) = unit_digests.get(mappings.unit()) else {
            continue;
        };

        let unit = index.units().iter().find(|unit| unit.digest() == digest)
            .expect("indexed unit must be present for every native mapping");

        let NativeUnitSummary::Exact { definitions, .. } = unit.summary() else {
            continue;
        };

        for mapping in mappings.symbols() {
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
                },
                SymbolKeyData::SourceDeclaration { .. } | SymbolKeyData::Synthesized(_) => {
                    let symbol = graph.symbol_for_key(bound.declared_owner())
                        .expect("emitted source declaration must belong to the loaded symbol graph");

                    crate::compilation::export::external_symbol_key(
                        graph, compilation.package_identity(), symbol,
                    )
                    .map_err(ProductEmissionErrorKind::PackageInterface)?
                }
                _ => continue,
            };

            let Some(owner) = bundle.surface().symbol_by_external_key(&external) else {
                continue;
            };

            let Some(symbol) = definitions.iter().find_map(|definition| {
                let name = definition.symbol().identity().name()?;

                (name == mapping.name().as_str()
                    || target.object_format() == bray_target::ObjectFormat::MachO
                        && name.strip_prefix('_') == Some(mapping.name().as_str()))
                    .then(|| NonEmptySharedStr::try_new(name))
                    .flatten()
            }) else {
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

            bindings.push(InterfaceNativeBinding::new(owner, key, digest.bytes(), symbol));
        }
    }

    let artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
        interface,
        bundle,
        &index_bytes,
        &payloads,
        &bindings,
        InterfaceValidationLimits::default(),
    )
    .map_err(ProductEmissionErrorKind::PackageImplementation)?;

    artifact.native_artifact()
        .expect("compiler-produced native package must authenticate before publication");

    Ok(artifact)
}

fn inspect_staged_unit(
    plan: &EmissionPlan,
    staged: &StagedArtifact,
    tools: NativeInspectionInputs<'_>,
) -> Result<Option<(CodegenUnitKey, NativeUnit, Arc<[u8]>)>, ProductEmissionErrorKind> {
    let planned = plan.artifact(staged.artifact())
        .expect("staged native input must belong to the immutable emission plan");

    let Some(artifact) = planned.producer().backend_artifact() else {
        return Ok(None);
    };

    let kind = match artifact.kind() {
        BackendArtifactKind::RelocatableObject => NativeUnitKind::Object,
        BackendArtifactKind::BackendBitcode => NativeUnitKind::Bitcode,
        _ => return Ok(None),
    };

    let bytes = std::fs::read(staged.path()).map_err(|error| {
        ProductEmissionErrorKind::NativeInspection(NativeInspectionError::Read {
            path: staged.path().to_path_buf(),
            kind: error.kind(),
        })
    })?;

    let digest = NativeContentDigest::new(
        bray_base::sha256_reader(bytes.as_slice())
            .expect("reading an in-memory native unit cannot fail"),
    );

    let symbols = run_inspector(
        tools.symbols,
        DiagnosticLlvmToolRole::SymbolInspector,
        &["--format=posix", "--extern-only"],
        staged.path(),
    )
    .map_err(ProductEmissionErrorKind::NativeInspection)?;

    let (tool, role, arguments): (&Path, DiagnosticLlvmToolRole, &[&str]) = match kind {
        NativeUnitKind::Object => (
            tools.objects,
            DiagnosticLlvmToolRole::ObjectInspector,
            &["--sections", "--symbols", "--relocations"],
        ),
        NativeUnitKind::Bitcode => (
            tools.bitcode,
            DiagnosticLlvmToolRole::BitcodeInspector,
            &["-o", "-"],
        ),
        NativeUnitKind::OpaqueArchive => unreachable!("backend linkable artifact is a unit"),
    };

    let structure = run_inspector(tool, role, arguments, staged.path())
        .map_err(ProductEmissionErrorKind::NativeInspection)?;

    let summary = scan_native_unit_summary(kind, &symbols, &structure);

    // The published unit map must outlive this borrowed backend artifact identifier.
    let key = artifact.unit().clone();

    Ok(Some((key, NativeUnit::new(digest, kind, summary, [], []), Arc::from(bytes))))
}

fn run_inspector(
    program: &Path,
    role: DiagnosticLlvmToolRole,
    arguments: &[&str],
    path: &Path,
) -> Result<String, NativeInspectionError> {
    let output = Command::new(program)
        .args(arguments)
        .arg(path)
        .output()
        .map_err(|error| NativeInspectionError::Invoke {
            tool: role,
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;

    if !output.status.success() {
        return Err(NativeInspectionError::Failed {
            tool: role,
            path: path.to_path_buf(),
            status: output.status.code(),
        });
    }

    String::from_utf8(output.stdout).map_err(|_| NativeInspectionError::Encoding {
        tool: role,
        path: path.to_path_buf(),
    })
}

/// Exact host or tool failure while inspecting one completed native unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeInspectionError {
    /// A completed backend unit could not be read.
    Read { path: PathBuf, kind: io::ErrorKind },
    /// The selected LLVM inspection tool could not be started.
    Invoke { tool: DiagnosticLlvmToolRole, path: PathBuf, kind: io::ErrorKind },
    /// The selected LLVM inspection tool rejected a unit.
    Failed { tool: DiagnosticLlvmToolRole, path: PathBuf, status: Option<i32> },
    /// The selected LLVM inspection tool returned invalid UTF-8.
    Encoding { tool: DiagnosticLlvmToolRole, path: PathBuf },
}
