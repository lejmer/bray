use std::ffi::{OsStr, OsString};
use std::path::Path;

use bray_target::{RelocationModel, TargetArchitecture};

use crate::LldFlavor;
use crate::SystemLinkerFamily;
use crate::{
    DeadStripPolicy, DebugLinkPolicy, LinkInput, LinkInputMode, LinkInputSource, LinkModel,
    LinkPlan, LinkSearchPathKind, LinkSubsystem, LinkTimeOptimizationPolicy, LinkedArtifactKind,
    LinkedProductKind, PlannedLinkedArtifact, SectionGarbageCollectionPolicy,
};

pub(super) fn system_arguments_for(
    plan: &LinkPlan,
    family: SystemLinkerFamily,
    current_directory: Option<&Path>,
) -> Result<Vec<OsString>, LldPlanError> {
    match family {
        SystemLinkerFamily::Gnu | SystemLinkerFamily::Microsoft | SystemLinkerFamily::Apple => {
            arguments_for_with_directory(plan, family.flavor(), current_directory)
        }
        SystemLinkerFamily::GnuCompiler => gnu_compiler_arguments(plan, false, current_directory),
        SystemLinkerFamily::WslGnuCompiler => gnu_compiler_arguments(plan, true, current_directory),
        SystemLinkerFamily::MicrosoftCompiler => {
            microsoft_compiler_arguments(plan, current_directory)
        }
        SystemLinkerFamily::AppleCompiler => apple_compiler_arguments(plan, current_directory),
    }
}

fn gnu_compiler_arguments(
    plan: &LinkPlan,
    through_wsl: bool,
    current_directory: Option<&Path>,
) -> Result<Vec<OsString>, LldPlanError> {
    let raw = arguments_for_with_directory(plan, LldFlavor::Elf, current_directory)?;
    let mut arguments = Vec::with_capacity(raw.len() + 4);

    if through_wsl {
        arguments.extend(["--exec".into(), "cc".into()]);
    }

    arguments.push("-pthread".into());

    if matches!(
        plan.policy().optimization(),
        LinkTimeOptimizationPolicy::ThinLto { .. }
    ) {
        arguments.push("-fuse-ld=lld".into());
        arguments.push("-flto=thin".into());
    }

    for argument in raw {
        let Some(argument) = argument.to_str() else {
            return Err(LldPlanError::NonUnicodeArgument);
        };

        if argument == "--entry=main" {
            continue;
        }

        let transformed = match argument {
            "--static" => OsString::from("-static"),
            "--pie" => OsString::from("-pie"),
            "--shared" => OsString::from("-shared"),
            argument if argument.starts_with("--") => OsString::from(format!("-Wl,{argument}")),
            argument if through_wsl => OsString::from(wsl_path(argument)),
            argument => OsString::from(argument),
        };

        arguments.push(transformed);
    }

    Ok(arguments)
}

pub(crate) fn wsl_path(argument: &str) -> String {
    let bytes = argument.as_bytes();

    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
    {
        return argument.replace('\\', "/");
    }

    let drive = char::from(bytes[0]).to_ascii_lowercase();
    let suffix = argument[3..].replace('\\', "/");

    format!("/mnt/{drive}/{suffix}")
}

fn microsoft_compiler_arguments(
    plan: &LinkPlan,
    current_directory: Option<&Path>,
) -> Result<Vec<OsString>, LldPlanError> {
    let raw = arguments_for_with_directory(plan, LldFlavor::Coff, current_directory)?;
    let mut arguments = Vec::with_capacity(raw.len() * 2 + 2);

    arguments.push(format!("--target={}", plan.target().triple()).into());
    arguments.push("-fuse-ld=lld".into());

    if matches!(
        plan.policy().optimization(),
        LinkTimeOptimizationPolicy::ThinLto { .. }
    ) {
        arguments.push("-flto=thin".into());
    }

    for argument in raw {
        let Some(text) = argument.to_str() else {
            return Err(LldPlanError::NonUnicodeArgument);
        };

        if text.starts_with("/entry:") {
            continue;
        }

        if let Some(output) = text.strip_prefix("/out:") {
            arguments.push("-o".into());
            arguments.push(output.into());
        } else if text == "/dll" {
            arguments.push("-shared".into());
        } else if is_ordinary_file_input(plan, &argument, current_directory) {
            arguments.push(argument);
        } else {
            arguments.push("-Xlinker".into());
            arguments.push(argument);
        }
    }

    Ok(arguments)
}

fn is_ordinary_file_input(
    plan: &LinkPlan,
    argument: &OsStr,
    current_directory: Option<&Path>,
) -> bool {
    plan.inputs().iter().any(|input| {
        input.mode() == LinkInputMode::Ordinary
            && matches!(
                input.source(),
                LinkInputSource::File(path)
                    if linker_visible_path(path, current_directory).as_os_str() == argument
            )
    })
}

