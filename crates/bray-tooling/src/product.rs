use std::env;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use bray_codegen::{
    CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration,
};
use bray_codegen_llvm::LlvmCodeGenerator;
use bray_compilation::{
    Compilation, CompilationRequest, PackageInterfaceExportRequest,
    SelectedTarget,
};
use bray_linker::{
    ExternalToolHost, ExternalToolProcessBudget, Linker, LinkerDriver,
    LinkerDriverIdentity, LinkerDriverKind, LlvmArchiveDriver,
    NativeExternalToolHost, SystemLinkerConfiguration, SystemLinkerDriver,
    SystemLinkerFamily,
};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity,
    InterfaceProductKind, PackageInterfaceIdentity,
};
use bray_symbols::ProductIdentity;
use bray_target::{
    TargetOutputDescription, TargetOutputKind, TargetOutputName,
};

/// Loads a compilation without a code-generation backend.
pub fn load_compilation(request: CompilationRequest) -> Option<Compilation> {
    Compilation::load(request).ok()
}

/// Loads a compilation configured with the LLVM code-generation backend.
pub fn load_llvm_compilation(
    request: CompilationRequest,
) -> Option<Compilation> {
    let generator = LlvmCodeGenerator::try_new().ok()?;
    let identity = generator.identity().clone();

    let registry = CodeGeneratorRegistry::try_new([
        Arc::new(generator) as Arc<dyn CodeGenerator>
    ])
    .ok()?;

    let codegen = CodegenConfiguration::try_new(registry, identity).ok()?;

    Compilation::load_with_codegen(request, codegen).ok()
}

/// Creates the package-interface export request for a library product.
pub fn package_interface_export_request(
    product: ProductIdentity,
) -> PackageInterfaceExportRequest {
    let interface_product = InterfaceProductIdentity::try_new(product.name())
        .unwrap_or_else(|| panic!("the product identity must be valid"));

    let identity = PackageInterfaceIdentity::try_new(
        product.package().clone(),
        interface_product,
        InterfaceProductKind::Library,
        "public-v1",
    )
    .unwrap_or_else(|| panic!("the public-surface identity must be valid"));

    PackageInterfaceExportRequest::new(
        identity,
        InterfaceLanguageRevision::new(0),
    )
}

/// Returns the baseline filename contract for one target output kind.
pub fn baseline_output_name(kind: TargetOutputKind) -> TargetOutputName {
    let (prefix, suffix) = match kind {
        TargetOutputKind::Assembly => ("", ".s"),
        TargetOutputKind::BackendIr => ("", ".ll"),
        TargetOutputKind::BackendBitcode => ("", ".bc"),
        TargetOutputKind::RelocatableObject => ("", ".o"),
        TargetOutputKind::ExecutableModule => ("", ".wasm"),
        TargetOutputKind::DebugCompanion => ("", ".debug"),
        TargetOutputKind::PackageInterface => ("", ".brayi"),
        TargetOutputKind::DependencyMetadata => ("", ".brayd"),
        TargetOutputKind::Executable => ("", ""),
        TargetOutputKind::StaticLibrary => ("lib", ".a"),
        TargetOutputKind::SharedLibrary => ("lib", ".so"),
        TargetOutputKind::LinkedCompanion => ("", ".companion"),
    };

    TargetOutputName::try_new(kind, prefix, suffix)
        .unwrap_or_else(|error| {
            panic!("baseline target output name must be valid: {error:?}")
        })
}

/// Creates deterministic baseline output descriptions for selected artifact kinds.
pub fn baseline_target_outputs(
    selected: &SelectedTarget,
    kinds: impl IntoIterator<Item = TargetOutputKind>,
) -> TargetOutputDescription {
    TargetOutputDescription::try_new(
        selected.profile().clone(),
        kinds.into_iter().map(baseline_output_name),
    )
    .unwrap_or_else(|error| {
        panic!("baseline target output names must be valid: {error:?}")
    })
}

/// Creates the native linker and archiver composition available to command drivers.
pub fn native_linker() -> Option<Linker> {
    let archive = llvm_tool("llvm-ar")?;

    let host = Arc::new(NativeExternalToolHost::new(
        ExternalToolProcessBudget::new(NonZeroUsize::MIN),
    ));

    let system_identity = LinkerDriverIdentity::try_new(
        LinkerDriverKind::System,
        "gnu-compiler",
        "1",
        "1",
    )?;

    let archive_identity = LinkerDriverIdentity::try_new(
        LinkerDriverKind::Archiver,
        "llvm-ar",
        "1",
        "22",
    )?;

    let (family, program, environment) = if cfg!(windows) {
        let root = env::var_os("SystemRoot").map(PathBuf::from)?;

        let environment = [
            "SystemRoot",
            "USERPROFILE",
            "LOCALAPPDATA",
            "WSLENV",
            "PATH",
            "TEMP",
            "TMP",
        ]
        .into_iter()
        .filter_map(|name| env::var_os(name).map(|value| (name.into(), value)));

        (
            SystemLinkerFamily::WslGnuCompiler,
            root.join("System32").join("wsl.exe"),
            environment.collect::<Vec<_>>(),
        )
    } else {
        (
            SystemLinkerFamily::GnuCompiler,
            PathBuf::from("/usr/bin/cc"),
            Vec::new(),
        )
    };

    let configuration =
        SystemLinkerConfiguration::try_new(family, program, environment, None).ok()?;

    let system = SystemLinkerDriver::try_new(
        system_identity,
        configuration,
        Arc::clone(&host) as Arc<dyn ExternalToolHost>,
    )
    .ok()?;

    let archive = LlvmArchiveDriver::try_new(
        archive_identity,
        archive,
        [],
        None,
        host as Arc<dyn ExternalToolHost>,
    )
    .ok()?;

    Linker::try_new([
        Arc::new(system) as Arc<dyn LinkerDriver>,
        Arc::new(archive) as Arc<dyn LinkerDriver>,
    ])
    .ok()
}

fn llvm_tool(name: &str) -> Option<PathBuf> {
    let executable_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };

    let configured = env::var_os("LLVM_SYS_221_PREFIX")
        .map(PathBuf::from)
        .or_else(|| option_env!("LLVM_SYS_221_PREFIX").map(PathBuf::from))
        .map(|prefix| prefix.join("bin").join(&executable_name));

    if configured.as_ref().is_some_and(|path| path.is_file()) {
        return configured;
    }

    let executable = env::current_exe().ok()?;

    executable
        .ancestors()
        .map(|ancestor| {
            ancestor
                .join("toolchains")
                .join("llvm")
                .join("active")
                .join("bin")
                .join(&executable_name)
        })
        .find(|path| path.is_file())
}
