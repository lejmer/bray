use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_codegen::{BackendIdentity, CodegenTarget};
use bray_runtime_interface::{BinarySymbolName, PlatformServiceRole, RuntimeAbiVersion};
use bray_standard_library::{
    StandardLibraryArtifact, StandardLibraryArtifactKind,
    StandardLibraryOptimizationCompatibility, StandardLibraryOptimizationDependency,
    StandardLibraryOptimizationFallback, StandardLibraryOptimizationMetadata,
    StandardLibraryOptimizationProducer, StandardLibraryOptimizationProducerKind,
};
use bray_target::{NativeTarget, ObjectFormat, TargetOutputKind, TargetOutputName};

use super::command::BuildError;

pub(super) struct BuiltOptimizationArchive {
    pub(super) bytes: Vec<u8>,
    pub(super) module_count: NonZeroU32,
    pub(super) preservation_roots: Vec<BinarySymbolName>,
    pub(super) triple: String,
    pub(super) data_layout: String,
}

pub(super) struct OptimizationPublication<'publication> {
    bundle: &'publication Path,
    target_path: &'publication str,
    native: NativeTarget,
    runtime_abi: RuntimeAbiVersion,
    backend: &'publication BackendIdentity,
    target: &'publication CodegenTarget,
    triple: String,
    data_layout: String,
}

impl<'publication> OptimizationPublication<'publication> {
    pub(super) fn new(
        bundle: &'publication Path,
        target_path: &'publication str,
        native: NativeTarget,
        runtime_abi: RuntimeAbiVersion,
        backend: &'publication BackendIdentity,
        target: &'publication CodegenTarget,
        baseline: &BuiltOptimizationArchive,
    ) -> Self {
        Self {
            bundle,
            target_path,
            native,
            runtime_abi,
            backend,
            target,
            triple: baseline.triple.clone(),
            data_layout: baseline.data_layout.clone(),
        }
    }

    pub(super) fn publish_bray(
        &self,
        fallback: &StandardLibraryArtifact,
        built: BuiltOptimizationArchive,
    ) -> Result<StandardLibraryArtifact, BuildError> {
        let producer = StandardLibraryOptimizationProducer::try_new(
            StandardLibraryOptimizationProducerKind::Bray,
            self.backend.name(),
            self.backend.revision(),
            "llvm",
            self.backend.toolchain_revision(),
        )
        .map_err(manifest_error)?;

        self.publish("std", &[], &[], fallback, built, producer)
    }

    pub(super) fn publish_native(
        &self,
        partition: &str,
        services: &[PlatformServiceRole],
        dependencies: &[StandardLibraryArtifact],
        fallback: &StandardLibraryArtifact,
        built: BuiltOptimizationArchive,
    ) -> Result<StandardLibraryArtifact, BuildError> {
        let producer = StandardLibraryOptimizationProducer::try_new(
            StandardLibraryOptimizationProducerKind::PinnedNative,
            partition,
            env!("CARGO_PKG_VERSION"),
            "llvm",
            self.backend.toolchain_revision(),
        )
        .map_err(manifest_error)?;

        self.publish(
            partition,
            services,
            dependencies,
            fallback,
            built,
            producer,
        )
    }