fn apple_compiler_arguments(
    plan: &LinkPlan,
    current_directory: Option<&Path>,
) -> Result<Vec<OsString>, LldPlanError> {
    let raw = arguments_for_with_directory(plan, LldFlavor::MachO, current_directory)?;
    let mut arguments = Vec::with_capacity(raw.len());
    let mut raw = raw.into_iter();

    if matches!(
        plan.policy().optimization(),
        LinkTimeOptimizationPolicy::ThinLto { .. }
    ) {
        arguments.push("-fuse-ld=lld".into());
        arguments.push("-flto=thin".into());
    }

    while let Some(argument) = raw.next() {
        let Some(text) = argument.to_str() else {
            return Err(LldPlanError::NonUnicodeArgument);
        };

        match text {
            "-e" => {
                raw.next().ok_or(LldPlanError::MissingArgumentValue)?;
            }
            "-dylib" => arguments.push("-dynamiclib".into()),
            "-no_uuid" | "-dead_strip" => {
                arguments.push(OsString::from(format!("-Wl,{text}")));
            }
            text if text.starts_with("--thinlto-") => {
                arguments.push(OsString::from(format!("-Wl,{text}")));
            }
            "-exported_symbol" | "-u" | "-force_load" => {
                let value = raw.next().ok_or(LldPlanError::MissingArgumentValue)?;

                arguments.push(OsString::from(format!("-Wl,{text}")));
                arguments.push(prefixed("-Wl,", value));
            }
            _ => arguments.push(argument),
        }
    }

    Ok(arguments)
}

pub(super) fn arguments_for(
    plan: &LinkPlan,
    flavor: LldFlavor,
) -> Result<Vec<OsString>, LldPlanError> {
    arguments_for_with_directory(plan, flavor, None)
}

fn arguments_for_with_directory(
    plan: &LinkPlan,
    flavor: LldFlavor,
    current_directory: Option<&Path>,
) -> Result<Vec<OsString>, LldPlanError> {
    let mut arguments = Vec::new();

    push_determinism_arguments(&mut arguments, flavor);
    push_target_arguments(&mut arguments, plan, flavor)?;
    push_output_arguments(&mut arguments, plan, flavor, current_directory)?;
    push_policy_arguments(&mut arguments, plan, flavor)?;
    push_symbol_arguments(&mut arguments, plan, flavor);
    push_search_paths(&mut arguments, plan, flavor, current_directory)?;

    for input in plan.inputs() {
        push_input(&mut arguments, input, flavor, current_directory)?;
    }

    Ok(arguments)
}

fn push_determinism_arguments(arguments: &mut Vec<OsString>, flavor: LldFlavor) {
    match flavor {
        LldFlavor::Elf => arguments.push("--build-id=none".into()),
        LldFlavor::Coff => arguments.push("/brepro".into()),
        LldFlavor::MachO => arguments.push("-no_uuid".into()),
    }
}

fn push_target_arguments(
    arguments: &mut Vec<OsString>,
    plan: &LinkPlan,
    flavor: LldFlavor,
) -> Result<(), LldPlanError> {
    match plan.product_kind() {
        LinkedProductKind::Executable => {
            if flavor == LldFlavor::Elf
                && plan.target().relocation_model() == RelocationModel::PositionIndependent
            {
                arguments.push("--pie".into());
            }
        }
        LinkedProductKind::SharedLibrary => arguments.push(match flavor {
            LldFlavor::Elf => "--shared".into(),
            LldFlavor::Coff => "/dll".into(),
            LldFlavor::MachO => "-dylib".into(),
        }),
        LinkedProductKind::StaticLibrary => return Err(LldPlanError::UnsupportedProduct),
    }

    if plan.target().link_model() == LinkModel::Static {
        match flavor {
            LldFlavor::Elf => arguments.push("--static".into()),
            LldFlavor::Coff => {}
            LldFlavor::MachO => return Err(LldPlanError::UnsupportedLinkModel),
        }
    }

    match flavor {
        LldFlavor::Elf => {}
        LldFlavor::Coff => {
            arguments.push(prefixed(
                "/machine:",
                coff_machine(plan.target().architecture())?,
            ));

            if plan.product_kind() == LinkedProductKind::Executable {
                arguments.push("/noimplib".into());
            }
        }
        LldFlavor::MachO => {
            arguments.push("-arch".into());
            arguments.push(macho_architecture(plan.target().architecture())?.into());
        }
    }

    push_subsystem(arguments, plan, flavor)
}

fn push_subsystem(
    arguments: &mut Vec<OsString>,
    plan: &LinkPlan,
    flavor: LldFlavor,
) -> Result<(), LldPlanError> {
    let Some(subsystem) = plan.policy().subsystem() else {
        return Ok(());
    };

    match (flavor, subsystem) {
        (LldFlavor::Coff, LinkSubsystem::Console) => {
            arguments.push("/subsystem:console".into());
        }
        (LldFlavor::Coff, LinkSubsystem::Windowed) => {
            arguments.push("/subsystem:windows".into());
        }
        (LldFlavor::Coff, LinkSubsystem::Native) => {
            arguments.push("/subsystem:native".into());
        }
        (LldFlavor::Elf | LldFlavor::MachO, LinkSubsystem::Console) => {}
        (
            LldFlavor::Elf | LldFlavor::Coff | LldFlavor::MachO,
            LinkSubsystem::WasiCommand
            | LinkSubsystem::WasiReactor
            | LinkSubsystem::Windowed
            | LinkSubsystem::Native,
        ) => return Err(LldPlanError::UnsupportedSubsystem),
    }

    Ok(())
}

