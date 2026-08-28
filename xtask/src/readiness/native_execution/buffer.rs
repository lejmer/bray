use std::path::Path;

use bray_compilation::{
    Compilation, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::NativeTarget;
use bray_tooling::{load_llvm_compilation, source_inputs_from_file_arguments};

use super::core::{BuiltFixture, PRODUCT_NAME, STANDARD_MEMORY_FIXTURE, execute_product};

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
const STANDARD_FORMAT_FIXTURES: &[&str] = &[
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
const STANDARD_MEMORY_RUNTIME_SOURCES: &[&str] = &[
    "standard-library/std/src/runtime.bray",
    "standard-library/std/src/runtime/memory.bray",
];
const STANDARD_TEXT_RUNTIME_SOURCES: &[&str] = &[
    "standard-library/std/src/runtime/text.bray",
    "standard-library/std/src/runtime/character.bray",
    "standard-library/std/src/runtime/character/unicode_tables.bray",
];

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
    let output =
        BuiltFixture::build_standard_library("bray-native-standard-format-", target, |output| {
            build_standard_library_fixtures(root, target, runtime, output, STANDARD_FORMAT_FIXTURES)
        })?;

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
    let diagnostics = compilation.check_diagnostics();

    if diagnostics.has_errors() {
        return Err(format!(
            "standard-library fixture sources did not pass compilation checks: {diagnostics:?}"
        ));
    }

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
    let mut source_paths: Vec<_> = fixtures.iter().map(|fixture| root.join(fixture)).collect();

    if !fixtures.contains(&STANDARD_ROOT_SOURCE) {
        source_paths.push(root.join(STANDARD_ROOT_SOURCE));
    }

    if !fixtures.contains(&STANDARD_MEMORY_SOURCE) && !fixtures.contains(&STANDARD_MEMORY_FIXTURE) {
        source_paths.push(root.join(STANDARD_MEMORY_SOURCE));
    }

    source_paths.extend(
        STANDARD_MEMORY_RUNTIME_SOURCES
            .iter()
            .map(|source| root.join(source)),
    );

    if fixtures.contains(&STANDARD_STRING_SOURCE) {
        source_paths.extend(
            STANDARD_TEXT_RUNTIME_SOURCES
                .iter()
                .map(|source| root.join(source)),
        );
    }

    source_paths.extend(
        runtime_platform_sources(target)
            .iter()
            .map(|source| root.join(source)),
    );

    let sources = source_inputs_from_file_arguments(source_paths)
        .map_err(|error| format!("could not load standard-library fixture source: {error:?}"))?;

    let package = PackageIdentity::try_new("std")
        .ok_or_else(|| "standard-library package identity is invalid".to_owned())?;

    let selected = SelectedTarget::for_native(target);

    let native_links = crate::standard_library::native_links(selected.profile().identity())
        .map_err(|error| format!("could not load native link inputs: {error}"))?;

    let options = CompilationOptions::new(
        WorkerBudget::default(),
        ProductKind::Executable,
        selected.clone(),
    )
    .with_native_link_inputs(native_links);

    let request = CompilationRequest::with_options(package, sources, options)
        .with_standard_library_source_authority();

    load_llvm_compilation(request)
        .map_err(|error| format!("LLVM compiler backend is unavailable: {error}"))
}

const fn runtime_platform_sources(target: NativeTarget) -> &'static [&'static str] {
    if matches!(
        target,
        NativeTarget::X86_64WindowsMsvc | NativeTarget::Aarch64WindowsMsvc
    ) {
        &[
            "standard-library/std/src/platform.bray",
            "standard-library/std/src/platform/heap.bray",
            "standard-library/std/src/platform/heap/windows.bray",
        ]
    } else {
        &[
            "standard-library/std/src/platform.bray",
            "standard-library/std/src/platform/heap.bray",
            "standard-library/std/src/platform/heap/unix.bray",
        ]
    }
}

#[cfg(test)]
mod tests {
    use bray_target::NativeTarget;

    use super::{
        STANDARD_BUFFER_FIXTURES, STANDARD_BYTES_ROOT_SOURCE, STANDARD_BYTES_SOURCE,
        STANDARD_FORMAT_FIXTURES, STANDARD_MEMORY_FIXTURE, STANDARD_MEMORY_SOURCE,
        STANDARD_ROOT_SOURCE, STANDARD_STRING_SOURCE, standard_library_compilation,
    };

    #[test]
    fn standard_format_fixture_source_authority_is_complete() {
        assert_source_authority(STANDARD_FORMAT_FIXTURES);
    }

    #[test]
    fn standard_buffer_fixture_source_authority_is_complete() {
        assert_source_authority(&[
            STANDARD_ROOT_SOURCE,
            STANDARD_MEMORY_SOURCE,
            STANDARD_BYTES_ROOT_SOURCE,
            STANDARD_BYTES_SOURCE,
            STANDARD_BUFFER_FIXTURES[0],
        ]);
    }

    #[test]
    fn text_cursor_fixture_source_authority_is_complete() {
        assert_source_authority(&[
            STANDARD_STRING_SOURCE,
            "xtask/fixtures/native-execution/standard-text-cursor.bray",
        ]);
    }

    #[test]
    fn memory_layout_fixture_source_authority_is_complete() {
        assert_source_authority(&[
            STANDARD_MEMORY_FIXTURE,
            "xtask/fixtures/native-execution/memory-layout.bray",
        ]);
    }

    fn assert_source_authority(fixtures: &[&str]) {
        let root = crate::workspace::root()
            .unwrap_or_else(|error| panic!("workspace root must resolve: {error}"));

        let target = NativeTarget::current()
            .unwrap_or_else(|| panic!("test host must have a supported native target"));

        let compilation = standard_library_compilation(&root, target, fixtures)
            .unwrap_or_else(|error| panic!("fixture compilation must load: {error}"));

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "fixture source authority must be complete: {:?}",
            compilation.check_diagnostics()
        );
    }
}
