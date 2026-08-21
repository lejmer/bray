use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact native link-requirement argument.
    pub fn link_requirement(requirement: DiagnosticLinkRequirement) -> Self {
        Self::new(
            DiagnosticArgName::LinkRequirement,
            DiagnosticArgValue::LinkRequirement(requirement),
        )
    }
}

/// Locale-neutral native link requirement with category-correct values.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLinkRequirement {
    Target(String),
    ProductExecutable,
    ProductSharedLibrary,
    ProductStaticLibrary,
    InputRelocatableObject,
    InputBitcode,
    InputArchive,
    InputStartupObject,
    InputTerminationObject,
    InputRuntimeComponent,
    InputNativeLibrary,
    InputFramework,
    InputModeOrdinary,
    InputModeWholeArchive,
    OutputExecutable,
    OutputSharedLibrary,
    OutputStaticLibrary,
    OutputImportLibrary,
    OutputDebugCompanion,
    OutputPlatformCompanion,
    SearchPathLibrary,
    SearchPathFramework,
    LinkModelDefault,
    LinkModelStatic,
    LinkModelDynamic,
    DeadStripPreserve,
    DeadStripRemoveUnreachable,
    SectionGarbageCollectionPreserve,
    SectionGarbageCollectionRemoveUnreferenced,
    DebugNone,
    DebugEmbedded,
    DebugCompanion,
    SubsystemConsole,
    SubsystemWindowed,
    SubsystemNative,
    SubsystemWasiCommand,
    SubsystemWasiReactor,
    SymbolEntryPoint,
    SymbolExportedSymbols,
    SymbolRetainedSymbols,
    StartupNotApplicable,
    StartupExplicitInputs,
    StartupPlatformCompilerDriver,
    RuntimeExplicitInput,
    OptimizationThinLto,
}

impl DiagnosticLinkRequirement {
    /// Returns the rejected requirement category.
    pub const fn kind(&self) -> DiagnosticLinkRequirementKind {
        match self {
            Self::Target(_) => DiagnosticLinkRequirementKind::Target,
            Self::ProductExecutable | Self::ProductSharedLibrary | Self::ProductStaticLibrary => {
                DiagnosticLinkRequirementKind::Product
            }
            Self::InputRelocatableObject
            | Self::InputBitcode
            | Self::InputArchive
            | Self::InputStartupObject
            | Self::InputTerminationObject
            | Self::InputRuntimeComponent
            | Self::InputNativeLibrary
            | Self::InputFramework => DiagnosticLinkRequirementKind::Input,
            Self::InputModeOrdinary | Self::InputModeWholeArchive => {
                DiagnosticLinkRequirementKind::InputMode
            }
            Self::OutputExecutable
            | Self::OutputSharedLibrary
            | Self::OutputStaticLibrary
            | Self::OutputImportLibrary
            | Self::OutputDebugCompanion
            | Self::OutputPlatformCompanion => DiagnosticLinkRequirementKind::Output,
            Self::SearchPathLibrary | Self::SearchPathFramework => {
                DiagnosticLinkRequirementKind::SearchPath
            }
            Self::LinkModelDefault | Self::LinkModelStatic | Self::LinkModelDynamic => {
                DiagnosticLinkRequirementKind::LinkModel
            }
            Self::DeadStripPreserve | Self::DeadStripRemoveUnreachable => {
                DiagnosticLinkRequirementKind::DeadStrip
            }
            Self::SectionGarbageCollectionPreserve
            | Self::SectionGarbageCollectionRemoveUnreferenced => {
                DiagnosticLinkRequirementKind::SectionGarbageCollection
            }
            Self::DebugNone | Self::DebugEmbedded | Self::DebugCompanion => {
                DiagnosticLinkRequirementKind::Debug
            }
            Self::SubsystemConsole
            | Self::SubsystemWindowed
            | Self::SubsystemNative
            | Self::SubsystemWasiCommand
            | Self::SubsystemWasiReactor => DiagnosticLinkRequirementKind::Subsystem,
            Self::SymbolEntryPoint | Self::SymbolExportedSymbols | Self::SymbolRetainedSymbols => {
                DiagnosticLinkRequirementKind::Symbol
            }
            Self::StartupNotApplicable
            | Self::StartupExplicitInputs
            | Self::StartupPlatformCompilerDriver => DiagnosticLinkRequirementKind::Startup,
            Self::RuntimeExplicitInput => DiagnosticLinkRequirementKind::Runtime,
            Self::OptimizationThinLto => DiagnosticLinkRequirementKind::Optimization,
        }
    }