    fn publish(
        &self,
        partition: &str,
        services: &[PlatformServiceRole],
        dependencies: &[StandardLibraryArtifact],
        fallback: &StandardLibraryArtifact,
        built: BuiltOptimizationArchive,
        producer: StandardLibraryOptimizationProducer,
    ) -> Result<StandardLibraryArtifact, BuildError> {
        let compatibility = self.compatibility(&built)?;
        let file_name = optimization_archive_name(self.native, partition)?;
        let path = format!("{}/{file_name}", self.target_path);

        super::command::write_bundle_artifact(self.bundle, &path, &built.bytes)?;

        let fallback = StandardLibraryOptimizationFallback::try_new(
            fallback.path(),
            fallback.digest(),
        )
        .map_err(manifest_error)?;

        let dependencies = dependencies
            .iter()
            .map(|dependency| {
                StandardLibraryOptimizationDependency::try_new(
                    dependency.path(),
                    dependency.digest(),
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(manifest_error)?;

        let metadata = StandardLibraryOptimizationMetadata::try_new(
            partition,
            producer,
            compatibility,
            fallback,
            built.module_count,
        )
        .map(|metadata| metadata.with_preservation_roots(built.preservation_roots))
        .map(|metadata| metadata.with_platform_services(services.iter().copied()))
        .map(|metadata| metadata.with_dependencies(dependencies))
        .map_err(manifest_error)?;

        StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::OptimizationArchive,
            path,
            &built.bytes,
        )
        .map(|artifact| artifact.with_optimization(metadata))
        .map_err(manifest_error)
    }

    fn compatibility(
        &self,
        optimization: &BuiltOptimizationArchive,
    ) -> Result<StandardLibraryOptimizationCompatibility, BuildError> {
        if optimization.triple != self.triple || optimization.data_layout != self.data_layout {
            return Err(BuildError::NativeArchive(format!(
                "optimization target {} with data layout {} differs from Bray target {} with data layout {}",
                optimization.triple,
                optimization.data_layout,
                self.triple,
                self.data_layout,
            )));
        }

        let compatibility = StandardLibraryOptimizationCompatibility::try_new(
            optimization.triple.clone(),
            optimization.data_layout.clone(),
            self.target.relocation_model(),
            self.target.code_model(),
            self.runtime_abi,
        )
        .map_err(manifest_error)?;

        if compatibility.triple() != self.target.triple() {
            return Err(BuildError::NativeArchive(format!(
                "optimization target {} differs from selected target {}",
                compatibility.triple(),
                self.target.triple(),
            )));
        }

        Ok(compatibility)
    }
}

fn optimization_archive_name(
    target: NativeTarget,
    partition: &str,
) -> Result<String, BuildError> {
    TargetOutputName::for_native(target.object_format(), TargetOutputKind::StaticLibrary)
        .file_name(&format!("{partition}_optimization"))
        .ok_or(BuildError::InvalidIdentity)
}

fn manifest_error(error: bray_standard_library::StandardLibraryManifestError) -> BuildError {
    BuildError::Manifest(format!("{error:?}"))
}

pub(super) fn from_bray_modules(
    root: &Path,
    work: &Path,
    modules: impl IntoIterator<Item = Vec<u8>>,
    preservation_roots: impl IntoIterator<Item = BinarySymbolName>,
) -> Result<BuiltOptimizationArchive, BuildError> {
    let preservation_roots = preservation_roots.into_iter().collect::<BTreeSet<_>>();

    build_archive(root, work, modules, preservation_roots, None)
}

pub(super) fn from_native_archive(
    root: &Path,
    work: &Path,
    archive: &Path,
    target: NativeTarget,
) -> Result<Option<BuiltOptimizationArchive>, BuildError> {
    let modules = bitcode_members(root, archive)?;

    if modules.is_empty() {
        return Ok(None);
    }

    let built = build_archive(root, work, modules, BTreeSet::new(), Some(target.as_str()))?;

    let preservation_roots = native_exports(root, &built.bytes, work, target.object_format())?;

    Ok(Some(BuiltOptimizationArchive {
        preservation_roots,
        ..built
    }))
}

fn build_archive(
    root: &Path,
    work: &Path,
    modules: impl IntoIterator<Item = Vec<u8>>,
    preservation_roots: BTreeSet<BinarySymbolName>,
    target_triple: Option<&str>,
) -> Result<BuiltOptimizationArchive, BuildError> {
    let staging = tempfile::Builder::new()
        .prefix("bray-thin-lto-")
        .tempdir_in(work)
        .map_err(BuildError::TemporaryDirectory)?;

    let mut modules: Vec<_> = modules.into_iter().collect();
    modules.sort_unstable();

    let module_count = u32::try_from(modules.len())
        .ok()
        .and_then(NonZeroU32::new)
        .ok_or_else(|| BuildError::NativeArchive("optimization module count is invalid".to_owned()))?;

    let mut summarized = Vec::with_capacity(modules.len());
    let mut contract = None;

    for (index, bytes) in modules.into_iter().enumerate() {
        if !is_bitcode(&bytes) {
            return Err(BuildError::NativeArchive(format!(
                "optimization module {index} is not raw LLVM bitcode"
            )));
        }

        let input = staging.path().join(format!("input_{index:04}.bc"));
        let output = staging.path().join(format!("module_{index:04}.bc"));

        fs::write(&input, bytes).map_err(|error| BuildError::write(&input, error))?;
        summarize_module(root, &input, &output, target_triple)?;

        let module_contract = inspect_module_contract(root, &output)?;

        if contract
            .as_ref()
            .is_some_and(|expected| expected != &module_contract)
        {
            return Err(BuildError::NativeArchive(
                "optimization modules have incompatible target contracts".to_owned(),
            ));
        }

        contract.get_or_insert(module_contract);
        summarized.push(output);
    }

    let archive = staging.path().join("optimization.a");
    create_archive(root, &archive, &summarized)?;

    let bytes = fs::read(&archive).map_err(|error| BuildError::read(&archive, error))?;

    let (triple, data_layout) = contract.ok_or_else(|| {
        BuildError::NativeArchive("optimization module target is unavailable".to_owned())
    })?;

    Ok(BuiltOptimizationArchive {
        bytes,
        module_count,
        preservation_roots: preservation_roots.into_iter().collect(),
        triple,
        data_layout,
    })
}

fn summarize_module(
    root: &Path,
    input: &Path,
    output: &Path,
    target_triple: Option<&str>,
) -> Result<(), BuildError> {
    let canonical = canonicalize_module(root, input)?;
    let mut summarize = Command::new(bray_llvm_toolchain::tool_path(root, "opt"));

    summarize
        .arg("-module-summary")
        .arg(canonical)
        .arg("-o")
        .arg(output);

    if let Some(target_triple) = target_triple {
        summarize.arg(format!("-mtriple={target_triple}"));
    }

    require_success(summarize, "LLVM could not create a module summary")?;

    let mut verify = Command::new(bray_llvm_toolchain::tool_path(root, "opt"));

    verify
        .args(["-passes=verify", "-disable-output"])
        .arg(output);

    require_success(verify, "LLVM rejected an optimization module")
}

fn canonicalize_module(root: &Path, input: &Path) -> Result<PathBuf, BuildError> {
    let llvm_ir = input.with_extension("ll");
    let canonical = input.with_extension("canonical.bc");
    let mut disassemble = Command::new(bray_llvm_toolchain::tool_path(root, "llvm-dis"));

    disassemble.arg(input).arg("-o").arg(&llvm_ir);

    require_success(
        disassemble,
        "LLVM could not canonicalize an optimization module",
    )?;

    let mut assemble = Command::new(bray_llvm_toolchain::tool_path(root, "llvm-as"));

    assemble.arg(&llvm_ir).arg("-o").arg(&canonical);

    require_success(
        assemble,
        "LLVM could not assemble a canonical optimization module",
    )?;

    Ok(canonical)
}

fn inspect_module_contract(root: &Path, module: &Path) -> Result<(String, String), BuildError> {
    let destination = module.with_extension("ll");
    let mut disassemble = Command::new(bray_llvm_toolchain::tool_path(root, "llvm-dis"));

    disassemble.arg(module).arg("-o").arg(&destination);
    require_success(disassemble, "LLVM could not inspect an optimization module")?;

    let file = fs::File::open(&destination)
        .map_err(|error| BuildError::read(&destination, error))?;

    let mut triple = None;
    let mut data_layout = None;

    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| BuildError::read(&destination, error))?;

        triple = triple.or_else(|| quoted_assignment(&line, "target triple"));
        data_layout = data_layout.or_else(|| quoted_assignment(&line, "target datalayout"));

        if triple.is_some() && data_layout.is_some() {
            break;
        }
    }

