use bray_binder::BindingQueryContext;
use bray_bound_tree::{BoundReferenceTarget, CheckedTemplateInputKind};
use bray_symbols::{
    CallableSignatureTemplate, GenericParameterSymbolId, TypeData, TypeExpressionTemplate, TypeId,
};

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
        inputs.push(SourceTemplateInput::new(
            CheckedTemplateInputKind::Receiver,
            Some(BoundReferenceTarget::Surface(receiver.parameter().into())),
            receiver.ty(),
        ));
    }

    let values = compilation.semantic_value_store()?;

    let types = signature
        .parameter_type_templates(values)
        .expect("checked callable signature exposes its parameter type templates");

    for (ordinal, (parameter, ty)) in signature.parameters().iter().zip(&types).enumerate() {
        inputs.push(SourceTemplateInput::new(
            CheckedTemplateInputKind::Parameter(bray_symbols::SymbolOrdinal::new(
                u32::try_from(ordinal)
                    .expect("declaration parameter ordinal fits its symbol table"),
            )),
            Some(BoundReferenceTarget::Surface((*parameter).into())),
            resolved_input_type(compilation, ty)?,
        ));
    }

    inputs.extend(generic_template_inputs(compilation, generic_parameters)?);

    Ok(inputs)
}

pub(in crate::compilation) fn predicate_template_inputs(
    compilation: &Compilation,
    signature: &bray_symbols::PredicateSignatureTemplate,
    parameters: Vec<GenericParameterSymbolId>,
) -> Result<Vec<SourceTemplateInput>, FactQueryError> {
    let mut inputs = signature
        .parameters()
        .iter()
        .enumerate()
        .map(|(ordinal, parameter)| {
            Ok(SourceTemplateInput::new(
                CheckedTemplateInputKind::Parameter(bray_symbols::SymbolOrdinal::new(
                    u32::try_from(ordinal).expect("predicate parameter ordinal fits its signature"),
                )),
                Some(BoundReferenceTarget::Surface(parameter.parameter().into())),
                resolved_input_type(compilation, parameter.ty())?,
            ))
        })
        .collect::<Result<Vec<_>, FactQueryError>>()?;

    inputs.extend(generic_template_inputs(compilation, parameters)?);

    Ok(inputs)
}

pub(in crate::compilation) fn generic_template_inputs(
    compilation: &Compilation,
    parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
) -> Result<Vec<SourceTemplateInput>, FactQueryError> {
    let context = compilation.binding_context(&compilation.state.cancellation)?;
    let values = compilation.semantic_value_store()?;

    parameters
        .into_iter()
        .map(|parameter| {
            let symbol = parameter.into_any();

            // Template inputs retain the shared stable key beyond the binding view.
            let key = context
                .symbol_key(symbol)
                .map_err(crate::compilation::binder::binding_query_error)?
                .expect("checked generic template input has stable identity")
                .clone();

            let (kind, target, ty) = match parameter {
                GenericParameterSymbolId::Type(parameter) => (
                    CheckedTemplateInputKind::GenericType(key),
                    None,
                    values.intern_type(TypeData::TypeParameter(parameter))?,
                ),
                GenericParameterSymbolId::Const(_) => {
                    let template = compilation
                        .symbol_type_template(symbol)?
                        .expect("checked generic constant input has its declared type");

                    assert!(
                        !template.diagnostics().has_errors(),
                        "checked generic constant input has a valid type"
                    );

                    (
                        CheckedTemplateInputKind::GenericConstant(key),
                        Some(BoundReferenceTarget::Surface(symbol)),
                        resolved_input_type(compilation, template.value())?,
                    )
                }
            };

            Ok(SourceTemplateInput::new(kind, target, ty))
        })
        .collect()
}

pub(in crate::compilation) fn resolved_input_type(
    compilation: &Compilation,
    template: &TypeExpressionTemplate,
) -> Result<TypeId, FactQueryError> {
    let constants = compilation.checked_constant_terms(template)?;

    assert!(
        !constants.diagnostics().has_errors(),
        "checked template input has valid constant type arguments"
    );

    Ok(bray_checker::resolve_type_expression_template(
        compilation.semantic_value_store()?,
        template,
        constants.value(),
    )
    .expect("checked template input type resolves from its constant terms"))
}

impl Compilation {
    pub(in crate::compilation) fn default_template_inputs(
        &self,
        key: &bray_bound_tree::BoundUnitKey,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<Vec<SourceTemplateInput>, FactQueryError> {
        use bray_binder::SymbolQueryProvider;

        let symbols = self.symbol_graph()?;

        let provider = symbols
            .symbol_for_key(key.declared_owner())
            .expect("default unit retains its provider");

        let subject = symbols
            .runtime_default_subject(provider)
            .expect("default provider retains its declaration input");

        let parameters = crate::compilation::binder::visible_generic_parameters(symbols, subject);

        let bray_symbols::AnySymbolId::CallableParameter(parameter) = subject else {
            return generic_template_inputs(self, parameters);
        };

        let parameter = symbols
            .callable_parameter(parameter)
            .expect("source default retains its parameter");

        let context = self.binding_context(cancellation)?;

        let signature = context
            .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                bray_symbols::CallableSignatureQuery,
            >::new(parameter.owner()))
            .map_err(crate::compilation::binder::binding_query_error)?;

        let mut inputs = callable_template_inputs(self, signature.value(), parameters)?;

        inputs.retain(|input| {
            !matches!(input.kind, CheckedTemplateInputKind::Parameter(ordinal)
            if ordinal.raw() >= parameter.ordinal())
        });

        Ok(inputs)
    }

    pub(in crate::compilation) fn checked_default_template(
        &self,
        provider: bray_bound_tree::DefaultValueProvider,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<
        bray_diagnostics::DiagnosticResult<Option<bray_bound_tree::CheckedTemplate>>,
        FactQueryError,
    > {
        if let Some(key) = self.declared_unit_key(
            provider.symbol(),
            bray_bound_tree::BoundUnitKind::RuntimeDefault,
        )? {
            let result = self.constant_body_template(key, cancellation)?;

            // Both publications retain the same immutable, Arc-backed operation graph.
            return Ok(result.result().as_ref().clone());
        }

        let context = self.binding_context(cancellation)?;

        let address = context
            .imported_semantic_address(provider.symbol())
            .map_err(crate::compilation::binder::binding_query_error)?
            .expect("selected imported default retains its semantic address");

        let imported = crate::compilation::binder::imported_declaration_template(
            &context,
            address,
            bray_bound_tree::CheckedTemplateKind::RuntimeDefault,
        )
        .map_err(crate::compilation::binder::binding_query_error)?;

        Ok(imported.map(|template| template.map(|template| template.template().clone())))
    }
}
