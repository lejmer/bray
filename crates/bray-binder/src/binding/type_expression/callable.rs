use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    CallableConstness, CallableDependencyContracts, CallableExecution, CallableParameterData,
    CallableParameterMode, CallableParameterName, CallableParameterSymbolId,
    CallableParameterTypeTemplate, CallablePosition, CallableSignatureTemplate, CallableSymbolId,
    CallableTrust, CallableTypeData, CallableTypeTemplate, ReceiverParameterSignature,
    ReceiverParameterSymbolId, TypeData, TypeExpressionTemplate,
};
use bray_syntax::{
    CallableDirectivesSyntax, CallableModifiersSyntax, LambdaExpressionSyntax, ParameterListSyntax,
    ParameterSyntax, SourceSyntaxNode, TypeExpressionSyntax,
};

use super::contract::CallableTypeQualifiers;
use super::core::{TypeExpressionBinder, token_text};
use super::diagnostic::source_diagnostic;
use crate::{BindingError, BindingQueryError, BindingQueryResult};

impl<Upstream> TypeExpressionBinder<'_, Upstream> {
    /// Binds the callable type owned by one anonymous callable semantic unit.
    pub fn bind_anonymous_callable_type(
        mut self,
        syntax: &LambdaExpressionSyntax,
    ) -> BindingQueryResult<DiagnosticResult<TypeExpressionTemplate>, Upstream> {
        self.check_cancellation()?;

        let result = syntax
            .callable_result_clause()
            .map(|result| result.type_expression());

        let ty = self.bind_callable_type_surface(
            Some(&syntax.parameter_list()),
            result.as_ref(),
            Some(&syntax.callable_modifiers()),
            Some(&syntax.callable_directives()),
        )?;

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(ty, self.diagnostics))
    }

    /// Binds one declaration callable signature through ordinary type-expression rules.
    pub fn bind_callable_signature(
        mut self,
        callable: CallableSymbolId,
        parameter_symbols: &[CallableParameterSymbolId],
        receiver: Option<ReceiverParameterSymbolId>,
        parameter_list: &ParameterListSyntax,
        result_type: Option<&TypeExpressionSyntax>,
        qualifiers: DiagnosticResult<CallableTypeQualifiers>,
    ) -> BindingQueryResult<DiagnosticResult<CallableSignatureTemplate>, Upstream> {
        self.check_cancellation()?;

        let (qualifiers, diagnostics) = qualifiers.into_parts();

        self.diagnostics.add_range(diagnostics);

        let parameters = parameter_list.parameters().collect::<Vec<_>>();
        let parameter_source = SyntaxAnchor::from_node(parameter_list);

        if parameters.len() != parameter_symbols.len() {
            return Err(BindingQueryError::Binding(
                BindingError::CallableParameterCountMismatch {
                    source: parameter_source,
                    callable,
                    syntax_count: parameters.len(),
                    symbol_count: parameter_symbols.len(),
                },
            ));
        }

        for parameter in parameter_symbols {
            let record =
                self.symbols
                    .callable_parameter(*parameter)
                    .ok_or(BindingQueryError::Binding(
                        BindingError::SymbolRecordUnavailable((*parameter).into()),
                    ))?;

            if record.owner() != callable {
                return Err(BindingQueryError::Binding(
                    BindingError::CallableParameterOwnerMismatch {
                        source: parameter_source,
                        callable,
                        parameter: *parameter,
                    },
                ));
            }
        }

        if let Some(receiver) = receiver {
            let record =
                self.symbols
                    .receiver_parameter(receiver)
                    .ok_or(BindingQueryError::Binding(
                        BindingError::SymbolRecordUnavailable(receiver.into()),
                    ))?;

            if record.owner() != callable {
                return Err(BindingQueryError::Binding(
                    BindingError::ReceiverParameterOwnerMismatch {
                        source: parameter_source,
                        callable,
                        receiver,
                    },
                ));
            }
        }

        if receiver.is_some() != qualifiers.receiver_mode.is_some() {
            return Err(receiver_context_error(
                callable,
                receiver,
                qualifiers.receiver_mode,
                self.self_type,
                parameter_source,
            ));
        }

        let mut signature_parameters = Vec::with_capacity(parameters.len());
        let mut callable_parameters = Vec::with_capacity(parameters.len());

        for (syntax, symbol) in parameters.iter().zip(parameter_symbols.iter().copied()) {
            let parameter = self.bind_callable_parameter(syntax)?;

            signature_parameters.push(symbol);
            callable_parameters.push(parameter);
        }

        let result = match result_type {
            Some(result) => self.bind_type(result)?,
            None => self.bind_compiler_known_type(RepresentationRole::Unit)?,
        };

        let receiver = match (receiver, qualifiers.receiver_mode, self.self_type) {
            (Some(parameter), Some(mode), Some(context)) => {
                let ty = self.intern_type(TypeData::ContextualSelf(context))?;

                Some(ReceiverParameterSignature::new(parameter, ty, mode))
            }
            (None, None, _) => None,
            _ => {
                return Err(receiver_context_error(
                    callable,
                    receiver,
                    qualifiers.receiver_mode,
                    self.self_type,
                    parameter_source,
                ));
            }
        };

        let dependency_contract = self.empty_dependency_contract()?;

        let dependencies = CallableDependencyContracts::for_execution(
            qualifiers.execution,
            dependency_contract,
            dependency_contract,
        );

        // The signature and callable identity share the same immutable result structure.
        let signature_result = result.clone();

        let callable_type = self.make_callable_type_template(
            callable_parameters,
            parameter_list.ellipsis_token().is_some(),
            result,
            qualifiers.constness,
            qualifiers.trust,
            qualifiers.abi,
            dependencies,
        )?;

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(
            CallableSignatureTemplate::new(
                callable_type,
                receiver,
                signature_parameters,
                signature_result,
            ),
            self.diagnostics,
        ))
    }

    pub(super) fn bind_callable_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let parameters = syntax.parameter_lists().next();
        let modifiers = syntax.callable_modifiers().next();
        let directives = syntax.callable_directives().next();

        let result = syntax
            .callable_result_clauses()
            .next()
            .map(|result| result.type_expression());

        let ty = self.bind_callable_type_surface(
            parameters.as_ref(),
            result.as_ref(),
            modifiers.as_ref(),
            directives.as_ref(),
        )?;

        let variadic = parameters
            .as_ref()
            .is_some_and(|parameters| parameters.ellipsis_token().is_some());

        let abi =
            callable_template_abi(self.semantic_values, &ty, SyntaxAnchor::from_node(syntax))?;

        let fixed_parameters = parameters
            .as_ref()
            .map_or(0, |parameters| parameters.parameters().count());

        if variadic
            && (fixed_parameters == 0
                || abi == bray_symbols::CallableAbi::Bray
                || modifiers.as_ref().is_some_and(|value| {
                    value.async_token().is_some() || value.const_token().is_some()
                })
                || parameters.as_ref().is_some_and(|parameters| {
                    parameters
                        .parameters()
                        .any(|parameter| parameter.equals_token().is_some())
                }))
        {
            self.diagnostics.add(source_diagnostic(
                syntax,
                bray_diagnostics::DiagnosticKind::CheckingVariadicCallableContractUnsupported,
            ));
        }

        Ok(ty)
    }

    fn bind_callable_type_surface(
        &mut self,
        parameters: Option<&ParameterListSyntax>,
        result: Option<&TypeExpressionSyntax>,
        modifiers: Option<&CallableModifiersSyntax>,
        directives: Option<&CallableDirectivesSyntax>,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let variadic = parameters.is_some_and(|parameters| parameters.ellipsis_token().is_some());

        let parameters = parameters
            .into_iter()
            .flat_map(ParameterListSyntax::parameters)
            .map(|parameter| self.bind_callable_parameter(&parameter))
            .collect::<Result<Vec<_>, _>>()?;

        let result = match result {
            Some(result) => self.bind_type(result)?,
            None => self.bind_compiler_known_type(RepresentationRole::Unit)?,
        };

        let constness = if modifiers.is_some_and(|value| value.const_token().is_some()) {
            CallableConstness::Constant
        } else {
            CallableConstness::Runtime
        };

        let execution = if modifiers.is_some_and(|value| value.async_token().is_some()) {
            CallableExecution::Asynchronous
        } else {
            CallableExecution::Synchronous
        };

        let trust = if modifiers.is_some_and(|value| value.trusted_token().is_some()) {
            CallableTrust::Trusted
        } else {
            CallableTrust::Safe
        };

        let abi = self.bind_optional_callable_abi(directives);
        let dependency_contract = self.empty_dependency_contract()?;

        let dependencies = CallableDependencyContracts::for_execution(
            execution,
            dependency_contract,
            dependency_contract,
        );

        self.make_callable_type_template(
            parameters,
            variadic,
            result,
            constness,
            trust,
            abi,
            dependencies,
        )
    }

    fn make_callable_type_template(
        &self,
        parameters: Vec<CallableParameterTypeTemplate>,
        variadic: bool,
        result: TypeExpressionTemplate,
        constness: CallableConstness,
        trust: CallableTrust,
        abi: bray_symbols::CallableAbi,
        dependencies: CallableDependencyContracts,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let parameters_are_resolved = parameters
            .iter()
            .all(|parameter| parameter.ty().resolved_type().is_some());

        if parameters_are_resolved && result.resolved_type().is_some() {
            let parameters = parameters
                .into_iter()
                .map(|parameter| {
                    let (name, position, mode, ty) = parameter.into_parts();

                    let ty = self.require_resolved_type(&ty)?;

                    Ok(CallableParameterData::new(name, position, mode, ty))
                })
                .collect::<BindingQueryResult<Vec<_>, Upstream>>()?;

            let result = self.require_resolved_type(&result)?;

            let callable =
                CallableTypeData::new(parameters, result, constness, trust, abi, dependencies)
                    .with_variadic(variadic);

            return self
                .intern_type(TypeData::Callable(callable))
                .map(TypeExpressionTemplate::Resolved);
        }

        Ok(TypeExpressionTemplate::Callable(
            CallableTypeTemplate::new(parameters, result, constness, trust, abi, dependencies)
                .with_variadic(variadic),
        ))
    }

    fn bind_callable_parameter(
        &mut self,
        syntax: &ParameterSyntax,
    ) -> BindingQueryResult<CallableParameterTypeTemplate, Upstream> {
        let name = token_text(syntax.source(), &syntax.identifier_token())
            .and_then(CallableParameterName::try_new)
            .ok_or(BindingQueryError::Binding(BindingError::SyntaxContract(
                SyntaxAnchor::from_node(syntax),
            )))?;

        let modifiers = syntax.parameter_modifiers();

        let position = if modifiers.pos_token().is_some() {
            CallablePosition::PositionalOrNamed
        } else {
            CallablePosition::NamedOnly
        };

        let mode = if modifiers.mut_token().is_some() {
            CallableParameterMode::Mutable
        } else {
            CallableParameterMode::Immutable
        };

        let ty = self.bind_type(&syntax.type_expression())?;

        Ok(CallableParameterTypeTemplate::new(name, position, mode, ty))
    }

    fn empty_dependency_contract(
        &self,
    ) -> BindingQueryResult<bray_symbols::DependencyContractTemplateId, Upstream> {
        self.semantic_values
            .empty_dependency_contract_template()
            .map_err(BindingQueryError::SemanticValue)
    }
}

