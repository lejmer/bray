use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundUnit,
    DeclaredValueTypeTemplates, DeclaredValueTypeTerm, SelectionKind,
};
use bray_checker::{
    CallableCandidateTemplate, CallableCandidateTemplateState, CallableCandidateTemplates,
    CallableDeclarationCandidateTemplate, CallableParameterDefaultTemplate,
    CallableValueCandidateTemplate, CandidateAbsence, ExpressionCandidateSet,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableOverloadSymbolId, CallableOverloadTemplateFact,
    CallableParameterDefaultTemplateFact, CallableSignatureFact, CallableSymbolId,
    GenericDeclarationTemplateFact, GenericOwnerId, MemberLookupResult, OverloadArmTemplate,
    SymbolFactContract, SymbolFactRequest, TypeData, TypeExpressionTemplate,
};
use bray_syntax::PathSyntax;

use crate::lookup::{NameAccess, ResolvedName, bind_module_path};
use crate::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};

pub(super) fn bind_call_candidates<C>(
    context: &C,
    unit: &BoundUnit,
    declared_types: &DeclaredValueTypeTemplates,
    expression: BoundExpressionId,
    callee: BoundExpressionId,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<ExpressionCandidateSet>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    let Some(callee_expression) = unit.view().expression(callee) else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let mut candidates = Vec::new();

    let absence = match callee_expression {
        BoundExpression::Name(name) => bind_reference_target(
            context,
            declared_types,
            name.target(),
            state_for_recovery(name.is_recovered()),
            diagnostics,
            &mut candidates,
        )?,
        BoundExpression::UnresolvedReference(reference) => {
            for target in reference.candidates() {
                bind_reference_target(
                    context,
                    declared_types,
                    *target,
                    CallableCandidateTemplateState::Recovered,
                    diagnostics,
                    &mut candidates,
                )?;
            }

            CandidateAbsence::UnresolvedReference
        }
        BoundExpression::MemberAccess(_) | BoundExpression::TraitQualifiedMember(_) => {
            return Ok(ExpressionCandidateSet::Unsupported {
                expression,
                kind: SelectionKind::Member,
            });
        }
        _ => bind_callable_value(
            context,
            declared_types,
            DeclaredValueTypeTerm::Expression(callee),
            state_for_recovery(callee_expression.is_recovered()),
            &mut candidates,
        )?,
    };

    let candidate_set = CallableCandidateTemplates::present(expression, candidates)
        .unwrap_or_else(|| CallableCandidateTemplates::absent(expression, absence));

    Ok(ExpressionCandidateSet::Callable(candidate_set))
}

fn bind_reference_target<C>(
    context: &C,
    declared_types: &DeclaredValueTypeTemplates,
    target: BoundReferenceTarget,
    state: CallableCandidateTemplateState,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<CandidateAbsence>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    match target {
        BoundReferenceTarget::Surface(AnySymbolId::CallableOverload(overload)) => {
            bind_overload_candidates(context, overload, state, diagnostics, candidates)?;

            Ok(CandidateAbsence::EmptyOverload)
        }
        BoundReferenceTarget::Surface(symbol) if symbol.kind().is_callable() => {
            bind_declaration_candidate(context, symbol, state, diagnostics, candidates)?;

            Ok(CandidateAbsence::UnresolvedReference)
        }
        BoundReferenceTarget::Local(_) | BoundReferenceTarget::Surface(_) => bind_callable_value(
            context,
            declared_types,
            DeclaredValueTypeTerm::Value(target),
            state,
            candidates,
        ),
    }
}

