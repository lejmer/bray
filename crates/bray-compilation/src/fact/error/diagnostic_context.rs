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

pub(crate) fn push_semantic_type_data(
    context: &mut Vec<DiagnosticFailureField>,
    ty: &bray_symbols::TypeData,
) {
    use bray_symbols::TypeData as Type;

    context.extend([
        text_field("actual_semantic_type_kind", semantic_type_kind(ty)),
        identity_field("actual_semantic_type", ty),
    ]);

    match ty {
        Type::Error => {}
        Type::Named {
            definition,
            substitution,
        } => context.extend([
            identity_field("actual_type_definition", definition),
            identity_field("actual_type_substitution", substitution),
        ]),
        Type::TypeParameter(parameter) => {
            context.push(identity_field("actual_type_parameter", parameter));
        }
        Type::ContextualSelf(contextual) => {
            context.push(identity_field("actual_self_context", contextual));
        }
        Type::TypeValuedMemberProjection {
            subject,
            application,
            member,
        } => context.extend([
            identity_field("actual_projection_subject", subject),
            identity_field("actual_projection_application", application),
            identity_field("actual_projection_member", member),
        ]),
        Type::Tuple(elements) => {
            context.push(identity_list_field("actual_tuple_elements", elements.iter()));
        }
        Type::Array { element, length } => context.extend([
            identity_field("actual_array_element", element),
            identity_field("actual_array_length", length),
        ]),
        Type::FlexibleArray(element) => {
            context.push(identity_field("actual_flexible_array_element", element));
        }
        Type::Slice(element) => {
            context.push(identity_field("actual_slice_element", element));
        }
        Type::Generator(element) => {
            context.push(identity_field("actual_generator_element", element));
        }
        Type::Nullable(target) => {
            context.push(identity_field("actual_nullable_target", target));
        }
        Type::Borrow { kind, target } => context.extend([
            text_field("actual_borrow_kind", kind.as_str()),
            identity_field("actual_borrow_target", target),
        ]),
        Type::TraitView(application) => {
            context.push(identity_field("actual_trait_application", application));
        }
        Type::OwnedIndirection { storage, target } => context.extend([
            identity_field("actual_owned_storage", storage),
            identity_field("actual_owned_target", target),
        ]),
        Type::Callable(callable) => context.extend([
            natural_field("actual_callable_parameter_count", callable.parameters().len()),
            boolean_field("actual_callable_variadic", callable.is_variadic()),
            identity_field("actual_callable_result", &callable.result()),
            identity_field("actual_callable_contract", callable),
        ]),
    }
}

pub(crate) fn push_type_template_data(
    context: &mut Vec<DiagnosticFailureField>,
    template: &bray_symbols::TypeExpressionTemplate,
) {
    use bray_symbols::TypeExpressionTemplate as Template;

    context.extend([
        text_field("actual_type_template_kind", template.kind_name()),
        identity_field("actual_type_template", template),
    ]);

    match template {
        Template::Resolved(ty) => {
            context.push(identity_field("actual_resolved_type", ty));
        }
        Template::Named {
            definition,
            parameters,
            arguments,
        } => context.extend([
            identity_field("actual_type_definition", definition),
            identity_list_field("actual_generic_parameters", parameters.iter()),
            identity_list_field("actual_generic_arguments", arguments.iter()),
        ]),
        Template::CallableContract {
            definition,
            target,
            parameters,
            arguments,
        } => context.extend([
            identity_field("actual_callable_contract_definition", definition),
            identity_field("actual_callable_contract_target", target),
            identity_list_field("actual_generic_parameters", parameters.iter()),
            identity_list_field("actual_generic_arguments", arguments.iter()),
        ]),
        Template::TypeValuedMemberProjection {
            subject,
            application,
            member,
        } => context.extend([
            identity_field("actual_projection_subject", subject),
            identity_field("actual_projection_application", application),
            identity_field("actual_projection_member", member),
        ]),
        Template::Tuple(elements) => {
            context.push(identity_list_field("actual_tuple_elements", elements.iter()));
        }
        Template::Array { element, length } => context.extend([
            identity_field("actual_array_element", element),
            identity_field("actual_array_length", length),
        ]),
        Template::FlexibleArray(element) => {
            context.push(identity_field("actual_flexible_array_element", element));
        }
        Template::Slice(element) => {
            context.push(identity_field("actual_slice_element", element));
        }
        Template::Nullable(target) => {
            context.push(identity_field("actual_nullable_target", target));
        }
        Template::Borrow { kind, target } => context.extend([
            text_field("actual_borrow_kind", kind.as_str()),
            identity_field("actual_borrow_target", target),
        ]),
        Template::TraitView(application) => {
            context.push(identity_field("actual_trait_application", application));
        }
        Template::OwnedIndirection { storage, target } => context.extend([
            identity_field("actual_owned_storage", storage),
            identity_field("actual_owned_target", target),
        ]),
        Template::Callable(callable) => context.extend([
            natural_field("actual_callable_parameter_count", callable.parameters().len()),
            boolean_field("actual_callable_variadic", callable.is_variadic()),
            identity_field("actual_callable_result", callable.result()),
            identity_field("actual_callable_contract", callable),
        ]),
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

#[cfg(test)]
mod tests {
    #[test]
    fn type_data_and_templates_emit_shared_variant_components() {
        let mut type_fields = Vec::new();
        let ty = bray_symbols::TypeData::tuple([]);

        super::push_semantic_type_data(&mut type_fields, &ty);

        assert_eq!(
            type_fields
                .iter()
                .map(|field| field.name())
                .collect::<Vec<_>>(),
            [
                "actual_semantic_type_kind",
                "actual_semantic_type",
                "actual_tuple_elements",
            ]
        );

        let mut template_fields = Vec::new();

        let template = bray_symbols::TypeExpressionTemplate::Tuple(
            Vec::new().into_boxed_slice().into(),
        );

        super::push_type_template_data(&mut template_fields, &template);

        assert_eq!(
            template_fields
                .iter()
                .map(|field| field.name())
                .collect::<Vec<_>>(),
            [
                "actual_type_template_kind",
                "actual_type_template",
                "actual_tuple_elements",
            ]
        );
    }
}
