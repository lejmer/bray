use crate::compilation::binder::BindingQueryResult;
use std::sync::Arc;

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{
    BoundReferenceTarget, BoundUnitKind, BoundUnitRoot, DeclaredValueTypeConstraintKind,
    DeclaredValueTypeTerm,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    AnySymbolId, CallableParameterSymbolId, CallableSignatureQuery, CallableSignatureTemplate,
    CallableSymbolId, ConstantDeclaredTypeQuery, ConstantExpressionExpectedType,
    ConstantExpressionOccurrenceKey, ConstantSymbolId, GenericArgumentTemplate,
    GenericConstParameterDeclaredTypeQuery, GenericParameterSymbolId, ImplementationSubjectQuery,
    NamedTypeSymbolId, PredicateDefinitionSymbolId, PredicateSignatureTemplateQuery,
    StaticDeclaredTypeQuery, StructFieldTypeQuery, SymbolProvider, SymbolQueryRequest,
    TraitConstantFulfillmentDeclaredTypeQuery, TraitConstantMemberDeclaredTypeQuery,
    TypeExpressionTemplate, UnionPayloadFieldTypeQuery,
};
use bray_syntax::{LambdaExpressionSyntax, StaticDeclarationSyntax};
use bray_target::TargetPropertyKind;

use super::symbol::{type_binder, visible_generic_const_parameters};
use super::value_type::{DeclaredValueTypeBinding, local_value};
use super::{CompilationBindingContext, semantic_contract_binding_error as binding_contract};
use crate::compilation::substitution::contextual_self_type;
use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryViolation, SemanticSymbolCategory,
};