fn push_output_arguments(
    arguments: &mut Vec<OsString>,
    plan: &LinkPlan,
    flavor: LldFlavor,
    current_directory: Option<&Path>,
) -> Result<(), LldPlanError> {
    let Some(primary) = plan.primary_output() else {
        return Err(LldPlanError::MissingPrimaryOutput);
    };

    push_output_path(
        arguments,
        flavor,
        linker_visible_path(primary.destination().path(), current_directory),
    );

    for output in plan.outputs() {
        if output.destination().id() == primary.destination().id() {
            continue;
        }

        push_companion_output(arguments, output, flavor, current_directory)?;
    }

    Ok(())
}

fn push_output_path(arguments: &mut Vec<OsString>, flavor: LldFlavor, path: &Path) {
    match flavor {
        LldFlavor::Elf | LldFlavor::MachO => {
            arguments.push("-o".into());
            arguments.push(path.as_os_str().to_owned());
        }
        LldFlavor::Coff => arguments.push(prefixed("/out:", path)),
    }
}

fn push_companion_output(
    arguments: &mut Vec<OsString>,
    output: &PlannedLinkedArtifact,
    flavor: LldFlavor,
    current_directory: Option<&Path>,
) -> Result<(), LldPlanError> {
    let path = linker_visible_path(output.destination().path(), current_directory);

    match (flavor, output.kind()) {
        (LldFlavor::Coff, LinkedArtifactKind::ImportLibrary) => {
            arguments.push(prefixed("/implib:", path));
        }
        (LldFlavor::Coff, LinkedArtifactKind::DebugCompanion) => {
            arguments.push("/debug".into());
            arguments.push(prefixed("/pdb:", path));
            arguments.push("/pdbaltpath:%_PDB%".into());
            arguments.push(r"/pdbsourcepath:X:\bray".into());
        }
        (
            LldFlavor::Elf | LldFlavor::Coff | LldFlavor::MachO,
            LinkedArtifactKind::Executable
            | LinkedArtifactKind::SharedLibrary
            | LinkedArtifactKind::StaticLibrary
            | LinkedArtifactKind::ImportLibrary
            | LinkedArtifactKind::DebugCompanion
            | LinkedArtifactKind::PlatformCompanion,
        ) => return Err(LldPlanError::UnsupportedCompanionOutput),
    }

    Ok(())
}

fn push_policy_arguments(
    arguments: &mut Vec<OsString>,
    plan: &LinkPlan,
    flavor: LldFlavor,
) -> Result<(), LldPlanError> {
    let policy = plan.policy();

    let remove_unreachable = policy.dead_strip() == DeadStripPolicy::RemoveUnreachable
        || policy.section_garbage_collection()
            == SectionGarbageCollectionPolicy::RemoveUnreferenced;

    if remove_unreachable {
        arguments.push(match flavor {
            LldFlavor::Elf => "--gc-sections".into(),
            LldFlavor::Coff => "/opt:ref".into(),
            LldFlavor::MachO => "-dead_strip".into(),
        });
    } else if flavor == LldFlavor::Coff {
        arguments.push("/opt:noref".into());
    }

    match (flavor, policy.debug()) {
        (LldFlavor::Coff, DebugLinkPolicy::None | DebugLinkPolicy::Companion)
        | (LldFlavor::Elf | LldFlavor::MachO, DebugLinkPolicy::None | DebugLinkPolicy::Embedded) => {
        }
        (LldFlavor::Elf | LldFlavor::MachO, DebugLinkPolicy::Companion)
        | (LldFlavor::Coff, DebugLinkPolicy::Embedded) => {
            return Err(LldPlanError::UnsupportedDebugPolicy);
        }
    }

    if let LinkTimeOptimizationPolicy::ThinLto { jobs } = policy.optimization() {
        arguments.push(match flavor {
            LldFlavor::Elf => format!("--thinlto-jobs={jobs}").into(),
            LldFlavor::Coff => format!("/opt:lldltojobs={jobs}").into(),
            LldFlavor::MachO => format!("--thinlto-jobs={jobs}").into(),
        });
    }

    Ok(())
}

fn push_symbol_arguments(arguments: &mut Vec<OsString>, plan: &LinkPlan, flavor: LldFlavor) {
    if let Some(entry_point) = plan.entry_point() {
        push_symbol(
            arguments,
            flavor,
            SymbolArgument::Entry,
            entry_point.as_str(),
        );
    }

    for symbol in plan.exported_symbols() {
        push_symbol(arguments, flavor, SymbolArgument::Export, symbol.as_str());
    }

    for symbol in plan.retained_symbols() {
        push_symbol(arguments, flavor, SymbolArgument::Retain, symbol.as_str());
    }
}

