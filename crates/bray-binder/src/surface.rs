use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractClauseKind, CallableSignatureQuery, CallableSymbolId,
    LocalSymbolRegionId, MemberLookupResult, PredicateSemanticSummary, TrustedCapabilitySymbolId,
};
use bray_syntax::{ExpressionSyntax, SyntaxNodeView, UsesClauseSyntax, syntax_node_view};

use crate::binder::Binder;
use crate::binding::{
    BindingError, ExpressionBinder, callable_normal_completion_has_value, push_contract_scope,
};
use crate::lookup::{NameAccess, PathBindingContext};
use crate::unit::BoundUnitLocalBuilder;
use crate::{BindingQueryContext, BindingQueryError, BindingQueryResult, SymbolQueryProvider};

/// The declaration-surface role of one predicate-bearing clause.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredicateClauseBindingContext {
    /// A declaration's static generic constraint.
    GenericConstraint,
    /// A callable precondition, postcondition, or static contract clause.
    CallableContract(CallableContractClauseKind),
}

#[derive(Clone, Copy)]
enum SurfaceUnitKind {
    Constraint,
    ContractClause,
}

/// One trusted capability resolved from an exact source path in a `uses` clause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundTrustedCapability {
    symbol: TrustedCapabilitySymbolId,
    source: SyntaxAnchor,
}

impl BoundTrustedCapability {
    /// Creates a source-correlated trusted capability after successful name binding.
    pub const fn new(symbol: TrustedCapabilitySymbolId, source: SyntaxAnchor) -> Self {
        Self { symbol, source }
    }

    /// Returns the resolved trusted-capability declaration.
    pub const fn symbol(self) -> TrustedCapabilitySymbolId {
        self.symbol
    }

    /// Returns the source path that requested the capability.
    pub const fn source(self) -> SyntaxAnchor {
        self.source
    }
}

/// Binds one declaration-surface predicate clause through the ordinary expression binder.
pub fn bind_predicate_clause<C>(
    binding_context: &C,
    owner: AnySymbolId,
    clause: SyntaxNodeView<'_>,
    expressions: impl IntoIterator<Item = ExpressionSyntax>,
    context: PredicateClauseBindingContext,
) -> BindingQueryResult<DiagnosticResult<Box<[PredicateSemanticSummary]>>, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    let has_result = match context {
        PredicateClauseBindingContext::CallableContract(CallableContractClauseKind::Ensures) => {
            callable_normal_completion_has_value(binding_context, owner)?
        }
        PredicateClauseBindingContext::GenericConstraint
        | PredicateClauseBindingContext::CallableContract(
            CallableContractClauseKind::Requires | CallableContractClauseKind::Static,
        ) => false,
    };

    let syntax_anchor = SyntaxAnchor::from_node(&clause);

    bind_surface(
        binding_context,
        owner,
        clause,
        match context {
            PredicateClauseBindingContext::GenericConstraint => SurfaceUnitKind::Constraint,
            PredicateClauseBindingContext::CallableContract(_) => SurfaceUnitKind::ContractClause,
        },
        move |binder, path| {
            let path = match context {
                PredicateClauseBindingContext::GenericConstraint => path,
                PredicateClauseBindingContext::CallableContract(_) => {
                    let scope =
                        push_contract_scope(binder, path.scope(), syntax_anchor, has_result)?;

                    path.with_scope(scope)
                }
            };

            let error_type = binding_context
                .semantic_values()
                .intern_type(bray_symbols::TypeData::Error)
                .map_err(|_| BindingError::IdentityCapacityExceeded)?;

            let mut expression_binder = ExpressionBinder::new(path, error_type);
            let mut predicates = Vec::new();

            let dependency = binding_context
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

/// Resolves one callable `uses(...)` clause through ordinary symbol lookup.
pub fn bind_trusted_capability_clause<C>(
    binding_context: &C,
    owner: AnySymbolId,
    clause: &UsesClauseSyntax,
) -> BindingQueryResult<DiagnosticResult<Box<[BoundTrustedCapability]>>, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    bind_surface(
        binding_context,
        owner,
        syntax_node_view(clause),
        SurfaceUnitKind::ContractClause,
        |binder, path| {
            let mut capabilities = Vec::new();

            for capability in clause.paths() {
                match binder.bind_trusted_capability_path(path, &capability)? {
                    MemberLookupResult::Found(symbol) => capabilities.push(
                        BoundTrustedCapability::new(symbol, SyntaxAnchor::from_node(&capability)),
                    ),
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
    binding_context: &C,
    owner: AnySymbolId,
    syntax: SyntaxNodeView<'_>,
    unit_kind: SurfaceUnitKind,
    bind: impl FnOnce(
        &mut Binder<'_, C>,
        PathBindingContext,
    ) -> Result<T, BindingError<C::UpstreamError>>,
) -> BindingQueryResult<DiagnosticResult<T>, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    if binding_context.is_cancelled() {
        return Err(BindingQueryError::Cancelled);
    }

    let symbols = binding_context.symbols();

    let Some(owner_key) = symbols.symbol_key(owner) else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    let source =
        BoundSourceAnchor::new(SyntaxAnchor::from_node(&syntax), syntax.source().version());

    // The transient bound unit owns its immutable key independently of the symbol graph.
    let owner_key = owner_key.clone();

    let key = match unit_kind {
        SurfaceUnitKind::Constraint => BoundUnitKey::constraint(owner_key, source),
        SurfaceUnitKind::ContractClause => BoundUnitKey::contract_clause(owner_key, source),
    }
    .ok_or(BindingQueryError::DependencyUnavailable)?;

    let unit = BoundUnitId::new(0);
    let region = LocalSymbolRegionId::new(unit.raw());

    let builder = BoundUnitLocalBuilder::new(unit, key, region, syntax.full_range().start())
        .map_err(|_| BindingQueryError::DependencyUnavailable)?;

    let mut binder = Binder::new(binding_context, builder);
    let scope = binder.unit().root_scope();

    if matches!(unit_kind, SurfaceUnitKind::ContractClause)
        && let Some(callable) = CallableSymbolId::try_from_any(owner)
    {
        crate::entry::push_callable_inputs_for(&mut binder, scope, callable)
            .map_err(|_| BindingQueryError::DependencyUnavailable)?;
    }

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
        .map_err(|_| BindingQueryError::DependencyUnavailable)?;

    let (_, diagnostics, _) = output.into_parts();

    Ok(DiagnosticResult::new(value, diagnostics))
}

fn binding_error<Upstream>(error: BindingError<Upstream>) -> BindingQueryError<Upstream> {
    match error {
        BindingError::Cancelled => BindingQueryError::Cancelled,
        BindingError::CheckerInfrastructure(error) => {
            BindingQueryError::CheckerInfrastructure(error)
        }
        BindingError::Upstream(error) => BindingQueryError::Upstream(error),
        BindingError::DependencyUnavailable
        | BindingError::Construction(_)
        | BindingError::IdentityCapacityExceeded
        | BindingError::RollbackFailed
        | BindingError::TransactionContextMismatch
        | BindingError::ControlTargetMismatch
        | BindingError::UnsupportedSyntax => BindingQueryError::DependencyUnavailable,
    }
}
