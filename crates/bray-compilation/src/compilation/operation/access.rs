// rust-style: allow(module-too-large, reason = "member and index operation resolution share one constrained lookup algorithm")

use std::collections::BTreeMap;

use bray_binder::{BindingQueryContext, SymbolQueryProvider, bind_member_callable_template};
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundMemberSelector, BoundStructuredExpressionKind,
    IndexTarget, MemberTarget, SelectedImplementationWitness, SelectedOperation,
};
use bray_checker::{
    ImplementationSelectionEvidence, OperationCandidate, OperationCandidateState,
    resolve_type_expression_template,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, BorrowKind, CallableDefinitionId, CallableInstanceData,
    CallableParameterDefaultProviderSymbolId, CallableParameterDefaultTemplateQuery,
    CallableParameterSymbolId, CallableSignature, CheckedConstraintKind, ExactSymbolId,
    GenericDeclarationTemplateQuery, GenericOwnerId, ImplementationSelection,
    ImplementationSubjectQuery, MemberLookupResult, NamedTypeSymbolId, SelfTypeContext,
    StructFieldTypeQuery, SymbolQueryContract, SymbolQueryRequest, TraitApplicationId,
    TraitCallableMemberSymbolId, TraitConstraintDispatch, TypeAssociatedMemberOrigin, TypeData,
    TypeExpressionTemplate, TypeId,
};
use bray_syntax::{GenericArgumentListSyntax, GenericArgumentSyntax};

use super::super::Compilation;
use super::super::binder::{
    CompilationBindingContext, binding_query_error, type_binder, type_scope,
    visible_generic_parameters,
};
use super::super::implementation::{
    implementation_callable_instance, implementation_fulfillments,
    implementation_match_query_error, match_implementation_subject,
};
use super::super::substitution::{
    contextual_self_type, identity_substitution, substitution_for_owner,
};
use crate::compilation::operation::OperationSubject;
use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    SemanticSymbolCategory,
};
use crate::fact::{CancellationToken, FactQueryError};

use super::model::{OperationResolution, TraitOperation};
use super::query::{
    expression_contract_failure, expression_type, operation_contract_failure,
    symbol_contract_failure, unit_contract_failure,
};
use super::signature::{
    member_callable_signature, normalize_callable_type_equalities,
    normalize_callable_type_valued_members, substitute_callable_self,
};

struct ResolvedCallableMember {
    signature: CallableSignature,
    instance: CallableInstanceData,
    template: Option<bray_bound_tree::CallableDeclarationTemplate>,
}

fn member_call_generic_arguments(
    compilation: &Compilation,
    unit: &bray_bound_tree::BoundUnit,
    expression: BoundExpressionId,
) -> Result<Vec<GenericArgumentSyntax>, FactQueryError> {
    let Some(parent) = unit.view().expression_parent(expression) else {
        return Ok(Vec::new());
    };

    let Some(BoundExpression::Call(call)) = unit.view().expression(parent) else {
        return Ok(Vec::new());
    };

    if call.callee() != expression {
        return Ok(Vec::new());
    }

    call.generic_arguments()
        .iter()
        .map(|argument| {
            argument
                .syntax()
                .find_descendant::<GenericArgumentSyntax>(compilation.syntax_tree())
                .ok_or_else(|| {
                    expression_contract_failure(
                        unit.key(),
                        expression,
                        SemanticQueryViolation::Missing(SemanticDataKind::Syntax),
                    )
                })
        })
        .collect()
}