fn bind_callable_value<C>(
    context: &C,
    declared_types: &DeclaredValueTypeTemplates,
    value: DeclaredValueTypeTerm,
    state: CallableCandidateTemplateState,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<CandidateAbsence>
where
    C: BinderFactContext + ?Sized,
{
    let template = match callable_value_template(context, declared_types, value) {
        Ok(template) => template,
        Err(absence) => return Ok(absence),
    };

    // The candidate owns its top-level template while recursive storage remains Arc-shared.
    let candidate = CallableValueCandidateTemplate::new(value, template.clone(), state);

    candidates.push(CallableCandidateTemplate::Value(candidate));

    Ok(CandidateAbsence::UnresolvedReference)
}

fn callable_value_template<'facts>(
    context: &(impl BinderFactContext + ?Sized),
    declared_types: &'facts DeclaredValueTypeTemplates,
    value: DeclaredValueTypeTerm,
) -> Result<&'facts TypeExpressionTemplate, CandidateAbsence> {
    let mut relationships = BTreeMap::<DeclaredValueTypeTerm, Vec<DeclaredValueTypeTerm>>::new();

    for constraint in declared_types.constraints() {
        relationships
            .entry(constraint.left())
            .or_default()
            .push(constraint.right());
        relationships
            .entry(constraint.right())
            .or_default()
            .push(constraint.left());
    }

    let mut visited = BTreeSet::from([value]);
    let mut pending = VecDeque::from([value]);

    while let Some(term) = pending.pop_front() {
        if let Some(related_terms) = relationships.get(&term) {
            for related in related_terms {
                if visited.insert(*related) {
                    pending.push_back(*related);
                }
            }
        }
    }

    let mut has_evidence = false;

    for evidence in declared_types
        .evidence()
        .iter()
        .filter(|evidence| visited.contains(&evidence.term()))
    {
        has_evidence = true;

        if template_is_callable(context, evidence.template()) {
            return Ok(evidence.template());
        }
    }

    Err(if has_evidence {
        CandidateAbsence::NonCallableValue
    } else {
        CandidateAbsence::MissingValueType
    })
}

fn bind_overload_candidates<C>(
    context: &C,
    overload: CallableOverloadSymbolId,
    state: CallableCandidateTemplateState,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<()>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    let (template, has_diagnostics) =
        symbol_fact_value::<_, CallableOverloadTemplateFact>(context, overload, diagnostics)?;

    let state = combine_recovery(state, has_diagnostics);

    for arm in template.arms() {
        match arm {
            OverloadArmTemplate::Resolved(symbol) => {
                bind_declaration_candidate(context, *symbol, state, diagnostics, candidates)?
            }
            OverloadArmTemplate::Source(anchor) => {
                bind_source_overload_arm(
                    context,
                    overload,
                    *anchor,
                    state,
                    diagnostics,
                    candidates,
                )?;
            }
        }
    }

    Ok(())
}

fn bind_source_overload_arm<C>(
    context: &C,
    overload: CallableOverloadSymbolId,
    anchor: bray_declarations::SyntaxAnchor,
    state: CallableCandidateTemplateState,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<()>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    let Some(module) = context.symbols().containing_module(overload.into()) else {
        return Ok(());
    };

    let Some(path) = anchor.find_descendant::<PathSyntax>(context.syntax()) else {
        return Ok(());
    };

    let lookup = bind_module_path(context, module.id(), &path, NameAccess::Internal)?;

    match lookup {
        MemberLookupResult::Found(name) => {
            bind_resolved_name_candidate(context, name, state, diagnostics, candidates)?
        }
        MemberLookupResult::Inaccessible(names) => {
            for name in names {
                bind_resolved_name_candidate(
                    context,
                    name,
                    CallableCandidateTemplateState::Inaccessible,
                    diagnostics,
                    candidates,
                )?;
            }
        }
        MemberLookupResult::WrongKind(names)
        | MemberLookupResult::Ambiguous(names)
        | MemberLookupResult::Malformed(names) => {
            for name in names {
                bind_resolved_name_candidate(
                    context,
                    name,
                    CallableCandidateTemplateState::Recovered,
                    diagnostics,
                    candidates,
                )?;
            }
        }
        MemberLookupResult::NotFound => {}
    }

    Ok(())
}

