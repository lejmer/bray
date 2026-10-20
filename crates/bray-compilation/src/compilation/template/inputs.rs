use bray_binder::BindingQueryContext;
use bray_bound_tree::{BoundReferenceTarget, CheckedTemplateInputKind};
use bray_symbols::{ CallableSignatureTemplate, GenericParameterSymbolId, TypeData, TypeExpressionTemplate, TypeId};

use super::SourceTemplateInput;
use crate::compilation::Compilation;
use crate::fact::FactQueryError;

/// Constructs observation inputs after the declaration signature has passed checking.
pub(in crate::compilation) fn callable_template_inputs(
    compilation: &Compilation,
    signature: &CallableSignatureTemplate,
    generic_parameters: Vec<GenericParameterSymbolId>,
) -> Result<Vec<SourceTemplateInput>, FactQueryError> {
    let mut inputs = Vec::new();

    if let Some(receiver) = signature.receiver() {
        inputs.push(SourceTemplateInput::new(CheckedTemplateInputKind::Receiver,
            Some(BoundReferenceTarget::Surface(receiver.parameter().into())), receiver.ty()));
    }

    let values = compilation.semantic_value_store()?;
    let types = signature.parameter_type_templates(values)
        .expect("checked callable signature exposes its parameter type templates");

    for (ordinal, (parameter, ty)) in signature.parameters().iter().zip(&types).enumerate() {
        inputs.push(SourceTemplateInput::new(CheckedTemplateInputKind::Parameter(
            bray_symbols::SymbolOrdinal::new(u32::try_from(ordinal).expect("declaration parameter ordinal fits its symbol table"))),
            Some(BoundReferenceTarget::Surface((*parameter).into())), resolved_input_type(compilation, ty)?));
    }

    inputs.extend(generic_template_inputs(compilation, generic_parameters)?);

    Ok(inputs)
}

pub(in crate::compilation) fn generic_template_inputs(
    compilation: &Compilation,
    parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
) -> Result<Vec<SourceTemplateInput>, FactQueryError> {
    let context = compilation.binding_context(&compilation.state.cancellation)?;
    let values = compilation.semantic_value_store()?;

    parameters.into_iter().map(|parameter| {
        let symbol = parameter.into_any();
        // Template inputs retain the shared stable key beyond the binding view.
        let key = context.symbol_key(symbol).map_err(crate::compilation::binder::binding_query_error)?
            .expect("checked generic template input has stable identity").clone();

        let (kind, target, ty) = match parameter {
            GenericParameterSymbolId::Type(parameter) => (
                CheckedTemplateInputKind::GenericType(key), None,
                values.intern_type(TypeData::TypeParameter(parameter))?,
            ),
            GenericParameterSymbolId::Const(_) => {
                let template = compilation.symbol_type_template(symbol)?
                    .expect("checked generic constant input has its declared type");

                assert!(!template.diagnostics().has_errors(), "checked generic constant input has a valid type");

                (CheckedTemplateInputKind::GenericConstant(key), Some(BoundReferenceTarget::Surface(symbol)),
                    resolved_input_type(compilation, template.value())?)
            }
        };

        Ok(SourceTemplateInput::new(kind, target, ty))
    }).collect()
}

pub(in crate::compilation) fn resolved_input_type(compilation: &Compilation, template: &TypeExpressionTemplate) -> Result<TypeId, FactQueryError> {
    let constants = compilation.checked_constant_terms(template)?;

    assert!(!constants.diagnostics().has_errors(), "checked template input has valid constant type arguments");

    Ok(bray_checker::resolve_type_expression_template(compilation.semantic_value_store()?, template, constants.value())
        .expect("checked template input type resolves from its constant terms"))
}
