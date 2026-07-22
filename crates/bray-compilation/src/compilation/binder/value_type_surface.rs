use bray_binder::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};
use bray_bound_tree::{
    BoundReferenceTarget, BoundUnitKind, BoundUnitRoot, DeclaredValueTypeConstraintKind,
    DeclaredValueTypeTerm,
};
use bray_symbols::{
    AnySymbolId, CallableParameterSymbolId, CallableSignatureFact, CallableSignatureTemplate,
    CallableSymbolId, ConstantDeclaredTypeFact, ConstantSymbolId,
    GenericConstParameterDeclaredTypeFact, PredicateDefinitionSymbolId,
    PredicateSignatureTemplateFact, StructFieldTypeFact, SymbolFactRequest,
    TraitConstantFulfillmentDeclaredTypeFact, TraitConstantMemberDeclaredTypeFact,
    TypeExpressionTemplate, UnionPayloadFieldTypeFact,
};
use bray_syntax::LambdaExpressionSyntax;

use super::CompilationBinderFacts;
use super::symbol::{type_binder, visible_generic_const_parameters};
use super::value_type::{DeclaredValueTypeBinding, local_value};

impl DeclaredValueTypeBinding<'_> {
    pub(super) fn bind_visible_generic_const_parameters(&mut self) -> BinderFactResult<()> {
        for parameter in visible_generic_const_parameters(self.context.symbols(), self.owner) {
            self.check_cancellation()?;

            let result = self.context.symbol_fact(SymbolFactRequest::<
                GenericConstParameterDeclaredTypeFact,
            >::new(parameter))?;

            self.add_evidence(
                surface_value(parameter.into()),
                owned_template(result.value()),
            );
        }

        Ok(())
    }

    pub(super) fn bind_unit_surface(&mut self) -> BinderFactResult<()> {
        match self.unit.key().kind() {
            BoundUnitKind::CallableBody => self.bind_callable_surface(self.owner, true),
            BoundUnitKind::AnonymousCallable => self.bind_anonymous_callable_surface(),
            BoundUnitKind::RuntimeDefault => self.bind_runtime_default_surface(),
            BoundUnitKind::ConstantTemplate => self.bind_constant_surface(),
            BoundUnitKind::PredicateDefinition => self.bind_predicate_surface(),
            BoundUnitKind::Constraint => Ok(()),
            BoundUnitKind::ContractClause => self.bind_contract_surface(),
        }
    }

    fn bind_callable_surface(
        &mut self,
        owner: AnySymbolId,
        supplies_result_expectation: bool,
    ) -> BinderFactResult<()> {
        let callable =
            CallableSymbolId::try_from_any(owner).ok_or(BinderFactError::DependencyUnavailable)?;

        let result = self
            .context
            .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(callable))?;

        let signature = result.value();

        if let Some(receiver) = signature.receiver() {
            self.add_evidence(
                surface_value(receiver.parameter().into()),
                TypeExpressionTemplate::Resolved(receiver.ty()),
            );
        }

        for (parameter, ty) in callable_parameter_templates(self.context, signature)? {
            self.add_evidence(surface_value(parameter.into()), ty);
        }

        if supplies_result_expectation {
            self.callable_result = Some(owned_template(signature.result()));
        }

        Ok(())
    }

    fn bind_anonymous_callable_surface(&mut self) -> BinderFactResult<()> {
        let BoundUnitRoot::AnonymousCallable { callable, .. } = self.unit.root() else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        let symbol = self
            .unit
            .local_symbols()
            .anonymous_callable(callable)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let syntax = self
            .unit
            .key()
            .source()
            .syntax()
            .find_descendant::<LambdaExpressionSyntax>(self.context.syntax())
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let callable_type =
            type_binder(self.context, self.owner)?.bind_anonymous_callable_type(&syntax)?;
        let (callable_type, diagnostics) = callable_type.into_parts();

        self.diagnostics.add_range(diagnostics);
        self.callable_type = Some(callable_type);

        let parameters = syntax.parameter_list().parameters().collect::<Vec<_>>();

        if parameters.len() != symbol.parameters().len() {
            return Err(BinderFactError::DependencyUnavailable);
        }

        for (parameter, syntax) in symbol.parameters().iter().copied().zip(parameters) {
            let ty = self.bind_type_syntax(&syntax.type_expression())?;

            self.add_evidence(local_value(parameter.into()), ty);
        }

        self.callable_result = Some(match syntax.callable_result_clause() {
            Some(result) => self.bind_type_syntax(&result.type_expression())?,
            None => {
                let result =
                    type_binder(self.context, self.owner)?.bind_omitted_callable_result()?;

                let (template, diagnostics) = result.into_parts();

                self.diagnostics.add_range(diagnostics);

                template
            }
        });

        Ok(())
    }

    fn bind_runtime_default_surface(&mut self) -> BinderFactResult<()> {
        let declaration = self
            .context
            .symbols()
            .runtime_default_subject(self.owner)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let template = self.declared_surface_value_type(declaration)?;

        let BoundUnitRoot::Expression(expression) = self.unit.root() else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        let value = surface_value(declaration);

        self.add_evidence(value, template);

        self.add_constraint(
            DeclaredValueTypeConstraintKind::Initializer,
            DeclaredValueTypeTerm::Expression(expression),
            value,
        );

        Ok(())
    }

    fn bind_constant_surface(&mut self) -> BinderFactResult<()> {
        let template = self.constant_declared_type(self.owner)?;

        let BoundUnitRoot::Expression(initializer) = self.unit.root() else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        let value = surface_value(self.owner);

        self.add_evidence(value, template);

        self.add_constraint(
            DeclaredValueTypeConstraintKind::Initializer,
            DeclaredValueTypeTerm::Expression(initializer),
            value,
        );

        Ok(())
    }

    fn bind_predicate_surface(&mut self) -> BinderFactResult<()> {
        let predicate = PredicateDefinitionSymbolId::try_from_any(self.owner)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let result =
            self.context
                .symbol_fact(SymbolFactRequest::<PredicateSignatureTemplateFact>::new(
                    predicate,
                ))?;

        for parameter in result.value().parameters() {
            self.add_evidence(
                surface_value(parameter.parameter().into()),
                owned_template(parameter.ty()),
            );
        }

        Ok(())
    }

    fn bind_contract_surface(&mut self) -> BinderFactResult<()> {
        self.bind_callable_surface(self.owner, false)?;

        let result = self
            .unit
            .local_symbols()
            .scopes()
            .iter()
            .find_map(bray_symbols::LocalScope::postcondition_result);

        if let Some(result) = result {
            let signature = self.callable_signature(self.owner)?;
            let callable_result = owned_template(signature.value().result());

            self.add_evidence(local_value(result.into()), callable_result);
        }

        Ok(())
    }

    fn callable_signature(
        &self,
        owner: AnySymbolId,
    ) -> BinderFactResult<
        std::sync::Arc<bray_diagnostics::DiagnosticResult<CallableSignatureTemplate>>,
    > {
        let callable =
            CallableSymbolId::try_from_any(owner).ok_or(BinderFactError::DependencyUnavailable)?;

        self.context
            .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(callable))
    }

    fn declared_surface_value_type(
        &self,
        declaration: AnySymbolId,
    ) -> BinderFactResult<TypeExpressionTemplate> {
        match declaration {
            AnySymbolId::CallableParameter(parameter) => {
                let record = self
                    .context
                    .symbols()
                    .callable_parameter(parameter)
                    .ok_or(BinderFactError::DependencyUnavailable)?;

                let signature = self.callable_signature(record.owner().into_any())?;

                signature
                    .value()
                    .parameter_type_template(
                        parameter,
                        record.ordinal(),
                        self.context.semantic_values(),
                    )
                    .map_err(|_| BinderFactError::DependencyUnavailable)
            }
            AnySymbolId::StructField(field) => self
                .context
                .symbol_fact(SymbolFactRequest::<StructFieldTypeFact>::new(field))
                .map(|result| owned_template(result.value())),
            AnySymbolId::UnionPayloadField(field) => self
                .context
                .symbol_fact(SymbolFactRequest::<UnionPayloadFieldTypeFact>::new(field))
                .map(|result| owned_template(result.value())),
            _ => Err(BinderFactError::DependencyUnavailable),
        }
    }

    fn constant_declared_type(
        &self,
        owner: AnySymbolId,
    ) -> BinderFactResult<TypeExpressionTemplate> {
        match owner {
            AnySymbolId::Constant(constant) => self.constant_type(constant),
            AnySymbolId::TraitConstantMember(member) => self
                .context
                .symbol_fact(SymbolFactRequest::<TraitConstantMemberDeclaredTypeFact>::new(member))
                .map(|result| owned_template(result.value())),
            AnySymbolId::TraitConstantFulfillment(fulfillment) => self
                .context
                .symbol_fact(
                    SymbolFactRequest::<TraitConstantFulfillmentDeclaredTypeFact>::new(fulfillment),
                )
                .map(|result| owned_template(result.value())),
            _ => Err(BinderFactError::DependencyUnavailable),
        }
    }

    pub(super) fn bind_surface_reference_type(
        &mut self,
        target: BoundReferenceTarget,
    ) -> BinderFactResult<()> {
        let BoundReferenceTarget::Surface(symbol) = target else {
            return Ok(());
        };

        let template = match symbol {
            AnySymbolId::Constant(_)
            | AnySymbolId::TraitConstantMember(_)
            | AnySymbolId::TraitConstantFulfillment(_) => {
                Some(self.constant_declared_type(symbol)?)
            }
            _ => None,
        };

        if let Some(template) = template {
            self.add_evidence(surface_value(symbol), template);
        }

        Ok(())
    }

    fn constant_type(
        &self,
        constant: ConstantSymbolId,
    ) -> BinderFactResult<TypeExpressionTemplate> {
        self.context
            .symbol_fact(SymbolFactRequest::<ConstantDeclaredTypeFact>::new(constant))
            .map(|result| owned_template(result.value()))
    }
}

fn callable_parameter_templates(
    context: &CompilationBinderFacts<'_>,
    signature: &CallableSignatureTemplate,
) -> BinderFactResult<Vec<(CallableParameterSymbolId, TypeExpressionTemplate)>> {
    let types = signature
        .parameter_type_templates(context.semantic_values())
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    Ok(signature.parameters().iter().copied().zip(types).collect())
}

fn owned_template(template: &TypeExpressionTemplate) -> TypeExpressionTemplate {
    // Published facts own their templates independently; recursive storage remains Arc-shared.
    template.clone()
}

const fn surface_value(symbol: AnySymbolId) -> DeclaredValueTypeTerm {
    DeclaredValueTypeTerm::Value(BoundReferenceTarget::Surface(symbol))
}