impl DeclaredValueTypeBinding<'_> {
    pub(super) fn bind_visible_generic_const_parameters(&mut self) -> BindingQueryResult<()> {
        for parameter in visible_generic_const_parameters(self.context.symbols(), self.owner) {
            self.check_cancellation()?;

            let result = self.context.resolve_symbol_query(SymbolQueryRequest::<
                GenericConstParameterDeclaredTypeQuery,
            >::new(parameter))?;

            self.add_evidence(
                surface_value(parameter.into()),
                owned_template(result.value()),
            );
        }

        Ok(())
    }

    pub(super) fn bind_unit_surface(&mut self) -> BindingQueryResult<()> {
        match self.unit.key().kind() {
            BoundUnitKind::CallableBody => self.bind_callable_surface(self.owner, true),
            BoundUnitKind::AnonymousCallable => self.bind_anonymous_callable_surface(),
            BoundUnitKind::RuntimeDefault => self.bind_runtime_default_surface(),
            BoundUnitKind::ConstantTemplate => self.bind_constant_surface(),
            BoundUnitKind::EmbeddedConstant => self.bind_embedded_constant_surface(),
            BoundUnitKind::PredicateDefinition => self.bind_predicate_surface(),
            BoundUnitKind::Constraint => Ok(()),
            BoundUnitKind::ContractClause => self.bind_contract_surface(),
            BoundUnitKind::TargetGate => self.bind_target_gate_surface(),
        }
    }

    fn bind_callable_surface(
        &mut self,
        owner: AnySymbolId,
        supplies_result_expectation: bool,
    ) -> BindingQueryResult<()> {
        let callable = CallableSymbolId::try_from_any(owner).ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Symbol(owner),
                SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: SemanticSymbolCategory::Callable,
                    actual: owner.kind(),
                },
            )
        })?;

        let result = self
            .context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))?;

        let signature = result.value();

        if let Some(receiver) = signature.receiver() {
            let implementation = self
                .context
                .symbols()
                .containing_symbol(self.owner)
                .and_then(bray_symbols::ImplementationSymbolId::try_from_any);

            let receiver_type = match implementation {
                Some(implementation) => self.implementation_subject_type(implementation)?,
                None => {
                    let receiver_type = self
                        .context
                        .semantic_values()
                        .type_data(receiver.ty())
                        .map_err(super::semantic_value_binding_error)?;

                    match receiver_type.as_ref() {
                        bray_symbols::TypeData::ContextualSelf(
                            context @ bray_symbols::SelfTypeContext::NamedType(_),
                        ) => TypeExpressionTemplate::Resolved(
                            contextual_self_type(self.context, *context)
                                .map_err(super::symbol::binder_error)?,
                        ),
                        bray_symbols::TypeData::ContextualSelf(
                            bray_symbols::SelfTypeContext::Implementation(implementation),
                        ) => self.implementation_subject_type(*implementation)?,
                        _ => TypeExpressionTemplate::Resolved(receiver.ty()),
                    }
                }
            };

            self.add_evidence(surface_value(receiver.parameter().into()), receiver_type);
        }

        for (parameter, ty) in callable_parameter_templates(self.context, signature)? {
            let ty = self.callable_body_type(owner, &ty)?;
            self.add_evidence(surface_value(parameter.into()), ty);
        }

        if supplies_result_expectation {
            self.callable_result = Some(self.callable_body_type(owner, signature.result())?);
        }

        Ok(())
    }

    fn callable_body_type(
        &mut self,
        owner: AnySymbolId,
        result: &TypeExpressionTemplate,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let TypeExpressionTemplate::Resolved(ty) = result else {
            return Ok(owned_template(result));
        };

        if let Some(implementation) = self
            .context
            .symbols()
            .containing_symbol(owner)
            .and_then(bray_symbols::ImplementationSymbolId::try_from_any)
        {
            let subject = self.implementation_subject_type(implementation)?;

            if let TypeExpressionTemplate::Resolved(subject) = subject {
                return self
                    .context
                    .semantic_values()
                    .substitute_contextual_self(
                        *ty,
                        bray_symbols::SelfTypeContext::Implementation(implementation),
                        subject,
                    )
                    .map(TypeExpressionTemplate::Resolved)
                    .map_err(super::semantic_value_binding_error);
            }
        }

        if let Some(definition) = self
            .context
            .symbols()
            .containing_symbol(owner)
            .and_then(NamedTypeSymbolId::try_from_any)
        {
            let self_ty = contextual_self_type(
                self.context,
                bray_symbols::SelfTypeContext::NamedType(definition),
            )
            .map_err(super::symbol::binder_error)?;

            let self_data = self
                .context
                .semantic_values()
                .type_data(self_ty)
                .map_err(super::semantic_value_binding_error)?;

            let bray_symbols::TypeData::Named { substitution, .. } = self_data.as_ref() else {
                return Err(binding_contract(
                    SemanticQueryContext::Type(self_ty),
                    SemanticQueryViolation::Missing(SemanticDataKind::GenericSubstitution),
                ));
            };

            let result = self
                .context
                .semantic_values()
                .substitute_type(*ty, *substitution)
                .map_err(super::semantic_value_binding_error)?;

            return Ok(TypeExpressionTemplate::Resolved(result));
        }

        let data = self
            .context
            .semantic_values()
            .type_data(*ty)
            .map_err(super::semantic_value_binding_error)?;

        match data.as_ref() {
            bray_symbols::TypeData::ContextualSelf(
                context @ bray_symbols::SelfTypeContext::NamedType(_),
            ) => contextual_self_type(self.context, *context)
                .map(TypeExpressionTemplate::Resolved)
                .map_err(super::symbol::binder_error),
            bray_symbols::TypeData::ContextualSelf(
                bray_symbols::SelfTypeContext::Implementation(implementation),
            ) => self.implementation_subject_type(*implementation),
            _ => Ok(owned_template(result)),
        }
    }

    fn bind_anonymous_callable_surface(&mut self) -> BindingQueryResult<()> {
        let BoundUnitRoot::AnonymousCallable { callable, .. } = self.unit.root() else {
            return Err(binding_contract(
                SemanticQueryContext::Unit(self.unit.key().clone()),
                SemanticQueryViolation::Unsupported(SemanticDataKind::BoundUnit),
            ));
        };

        let symbol = self
            .unit
            .local_symbols()
            .anonymous_callable(callable)
            .ok_or_else(|| {
                binding_contract(
                    SemanticQueryContext::Unit(self.unit.key().clone()),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let syntax = self
            .unit
            .key()
            .source()
            .syntax()
            .find_descendant::<LambdaExpressionSyntax>(self.context.syntax())
            .ok_or_else(|| {
                binding_contract(
                    SemanticQueryContext::Unit(self.unit.key().clone()),
                    SemanticQueryViolation::Missing(SemanticDataKind::Syntax),
                )
            })?;

        let callable_type =
            type_binder(self.context, self.owner)?.bind_anonymous_callable_type(&syntax)?;

        let (callable_type, diagnostics) = callable_type.into_parts();

        self.diagnostics.add_range(diagnostics);
        self.callable_type = Some(callable_type);

        let parameters = syntax.parameter_list().parameters().collect::<Vec<_>>();

        if parameters.len() != symbol.parameters().len() {
            return Err(binding_contract(
                SemanticQueryContext::Unit(self.unit.key().clone()),
                SemanticQueryViolation::CountMismatch {
                    data: SemanticDataKind::CallableSignature,
                    expected: symbol.parameters().len(),
                    actual: parameters.len(),
                },
            ));
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

    fn implementation_subject_type(
        &mut self,
        implementation: bray_symbols::ImplementationSymbolId,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let subject = self.context.resolve_symbol_query(SymbolQueryRequest::<
            ImplementationSubjectQuery,
        >::new(implementation))?;

        self.diagnostics = self.diagnostics.merged(subject.diagnostics());

        Ok(owned_template(subject.value().ty()))
    }

    fn bind_runtime_default_surface(&mut self) -> BindingQueryResult<()> {
        let declaration = self
            .context
            .symbols()
            .runtime_default_subject(self.owner)
            .ok_or_else(|| {
                binding_contract(
                    SemanticQueryContext::Symbol(self.owner),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let template = self.declared_surface_value_type(declaration)?;

        let BoundUnitRoot::Expression(expression) = self.unit.root() else {
            return Err(binding_contract(
                SemanticQueryContext::Unit(self.unit.key().clone()),
                SemanticQueryViolation::Unsupported(SemanticDataKind::BoundExpression),
            ));
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

    fn bind_constant_surface(&mut self) -> BindingQueryResult<()> {
        let template = self.constant_declared_type(self.owner)?;

        let BoundUnitRoot::Expression(initializer) = self.unit.root() else {
            return Err(binding_contract(
                SemanticQueryContext::Unit(self.unit.key().clone()),
                SemanticQueryViolation::Unsupported(SemanticDataKind::BoundExpression),
            ));
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

    fn bind_embedded_constant_surface(&mut self) -> BindingQueryResult<()> {
        let BoundUnitRoot::Expression(expression) = self.unit.root() else {
            return Err(binding_contract(
                SemanticQueryContext::Unit(self.unit.key().clone()),
                SemanticQueryViolation::Unsupported(SemanticDataKind::BoundExpression),
            ));
        };

        let occurrence =
            ConstantExpressionOccurrenceKey::new(self.owner, self.unit.key().source().syntax());

        let expected = self
            .context
            .compilation()
            .embedded_constant_expected_type(occurrence)
            .map_err(super::symbol::binder_error)?;

        let template = match expected {
            ConstantExpressionExpectedType::Resolved(ty) => TypeExpressionTemplate::Resolved(ty),
            ConstantExpressionExpectedType::GenericParameter(parameter) => self
                .context
                .resolve_symbol_query(
                    SymbolQueryRequest::<GenericConstParameterDeclaredTypeQuery>::new(parameter),
                )
                .map(|result| owned_template(result.value()))?,
        };

        self.add_evidence(DeclaredValueTypeTerm::Expression(expression), template);

        Ok(())
    }

    fn bind_predicate_surface(&mut self) -> BindingQueryResult<()> {
        let predicate = PredicateDefinitionSymbolId::try_from_any(self.owner).ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Symbol(self.owner),
                SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: SemanticSymbolCategory::PredicateDefinition,
                    actual: self.owner.kind(),
                },
            )
        })?;

        let result = self.context.resolve_symbol_query(SymbolQueryRequest::<
            PredicateSignatureTemplateQuery,
        >::new(predicate))?;

        for parameter in result.value().parameters() {
            self.add_evidence(
                surface_value(parameter.parameter().into()),
                owned_template(parameter.ty()),
            );
        }

        Ok(())
    }

    fn bind_contract_surface(&mut self) -> BindingQueryResult<()> {
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

    fn bind_target_gate_surface(&mut self) -> BindingQueryResult<()> {
        let BoundUnitRoot::Expression(expression) = self.unit.root() else {
            return Err(binding_contract(
                SemanticQueryContext::Unit(self.unit.key().clone()),
                SemanticQueryViolation::Unsupported(SemanticDataKind::BoundExpression),
            ));
        };

        let boolean = self
            .context
            .compilation()
            .target_property_type(TargetPropertyKind::ScalarBool)
            .map_err(super::symbol::binder_error)?;

        self.add_evidence(
            DeclaredValueTypeTerm::Expression(expression),
            TypeExpressionTemplate::Resolved(boolean),
        );

        Ok(())
    }

    fn callable_signature(
        &self,
        owner: AnySymbolId,
    ) -> BindingQueryResult<Arc<bray_diagnostics::DiagnosticResult<CallableSignatureTemplate>>>
    {
        let callable = CallableSymbolId::try_from_any(owner).ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Symbol(owner),
                SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: SemanticSymbolCategory::Callable,
                    actual: owner.kind(),
                },
            )
        })?;

        self.context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))
    }

    fn declared_surface_value_type(
        &mut self,
        declaration: AnySymbolId,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        match declaration {
            AnySymbolId::CallableParameter(parameter) => {
                let record = self
                    .context
                    .symbols()
                    .callable_parameter(parameter)
                    .ok_or_else(|| {
                        binding_contract(
                            SemanticQueryContext::Symbol(parameter.into()),
                            SemanticQueryViolation::Missing(SemanticDataKind::DeclarationRecord),
                        )
                    })?;

                let signature = self.callable_signature(record.owner().into_any())?;

                let template = signature
                    .value()
                    .parameter_type_template(
                        parameter,
                        record.ordinal(),
                        self.context.semantic_values(),
                    )
                    .map_err(super::callable_signature_binding_error)?;

                self.callable_body_type(record.owner().into_any(), &template)
            }
            AnySymbolId::StructField(field) => self
                .context
                .resolve_symbol_query(SymbolQueryRequest::<StructFieldTypeQuery>::new(field))
                .map(|result| owned_template(result.value())),
            AnySymbolId::UnionPayloadField(field) => self
                .context
                .resolve_symbol_query(SymbolQueryRequest::<UnionPayloadFieldTypeQuery>::new(field))
                .map(|result| owned_template(result.value())),
            _ => Err(binding_contract(
                SemanticQueryContext::Symbol(declaration),
                SemanticQueryViolation::Unsupported(SemanticDataKind::TypeSurface),
            )),
        }
    }

    fn constant_declared_type(
        &self,
        owner: AnySymbolId,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        match owner {
            AnySymbolId::Constant(constant) => self.constant_type(constant),
            AnySymbolId::Static(static_symbol) => {
                let declared =
                    self.context.resolve_symbol_query(SymbolQueryRequest::<
                        StaticDeclaredTypeQuery,
                    >::new(static_symbol))?;

                self.static_reference_type(static_symbol, owned_template(declared.value()))
            }
            AnySymbolId::TraitConstantMember(member) => self
                .context
                .resolve_symbol_query(
                    SymbolQueryRequest::<TraitConstantMemberDeclaredTypeQuery>::new(member),
                )
                .map(|result| owned_template(result.value())),
            AnySymbolId::TraitConstantFulfillment(fulfillment) => self
                .context
                .resolve_symbol_query(SymbolQueryRequest::<
                    TraitConstantFulfillmentDeclaredTypeQuery,
                >::new(fulfillment))
                .map(|result| owned_template(result.value())),
            _ => Err(binding_contract(
                SemanticQueryContext::Symbol(owner),
                SemanticQueryViolation::Unsupported(SemanticDataKind::ConstantDefinition),
            )),
        }
    }

    fn static_reference_type(
        &self,
        declaration: bray_symbols::StaticSymbolId,
        declared: TypeExpressionTemplate,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let record = self
            .context
            .symbols()
            .static_symbol(declaration)
            .ok_or_else(|| {
                binding_contract(
                    SemanticQueryContext::Symbol(declaration.into()),
                    SemanticQueryViolation::Missing(SemanticDataKind::DeclarationRecord),
                )
            })?;

        let exposes_address = match record.syntax_anchor() {
            Some(anchor) => {
                let syntax = anchor
                    .find_descendant::<StaticDeclarationSyntax>(
                        self.context.compilation().syntax_tree(),
                    )
                    .ok_or_else(|| {
                        binding_contract(
                            SemanticQueryContext::Symbol(declaration.into()),
                            SemanticQueryViolation::Missing(SemanticDataKind::Syntax),
                        )
                    })?;

                syntax
                    .static_declaration_modifiers()
                    .extern_token()
                    .is_some()
                    || (syntax.mut_token().is_some()
                        && syntax
                            .static_directives()
                            .symbol_directives()
                            .next()
                            .is_some())
            }
            None => self
                .context
                .compilation()
                .imported_native_boundary_with_cancellation(
                    declaration.into(),
                    self.context.cancellation(),
                )
                .map_err(super::symbol::binder_error)?
                .is_some_and(|boundary| {
                    matches!(
                        boundary.kind(),
                        bray_package_interface::InterfaceNativeBoundaryKind::Static { .. }
                    )
                }),
        };

        if !exposes_address {
            return Ok(declared);
        }

        let available = self
            .context
            .compilation()
            .available_compiler_known_symbols();

        let raw_pointer = available
            .representation_symbol::<bray_symbols::StructSymbolId>(RepresentationRole::RawPointer)
            .ok_or_else(|| {
                binding_contract(
                    SemanticQueryContext::CompilerKnownRepresentation(
                        RepresentationRole::RawPointer,
                    ),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let raw_pointer = available.provider().symbol(raw_pointer).ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Symbol(raw_pointer.into()),
                SemanticQueryViolation::Missing(SemanticDataKind::DeclarationRecord),
            )
        })?;

        let [parameter] = raw_pointer.generic_type_parameters() else {
            return Err(binding_contract(
                SemanticQueryContext::Symbol(raw_pointer.id().into()),
                SemanticQueryViolation::CountMismatch {
                    data: SemanticDataKind::GenericSubstitution,
                    expected: 1,
                    actual: raw_pointer.generic_type_parameters().len(),
                },
            ));
        };

        Ok(TypeExpressionTemplate::Named {
            definition: NamedTypeSymbolId::Struct(raw_pointer.id()),
            parameters: Arc::from([GenericParameterSymbolId::Type(*parameter)]),
            arguments: Arc::from([GenericArgumentTemplate::Type(declared)]),
        })
    }

    pub(super) fn bind_surface_reference_type(
        &mut self,
        target: BoundReferenceTarget,
    ) -> BindingQueryResult<()> {
        let BoundReferenceTarget::Surface(symbol) = target else {
            return Ok(());
        };

        let template = match symbol {
            AnySymbolId::CallableParameter(_) => Some(self.declared_surface_value_type(symbol)?),
            AnySymbolId::Constant(_)
            | AnySymbolId::Static(_)
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
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        self.context
            .resolve_symbol_query(SymbolQueryRequest::<ConstantDeclaredTypeQuery>::new(
                constant,
            ))
            .map(|result| owned_template(result.value()))
    }
}

fn callable_parameter_templates(
    context: &CompilationBindingContext<'_>,
    signature: &CallableSignatureTemplate,
) -> BindingQueryResult<Vec<(CallableParameterSymbolId, TypeExpressionTemplate)>> {
    let types = signature
        .parameter_type_templates(context.semantic_values())
        .map_err(super::callable_signature_binding_error)?;

    Ok(signature.parameters().iter().copied().zip(types).collect())
}

fn owned_template(template: &TypeExpressionTemplate) -> TypeExpressionTemplate {
    // Published query results own their templates independently. Recursive storage remains Arc-shared.
    template.clone()
}

const fn surface_value(symbol: AnySymbolId) -> DeclaredValueTypeTerm {
    DeclaredValueTypeTerm::Value(BoundReferenceTarget::Surface(symbol))
}