fn bind_resolved_name_candidate<C>(
    context: &C,
    name: ResolvedName,
    state: CallableCandidateTemplateState,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<()>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    if let ResolvedName::Surface(symbol) = name
        && symbol.kind().is_callable()
    {
        bind_declaration_candidate(context, symbol, state, diagnostics, candidates)?;
    }

    Ok(())
}

fn bind_declaration_candidate<C>(
    context: &C,
    symbol: AnySymbolId,
    state: CallableCandidateTemplateState,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<()>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>,
{
    let Some(definition) = CallableDefinitionId::try_new(symbol) else {
        return Ok(());
    };

    let Some(callable) = CallableSymbolId::try_from_any(symbol) else {
        return Ok(());
    };

    let Some(generic_owner) = GenericOwnerId::try_new(symbol) else {
        return Ok(());
    };

    // Candidate records outlive the provider borrow and therefore own the stable key.
    let Some(key) = context.symbol_key(symbol)?.cloned() else {
        return Ok(());
    };

    let (signature, signature_diagnostics) =
        symbol_fact_value::<_, CallableSignatureFact>(context, callable, diagnostics)?;

    let (generic, generic_diagnostics) = symbol_fact_value::<_, GenericDeclarationTemplateFact>(
        context,
        generic_owner,
        diagnostics,
    )?;

    let mut defaults = Vec::with_capacity(signature.parameters().len());
    let mut has_diagnostics = signature_diagnostics || generic_diagnostics;

    for parameter in signature.parameters() {
        let (value, default_diagnostics) = symbol_fact_value::<
            _,
            CallableParameterDefaultTemplateFact,
        >(context, *parameter, diagnostics)?;

        has_diagnostics |= default_diagnostics;
        defaults.push(CallableParameterDefaultTemplate::new(*parameter, value));
    }

    let is_recovered = context.symbol_is_recovered(symbol)?.unwrap_or(true);
    let state = combine_recovery(state, is_recovered || has_diagnostics);

    candidates.push(CallableCandidateTemplate::Declaration(
        CallableDeclarationCandidateTemplate::new(
            key, definition, signature, generic, defaults, state,
        ),
    ));

    Ok(())
}

fn symbol_fact_value<C, F>(
    context: &C,
    owner: F::Owner,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<(F::Value, bool)>
where
    C: BinderFactContext + ?Sized,
    F: SymbolFactContract,
    F::Value: Clone,
    C::SymbolFacts: SymbolFactProvider<F>,
{
    let result = context
        .symbol_facts()
        .symbol_fact(SymbolFactRequest::<F>::new(owner))?;

    let has_diagnostics = !result.diagnostics().is_empty();
    *diagnostics = diagnostics.merged(result.diagnostics());

    // The candidate owns the fact value while recursive semantic storage remains Arc-shared.
    Ok((result.value().clone(), has_diagnostics))
}

fn template_is_callable(
    context: &(impl BinderFactContext + ?Sized),
    template: &TypeExpressionTemplate,
) -> bool {
    match template {
        TypeExpressionTemplate::Callable(_) => true,
        TypeExpressionTemplate::Resolved(ty) => context
            .semantic_values()
            .type_data(*ty)
            .is_ok_and(|data| matches!(data.as_ref(), TypeData::Callable(_))),
        _ => false,
    }
}

const fn state_for_recovery(is_recovered: bool) -> CallableCandidateTemplateState {
    if is_recovered {
        CallableCandidateTemplateState::Recovered
    } else {
        CallableCandidateTemplateState::Visible
    }
}

const fn combine_recovery(
    state: CallableCandidateTemplateState,
    is_recovered: bool,
) -> CallableCandidateTemplateState {
    if is_recovered {
        CallableCandidateTemplateState::Recovered
    } else {
        state
    }
}