    /// Returns the exact stable domain value.
    pub fn value(&self) -> &str {
        match self {
            Self::Target(value) => value,
            Self::ProductExecutable | Self::OutputExecutable => "executable",
            Self::ProductSharedLibrary | Self::OutputSharedLibrary => "shared_library",
            Self::ProductStaticLibrary | Self::OutputStaticLibrary => "static_library",
            Self::InputRelocatableObject => "relocatable_object",
            Self::InputBitcode => "bitcode",
            Self::InputArchive => "archive",
            Self::InputStartupObject => "startup_object",
            Self::InputTerminationObject => "termination_object",
            Self::InputRuntimeComponent => "runtime_component",
            Self::InputNativeLibrary => "native_library",
            Self::InputFramework => "framework",
            Self::InputModeOrdinary => "ordinary",
            Self::InputModeWholeArchive => "whole_archive",
            Self::OutputImportLibrary => "import_library",
            Self::OutputDebugCompanion => "debug_companion",
            Self::OutputPlatformCompanion => "platform_companion",
            Self::SearchPathLibrary => "library",
            Self::SearchPathFramework => "framework",
            Self::LinkModelDefault => "default",
            Self::LinkModelStatic => "static",
            Self::LinkModelDynamic => "dynamic",
            Self::DeadStripPreserve | Self::SectionGarbageCollectionPreserve => "preserve",
            Self::DeadStripRemoveUnreachable => "remove_unreachable",
            Self::SectionGarbageCollectionRemoveUnreferenced => "remove_unreferenced",
            Self::DebugNone => "none",
            Self::DebugEmbedded => "embedded",
            Self::DebugCompanion => "companion",
            Self::SubsystemConsole => "console",
            Self::SubsystemWindowed => "windowed",
            Self::SubsystemNative => "native",
            Self::SubsystemWasiCommand => "wasi_command",
            Self::SubsystemWasiReactor => "wasi_reactor",
            Self::SymbolEntryPoint => "entry_point",
            Self::SymbolExportedSymbols => "exported_symbols",
            Self::SymbolRetainedSymbols => "retained_symbols",
            Self::StartupNotApplicable => "not_applicable",
            Self::StartupExplicitInputs => "explicit_inputs",
            Self::StartupPlatformCompilerDriver => "platform_compiler_driver",
            Self::RuntimeExplicitInput => "explicit_input",
            Self::OptimizationThinLto => "thin_lto",
        }
    }
}

/// Category of native link requirement rejected by a driver.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLinkRequirementKind {
    Target,
    Product,
    Input,
    InputMode,
    Output,
    SearchPath,
    LinkModel,
    DeadStrip,
    SectionGarbageCollection,
    Debug,
    Subsystem,
    Symbol,
    Startup,
    Runtime,
    Optimization,
}

impl DiagnosticLinkRequirementKind {
    /// Returns the stable machine key for this requirement category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::Product => "product",
            Self::Input => "input",
            Self::InputMode => "input_mode",
            Self::Output => "output",
            Self::SearchPath => "search_path",
            Self::LinkModel => "link_model",
            Self::DeadStrip => "dead_strip",
            Self::SectionGarbageCollection => "section_garbage_collection",
            Self::Debug => "debug",
            Self::Subsystem => "subsystem",
            Self::Symbol => "symbol",
            Self::Startup => "startup",
            Self::Runtime => "runtime",
            Self::Optimization => "optimization",
        }
    }
}
