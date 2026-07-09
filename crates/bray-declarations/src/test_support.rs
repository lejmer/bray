use bray_source::SourceSnapshot;
use bray_syntax::SourceUnitSyntax;

pub(crate) fn parse_valid_source_unit(source: &SourceSnapshot) -> SourceUnitSyntax {
    let result = bray_parser::parse_source_unit(source);

    assert!(
        result.diagnostics().is_empty(),
        "source should parse without diagnostics: {:?}",
        result.diagnostics()
    );

    let (_source_id, source_unit, _diagnostics) = result.into_parts();

    source_unit
}
