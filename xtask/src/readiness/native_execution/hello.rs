use std::path::Path;

use bray_compilation::{CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget};
use bray_standard_library::StandardLibraryRoot;
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::NativeTarget;
use bray_tooling::{load_llvm_compilation, source_inputs_from_file_arguments};

use super::buffer::emit_native_executable;
use super::core::{PRODUCT_NAME, command_failure, executable_path, native_output, product_output};

const HELLO_WORLD_FIXTURE: &str = "xtask/fixtures/native-execution/standard-hello-world.bray";
const HELLO_WORLD_OUTPUT: &[u8] = b"Hello world!";

pub(super) fn audit_standard_hello_world(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    let bundle_root = native_output("bray-standard-library-bundle-")?;
    let bundle = bundle_root.path().join("bundle");

    crate::standard_library::build_target_bundle(&root.join("standard-library"), &bundle, target)?;

    let compilation = hello_world_compilation(root, target, &bundle)?;
    let output = native_output("bray-standard-hello-world-")?;

    let package = PackageIdentity::try_new("example.hello")
        .ok_or_else(|| "hello-world package identity is invalid".to_owned())?;

    let product = ProductIdentity::try_new(package, PRODUCT_NAME)
        .ok_or_else(|| "hello-world product identity is invalid".to_owned())?;

    emit_native_executable(compilation, product, target, runtime, output.path())?;

    let executable = executable_path(output.path(), target);
    let result = product_output(&executable, "executing standard-library hello world")?;

    if !result.status.success() {
        return Err(command_failure(
            "executing standard-library hello world",
            &result,
        ));
    }

    if result.stdout != HELLO_WORLD_OUTPUT {
        return Err(format!(
            "standard-library hello world wrote unexpected stdout: {:?}",
            String::from_utf8_lossy(&result.stdout)
        ));
    }

    if !result.stderr.is_empty() {
        return Err(format!(
            "standard-library hello world wrote unexpected stderr: {:?}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }

    Ok(())
}

fn hello_world_compilation(
    root: &Path,
    target: NativeTarget,
    standard_library: &Path,
) -> Result<bray_compilation::Compilation, String> {
    let sources = source_inputs_from_file_arguments([root.join(HELLO_WORLD_FIXTURE)])
        .map_err(|error| format!("could not load hello-world source: {error:?}"))?;

    let package = PackageIdentity::try_new("example.hello")
        .ok_or_else(|| "hello-world package identity is invalid".to_owned())?;

    let selected = SelectedTarget::for_native(target);

    let options =
        CompilationOptions::new(WorkerBudget::default(), ProductKind::Executable, selected);

    let standard_library = StandardLibraryRoot::try_new(standard_library)
        .ok_or_else(|| "standard-library bundle root is invalid".to_owned())?;

    let request = CompilationRequest::with_options(package, sources, options)
        .with_standard_library_root(standard_library);

    load_llvm_compilation(request).ok_or_else(|| "LLVM compiler backend is unavailable".to_owned())
}
