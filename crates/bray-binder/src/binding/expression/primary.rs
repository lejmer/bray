use bray_bound_tree::{
    BoundAnonymousCallableExpression, BoundBlockExpression, BoundExpression, BoundExpressionId,
    BoundNameExpression, BoundStructuredExpressionKind, BoundUnresolvedReferenceExpression,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::LocalScopeId;
use bray_syntax::{
    AccessExpressionSyntax, ArrayExpressionSyntax, AwaitExpressionSyntax, ForExpressionSyntax,
    GeneralGeneratorExpressionSyntax, LambdaExpressionSyntax, LeadingDotVariantExpressionSyntax,
    MatchExpressionSyntax, PrimaryExpressionSyntax, SourceSyntaxNode, SyntaxKind, SyntaxNodeView,
    SyntaxWalkControl, walk_direct_child_nodes,
};

use super::super::{BindingError, BindingResult};
use super::ExpressionBinder;
use super::support::{ReferenceResolution, classify_reference_result, structured_kind};
use crate::BinderFactContext;
use crate::binder::Binder;
use crate::lookup::NameAccess;

impl ExpressionBinder {
    pub(super) fn bind_primary<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &PrimaryExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        if let Some(access) = syntax.access_expression() {
            let head = self.bind_access(binder, scope, &access)?;

            return match syntax.struct_construction_body() {
                Some(body) => self.bind_struct_construction(binder, scope, &body, Some(head)),
                None => Ok(head),
            };
        }

        if let Some(body) = syntax.struct_construction_body() {
            return self.bind_struct_construction(binder, scope, &body, None);
        }

        if let Some(block) = syntax.block_expression() {
            let block_id = binder.bind_block(scope, &block, self)?;
            let recovered = block.is_recovered();

            return self.push(
                binder,
                BoundExpression::Block(BoundBlockExpression::new(
                    binder.source_origin(&block),
                    block_id,
                    None,
                    recovered,
                )),
            );
        }

        let mut result = None;

        walk_direct_child_nodes(syntax, |root| {
            result = Some(self.bind_primary_node(binder, scope, root, syntax));

            SyntaxWalkControl::Stop
        });

        match result {
            Some(result) => result,
            None => self.push_error(binder, Some(syntax)),
        }
    }

    fn bind_primary_node<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        root: SyntaxNodeView<'_>,
        recovery_origin: &PrimaryExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        if matches!(
            root.kind(),
            SyntaxKind::YieldExpression
                | SyntaxKind::ReturnExpression
                | SyntaxKind::BreakExpression
                | SyntaxKind::ContinueExpression
        ) {
            return self.bind_control_transfer(binder, scope, root);
        }

        if root.kind() == SyntaxKind::GeneralGeneratorExpression {
            let Some(generator) = root.cast::<GeneralGeneratorExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            return self.bind_generator_iteration(
                binder,
                scope,
                &generator.generator_iteration_expression(),
                SyntaxAnchor::from_node(&generator),
            );
        }

        if root.kind() == SyntaxKind::ForExpression {
            let Some(expression) = root.cast::<ForExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            return self.bind_for_expression(binder, scope, &expression);
        }

        if root.kind() == SyntaxKind::AwaitExpression {
            let Some(await_expression) = root.cast::<AwaitExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            return self.bind_await(binder, scope, &await_expression);
        }

        if root.kind() == SyntaxKind::MatchExpression {
            let Some(expression) = root.cast::<MatchExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            return self.bind_match_expression(binder, scope, &expression);
        }

        if root.kind() == SyntaxKind::LambdaExpression {
            let Some(lambda) = root.cast::<LambdaExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            let unit = binder.bind_anonymous_callable_reference(&lambda)?;

            return self.push(
                binder,
                BoundExpression::AnonymousCallable(BoundAnonymousCallableExpression::new(
                    binder.source_origin(&lambda),
                    unit,
                    None,
                    lambda.is_recovered(),
                )),
            );
        }

        if root.kind() == SyntaxKind::LeadingDotVariantExpression {
            let Some(variant) = root.cast::<LeadingDotVariantExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            return self.bind_leading_dot_variant(binder, &variant);
        }

        if root.kind() == SyntaxKind::GroupedExpression {
            return self.bind_first_descendant_expression(binder, scope, root, recovery_origin);
        }

        if root.kind() == SyntaxKind::ArrayExpression {
            let Some(array) = root.cast::<ArrayExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            let kind = if array.semicolon_token().is_some() {
                BoundStructuredExpressionKind::RepeatedArray
            } else {
                BoundStructuredExpressionKind::Array
            };

            return self.bind_structured(binder, scope, root, kind);
        }

        let Some(kind) = structured_kind(root.kind()) else {
            return self.push_error(binder, Some(recovery_origin));
        };

        self.bind_structured(binder, scope, root, kind)
    }

    pub(in crate::binding::expression) fn bind_access<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &AccessExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        self.bind_access_with_access(binder, scope, syntax, self.path_context.access())
    }

    fn bind_access_with_access<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &AccessExpressionSyntax,
        access: NameAccess,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let root_token = syntax.identifier_token().or_else(|| syntax.self_token());

        let mut current = match root_token {
            Some(token) => {
                let target = classify_reference_result(binder.bind_reference_identifier(
                    self.path_context_for(scope, access),
                    syntax.source(),
                    token,
                ));

                match target {
                    ReferenceResolution::Resolved(target) => self.push(
                        binder,
                        BoundExpression::Name(BoundNameExpression::new(
                            binder.source_origin(syntax),
                            target,
                            None,
                            syntax.is_recovered(),
                        )),
                    )?,
                    ReferenceResolution::Unresolved(kind, candidates) => self.push(
                        binder,
                        BoundExpression::UnresolvedReference(
                            BoundUnresolvedReferenceExpression::new(
                                binder.source_origin(syntax),
                                kind,
                                candidates,
                                self.error_type,
                            ),
                        ),
                    )?,
                }
            }
            None => match syntax.access_expressions().next() {
                Some(nested) => {
                    let nested_access = if syntax.internal_token().is_some() {
                        NameAccess::Internal
                    } else {
                        access
                    };

                    self.bind_access_with_access(binder, scope, &nested, nested_access)?
                }
                None => self.push_error(binder, Some(syntax))?,
            },
        };

        let mut failure = None;

        walk_direct_child_nodes(syntax, |operation| {
            let result = match operation.kind() {
                SyntaxKind::MemberAccessOperation => {
                    let Some(member) = operation.cast::<bray_syntax::MemberAccessOperationSyntax>()
                    else {
                        failure = Some(BindingError::UnsupportedSyntax);

                        return SyntaxWalkControl::Stop;
                    };

                    self.bind_member_access(binder, &member, current)
                }
                SyntaxKind::ElementIndexOperation => self.bind_structured_with_operand(
                    binder,
                    scope,
                    operation,
                    BoundStructuredExpressionKind::ElementIndex,
                    current,
                ),
                _ => return SyntaxWalkControl::Continue,
            };

            match result {
                Ok(expression) => current = expression,
                Err(error) => {
                    failure = Some(error);

                    return SyntaxWalkControl::Stop;
                }
            }

            SyntaxWalkControl::Continue
        });

        failure.map_or(Ok(current), Err)
    }
}
