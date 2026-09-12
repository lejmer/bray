use crate::compilation::binder::BindingQueryResult;
use bray_binder::{BindingQueryError, CallableTypeQualifiers, bind_callable_abi};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableAbi, CallableConstness, CallableExecution, CallableTrust, ReceiverMode,
    SymbolGraph,
};
use bray_syntax::{
    CallableDirectivesSyntax, CallableResultClauseSyntax, FunctionDirectivesSyntax,
    ParameterListSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl, TypeExpressionSyntax,
    walk_direct_child_nodes,
};

use super::super::context::CompilationBindingContext;
use crate::compilation::binder::semantic_contract_binding_error as binding_contract;
use crate::compilation::{SemanticDataKind, SemanticQueryContext, SemanticQueryViolation};

pub(super) struct CallableSurface {
    pub(super) parameters: ParameterListSyntax,
    pub(super) result: Option<TypeExpressionSyntax>,
    pub(super) qualifiers: DiagnosticResult<CallableTypeQualifiers>,
    pub(super) has_body: bool,
}

pub(super) fn declaration_callable_surface(
    context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> BindingQueryResult<CallableSurface> {
    with_declaration_root(context, symbol, |root| callable_surface(root, symbol))
}

fn callable_surface(
    root: SyntaxNodeView<'_>,
    symbol: AnySymbolId,
) -> BindingQueryResult<CallableSurface> {
    let mut parameters = None;
    let mut result = None;
    let mut abi_directives = Vec::new();
    let mut modifiers = CallableModifierPresence::default();
    let mut has_body = false;

    walk_direct_child_nodes(&root, |child| {
        match child.kind() {
            SyntaxKind::ParameterList => parameters = child.cast::<ParameterListSyntax>(),
            SyntaxKind::CallableDirectives => {
                if let Some(directives) = child.cast::<CallableDirectivesSyntax>() {
                    abi_directives.extend(directives.abi_directives());
                }
            }
            SyntaxKind::FunctionDirectives => {
                if let Some(directives) = child.cast::<FunctionDirectivesSyntax>() {
                    abi_directives.extend(directives.abi_directives());
                }
            }
            SyntaxKind::CallableResultClause => {
                result = child
                    .cast::<CallableResultClauseSyntax>()
                    .map(|clause| clause.type_expression());
            }
            SyntaxKind::CallableBodyBlockExpression => has_body = true,
            kind if is_callable_modifier_kind(kind) => collect_modifiers(child, &mut modifiers),
            _ => {}
        }

        SyntaxWalkControl::Continue
    });

    let parameters = parameters.ok_or_else(|| {
        binding_contract(
            SemanticQueryContext::Symbol(symbol),
            SemanticQueryViolation::Missing(SemanticDataKind::Syntax),
        )
    })?;

    // Declaration discovery owns duplicate directive diagnostics. Binding only needs the first
    // ABI directive to choose the recovered callable ABI.
    let abi = bind_callable_abi(abi_directives.into_iter().take(1));

    Ok(CallableSurface {
        parameters,
        result,
        qualifiers: abi.map(|abi| {
            callable_qualifiers(symbol, modifiers, abi)
                .with_execution_properties(bray_binder::callable_execution_properties(root))
        }),
        has_body,
    })
}

fn collect_modifiers(node: SyntaxNodeView<'_>, modifiers: &mut CallableModifierPresence) {
    for token in node.tokens() {
        match token.kind() {
            SyntaxKind::ConstKeyword => modifiers.is_constant = true,
            SyntaxKind::AsyncKeyword => modifiers.is_async = true,
            SyntaxKind::TrustedKeyword => modifiers.is_trusted = true,
            SyntaxKind::StaticKeyword => modifiers.is_static = true,
            SyntaxKind::ConsumeKeyword => modifiers.is_consuming = true,
            SyntaxKind::MutKeyword => modifiers.is_mutable = true,
            _ => {}
        }
    }
}

const fn is_callable_modifier_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CallableModifiers
            | SyntaxKind::FunctionModifiers
            | SyntaxKind::TypeCallableMemberModifiers
            | SyntaxKind::TraitCallableMemberModifiers
            | SyntaxKind::ConstructorMemberModifiers
            | SyntaxKind::AsyncCapableLifecycleMemberModifiers
            | SyntaxKind::ScopeEnterMemberModifiers
            | SyntaxKind::SyncLifecycleMemberModifiers
    )
}

#[derive(Clone, Copy, Default)]
struct CallableModifierPresence {
    is_constant: bool,
    is_async: bool,
    is_trusted: bool,
    is_static: bool,
    is_consuming: bool,
    is_mutable: bool,
}

fn callable_qualifiers(
    symbol: AnySymbolId,
    modifiers: CallableModifierPresence,
    abi: CallableAbi,
) -> CallableTypeQualifiers {
    CallableTypeQualifiers::new(
        if modifiers.is_constant {
            CallableConstness::Constant
        } else {
            CallableConstness::Runtime
        },
        if modifiers.is_async {
            CallableExecution::Asynchronous
        } else {
            CallableExecution::Synchronous
        },
        if modifiers.is_trusted {
            CallableTrust::Trusted
        } else {
            CallableTrust::Safe
        },
        abi,
        receiver_mode(symbol, modifiers),
    )
}