impl Compilation {
    pub(super) fn resolve_member_operation(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        expression: BoundExpressionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        if let Some(BoundExpression::TraitQualifiedMember(member)) =
            unit.view().expression(expression)
        {
            return self.resolve_trait_qualified_member_operation(
                binding_context,
                unit,
                types,
                expression,
                member,
                diagnostics,
            );
        }

        let (receiver, selector) = match unit.view().expression(expression) {
            Some(BoundExpression::MemberAccess(member)) => (member.receiver(), member.selector()),
            _ => {
                return Err(expression_contract_failure(
                    unit.key(),
                    expression,
                    SemanticQueryViolation::Unsupported(SemanticDataKind::OperationSelection),
                ));
            }
        };

        let raw_receiver_type = expression_type(types, receiver)?;

        let receiver_type =
            self.resolve_access_subject_type(binding_context, raw_receiver_type, diagnostics)?;

        let data = binding_context
            .semantic_values()
            .type_data(receiver_type)
            .map_err(FactQueryError::SemanticValueStore)?;

        if let (TypeData::Tuple(elements), Some(BoundMemberSelector::TupleElement(index))) =
            (data.as_ref(), selector)
        {
            let Some(result_type) = elements.get(*index as usize).copied() else {
                return Ok(None);
            };

            return Ok(Some(OperationResolution::new(
                expression,
                result_type,
                [],
                None,
            )));
        }

        if matches!(data.as_ref(), TypeData::TypeParameter(_))
            && let Some(BoundMemberSelector::Name(name)) = selector
        {
            return self.resolve_constrained_member_operation(
                binding_context,
                unit,
                expression,
                receiver_type,
                name.as_str(),
                diagnostics,
            );
        }

        if let TypeData::ContextualSelf(SelfTypeContext::Trait(trait_definition)) = data.as_ref()
            && let Some(BoundMemberSelector::Name(name)) = selector
        {
            return self.resolve_trait_default_body_member_operation(
                binding_context,
                unit,
                expression,
                receiver_type,
                *trait_definition,
                name.as_str(),
                diagnostics,
            );
        }

        let Some(BoundMemberSelector::Name(name)) = selector else {
            return Ok(None);
        };

        let (definition, substitution) = match data.as_ref() {
            TypeData::Named {
                definition,
                substitution,
            } => (*definition, *substitution),
            TypeData::Array { .. } | TypeData::Slice(_) => {
                self.compiler_provided_surface(binding_context, "SequenceSurface")?
            }
            TypeData::Nullable(_) => {
                self.compiler_provided_surface(binding_context, "NullableSurface")?
            }
            _ => return Ok(None),
        };

        let surface = binding_context
            .type_associated_surface(definition)
            .map_err(binding_query_error)?;

        *diagnostics = diagnostics.merged(surface.diagnostics());

        let member = match surface.value().lookup(name.as_str()) {
            MemberLookupResult::Found(member) => member,
            MemberLookupResult::NotFound => {
                return self.resolve_participating_trait_member_operation(
                    binding_context,
                    unit,
                    types,
                    expression,
                    receiver,
                    raw_receiver_type,
                    receiver_type,
                    name.as_str(),
                    diagnostics,
                );
            }
            MemberLookupResult::WrongKind(_)
            | MemberLookupResult::Ambiguous(_)
            | MemberLookupResult::Inaccessible(_)
            | MemberLookupResult::Malformed(_) => return Ok(None),
        };

        let member_origin = surface
            .value()
            .member(member)
            .ok_or_else(|| {
                symbol_contract_failure(
                    member,
                    SemanticQueryViolation::Missing(SemanticDataKind::TypeSurface),
                )
            })?
            .origin();

        let (result_type, operation) = match member {
            AnySymbolId::StructField(field) => {
                let result_type = self.resolve_member_type(
                    binding_context,
                    SymbolQueryRequest::<StructFieldTypeQuery>::new(field),
                    substitution,
                    diagnostics,
                )?;

                let Some(result_type) = result_type else {
                    return Ok(None);
                };

                let operation =
                    SelectedOperation::Member(MemberTarget::new(field.into(), result_type, []));

                (result_type, Some(operation))
            }
            member if CallableDefinitionId::try_new(member).is_some() => {
                let member_substitution = match member_origin {
                    TypeAssociatedMemberOrigin::Direct => substitution,
                    TypeAssociatedMemberOrigin::InherentImplementation(implementation) => {
                        let contribution = surface
                            .value()
                            .implementations()
                            .iter()
                            .find(|candidate| candidate.implementation() == implementation)
                            .ok_or_else(|| {
                                symbol_contract_failure(
                                    implementation.into(),
                                    SemanticQueryViolation::Missing(
                                        SemanticDataKind::ImplementationUsing,
                                    ),
                                )
                            })?;

                        let pattern = self.resolve_implementation_self_type(
                            binding_context,
                            implementation.into(),
                            diagnostics,
                        )?;

                        match match_implementation_subject(
                            implementation.into(),
                            contribution.generic().parameters(),
                            pattern,
                            receiver_type,
                            binding_context.semantic_values(),
                        ) {
                            Ok(Some(substitution)) => substitution,
                            Ok(None) => return Ok(None),
                            Err(error) => {
                                return Err(implementation_match_query_error(
                                    implementation.into(),
                                    error,
                                ));
                            }
                        }
                    }
                };

                let Some(callable) = self.resolve_callable_member_signature(
                    binding_context,
                    unit,
                    expression,
                    member,
                    member_substitution,
                    diagnostics,
                )?
                else {
                    return Ok(None);
                };

                let result_type = callable.signature.callable_type();

                let defaults = self.resolve_callable_defaults(
                    binding_context,
                    &callable.signature,
                    diagnostics,
                )?;

                let signature = member_callable_signature(callable.signature, receiver_type);

                let mut target = MemberTarget::new(member, result_type, []).with_callable(
                    callable.instance,
                    signature,
                    defaults,
                );

                if let Some(template) = callable.template {
                    target = target.with_callable_template(template);
                }

                let operation = SelectedOperation::Member(target);

                (result_type, Some(operation))
            }
            AnySymbolId::UnionVariant(_) => (receiver_type, None),
            _ => return Ok(None),
        };

        Ok(Some(OperationResolution::new(
            expression,
            result_type,
            [],
            operation,
        )))
    }

