use std::fs;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use bray_base::NonEmptySharedStr;
use bray_compilation::{
    BuildConfiguration, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_diagnostics::DiagnosticLlvmToolRole;
use bray_linker::{LinkSearchPath, LinkSearchPathKind, SystemLinkerMapOutput};
use bray_source::{SourceIdentity, SourceInput};
use bray_standard_library::StandardLibraryRoot;
use bray_symbols::{
    NativeLinkKind, NativeLinkRequirement, PackageIdentity, ProductIdentity, ProductKind,
};
use bray_target::NativeTarget;

use super::command::BuildError;

const CHECK: &str = "Unicode classification";
const ROUNDS: u32 = 65_536;
const METHODS: [&str; 3] = ["is_alphabetic", "is_numeric", "is_whitespace"];
const WORKLOADS: [(&str, [u32; 16]); 4] = [
    (
        "ascii-heavy",
        [
            65, 66, 67, 90, 97, 98, 99, 122, 48, 49, 57, 32, 9, 13, 64, 91,
        ],
    ),
    (
        "multilingual",
        [
            0xe9, 0x345, 0x3b1, 0x416, 0x5d0, 0x627, 0x905, 0x754c, 0x56db, 0x667, 0xb2, 0x2153,
            0x85, 0xa0, 0x2003, 0x3000,
        ],
    ),
    (
        "mixed-plane",
        [
            0x41, 0x10400, 0x11db0, 0x1d7ce, 0x11de0, 0x20000, 0x31350, 0x1f600, 0x10ffff,
            0x100000, 0xf0000, 0x40000, 0x3000, 0x10000, 0xe000, 0xd7ff,
        ],
    ),
    (
        "boundary",
        [
            0x40, 0x41, 0x5a, 0x5b, 0x2f, 0x30, 0x39, 0x3a, 0x8, 0x9, 0xd, 0xe, 0x200a, 0x200b,
            0x11ddf, 0x11de0,
        ],
    ),
];

pub(super) fn audit(
    root: &Path,
    output: &Path,
    toolchain: &Path,
    runtime: &Path,
    target: NativeTarget,
) -> Result<(), BuildError> {
    let output = output.join("unicode");

    fs::create_dir(&output).map_err(|error| BuildError::write(&output, error))?;
    build_opaque_scalar(root, &output)?;

    let payloads = super::unicode_data::table_payloads().map_err(failure)?;

    println!(
        "  Unicode table bytes [root, branches, alphabetic, numeric, whitespace]: {:?}",
        payloads.each_ref().map(Vec::len)
    );

    for mask in [0, 1, 2, 4, 7] {
        for configuration in [
            BuildConfiguration::Release,
            BuildConfiguration::ObjectRelease,
        ] {
            let name = format!("retention-{mask}-{}", configuration.as_str());
            let source = source(&WORKLOADS[2].1, mask, &payloads);

            let (executable, _) = emit(
                &output,
                toolchain,
                runtime,
                target,
                &name,
                source,
                configuration,
            )?;

            verify_retention(&executable, mask, &payloads)?;
            execute(&executable, &output)?;
        }
    }

    for (name, scalars) in WORKLOADS {
        let source = source(&scalars, 7, &payloads);

        let (executable, map) = emit(
            &output,
            toolchain,
            runtime,
            target,
            &format!("{name}-memory"),
            source.clone(),
            BuildConfiguration::ObservedRelease,
        )?;

        crate::performance::verify_no_memory_work(&executable, &map, &output).map_err(failure)?;

        let mut timings = Vec::new();

        for repeats in [1, 4] {
            let configuration = BuildConfiguration::TimedRelease {
                inner_iterations: NonZeroU64::new(repeats).expect("repeat count is nonzero"),
            };

            let (executable, _) = emit(
                &output,
                toolchain,
                runtime,
                target,
                &format!("{name}-timing-{repeats}"),
                source.clone(),
                configuration,
            )?;

            let mut samples = Vec::new();

            for iteration in 0..3 {
                samples.push(
                    crate::performance::time_empty_program(&executable, &output, iteration)
                        .map_err(failure)?,
                );
            }

            samples.sort_unstable();
            timings.push(samples[1]);
        }

        // A generous upper bound rejects superlinear work without timing noise becoming a correctness failure.
        if timings[1] > timings[0].saturating_mul(8).saturating_add(5_000_000) {
            return Err(failure(format!(
                "{name} classification does not scale linearly: {timings:?}"
            )));
        }

        println!(
            "  {name}: {} and {} scalars, median controlled ns {:?}, allocations 0",
            ROUNDS,
            ROUNDS * 4,
            timings
        );
    }

    Ok(())
}

fn build_opaque_scalar(root: &Path, output: &Path) -> Result<(), BuildError> {
    let object = output.join("unicode-scalar.o");

    let compiler = bray_tooling::llvm_tool_path(DiagnosticLlvmToolRole::CompilerDriver)
        .map_err(|error| failure(error.to_string()))?;

    crate::native_product::compile_c_fixture(
        &compiler,
        &root.join("xtask/fixtures/unicode-scalar.c"),
        &object,
        &[],
    )
    .map_err(failure)?;

    let bytes = fs::read(&object).map_err(|error| BuildError::read(&object, error))?;

    let archive = crate::native_archive::archive_bytes(
        &bray_llvm_toolchain::tool_path(root, "llvm-ar"),
        &[bytes],
        "o",
    )
    .map_err(|error| failure(error.to_string()))?;

    let path = output.join(if cfg!(windows) {
        "bray_unicode.lib"
    } else {
        "libbray_unicode.a"
    });

    fs::write(&path, archive).map_err(|error| BuildError::write(&path, error))
}

fn source(scalars: &[u32; 16], mask: u8, payloads: &[Vec<u8>; 5]) -> String {
    let expected: usize = scalars
        .iter()
        .map(|&scalar| {
            (0..3)
                .filter(|&property| {
                    mask & (1 << property) != 0 && contains(payloads, scalar as usize, property)
                })
                .count()
        })
        .sum();

    let mut operations = String::new();

    for (property, method) in METHODS.iter().enumerate() {
        if mask & (1 << property) != 0 {
            operations.push_str(&format!("        if value.{method}() {{ total += 1; }}\n"));
        }
    }

    format!(
        r#"trusted module unicode_fixture;
using std.character;

@link(name = "bray_unicode", kind = static)
@symbol(name = "bray_unicode_scalar")
@abi(c)
extern trusted func opaque_scalar(pos value: u32) -> u32
    uses(foreign_call);

trusted func main()
    uses(foreign_call)
{{
    let scalars: [u32; 16] = {scalars:?};
    let mut index: usize = 0;
    let mut total: usize = 0;

    while index < {ROUNDS}
    {{
        let scalar: u32 = trusted opaque_scalar(scalars[index % 16]);
        let value: char = match char.from_code_point(scalar)
        {{
            case ?character {{ yield character; }}
            case none {{ panic("workload scalar must be valid"); }}
        }};

        assert(value.code_point() == scalar);
{operations}
        index += 1;
    }}

    assert(total == {});
}}
"#,
        expected * ROUNDS as usize / scalars.len()
    )
}

fn contains(payloads: &[Vec<u8>; 5], scalar: usize, property: usize) -> bool {
    let read = |bytes: &[u8], index: usize| {
        usize::from(u16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]]))
    };

    let branch = read(&payloads[0], scalar >> 10);
    let leaf = read(&payloads[1], branch * 32 + ((scalar >> 5) & 31));

    payloads[property + 2][leaf * 4 + (scalar & 31) / 8] >> (scalar & 7) & 1 != 0
}

