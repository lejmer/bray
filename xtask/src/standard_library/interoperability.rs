use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_base::NonEmptySharedStr;
use bray_compilation::{
    CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_linker::{LinkSearchPath, LinkSearchPathKind};
use bray_source::{SourceIdentity, SourceInput};
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
    let output = output.join(FIXTURE_PRODUCT);

    fs::create_dir(&output).map_err(|error| BuildError::write(&output, error))?;

    let fixture = build_native_fixture(root, &output, target)?;

    emit_fixture(root, &output, toolchain, runtime, target, &fixture)?;

    let executable = output.join(output_name(target, TargetOutputKind::Executable, FIXTURE_PRODUCT)?);
    let mut command = Command::new(&executable);

    command.current_dir(&output);

    crate::command::require_success(command, "executing foreign interoperability fixture")
        .map(|_| ())
        .map_err(|error| BuildError::conformance("foreign interoperability", error))
}

fn build_native_fixture(
    root: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<NativeFixture, BuildError> {
    let source = root.join("xtask/fixtures/foreign-interoperability.c");
    let object = output.join(if cfg!(windows) { "fixture.obj" } else { "fixture.o" });

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

    let clang = native_tool("clang")?;
    let mut compile = Command::new(&clang);

    compile.args(["-std=c11", "-O2", "-c"]).arg(&source).arg("-o").arg(&object);

    if !cfg!(windows) {
        compile.args(["-fPIC", "-pthread"]);
    }

    crate::command::require_success(compile, "compiling foreign interoperability fixture")
        .map_err(|error| BuildError::conformance("foreign interoperability", error))?;

    let mut shared_command = Command::new(clang);

    shared_command.arg("-shared").arg(&object).arg("-o").arg(&shared);

    if !cfg!(windows) {
        shared_command.arg("-pthread");
    }

    crate::command::require_success(shared_command, "linking foreign interoperability fixture")
        .map_err(|error| BuildError::conformance("foreign interoperability", error))?;

    if archive.exists() {
        fs::remove_file(&archive).map_err(|error| BuildError::write(&archive, error))?;
    }

    let mut archive_command = Command::new(native_tool("llvm-ar")?);

    archive_command.args(["rcs"]).arg(&archive).arg(&object);

    crate::command::require_success(archive_command, "archiving foreign interoperability fixture")
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
) -> Result<(), BuildError> {
    let source_path = root.join("xtask/fixtures/foreign-interoperability.bray");

    let source = fs::read_to_string(&source_path)
        .map_err(|error| BuildError::read(&source_path, error))?
        .replace("__SHARED_LIBRARY_PATH__", &bray_path(&fixture.shared))
        .replace("__TARGET_HANDLE_AUDIT__", target_handle_audit(target));

    let package = PackageIdentity::try_new("bray.interoperability")
        .ok_or(BuildError::InvalidIdentity)?;

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

    let standard_library = StandardLibraryRoot::try_new(
        toolchain.join("lib").join("bray").join("standard-library"),
    )
    .ok_or_else(|| {
        BuildError::conformance("foreign interoperability", "invalid standard-library root")
    })?;

    let source = SourceInput::virtual_text(SourceIdentity::new(1), "foreign-interoperability.bray", 1, source);

    let request = CompilationRequest::with_options(package, vec![source], options)
        .with_standard_library_root(standard_library);

    let compilation = load_llvm_compilation(request).ok_or_else(|| {
        BuildError::conformance("foreign interoperability", "LLVM compiler backend is unavailable")
    })?;

    let product_kind = compilation
        .product_semantic_facts()
        .map_err(|error| compilation_error("resolving product semantics", error))?
        .value()
        .kind();

    if product_kind != ProductKind::Executable {
        return Err(BuildError::conformance(
            "foreign interoperability",
            format!("unexpected product kind: {product_kind:?}"),
        ));
    }

    let search_path = LinkSearchPath::try_new(LinkSearchPathKind::Library, output)
        .map_err(|error| BuildError::conformance("foreign interoperability", format!("invalid library search path: {error:?}")))?;

    crate::native_product::emit_executable(
        compilation,
        product,
        target,
        runtime,
        output,
        [search_path],
    )
    .map_err(|error| BuildError::conformance("foreign interoperability", error))
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
    path.to_string_lossy().replace('\\', "/")
}

const fn target_handle_audit(target: NativeTarget) -> &'static str {
    match target {
        NativeTarget::X86_64WindowsMsvc | NativeTarget::Aarch64WindowsMsvc => {
            r#"using std.os.windows;

trusted func audit_target_handle()
{
    let handle: std.os.windows.Handle = std.os.windows.handle(42);

    assert(handle.value() == 42);
}"#
        }
        NativeTarget::X86_64LinuxGnu | NativeTarget::Aarch64LinuxGnu => {
            r#"using std.os.linux;

trusted func audit_target_handle()
{
    let descriptor: std.os.linux.FileDescriptor = std.os.linux.file_descriptor(42);

    assert(descriptor.value() == 42);
}"#
        }
        NativeTarget::X86_64MacOs | NativeTarget::Aarch64MacOs => {
            r#"using std.os.darwin;

trusted func audit_target_handle()
{
    let descriptor: std.os.darwin.FileDescriptor = std.os.darwin.file_descriptor(42);

    assert(descriptor.value() == 42);
}"#
        }
    }
}

fn native_tool(name: &str) -> Result<PathBuf, BuildError> {
    llvm_tool_path(name).ok_or_else(|| {
        BuildError::conformance(
            "foreign interoperability",
            format!("the provisioned {name} tool is unavailable"),
        )
    })
}

struct NativeFixture {
    shared: PathBuf,
}
