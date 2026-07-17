use bray_binder::{BinderFactError, BinderFactResult, CallableTypeQualifiers};
use bray_symbols::{AnySymbolId, CallableConstness, CallableExecution, CallableTrust, SymbolGraph};
use bray_syntax::{
    CallableResultClauseSyntax, ParameterListSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl,
    TypeExpressionSyntax, walk_direct_child_nodes,
};

use super::super::context::CompilationBinderFacts;

pub(super) struct CallableSurface {
    pub(super) parameters: ParameterListSyntax,
    pub(super) result: Option<TypeExpressionSyntax>,
    pub(super) qualifiers: CallableTypeQualifiers,
}

pub(super) fn declaration_callable_surface(
    context: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> BinderFactResult<CallableSurface> {
    with_declaration_root(context, symbol, callable_surface)
}

fn callable_surface(root: SyntaxNodeView<'_>) -> BinderFactResult<CallableSurface> {
    let mut parameters = None;
    let mut result = None;
    let mut modifiers = CallableModifierPresence::default();

    walk_direct_child_nodes(&root, |child| {
        match child.kind() {
            SyntaxKind::ParameterList => parameters = child.cast::<ParameterListSyntax>(),
            SyntaxKind::CallableResultClause => {
                result = child
                    .cast::<CallableResultClauseSyntax>()
                    .map(|clause| clause.type_expression());
            }
            kind if is_callable_modifier_kind(kind) => collect_modifiers(child, &mut modifiers),
            _ => {}
        }

        SyntaxWalkControl::Continue
    });

    let parameters = parameters.ok_or(BinderFactError::DependencyUnavailable)?;

    Ok(CallableSurface {
        parameters,
        result,
        qualifiers: callable_qualifiers(modifiers),
    })
}

fn collect_modifiers(node: SyntaxNodeView<'_>, modifiers: &mut CallableModifierPresence) {
    for token in node.tokens() {
        match token.kind() {
            SyntaxKind::ConstKeyword => modifiers.is_constant = true,
            SyntaxKind::AsyncKeyword => modifiers.is_async = true,
            SyntaxKind::TrustedKeyword => modifiers.is_trusted = true,
            _ => {}
        }
    }
}

const fn is_callable_modifier_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FunctionModifiers
            | SyntaxKind::TypeCallableMemberModifiers
            | SyntaxKind::TraitCallableMemberModifiers
            | SyntaxKind::ConstructorMemberModifiers
            | SyntaxKind::AsyncCapableLifecycleMemberModifiers
            | SyntaxKind::SyncLifecycleMemberModifiers
    )
}

#[derive(Clone, Copy, Default)]
struct CallableModifierPresence {
    is_constant: bool,
    is_async: bool,
    is_trusted: bool,
}

fn callable_qualifiers(modifiers: CallableModifierPresence) -> CallableTypeQualifiers {
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
    )
}

pub(super) fn compiler_known_surface(
    symbols: &SymbolGraph,
    symbol: AnySymbolId,
) -> BinderFactResult<&'static bray_compiler_known::CatalogDeclarationSurfaceSyntax> {
    symbols
        .compiler_known_provider()
        .declaration_fact_for_symbol(symbol)
        .map(|fact| fact.surface())
        .ok_or(BinderFactError::DependencyUnavailable)
}

pub(super) fn declaration_syntax<T>(
    context: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> BinderFactResult<T>
where
    T: bray_syntax::SyntaxCast,
{
    with_declaration_root(context, symbol, |root| {
        root.cast::<T>()
            .ok_or(BinderFactError::DependencyUnavailable)
    })
}

pub(super) fn declaration_child<T>(
    context: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> BinderFactResult<T>
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

        child.ok_or(BinderFactError::DependencyUnavailable)
    })
}

fn with_declaration_root<R>(
    context: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
    consume: impl FnOnce(SyntaxNodeView<'_>) -> BinderFactResult<R>,
) -> BinderFactResult<R> {
    if let Some(anchor) = context.symbols.declaration_syntax_anchor(symbol) {
        let root = context
            .compilation
            .syntax_tree()
            .find_node(
                anchor.source_id(),
                anchor.syntax_kind(),
                anchor.full_range(),
                anchor.is_recovered(),
            )
            .ok_or(BinderFactError::DependencyUnavailable)?;

        return consume(root);
    }

    let surface = compiler_known_surface(context.symbols, symbol)?;

    let fragment = surface
        .syntax_fragment()
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    consume(fragment.root())
}