#[expect(
    clippy::too_many_arguments,
    reason = "fixture emission needs the exact toolchain, runtime, target, identity, source and build configuration"
)]
fn emit(
    output: &Path,
    toolchain: &Path,
    runtime: &Path,
    target: NativeTarget,
    name: &str,
    source: String,
    configuration: BuildConfiguration,
) -> Result<(PathBuf, PathBuf), BuildError> {
    let directory = output.join(name);

    fs::create_dir(&directory).map_err(|error| BuildError::write(&directory, error))?;

    let source =
        bray_formatter::format_text(&source, &bray_formatter::FormatterConfiguration::default())
            .expect("generated Unicode workload source must format")
            .into_text();

    let source_path = directory.join("unicode-fixture.bray");

    fs::write(&source_path, &source).map_err(|error| BuildError::write(&source_path, error))?;

    let package =
        PackageIdentity::try_new("bray.unicode_fixture").ok_or(BuildError::InvalidIdentity)?;

    let product =
        ProductIdentity::try_new(package.clone(), name).ok_or(BuildError::InvalidIdentity)?;

    let link = NativeLinkRequirement::new(
        NonEmptySharedStr::try_new("bray_unicode").ok_or(BuildError::InvalidIdentity)?,
        NativeLinkKind::Static,
    );

    let options = CompilationOptions::new(
        WorkerBudget::default(),
        ProductKind::Executable,
        SelectedTarget::for_native(target),
    )
    .with_native_link_inputs([link]);

    let standard_library =
        StandardLibraryRoot::try_new(toolchain.join("lib/bray/standard-library"))
            .ok_or(BuildError::InvalidIdentity)?;

    let request = CompilationRequest::with_options(
        package,
        vec![SourceInput::virtual_text(
            SourceIdentity::new(1),
            source_path.display().to_string(),
            1,
            source,
        )],
        options,
    )
    .with_standard_library_root(standard_library);

    let compilation =
        bray_tooling::load_llvm_compilation(request).map_err(|error| failure(error.to_string()))?;

    let diagnostics = compilation.check_diagnostics();

    if diagnostics.has_errors() {
        return Err(failure(crate::diagnostic_output::failure_detail(
            "Unicode fixture source did not pass compilation checks".to_owned(),
            [diagnostics],
            compilation.sources(),
        )));
    }

    let map = directory.join("application.map");

    let map_output =
        SystemLinkerMapOutput::try_new(map.clone()).ok_or(BuildError::InvalidIdentity)?;

    let search = LinkSearchPath::try_new(LinkSearchPathKind::Library, output)
        .map_err(|error| failure(format!("{error:?}")))?;

    crate::native_product::emit_executable_with_configuration(
        &compilation,
        product.clone(),
        target,
        runtime,
        &directory,
        [search],
        Some(map_output),
        configuration,
    )
    .map_err(failure)?;

    let destination = crate::native_product::managed_destination(&directory).map_err(failure)?;
    let executable = super::artifact::resolve_executable(destination, &product, CHECK)?;

    Ok((executable.path().to_owned(), map))
}

