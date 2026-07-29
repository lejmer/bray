use std::ffi::OsString;
use std::path::Path;

use bray_target::{RelocationModel, TargetArchitecture};

use super::LldFlavor;
use crate::{
    DeadStripPolicy, DebugLinkPolicy, LinkInput, LinkInputMode, LinkInputSource, LinkModel,
    LinkPlan, LinkSearchPathKind, LinkSubsystem, LinkedArtifactKind, LinkedProductKind,
    PlannedLinkedArtifact, SectionGarbageCollectionPolicy,
};

pub(super) fn external_arguments(
    plan: &LinkPlan,
    flavor: LldFlavor,
) -> Result<Vec<OsString>, LldPlanError> {
    let mut arguments = vec![
        OsString::from("-flavor"),
        OsString::from(flavor.external_selector()),
    ];

    arguments.extend(arguments_for(plan, flavor)?);

    Ok(arguments)
}

pub(super) fn arguments_for(
    plan: &LinkPlan,
    flavor: LldFlavor,
) -> Result<Vec<OsString>, LldPlanError> {
    let mut arguments = Vec::new();

    push_determinism_arguments(&mut arguments, flavor);
    push_target_arguments(&mut arguments, plan, flavor)?;
    push_output_arguments(&mut arguments, plan, flavor)?;
    push_policy_arguments(&mut arguments, plan, flavor)?;
    push_symbol_arguments(&mut arguments, plan, flavor);
    push_search_paths(&mut arguments, plan, flavor)?;

    for input in plan.inputs() {
        push_input(&mut arguments, input, flavor)?;
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
) -> Result<(), LldPlanError> {
    let Some(primary) = plan
        .outputs()
        .iter()
        .find(|output| output.kind() == plan.product_kind().primary_artifact_kind())
    else {
        return Err(LldPlanError::MissingPrimaryOutput);
    };

    push_output_path(arguments, flavor, primary.destination().path());

    for output in plan.outputs() {
        if output.destination().id() == primary.destination().id() {
            continue;
        }

        push_companion_output(arguments, output, flavor)?;
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
) -> Result<(), LldPlanError> {
    match (flavor, output.kind()) {
        (LldFlavor::Coff, LinkedArtifactKind::ImportLibrary) => {
            arguments.push(prefixed("/implib:", output.destination().path()));
        }
        (LldFlavor::Coff, LinkedArtifactKind::DebugCompanion) => {
            arguments.push("/debug".into());
            arguments.push(prefixed("/pdb:", output.destination().path()));
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
            arguments.push(symbol.into());
        }
        (LldFlavor::MachO, SymbolArgument::Export) => {
            arguments.push("-exported_symbol".into());
            arguments.push(symbol.into());
        }
        (LldFlavor::MachO, SymbolArgument::Retain) => {
            arguments.push("-u".into());
            arguments.push(symbol.into());
        }
    }
}

fn push_search_paths(
    arguments: &mut Vec<OsString>,
    plan: &LinkPlan,
    flavor: LldFlavor,
) -> Result<(), LldPlanError> {
    for search_path in plan.search_paths() {
        match (flavor, search_path.kind()) {
            (LldFlavor::Elf | LldFlavor::MachO, LinkSearchPathKind::Library) => {
                arguments.push("-L".into());
                arguments.push(search_path.path().as_os_str().to_owned());
            }
            (LldFlavor::Coff, LinkSearchPathKind::Library) => {
                arguments.push(prefixed("/libpath:", search_path.path()));
            }
            (LldFlavor::MachO, LinkSearchPathKind::Framework) => {
                arguments.push("-F".into());
                arguments.push(search_path.path().as_os_str().to_owned());
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
) -> Result<(), LldPlanError> {
    match input.source() {
        LinkInputSource::File(path) => {
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

fn prefixed(prefix: &str, value: impl AsRef<std::ffi::OsStr>) -> OsString {
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
    use std::ffi::{OsStr, OsString};

    use bray_runtime_interface::BinarySymbolName;
    use bray_target::{
        CodeModel, ObjectFormat, RelocationModel, TargetArchitecture, TargetIdentity,
    };

    use super::{LldFlavor, arguments_for, external_arguments};
    use crate::test_support::{link_input, link_plan_builder, planned_output, product};
    use crate::{
        LinkInput, LinkInputId, LinkInputKind, LinkInputMode, LinkInputProvenance, LinkInputSource,
        LinkModel, LinkPlan, LinkPlanBuilder, LinkPolicy, LinkSearchPath, LinkSearchPathKind,
        LinkTarget, LinkedArtifactKind, LinkedArtifactRequirement, LinkedProductKind,
        LinkerDriverIdentity, LinkerDriverKind,
    };

    #[test]
    fn elf_arguments_preserve_deterministic_plan_order() {
        let mut builder = link_plan_builder();

        builder.push_input(link_input(0, "first.o"));
        builder.push_input(link_input(1, "second.o"));
        builder.push_input(native_library(2, "pthread"));

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
    fn external_arguments_select_the_exact_lld_flavor() {
        let plan = crate::test_support::link_plan();

        let arguments = external_arguments(&plan, LldFlavor::Elf)
            .unwrap_or_else(|error| panic!("test plan must translate: {error:?}"));

        assert_eq!(arguments[0], OsStr::new("-flavor"));
        assert_eq!(arguments[1], OsStr::new("gnu"));
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
                    OsString::from("member.o"),
                ],
            ),
        ];

        for (flavor, plan, expected) in cases {
            assert_eq!(arguments_for(&plan, flavor), Ok(expected));
        }
    }

    fn native_library(ordinal: u32, name: &str) -> LinkInput {
        LinkInput::try_new(
            LinkInputId::new(ordinal),
            LinkInputKind::NativeLibrary,
            LinkInputSource::try_native_library(name)
                .unwrap_or_else(|| panic!("test native library name must be valid")),
            LinkInputProvenance::HostConfiguration,
            LinkInputMode::Ordinary,
        )
        .unwrap_or_else(|error| panic!("test native library input must be valid: {error:?}"))
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

        let mut builder = LinkPlanBuilder::new(
            product(),
            LinkedProductKind::SharedLibrary,
            target,
            driver,
            LinkPolicy::new(
                crate::DeadStripPolicy::Preserve,
                crate::SectionGarbageCollectionPolicy::Preserve,
                crate::DebugLinkPolicy::None,
                None,
            ),
        );

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
}
