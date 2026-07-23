use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractClauseKind, CallableSignatureFact, LocalSymbolRegionId,
    MemberLookupResult, PredicateSemanticSummary,
};
use bray_syntax::{ExpressionSyntax, SyntaxNodeView, UsesClauseSyntax, syntax_node_view};

use crate::binder::{Binder, BindingContext};
use crate::binding::{
    BindingError, ExpressionBinder, callable_normal_completion_has_value, push_contract_scope,
};
use crate::lookup::{NameAccess, PathBindingContext};
use crate::unit::BoundUnitLocalBuilder;
use crate::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};

/// The declaration-surface role of one predicate-bearing clause.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredicateClauseBindingContext {
    /// A declaration's static generic constraint.
    GenericConstraint,
    /// A callable precondition, postcondition, or static contract clause.
    CallableContract(CallableContractClauseKind),
}

/// Binds one generated declaration-surface predicate clause through the ordinary expression binder.
pub fn bind_predicate_clause<C>(
    facts: &C,
    owner: AnySymbolId,
    clause: SyntaxNodeView<'_>,
    expressions: impl IntoIterator<Item = ExpressionSyntax>,
    context: PredicateClauseBindingContext,
) -> BinderFactResult<DiagnosticResult<Box<[PredicateSemanticSummary]>>>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>,
{
    let has_result = match context {
        PredicateClauseBindingContext::CallableContract(CallableContractClauseKind::Ensures) => {
            callable_normal_completion_has_value(facts, owner)?
        }
        PredicateClauseBindingContext::GenericConstraint
        | PredicateClauseBindingContext::CallableContract(
            CallableContractClauseKind::Requires | CallableContractClauseKind::Static,
        ) => false,
    };

    let syntax_anchor = SyntaxAnchor::from_node(&clause);

    bind_surface(
        facts,
        owner,
        clause,
        binding_context(context),
        move |binder, path| {
            let path = match context {
                PredicateClauseBindingContext::GenericConstraint => path,
                PredicateClauseBindingContext::CallableContract(_) => {
                    let scope =
                        push_contract_scope(binder, path.scope(), syntax_anchor, has_result)?;

                    path.with_scope(scope)
                }
            };

            let error_type = facts
                .semantic_values()
                .intern_type(bray_symbols::TypeData::Error)
                .map_err(|_| BindingError::IdentityCapacityExceeded)?;

            let mut expression_binder = ExpressionBinder::new(path, error_type);
            let mut predicates = Vec::new();

            let dependency = facts
                .semantic_values()
                .empty_dependency_contract_template()
                .map_err(|_| BindingError::IdentityCapacityExceeded)?;

            for expression in expressions {
                expression_binder.bind_expression(binder, path.scope(), Some(&expression))?;

                predicates.push(PredicateSemanticSummary::new(dependency));
            }

            Ok(predicates.into_boxed_slice())
        },
    )
}

/// Resolves one generated callable `uses(...)` clause through ordinary symbol lookup.
pub fn bind_trusted_capability_clause<C>(
    facts: &C,
    owner: AnySymbolId,
    clause: &UsesClauseSyntax,
) -> BinderFactResult<DiagnosticResult<Box<[AnySymbolId]>>>
where
    C: BinderFactContext + ?Sized,
{
    bind_surface(
        facts,
        owner,
        syntax_node_view(clause),
        BindingContext::ContractClause,
        |binder, path| {
            let mut capabilities = Vec::new();

            for capability in clause.paths() {
                match binder.bind_surface_path(path, &capability)? {
                    MemberLookupResult::Found(symbol) => capabilities.push(symbol),
                    MemberLookupResult::NotFound
                    | MemberLookupResult::WrongKind(_)
                    | MemberLookupResult::Ambiguous(_)
                    | MemberLookupResult::Inaccessible(_)
                    | MemberLookupResult::Malformed(_) => {}
                }
            }

            Ok(capabilities.into_boxed_slice())
        },
    )
}

fn bind_surface<C, T>(
    facts: &C,
    owner: AnySymbolId,
    syntax: SyntaxNodeView<'_>,
    context: BindingContext,
    bind: impl FnOnce(&mut Binder<'_, C>, PathBindingContext) -> Result<T, BindingError>,
) -> BinderFactResult<DiagnosticResult<T>>
where
    C: BinderFactContext + ?Sized,
{
    if facts.is_cancelled() {
        return Err(BinderFactError::Cancelled);
    }

    let symbols = facts.symbols();

    if symbols
        .compiler_known_provider()
        .declaration_fact_for_symbol(owner)
        .is_none()
    {
        return Err(BinderFactError::DependencyUnavailable);
    }

    let Some(owner_key) = symbols.symbol_key(owner) else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let source =
        BoundSourceAnchor::new(SyntaxAnchor::from_node(&syntax), syntax.source().version());

    // The transient bound unit owns its immutable key independently of the symbol graph.
    let owner_key = owner_key.clone();

    let key = match context {
        BindingContext::PredicateExpression => BoundUnitKey::constraint(owner_key, source),
        BindingContext::ContractClause => BoundUnitKey::contract_clause(owner_key, source),
        _ => None,
    }
    .ok_or(BinderFactError::DependencyUnavailable)?;

    let unit = BoundUnitId::new(0);
    let region = LocalSymbolRegionId::new(unit.raw());

    let builder = BoundUnitLocalBuilder::new(unit, key, region, syntax.full_range().start())
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    let mut binder = Binder::new(facts, context, builder);
    let scope = binder.unit().root_scope();

    let path = match symbols.containing_module(owner) {
        Some(module) => {
            PathBindingContext::new(scope, module.id(), module.owner(), NameAccess::Internal)
        }
        None => PathBindingContext::without_module(
            scope,
            symbols.compiler_known_environment().id().into(),
            NameAccess::Internal,
        ),
    };

    let value = bind(&mut binder, path).map_err(binding_error)?;

    let output = binder
        .finish()
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    let (_, diagnostics, _) = output.into_parts();

    Ok(DiagnosticResult::new(value, diagnostics))
}

const fn binding_context(context: PredicateClauseBindingContext) -> BindingContext {
    match context {
        PredicateClauseBindingContext::GenericConstraint => BindingContext::PredicateExpression,
        PredicateClauseBindingContext::CallableContract(_) => BindingContext::ContractClause,
    }
}

const fn binding_error(error: BindingError) -> BinderFactError {
    match error {
        BindingError::Cancelled => BinderFactError::Cancelled,
        BindingError::DependencyUnavailable
        | BindingError::Construction(_)
        | BindingError::IdentityCapacityExceeded
        | BindingError::RollbackFailed
        | BindingError::CandidateContextMismatch
        | BindingError::ControlTargetMismatch
        | BindingError::UnsupportedSyntax => BinderFactError::DependencyUnavailable,
    }
}
