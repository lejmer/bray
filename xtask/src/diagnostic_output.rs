use bray_diagnostics::DiagnosticBag;
use bray_source::SourceStore;
use bray_tooling::{OutputFormat, write_diagnostics};

pub(crate) fn render_diagnostics(
    diagnostics: &DiagnosticBag,
    sources: &SourceStore,
) -> Option<String> {
    if diagnostics.is_empty() {
        return None;
    }

    let mut standard_output = Vec::new();
    let mut standard_error = Vec::new();

    write_diagnostics(
        diagnostics,
        Some(sources),
        OutputFormat::Text,
        &mut standard_output,
        &mut standard_error,
    )
    .ok()?;

    let rendered = String::from_utf8(standard_error).ok()?;
    let rendered = rendered.trim_end();

    (!rendered.is_empty()).then(|| rendered.to_owned())
}

pub(crate) fn failure_detail<'diagnostic>(
    cause: String,
    diagnostics: impl IntoIterator<Item = &'diagnostic DiagnosticBag>,
    sources: &SourceStore,
) -> String {
    let diagnostics = DiagnosticBag::merged_all(diagnostics);

    match render_diagnostics(&diagnostics, sources) {
        Some(rendered) => format!("{cause}\n{rendered}"),
        None => cause,
    }
}

#[cfg(test)]
mod tests {
    use bray_compilation::{Compilation, CompilationRequest};
    use bray_diagnostics::DiagnosticBag;
    use bray_source::{SourceIdentity, SourceInput};
    use bray_symbols::PackageIdentity;

    use super::failure_detail;

    #[test]
    fn failure_detail_preserves_cause_and_source_diagnostics() {
        let package = PackageIdentity::try_new("fixture").unwrap();

        let source = SourceInput::virtual_text(
            SourceIdentity::new(0),
            "fixture.bray",
            1,
            "module fixture;\nfunc broken( {\n",
        );

        let compilation = Compilation::load(CompilationRequest::new(package, vec![source]))
            .expect("malformed source must still load for diagnostics");

        let diagnostics = compilation.check_diagnostics();

        assert!(diagnostics.has_errors());

        let failure = failure_detail("native plan failed: leaf cause".into(), [diagnostics], compilation.sources());

        assert!(failure.contains("native plan failed: leaf cause"));
        assert!(failure.contains("fixture.bray"));
        assert!(failure.contains("error E"));

        let check_diagnostics = DiagnosticBag::new();

        assert_eq!(
            failure_detail(
                "native plan failed: leaf cause".into(),
                [&check_diagnostics, diagnostics],
                compilation.sources(),
            ),
            failure,
        );

        assert_eq!(
            failure_detail(
                "native plan failed: leaf cause".into(),
                [diagnostics, diagnostics],
                compilation.sources(),
            ),
            failure,
        );

        assert_eq!(
            failure_detail("leaf cause".into(), [&DiagnosticBag::new()], compilation.sources()),
            "leaf cause"
        );
    }
}
