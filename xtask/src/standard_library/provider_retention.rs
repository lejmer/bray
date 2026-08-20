use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_compilation::SelectedTarget;
use bray_runtime_interface::PlatformServiceRole;
use bray_standard_library::StandardLibraryArtifactKind;
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::{NativeTarget, ObjectFormat};

use super::command::BuildError;

const CIVIL_MEMBER: &str = "-civil.o";
const TEXT_MEMBER: &str = "-text.o";
const TIMEZONE_MEMBER: &str = "-timezone.o";
const DATABASE_MEMBER: &str = "-tz.o";
const STANDARD_INPUT_MEMBER: &str = "-input.o";
const STANDARD_OUTPUT_MEMBER: &str = "-output.o";
const STANDARD_ERROR_MEMBER: &str = "-error.o";

pub(super) fn audit(
    root: &Path,
    output: &Path,
    toolchain: &Path,
    target: NativeTarget,
) -> Result<(), BuildError> {
    let standard_stream =
        platform_archive(toolchain, target, PlatformServiceRole::StandardOutputWrite)?;

    let temporal = platform_archive(toolchain, target, PlatformServiceRole::TimeDateValidate)?;
    let temporal_include = root.join("crates/bray-platform-abi/native/temporal/include");

    let standard_stream_include =
        root.join("crates/bray-platform-abi/native/standard_stream/include");

    let output = output.join("provider-retention");

    fs::create_dir(&output).map_err(|error| BuildError::write(&output, error))?;

    let print = link_fixture(
        root,
        &output,
        &standard_stream_include,
        &standard_stream,
        target,
        "provider-retention-print",
    )?;

    require_members(
        &print.map,
        &[STANDARD_OUTPUT_MEMBER],
        &[STANDARD_INPUT_MEMBER, STANDARD_ERROR_MEMBER],
        "print-only",
    )?;

    require_atomic_standard_output(&print.standard_output)?;

    let input = link_fixture(
        root,
        &output,
        &standard_stream_include,
        &standard_stream,
        target,
        "provider-retention-input",
    )?;

    require_members(
        &input.map,
        &[STANDARD_INPUT_MEMBER],
        &[STANDARD_OUTPUT_MEMBER, STANDARD_ERROR_MEMBER],
        "input-only",
    )?;

    let civil = link_fixture(
        root,
        &output,
        &temporal_include,
        &temporal,
        target,
        "provider-retention-civil",
    )?;

    require_members(
        &civil.map,
        &[CIVIL_MEMBER],
        &[TEXT_MEMBER, TIMEZONE_MEMBER, DATABASE_MEMBER],
        "civil-date",
    )?;

    let text = link_fixture(
        root,
        &output,
        &temporal_include,
        &temporal,
        target,
        "provider-retention-text",
    )?;

    require_members(
        &text.map,
        &[TEXT_MEMBER],
        &[CIVIL_MEMBER, TIMEZONE_MEMBER, DATABASE_MEMBER],
        "parse-format",
    )?;

    let timezone = link_fixture(
        root,
        &output,
        &temporal_include,
        &temporal,
        target,
        "provider-retention-timezone",
    )?;

    require_members(
        &timezone.map,
        &[TIMEZONE_MEMBER, DATABASE_MEMBER],
        &[CIVIL_MEMBER, TEXT_MEMBER],
        "named-timezone",
    )
}

struct ProviderArchive {
    path: PathBuf,
    native_links: Vec<NativeLinkRequirement>,
}

struct ProviderFixtureOutput {
    map: String,
    standard_output: Vec<u8>,
}

fn platform_archive(
    toolchain: &Path,
    target: NativeTarget,
    role: PlatformServiceRole,
) -> Result<ProviderArchive, BuildError> {
    let root = toolchain.join("lib/bray/standard-library");
    let manifest = super::command::read_manifest(&root)?;
    let abi = SelectedTarget::for_native(target).runtime_abi();

    let artifacts = manifest
        .targets()
        .iter()
        .find(|artifacts| {
            artifacts.target() == &target.identity() && artifacts.runtime_abi() == abi
        })
        .ok_or_else(|| {
            BuildError::conformance(
                "native provider retention",
                format!(
                    "standard-library artifacts are unavailable for {}",
                    target.as_str()
                ),
            )
        })?;

    let artifact = artifacts
        .artifacts()
        .iter()
        .find(|artifact| {
            artifact.kind() == StandardLibraryArtifactKind::PlatformServiceLibrary
                && artifact.platform_services().binary_search(&role).is_ok()
        })
        .ok_or_else(|| {
            BuildError::conformance(
                "native provider retention",
                format!(
                    "platform provider archive is unavailable for {}",
                    target.as_str()
                ),
            )
        })?;

    Ok(ProviderArchive {
        path: artifact.beneath(&root),
        native_links: artifact.native_links().to_vec(),
    })
}

