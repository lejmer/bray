use std::hash::Hash;

use bray_base::StableDigestHasher;
use bray_diagnostics::{DiagnosticFailureField, DiagnosticFailureValue};

pub(crate) fn text_field(name: &'static str, value: impl Into<String>) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Text(value.into()))
}

pub(crate) fn text_list_field(
    name: &'static str,
    values: impl IntoIterator<Item = impl Into<String>>,
) -> DiagnosticFailureField {
    DiagnosticFailureField::new(
        name,
        DiagnosticFailureValue::TextList(
            values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ),
    )
}

pub(crate) fn identity_field(name: &'static str, value: &impl Hash) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Identity(identity(value)))
}

pub(crate) fn identity(value: &impl Hash) -> [u8; 32] {
    let mut hasher = StableDigestHasher::new();

    value.hash(&mut hasher);

    hasher.finalize()
}

pub(crate) fn identity_list_field<'a, T: Hash + 'a>(
    name: &'static str,
    values: impl IntoIterator<Item = &'a T>,
) -> DiagnosticFailureField {
    DiagnosticFailureField::new(
        name,
        DiagnosticFailureValue::IdentityList(
            values
                .into_iter()
                .map(identity)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ),
    )
}

pub(crate) fn path_field(name: &'static str, value: &std::path::Path) -> DiagnosticFailureField {
    DiagnosticFailureField::new(
        name,
        // Diagnostic fields own paths beyond the query-key borrow.
        DiagnosticFailureValue::Path(value.to_path_buf()),
    )
}

pub(crate) fn natural_field(name: &'static str, value: usize) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Natural(value.to_string()))
}

pub(crate) fn count_field(name: &'static str, value: u64) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Count(value))
}

pub(crate) fn boolean_field(name: &'static str, value: bool) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Boolean(value))
}

pub(crate) const fn target_endianness(value: bray_target::Endianness) -> &'static str {
    match value {
        bray_target::Endianness::Little => "little",
        bray_target::Endianness::Big => "big",
    }
}

pub(crate) const fn product_kind(value: bray_symbols::ProductKind) -> &'static str {
    match value {
        bray_symbols::ProductKind::Executable => "executable",
        bray_symbols::ProductKind::Library => "library",
        bray_symbols::ProductKind::Test => "test",
    }
}

pub(crate) const fn constant_value_kind(kind: &bray_symbols::ConstantValueKind) -> &'static str {
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
        Kind::Borrowed(_) => "borrowed",
        Kind::Unit => "unit",
        Kind::NullableAbsent => "nullable_absent",
        Kind::NullablePresent(_) => "nullable_present",
        Kind::Tuple(_) => "tuple",
        Kind::Array(_) => "array",
        Kind::Product(_) => "product",
        Kind::Union { .. } => "union",
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

    context.push(count_field(
        identity_name,
        u64::from(symbol.symbol_id().raw()),
    ));
}