const fn receiver_mode(
    symbol: AnySymbolId,
    modifiers: CallableModifierPresence,
) -> Option<ReceiverMode> {
    match symbol {
        AnySymbolId::TypeCallableMember(_)
        | AnySymbolId::TraitCallableMember(_)
        | AnySymbolId::TraitCallableFulfillment(_) => {
            if modifiers.is_static {
                None
            } else {
                Some(receiver_mode_from_modifiers(modifiers))
            }
        }
        AnySymbolId::Finalizer(_) | AnySymbolId::TraitFinalizerRequirement(_) => {
            Some(ReceiverMode::Mutable)
        }
        AnySymbolId::Destructor(_) | AnySymbolId::TraitDestructorRequirement(_) => {
            Some(ReceiverMode::ConsumingMutable)
        }
        AnySymbolId::ScopeEnter(_)
        | AnySymbolId::TraitScopeEnterRequirement(_)
        | AnySymbolId::TraitScopeEnterFulfillment(_) => {
            Some(receiver_mode_from_modifiers(modifiers))
        }
        _ => None,
    }
}

const fn receiver_mode_from_modifiers(modifiers: CallableModifierPresence) -> ReceiverMode {
    match (modifiers.is_consuming, modifiers.is_mutable) {
        (false, false) => ReceiverMode::Shared,
        (false, true) => ReceiverMode::Mutable,
        (true, false) => ReceiverMode::Consuming,
        (true, true) => ReceiverMode::ConsumingMutable,
    }
}

pub(super) fn compiler_known_surface(
    symbols: &SymbolGraph,
    symbol: AnySymbolId,
) -> BindingQueryResult<&'static bray_compiler_known::CatalogDeclarationSurfaceSyntax> {
    symbols
        .compiler_known_provider()
        .declaration_semantics_for_symbol(symbol)
        .map(|semantics| semantics.surface())
        .ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Symbol(symbol),
                SemanticQueryViolation::Missing(SemanticDataKind::DeclarationRecord),
            )
        })
}

pub(super) fn symbol_ordinal(index: usize) -> BindingQueryResult<bray_symbols::SymbolOrdinal> {
    let ordinal = u32::try_from(index).map_err(|_| {
        BindingQueryError::Binding(bray_binder::BindingError::IdentityCapacityExceeded)
    })?;

    Ok(bray_symbols::SymbolOrdinal::new(ordinal))
}

pub(super) fn declaration_syntax<T>(
    context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> BindingQueryResult<T>
where
    T: bray_syntax::SyntaxCast,
{
    with_declaration_root(context, symbol, |root| {
        root.cast::<T>().ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Symbol(symbol),
                SemanticQueryViolation::Unsupported(SemanticDataKind::Syntax),
            )
        })
    })
}

pub(super) fn declaration_child<T>(
    context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> BindingQueryResult<T>
where
    T: bray_syntax::SyntaxCast,
{
    with_declaration_root(context, symbol, |root| {
        let mut child = None;

        walk_direct_child_nodes(&root, |candidate| {
            if candidate.kind() == T::KIND {
                child = candidate.cast::<T>();

                return SyntaxWalkControl::Stop;
            }

            SyntaxWalkControl::Continue
        });

        child.ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Symbol(symbol),
                SemanticQueryViolation::Missing(SemanticDataKind::Syntax),
            )
        })
    })
}

pub(super) fn with_declaration_root<R>(
    context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
    consume: impl FnOnce(SyntaxNodeView<'_>) -> BindingQueryResult<R>,
) -> BindingQueryResult<R> {
    if let Some(anchor) = context.symbols.declaration_syntax_anchor(symbol) {
        let root = syntax_node_for_anchor(context, anchor)?;

        return consume(root);
    }

    let surface = compiler_known_surface(context.symbols, symbol)?;

    let fragment = surface.syntax_fragment().map_err(|cause| {
        crate::compilation::binder::semantic_query_binding_error(
            crate::compilation::SemanticQueryFailure::PreparsedSyntax {
                symbol,
                source: None,
                cause,
            },
        )
    })?;

    consume(fragment.root())
}

pub(super) fn syntax_node_for_anchor<'syntax>(
    context: &'syntax CompilationBindingContext<'_>,
    anchor: SyntaxAnchor,
) -> BindingQueryResult<SyntaxNodeView<'syntax>> {
    context
        .compilation
        .syntax_tree()
        .find_node(
            anchor.source_id(),
            anchor.syntax_kind(),
            anchor.full_range(),
            anchor.is_recovered(),
        )
        .ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Source(anchor.source_id()),
                SemanticQueryViolation::Missing(SemanticDataKind::Syntax),
            )
        })
}

#[cfg(test)]
mod tests {
    use bray_binder::{BindingError, BoundUnitBindingError};

    use super::symbol_ordinal;
    use crate::compilation::binder::binding_query_error;
    use crate::fact::FactQueryError;

    #[test]
    fn overflowing_symbol_ordinals_retain_identity_capacity_failure() {
        let overflow = usize::try_from(u64::from(u32::MAX) + 1)
            .unwrap_or_else(|_| panic!("test requires usize wider than u32"));

        let error = symbol_ordinal(overflow)
            .expect_err("overflowing ordinal must fail before symbol construction");

        assert_eq!(
            binding_query_error(error),
            FactQueryError::Binding(BoundUnitBindingError::Binding(
                BindingError::IdentityCapacityExceeded
            ))
        );
    }
}