fn push_symbol(
    arguments: &mut Vec<OsString>,
    flavor: LldFlavor,
    kind: SymbolArgument,
    symbol: &str,
) {
    match (flavor, kind) {
        (LldFlavor::Elf, SymbolArgument::Entry) => {
            arguments.push(prefixed("--entry=", symbol));
        }
        (LldFlavor::Elf, SymbolArgument::Export) => {
            arguments.push(prefixed("--export-dynamic-symbol=", symbol));
        }
        (LldFlavor::Elf, SymbolArgument::Retain) => {
            arguments.push(prefixed("--undefined=", symbol));
        }
        (LldFlavor::Coff, SymbolArgument::Entry) => {
            arguments.push(prefixed("/entry:", symbol));
        }
        (LldFlavor::Coff, SymbolArgument::Export) => {
            arguments.push(prefixed("/export:", symbol));
        }
        (LldFlavor::Coff, SymbolArgument::Retain) => {
            arguments.push(prefixed("/include:", symbol));
        }
        (LldFlavor::MachO, SymbolArgument::Entry) => {
            arguments.push("-e".into());
            arguments.push(macho_symbol(symbol));
        }
        (LldFlavor::MachO, SymbolArgument::Export) => {
            arguments.push("-exported_symbol".into());
            arguments.push(macho_symbol(symbol));
        }
        (LldFlavor::MachO, SymbolArgument::Retain) => {
            arguments.push("-u".into());
            arguments.push(macho_symbol(symbol));
        }
    }
}

fn macho_symbol(symbol: &str) -> OsString {
    prefixed("_", symbol)
}

fn push_search_paths(
    arguments: &mut Vec<OsString>,
    plan: &LinkPlan,
    flavor: LldFlavor,
    current_directory: Option<&Path>,
) -> Result<(), LldPlanError> {
    for search_path in plan.search_paths() {
        let path = linker_visible_path(search_path.path(), current_directory);

        match (flavor, search_path.kind()) {
            (LldFlavor::Elf | LldFlavor::MachO, LinkSearchPathKind::Library) => {
                arguments.push("-L".into());
                arguments.push(path.as_os_str().to_owned());
            }
            (LldFlavor::Coff, LinkSearchPathKind::Library) => {
                arguments.push(prefixed("/libpath:", path));
            }
            (LldFlavor::MachO, LinkSearchPathKind::Framework) => {
                arguments.push("-F".into());
                arguments.push(path.as_os_str().to_owned());
            }
            (LldFlavor::Elf | LldFlavor::Coff, LinkSearchPathKind::Framework) => {
                return Err(LldPlanError::UnsupportedFramework);
            }
        }
    }

    Ok(())
}

fn push_input(
    arguments: &mut Vec<OsString>,
    input: &LinkInput,
    flavor: LldFlavor,
    current_directory: Option<&Path>,
) -> Result<(), LldPlanError> {
    match input.source() {
        LinkInputSource::File(path) => {
            let path = linker_visible_path(path, current_directory);

            if input.mode() == LinkInputMode::WholeArchive {
                push_whole_archive(arguments, path, flavor);
            } else {
                arguments.push(path.as_os_str().to_owned());
            }
        }
        LinkInputSource::NativeLibrary(name) => match flavor {
            LldFlavor::Elf | LldFlavor::MachO => {
                arguments.push(prefixed("-l", name.as_str()));
            }
            LldFlavor::Coff => {
                arguments.push(prefixed("/defaultlib:", name.as_str()));
            }
        },
        LinkInputSource::Framework(name) => {
            if flavor != LldFlavor::MachO {
                return Err(LldPlanError::UnsupportedFramework);
            }

            arguments.push("-framework".into());
            arguments.push(name.as_str().into());
        }
    }

    Ok(())
}

pub(super) fn linker_visible_path<'path>(
    path: &'path Path,
    current_directory: Option<&Path>,
) -> &'path Path {
    let Some(current_directory) = current_directory else {
        return path;
    };

    path.strip_prefix(current_directory)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or(path)
}

fn push_whole_archive(arguments: &mut Vec<OsString>, path: &Path, flavor: LldFlavor) {
    match flavor {
        LldFlavor::Elf => {
            arguments.push("--whole-archive".into());
            arguments.push(path.as_os_str().to_owned());
            arguments.push("--no-whole-archive".into());
        }
        LldFlavor::Coff => arguments.push(prefixed("/wholearchive:", path)),
        LldFlavor::MachO => {
            arguments.push("-force_load".into());
            arguments.push(path.as_os_str().to_owned());
        }
    }
}

fn coff_machine(architecture: TargetArchitecture) -> Result<&'static str, LldPlanError> {
    match architecture {
        TargetArchitecture::X86 => Ok("x86"),
        TargetArchitecture::X86_64 => Ok("x64"),
        TargetArchitecture::Arm => Ok("arm"),
        TargetArchitecture::Aarch64 => Ok("arm64"),
        TargetArchitecture::Riscv32
        | TargetArchitecture::Riscv64
        | TargetArchitecture::PowerPc64
        | TargetArchitecture::Wasm32
        | TargetArchitecture::Wasm64 => Err(LldPlanError::UnsupportedArchitecture),
    }
}