fn verify_retention(
    executable: &Path,
    mask: u8,
    payloads: &[Vec<u8>; 5],
) -> Result<(), BuildError> {
    let bytes = fs::read(executable).map_err(|error| BuildError::read(executable, error))?;
    let mut retained = 0;

    for (index, payload) in payloads.iter().enumerate() {
        let expected = if index < 2 {
            mask != 0
        } else {
            mask & (1 << (index - 2)) != 0
        };

        let count = bytes
            .windows(payload.len())
            .filter(|window| *window == payload)
            .count();

        if count != usize::from(expected) {
            return Err(failure(format!(
                "mask {mask}, table {index}: retained {count} copies, expected {}",
                usize::from(expected)
            )));
        }

        retained += count * payload.len();
    }

    println!(
        "  predicate mask {mask}: {retained} retained table bytes, {} product bytes",
        bytes.len()
    );

    Ok(())
}

fn execute(executable: &Path, output: &Path) -> Result<(), BuildError> {
    crate::validation_process::success(
        subprocess::Exec::cmd(executable).cwd(output),
        crate::validation_process::TIMEOUT,
    )
    .map(|_| ())
    .map_err(|error| failure(error.to_string()))
}

fn failure(error: impl Into<String>) -> BuildError {
    BuildError::conformance(CHECK, error)
}
