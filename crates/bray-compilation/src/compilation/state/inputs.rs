use std::sync::{Arc, OnceLock};

use bray_source::{SourceId, SourceSnapshot, SourceStore};
use bray_symbols::{AvailableCompilerKnownSymbols, CompilerKnownSymbolProvider, PackageIdentity};

use crate::fact::CompilationInputKey;
use crate::{CompilationOptions, CompilationProfileReport, WorkerBudget};

use super::Compilation;

pub(super) fn shared_catalog() -> Arc<CompilerKnownSymbolProvider> {
    static PROVIDER: OnceLock<Arc<CompilerKnownSymbolProvider>> = OnceLock::new();

    let provider = PROVIDER.get_or_init(|| {
        let provider = CompilerKnownSymbolProvider::build()
            .unwrap_or_else(|error| panic!("compiler-known symbol provider is invalid: {error:?}"));

        Arc::new(provider)
    });

    // Compilation snapshots share the process-immutable generated catalog.
    Arc::clone(provider)
}

impl Compilation {
    /// Returns the source package identity selected for this compilation.
    pub fn package_identity(&self) -> &PackageIdentity {
        self.record_input(CompilationInputKey::PackageIdentity);

        &self.state.package_identity
    }

    /// Returns the authority governing this source package's identity.
    pub fn package_source_authority(&self) -> crate::PackageSourceAuthority {
        self.record_input(CompilationInputKey::PackageSourceAuthority);

        self.state.package_source_authority
    }

    /// Returns an immutable profile snapshot when profiling is enabled.
    pub fn profile_report(&self) -> Option<CompilationProfileReport> {
        self.state.fact_runtime.profile_report()
    }

    /// Returns the compilation options.
    pub fn options(&self) -> &CompilationOptions {
        &self.state.options
    }

    /// Returns the compiler-owned CPU worker budget.
    pub fn worker_budget(&self) -> WorkerBudget {
        self.state.options.worker_budget()
    }

    /// Returns the selected target and its available compiler-known declarations.
    pub fn selected_target(&self) -> &crate::SelectedTargetContext {
        self.record_input(CompilationInputKey::SelectedTarget);

        &self.state.selected_target
    }

    /// Returns the compiler-known symbols available for the selected target.
    pub fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
        self.selected_target().available_compiler_known_symbols()
    }

    /// Returns the loaded source snapshots.
    pub fn sources(&self) -> &SourceStore {
        self.record_input(CompilationInputKey::SourceSet);

        &self.state.sources
    }

    /// Returns diagnostics produced while loading source inputs.
    pub fn source_diagnostics(&self) -> &bray_diagnostics::DiagnosticBag {
        self.record_input(CompilationInputKey::SourceDiagnostics);

        &self.state.source_diagnostics
    }

    /// Returns the loaded source snapshot for `source_id`.
    pub fn source(&self, source_id: SourceId) -> Option<&SourceSnapshot> {
        self.record_input(CompilationInputKey::Source(source_id));

        self.state.sources.get(source_id)
    }

    /// Returns the loaded source text for `source_id`.
    pub fn source_text(&self, source_id: SourceId) -> Option<&str> {
        self.record_input(CompilationInputKey::Source(source_id));

        self.state.sources.text(source_id)
    }

    /// Returns the number of loaded source snapshots.
    pub fn source_count(&self) -> usize {
        self.record_input(CompilationInputKey::SourceSet);

        self.state.sources.len()
    }

    /// Returns whether this compilation has no source snapshots.
    pub fn is_empty(&self) -> bool {
        self.record_input(CompilationInputKey::SourceSet);

        self.state.sources.is_empty()
    }

    pub(super) fn compiler_known_provider(&self) -> &Arc<CompilerKnownSymbolProvider> {
        &self.state.compiler_known_symbols
    }
}