fn callable_template_abi<Upstream>(
    values: &bray_symbols::SemanticValueStore,
    template: &TypeExpressionTemplate,
    source: SyntaxAnchor,
) -> BindingQueryResult<bray_symbols::CallableAbi, Upstream> {
    match template {
        TypeExpressionTemplate::Callable(callable) => Ok(callable.abi()),
        TypeExpressionTemplate::Resolved(ty) => {
            let data = values
                .type_data(*ty)
                .map_err(BindingQueryError::SemanticValue)?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(BindingQueryError::Binding(
                    BindingError::CallableTypeExpected { source, ty: *ty },
                ));
            };

            Ok(callable.abi())
        }
        _ => Err(BindingQueryError::Binding(
            BindingError::CallableTypeTemplateExpected(source),
        )),
    }
}

fn receiver_context_error<Upstream>(
    callable: CallableSymbolId,
    receiver: Option<ReceiverParameterSymbolId>,
    mode: Option<bray_symbols::ReceiverMode>,
    self_type: Option<bray_symbols::SelfTypeContext>,
    source: SyntaxAnchor,
) -> BindingQueryError<Upstream> {
    BindingQueryError::Binding(BindingError::ReceiverContextMismatch {
        source,
        callable,
        receiver_present: receiver.is_some(),
        mode_present: mode.is_some(),
        self_type_present: self_type.is_some(),
    })
}
