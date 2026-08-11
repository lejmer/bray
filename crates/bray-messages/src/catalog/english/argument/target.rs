pub(super) const fn format_english_target_predicate_value_kind(
    kind: bray_diagnostics::DiagnosticTargetPredicateValueKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticTargetPredicateValueKind as Kind;

    match kind {
        Kind::String => "string",
        Kind::UnsignedInteger => "unsigned integer",
        Kind::Boolean => "Boolean",
    }
}
