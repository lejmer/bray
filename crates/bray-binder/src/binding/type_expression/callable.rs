use bray_compiler_known::RepresentationRole;
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
use crate::{BinderFactError, BinderFactResult};

impl TypeExpressionBinder<'_> {
    /// Binds the callable type owned by one anonymous callable semantic unit.
    pub fn bind_anonymous_callable_type(
        mut self,
        syntax: &LambdaExpressionSyntax,
    ) -> BinderFactResult<DiagnosticResult<TypeExpressionTemplate>> {
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
    ) -> BinderFactResult<DiagnosticResult<CallableSignatureTemplate>> {
        self.check_cancellation()?;

        let (qualifiers, diagnostics) = qualifiers.into_parts();

        self.diagnostics.add_range(diagnostics);

        let parameters = parameter_list.parameters().collect::<Vec<_>>();

        let parameters_match_owner = parameter_symbols.iter().all(|parameter| {
            self.symbols
                .callable_parameter(*parameter)
                .is_some_and(|record| record.owner() == callable)
        });

        let receiver_matches_owner = receiver.is_none_or(|parameter| {
            self.symbols
                .receiver_parameter(parameter)
                .is_some_and(|record| record.owner() == callable)
        });

        if parameters.len() != parameter_symbols.len()
            || !parameters_match_owner
            || !receiver_matches_owner
            || receiver.is_some() != qualifiers.receiver_mode.is_some()
        {
            return Err(BinderFactError::DependencyUnavailable);
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
            _ => return Err(BinderFactError::DependencyUnavailable),
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
    ) -> BinderFactResult<TypeExpressionTemplate> {
        let parameters = syntax.parameter_lists().next();
        let modifiers = syntax.callable_modifiers().next();
        let directives = syntax.callable_directives().next();

        let result = syntax
            .callable_result_clauses()
            .next()
            .map(|result| result.type_expression());

        self.bind_callable_type_surface(
            parameters.as_ref(),
            result.as_ref(),
            modifiers.as_ref(),
            directives.as_ref(),
        )
    }

    fn bind_callable_type_surface(
        &mut self,
        parameters: Option<&ParameterListSyntax>,
        result: Option<&TypeExpressionSyntax>,
        modifiers: Option<&CallableModifiersSyntax>,
        directives: Option<&CallableDirectivesSyntax>,
    ) -> BinderFactResult<TypeExpressionTemplate> {
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

        self.make_callable_type_template(parameters, result, constness, trust, abi, dependencies)
    }

    fn make_callable_type_template(
        &self,
        parameters: Vec<CallableParameterTypeTemplate>,
        result: TypeExpressionTemplate,
        constness: CallableConstness,
        trust: CallableTrust,
        abi: bray_symbols::CallableAbi,
        dependencies: CallableDependencyContracts,
    ) -> BinderFactResult<TypeExpressionTemplate> {
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
                .collect::<BinderFactResult<Vec<_>>>()?;

            let result = self.require_resolved_type(&result)?;

            let callable =
                CallableTypeData::new(parameters, result, constness, trust, abi, dependencies);

            return self
                .intern_type(TypeData::Callable(callable))
                .map(TypeExpressionTemplate::Resolved);
        }

        Ok(TypeExpressionTemplate::Callable(CallableTypeTemplate::new(
            parameters,
            result,
            constness,
            trust,
            abi,
            dependencies,
        )))
    }

    fn bind_callable_parameter(
        &mut self,
        syntax: &ParameterSyntax,
    ) -> BinderFactResult<CallableParameterTypeTemplate> {
        let name = token_text(syntax.source(), &syntax.identifier_token())
            .and_then(CallableParameterName::try_new)
            .ok_or(BinderFactError::DependencyUnavailable)?;

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
    ) -> BinderFactResult<bray_symbols::DependencyContractTemplateId> {
        self.semantic_values
            .empty_dependency_contract_template()
            .map_err(|_| BinderFactError::DependencyUnavailable)
    }
}
