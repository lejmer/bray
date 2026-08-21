use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_base::NonEmptySharedStr;
use bray_compilation::{
    Compilation, CompilationOptions, CompilationRequest, DependencyInterfaceInput, SelectedTarget,
    WorkerBudget,
};
use bray_diagnostics::DiagnosticKind;
use bray_diagnostics::DiagnosticLlvmToolRole;
use bray_linker::{LinkSearchPath, LinkSearchPathKind};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceValidationPolicy, encode_package_interface,
};
use bray_project::load_standard_library_project_graph;
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_standard_library::StandardLibraryRoot;
use bray_symbols::{
    NativeLinkKind, NativeLinkRequirement, PackageIdentity, ProductIdentity, ProductKind,
};
use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};
use bray_tooling::{llvm_tool_path, load_llvm_compilation};
use sha2::{Digest as _, Sha256};

use super::command::BuildError;

const FIXTURE_LIBRARY: &str = "bray_foreign_fixture";
const FIXTURE_PRODUCT: &str = "interoperability";
const MAXIMUM_TARGET_AUDIT_CONCURRENCY: usize = 2;

pub(super) fn audit(
    root: &Path,
    output: &Path,
    toolchain: &Path,
    runtime: &Path,
    target: NativeTarget,
) -> Result<(), BuildError> {
    audit_target_modules(root)?;

    let output = output.join(FIXTURE_PRODUCT);

    fs::create_dir(&output).map_err(|error| BuildError::write(&output, error))?;

    let fixture = build_native_fixture(root, &output, target)?;

    let executable = emit_fixture(root, &output, toolchain, runtime, target, &fixture)?;
    let first_cache = optimization_cache_snapshot(&output, target)?;

    if first_cache.is_empty() {
        return Err(BuildError::conformance(
            "foreign interoperability",
            "native optimization produced no reusable cache partitions",
        ));
    }

    let repeated = emit_fixture(root, &output, toolchain, runtime, target, &fixture)?;
    let second_cache = optimization_cache_snapshot(&output, target)?;

    if repeated != executable || second_cache != first_cache {
        return Err(BuildError::conformance(
            "foreign interoperability",
            "an unchanged native product did not reuse the same optimization partitions",
        ));
    }

    let mut command = Command::new(&executable);

    command.current_dir(&output);

    crate::command::require_success(command, "executing foreign interoperability fixture")
        .map(|_| ())
        .map_err(|error| BuildError::conformance("foreign interoperability", error))
}

fn optimization_cache_snapshot(
    output: &Path,
    target: NativeTarget,
) -> Result<BTreeMap<PathBuf, [u8; 32]>, BuildError> {
    let state_root = output.parent().ok_or_else(|| {
        BuildError::conformance(
            "foreign interoperability",
            "native output has no compiler state root",
        )
    })?;

    let root = bray_tooling::thin_lto_cache_root(state_root, target);
    let mut pending = vec![root.clone()];
    let mut snapshot = BTreeMap::new();

    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)
            .map_err(|error| BuildError::read(&directory, error))?
            .map(|entry| entry.map_err(|error| BuildError::read(&directory, error)))
            .collect::<Result<Vec<_>, _>>()?;

        entries.sort_by_key(fs::DirEntry::file_name);

        for entry in entries {
            let path = entry.path();

            let file_type = entry
                .file_type()
                .map_err(|error| BuildError::read(&path, error))?;

            if file_type.is_dir() {
                pending.push(path);
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let relative = path
                .strip_prefix(&root)
                .map_err(|_| {
                    BuildError::conformance(
                        "foreign interoperability",
                        "optimization cache entry escaped its root",
                    )
                })?
                .to_path_buf();

            let bytes = fs::read(&path).map_err(|error| BuildError::read(&path, error))?;
            snapshot.insert(relative, Sha256::digest(bytes).into());
        }
    }

    Ok(snapshot)
}

fn audit_target_modules(root: &Path) -> Result<(), BuildError> {
    let standard_library = root.join("standard-library");

    let graph = load_standard_library_project_graph(&standard_library)
        .map_err(|error| BuildError::Project(format!("{error:?}")))?;

    let product = super::command::standard_library_product(&graph)?;
    let version = super::command::standard_library_version(&graph)?;

    let source_paths = product
        .sources()
        .iter()
        .map(|source| source.beneath(&standard_library))
        .collect::<Vec<_>>();

    run_target_audits(|target| audit_target_module(product, version, &source_paths, target))
}

