use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_base::NonEmptySharedStr;
use bray_compilation::{
    Compilation, CompilationOptions, CompilationProfileConfiguration, CompilationProfileMode,
    CompilationProfileReport, CompilationRequest, DependencyInterfaceInput, SelectedTarget,
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

use super::command::BuildError;

const FIXTURE_LIBRARY: &str = "bray_foreign_fixture";
const FIXTURE_PRODUCT: &str = "interoperability";
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

    let first = emit_fixture(root, &output, toolchain, runtime, target, &fixture)?;
    let first_hits = profile_metric(&first.profile, "compiler.optimization.cache_hits");
    let first_misses = profile_metric(&first.profile, "compiler.optimization.cache_misses");
    let first_writes = profile_metric(&first.profile, "compiler.optimization.cache_writes");

    let first_peak_memory =
        profile_metric(&first.profile, "compiler.optimization.peak_resident_bytes");

    let first_active_workers =
        profile_metric(&first.profile, "compiler.optimization.active_workers");

    if first_hits != 0
        || first_misses == 0
        || first_writes == 0
        || first_writes > first_misses
        || first_peak_memory == 0
        || first_active_workers == 0
    {
        return Err(BuildError::conformance(
            "foreign interoperability",
            "the initial native link did not report optimization work, cache publication, and resource use",
        ));
    }

    let repeated = emit_fixture(root, &output, toolchain, runtime, target, &fixture)?;
    let repeated_hits = profile_metric(&repeated.profile, "compiler.optimization.cache_hits");
    let repeated_misses = profile_metric(&repeated.profile, "compiler.optimization.cache_misses");
    let repeated_writes = profile_metric(&repeated.profile, "compiler.optimization.cache_writes");

    let repeated_reuse =
        profile_metric(&repeated.profile, "compiler.optimization.reused_partitions");

    if repeated.executable != first.executable
        || repeated_hits == 0
        || repeated_reuse != repeated_hits
        || repeated_misses != 0
        || repeated_writes != 0
    {
        return Err(BuildError::conformance(
            "foreign interoperability",
            format!(
                "the repeated native link did not reuse every optimized partition: executable_match={}, cache_hits={repeated_hits}, reused_partitions={repeated_reuse}, cache_misses={repeated_misses}, cache_writes={repeated_writes}",
                repeated.executable == first.executable,
            ),
        ));
    }

    let mut command = Command::new(&first.executable);

    command.current_dir(&output);

    crate::command::require_success(command, "executing foreign interoperability fixture")
        .map(|_| ())
        .map_err(|error| BuildError::conformance("foreign interoperability", error))
}

fn profile_metric(report: &CompilationProfileReport, name: &str) -> u64 {
    report
        .metrics
        .iter()
        .find_map(|metric| {
            report
                .metric_descriptor(metric.id)
                .filter(|descriptor| descriptor.name == name)
                .map(|_| metric.value)
        })
        .unwrap_or(0)
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

    super::target::try_for_each_native_target_compilation(
        product,
        version,
        &source_paths,
        "foreign interoperability target modules",
        audit_target_module,
    )
}