fn link_fixture(
    root: &Path,
    output: &Path,
    include: &Path,
    provider: &ProviderArchive,
    target: NativeTarget,
    name: &str,
) -> Result<ProviderFixtureOutput, BuildError> {
    let source = root.join("xtask/fixtures").join(format!("{name}.cpp"));
    let executable = output.join(crate::native_toolchain::executable_name(name));
    let map = output.join(format!("{name}.map"));

    let clang =
        bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver)
            .map_err(|error| {
                BuildError::conformance(
                    "native provider retention",
                    format!("clang is unavailable: {error}"),
                )
            })?;

    let mut command = Command::new(clang);

    command
        .args(["--driver-mode=g++", "-std=c++17", "-O2", "-fuse-ld=lld"])
        .arg(format!("-I{}", include.display()))
        .arg(&source)
        .arg(&provider.path)
        .arg("-o")
        .arg(&executable);

    if target.object_format() != ObjectFormat::Coff {
        command.arg("-pthread");
    }

    append_native_links(&mut command, target.object_format(), &provider.native_links)?;
    configure_link_map(&mut command, target.object_format(), &map)?;

    crate::command::require_success(command, "linking native provider retention fixture")
        .map_err(|error| BuildError::conformance("native provider retention", error))?;

    let mut execution = Command::new(&executable);

    execution.current_dir(output);

    let execution = crate::command::require_success(
        execution,
        "executing native provider retention fixture",
    )
        .map_err(|error| BuildError::conformance("native provider retention", error))?;

    let map = fs::read_to_string(&map).map_err(|error| BuildError::read(&map, error))?;

    Ok(ProviderFixtureOutput {
        map,
        standard_output: execution.stdout,
    })
}

fn require_atomic_standard_output(output: &[u8]) -> Result<(), BuildError> {
    let first = [vec![b'a'; 64], vec![b'b'; 64]].concat();
    let second = [vec![b'b'; 64], vec![b'a'; 64]].concat();

    if output == first || output == second {
        return Ok(());
    }

    Err(BuildError::conformance(
        "native provider retention",
        "concurrent standard-output operations interleaved",
    ))
}

fn append_native_links(
    command: &mut Command,
    format: ObjectFormat,
    links: &[NativeLinkRequirement],
) -> Result<(), BuildError> {
    for link in links {
        match (format, link.kind()) {
            (
                ObjectFormat::Coff | ObjectFormat::Elf | ObjectFormat::MachO,
                NativeLinkKind::Dynamic | NativeLinkKind::Static | NativeLinkKind::System,
            ) => {
                command.arg(format!("-l{}", link.name()));
            }
            (ObjectFormat::MachO, NativeLinkKind::Framework) => {
                command.arg("-framework").arg(link.name());
            }
            (_, NativeLinkKind::Framework) => {
                return Err(BuildError::conformance(
                    "native provider retention",
                    "native framework requirement is invalid for the target",
                ));
            }
            (ObjectFormat::WebAssembly | ObjectFormat::Xcoff, _) => {
                return Err(BuildError::conformance(
                    "native provider retention",
                    format!("unsupported native object format: {}", format.as_str()),
                ));
            }
        }
    }

    Ok(())
}

fn configure_link_map(
    command: &mut Command,
    format: ObjectFormat,
    map: &Path,
) -> Result<(), BuildError> {
    let family = match format {
        ObjectFormat::Coff => {
            command.arg("-Xlinker").arg("/OPT:REF");

            bray_linker::SystemLinkerFamily::MicrosoftCompiler
        }
        ObjectFormat::Elf => {
            command.arg("-Xlinker").arg("--gc-sections");

            bray_linker::SystemLinkerFamily::GnuCompiler
        }
        ObjectFormat::MachO => {
            command.arg("-Xlinker").arg("-dead_strip");

            bray_linker::SystemLinkerFamily::AppleCompiler
        }
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
            return Err(BuildError::conformance(
                "native provider retention",
                format!("unsupported native object format: {}", format.as_str()),
            ));
        }
    };

    let output = bray_linker::SystemLinkerMapOutput::try_new(map)
        .ok_or_else(|| BuildError::conformance("native provider retention", "invalid map path"))?;

    command.args(output.arguments(family, None));

    Ok(())
}

fn require_members(
    map: &str,
    required: &[&str],
    forbidden: &[&str],
    fixture: &str,
) -> Result<(), BuildError> {
    let map = map.to_ascii_lowercase();

    for member in required {
        if !map.contains(member) {
            return Err(retention_error(fixture, "did not retain", member));
        }
    }

    for member in forbidden {
        if map.contains(member) {
            return Err(retention_error(fixture, "retained unrelated", member));
        }
    }

    Ok(())
}

fn retention_error(fixture: &str, behavior: &str, member: &str) -> BuildError {
    BuildError::conformance(
        "native provider retention",
        format!("{fixture} fixture {behavior} provider member {member}"),
    )
}