    triple
        .zip(data_layout)
        .ok_or_else(|| BuildError::NativeArchive("optimization module target is incomplete".to_owned()))
}

fn quoted_assignment(line: &str, name: &str) -> Option<String> {
    let value = line.strip_prefix(name)?.trim_start().strip_prefix('=')?.trim();
    let value = value.strip_prefix('"')?.strip_suffix('"')?;

    (!value.is_empty()).then(|| value.to_owned())
}

fn create_archive(root: &Path, archive: &Path, modules: &[PathBuf]) -> Result<(), BuildError> {
    let mut command = Command::new(bray_llvm_toolchain::tool_path(root, "llvm-ar"));

    command.arg("rcsD").arg(archive).args(modules);
    require_success(command, "LLVM could not create an optimization archive")?;

    let mut inspect = Command::new(bray_llvm_toolchain::tool_path(root, "llvm-ar"));

    let output = inspect
        .arg("t")
        .arg(archive)
        .output()
        .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

    if !output.status.success()
        || output.stdout.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()).count()
            != modules.len()
    {
        return Err(BuildError::NativeArchive(
            "optimization archive inventory is invalid".to_owned(),
        ));
    }

    Ok(())
}

fn bitcode_members(root: &Path, archive: &Path) -> Result<Vec<Vec<u8>>, BuildError> {
    let archiver = bray_llvm_toolchain::tool_path(root, "llvm-ar");

    let output = Command::new(&archiver)
        .arg("t")
        .arg(archive)
        .output()
        .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

    if !output.status.success() {
        return Err(BuildError::NativeArchive(
            "LLVM could not inspect a native provider archive".to_owned(),
        ));
    }

    let members = String::from_utf8(output.stdout)
        .map_err(|_| BuildError::NativeArchive("native archive inventory is not UTF-8".to_owned()))?;

    let mut bitcode = Vec::new();

    for member in members.lines().filter(|member| !member.is_empty()) {
        if member.contains(".rcgu.") {
            continue;
        }

        let output = Command::new(&archiver)
            .arg("p")
            .arg(archive)
            .arg(member)
            .output()
            .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

        if !output.status.success() {
            return Err(BuildError::NativeArchive(
                "LLVM could not read a native provider archive member".to_owned(),
            ));
        }

        if is_bitcode(&output.stdout) {
            bitcode.push(output.stdout);
        }
    }

    Ok(bitcode)
}