fn run_target_audits(
    audit: impl Fn(NativeTarget) -> Result<(), BuildError> + Sync,
) -> Result<(), BuildError> {
    for targets in NativeTarget::ALL.chunks(MAXIMUM_TARGET_AUDIT_CONCURRENCY) {
        let results = std::thread::scope(|scope| {
            let audit = &audit;

            targets
                .iter()
                .copied()
                .map(|target| scope.spawn(move || audit(target)))
                .collect::<Vec<_>>()
                .into_iter()
                .map(std::thread::ScopedJoinHandle::join)
                .collect::<Vec<_>>()
        });

        for (target, result) in targets.iter().copied().zip(results) {
            match result {
                Ok(result) => result?,
                Err(_) => {
                    return Err(BuildError::conformance(
                        "foreign interoperability target modules",
                        format!("{target:?} target audit worker panicked"),
                    ));
                }
            }
        }
    }

    Ok(())
}

fn audit_target_module(
    product: &bray_project::ProjectProduct,
    version: &bray_symbols::PackageVersion,
    source_paths: &[PathBuf],
    target: NativeTarget,
) -> Result<(), BuildError> {
    let selected = SelectedTarget::for_native(target);

    let request = super::command::standard_library_source_request(
        product,
        version,
        source_paths,
        &selected,
        WorkerBudget::serial(),
    )?;

    let standard_library = Compilation::load(request).map_err(|error| {
        BuildError::conformance(
            "foreign interoperability target modules",
            format!("could not load {target:?} standard library: {error:?}"),
        )
    })?;

    let bundle = standard_library
        .package_interface_export_bundle()
        .ok_or_else(|| {
            BuildError::conformance(
                "foreign interoperability target modules",
                format!("{target:?} standard-library interface export is unavailable"),
            )
        })?
        .as_ref()
        .map_err(|error| {
            BuildError::conformance(
                "foreign interoperability target modules",
                format!("could not export {target:?} standard-library interface: {error:?}"),
            )
        })?;

    let artifact = encode_package_interface(bundle).map_err(|error| {
        BuildError::conformance(
            "foreign interoperability target modules",
            format!("could not encode {target:?} standard-library interface: {error:?}"),
        )
    })?;

    let dependency = DependencyInterfaceInput::new(
        artifact.identity().package().clone(),
        artifact.identity().product().clone(),
        format!("{}-std.brayi", target.as_str()),
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    );

    let package = PackageIdentity::try_new("bray.interoperability.audit")
        .ok_or(BuildError::InvalidIdentity)?;

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "interoperability-target-audit.bray",
        SourceVersion::new(0),
        target_module_audit(target),
    );

    let options = CompilationOptions::new(WorkerBudget::serial(), ProductKind::Library, selected);

    let request = CompilationRequest::with_options(package, vec![source], options)
        .with_dependency_interfaces([dependency]);

    let compilation = Compilation::load(request).map_err(|error| {
        BuildError::conformance(
            "foreign interoperability target modules",
            format!("could not load {target:?} target audit: {error:?}"),
        )
    })?;

    let diagnostics = compilation.check_diagnostics();

    let unresolved = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind() == DiagnosticKind::BindingUnresolvedName)
        .count();

    if diagnostics.len() != 2 || unresolved != 2 {
        return Err(BuildError::conformance(
            "foreign interoperability target modules",
            format!("{target:?} did not accept only its selected OS module: {diagnostics:?}"),
        ));
    }

    Ok(())
}

