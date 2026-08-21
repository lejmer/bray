use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticLinkOptimizationReportProblem, DiagnosticLinkerDriverIdentity,
    DiagnosticLinkerDriverKind, SeverityKind,
};

use crate::{LinkOptimizationReportProblem, LinkerDriverIdentity};

pub(super) fn diagnostic_driver_identity(
    driver: &LinkerDriverIdentity,
) -> DiagnosticLinkerDriverIdentity {
    let kind = match driver.kind() {
        crate::LinkerDriverKind::EmbeddedLld => DiagnosticLinkerDriverKind::EmbeddedLld,
        crate::LinkerDriverKind::ExternalLld => DiagnosticLinkerDriverKind::ExternalLld,
        crate::LinkerDriverKind::System => DiagnosticLinkerDriverKind::System,
        crate::LinkerDriverKind::Archiver => DiagnosticLinkerDriverKind::Archiver,
        crate::LinkerDriverKind::TargetSpecific => DiagnosticLinkerDriverKind::TargetSpecific,
    };

    DiagnosticLinkerDriverIdentity::new(
        kind,
        driver.name(),
        driver.capability_revision(),
        driver.toolchain_revision(),
    )
}

pub(super) fn optimization_report_diagnostic(
    problem: &LinkOptimizationReportProblem,
) -> Diagnostic {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::LinkerOptimizationReportInvalid,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::link_optimization_report_problem(
        diagnostic_optimization_report_problem(problem),
    ));

    match problem {
        LinkOptimizationReportProblem::Inaccessible(kind) => diagnostic.with_arg(
            DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(*kind)),
        ),
        LinkOptimizationReportProblem::Missing
        | LinkOptimizationReportProblem::Malformed
        | LinkOptimizationReportProblem::UnsupportedFormat(_)
        | LinkOptimizationReportProblem::ToolchainMismatch(_)
        | LinkOptimizationReportProblem::DriverMismatch(_)
        | LinkOptimizationReportProblem::InconsistentCacheOutcomes
        | LinkOptimizationReportProblem::MissingResourceMeasurement => diagnostic,
    }
}

const fn diagnostic_optimization_report_problem(
    problem: &LinkOptimizationReportProblem,
) -> DiagnosticLinkOptimizationReportProblem {
    match problem {
        LinkOptimizationReportProblem::Missing => DiagnosticLinkOptimizationReportProblem::Missing,
        LinkOptimizationReportProblem::Inaccessible(_) => {
            DiagnosticLinkOptimizationReportProblem::Inaccessible
        }
        LinkOptimizationReportProblem::Malformed => {
            DiagnosticLinkOptimizationReportProblem::Malformed
        }
        LinkOptimizationReportProblem::UnsupportedFormat(_) => {
            DiagnosticLinkOptimizationReportProblem::UnsupportedFormat
        }
        LinkOptimizationReportProblem::ToolchainMismatch(_) => {
            DiagnosticLinkOptimizationReportProblem::ToolchainMismatch
        }
        LinkOptimizationReportProblem::DriverMismatch(_) => {
            DiagnosticLinkOptimizationReportProblem::DriverMismatch
        }
        LinkOptimizationReportProblem::InconsistentCacheOutcomes => {
            DiagnosticLinkOptimizationReportProblem::InconsistentCacheOutcomes
        }
        LinkOptimizationReportProblem::MissingResourceMeasurement => {
            DiagnosticLinkOptimizationReportProblem::MissingResourceMeasurement
        }
    }
}
