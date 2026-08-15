use std::path::Path;

use bray_compilation::{
    Compilation, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::NativeTarget;
use bray_tooling::{load_llvm_compilation, source_inputs_from_file_arguments};

use super::core::{BuiltFixture, PRODUCT_NAME, execute_product};

const STANDARD_BUFFER_FIXTURES: &[&str] = &[
    "xtask/fixtures/native-execution/standard-raw-buffer.bray",
    "xtask/fixtures/native-execution/standard-buffer.bray",
    "xtask/fixtures/native-execution/standard-buffer-reserve.bray",
    "xtask/fixtures/native-execution/standard-buffer-from-slice.bray",
    "xtask/fixtures/native-execution/standard-buffer-push.bray",
    "xtask/fixtures/native-execution/standard-buffer-append.bray",
    "xtask/fixtures/native-execution/standard-buffer-resize.bray",
    "xtask/fixtures/native-execution/standard-buffer-truncate.bray",
    "xtask/fixtures/native-execution/standard-buffer-clear.bray",
    "xtask/fixtures/native-execution/standard-buffer-pop.bray",
];
const STANDARD_ROOT_SOURCE: &str = "standard-library/std/src/std.bray";
const STANDARD_MEMORY_SOURCE: &str = "standard-library/std/src/memory.bray";
const STANDARD_BYTES_ROOT_SOURCE: &str = "standard-library/std/src/bytes.bray";
const STANDARD_BYTES_SOURCE: &str = "standard-library/std/src/bytes/buffer.bray";
const STANDARD_CHARACTER_SOURCE: &str = "standard-library/std/src/character.bray";
const STANDARD_NUMERIC_CHECKED_SOURCE: &str = "standard-library/std/src/numeric/checked.bray";
const STANDARD_NUMERIC_LIMITS_SOURCE: &str = "standard-library/std/src/numeric/limits.bray";
const STANDARD_FORMAT_ROOT_SOURCE: &str = "standard-library/std/src/format.bray";
const STANDARD_FORMAT_OPTIONS_SOURCE: &str = "standard-library/std/src/format/options.bray";
const STANDARD_FORMAT_ARGUMENT_SOURCE: &str = "standard-library/std/src/format/argument.bray";
const STANDARD_FORMAT_SINK_SOURCE: &str = "standard-library/std/src/format/sink.bray";
const STANDARD_FORMAT_INTEGER_WIDTH_SOURCE: &str =
    "standard-library/std/src/format/integer_width.bray";
const STANDARD_FORMAT_RENDERING_SOURCE: &str = "standard-library/std/src/format/rendering.bray";
const STANDARD_FORMAT_FIXTURE: &str = "xtask/fixtures/native-execution/standard-format.bray";
const STANDARD_STRING_SOURCE: &str = "standard-library/std/src/string.bray";

pub(super) fn audit_standard_buffer(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    for fixture in STANDARD_BUFFER_FIXTURES {
        let fixtures = [
            STANDARD_ROOT_SOURCE,
            STANDARD_MEMORY_SOURCE,
            STANDARD_BYTES_ROOT_SOURCE,
            STANDARD_BYTES_SOURCE,
            fixture,
        ];

        let output = BuiltFixture::build_standard_library(
            "bray-native-standard-buffer-",
            target,
            |output| build_standard_library_fixtures(root, target, runtime, output, &fixtures),
        )
        .map_err(|error| format!("{fixture}: {error}"))?;

        let context = format!("executing standard byte-buffer fixture {fixture}");

        execute_product(output.executable(), 0, &context)?;
    }

    Ok(())
}

pub(super) fn audit_standard_format(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    let fixtures = [
        STANDARD_ROOT_SOURCE,
        STANDARD_MEMORY_SOURCE,
        STANDARD_BYTES_ROOT_SOURCE,
        STANDARD_BYTES_SOURCE,
        STANDARD_STRING_SOURCE,
        STANDARD_CHARACTER_SOURCE,
        STANDARD_NUMERIC_CHECKED_SOURCE,
        STANDARD_NUMERIC_LIMITS_SOURCE,
        STANDARD_FORMAT_ROOT_SOURCE,
        STANDARD_FORMAT_OPTIONS_SOURCE,
        STANDARD_FORMAT_ARGUMENT_SOURCE,
        STANDARD_FORMAT_SINK_SOURCE,
        STANDARD_FORMAT_INTEGER_WIDTH_SOURCE,
        STANDARD_FORMAT_RENDERING_SOURCE,
        STANDARD_FORMAT_FIXTURE,
    ];

    let output = BuiltFixture::build_standard_library(
        "bray-native-standard-format-",
        target,
        |output| build_standard_library_fixtures(root, target, runtime, output, &fixtures),
    )?;

    execute_product(
        output.executable(),
        0,
        "executing standard formatting fixture",
    )
}

pub(super) fn build_standard_library_fixtures(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    output: &Path,
    fixtures: &[&str],
) -> Result<(), String> {
    let compilation = standard_library_compilation(root, target, fixtures)?;

    let package = PackageIdentity::try_new("std")
        .ok_or_else(|| "standard-library package identity is invalid".to_owned())?;

    let product = ProductIdentity::try_new(package, PRODUCT_NAME)
        .ok_or_else(|| "standard-library fixture product identity is invalid".to_owned())?;

    crate::native_product::emit_executable(&compilation, product, target, runtime, output, [], None)
}

pub(super) fn standard_library_compilation(
    root: &Path,
    target: NativeTarget,
    fixtures: &[&str],
) -> Result<Compilation, String> {
    let source_paths = fixtures.iter().map(|fixture| root.join(fixture));

    let sources = source_inputs_from_file_arguments(source_paths)
        .map_err(|error| format!("could not load standard-library fixture source: {error:?}"))?;

    let package = PackageIdentity::try_new("std")
        .ok_or_else(|| "standard-library package identity is invalid".to_owned())?;

    let selected = SelectedTarget::for_native(target);

    let options = CompilationOptions::new(
        WorkerBudget::default(),
        ProductKind::Executable,
        selected.clone(),
    );

    let request = CompilationRequest::with_options(package, sources, options)
        .with_standard_library_source_authority();

    load_llvm_compilation(request)
        .map_err(|error| format!("LLVM compiler backend is unavailable: {error}"))
}
