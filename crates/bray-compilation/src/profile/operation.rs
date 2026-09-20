use super::descriptor::ProfileOperation;
use super::session::ProfileSession;

#[inline(always)]
pub(crate) fn profile_operation<T>(
    session: Option<&ProfileSession>,
    operation: ProfileOperation,
    action: impl FnOnce() -> T,
    outcome: impl FnOnce(&T) -> bray_profile::CompilationProfileOutcome,
) -> T {
    let Some(session) = session else {
        return action();
    };

    let span = session.start(operation, None);
    let result = action();

    span.finish(outcome(&result));

    result
}

pub(crate) fn merge_diagnostics<'diagnostic>(
    session: Option<&ProfileSession>,
    diagnostics: impl IntoIterator<Item = &'diagnostic bray_diagnostics::DiagnosticBag>,
) -> bray_diagnostics::DiagnosticBag {
    let Some(session) = session else {
        return bray_diagnostics::DiagnosticBag::merged_all(diagnostics);
    };

    let diagnostics = diagnostics.into_iter().collect::<Vec<_>>();

    let volume = diagnostics.iter().fold(0_usize, |total, diagnostics| {
        total.saturating_add(diagnostics.len())
    });

    let merged = bray_diagnostics::DiagnosticBag::merged_all(diagnostics);

    session.record_diagnostic_merge(volume);

    merged
}
