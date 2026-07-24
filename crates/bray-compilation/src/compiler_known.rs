use bray_compiler_known::AvailabilityRule;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CompilerKnownCatalogAudit, CompilerKnownCatalogAuditError, CompilerKnownCatalogAuditReport,
    CompilerKnownTargetProfile, PackageIdentity, SymbolCompletionLevel,
};

use crate::{
    CancellationToken, Compilation, CompilationLoadError, CompilationOptions, CompilationRequest,
    FactQueryError, SymbolCompletionError, WorkerBudget, WorkerBudgetError,
};

/// Deterministic result of checking generated compiler-known semantic data.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CompilerKnownCatalogCheckReport {
    audit: CompilerKnownCatalogAuditReport,
}

impl CompilerKnownCatalogCheckReport {
    /// Returns the catalog-owned semantic audit summary.
    pub const fn audit(self) -> CompilerKnownCatalogAuditReport {
        self.audit
    }
}

/// Validates generated compiler-known identities, roles, target views, and semantic completion.
pub fn check_compiler_known_catalog()
-> Result<CompilerKnownCatalogCheckReport, CompilerKnownCatalogCheckError> {
    let serial_workers = WorkerBudget::serial();

    let parallel_workers =
        WorkerBudget::new(4).map_err(CompilerKnownCatalogCheckError::WorkerBudget)?;

    let serial = check_compiler_known_catalog_with(serial_workers)?;
    let parallel = check_compiler_known_catalog_with(parallel_workers)?;

    if serial != parallel {
        return Err(CompilerKnownCatalogCheckError::NondeterministicValidation);
    }

    if !serial.diagnostics.is_empty() {
        return Err(CompilerKnownCatalogCheckError::SemanticDiagnostics(
            serial.diagnostics,
        ));
    }

    Ok(CompilerKnownCatalogCheckReport {
        audit: serial.audit,
    })
}

#[derive(Debug, Eq, PartialEq)]
struct CompilerKnownCatalogCheckOutcome {
    audit: CompilerKnownCatalogAuditReport,
    diagnostics: DiagnosticBag,
}

fn check_compiler_known_catalog_with(
    workers: WorkerBudget,
) -> Result<CompilerKnownCatalogCheckOutcome, CompilerKnownCatalogCheckError> {
    let Some(package) = PackageIdentity::try_new("bray.catalog.validation") else {
        return Err(CompilerKnownCatalogCheckError::InvalidPackageIdentity);
    };

    let options = CompilationOptions::new(
        workers,
        bray_symbols::ProductKind::Library,
        crate::SelectedTarget::baseline(),
    );

    // The private validation compilation has no source package because catalog symbols are its
    // only semantic roots.
    let request = CompilationRequest::with_options(package, Vec::new(), options);

    let compilation =
        Compilation::load(request).map_err(CompilerKnownCatalogCheckError::CompilationLoad)?;

    if !compilation.declaration_diagnostics().is_empty() {
        return Err(CompilerKnownCatalogCheckError::DeclarationDiagnostics);
    }

    let graph = compilation
        .symbol_graph()
        .map_err(CompilerKnownCatalogCheckError::Fact)?;

    let mut audit =
        CompilerKnownCatalogAudit::new(graph).map_err(CompilerKnownCatalogCheckError::Audit)?;

    audit_target_views(&mut audit)?;

    let root = graph.roots().compiler_known().into();
    let cancellation = CancellationToken::new();

    let diagnostics = compilation
        .force_complete_symbol(
            root,
            SymbolCompletionLevel::DeclarationSurface,
            &cancellation,
        )
        .map_err(CompilerKnownCatalogCheckError::Completion)?;

    Ok(CompilerKnownCatalogCheckOutcome {
        audit: audit.report(),
        diagnostics,
    })
}

