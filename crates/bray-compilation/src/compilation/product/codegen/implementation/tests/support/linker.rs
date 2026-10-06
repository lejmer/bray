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