fn build_native_fixture(
    root: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<NativeFixture, BuildError> {
    let source = root.join("xtask/fixtures/foreign-interoperability.c");

    let object = output.join(if cfg!(windows) {
        "fixture.obj"
    } else {
        "fixture.o"
    });

    let archive = output.join(output_name(
        target,
        TargetOutputKind::StaticLibrary,
        FIXTURE_LIBRARY,
    )?);

    let shared = output.join(output_name(
        target,
        TargetOutputKind::SharedLibrary,
        FIXTURE_LIBRARY,
    )?);

    let clang = native_tool(DiagnosticLlvmToolRole::CompilerDriver)?;
    let mut compile = Command::new(&clang);

    compile
        .args(["-std=c11", "-O2", "-c"])
        .arg(&source)
        .arg("-o")
        .arg(&object);

    if !cfg!(windows) {
        compile.args(["-fPIC", "-pthread"]);
    }

    crate::command::require_success(compile, "compiling foreign interoperability fixture")
        .map_err(|error| BuildError::conformance("foreign interoperability", error))?;

    let mut shared_command = Command::new(clang);

    shared_command
        .arg("-shared")
        .arg(&object)
        .arg("-o")
        .arg(&shared);

    if !cfg!(windows) {
        shared_command.arg("-pthread");
    }

    crate::command::require_success(shared_command, "linking foreign interoperability fixture")
        .map_err(|error| BuildError::conformance("foreign interoperability", error))?;

    if archive.exists() {
        fs::remove_file(&archive).map_err(|error| BuildError::write(&archive, error))?;
    }

    let mut archive_command = Command::new(native_tool(DiagnosticLlvmToolRole::Archiver)?);

    archive_command.args(["rcs"]).arg(&archive).arg(&object);

    crate::command::require_success(
        archive_command,
        "archiving foreign interoperability fixture",
    )
    .map_err(|error| BuildError::conformance("foreign interoperability", error))?;

    Ok(NativeFixture { shared })
}

fn emit_fixture(
    root: &Path,
    output: &Path,
    toolchain: &Path,
    runtime: &Path,
    target: NativeTarget,
    fixture: &NativeFixture,
) -> Result<PathBuf, BuildError> {
    let source_path = root.join("xtask/fixtures/foreign-interoperability.bray");
    let target_handle_audit = target_handle_audit(target);

    let source = fs::read_to_string(&source_path)
        .map_err(|error| BuildError::read(&source_path, error))?
        .replace("__SHARED_LIBRARY_PATH__", &bray_path(&fixture.shared))
        .replace("__TARGET_HANDLE_AUDIT__", &target_handle_audit);

    let package =
        PackageIdentity::try_new("bray.interoperability").ok_or(BuildError::InvalidIdentity)?;

    let product = ProductIdentity::try_new(package.clone(), FIXTURE_PRODUCT)
        .ok_or(BuildError::InvalidIdentity)?;

    let selected = SelectedTarget::for_native(target);
    let library = NonEmptySharedStr::try_new(FIXTURE_LIBRARY).ok_or(BuildError::InvalidIdentity)?;
    let native_link = NativeLinkRequirement::new(library, NativeLinkKind::Static);

    let options = CompilationOptions::new(
        WorkerBudget::default(),
        ProductKind::Executable,
        selected.clone(),
    )
    .with_native_link_inputs([native_link]);

    let standard_library =
        StandardLibraryRoot::try_new(toolchain.join("lib").join("bray").join("standard-library"))
            .ok_or_else(|| {
            BuildError::conformance("foreign interoperability", "invalid standard-library root")
        })?;

    let source = SourceInput::virtual_text(
        SourceIdentity::new(1),
        "foreign-interoperability.bray",
        1,
        source,
    );

    let request = CompilationRequest::with_options(package, vec![source], options)
        .with_standard_library_root(standard_library);

    let compilation = load_llvm_compilation(request).map_err(|error| {
        BuildError::conformance(
            "foreign interoperability",
            format!("LLVM compiler backend is unavailable: {error}"),
        )
    })?;

    let product_kind = compilation
        .product_semantics()
        .map_err(|error| compilation_error("resolving product semantics", error))?
        .value()
        .kind();

    if product_kind != ProductKind::Executable {
        return Err(BuildError::conformance(
            "foreign interoperability",
            format!("unexpected product kind: {product_kind:?}"),
        ));
    }

    let search_path =
        LinkSearchPath::try_new(LinkSearchPathKind::Library, output).map_err(|error| {
            BuildError::conformance(
                "foreign interoperability",
                format!("invalid library search path: {error:?}"),
            )
        })?;

    crate::native_product::emit_executable(
        &compilation,
        product.clone(),
        target,
        runtime,
        output,
        [search_path],
        None,
    )
    .map_err(|error| BuildError::conformance("foreign interoperability", error))?;

    super::artifact::resolve_executable(output, &product, "foreign interoperability")
}

fn compilation_error(operation: &str, error: impl std::fmt::Debug) -> BuildError {
    BuildError::conformance(
        "foreign interoperability",
        format!("{operation} failed: {error:?}"),
    )
}

fn output_name(
    target: NativeTarget,
    kind: TargetOutputKind,
    stem: &str,
) -> Result<PathBuf, BuildError> {
    TargetOutputName::for_native(target.object_format(), kind)
        .file_name(stem)
        .map(PathBuf::from)
        .ok_or(BuildError::InvalidIdentity)
}

fn bray_path(path: &Path) -> String {
    crate::path::slash_separated(path)
}

fn target_module_audit(target: NativeTarget) -> String {
    format!(
        r#"module bray.interoperability.audit;

using std.os.windows;
using std.os.linux;
using std.os.darwin;

{}"#,
        target_handle_assertion(target)
    )
}

fn target_handle_audit(target: NativeTarget) -> String {
    format!(
        "using {};\n\n{}",
        target_os_module(target),
        target_handle_assertion(target)
    )
}

const fn target_os_module(target: NativeTarget) -> &'static str {
    match target {
        NativeTarget::X86_64WindowsMsvc | NativeTarget::Aarch64WindowsMsvc => "std.os.windows",
        NativeTarget::X86_64LinuxGnu | NativeTarget::Aarch64LinuxGnu => "std.os.linux",
        NativeTarget::X86_64MacOs | NativeTarget::Aarch64MacOs => "std.os.darwin",
    }
}

