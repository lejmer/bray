use super::super::source::format_english_quoted_text;
use bray_diagnostics::DiagnosticLinkRequirement;

pub(crate) fn format_english_link_requirement(requirement: &DiagnosticLinkRequirement) -> String {
    use DiagnosticLinkRequirement as Requirement;

    match requirement {
        Requirement::Target(triple) => format_english_quoted_text(triple),
        Requirement::ProductExecutable => "executable product".to_owned(),
        Requirement::ProductSharedLibrary => "shared-library product".to_owned(),
        Requirement::ProductStaticLibrary => "static-library product".to_owned(),
        Requirement::InputRelocatableObject => "relocatable object".to_owned(),
        Requirement::InputBitcode => "LLVM bitcode".to_owned(),
        Requirement::InputArchive => "native archive".to_owned(),
        Requirement::InputStartupObject => "startup object".to_owned(),
        Requirement::InputTerminationObject => "termination object".to_owned(),
        Requirement::InputRuntimeComponent => "Bray runtime component".to_owned(),
        Requirement::InputNativeLibrary => "native library".to_owned(),
        Requirement::InputFramework => "platform framework".to_owned(),
        Requirement::InputModeOrdinary => "ordinary archive treatment".to_owned(),
        Requirement::InputModeWholeArchive => "whole-archive treatment".to_owned(),
        Requirement::OutputExecutable => "executable output".to_owned(),
        Requirement::OutputSharedLibrary => "shared-library output".to_owned(),
        Requirement::OutputStaticLibrary => "static-library output".to_owned(),
        Requirement::OutputImportLibrary => "import-library output".to_owned(),
        Requirement::OutputDebugCompanion => "separate debug-information output".to_owned(),
        Requirement::OutputPlatformCompanion => "platform companion output".to_owned(),
        Requirement::SearchPathLibrary => "native-library search path".to_owned(),
        Requirement::SearchPathFramework => "platform-framework search path".to_owned(),
        Requirement::LinkModelDefault => "target-default linkage".to_owned(),
        Requirement::LinkModelStatic => "static linkage".to_owned(),
        Requirement::LinkModelDynamic => "dynamic linkage".to_owned(),
        Requirement::DeadStripPreserve => "preserve unreachable code".to_owned(),
        Requirement::DeadStripRemoveUnreachable => "remove unreachable code".to_owned(),
        Requirement::SectionGarbageCollectionPreserve => {
            "preserve unreferenced sections".to_owned()
        }
        Requirement::SectionGarbageCollectionRemoveUnreferenced => {
            "remove unreferenced sections".to_owned()
        }
        Requirement::DebugNone => "omit debug information".to_owned(),
        Requirement::DebugEmbedded => "embed debug information".to_owned(),
        Requirement::DebugCompanion => "emit separate debug information".to_owned(),
        Requirement::SubsystemConsole => "console subsystem".to_owned(),
        Requirement::SubsystemWindowed => "windowed subsystem".to_owned(),
        Requirement::SubsystemNative => "native subsystem".to_owned(),
        Requirement::SubsystemWasiCommand => "WASI command".to_owned(),
        Requirement::SubsystemWasiReactor => "WASI reactor".to_owned(),
        Requirement::SymbolEntryPoint => "explicit entry point".to_owned(),
        Requirement::SymbolExportedSymbols => "explicit exported-symbol set".to_owned(),
        Requirement::SymbolRetainedSymbols => "explicit retained-symbol set".to_owned(),
        Requirement::StartupNotApplicable => "no startup ownership".to_owned(),
        Requirement::StartupExplicitInputs => "explicit startup inputs".to_owned(),
        Requirement::StartupPlatformCompilerDriver => "platform compiler-driver startup".to_owned(),
        Requirement::RuntimeExplicitInput => "explicit runtime input".to_owned(),
        Requirement::OptimizationThinLto => "LLVM ThinLTO".to_owned(),
    }
}