fn is_bitcode(bytes: &[u8]) -> bool {
    bytes.starts_with(b"BC\xc0\xde")
}

fn native_exports(
    root: &Path,
    archive_bytes: &[u8],
    work: &Path,
    object_format: ObjectFormat,
) -> Result<Vec<BinarySymbolName>, BuildError> {
    let staging = tempfile::Builder::new()
        .prefix("bray-native-symbols-")
        .tempdir_in(work)
        .map_err(BuildError::TemporaryDirectory)?;

    let archive = staging.path().join("optimization.a");
    fs::write(&archive, archive_bytes).map_err(|error| BuildError::write(&archive, error))?;

    let output = Command::new(bray_llvm_toolchain::tool_path(root, "llvm-nm"))
        .args(["--defined-only", "--extern-only"])
        .arg(&archive)
        .output()
        .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

    if !output.status.success() {
        return Err(BuildError::NativeArchive(
            "LLVM could not inspect native optimization symbols".to_owned(),
        ));
    }

    let symbols = String::from_utf8(output.stdout)
        .map_err(|_| BuildError::NativeArchive("native symbol inventory is not UTF-8".to_owned()))?;

    Ok(symbols
        .lines()
        .filter_map(|line| line.split_whitespace().next_back())
        .map(|name| {
            if object_format == ObjectFormat::MachO {
                name.strip_prefix('_').unwrap_or(name)
            } else {
                name
            }
        })
        .filter(|name| name.starts_with("bray_"))
        .filter_map(BinarySymbolName::try_new)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn require_success(mut command: Command, failure: &str) -> Result<(), BuildError> {
    let output = command
        .output()
        .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

    if output.status.success() {
        return Ok(());
    }

    let detail = String::from_utf8_lossy(&output.stderr);

    Err(BuildError::NativeArchive(format!(
        "{failure}: {}",
        detail.trim()
    )))
}

#[cfg(test)]
mod tests {
    use super::{is_bitcode, quoted_assignment};

    #[test]
    fn bitcode_detection_accepts_raw_llvm_modules() {
        assert!(is_bitcode(b"BC\xc0\xdepayload"));
        assert!(!is_bitcode(b"\xde\xc0\x17\x0bpayload"));
        assert!(!is_bitcode(b"native object"));
    }

    #[test]
    fn module_contract_assignments_require_quoted_values() {
        assert_eq!(
            quoted_assignment("target triple = \"x86_64-pc-windows-msvc\"", "target triple"),
            Some("x86_64-pc-windows-msvc".to_owned())
        );

        assert_eq!(quoted_assignment("target triple = empty", "target triple"), None);
    }
}
