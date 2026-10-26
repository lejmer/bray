use bray_linker::{
    LinkFailure, LinkOutcome, LinkPlan, Linker, LinkerDriver, LinkerDriverCapabilities,
    LinkerDriverIdentity, LinkerDriverKind,
};
use bray_symbols::ProductKind;
use std::sync::Arc;

pub(in super::super) fn test_linked_product(kind: ProductKind) -> bray_linker::LinkedProductKind {
    match kind {
        ProductKind::Library => bray_linker::LinkedProductKind::StaticLibrary,
        ProductKind::Executable | ProductKind::Test => bray_linker::LinkedProductKind::Executable,
    }
}

pub(in super::super) fn test_linker() -> Linker {
    Linker::try_new([Arc::new(TestLinkerDriver) as Arc<dyn LinkerDriver>])
        .unwrap_or_else(|error| panic!("test linker must validate: {error:?}"))
}

struct TestLinkerDriver;

impl LinkerDriver for TestLinkerDriver {
    fn capabilities(&self) -> &LinkerDriverCapabilities {
        static CAPABILITIES: std::sync::OnceLock<LinkerDriverCapabilities> =
            std::sync::OnceLock::new();

        CAPABILITIES.get_or_init(|| {
            let identity =
                LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "test-lld", "1", "22")
                    .unwrap_or_else(|| panic!("test linker identity must be valid"));

            LinkerDriverCapabilities::try_for_lld(identity)
                .unwrap_or_else(|error| panic!("test capabilities must be valid: {error:?}"))
        })
    }

    fn link(&self, plan: &LinkPlan, _cancellation: &dyn bray_base::Cancellation) -> LinkOutcome {
        LinkOutcome::failed(
            plan,
            LinkFailure::Invocation,
            bray_diagnostics::DiagnosticBag::new(),
        )
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
pub(in super::super) fn run_native_caller(
    backend: &bray_codegen_llvm::LlvmCodeGenerator,
    plan: &crate::compilation::product::codegen::NativeProductPlan,
    caller: &str,
) {
    let directory = tempfile::tempdir().expect("temporary fixture directory must be created");
    let mut objects = Vec::new();

    for (index, bytes) in super::artifacts::generated_artifacts(backend, plan)
        .into_iter()
        .enumerate()
    {
        let path = directory.path().join(format!("unit-{index}.obj"));

        std::fs::write(&path, bytes).expect("fixture contents must be written");
        objects.push(path);
    }

    let source = directory.path().join("caller.c");
    let executable = directory.path().join("caller.exe");

    std::fs::write(&source, caller).expect("fixture contents must be written");

    let compiler = std::path::Path::new(bray_codegen_llvm::COMPILED_LLVM_PREFIX.expect("native fixture requires the configured LLVM toolchain prefix"))
        .join("bin/clang.exe");

    let linked = std::process::Command::new(compiler)
        .args(["--target=x86_64-pc-windows-msvc", "-fuse-ld=lld"])
        .arg(&source)
        .args(objects)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("fixture command must execute");

    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );

    assert_eq!(
        std::process::Command::new(executable)
            .status()
            .expect("fixture command must execute")
            .code(),
        Some(0)
    );
}
