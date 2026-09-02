use std::hash::Hash;

use bray_base::StableDigestHasher;
use bray_diagnostics::{DiagnosticFailureField, DiagnosticFailureValue};

pub(crate) fn text_field(
    name: &'static str,
    value: impl Into<String>,
) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Text(value.into()))
}

pub(crate) fn identity_field(
    name: &'static str,
    value: &impl Hash,
) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Identity(identity(value)))
}

pub(crate) fn identity(value: &impl Hash) -> [u8; 32] {
    let mut hasher = StableDigestHasher::new();
    value.hash(&mut hasher);

    hasher.finalize()
}

pub(crate) fn natural_field(name: &'static str, value: usize) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Natural(value.to_string()))
}

pub(crate) fn count_field(name: &'static str, value: u64) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Count(value))
}

pub(super) fn signed_field(name: &'static str, value: i64) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Signed(value))
}

pub(crate) fn push_symbol(
    context: &mut Vec<DiagnosticFailureField>,
    kind_name: &'static str,
    identity_name: &'static str,
    symbol: bray_symbols::AnySymbolId,
) {
    context.push(text_field(kind_name, symbol.kind().as_str()));
    context.push(count_field(identity_name, u64::from(symbol.symbol_id().raw())));
}

pub(crate) fn push_source_span(
    context: &mut Vec<DiagnosticFailureField>,
    source_name: &'static str,
    start_name: &'static str,
    end_name: &'static str,
    source: bray_source::SourceSpan,
) {
    context.push(count_field(
        source_name,
        u64::from(source.source_id().raw()),
    ));

    context.push(count_field(
        start_name,
        u64::from(source.range().start().bytes()),
    ));

    context.push(count_field(
        end_name,
        u64::from(source.range().end().bytes()),
    ));
}
