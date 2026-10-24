use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractClauseKind, CallableSymbolId, LocalSymbolRegionId,
    MemberLookupResult, TrustedCapabilitySymbolId,
};
use bray_syntax::{SyntaxNodeView, UsesClauseSyntax, syntax_node_view};

use crate::binder::Binder;
use crate::binding::{BindingError, push_contract_scope};
use crate::lookup::PathBindingContext;
use crate::unit::BoundUnitLocalBuilder;
use crate::{BindingQueryContext, BindingQueryError, BindingQueryResult};

/// The declaration-surface role of one predicate-bearing clause.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredicateClauseBindingContext {
    /// A declaration's static generic constraint.
    GenericConstraint,
    /// A callable precondition, postcondition, or static contract clause.
    CallableContract(CallableContractClauseKind),
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

/// Resolves one callable `uses(...)` clause through ordinary symbol lookup.
pub fn bind_trusted_capability_clause<C>(
    binding_context: &C,
    owner: AnySymbolId,
    clause: &UsesClauseSyntax,
    parameters: Option<&bray_syntax::ParameterListSyntax>,
) -> BindingQueryResult<DiagnosticResult<Box<[BoundTrustedCapability]>>, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
{
    bind_surface(
        binding_context,
        owner,
        syntax_node_view(clause),
        |binder, path| {
            let path = if let Some(parameters) = parameters {
                let scope = push_contract_scope(
                    binder,
                    path.scope(),
                    SyntaxAnchor::from_node(clause),
                    false,
                )?;

                for (ordinal, parameter) in parameters.parameters().enumerate() {
                    let token = parameter.identifier_token();
                    let source = bray_syntax::SourceSyntaxNode::source(&parameter);

                    if let Some(name) = token
                        .text(source.text())
                        .and_then(bray_symbols::SymbolName::try_new)
                    {
                        crate::entry::push_contract_parameter(
                            binder,
                            scope,
                            ordinal,
                            name,
                            SyntaxAnchor::from_node(&parameter),
                        )?;
                    }
                }

                path.with_scope(scope)
            } else {
                path
            };

            let mut capabilities = Vec::new();

            for capability in clause.paths() {
                match binder.bind_trusted_capability_path(path, &capability)? {
                    MemberLookupResult::Found(symbol) => capabilities.push(
                        BoundTrustedCapability::new(symbol, SyntaxAnchor::from_node(&capability)),
                    ),
                    MemberLookupResult::NotFound
                    | MemberLookupResult::UndeclaredDependency
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
    bind: impl FnOnce(
        &mut Binder<'_, C>,
        PathBindingContext,
    ) -> Result<T, BindingError<C::UpstreamError>>,
) -> BindingQueryResult<DiagnosticResult<T>, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
{
    if binding_context.is_cancelled() {
        return Err(BindingQueryError::Cancelled);
    }

    let symbols = binding_context.symbols();

    let Some(owner_key) = symbols.symbol_key(owner) else {
        return Err(BindingQueryError::Binding(
            BindingError::SymbolRecordUnavailable(owner),
        ));
    };

    let source =
        BoundSourceAnchor::new(SyntaxAnchor::from_node(&syntax), syntax.source().version());

    // The transient bound unit owns its immutable key independently of the symbol graph.
    let owner_key = owner_key.clone();

    let key = BoundUnitKey::contract_clause(owner_key, source).ok_or(
        BindingQueryError::Binding(BindingError::InvalidUnitKey { source, owner }),
    )?;

    let unit = BoundUnitId::new(0);
    let region = LocalSymbolRegionId::new(unit.raw());

    let builder = BoundUnitLocalBuilder::new(unit, key, region, syntax.full_range().start())
        .map_err(BindingQueryError::Construction)?;

    let mut binder = Binder::new(binding_context, builder);
    let scope = binder.unit().root_scope();

    // Capability lookup needs lexical names, but never runtime input types.
    // Keep shadowing without demanding the signature currently being constructed.
    if let Some(callable) = CallableSymbolId::try_from_any(owner) {
        crate::entry::push_callable_input_names_for(&mut binder, scope, callable);
    }

    let path = crate::entry::path_context(&binder, scope);

    let value = bind(&mut binder, path).map_err(binding_error)?;

    let output = binder.finish();

    let (_, diagnostics, _) = output.into_parts();

    Ok(DiagnosticResult::new(value, diagnostics))
}

fn binding_error<Upstream>(error: BindingError<Upstream>) -> BindingQueryError<Upstream> {
    match error {
        BindingError::Cancelled => BindingQueryError::Cancelled,
        BindingError::SemanticValue(error) => BindingQueryError::SemanticValue(error),
        BindingError::Upstream(error) => BindingQueryError::Upstream(error),
        BindingError::Construction(error) => BindingQueryError::Construction(error),
        error @ (BindingError::DependencyUnavailable
        | BindingError::MissingSyntax { .. }
        | BindingError::MissingOwner { .. }
        | BindingError::MissingModule { .. }
        | BindingError::InvalidSurfaceName { .. }
        | BindingError::IdentityCapacityExceeded
        | BindingError::RollbackFailed
        | BindingError::TransactionContextMismatch
        | BindingError::ControlTargetMismatch
        | BindingError::UnsupportedSyntax
        | BindingError::SyntaxContract(_)
        | BindingError::GenericOwnerUnavailable(_)
        | BindingError::CompilerKnownRepresentationUnavailable(_)
        | BindingError::SymbolRecordUnavailable(_)
        | BindingError::ModulePartRecordUnavailable(_)
        | BindingError::DeclarationRecordUnavailable(_)
        | BindingError::ContextualSelfUnavailable(_)
        | BindingError::UnresolvedTypeTemplate
        | BindingError::InvalidUnitKey { .. }
        | BindingError::CallableParameterCountMismatch { .. }
        | BindingError::CallableParameterOwnerMismatch { .. }
        | BindingError::ReceiverParameterOwnerMismatch { .. }
        | BindingError::ReceiverContextMismatch { .. }
        | BindingError::CallableTypeExpected { .. }
        | BindingError::CallableTypeTemplateExpected(_)
        | BindingError::CompilerKnownHeapStoragePolicyUnavailable
        | BindingError::ImportedPackageUnavailable(_)
        | BindingError::ImportedPathUnavailable { .. }
        | BindingError::BoundWalkStopped(_)
        | BindingError::CallableSignature(_)
        | BindingError::GenericSubstitution(_)) => BindingQueryError::Binding(error),
    }
}
