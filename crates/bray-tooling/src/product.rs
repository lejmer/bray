use std::env;
use std::ffi::OsString;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use bray_codegen::{
    CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration,
};
use bray_codegen_llvm::LlvmCodeGenerator;
use bray_compilation::{
    Compilation, CompilationRequest, PackageInterfaceExportRequest,
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
use bray_target::{NativeTarget, ObjectFormat};

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

/// Creates the linker composition available for one native toolchain target.
pub fn native_linker(target: NativeTarget) -> Option<Linker> {
    let archive = llvm_tool("llvm-ar")?;

    let host = Arc::new(NativeExternalToolHost::new(
        ExternalToolProcessBudget::new(NonZeroUsize::MIN),
    ));

    let archive_identity = LinkerDriverIdentity::try_new(
        LinkerDriverKind::Archiver,
        "llvm-ar",
        "1",
        "22",
    )?;

    let archive = LlvmArchiveDriver::try_new(
        archive_identity,
        archive,
        [],
        None,
        Arc::clone(&host) as Arc<dyn ExternalToolHost>,
    )
    .ok()?;

    let mut drivers = vec![Arc::new(archive) as Arc<dyn LinkerDriver>];

    if let Some((family, program, environment)) = system_linker_configuration(target) {
        let system_identity = LinkerDriverIdentity::try_new(
            LinkerDriverKind::System,
            system_linker_name(family),
            "1",
            "1",
        )?;

        let configuration =
            SystemLinkerConfiguration::try_new(family, program, environment, None).ok()?;

        let system = SystemLinkerDriver::try_new(
            system_identity,
            configuration,
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        )
        .ok()?;

        drivers.push(Arc::new(system) as Arc<dyn LinkerDriver>);
    }

    Linker::try_new(drivers).ok()
}

fn system_linker_configuration(
    target: NativeTarget,
) -> Option<(SystemLinkerFamily, PathBuf, Vec<(OsString, OsString)>)> {
    if NativeTarget::current()
        .is_none_or(|host| host.architecture() != target.architecture())
    {
        return None;
    }

    match target.object_format() {
        ObjectFormat::Elf if cfg!(windows) => {
            let root = env::var_os("SystemRoot").map(PathBuf::from)?;

            Some((
                SystemLinkerFamily::WslGnuCompiler,
                root.join("System32").join("wsl.exe"),
                selected_environment(&[
                    "SystemRoot",
                    "USERPROFILE",
                    "LOCALAPPDATA",
                    "WSLENV",
                    "PATH",
                    "TEMP",
                    "TMP",
                ]),
            ))
        }
        ObjectFormat::Elf if cfg!(target_os = "linux") => Some((
            SystemLinkerFamily::GnuCompiler,
            PathBuf::from("/usr/bin/cc"),
            Vec::new(),
        )),
        ObjectFormat::Coff if cfg!(windows) => Some((
            SystemLinkerFamily::MicrosoftCompiler,
            llvm_tool("clang")?,
            selected_environment(&[
                "SystemRoot",
                "USERPROFILE",
                "LOCALAPPDATA",
                "PATH",
                "LIB",
                "INCLUDE",
                "TEMP",
                "TMP",
            ]),
        )),
        ObjectFormat::MachO if cfg!(target_os = "macos") => Some((
            SystemLinkerFamily::AppleCompiler,
            PathBuf::from("/usr/bin/clang"),
            selected_environment(&[
                "HOME",
                "PATH",
                "SDKROOT",
                "DEVELOPER_DIR",
                "TMPDIR",
            ]),
        )),
        ObjectFormat::Coff
        | ObjectFormat::Elf
        | ObjectFormat::MachO
        | ObjectFormat::WebAssembly
        | ObjectFormat::Xcoff => None,
    }
}

fn selected_environment(names: &[&str]) -> Vec<(OsString, OsString)> {
    names
        .iter()
        .filter_map(|name| env::var_os(name).map(|value| ((*name).into(), value)))
        .collect()
}

const fn system_linker_name(family: SystemLinkerFamily) -> &'static str {
    match family {
        SystemLinkerFamily::GnuCompiler => "gnu-compiler",
        SystemLinkerFamily::WslGnuCompiler => "wsl-gnu-compiler",
        SystemLinkerFamily::MicrosoftCompiler => "microsoft-compiler",
        SystemLinkerFamily::AppleCompiler => "apple-compiler",
        SystemLinkerFamily::Gnu
        | SystemLinkerFamily::Microsoft
        | SystemLinkerFamily::Apple => "system-linker",
    }
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