fn audit_target_module(
    target: NativeTarget,
    selected: &SelectedTarget,
    standard_library: &Compilation,
) -> Result<(), BuildError> {
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
            BuildError::compilation_failed(
                selected.profile().identity().clone(),
                format!("{error:?}"),
                standard_library.check_diagnostics(),
                standard_library.sources(),
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

    let options = CompilationOptions::new(
        WorkerBudget::serial(),
        ProductKind::Library,
        selected.clone(),
    );

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

    let shared_object = output.join(if cfg!(windows) {
        "fixture-shared.obj"
    } else {
        "fixture-shared.o"
    });

    let static_object = output.join(if cfg!(windows) {
        "fixture-static.obj"
    } else {
        "fixture-static.o"
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
    compile_native_fixture_source(&clang, &source, &shared_object, true)?;

    let mut shared_command = Command::new(&clang);

    shared_command
        .arg("-shared")
        .arg(&shared_object)
        .arg("-o")
        .arg(&shared);

    if !cfg!(windows) {
        shared_command.arg("-pthread");
    }

    crate::command::require_success(shared_command, "linking foreign interoperability fixture")
        .map_err(|error| BuildError::conformance("foreign interoperability", error))?;

    compile_native_fixture_source(&clang, &source, &static_object, false)?;

    if archive.exists() {
        fs::remove_file(&archive).map_err(|error| BuildError::write(&archive, error))?;
    }

    let mut archive_command = Command::new(native_tool(DiagnosticLlvmToolRole::Archiver)?);

    archive_command
        .args(["rcs"])
        .arg(&archive)
        .arg(&static_object);

    crate::command::require_success(
        archive_command,
        "archiving foreign interoperability fixture",
    )
    .map_err(|error| BuildError::conformance("foreign interoperability", error))?;

    Ok(NativeFixture { shared })
}

fn compile_native_fixture_source(
    clang: &Path,
    source: &Path,
    object: &Path,
    shared: bool,
) -> Result<(), BuildError> {
    let mut command = Command::new(clang);

    command.args(["-std=c11", "-O2", "-c"]);

    if shared {
        command.arg("-DBRAY_SHARED_FIXTURE=1");
    }

    if !cfg!(windows) {
        command.args(["-fPIC", "-pthread"]);
    }

    command.arg(source).arg("-o").arg(object);

    crate::command::require_success(command, "compiling foreign interoperability fixture")
        .map(|_| ())
        .map_err(|error| BuildError::conformance("foreign interoperability", error))
}

fn emit_fixture(
    root: &Path,
    output: &Path,
    toolchain: &Path,
    runtime: &Path,
    target: NativeTarget,
    fixture: &NativeFixture,
) -> Result<EmittedFixture, BuildError> {
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
        .with_standard_library_root(standard_library)
        .with_profile(CompilationProfileConfiguration::new(
            CompilationProfileMode::Summary,
        ));

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

    let native_output = output.join("native");

    fs::create_dir_all(&native_output).map_err(|error| BuildError::write(&native_output, error))?;

    crate::native_product::emit_executable(
        &compilation,
        product.clone(),
        target,
        runtime,
        &native_output,
        [search_path],
        None,
    )
    .map_err(|error| BuildError::conformance("foreign interoperability", error))?;

    let executable =
        super::artifact::resolve_executable(&native_output, &product, "foreign interoperability")?;

    let profile = compilation.profile_report().ok_or_else(|| {
        BuildError::conformance(
            "foreign interoperability",
            "the native compiler did not publish its requested profile",
        )
    })?;

    Ok(EmittedFixture {
        executable,
        profile,
    })
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

struct EmittedFixture {
    executable: PathBuf,
    profile: CompilationProfileReport,
}

#[cfg(test)]
mod tests {
    use bray_compilation::{
        CompilationProfileAggregation, CompilationProfileCategory, CompilationProfileContext,
        CompilationProfileDescriptorCatalog, CompilationProfileMetric,
        CompilationProfileMetricDescriptor, CompilationProfileMode, CompilationProfileReport,
        CompilationProfileSchedulerStatistics, CompilationProfileSubjectKind,
        CompilationProfileTimeBreakdown, CompilationProfileUnit,
    };

    #[test]
    fn profile_metric_resolves_values_through_the_descriptor_catalog() {
        let report = CompilationProfileReport {
            schema_revision: 1,
            mode: CompilationProfileMode::Summary,
            context: CompilationProfileContext {
                package: "package".to_owned(),
                product: "product".to_owned(),
                target: "target".to_owned(),
            },
            trace_event_limit: None,
            elapsed_nanoseconds: 0,
            time: CompilationProfileTimeBreakdown {
                active_work_nanoseconds: 0,
                same_thread_self_nanoseconds: 0,
                scheduler_queue_nanoseconds: 0,
                dependency_wait_nanoseconds: 0,
                external_work_nanoseconds: 0,
            },
            scheduler: CompilationProfileSchedulerStatistics::default(),
            descriptors: CompilationProfileDescriptorCatalog {
                operations: Vec::new(),
                queries: Vec::new(),
                metrics: vec![CompilationProfileMetricDescriptor {
                    id: 1,
                    name: "compiler.optimization.cache_hits".to_owned(),
                    category: CompilationProfileCategory::Measurement,
                    unit: CompilationProfileUnit::Count,
                    aggregation: CompilationProfileAggregation::Sum,
                    allowed_subjects: vec![CompilationProfileSubjectKind::Product],
                }],
            },
            operations: Vec::new(),
            queries: Vec::new(),
            metrics: vec![CompilationProfileMetric { id: 1, value: 3 }],
            runtime_artifacts: Vec::new(),
            runtime_roles: Vec::new(),
            native_callback_entries: Vec::new(),
            events: Vec::new(),
            dropped_events: 0,
        };

        assert_eq!(
            super::profile_metric(&report, "compiler.optimization.cache_hits"),
            3
        );

        assert_eq!(
            super::profile_metric(&report, "compiler.optimization.cache_misses"),
            0
        );
    }
}
