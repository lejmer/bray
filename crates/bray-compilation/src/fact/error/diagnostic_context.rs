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

pub(crate) fn push_mir_target_contract(
    context: &mut Vec<DiagnosticFailureField>,
    expected: bool,
    target: &bray_ir::MirTargetContract,
) {
    let machine = target.machine();
    let version = target.runtime_abi();

    let names = if expected {
        [
            "expected_target_identity",
            "expected_target_architecture",
            "expected_target_object_format",
            "expected_target_endianness",
            "expected_target_pointer_width_bits",
            "expected_target_pointer_alignment_bytes",
            "expected_target_stack_alignment_bytes",
            "expected_runtime_abi_major",
            "expected_runtime_abi_minor",
        ]
    } else {
        [
            "actual_target_identity",
            "actual_target_architecture",
            "actual_target_object_format",
            "actual_target_endianness",
            "actual_target_pointer_width_bits",
            "actual_target_pointer_alignment_bytes",
            "actual_target_stack_alignment_bytes",
            "actual_runtime_abi_major",
            "actual_runtime_abi_minor",
        ]
    };

    context.extend([
        text_field(names[0], target.identity().as_str()),
        text_field(names[1], machine.architecture().as_str()),
        text_field(names[2], machine.object_format().as_str()),
        text_field(
            names[3],
            match machine.endianness() {
                bray_target::Endianness::Little => "little",
                bray_target::Endianness::Big => "big",
            },
        ),
        count_field(names[4], u64::from(machine.pointer_width_bits().get())),
        count_field(names[5], u64::from(machine.pointer_alignment_bytes().get())),
        count_field(names[6], u64::from(machine.stack_alignment_bytes().get())),
        count_field(names[7], u64::from(version.major())),
        count_field(names[8], u64::from(version.minor())),
    ]);
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticFailureValue;

    use super::push_mir_target_contract;

    #[test]
    fn mir_target_contract_context_preserves_machine_and_runtime_abi_fields() {
        let target = bray_ir::MirTargetContract::new(
            bray_target::test_support::test_target_profile(),
            bray_runtime_interface::RuntimeAbiVersion::new(3, 7),
        );

        let mut fields = Vec::new();

        push_mir_target_contract(&mut fields, true, &target);

        let names: Vec<_> = fields.iter().map(|field| field.name()).collect();

        assert_eq!(
            names,
            [
                "expected_target_identity",
                "expected_target_architecture",
                "expected_target_object_format",
                "expected_target_endianness",
                "expected_target_pointer_width_bits",
                "expected_target_pointer_alignment_bytes",
                "expected_target_stack_alignment_bytes",
                "expected_runtime_abi_major",
                "expected_runtime_abi_minor",
            ]
        );

        assert_eq!(fields[7].value(), &DiagnosticFailureValue::Count(3));
        assert_eq!(fields[8].value(), &DiagnosticFailureValue::Count(7));
    }
}