fn audit_target_views(
    audit: &mut CompilerKnownCatalogAudit<'_>,
) -> Result<(), CompilerKnownCatalogCheckError> {
    audit
        .audit_target_view(CompilerKnownTargetProfile::Portable, |rule| {
            rule == AvailabilityRule::Always
        })
        .map_err(CompilerKnownCatalogCheckError::Audit)?;

    audit
        .audit_target_view(CompilerKnownTargetProfile::Complete, |_| true)
        .map_err(CompilerKnownCatalogCheckError::Audit)?;

    for capability in AvailabilityRule::ALL
        .iter()
        .copied()
        .filter(|rule| *rule != AvailabilityRule::Always)
    {
        audit
            .audit_target_view(CompilerKnownTargetProfile::Capability(capability), |rule| {
                rule == AvailabilityRule::Always || rule == capability
            })
            .map_err(CompilerKnownCatalogCheckError::Audit)?;
    }

    Ok(())
}

/// A violated invariant while checking generated compiler-known semantic data.
#[derive(Debug, Eq, PartialEq)]
pub enum CompilerKnownCatalogCheckError {
    /// The private validation package identity is invalid.
    InvalidPackageIdentity,
    /// An empty declaration table unexpectedly produced diagnostics.
    DeclarationDiagnostics,
    /// The private catalog-validation compilation could not be loaded.
    CompilationLoad(CompilationLoadError),
    /// A compiler fact required by catalog validation failed.
    Fact(FactQueryError),
    /// The compiler-known semantic audit found an inconsistent identity or role.
    Audit(CompilerKnownCatalogAuditError),
    /// Recursive semantic completion failed.
    Completion(SymbolCompletionError<FactQueryError>),
    /// The fixed parallel validation budget could not be constructed.
    WorkerBudget(WorkerBudgetError),
    /// Independent serial and parallel validation produced different immutable results.
    NondeterministicValidation,
    /// Semantic completion produced diagnostics for checked-in catalog data.
    SemanticDiagnostics(DiagnosticBag),
}

impl std::fmt::Display for CompilerKnownCatalogCheckError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPackageIdentity => {
                formatter.write_str("compiler-known validation package identity is invalid")
            }
            Self::DeclarationDiagnostics => {
                formatter.write_str("empty compiler-known validation package produced declarations")
            }
            Self::CompilationLoad(error) => {
                write!(
                    formatter,
                    "compiler-known validation compilation failed: {error}"
                )
            }
            Self::Fact(error) => write!(formatter, "compiler-known fact failed: {error}"),
            Self::Audit(error) => write!(formatter, "compiler-known audit failed: {error}"),
            Self::Completion(error) => {
                write!(formatter, "compiler-known completion failed: {error}")
            }
            Self::WorkerBudget(error) => {
                write!(
                    formatter,
                    "compiler-known worker budget is invalid: {error}"
                )
            }
            Self::NondeterministicValidation => {
                formatter.write_str("serial and parallel compiler-known validation disagree")
            }
            Self::SemanticDiagnostics(_) => {
                formatter.write_str("compiler-known completion produced diagnostics")
            }
        }
    }
}

impl std::error::Error for CompilerKnownCatalogCheckError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CompilationLoad(error) => Some(error),
            Self::Fact(error) => Some(error),
            Self::Audit(error) => Some(error),
            Self::Completion(error) => Some(error),
            Self::WorkerBudget(error) => Some(error),
            Self::InvalidPackageIdentity
            | Self::DeclarationDiagnostics
            | Self::NondeterministicValidation
            | Self::SemanticDiagnostics(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AvailabilityRule, check_compiler_known_catalog};

    #[test]
    fn generated_catalog_passes_complete_serial_and_parallel_validation() {
        let report = match check_compiler_known_catalog() {
            Ok(report) => report,
            Err(error) => panic!("generated catalog must validate: {error:?}"),
        };

        let audit = report.audit();

        assert!(audit.scopes() > 0);
        assert!(audit.declarations() > 0);
        assert!(audit.values() > 0);
        assert!(audit.representation_roles() > 0);
        assert!(audit.implementation_roles() > 0);
        assert!(audit.completion_units() >= audit.declarations());
        assert!(audit.completion_facts() > 0);
        assert_eq!(audit.target_profiles(), AvailabilityRule::ALL.len() + 1);
    }

    #[test]
    fn repeated_catalog_checks_produce_the_same_report() {
        let first = check_compiler_known_catalog();
        let second = check_compiler_known_catalog();

        assert_eq!(first, second);
    }
}
