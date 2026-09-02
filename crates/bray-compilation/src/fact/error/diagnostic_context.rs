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

pub(crate) const fn constant_value_kind(
    kind: &bray_symbols::ConstantValueKind,
) -> &'static str {
    use bray_symbols::ConstantValueKind as Kind;

    match kind {
        Kind::Error => "error",
        Kind::Boolean(_) => "boolean",
        Kind::Character(_) => "character",
        Kind::Integer(_) => "integer",
        Kind::Real(_) => "real",
        Kind::Complex { .. } => "complex",
        Kind::String(_) => "string",
        Kind::StaticAddress(_) => "static_address",
        Kind::Unit => "unit",
        Kind::NullableAbsent => "nullable_absent",
        Kind::NullablePresent(_) => "nullable_present",
        Kind::Tuple(_) => "tuple",
        Kind::Array(_) => "array",
        Kind::Product(_) => "product",
        Kind::Union { .. } => "union",
    }
}

pub(crate) const fn semantic_type_kind(ty: &bray_symbols::TypeData) -> &'static str {
    use bray_symbols::TypeData as Type;

    match ty {
        Type::Error => "error",
        Type::Named { .. } => "named",
        Type::TypeParameter(_) => "type_parameter",
        Type::ContextualSelf(_) => "contextual_self",
        Type::TypeValuedMemberProjection { .. } => "type_valued_member_projection",
        Type::Tuple(_) => "tuple",
        Type::Array { .. } => "array",
        Type::FlexibleArray(_) => "flexible_array",
        Type::Slice(_) => "slice",
        Type::Generator(_) => "generator",
        Type::Nullable(_) => "nullable",
        Type::Borrow { .. } => "borrow",
        Type::TraitView(_) => "trait_view",
        Type::OwnedIndirection { .. } => "owned_indirection",
        Type::Callable(_) => "callable",
    }
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