    fn compiler_provided_surface(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        key: &'static str,
    ) -> Result<(NamedTypeSymbolId, bray_symbols::GenericSubstitutionId), FactQueryError> {
        let key =
            bray_compiler_known::CompilerKnownDeclarationKey::try_new(key).ok_or_else(|| {
                SemanticQueryFailure::contract(
                    SemanticQueryContext::CompilerKnownDeclarationName(key),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let definition = self
            .available_compiler_known_symbols()
            .declaration_symbol::<bray_symbols::StructSymbolId>(&key)
            .ok_or_else(|| {
                SemanticQueryFailure::contract(
                    SemanticQueryContext::CompilerKnownDeclaration(key.clone()),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let substitution = super::super::substitution::empty_substitution(
            binding_context.semantic_values(),
            definition.into(),
        )?;

        Ok((NamedTypeSymbolId::Struct(definition), substitution))
    }

    fn resolve_participating_trait_member_operation(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        expression: BoundExpressionId,
        receiver: BoundExpressionId,
        raw_receiver_type: TypeId,
        receiver_type: TypeId,
        name: &str,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let index = self.implementation_header_index(binding_context.cancellation())?;

        *diagnostics = diagnostics.merged(index.diagnostics());

        let values = binding_context.semantic_values();
        let mut candidates = Vec::new();

        for subject_type in [raw_receiver_type, receiver_type]
            .into_iter()
            .enumerate()
            .filter_map(|(index, ty)| (index == 0 || ty != raw_receiver_type).then_some(ty))
        {
            for header in index.value().headers() {
                let application = values
                    .trait_application_data(header.trait_application())
                    .map_err(FactQueryError::SemanticValueStore)?;

                let MemberLookupResult::Found(member) = binding_context
                    .lookup_member(application.definition().into(), name)
                    .map_err(binding_query_error)?
                else {
                    continue;
                };

                let Some(member) = TraitCallableMemberSymbolId::try_from_any(member) else {
                    continue;
                };

                let substitution = match match_implementation_subject(
                    header.implementation(),
                    header.parameters(),
                    header.subject(),
                    subject_type,
                    values,
                ) {
                    Ok(Some(substitution)) => substitution,
                    Ok(None) => continue,
                    Err(error) => {
                        return Err(implementation_match_query_error(
                            header.implementation(),
                            error,
                        ));
                    }
                };

                let application = values
                    .substitute_trait_application(header.trait_application(), substitution)
                    .map_err(FactQueryError::SemanticValueStore)?;

                let requirement =
                    bray_symbols::ImplementationRequirementKey::new(subject_type, application);

                let selected = self.implementation_selection_result_with_cancellation(
                    requirement,
                    binding_context.cancellation(),
                )?;

                *diagnostics = diagnostics.merged(selected.diagnostics());

                let ImplementationSelection::Selected(witness) = selected.value() else {
                    continue;
                };

                let candidate = (subject_type, application, member, requirement, *witness);

                if !candidates.contains(&candidate) {
                    candidates.push(candidate);
                }
            }
        }

        if let [(subject_type, application, member, requirement, witness)] = candidates.as_slice() {
            return self.resolve_selected_trait_member_operation(
                binding_context,
                unit,
                expression,
                *subject_type,
                *application,
                *member,
                *requirement,
                *witness,
                diagnostics,
            );
        }

        if candidates.is_empty() {
            return Ok(None);
        }

        let mut operation_candidates = Vec::with_capacity(candidates.len());

        for (subject_type, application, member, requirement, witness) in candidates {
            let resolution = self.resolve_selected_trait_member_operation(
                binding_context,
                unit,
                expression,
                subject_type,
                application,
                member,
                requirement,
                witness,
                diagnostics,
            )?;

            let Some(operation) = resolution.and_then(|resolution| resolution.selection().cloned())
            else {
                continue;
            };

            let implementation = binding_context
                .semantic_values()
                .implementation_instance_data(witness)
                .map_err(FactQueryError::SemanticValueStore)?;

            let key = binding_context
                .symbol_key(implementation.definition().into_any())
                .map_err(binding_query_error)?
                .ok_or_else(|| {
                    symbol_contract_failure(
                        implementation.definition().into_any(),
                        SemanticQueryViolation::Missing(SemanticDataKind::SymbolKey),
                    )
                })?;

            let candidate = OperationCandidate::symbol(
                key.clone(),
                operation,
                [raw_receiver_type],
                OperationCandidateState::Available,
            )
            .with_implementation_selections([ImplementationSelectionEvidence::new(
                requirement,
                ImplementationSelection::Selected(witness),
            )]);

            operation_candidates.push(candidate);
        }

        let key = OperationSubject::new(unit.key().clone(), expression);

        self.select_operation(
            &key,
            binding_context,
            unit,
            types,
            [receiver],
            operation_candidates,
            binding_context.cancellation(),
            diagnostics,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "selected trait methods retain the exact subject, application, member, and witness"
    )]
    fn resolve_selected_trait_member_operation(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        expression: BoundExpressionId,
        subject_type: TypeId,
        application: TraitApplicationId,
        trait_member: TraitCallableMemberSymbolId,
        requirement: bray_symbols::ImplementationRequirementKey,
        witness: bray_symbols::ImplementationInstanceId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let values = binding_context.semantic_values();

        let application_data = values
            .trait_application_data(application)
            .map_err(FactQueryError::SemanticValueStore)?;

        let implementation = values
            .implementation_instance_data(witness)
            .map_err(FactQueryError::SemanticValueStore)?;

        let fulfillments =
            implementation_fulfillments(binding_context, implementation.definition())?;

        let Some(selected) = implementation_callable_instance(
            binding_context,
            fulfillments.callables,
            trait_member,
            application_data.substitution(),
            implementation.substitution(),
        )?
        else {
            return Ok(None);
        };

        let uses_trait_default = selected.uses_trait_default();
        let selected = selected.instance();

        let callable =
            self.resolved_callable_instance_member(binding_context, selected, diagnostics)?;

        let Some(mut callable) = callable else {
            return Ok(None);
        };

        let inherited_substitution = if uses_trait_default {
            application_data.substitution()
        } else {
            implementation.substitution()
        };

        callable.template = self.bind_trait_callable_member_template(
            binding_context,
            unit,
            expression,
            selected.definition().symbol(),
            inherited_substitution,
            diagnostics,
        )?;

        let signature = if uses_trait_default {
            substitute_callable_self(
                binding_context,
                callable.signature,
                SelfTypeContext::Trait(application_data.definition()),
                subject_type,
            )?
        } else {
            substitute_callable_self(
                binding_context,
                callable.signature,
                SelfTypeContext::Implementation(implementation.definition()),
                subject_type,
            )?
        };

        let signature = if uses_trait_default {
            normalize_callable_type_valued_members(
                binding_context,
                signature,
                witness,
                diagnostics,
            )?
        } else {
            signature
        };

        let signature = member_callable_signature(signature, subject_type);
        let result_type = signature.callable_type();
        let defaults = self.resolve_callable_defaults(binding_context, &signature, diagnostics)?;

        let mut target = MemberTarget::new(
            selected.definition().symbol(),
            result_type,
            [SelectedImplementationWitness::new(requirement, witness)],
        )
        .with_callable(callable.instance, signature, defaults);

        if let Some(template) = callable.template {
            target = target.with_callable_template(template);
        }

        if uses_trait_default {
            target =
                target.with_trait_dispatch(TraitConstraintDispatch::trait_default(requirement));
        }

        Ok(Some(OperationResolution::new(
            expression,
            result_type,
            [],
            Some(SelectedOperation::Member(target)),
        )))
    }

    fn resolve_trait_default_body_member_operation(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        expression: BoundExpressionId,
        receiver_type: TypeId,
        trait_definition: bray_symbols::TraitSymbolId,
        name: &str,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let MemberLookupResult::Found(member) = binding_context
            .lookup_member(trait_definition.into(), name)
            .map_err(binding_query_error)?
        else {
            return Ok(None);
        };

        let Some(member) = TraitCallableMemberSymbolId::try_from_any(member) else {
            return Ok(None);
        };

        let owner = GenericOwnerId::try_new(trait_definition.into()).ok_or_else(|| {
            symbol_contract_failure(
                trait_definition.into(),
                SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: SemanticSymbolCategory::GenericOwner,
                    actual: trait_definition.kind(),
                },
            )
        })?;

        let parameters =
            visible_generic_parameters(binding_context.symbols(), trait_definition.into());

        let substitution =
            identity_substitution(binding_context.semantic_values(), owner, &parameters)?;

        let application = binding_context
            .semantic_values()
            .intern_trait_application(bray_symbols::TraitApplicationData::new(
                trait_definition,
                substitution,
            ))
            .map_err(FactQueryError::SemanticValueStore)?;

        self.resolve_constrained_trait_member_operation(
            binding_context,
            unit,
            expression,
            receiver_type,
            application,
            TraitConstraintDispatch::trait_default(
                bray_symbols::ImplementationRequirementKey::new(receiver_type, application),
            ),
            member,
            &[],
            diagnostics,
        )
    }

    fn resolve_constrained_member_operation(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        expression: BoundExpressionId,
        receiver_type: TypeId,
        name: &str,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let owner = binding_context
            .symbols()
            .symbol_for_key(unit.key().declared_owner())
            .ok_or_else(|| {
                unit_contract_failure(
                    unit.key(),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let constraints =
            super::constraint::enclosing_generic_constraints(binding_context, owner, diagnostics)?;

        let requirements = Self::trait_constraint_requirements(&constraints, receiver_type);

        let mut matches = Vec::new();

        for (application, dispatch) in requirements {
            let application_data = binding_context
                .semantic_values()
                .trait_application_data(application)
                .map_err(FactQueryError::SemanticValueStore)?;

            let MemberLookupResult::Found(member) = binding_context
                .lookup_member(application_data.definition().into(), name)
                .map_err(binding_query_error)?
            else {
                continue;
            };

            let Some(member) = TraitCallableMemberSymbolId::try_from_any(member) else {
                continue;
            };

            matches.push((dispatch, application, member));
        }

        let [(dispatch, application, member)] = matches.as_slice() else {
            return Ok(None);
        };

        self.resolve_constrained_trait_member_operation(
            binding_context,
            unit,
            expression,
            receiver_type,
            *application,
            *dispatch,
            *member,
            &constraints,
            diagnostics,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the operation subject, trait application, dispatch, and member are distinct semantic inputs"
    )]
    fn resolve_constrained_trait_member_operation(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        expression: BoundExpressionId,
        subject_type: TypeId,
        application: TraitApplicationId,
        dispatch: TraitConstraintDispatch,
        member: TraitCallableMemberSymbolId,
        constraints: &[(GenericOwnerId, bray_symbols::CheckedConstraint)],
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let application = binding_context
            .semantic_values()
            .trait_application_data(application)
            .map_err(FactQueryError::SemanticValueStore)?;

        let callable = self.resolve_callable_signature(
            binding_context,
            member.into(),
            [application.substitution()],
            diagnostics,
        )?;

        let Some(mut callable) = callable else {
            return Ok(None);
        };

        callable.template = self.bind_trait_callable_member_template(
            binding_context,
            unit,
            expression,
            member.into(),
            application.substitution(),
            diagnostics,
        )?;

        let trait_context = SelfTypeContext::Trait(application.definition());

        let signature = substitute_callable_self(
            binding_context,
            callable.signature,
            trait_context,
            subject_type,
        )?;

        let signature =
            normalize_callable_type_equalities(binding_context, signature, constraints)?;

        let result_type = signature.callable_type();
        let defaults = self.resolve_callable_defaults(binding_context, &signature, diagnostics)?;

        let signature = member_callable_signature(signature, subject_type);

        let mut target = MemberTarget::new(member.into(), result_type, [])
            .with_callable(callable.instance, signature, defaults)
            .with_trait_dispatch(dispatch);

        if let Some(template) = callable.template {
            target = target.with_callable_template(template);
        }

        Ok(Some(OperationResolution::new(
            expression,
            result_type,
            [],
            Some(SelectedOperation::Member(target)),
        )))
    }

    fn trait_constraint_requirements(
        constraints: &[(GenericOwnerId, bray_symbols::CheckedConstraint)],
        receiver_type: TypeId,
    ) -> BTreeMap<TraitApplicationId, TraitConstraintDispatch> {
        let mut requirements = BTreeMap::new();

        for (generic_owner, constraint) in constraints {
            let CheckedConstraintKind::TraitSatisfaction {
                subject,
                application,
            } = constraint.kind()
            else {
                continue;
            };

            if subject == receiver_type {
                requirements.entry(application).or_insert_with(|| {
                    TraitConstraintDispatch::new(*generic_owner, constraint.ordinal())
                });
            }
        }

        requirements
    }

    fn resolve_access_subject_type(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        mut ty: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<TypeId, FactQueryError> {
        loop {
            let data = binding_context
                .semantic_values()
                .type_data(ty)
                .map_err(FactQueryError::SemanticValueStore)?;

            match data.as_ref() {
                TypeData::Borrow { target, .. } => ty = *target,
                TypeData::ContextualSelf(SelfTypeContext::Implementation(implementation)) => {
                    ty = self.resolve_implementation_self_type(
                        binding_context,
                        *implementation,
                        diagnostics,
                    )?;
                }
                TypeData::ContextualSelf(SelfTypeContext::Trait(_)) => return Ok(ty),
                TypeData::ContextualSelf(context) => {
                    ty = contextual_self_type(binding_context, *context)?;
                }
                _ => return Ok(ty),
            }
        }
    }

    fn resolve_implementation_self_type(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        implementation: bray_symbols::ImplementationSymbolId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<TypeId, FactQueryError> {
        let subject = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<ImplementationSubjectQuery>::new(
                implementation,
            ))
            .map_err(binding_query_error)?;

        *diagnostics = diagnostics.merged(subject.diagnostics());

        let checked = self.checked_constant_terms(subject.value().ty())?;

        *diagnostics = diagnostics.merged(checked.diagnostics());

        resolve_type_expression_template(
            binding_context.semantic_values(),
            subject.value().ty(),
            checked.value(),
        )
        .map_err(FactQueryError::from)?
        .ok_or_else(|| {
            SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(implementation.into_any()),
                SemanticQueryViolation::Unsupported(SemanticDataKind::Type),
            )
            .into()
        })
    }

    fn resolve_trait_qualified_member_operation(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        expression: BoundExpressionId,
        member: &bray_bound_tree::BoundTraitQualifiedMemberExpression,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let Some(BoundMemberSelector::Name(name)) = member.selector() else {
            return Ok(None);
        };

        let receiver_type = expression_type(types, member.receiver())?;

        let owner = binding_context
            .symbols()
            .symbol_for_key(unit.key().declared_owner())
            .ok_or_else(|| {
                unit_contract_failure(
                    unit.key(),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let reference = unit
            .view()
            .expression(member.trait_reference())
            .ok_or_else(|| {
                expression_contract_failure(
                    unit.key(),
                    member.trait_reference(),
                    SemanticQueryViolation::Missing(SemanticDataKind::BoundExpression),
                )
            })?;

        let BoundExpression::Name(reference) = reference else {
            return Err(expression_contract_failure(
                unit.key(),
                member.trait_reference(),
                SemanticQueryViolation::Unsupported(SemanticDataKind::BoundExpression),
            ));
        };

        let bray_bound_tree::BoundReferenceTarget::Surface(AnySymbolId::Trait(definition)) =
            reference.target()
        else {
            return Err(expression_contract_failure(
                unit.key(),
                member.trait_reference(),
                SemanticQueryViolation::Unsupported(SemanticDataKind::TraitApplication),
            ));
        };

        let anchor = reference.origin().source_anchor().syntax();

        let syntax = binding_context
            .syntax()
            .find_node(
                anchor.source_id(),
                anchor.syntax_kind(),
                anchor.full_range(),
                anchor.is_recovered(),
            )
            .ok_or_else(|| {
                expression_contract_failure(
                    unit.key(),
                    expression,
                    SemanticQueryViolation::Missing(SemanticDataKind::Syntax),
                )
            })?;

        let arguments = reference
            .generic_argument_list()
            .map(|anchor| {
                anchor
                    .find_descendant::<GenericArgumentListSyntax>(binding_context.syntax())
                    .ok_or_else(|| {
                        expression_contract_failure(
                            unit.key(),
                            member.trait_reference(),
                            SemanticQueryViolation::Missing(SemanticDataKind::Syntax),
                        )
                    })
            })
            .transpose()?;

        let bound = type_binder(binding_context, owner)
            .map_err(binding_query_error)?
            .bind_trait_reference(definition, &syntax, arguments.as_ref())
            .map_err(binding_query_error)?;

        *diagnostics = diagnostics.merged(bound.diagnostics());

        let Some(bound) = bound.value() else {
            return Ok(None);
        };

        let application = type_binder(binding_context, owner)
            .map_err(binding_query_error)?
            .resolve_trait_application_template(bound)
            .map_err(binding_query_error)?
            .ok_or_else(|| {
                expression_contract_failure(
                    unit.key(),
                    expression,
                    SemanticQueryViolation::Missing(SemanticDataKind::TraitApplication),
                )
            })?;

        let application_data = binding_context
            .semantic_values()
            .trait_application_data(application)
            .map_err(FactQueryError::SemanticValueStore)?;

        let lookup = binding_context
            .lookup_member(application_data.definition().into(), name.as_str())
            .map_err(binding_query_error)?;

        let MemberLookupResult::Found(trait_member) = lookup else {
            return Ok(None);
        };

        let Some(trait_member) = TraitCallableMemberSymbolId::try_from_any(trait_member) else {
            return Ok(None);
        };

        let constraints =
            super::constraint::enclosing_generic_constraints(binding_context, owner, diagnostics)?;

        let dereferenced_type =
            self.resolve_access_subject_type(binding_context, receiver_type, diagnostics)?;

        let mut selected_implementation = None;

        for subject_type in [receiver_type, dereferenced_type]
            .into_iter()
            .enumerate()
            .filter_map(|(index, ty)| (index == 0 || ty != receiver_type).then_some(ty))
        {
            let requirements = Self::trait_constraint_requirements(&constraints, subject_type);

            if let Some(dispatch) = requirements.get(&application).copied() {
                return self.resolve_constrained_trait_member_operation(
                    binding_context,
                    unit,
                    expression,
                    subject_type,
                    application,
                    dispatch,
                    trait_member,
                    &constraints,
                    diagnostics,
                );
            }

            let requirement =
                bray_symbols::ImplementationRequirementKey::new(subject_type, application);

            let selected = self.implementation_selection_result_with_cancellation(
                requirement,
                binding_context.cancellation(),
            )?;

            *diagnostics = diagnostics.merged(selected.diagnostics());

            if let ImplementationSelection::Selected(witness) = selected.value() {
                selected_implementation = Some((subject_type, requirement, *witness));

                break;
            }
        }

        let Some((subject_type, requirement, witness)) = selected_implementation else {
            return Ok(None);
        };

        self.resolve_selected_trait_member_operation(
            binding_context,
            unit,
            expression,
            subject_type,
            application,
            trait_member,
            requirement,
            witness,
            diagnostics,
        )
    }

    fn resolve_callable_member_signature(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        expression: BoundExpressionId,
        member: AnySymbolId,
        receiver_substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<ResolvedCallableMember>, FactQueryError> {
        let callable = self.resolve_callable_signature(
            binding_context,
            member,
            [receiver_substitution],
            diagnostics,
        )?;

        let Some(mut callable) = callable else {
            return Ok(None);
        };

        callable.template = self.bind_callable_member_template(
            binding_context,
            unit,
            expression,
            member,
            receiver_substitution,
            diagnostics,
        )?;

        Ok(Some(callable))
    }

    fn bind_callable_member_template(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        expression: BoundExpressionId,
        member: AnySymbolId,
        inherited_substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<bray_bound_tree::CallableDeclarationTemplate>, FactQueryError> {
        let arguments = member_call_generic_arguments(self, unit, expression)?;

        self.bind_callable_member_template_with_arguments(
            binding_context,
            unit,
            member,
            inherited_substitution,
            &arguments,
            diagnostics,
        )
    }

    fn bind_trait_callable_member_template(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        expression: BoundExpressionId,
        member: AnySymbolId,
        inherited_substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<bray_bound_tree::CallableDeclarationTemplate>, FactQueryError> {
        let arguments = member_call_generic_arguments(self, unit, expression)?;

        let member_owner = GenericOwnerId::try_new(member).ok_or_else(|| {
            symbol_contract_failure(
                member,
                SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: SemanticSymbolCategory::GenericOwner,
                    actual: member.kind(),
                },
            )
        })?;

        let direct_generic = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(
                member_owner,
            ))
            .map_err(binding_query_error)?;

        *diagnostics = diagnostics.merged(direct_generic.diagnostics());

        if arguments.is_empty() && direct_generic.value().parameters().is_empty() {
            return Ok(None);
        }

        self.bind_callable_member_template_with_arguments(
            binding_context,
            unit,
            member,
            inherited_substitution,
            &arguments,
            diagnostics,
        )
    }

    fn bind_callable_member_template_with_arguments(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        member: AnySymbolId,
        inherited_substitution: bray_symbols::GenericSubstitutionId,
        arguments: &[GenericArgumentSyntax],
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<bray_bound_tree::CallableDeclarationTemplate>, FactQueryError> {
        let owner = binding_context
            .symbols()
            .symbol_for_key(unit.key().declared_owner())
            .ok_or_else(|| {
                unit_contract_failure(
                    unit.key(),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let scope = type_scope(binding_context, owner).map_err(binding_query_error)?;

        let template = bind_member_callable_template(
            binding_context,
            member,
            inherited_substitution,
            arguments,
            &scope,
        )
        .map_err(binding_query_error)?;

        *diagnostics = diagnostics.merged(template.diagnostics());

        Ok(template.value().clone())
    }

    fn resolve_callable_defaults(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        signature: &CallableSignature,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<
        Vec<(
            CallableParameterSymbolId,
            CallableParameterDefaultProviderSymbolId,
        )>,
        FactQueryError,
    > {
        let mut defaults = Vec::new();

        for parameter in signature.parameters() {
            let parameter = parameter.parameter();

            let result = binding_context
                .resolve_symbol_query(
                    SymbolQueryRequest::<CallableParameterDefaultTemplateQuery>::new(parameter),
                )
                .map_err(binding_query_error)?;

            *diagnostics = diagnostics.merged(result.diagnostics());

            if !result.value().is_present() {
                continue;
            }

            let provider = binding_context
                .callable_parameter_default_provider(parameter)
                .map_err(binding_query_error)?
                .ok_or_else(|| {
                    symbol_contract_failure(
                        parameter.into(),
                        SemanticQueryViolation::Missing(SemanticDataKind::ConstantDefinition),
                    )
                })?;

            defaults.push((parameter, provider));
        }

        Ok(defaults)
    }

    fn resolve_callable_signature(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        member: AnySymbolId,
        substitutions: impl IntoIterator<Item = bray_symbols::GenericSubstitutionId>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<ResolvedCallableMember>, FactQueryError> {
        let definition = CallableDefinitionId::try_new(member).ok_or_else(|| {
            symbol_contract_failure(
                member,
                SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: SemanticSymbolCategory::Callable,
                    actual: member.kind(),
                },
            )
        })?;

        let substitution =
            substitution_for_owner(binding_context.semantic_values(), member, substitutions)?;

        self.resolved_callable_instance_member(
            binding_context,
            CallableInstanceData::new(definition, substitution),
            diagnostics,
        )
    }

    fn resolved_callable_instance_member(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        instance: CallableInstanceData,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<ResolvedCallableMember>, FactQueryError> {
        Ok(self
            .resolve_callable_instance_signature(binding_context, instance, diagnostics)?
            .map(|signature| ResolvedCallableMember {
                signature,
                instance,
                template: None,
            }))
    }
    pub(super) fn resolve_member_type<'binding_context, F>(
        &self,
        binding_context: &CompilationBindingContext<'binding_context>,
        request: SymbolQueryRequest<F>,
        substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TypeId>, FactQueryError>
    where
        F: SymbolQueryContract<Value = TypeExpressionTemplate>,
        CompilationBindingContext<'binding_context>: bray_binder::SymbolQueryErrorProvider<UpstreamError = FactQueryError>
            + SymbolQueryProvider<F>,
    {
        let result = binding_context
            .resolve_symbol_query(request)
            .map_err(binding_query_error)?;

        *diagnostics = diagnostics.merged(result.diagnostics());

        let checked = self.checked_constant_terms(result.value())?;

        *diagnostics = diagnostics.merged(checked.diagnostics());

        let Some(ty) = resolve_type_expression_template(
            binding_context.semantic_values(),
            result.value(),
            checked.value(),
        )
        .map_err(FactQueryError::from)?
        else {
            return Ok(None);
        };

        binding_context
            .semantic_values()
            .substitute_type(ty, substitution)
            .map(Some)
            .map_err(FactQueryError::SemanticValueStore)
    }

    pub(super) fn resolve_index_operation(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        expression: BoundExpressionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let Some(BoundExpression::Structured(index)) = unit.view().expression(expression) else {
            return Err(expression_contract_failure(
                unit.key(),
                expression,
                SemanticQueryViolation::Unsupported(SemanticDataKind::OperationSelection),
            ));
        };

        let Some(receiver) = index.operands().first().copied() else {
            return Ok(None);
        };

        let receiver_type = self.resolve_access_subject_type(
            binding_context,
            expression_type(types, receiver)?,
            diagnostics,
        )?;

        let data = binding_context
            .semantic_values()
            .type_data(receiver_type)
            .map_err(FactQueryError::SemanticValueStore)?;

        let built_in = match (index.kind(), data.as_ref()) {
            (BoundStructuredExpressionKind::ElementIndex, TypeData::Array { element, .. }) => {
                Some((IndexTarget::ArrayElement, *element))
            }
            (BoundStructuredExpressionKind::ElementIndex, TypeData::Slice(element)) => {
                Some((IndexTarget::SliceElement, *element))
            }
            (BoundStructuredExpressionKind::SliceIndex, TypeData::Array { element, .. }) => Some((
                IndexTarget::ArraySlice,
                binding_context
                    .semantic_values()
                    .intern_type(TypeData::Slice(*element))
                    .map_err(FactQueryError::SemanticValueStore)?,
            )),
            (BoundStructuredExpressionKind::SliceIndex, TypeData::Slice(element)) => Some((
                IndexTarget::Slice,
                binding_context
                    .semantic_values()
                    .intern_type(TypeData::Slice(*element))
                    .map_err(FactQueryError::SemanticValueStore)?,
            )),
            _ => None,
        };

        if let Some((target, result_type)) = built_in {
            let role = bray_compiler_known::RepresentationRole::ScalarUsize;

            let usize_type = self
                .available_compiler_known_symbols()
                .representation_symbol::<bray_symbols::StructSymbolId>(role)
                .ok_or_else(|| {
                    SemanticQueryFailure::contract(
                        SemanticQueryContext::CompilerKnownRepresentation(role),
                        SemanticQueryViolation::Missing(SemanticDataKind::Type),
                    )
                })?;

            let substitution = super::super::substitution::empty_substitution(
                binding_context.semantic_values(),
                usize_type.into(),
            )?;

            let usize_type = binding_context
                .semantic_values()
                .intern_type(TypeData::Named {
                    definition: NamedTypeSymbolId::Struct(usize_type),
                    substitution,
                })
                .map_err(FactQueryError::SemanticValueStore)?;

            let expectations = index
                .operands()
                .iter()
                .copied()
                .skip(1)
                .map(|operand| (operand, usize_type));

            let operation = SelectedOperation::Index {
                target,
                result_type,
            };

            return Ok(Some(OperationResolution::new(
                expression,
                result_type,
                expectations,
                Some(operation),
            )));
        }

        Ok(None)
    }

    pub(super) fn resolve_custom_index_operation(
        &self,
        key: &OperationSubject,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let Some(BoundExpression::Structured(index)) = unit.view().expression(key.expression())
        else {
            return Err(operation_contract_failure(
                key,
                SemanticQueryViolation::Unsupported(SemanticDataKind::OperationSelection),
            ));
        };

        let operand_types = index
            .operands()
            .iter()
            .copied()
            .map(|operand| expression_type(types, operand))
            .collect::<Result<Vec<_>, _>>()?;

        let Some((subject, selectors)) = operand_types
            .split_first()
            .map(|(subject, selectors)| (*subject, selectors))
        else {
            return Err(operation_contract_failure(
                key,
                SemanticQueryViolation::CountMismatch {
                    data: SemanticDataKind::Type,
                    expected: 1,
                    actual: 0,
                },
            ));
        };

        let source_subject = subject;
        let subject = self.resolve_access_subject_type(binding_context, subject, diagnostics)?;

        let borrow_kind = custom_index_borrow_kind(unit, key.expression())?;

        let (role, trait_arguments, callable_parameters, operand_types) = match index.kind() {
            BoundStructuredExpressionKind::ElementIndex => (
                match borrow_kind {
                    BorrowKind::Shared => {
                        bray_compiler_known::CompilerKnownOperationRole::ElementIndex
                    }
                    BorrowKind::Mutable => {
                        bray_compiler_known::CompilerKnownOperationRole::MutableElementIndex
                    }
                },
                selectors.to_vec(),
                selectors.to_vec(),
                selectors.to_vec(),
            ),
            BoundStructuredExpressionKind::SliceIndex => {
                let Some(bound) = selectors.first().copied() else {
                    return Ok(None);
                };

                let nullable_bound = binding_context
                    .semantic_values()
                    .intern_type(TypeData::Nullable(bound))
                    .map_err(FactQueryError::SemanticValueStore)?;

                (
                    match borrow_kind {
                        BorrowKind::Shared => {
                            bray_compiler_known::CompilerKnownOperationRole::SliceIndex
                        }
                        BorrowKind::Mutable => {
                            bray_compiler_known::CompilerKnownOperationRole::MutableSliceIndex
                        }
                    },
                    vec![bound],
                    vec![nullable_bound, nullable_bound],
                    vec![bound; selectors.len()],
                )
            }
            _ => {
                return Err(operation_contract_failure(
                    key,
                    SemanticQueryViolation::Unsupported(SemanticDataKind::OperationSelection),
                ));
            }
        };

        let owner = binding_context
            .symbols()
            .symbol_for_key(unit.key().declared_owner())
            .ok_or_else(|| {
                unit_contract_failure(
                    unit.key(),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let candidate = self.trait_operation_candidate_data(
            binding_context,
            owner,
            role,
            subject,
            &trait_arguments,
            &callable_parameters,
            &operand_types,
            TraitOperation::Index(borrow_kind),
            cancellation,
            diagnostics,
        )?;

        if candidate.is_none() && borrow_kind == BorrowKind::Mutable {
            let shared_role = match index.kind() {
                BoundStructuredExpressionKind::ElementIndex => {
                    bray_compiler_known::CompilerKnownOperationRole::ElementIndex
                }
                BoundStructuredExpressionKind::SliceIndex => {
                    bray_compiler_known::CompilerKnownOperationRole::SliceIndex
                }
                _ => {
                    return Err(operation_contract_failure(
                        key,
                        SemanticQueryViolation::Unsupported(SemanticDataKind::OperationSelection),
                    ));
                }
            };

            let shared = self.trait_operation_candidate_data(
                binding_context,
                owner,
                shared_role,
                subject,
                &trait_arguments,
                &callable_parameters,
                &operand_types,
                TraitOperation::Index(BorrowKind::Shared),
                cancellation,
                diagnostics,
            )?;

            if let Some(shared) = shared {
                let anchor = index.origin().source_anchor().syntax();

                let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticId::new(anchor.full_range().start().bytes()),
                        DiagnosticKind::CheckingMutableIndexContractRequired,
                        SeverityKind::Error,
                    )
                    .with_primary_span(span)
                    .with_label(DiagnosticLabel::primary(
                        DiagnosticLabelKind::SelectionFailure,
                        span,
                    ))
                    .with_arg(DiagnosticArg::referenced_name(role.as_str())),
                );

                let result_type = shared.operation.result_type().ok_or_else(|| {
                    operation_contract_failure(
                        key,
                        SemanticQueryViolation::Missing(SemanticDataKind::Type),
                    )
                })?;

                return Ok(Some(OperationResolution::new(
                    key.expression(),
                    result_type,
                    [],
                    None,
                )));
            }
        }

        let candidate = candidate.map(|mut candidate| {
            // The contract belongs to the reached type. Indexing accepts the receiver's borrow layers.
            if let Some(receiver) = candidate.operand_types.first_mut() {
                *receiver = source_subject;
            }

            candidate.into_candidate()
        });

        self.select_operation(
            key,
            binding_context,
            unit,
            types,
            index.operands().iter().copied(),
            candidate,
            cancellation,
            diagnostics,
        )
    }
}

fn custom_index_borrow_kind(
    unit: &bray_bound_tree::BoundUnit,
    expression: BoundExpressionId,
) -> Result<BorrowKind, FactQueryError> {
    let view = unit.view();
    let mut child = expression;

    while let Some(parent) = view.expression_parent(child) {
        let parent_expression = view.expression(parent).ok_or_else(|| {
            expression_contract_failure(
                unit.key(),
                parent,
                SemanticQueryViolation::Missing(SemanticDataKind::BoundExpression),
            )
        })?;

        match parent_expression {
            BoundExpression::Assignment(assignment)
                if assignment.operands().first() == Some(&child) =>
            {
                return Ok(BorrowKind::Mutable);
            }
            BoundExpression::Structured(structured)
                if structured.operands().first() == Some(&child)
                    && structured.kind() == BoundStructuredExpressionKind::Borrow =>
            {
                return structured.borrow_kind().ok_or_else(|| {
                    expression_contract_failure(
                        unit.key(),
                        parent,
                        SemanticQueryViolation::Unsupported(SemanticDataKind::OperationSelection),
                    )
                });
            }
            BoundExpression::Structured(structured)
                if structured.operands().first() == Some(&child)
                    && matches!(
                        structured.kind(),
                        BoundStructuredExpressionKind::ElementIndex
                            | BoundStructuredExpressionKind::SliceIndex
                            | BoundStructuredExpressionKind::NullablePropagation
                    ) =>
            {
                child = parent;
            }
            BoundExpression::MemberAccess(_)
                if parent_expression.child_expressions().next() == Some(child) =>
            {
                child = parent;
            }
            _ => return Ok(BorrowKind::Shared),
        }
    }

    Ok(BorrowKind::Shared)
}
