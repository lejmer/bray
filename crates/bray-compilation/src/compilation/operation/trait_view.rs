use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{
    BoundExpressionId, BoundMemberSelector, BoundStructuredExpression, BoundUnit,
    CheckedExpressionTypes, MemberTarget, SelectedOperation,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticSelectionKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    ExactSymbolId, ImplementationRequirementKey, ImplementationSelection, MemberLookupResult,
    SelfTypeContext, TraitApplicationId, TraitCallableMemberSymbolId, TypeData, TypeId,
};

use super::super::Compilation;
use super::super::binder::{CompilationBindingContext, binding_query_error};
use super::constraint::enclosing_generic_constraints;
use super::model::{OperationSubject, SemanticResolution};
use super::signature::{member_callable_signature, substitute_callable_self};
use crate::fact::FactQueryError;

impl Compilation {
    pub(super) fn resolve_trait_view_borrow(
        &self,
        key: &OperationSubject,
        context: &CompilationBindingContext<'_>,
        unit: &BoundUnit,
        types: &CheckedExpressionTypes,
        borrow: &BoundStructuredExpression,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<SemanticResolution>, FactQueryError> {
        let Some(result) = types
            .expression(key.expression())
            .filter(|ty| !ty.is_recovered())
        else {
            return Ok(None);
        };

        let result_type = result.ty();
        let values = context.semantic_values();
        let result = values.type_data(result_type);

        let TypeData::Borrow { target, .. } = result.as_ref() else {
            return Ok(None);
        };

        let target = values.type_data(*target);

        let TypeData::TraitView(application) = target.as_ref() else {
            return Ok(None);
        };

        let [operand] = borrow.operands() else {
            panic!("checked view formation retains one borrowed operand");
        };

        let Some(subject) = types.expression(*operand).filter(|ty| !ty.is_recovered()) else {
            return Ok(None);
        };

        let subject = subject.ty();

        if matches!(values.type_data(subject).as_ref(), TypeData::Borrow { target, .. } if values.type_data(*target).as_ref() == &TypeData::TraitView(*application))
        {
            return Ok(None);
        }

        let subject = values.unborrowed_type(subject);
        let requirement = ImplementationRequirementKey::new(subject, *application);

        let selected = self.implementation_selection_result_with_cancellation(
            requirement,
            context.cancellation(),
        )?;

        diagnostics.add_range(selected.diagnostics().clone());

        if selected.diagnostics().has_errors() {
            return Ok(None);
        }

        let owner = context
            .symbols()
            .symbol_for_key(unit.key().declared_owner())
            .expect("bound view formation retains its declaration owner");

        let constraints = enclosing_generic_constraints(context, owner, diagnostics)?;

        let dispatch = Self::trait_constraint_requirements(&constraints, subject)
            .get(application)
            .copied();

        let constrained = dispatch.is_some();

        if !matches!(selected.value(), ImplementationSelection::Selected(_)) && !constrained {
            diagnostics.add(unavailable_view_operation(
                unit,
                key.expression(),
                DiagnosticSelectionKind::Implementation,
            ));

            return Ok(None);
        }

        let trait_data = values.trait_application_data(*application);

        let members = view_trait(context, trait_data.definition())?.callable_members();

        let mut methods = Vec::new();
        let mut defaults = Vec::new();

        for &member in members {
            if self
                .trait_view_signature(context, member, *application, result_type, diagnostics)?
                .is_none()
            {
                methods.push(None);

                defaults.extend(
                    view_member(context, member)?
                        .parameters()
                        .iter()
                        .map(|_| None),
                );

                continue;
            }

            let resolution = match selected.value() {
                ImplementationSelection::Selected(witness) => self
                    .resolve_selected_trait_member_operation(
                        context,
                        unit,
                        key.expression(),
                        subject,
                        *application,
                        member,
                        requirement,
                        *witness,
                        diagnostics,
                    )?,
                _ => self.resolve_constrained_trait_member_operation(
                    context,
                    unit,
                    key.expression(),
                    subject,
                    *application,
                    dispatch.expect("view formation retains its generic proof"),
                    member,
                    &constraints,
                    diagnostics,
                )?,
            };

            let Some(bray_bound_tree::SemanticSelection::Operation(SelectedOperation::Member(
                method,
            ))) = resolution.as_ref().and_then(SemanticResolution::selection)
            else {
                if let ImplementationSelection::Selected(witness) = selected.value() {
                    let implementation = values.implementation_instance_data(*witness).definition();

                    let conformance = self.trait_implementation_conformance_with_cancellation(
                        implementation,
                        context.cancellation(),
                    )?;

                    diagnostics.add_range(conformance.diagnostics().iter().cloned());
                }

                if diagnostics.has_errors() {
                    return Ok(None);
                }

                panic!(
                    "view formation retains its selected callable fulfillment: {requirement:?}, {member:?}"
                );
            };

            let (instance, signature) = self
                .trait_view_signature(context, member, *application, subject, diagnostics)?
                .expect("selected view member retains its abstract declaration");

            let providers = self.resolve_callable_defaults(context, &signature, diagnostics)?;

            let default_owner = MemberTarget::new(
                member.into(),
                signature.callable_type(),
                method.witnesses().iter().copied(),
            )
            .with_callable(instance, signature, [])
            .with_trait_dispatch(dispatch.unwrap_or_else(|| {
                bray_symbols::TraitConstraintDispatch::trait_default(requirement)
            }));

            let declaration = view_member(context, member)?;

            // Slots share the checked signature and witness arrays from this immutable selection.
            for parameter in declaration.parameters() {
                defaults.push(
                    providers
                        .iter()
                        .find(|(candidate, _)| candidate == parameter)
                        .map(|(_, provider)| (default_owner.clone(), *provider)),
                );
            }

            methods.push(Some(method.clone()));
        }

        Ok(Some(SemanticResolution::new(
            key.expression(),
            result_type,
            [],
            Some(SelectedOperation::TraitView {
                requirement,
                result_type,
                methods: methods.into(),
                defaults: defaults.into(),
            }),
        )))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "view lowering and selection keep the checked receiver, witness slots, staged inputs, and source context explicit"
    )]
    pub(super) fn resolve_trait_view_member(
        &self,
        context: &CompilationBindingContext<'_>,
        unit: &BoundUnit,
        expression: BoundExpressionId,
        view: TypeId,
        application: TraitApplicationId,
        selector: Option<&BoundMemberSelector>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<SemanticResolution>, FactQueryError> {
        let before = diagnostics.len();

        let resolution = (|| -> Result<Option<SemanticResolution>, FactQueryError> {
            let Some(BoundMemberSelector::Name(name)) = selector else {
                return Ok(None);
            };

            let application_id = application;

            let application = context
                .semantic_values()
                .trait_application_data(application);

            let MemberLookupResult::Found(member) = context
                .lookup_member(application.definition().into(), name.as_str())
                .map_err(binding_query_error)?
            else {
                return Ok(None);
            };

            let Some(member) = TraitCallableMemberSymbolId::try_from_any(member) else {
                return Ok(None);
            };

            if !bray_binder::check_member_access(
                context,
                unit.view(),
                expression.into(),
                application.definition().into(),
                member.into(),
                diagnostics,
            )
            .map_err(binding_query_error)?
            {
                return Ok(None);
            }

            let Some((instance, signature)) =
                self.trait_view_signature(context, member, application_id, view, diagnostics)?
            else {
                return Ok(None);
            };

            let signature = member_callable_signature(signature, view);
            let result_type = signature.callable_type();
            let defaults = self.resolve_callable_defaults(context, &signature, diagnostics)?;

            let members = view_trait(context, application.definition())?.callable_members();

            let slot = members
                .iter()
                .position(|candidate| *candidate == member)
                .expect("view callable belongs to its trait");

            let mut default_base = members.len();

            for &prior in members.iter().take(slot) {
                default_base += view_member(context, prior)?.parameters().len();
            }

            let default_base = bray_symbols::SymbolOrdinal::new(
                u32::try_from(default_base)
                    .expect("view default ordinal fits its declaration table"),
            );

            let slot = bray_symbols::SymbolOrdinal::new(
                u32::try_from(slot).expect("view member ordinal fits its declaration table"),
            );

            let target = MemberTarget::new(member.into(), result_type, [])
                .with_callable(instance, signature, defaults)
                .with_trait_view_dispatch(slot, default_base);

            Ok(Some(SemanticResolution::new(
                expression,
                result_type,
                [],
                Some(SelectedOperation::Member(target)),
            )))
        })()?;

        if resolution.is_none() && diagnostics.len() == before {
            diagnostics.add(unavailable_view_operation(
                unit,
                expression,
                DiagnosticSelectionKind::Member,
            ));
        }

        Ok(resolution)
    }

    fn view_constraints_are_independent(
        context: &CompilationBindingContext<'_>,
        member: TraitCallableMemberSymbolId,
        self_context: SelfTypeContext,
        view: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<bool, FactQueryError> {
        let owner = bray_symbols::GenericOwnerId::try_new(member.into())
            .expect("trait member owns its static constraints");

        let constraints = context
            .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                bray_symbols::GenericConstraintsQuery,
            >::new(owner))
            .map_err(binding_query_error)?;

        diagnostics.add_range(constraints.diagnostics().iter().cloned());

        if constraints.diagnostics().has_errors() {
            return Ok(false);
        }

        if constraints.value().constraints().is_empty() {
            return Ok(true);
        }

        let values = context.semantic_values();
        let hidden_self = values.intern_type(TypeData::ContextualSelf(self_context))?;
        let replacements = std::collections::BTreeMap::from([(hidden_self, view)]);

        let independent_type = |ty| -> Result<bool, FactQueryError> {
            Ok(values.substitute_contextual_self(ty, self_context, view)? == ty)
        };

        for constraint in constraints.value().constraints() {
            let independent = match constraint.kind() {
                bray_symbols::CheckedConstraintKind::TraitSatisfaction {
                    subject,
                    application,
                } => {
                    independent_type(subject)?
                        && values.substitute_contextual_self_in_application(
                            application,
                            self_context,
                            view,
                        )? == application
                }
                bray_symbols::CheckedConstraintKind::TypeEquality { left, right } => {
                    independent_type(left)? && independent_type(right)?
                }
                bray_symbols::CheckedConstraintKind::Predicate(predicate) => {
                    values.replace_predicate_types(predicate, &replacements)? == predicate
                }
            };

            if !independent {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn trait_view_signature(
        &self,
        context: &CompilationBindingContext<'_>,
        member: TraitCallableMemberSymbolId,
        application: TraitApplicationId,
        view: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<
        Option<(
            bray_symbols::CallableInstanceData,
            bray_symbols::CallableSignature,
        )>,
        FactQueryError,
    > {
        let declaration = view_member(context, member)?;

        if !declaration.generic_type_parameters().is_empty()
            || !declaration.generic_const_parameters().is_empty()
        {
            return Ok(None);
        }

        let application = context
            .semantic_values()
            .trait_application_data(application);

        let Some(callable) = self.resolve_callable_signature(
            context,
            member.into(),
            [application.substitution()],
            diagnostics,
        )?
        else {
            return Ok(None);
        };

        let Some(receiver) = callable.signature.receiver() else {
            return Ok(None);
        };

        if matches!(
            receiver.mode(),
            bray_symbols::ReceiverMode::Consuming | bray_symbols::ReceiverMode::ConsumingMutable
        ) {
            return Ok(None);
        }

        let callable_type = callable.signature.callable_type();

        let signature = substitute_callable_self(
            context,
            callable.signature,
            SelfTypeContext::Trait(application.definition()),
            view,
        )?;

        if signature.callable_type() != callable_type
            || !Self::view_constraints_are_independent(
                context,
                member,
                SelfTypeContext::Trait(application.definition()),
                view,
                diagnostics,
            )?
        {
            return Ok(None);
        }

        Ok(Some((callable.instance, signature)))
    }
}

pub(super) fn unavailable_view_operation(
    unit: &BoundUnit,
    expression: BoundExpressionId,
    kind: DiagnosticSelectionKind,
) -> Diagnostic {
    let anchor = unit
        .view()
        .expression(expression)
        .expect("view selection retains its bound expression")
        .origin()
        .source_anchor()
        .syntax();

    let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

    Diagnostic::new(
        DiagnosticId::new(span.start().bytes()),
        DiagnosticKind::CheckingNoApplicableCandidate,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::SelectionFailure,
        span,
    ))
    .with_arg(DiagnosticArg::selection_kind(kind))
}

fn view_trait<'a>(
    context: &'a CompilationBindingContext<'_>,
    definition: bray_symbols::TraitSymbolId,
) -> Result<&'a bray_symbols::TraitSymbol, FactQueryError> {
    if let Some(declaration) = context.symbols().trait_symbol(definition) {
        return Ok(declaration);
    }

    Ok(context
        .imported_symbols()
        .map_err(binding_query_error)?
        .and_then(|symbols| symbols.trait_symbol(definition))
        .expect("checked view retains its source or imported trait declaration"))
}

fn view_member<'a>(
    context: &'a CompilationBindingContext<'_>,
    member: TraitCallableMemberSymbolId,
) -> Result<&'a bray_symbols::TraitCallableMemberSymbol, FactQueryError> {
    if let Some(declaration) = context.symbols().trait_callable_member(member) {
        return Ok(declaration);
    }

    Ok(context
        .imported_symbols()
        .map_err(binding_query_error)?
        .and_then(|symbols| symbols.trait_callable_member(member))
        .expect("checked view retains its source or imported callable declaration"))
}