fn macho_architecture(architecture: TargetArchitecture) -> Result<&'static str, LldPlanError> {
    match architecture {
        TargetArchitecture::X86_64 => Ok("x86_64"),
        TargetArchitecture::Aarch64 => Ok("arm64"),
        TargetArchitecture::X86
        | TargetArchitecture::Arm
        | TargetArchitecture::Riscv32
        | TargetArchitecture::Riscv64
        | TargetArchitecture::PowerPc64
        | TargetArchitecture::Wasm32
        | TargetArchitecture::Wasm64 => Err(LldPlanError::UnsupportedArchitecture),
    }
}

fn prefixed(prefix: &str, value: impl AsRef<OsStr>) -> OsString {
    let mut argument = OsString::from(prefix);

    argument.push(value);

    argument
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LldPlanError {
    MissingPrimaryOutput,
    UnsupportedArchitecture,
    UnsupportedCompanionOutput,
    UnsupportedDebugPolicy,
    UnsupportedFramework,
    UnsupportedLinkModel,
    MissingArgumentValue,
    NonUnicodeArgument,
    UnsupportedProduct,
    UnsupportedSubsystem,
}

#[derive(Clone, Copy)]
enum SymbolArgument {
    Entry,
    Export,
    Retain,
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::num::NonZeroUsize;

    use bray_runtime_interface::{BinarySymbolName, RuntimeArtifactId};
    use bray_target::{
        CodeModel, NativeTarget, ObjectFormat, RelocationModel, TargetArchitecture, TargetIdentity,
    };

    use super::{LldFlavor, arguments_for, system_arguments_for};
    use crate::test_support::{link_input, link_plan_builder, planned_output, product};
    use crate::{
        DebugLinkPolicy, LinkInput, LinkInputId, LinkInputKind, LinkInputMode, LinkInputProvenance,
        LinkInputSource, LinkModel, LinkPlan, LinkPlanBuilder, LinkPolicy, LinkSearchPath,
        LinkSearchPathKind, LinkTarget, LinkTimeOptimizationPolicy, LinkedArtifactKind,
        LinkedArtifactRequirement, LinkedProductKind, LinkerDriverIdentity, LinkerDriverKind,
        SystemLinkerFamily,
    };

    #[test]
    fn elf_arguments_preserve_deterministic_plan_order() {
        let mut builder = link_plan_builder();

        builder.push_input(link_input(0, "first.o"));
        builder.push_input(link_input(1, "second.o"));

        builder.push_input(
            LinkInput::try_native_library(
                LinkInputId::new(2),
                "pthread",
                LinkInputProvenance::HostConfiguration,
            )
            .unwrap_or_else(|| panic!("test native library name must be valid")),
        );

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            "application.stage",
        ));

        builder.set_executable_host(crate::test_support::executable_host_contract());
        builder.push_exported_symbol(symbol("bray_export"));
        builder.push_retained_symbol(symbol("bray_keep"));

        builder.push_search_path(
            LinkSearchPath::try_new(LinkSearchPathKind::Library, "native/lib")
                .unwrap_or_else(|error| panic!("test search path must be valid: {error:?}")),
        );

        let plan = builder
            .finish()
            .unwrap_or_else(|error| panic!("test link plan must be valid: {error:?}"));

        assert_eq!(
            arguments_for(&plan, LldFlavor::Elf),
            Ok(vec![
                OsString::from("--build-id=none"),
                OsString::from("--pie"),
                OsString::from("-o"),
                OsString::from("application.stage"),
                OsString::from("--entry=_bray_host_start"),
                OsString::from("--export-dynamic-symbol=bray_export"),
                OsString::from("--undefined=bray_keep"),
                OsString::from("-L"),
                OsString::from("native/lib"),
                OsString::from("first.o"),
                OsString::from("second.o"),
                OsString::from("-lpthread"),
            ])
        );
    }

    #[test]
    fn runtime_dependency_archives_reach_linker_arguments_after_their_owners() {
        let runtime = RuntimeArtifactId::try_new("runtime.test")
            .unwrap_or_else(|| panic!("test runtime identity must be valid"));

        let mut builder = link_plan_builder();

        for (ordinal, path) in ["runtime-owner.lib", "runtime-support.lib"]
            .into_iter()
            .enumerate()
        {
            builder.push_input(
                LinkInput::try_new(
                    LinkInputId::new(ordinal as u32),
                    LinkInputKind::RuntimeComponent,
                    LinkInputSource::file(path),
                    LinkInputProvenance::Runtime(runtime.clone()),
                    LinkInputMode::Ordinary,
                )
                .unwrap_or_else(|error| panic!("runtime input must be valid: {error:?}")),
            );
        }

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            "application.stage",
        ));

        builder.set_executable_host(crate::test_support::async_executable_host_contract(runtime));

        let plan = builder
            .finish()
            .unwrap_or_else(|error| panic!("runtime link plan must be valid: {error:?}"));

        let arguments = arguments_for(&plan, LldFlavor::Coff)
            .unwrap_or_else(|error| panic!("runtime linker arguments must be valid: {error:?}"));

        let owner = arguments
            .iter()
            .position(|argument| argument == "runtime-owner.lib")
            .unwrap_or_else(|| panic!("runtime owner must reach linker arguments"));

        let support = arguments
            .iter()
            .position(|argument| argument == "runtime-support.lib")
            .unwrap_or_else(|| panic!("runtime support must reach linker arguments"));

        assert!(owner < support);
    }

    #[test]
    fn coff_and_macho_arguments_use_target_specific_command_languages() {
        let cases = [
            (
                LldFlavor::Coff,
                shared_library_plan(
                    TargetArchitecture::X86_64,
                    ObjectFormat::Coff,
                    "library.dll",
                    Some(("library.lib", LinkedArtifactKind::ImportLibrary)),
                ),
                vec![
                    OsString::from("/brepro"),
                    OsString::from("/dll"),
                    OsString::from("/machine:x64"),
                    OsString::from("/out:library.dll"),
                    OsString::from("/implib:library.lib"),
                    OsString::from("/opt:noref"),
                    OsString::from("crt/start.o"),
                    OsString::from("member.o"),
                ],
            ),
            (
                LldFlavor::MachO,
                shared_library_plan(
                    TargetArchitecture::Aarch64,
                    ObjectFormat::MachO,
                    "library.dylib",
                    None,
                ),
                vec![
                    OsString::from("-no_uuid"),
                    OsString::from("-dylib"),
                    OsString::from("-arch"),
                    OsString::from("arm64"),
                    OsString::from("-o"),
                    OsString::from("library.dylib"),
                    OsString::from("crt/start.o"),
                    OsString::from("member.o"),
                ],
            ),
        ];

        for (flavor, plan, expected) in cases {
            assert_eq!(arguments_for(&plan, flavor), Ok(expected));
        }
    }

    #[test]
    fn coff_debug_companions_embed_output_independent_pdb_references() {
        let plan = shared_library_plan(
            TargetArchitecture::X86_64,
            ObjectFormat::Coff,
            "staging/application.dll",
            Some((
                "staging/application.pdb",
                LinkedArtifactKind::DebugCompanion,
            )),
        );

        let arguments = arguments_for(&plan, LldFlavor::Coff)
            .unwrap_or_else(|error| panic!("COFF debug arguments must be valid: {error:?}"));

        let debug = arguments
            .iter()
            .position(|argument| argument == "/debug")
            .unwrap_or_else(|| panic!("COFF debug arguments must request linked debug data"));

        assert_eq!(
            &arguments[debug..debug + 4],
            [
                OsString::from("/debug"),
                OsString::from("/pdb:staging/application.pdb"),
                OsString::from("/pdbaltpath:%_PDB%"),
                OsString::from(r"/pdbsourcepath:X:\bray"),
            ]
        );

        let stable_arguments = system_arguments_for(
            &plan,
            SystemLinkerFamily::MicrosoftCompiler,
            Some(std::path::Path::new("staging")),
        )
        .unwrap_or_else(|error| panic!("COFF compiler arguments must be valid: {error:?}"));

        assert!(stable_arguments.contains(&OsString::from("application.dll")));
        assert!(stable_arguments.contains(&OsString::from("/pdb:application.pdb")));

        assert!(
            !stable_arguments
                .iter()
                .any(|argument| { argument.to_string_lossy().contains("staging/application") })
        );
    }

    #[test]
    fn native_compiler_drivers_preserve_typed_link_plans() {
        let cases = [
            (
                SystemLinkerFamily::MicrosoftCompiler,
                executable_plan(
                    TargetArchitecture::X86_64,
                    ObjectFormat::Coff,
                    "application.exe",
                    "main.obj",
                ),
                vec![
                    OsString::from("--target=test-target-triple"),
                    OsString::from("-fuse-ld=lld"),
                    OsString::from("-Xlinker"),
                    OsString::from("/brepro"),
                    OsString::from("-Xlinker"),
                    OsString::from("/machine:x64"),
                    OsString::from("-Xlinker"),
                    OsString::from("/noimplib"),
                    OsString::from("-o"),
                    OsString::from("application.exe"),
                    OsString::from("-Xlinker"),
                    OsString::from("/opt:noref"),
                    OsString::from("main.obj"),
                ],
            ),
            (
                SystemLinkerFamily::AppleCompiler,
                executable_plan(
                    TargetArchitecture::Aarch64,
                    ObjectFormat::MachO,
                    "application",
                    "main.o",
                ),
                vec![
                    OsString::from("-Wl,-no_uuid"),
                    OsString::from("-arch"),
                    OsString::from("arm64"),
                    OsString::from("-o"),
                    OsString::from("application"),
                    OsString::from("main.o"),
                ],
            ),
        ];

        for (family, plan, expected) in cases {
            assert_eq!(system_arguments_for(&plan, family, None), Ok(expected));

            assert_eq!(
                system_arguments_for(&plan, family, None),
                system_arguments_for(&plan, family, None)
            );
        }
    }

    #[test]
    fn thin_lto_uses_each_linker_backend_job_contract() {
        let jobs =
            NonZeroUsize::new(3).unwrap_or_else(|| panic!("test worker budget must be nonzero"));

        let cases = [
            (
                TargetArchitecture::X86_64,
                ObjectFormat::Elf,
                LldFlavor::Elf,
                SystemLinkerFamily::GnuCompiler,
                "--thinlto-jobs=3",
                "-Wl,--thinlto-jobs=3",
            ),
            (
                TargetArchitecture::X86_64,
                ObjectFormat::Coff,
                LldFlavor::Coff,
                SystemLinkerFamily::MicrosoftCompiler,
                "/opt:lldltojobs=3",
                "/opt:lldltojobs=3",
            ),
            (
                TargetArchitecture::Aarch64,
                ObjectFormat::MachO,
                LldFlavor::MachO,
                SystemLinkerFamily::AppleCompiler,
                "--thinlto-jobs=3",
                "-Wl,--thinlto-jobs=3",
            ),
        ];

        for (architecture, format, flavor, family, raw_job, compiler_job) in cases {
            let plan = executable_plan_with_startup(
                architecture,
                format,
                "application",
                "main.bc",
                crate::LinkStartupMode::PlatformCompilerDriver,
                LinkTimeOptimizationPolicy::ThinLto { jobs },
            );

            let raw = arguments_for(&plan, flavor)
                .unwrap_or_else(|error| panic!("ThinLTO plan must be valid: {error:?}"));

            assert!(raw.contains(&OsString::from(raw_job)), "{family:?}");

            let compiler = system_arguments_for(&plan, family, None)
                .unwrap_or_else(|error| panic!("ThinLTO compiler plan must be valid: {error:?}"));

            assert!(
                compiler.contains(&OsString::from("-flto=thin")),
                "{family:?}"
            );

            assert!(
                compiler.contains(&OsString::from("-fuse-ld=lld")),
                "{family:?}"
            );

            assert!(
                compiler.contains(&OsString::from(compiler_job)),
                "{family:?}"
            );
        }
    }

    #[test]
    fn native_link_plans_cover_every_supported_platform_and_architecture() {
        for native in NativeTarget::ALL {
            let plan = native_executable_plan(native);

            let flavor = match native.object_format() {
                ObjectFormat::Coff => LldFlavor::Coff,
                ObjectFormat::Elf => LldFlavor::Elf,
                ObjectFormat::MachO => LldFlavor::MachO,
                ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
                    unreachable!("native target matrix excludes non-native object formats")
                }
            };

            let first = arguments_for(&plan, flavor)
                .unwrap_or_else(|error| panic!("native link plan must be valid: {error:?}"));

            let second = arguments_for(&plan, flavor)
                .unwrap_or_else(|error| panic!("native link plan must be valid: {error:?}"));

            assert_eq!(first, second, "{}", native.as_str());
            assert!(!first.is_empty(), "{}", native.as_str());
        }
    }

    #[test]
    fn raw_macho_arguments_use_object_symbol_spelling() {
        let mut plan = executable_plan_with_startup(
            TargetArchitecture::X86_64,
            ObjectFormat::MachO,
            "application",
            "main.o",
            crate::LinkStartupMode::ExplicitInputs,
            LinkTimeOptimizationPolicy::None,
        );

        let mut builder = LinkPlanBuilder::new(
            plan.product().clone(),
            plan.product_kind(),
            plan.target().clone(),
            plan.driver().clone(),
            plan.startup_mode(),
            plan.policy(),
        );

        for input in plan.inputs() {
            builder.push_input(input.clone());
        }

        for output in plan.outputs() {
            builder.push_output(output.clone());
        }

        builder.set_executable_host(
            plan.executable_host()
                .cloned()
                .unwrap_or_else(|| panic!("test executable plan must retain its host")),
        );

        builder.push_exported_symbol(symbol("bray_export"));

        plan = builder
            .finish()
            .unwrap_or_else(|error| panic!("test Mach-O plan must be valid: {error:?}"));

        let arguments = arguments_for(&plan, LldFlavor::MachO)
            .unwrap_or_else(|error| panic!("test Mach-O arguments must be valid: {error:?}"));

        assert!(arguments.contains(&OsString::from("__bray_host_start")));
        assert!(arguments.contains(&OsString::from("_bray_export")));
    }

    fn symbol(name: &str) -> BinarySymbolName {
        BinarySymbolName::try_new(name)
            .unwrap_or_else(|| panic!("test binary symbol name must be valid"))
    }

    fn shared_library_plan(
        architecture: TargetArchitecture,
        object_format: ObjectFormat,
        output: &str,
        companion: Option<(&str, LinkedArtifactKind)>,
    ) -> LinkPlan {
        let identity = TargetIdentity::try_new("test-target")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        let target = LinkTarget::try_new(
            identity,
            "test-target-triple",
            architecture,
            object_format,
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            LinkModel::Dynamic,
        )
        .unwrap_or_else(|error| panic!("test link target must be valid: {error:?}"));

        let driver = LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "1", "20")
            .unwrap_or_else(|| panic!("test linker identity must be valid"));

        let debug = if matches!(companion, Some((_, LinkedArtifactKind::DebugCompanion))) {
            DebugLinkPolicy::Companion
        } else {
            DebugLinkPolicy::None
        };

        let mut builder = LinkPlanBuilder::new(
            product(),
            LinkedProductKind::SharedLibrary,
            target,
            driver,
            crate::LinkStartupMode::ExplicitInputs,
            LinkPolicy::new(
                crate::DeadStripPolicy::Preserve,
                crate::SectionGarbageCollectionPolicy::Preserve,
                debug,
                None,
            ),
        );

        builder.push_input(startup_input(1, "crt/start.o"));

        builder.push_input(link_input(0, "member.o"));

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::SharedLibrary,
            LinkedArtifactRequirement::Required,
            output,
        ));

        if let Some((path, kind)) = companion {
            builder.push_output(planned_output(
                1,
                kind,
                LinkedArtifactRequirement::Required,
                path,
            ));
        }

        builder
            .finish()
            .unwrap_or_else(|error| panic!("test link plan must be valid: {error:?}"))
    }

    fn executable_plan(
        architecture: TargetArchitecture,
        object_format: ObjectFormat,
        output: &str,
        input: &str,
    ) -> LinkPlan {
        executable_plan_with_startup(
            architecture,
            object_format,
            output,
            input,
            crate::LinkStartupMode::PlatformCompilerDriver,
            LinkTimeOptimizationPolicy::None,
        )
    }

    fn executable_plan_with_startup(
        architecture: TargetArchitecture,
        object_format: ObjectFormat,
        output: &str,
        input: &str,
        startup_mode: crate::LinkStartupMode,
        optimization: LinkTimeOptimizationPolicy,
    ) -> LinkPlan {
        let identity = TargetIdentity::try_new("test-target")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        let target = LinkTarget::try_new(
            identity,
            "test-target-triple",
            architecture,
            object_format,
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            LinkModel::Dynamic,
        )
        .unwrap_or_else(|error| panic!("test link target must be valid: {error:?}"));

        let driver = LinkerDriverIdentity::try_new(LinkerDriverKind::System, "system", "1", "1")
            .unwrap_or_else(|| panic!("test linker identity must be valid"));

        let mut builder = LinkPlanBuilder::new(
            product(),
            LinkedProductKind::Executable,
            target.clone(),
            driver,
            startup_mode,
            LinkPolicy::new(
                crate::DeadStripPolicy::Preserve,
                crate::SectionGarbageCollectionPolicy::Preserve,
                crate::DebugLinkPolicy::None,
                None,
            )
            .with_optimization(optimization),
        );

        if startup_mode == crate::LinkStartupMode::ExplicitInputs {
            builder.push_input(startup_input(1, "crt/start.o"));
        }

        builder.push_input(link_input(0, input));

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            output,
        ));

        builder.set_executable_host(bray_testing::test_executable_host_contract_for(
            product(),
            target.identity().clone(),
        ));

        builder
            .finish()
            .unwrap_or_else(|error| panic!("test executable plan must be valid: {error:?}"))
    }

    fn native_executable_plan(native: NativeTarget) -> LinkPlan {
        let target = LinkTarget::try_new(
            native.identity(),
            native.as_str(),
            native.architecture(),
            native.object_format(),
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            if native.object_format() == ObjectFormat::MachO {
                LinkModel::Dynamic
            } else {
                LinkModel::Static
            },
        )
        .unwrap_or_else(|error| panic!("native link target must be valid: {error:?}"));

        let driver = LinkerDriverIdentity::try_new(LinkerDriverKind::System, "system", "1", "1")
            .unwrap_or_else(|| panic!("test linker identity must be valid"));

        let mut builder = LinkPlanBuilder::new(
            product(),
            LinkedProductKind::Executable,
            target.clone(),
            driver,
            crate::LinkStartupMode::ExplicitInputs,
            LinkPolicy::new(
                crate::DeadStripPolicy::Preserve,
                crate::SectionGarbageCollectionPolicy::Preserve,
                crate::DebugLinkPolicy::None,
                None,
            ),
        );

        builder.push_input(startup_input(1, "crt/start.o"));

        let object_name = bray_target::TargetOutputName::for_native(
            native.object_format(),
            bray_target::TargetOutputKind::RelocatableObject,
        )
        .file_name("main")
        .unwrap_or_else(|| panic!("native object name must be valid"));

        let executable_name = bray_target::TargetOutputName::for_native(
            native.object_format(),
            bray_target::TargetOutputKind::Executable,
        )
        .file_name("application")
        .unwrap_or_else(|| panic!("native executable name must be valid"));

        builder.push_input(link_input(0, &object_name));

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            &executable_name,
        ));

        builder.set_executable_host(bray_testing::test_executable_host_contract_for(
            product(),
            target.identity().clone(),
        ));

        builder
            .finish()
            .unwrap_or_else(|error| panic!("native executable plan must be valid: {error:?}"))
    }

    fn startup_input(ordinal: u32, path: &str) -> LinkInput {
        LinkInput::try_new(
            LinkInputId::new(ordinal),
            LinkInputKind::StartupObject,
            LinkInputSource::file(path),
            LinkInputProvenance::TargetProfile,
            LinkInputMode::Ordinary,
        )
        .unwrap_or_else(|error| panic!("test startup input must be valid: {error:?}"))
    }
}