const fn target_handle_assertion(target: NativeTarget) -> &'static str {
    match target {
        NativeTarget::X86_64WindowsMsvc | NativeTarget::Aarch64WindowsMsvc => {
            r#"trusted func audit_target_handle()
{
    let handle: std.os.windows.Handle = std.os.windows.handle(42);

    assert(handle.value() == 42);
}"#
        }
        NativeTarget::X86_64LinuxGnu | NativeTarget::Aarch64LinuxGnu => {
            r#"trusted func audit_target_handle()
{
    let descriptor: std.os.linux.FileDescriptor = std.os.linux.file_descriptor(42);

    assert(descriptor.value() == 42);
}"#
        }
        NativeTarget::X86_64MacOs | NativeTarget::Aarch64MacOs => {
            r#"trusted func audit_target_handle()
{
    let descriptor: std.os.darwin.FileDescriptor = std.os.darwin.file_descriptor(42);

    assert(descriptor.value() == 42);
}"#
        }
    }
}

fn native_tool(tool: DiagnosticLlvmToolRole) -> Result<PathBuf, BuildError> {
    llvm_tool_path(tool).map_err(|error| {
        BuildError::conformance(
            "foreign interoperability",
            format!(
                "the provisioned {} tool is unavailable: {error}",
                tool.executable_name()
            ),
        )
    })
}

struct NativeFixture {
    shared: PathBuf,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    use bray_target::NativeTarget;

    #[test]
    fn optimization_cache_snapshots_include_recursive_partition_content() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test directory must be available: {error}"));

        let output = directory.path().join("product");
        let target = NativeTarget::X86_64WindowsMsvc;
        let cache = bray_tooling::thin_lto_cache_root(directory.path(), target);
        let nested = cache.join("partitions");

        fs::create_dir_all(&nested)
            .unwrap_or_else(|error| panic!("test cache must be writable: {error}"));

        fs::write(nested.join("first"), b"first")
            .unwrap_or_else(|error| panic!("test cache entry must be writable: {error}"));

        let first = super::optimization_cache_snapshot(&output, target)
            .unwrap_or_else(|error| panic!("test cache must be readable: {error}"));

        fs::write(nested.join("first"), b"second")
            .unwrap_or_else(|error| panic!("test cache entry must be replaceable: {error}"));

        let second = super::optimization_cache_snapshot(&output, target)
            .unwrap_or_else(|error| panic!("test cache must be readable: {error}"));

        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_ne!(first, second);
    }

    #[test]
    fn target_audits_run_concurrently_and_report_failures_in_target_order() {
        let concurrency = super::MAXIMUM_TARGET_AUDIT_CONCURRENCY;
        let barrier = Arc::new(Barrier::new(concurrency));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let first = NativeTarget::ALL[0];
        let second = NativeTarget::ALL[1];

        let result = super::run_target_audits(|target| {
            let active_count = active.fetch_add(1, Ordering::SeqCst).saturating_add(1);

            maximum.fetch_max(active_count, Ordering::SeqCst);
            barrier.wait();
            active.fetch_sub(1, Ordering::SeqCst);

            if target == first || target == second {
                Err(super::BuildError::conformance(
                    "target audit test",
                    target.as_str(),
                ))
            } else {
                Ok(())
            }
        });

        let Err(error) = result else {
            panic!("two target audits should fail");
        };

        assert_eq!(maximum.load(Ordering::SeqCst), concurrency);
        assert!(error.to_string().contains(first.as_str()));
    }
}
